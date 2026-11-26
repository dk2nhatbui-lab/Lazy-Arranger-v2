//! Live synth lanes: a track's notes played in the audio callback.
//!
//! A lane is built on the control thread from a track's notes. Each note
//! knows its start and length in frames, and either a SoundFont voice or a
//! built-in synth sound. The audio callback walks the notes in time order,
//! keeps a fixed pool of sounding voices, and writes the track's signal
//! before its fader. It never allocates: the pool's capacity is reserved up
//! front, and a SoundFont voice is a plain value copied from its note.
//!
//! Every note renders the same samples as the offline renderer
//! (`render_parts` in the crate root), so playback and export match.

use std::sync::Arc;

use project::song::{Patch, Song, Track};

use crate::soundfont::{NoteVoice, SoundFont};
use crate::{Timbre, frequency, sample, timbre_for};

/// Notes that can sound at once in one lane. Further notes wait for a free
/// voice.
pub const MAX_VOICES: usize = 128;

/// How one note makes its sound.
// Kept inline: a box per note would add an allocation per note for no gain,
// since notes are read in place.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug)]
enum Sound {
    /// A SoundFont note, its voice ready to start.
    Font(NoteVoice),
    /// The built-in synth.
    Synth {
        timbre: Timbre,
        patch: Option<Patch>,
        frequency: f64,
        amplitude: f64,
        release: usize,
        note_seconds: f64,
        pitch: u8,
    },
}

/// One note of a lane.
#[derive(Clone, Copy, Debug)]
struct Note {
    /// First frame, before the lane's delay.
    start: usize,
    /// Frames it sounds for.
    len: usize,
    sound: Sound,
}

/// A sounding note.
#[derive(Clone, Copy, Debug)]
struct Voice {
    note: usize,
    /// Frames already played.
    at: usize,
    /// The running SoundFont voice, for a font note.
    font: Option<NoteVoice>,
}

/// A track's notes, ready to play in the audio callback.
#[derive(Debug)]
pub struct SynthLane {
    notes: Vec<Note>,
    /// Longest note, to find the notes sounding at a position.
    longest: usize,
    font: Option<Arc<SoundFont>>,
    sample_rate: u32,
    /// Frames the lane is delayed by, to line up with plug-in latency
    /// elsewhere in the song.
    delay: usize,
    voices: Vec<Voice>,
    /// Next note to start.
    cursor: usize,
    /// The source frame the next call continues from, when it is contiguous.
    next: Option<usize>,
}

impl Clone for SynthLane {
    fn clone(&self) -> Self {
        let mut voices = Vec::with_capacity(MAX_VOICES);
        voices.extend_from_slice(&self.voices);
        Self {
            notes: self.notes.clone(),
            longest: self.longest,
            font: self.font.clone(),
            sample_rate: self.sample_rate,
            delay: self.delay,
            voices,
            cursor: self.cursor,
            next: self.next,
        }
    }
}

/// True when the live synth can play this track: notes only, no recorded
/// audio and no plug-ins (stage 3 brings those into the engine).
#[must_use]
pub fn can_play(track: &Track) -> bool {
    track.audio.is_none()
        && track.comps.is_empty()
        && !track
            .plugins
            .iter()
            .any(|slot| !slot.bypassed && !slot.identifier.is_empty())
}

/// A fingerprint of everything a track's synth lane is built from: its
/// notes, sound and the tempo. When it changes, the lane is out of date.
#[must_use]
pub fn signature(song: &Song, track: &Track) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for note in &track.notes {
        note.start.hash(&mut hasher);
        note.duration.hash(&mut hasher);
        note.pitch.as_u8().hash(&mut hasher);
        note.velocity.as_u8().hash(&mut hasher);
    }
    // Small values without `Hash`: their debug text is enough.
    format!(
        "{:?}{:?}{:?}{:?}",
        track.soundfont,
        track.patch,
        timbre_for(track),
        song.tempo_map
    )
    .hash(&mut hasher);
    for lane in &track.automation {
        if lane.target == project::song::AutomationTarget::Brightness {
            format!("{:?}", lane.points).hash(&mut hasher);
        }
    }
    hasher.finish()
}

impl SynthLane {
    /// Builds a track's lane. `font` is the track's SoundFont, already
    /// loaded; without one the built-in synth plays every note.
    #[must_use]
    pub fn build(
        song: &Song,
        track: &Track,
        font: Option<Arc<SoundFont>>,
        sample_rate: u32,
        delay: usize,
    ) -> Self {
        let timbre = timbre_for(track);
        let rate = f64::from(sample_rate);
        let mut notes: Vec<Note> = track
            .notes
            .iter()
            .filter_map(|note| {
                // The same arithmetic as the offline renderer, so the frames
                // match exactly.
                let start_seconds = song.tempo_map.seconds_at(note.start);
                let end_seconds = song
                    .tempo_map
                    .seconds_at(note.start.saturating_add(note.duration.max(1)));
                let start = (start_seconds * rate).round() as usize;
                let stop = (end_seconds * rate).round() as usize;
                let note_seconds = (stop - start) as f64 / rate;
                if let (Some(font), Some(reference)) = (&font, &track.soundfont)
                    && let Some(voice) = NoteVoice::new(
                        font,
                        reference.bank,
                        reference.program,
                        note.pitch.as_u8(),
                        note.velocity.as_u8(),
                        note_seconds as f32,
                        sample_rate,
                    )
                {
                    return Some(Note {
                        start,
                        len: voice.frames(),
                        sound: Sound::Font(voice),
                    });
                }
                let patch = track.patch.map(|mut patch| {
                    if let Some(brightness) = track.brightness_at(note.start) {
                        patch.brightness = brightness;
                    }
                    patch
                });
                (stop > start).then(|| Note {
                    start,
                    len: stop - start,
                    sound: Sound::Synth {
                        timbre,
                        patch,
                        frequency: frequency(note.pitch),
                        amplitude: f64::from(note.velocity.as_u8()) / 127.0 * 0.18,
                        release: (0.08 * rate) as usize,
                        note_seconds,
                        pitch: note.pitch.as_u8(),
                    },
                })
            })
            .collect();
        notes.sort_by_key(|note| note.start);
        let longest = notes.iter().map(|note| note.len).max().unwrap_or(0);
        Self {
            notes,
            longest,
            font,
            sample_rate,
            delay,
            voices: Vec::with_capacity(MAX_VOICES),
            cursor: 0,
            next: None,
        }
    }

    /// Restarts at `frame` (a source frame): notes already sounding there
    /// join part way through.
    fn locate(&mut self, frame: usize) {
        self.voices.clear();
        self.cursor = self.notes.partition_point(|note| note.start < frame);
        let first = self
            .notes
            .partition_point(|note| note.start < frame.saturating_sub(self.longest));
        let pcm: &[i16] = self.font.as_deref().map_or(&[], |font| &font.pcm);
        for index in first..self.cursor {
            let note = &self.notes[index];
            let at = frame - note.start;
            if at >= note.len || self.voices.len() == MAX_VOICES {
                continue;
            }
            let font = match note.sound {
                Sound::Font(mut voice) => {
                    // Run the voice up to here so its filter and loop state
                    // are where they would be.
                    let mut alive = true;
                    for _ in 0..at {
                        if voice.next(pcm).is_none() {
                            alive = false;
                            break;
                        }
                    }
                    if !alive {
                        continue;
                    }
                    Some(voice)
                }
                Sound::Synth { .. } => None,
            };
            self.voices.push(Voice {
                note: index,
                at,
                font,
            });
        }
        self.next = Some(frame);
    }

    /// Writes the lane's interleaved stereo signal for output samples
    /// `pos..pos + output.len()` into `output`.
    pub fn render(&mut self, output: &mut [f32], pos: usize) {
        output.fill(0.0);
        let first_frame = pos / 2;
        let frames = output.len() / 2;
        // Source frames start `delay` later than output frames.
        let lead = self.delay.saturating_sub(first_frame).min(frames);
        let source = (first_frame + lead).saturating_sub(self.delay);
        if lead == frames {
            return;
        }
        if self.next != Some(source) {
            self.locate(source);
        }
        let pcm: &[i16] = self.font.as_deref().map_or(&[], |font| &font.pcm);
        let rate = f64::from(self.sample_rate);
        for (offset, pair) in output
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .enumerate()
            .skip(lead)
        {
            let frame = source + offset - lead;
            while self.cursor < self.notes.len() && self.notes[self.cursor].start <= frame {
                if self.voices.len() < MAX_VOICES {
                    let note = &self.notes[self.cursor];
                    self.voices.push(Voice {
                        note: self.cursor,
                        at: frame - note.start,
                        font: match note.sound {
                            Sound::Font(voice) => Some(voice),
                            Sound::Synth { .. } => None,
                        },
                    });
                }
                self.cursor += 1;
            }
            let mut index = 0;
            while index < self.voices.len() {
                let voice = &mut self.voices[index];
                let note = &self.notes[voice.note];
                let value = if voice.at >= note.len {
                    None
                } else {
                    match &mut voice.font {
                        Some(font) => font.next(pcm),
                        None => synth_value(&note.sound, voice.at, note.len, rate),
                    }
                };
                match value {
                    Some((left, right)) => {
                        pair[0] += left;
                        pair[1] += right;
                        voice.at += 1;
                        index += 1;
                    }
                    None => {
                        self.voices.swap_remove(index);
                    }
                }
            }
        }
        self.next = Some(source + frames - lead);
    }

    /// The whole lane rendered into a new buffer of `len` samples, for
    /// export and tests.
    #[must_use]
    pub fn render_all(&self, len: usize) -> Vec<f32> {
        let mut lane = self.clone();
        lane.next = None;
        let mut out = vec![0.0; len & !1];
        for (index, chunk) in out.chunks_mut(1024).enumerate() {
            lane.render(chunk, index * 1024);
        }
        out
    }
}

/// One frame of a built-in synth note, `at` frames in, as the offline
/// renderer computes it.
fn synth_value(sound: &Sound, at: usize, len: usize, rate: f64) -> Option<(f32, f32)> {
    let Sound::Synth {
        timbre,
        patch,
        frequency,
        amplitude,
        release,
        note_seconds,
        pitch,
    } = *sound
    else {
        return None;
    };
    let t = at as f64 / rate;
    let remaining = len - at;
    let envelope = if remaining < release {
        remaining as f64 / release as f64
    } else {
        1.0
    };
    let sound = match patch {
        Some(patch) => patch.sample(frequency, t as f32, note_seconds as f32),
        None => sample(timbre, frequency, t, note_seconds, pitch),
    };
    let value = (sound * amplitude * envelope) as f32;
    Some((value, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, Pitch, Velocity};
    use project::TrackId;
    use project::song::Note as SongNote;

    const RATE: u32 = 8_000;

    fn song(patch: bool) -> Song {
        let mut song = Song::default();
        let mut track = Track::new(TrackId::new(1).expect("track"), "Piano");
        for (id, (start, pitch)) in [(0, 60), (240, 64), (480, 67), (480, 72)]
            .into_iter()
            .enumerate()
        {
            track.notes.push(SongNote {
                id: project::NoteId::new(id as u32 + 1).expect("note"),
                start,
                duration: music_core::PPQ,
                pitch: Pitch::new(pitch).expect("pitch"),
                velocity: Velocity::new(90).expect("velocity"),
                channel: Channel::first(),
            });
        }
        if patch {
            track.patch = Some(Patch {
                waveform: project::song::Waveform::Saw,
                attack: 0.01,
                decay: 0.1,
                sustain: 0.6,
                release: 0.05,
                brightness: 0.4,
            });
        }
        song.tracks.push(track);
        song
    }

    fn font(looped: bool) -> SoundFont {
        use crate::soundfont::{Instrument, InstrumentZone, Preset, PresetZone, Sample};
        SoundFont {
            name: String::from("Test"),
            presets: vec![Preset {
                name: String::from("Test"),
                bank: 0,
                program: 0,
                zones: vec![PresetZone {
                    instrument: Some(0),
                    key_low: 0,
                    key_high: 127,
                    vel_low: 0,
                    vel_high: 127,
                }],
            }],
            instruments: vec![Instrument {
                name: String::from("Test"),
                zones: vec![InstrumentZone {
                    sample: Some(0),
                    key_low: 0,
                    key_high: 127,
                    vel_low: 0,
                    vel_high: 127,
                    root_override: Some(60),
                    coarse_tune: 0,
                    fine_tune: 0,
                    sample_modes: u16::from(looped),
                    delay: -12_000,
                    attack: -4_000,
                    hold: -12_000,
                    decay: -2_000,
                    sustain: 100,
                    release: -3_000,
                    filter_cutoff: 9_000,
                    filter_q: 50,
                }],
            }],
            samples: vec![Sample {
                name: String::from("Wave"),
                start: 0,
                end: 400,
                loop_start: 100,
                loop_end: 380,
                rate: 8_000,
                root: 60,
                correction: 0,
                kind: 1,
                link: 0,
            }],
            pcm: (0..400)
                .map(|index| ((index as f32 * 0.3).sin() * 12_000.0) as i16)
                .collect(),
        }
    }

    /// The offline render of the song's only track at unity gain.
    fn offline(song: &Song, font: Option<&SoundFont>) -> Vec<f32> {
        crate::render_offline_with_soundfont(song, RATE, &mut |_| font.cloned()).samples
    }

    fn assert_close(live: &[f32], offline: &[f32]) {
        assert_eq!(live.len(), offline.len());
        let worst = live
            .iter()
            .zip(offline)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        // -90 dB.
        assert!(worst < 3.2e-5, "live and offline differ by {worst}");
    }

    fn lane(song: &Song, font: Option<SoundFont>) -> SynthLane {
        SynthLane::build(song, &song.tracks[0], font.map(Arc::new), RATE, 0)
    }

    #[test]
    fn the_builtin_synth_plays_live_as_it_renders() {
        for patch in [false, true] {
            let song = song(patch);
            let expected = offline(&song, None);
            assert_close(&lane(&song, None).render_all(expected.len()), &expected);
        }
    }

    #[test]
    fn soundfont_notes_play_live_as_they_render() {
        for looped in [false, true] {
            let mut song = song(false);
            song.tracks[0].soundfont = Some(project::song::SoundFontRef {
                path: String::from("test.sf2"),
                bank: 0,
                program: 0,
            });
            let font = font(looped);
            let expected = offline(&song, Some(&font));
            assert!(expected.iter().any(|value| value.abs() > 0.01));
            assert_close(
                &lane(&song, Some(font)).render_all(expected.len()),
                &expected,
            );
        }
    }

    #[test]
    fn a_jump_lands_inside_sounding_notes() {
        let mut song = song(false);
        song.tracks[0].soundfont = Some(project::song::SoundFontRef {
            path: String::from("test.sf2"),
            bank: 0,
            program: 0,
        });
        let font = font(true);
        let mut lane = lane(&song, Some(font));
        let whole = lane.render_all(20_000);
        // Jump into the middle of the chord and play on, in quantum steps.
        let from = 7_000 * 2;
        let mut out = vec![0.0; 2_000];
        for (index, chunk) in out.chunks_mut(64).enumerate() {
            lane.render(chunk, from + index * 64);
        }
        assert_close(&out, &whole[from..from + 2_000]);
    }

    #[test]
    fn a_delayed_lane_starts_later() {
        let song = song(false);
        let plain = lane(&song, None).render_all(4_000);
        let delayed = SynthLane::build(&song, &song.tracks[0], None, RATE, 100).render_all(4_000);
        assert!(delayed[..200].iter().all(|value| *value == 0.0));
        assert_close(&delayed[200..], &plain[..3_800]);
    }

    #[test]
    fn a_note_edit_changes_the_signature() {
        let song = song(false);
        let before = signature(&song, &song.tracks[0]);
        let mut edited = song.clone();
        edited.tracks[0].notes[1].start += 10;
        assert_ne!(before, signature(&edited, &edited.tracks[0]));
        let mut renamed = song.clone();
        renamed.tracks[0].volume = 10;
        assert_eq!(before, signature(&renamed, &renamed.tracks[0]));
    }
}
