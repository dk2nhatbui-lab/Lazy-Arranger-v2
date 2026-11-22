//! Headless synthesis and WAV export.
//!
//! This is the renderer the desktop shell will play through once a realtime
//! backend exists: one deterministic function from song to samples, so tests
//! can check timing and level without opening an audio device.

pub mod analysis;
pub mod chords;
pub mod link;
pub mod player;
pub mod wav;

use std::f64::consts::TAU;

use music_core::{Pitch, Velocity};
use project::song::Song;

/// Sample rate used by export and playback.
pub const SAMPLE_RATE: u32 = 48_000;
/// Stereo channel count.
pub const CHANNELS: u16 = 2;

/// A rendered stereo buffer.
#[derive(Clone, Debug, Default)]
pub struct RenderedAudio {
    /// Interleaved stereo samples in `-1.0..=1.0`.
    pub samples: Vec<f32>,
    /// Sample rate the buffer was produced at.
    pub sample_rate: u32,
}

impl RenderedAudio {
    /// Number of frames (one frame holds both channels).
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(CHANNELS)
    }

    /// Duration in seconds.
    #[must_use]
    pub fn duration_seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    /// Encodes the buffer as a 16-bit PCM WAV file.
    #[must_use]
    pub fn to_wav(&self) -> Vec<u8> {
        let data_len = self.samples.len() * 2;
        let mut out = Vec::with_capacity(44 + data_len);
        let byte_rate = self.sample_rate * u32::from(CHANNELS) * 2;
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&CHANNELS.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&(CHANNELS * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data_len as u32).to_le_bytes());
        for sample in &self.samples {
            let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }
}

/// Renders every track of a song with a simple additive voice.
#[must_use]
pub fn render(song: &Song, sample_rate: u32) -> RenderedAudio {
    let end_tick = song.end_tick();
    let duration = song.tempo_map.duration_seconds(0, end_tick.max(1)) + 0.75;
    let frames = (duration * f64::from(sample_rate)).ceil() as usize;
    let mut samples = vec![0.0_f32; frames * usize::from(CHANNELS)];

    for track in song.tracks.iter().filter(|track| song.is_audible(track)) {
        for note in &track.notes {
            let start_seconds = song.tempo_map.seconds_at(note.start);
            let end_seconds = song
                .tempo_map
                .seconds_at(note.start.saturating_add(note.duration.max(1)));
            let start = (start_seconds * f64::from(sample_rate)).round() as usize;
            let stop = (end_seconds * f64::from(sample_rate)).round() as usize;
            let frequency = frequency(note.pitch);
            let amplitude = f64::from(note.velocity.as_u8()) / 127.0 * 0.18;
            let release = (0.08 * f64::from(sample_rate)) as usize;
            for frame in start..stop.min(frames) {
                let t = (frame - start) as f64 / f64::from(sample_rate);
                let remaining = stop.saturating_sub(frame);
                let envelope = if remaining < release {
                    remaining as f64 / release as f64
                } else {
                    1.0
                };
                let value = (voice(frequency, t) * amplitude * envelope) as f32;
                samples[frame * 2] += value;
                samples[frame * 2 + 1] += value;
            }
        }
    }
    for sample in &mut samples {
        *sample = sample.clamp(-1.0, 1.0);
    }
    RenderedAudio {
        samples,
        sample_rate,
    }
}

/// Equal-tempered frequency for a MIDI pitch.
#[must_use]
pub fn frequency(pitch: Pitch) -> f64 {
    440.0 * 2.0_f64.powf((f64::from(pitch.as_u8()) - 69.0) / 12.0)
}

/// One voice: a sine fundamental with two quiet harmonics.
fn voice(frequency: f64, seconds: f64) -> f64 {
    let phase = TAU * frequency * seconds;
    let attack = (seconds / 0.005).min(1.0);
    attack * (phase.sin() + 0.25 * (2.0 * phase).sin() + 0.1 * (3.0 * phase).sin())
}

/// Converts a pitch and velocity into an approximate peak amplitude.
#[must_use]
pub fn peak_for(velocity: Velocity) -> f32 {
    f32::from(velocity.as_u8()) / 127.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::Channel;
    use project::song::{Note, Track};
    use project::{NoteId, TrackId};

    fn song_with_note() -> Song {
        let mut song = Song::new("Audio", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: music_core::PPQ,
            pitch: Pitch::from_name("A4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        song
    }

    #[test]
    fn a_note_produces_sound_where_it_sounds() {
        let rendered = render(&song_with_note(), SAMPLE_RATE);
        assert!(rendered.samples.iter().any(|sample| sample.abs() > 0.01));
        assert_eq!(rendered.samples[rendered.samples.len() - 1], 0.0);
        // One beat at 120 bpm is 0.5 s, plus the release tail.
        assert!(rendered.duration_seconds() > 0.5);
        assert!(rendered.duration_seconds() < 2.0);
    }

    #[test]
    fn muted_and_soloed_tracks_follow_the_mix_rules() {
        let mut song = song_with_note();
        let mut second =
            project::song::Track::new(project::TrackId::new(2).expect("valid"), "Second");
        second.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: music_core::PPQ,
            pitch: Pitch::from_name("A5").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(second);
        let both = render(&song, SAMPLE_RATE);
        song.tracks[1].muted = true;
        let muted = render(&song, SAMPLE_RATE);
        assert_ne!(both.samples, muted.samples);
        song.tracks[1].muted = false;
        song.tracks[1].soloed = true;
        let soloed = render(&song, SAMPLE_RATE);
        assert_ne!(
            both.samples, soloed.samples,
            "solo silences the first track"
        );
        song.tracks[1].soloed = false;
        song.tracks[0].muted = true;
        song.tracks[1].muted = true;
        let silent = render(&song, SAMPLE_RATE);
        assert!(silent.samples.iter().all(|sample| sample.abs() < 1.0e-6));
    }

    #[test]
    fn a4_sits_at_440_hz() {
        let pitch = Pitch::from_name("A4").expect("valid");
        assert!((frequency(pitch) - 440.0).abs() < 0.01);
        assert!((frequency(Pitch::new(60).expect("valid")) - 261.63).abs() < 0.01);
    }

    #[test]
    fn output_never_clips() {
        let mut song = song_with_note();
        let track = &mut song.tracks[0];
        for index in 2..12 {
            track.notes.push(Note {
                id: NoteId::new(index).expect("valid"),
                start: 0,
                duration: music_core::PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(127).expect("valid"),
                channel: Channel::first(),
            });
        }
        let rendered = render(&song, SAMPLE_RATE);
        assert!(rendered.samples.iter().all(|sample| sample.abs() <= 1.0));
    }

    #[test]
    fn where_the_note_ends_the_tail_decays() {
        let rendered = render(&song_with_note(), SAMPLE_RATE);
        let frames = rendered.frames();
        let release_start = (0.5 * f64::from(SAMPLE_RATE)) as usize;
        let at_release = rendered.samples[release_start * 2].abs();
        let tail_end = rendered.samples[(frames - 1) * 2].abs();
        assert!(tail_end <= at_release);
    }

    #[test]
    fn wav_header_is_standard() {
        let rendered = render(&song_with_note(), SAMPLE_RATE);
        let wav = rendered.to_wav();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + rendered.samples.len() * 2);
        let data_len = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_len as usize, rendered.samples.len() * 2);
    }

    #[test]
    fn peak_tracks_velocity() {
        assert!(peak_for(Velocity::new(127).expect("valid")) > 0.99);
        assert!(peak_for(Velocity::new(64).expect("valid")) < 0.6);
    }
}
