//! A click track generated from the meter and tempo map.

use music_core::Tick;
use project::song::Song;

use crate::RenderedAudio;

/// One click: tick and whether it is a bar downbeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Click {
    /// Position.
    pub tick: Tick,
    /// True on the first beat of a bar.
    pub accent: bool,
}

/// Every beat from zero to the song end, with downbeats accented.
#[must_use]
pub fn clicks(song: &Song) -> Vec<Click> {
    let beat = song
        .time_signature
        .bar_ticks()
        .max(1)
        .div_euclid(Tick::from(song.time_signature.numerator).max(1));
    let mut out = Vec::new();
    let mut tick = 0;
    let end = song.end_tick().max(1);
    let mut index = 0_i64;
    while tick < end {
        out.push(Click {
            tick,
            accent: index % i64::from(song.time_signature.numerator.max(1)) == 0,
        });
        tick += beat;
        index += 1;
    }
    out
}

/// Mixes short clicks into a rendered buffer.
pub fn mix(rendered: &mut RenderedAudio, song: &Song, gain: f32) {
    mix_accent(rendered, song, gain, 1.0);
}

/// Mixes clicks with a separate downbeat accent amount, 0..=1.
pub fn mix_accent(rendered: &mut RenderedAudio, song: &Song, gain: f32, accent: f32) {
    let rate = rendered.sample_rate;
    for click in clicks(song) {
        let start = (song.tempo_map.seconds_at(click.tick) * f64::from(rate)).round() as usize;
        let length = (f64::from(rate) * 0.03) as usize;
        let frequency = if click.accent { 1500.0 } else { 1000.0 };
        let amplitude = if click.accent {
            0.22 * gain * accent.clamp(0.0, 1.0)
        } else {
            0.12 * gain
        };
        for frame in 0..length {
            let index = (start + frame) * 2;
            if index + 1 >= rendered.samples.len() {
                break;
            }
            let seconds = frame as f64 / f64::from(rate);
            let envelope = 1.0 - (frame as f64 / length as f64);
            let value = (std::f64::consts::TAU * frequency * seconds).sin() as f32
                * amplitude
                * envelope as f32;
            rendered.samples[index] += value;
            rendered.samples[index + 1] += value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{PPQ, TimeSignature};
    use project::song::Song;

    fn song() -> Song {
        let mut song = Song::new("Metronome", 120.0);
        song.time_signature = TimeSignature::new(4, 4).expect("valid");
        song
    }

    #[test]
    fn an_accent_scale_reduces_the_downbeat_only() {
        let mut song = project::Song::new("Clicks", 120.0);
        let mut track =
            project::song::Track::new(project::TrackId::new(1).expect("track"), "Click");
        track.notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("note"),
            start: 0,
            duration: music_core::PPQ * 4,
            pitch: music_core::Pitch::from_name("C4").expect("pitch"),
            velocity: music_core::Velocity::new(100).expect("velocity"),
            channel: music_core::Channel::first(),
        });
        song.tracks.push(track);
        let mut full = RenderedAudio {
            samples: vec![0.0; 96_000 * 2],
            sample_rate: 48_000,
        };
        let mut muted = full.clone();
        mix_accent(&mut full, &song, 1.0, 1.0);
        mix_accent(&mut muted, &song, 1.0, 0.0);
        let difference: f32 = full
            .samples
            .iter()
            .zip(&muted.samples)
            .map(|(left, right)| (left - right).abs())
            .sum();
        assert!(difference > 0.0);
    }

    #[test]
    fn every_beat_gets_a_click_and_bars_accent_the_first() {
        let mut song = song();
        song.chords.push(project::song::ChordEvent {
            tick: 0,
            chord: music_core::Chord::parse("C").expect("valid"),
        });
        song.tracks.push(project::song::Track::new(
            project::TrackId::new(1).expect("valid"),
            "Piano",
        ));
        song.tracks[0].notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: 8 * PPQ,
            pitch: music_core::Pitch::from_name("C4").expect("valid"),
            velocity: music_core::Velocity::new(100).expect("valid"),
            channel: music_core::Channel::first(),
        });
        let clicks = clicks(&song);
        assert_eq!(clicks.len(), 8, "two bars of four beats");
        assert!(clicks[0].accent && clicks[4].accent);
        assert!(!clicks[1].accent && !clicks[5].accent);
        assert_eq!(clicks[1].tick, PPQ);
    }

    #[test]
    fn mixing_adds_energy_where_clicks_land() {
        let mut song = song();
        let mut track =
            project::song::Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: 8 * PPQ,
            pitch: music_core::Pitch::from_name("C4").expect("valid"),
            velocity: music_core::Velocity::new(1).expect("valid"),
            channel: music_core::Channel::first(),
        });
        song.tracks.push(track);
        let mut rendered = RenderedAudio {
            samples: vec![0.0; 48_000 * 2],
            sample_rate: 48_000,
        };
        mix(&mut rendered, &song, 1.0);
        assert!(rendered.samples[5 * 2].abs() > 0.01, "click near zero");
        let frame = (48_000.0 * 0.5) as usize + 40;
        assert!(
            rendered.samples[frame * 2].abs() > 0.001,
            "click at one beat"
        );
        let quiet = (48_000.0 * 0.2) as usize;
        assert!(rendered.samples[quiet * 2].abs() < 0.01, "silence between");
    }
}
