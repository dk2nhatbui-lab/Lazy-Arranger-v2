//! The built-in demo song: sixteen bars of drums, bass, piano and pad.
//!
//! A: C Am F G twice. B: F G C Am twice. 108 bpm in 4/4, with clips covering
//! the whole form and two marks so the score and the mixer have something to
//! show on first run.

use music_core::{Channel, Chord, PPQ, Pitch, Tick, Velocity};
use project::song::{Clip, Note, Song, Track};
use project::{ClipId, NoteId, TrackId};

const PROGRESSION: [&str; 8] = ["C", "Am", "F", "G", "F", "G", "C", "Am"];

/// Builds the demo song.
#[must_use]
pub fn demo_song() -> Song {
    let mut song = Song::new("Lazy Arranger demo", 108.0);
    let bar = song.time_signature.bar_ticks();
    let drum_channel = Channel::drums();
    let mut next_note = 0_u32;
    let mut next_clip = 0_u32;

    let mut drums = Track::new(TrackId::new(1).expect("valid"), "Drums");
    let mut bass = Track::new(TrackId::new(2).expect("valid"), "Bass");
    let mut piano = Track::new(TrackId::new(3).expect("valid"), "Electric Piano");
    let mut pad = Track::new(TrackId::new(4).expect("valid"), "Pad");
    drums.pan = 64;
    piano.pan = 54;
    pad.pan = 78;
    piano.volume = 92;
    pad.volume = 70;

    for phrase in 0..2 {
        for (index, symbol) in PROGRESSION.iter().enumerate() {
            let start = (phrase * 8 + index) as Tick * bar;
            let Ok(chord) = Chord::parse(symbol) else {
                continue;
            };
            song.chords.push(project::ChordEvent { tick: start, chord });
            // Drums: kick 1 and 3, snare 2 and 4, closed hats on eighths.
            for beat in 0..8_u8 {
                let tick = start + Tick::from(beat) * (PPQ / 2);
                let pitch = match beat {
                    0 | 4 => Some(36_u8),
                    2 | 6 => Some(38),
                    _ => Some(42),
                };
                if let Some(pitch) = pitch {
                    next_note += 1;
                    push_note(
                        &mut drums,
                        next_note,
                        tick,
                        PPQ / 2,
                        pitch,
                        if matches!(beat, 0 | 4) { 110 } else { 76 },
                        drum_channel,
                    );
                }
            }
            // Bass: root on every beat, the fifth on beat three.
            for beat in 0..4_u8 {
                let tick = start + Tick::from(beat) * PPQ;
                let pitch = if beat == 2 {
                    chord.bass_tone(2).map(|root| transpose(root, 7))
                } else {
                    chord.bass_tone(2)
                };
                if let Some(pitch) = pitch {
                    next_note += 1;
                    push_note(
                        &mut bass,
                        next_note,
                        tick,
                        PPQ,
                        pitch.as_u8(),
                        96,
                        Channel::new(1).unwrap_or_else(|_| Channel::first()),
                    );
                }
            }
            // Piano: triads on beats one and three; pad holds the bar.
            for (offset, velocity) in [(0_u8, 88_u8), (2, 76)] {
                for tone in chord.tones(4) {
                    next_note += 1;
                    push_note(
                        &mut piano,
                        next_note,
                        start + Tick::from(offset) * PPQ,
                        PPQ,
                        tone.as_u8(),
                        velocity,
                        Channel::new(2).unwrap_or_else(|_| Channel::first()),
                    );
                }
            }
            for tone in chord.tones(3) {
                next_note += 1;
                push_note(
                    &mut pad,
                    next_note,
                    start,
                    bar,
                    tone.as_u8(),
                    58,
                    Channel::new(3).unwrap_or_else(|_| Channel::first()),
                );
            }
        }
    }

    let form_end = 16 * bar;
    for track in [&mut drums, &mut bass, &mut piano, &mut pad] {
        next_clip += 1;
        if let Ok(id) = ClipId::new(next_clip) {
            track.clips.push(Clip {
                id,
                name: String::from(if form_end > 8 * bar {
                    "Whole form"
                } else {
                    "Loop"
                }),
                start: 0,
                length: form_end,
            });
        }
        song.tracks.push(track.clone());
    }
    song.lyrics.push(project::song::LyricEvent {
        tick: 0,
        text: String::from("Lazy"),
    });
    song.lyrics.push(project::song::LyricEvent {
        tick: bar,
        text: String::from("Arrange"),
    });
    song.marks.push(project::song::Mark {
        tick: 0,
        kind: project::song::MarkKind::Dynamic(project::song::Dynamic::Mf),
    });
    song.marks.push(project::song::Mark {
        tick: 8 * bar,
        kind: project::song::MarkKind::Dynamic(project::song::Dynamic::F),
    });
    song.markers.push(project::Marker {
        tick: 0,
        name: String::from("A"),
    });
    song.markers.push(project::Marker {
        tick: 8 * bar,
        name: String::from("B"),
    });
    song.scene_names = vec![String::from("Demo")];
    song
}

/// A short, slow four-chord demo for Kids Mode.
#[must_use]
pub fn kids_song() -> Song {
    let mut song = Song::new("Kids demo", 90.0);
    let bar = song.time_signature.bar_ticks();
    let progression = ["C", "F", "G", "C"];
    let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
    let mut next = 0_u32;
    for (index, symbol) in progression.iter().enumerate() {
        let start = index as Tick * bar;
        let Ok(chord) = Chord::parse(symbol) else {
            continue;
        };
        song.chords.push(project::ChordEvent { tick: start, chord });
        for tone in chord.tones(3) {
            next += 1;
            push_note(
                &mut track,
                next,
                start,
                bar,
                tone.as_u8(),
                80,
                Channel::first(),
            );
        }
    }
    song.tracks.push(track);
    song.lyrics.push(project::song::LyricEvent {
        tick: 0,
        text: String::from("C"),
    });
    song
}

fn transpose(pitch: Pitch, semitones: u8) -> Pitch {
    let value = pitch.as_u8().saturating_add(semitones).min(127);
    Pitch::new(value).unwrap_or(pitch)
}

fn push_note(
    track: &mut Track,
    id: u32,
    start: Tick,
    duration: Tick,
    pitch: u8,
    velocity: u8,
    channel: Channel,
) {
    let Ok(note_id) = NoteId::new(id) else {
        return;
    };
    let Ok(pitch) = Pitch::new(pitch) else {
        return;
    };
    let velocity = Velocity::new(velocity).unwrap_or_else(|_| Velocity::new(80).expect("valid"));
    track.notes.push(Note {
        id: note_id,
        start,
        duration,
        pitch,
        velocity,
        channel,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_is_a_valid_four_track_song() {
        let song = demo_song();
        let project = project::song::ProjectFile::new(song.clone());
        let issues = project::validate::validate(&project);
        assert!(issues.is_empty(), "demo must validate: {issues:?}");
        assert_eq!(song.tracks.len(), 4);
        assert_eq!(song.chords.len(), 16);
        assert_eq!(song.tracks[0].notes.len(), 128, "sixteen bars of eighths");
        assert!(
            song.tracks[0]
                .notes
                .iter()
                .all(|note| note.channel.zero_based() == 9)
        );
        let end = 16 * song.time_signature.bar_ticks();
        assert!(
            song.tracks
                .iter()
                .flat_map(|track| &track.notes)
                .all(|note| note.start < end)
        );
        assert!(song.tracks.iter().all(|track| track.clips.len() == 1));
        assert_eq!(song.marks.len(), 2);
        assert_eq!(song.markers.len(), 2);
    }

    #[test]
    fn the_kids_demo_is_short_and_slow() {
        let song = kids_song();
        assert_eq!(song.chords.len(), 4);
        assert_eq!(song.tracks.len(), 1);
        assert_eq!(song.tracks[0].notes.len(), 12);
        assert!(song.tempo_map.bpm_at(0) < 100.0);
        let issues = project::validate::validate(&project::song::ProjectFile::new(song.clone()));
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn the_demo_records_a_wav_and_midi_note_range() {
        let rendered = audio::render(&demo_song(), audio::SAMPLE_RATE);
        assert!(rendered.duration_seconds() > 30.0);
        assert!(rendered.samples.iter().any(|sample| sample.abs() > 0.01));
    }
}
