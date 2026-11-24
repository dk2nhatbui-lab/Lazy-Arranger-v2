//! Deterministic music generators: humanise, drumify and chord-derived parts.
//!
//! These are rule-based assistants, not models: every result comes from the
//! song's own data plus a seed, and each call is one undoable edit.

use music_core::{Channel, PPQ, Pitch, Tick, Velocity};
use project::song::{Note, Song};
use project::{NoteId, TrackId};

use crate::ArrangingError;
use app_commands::{Command, EditSession};

/// A generated part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// Root notes on each chord.
    Bass,
    /// Sustained triads.
    Pad,
    /// Arpeggiated triads.
    Arpeggio,
}

/// Nudges note starts and velocities, keeping them inside the track.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn humanize(
    session: &mut EditSession,
    track: TrackId,
    timing: Tick,
    velocity: u8,
    seed: u64,
) -> Result<usize, ArrangingError> {
    let Some((mut notes, count)) = session
        .song()
        .tracks
        .iter()
        .find(|entry| entry.id == track)
        .map(|entry| (entry.notes.clone(), entry.notes.len()))
    else {
        return Ok(0);
    };
    let mut state = seed | 1;
    for note in &mut notes {
        let offset = jitter(&mut state, timing);
        note.start = (note.start + offset).max(0);
        let delta = jitter(&mut state, i64::from(velocity));
        let value = i64::from(note.velocity.as_u8()) + delta;
        if let Ok(velocity) = Velocity::new(value.clamp(1, 127) as u8) {
            note.velocity = velocity;
        }
    }
    notes.sort_by_key(|note| note.start);
    session.apply(Command::ReplaceTrackNotes { track, notes })?;
    Ok(count)
}

/// Replaces a track with a basic kit for every bar that has a chord.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn drumify(
    session: &mut EditSession,
    track: TrackId,
    song: &Song,
) -> Result<usize, ArrangingError> {
    let bar = song.time_signature.bar_ticks().max(1);
    let mut bars: Vec<Tick> = song
        .chords
        .iter()
        .map(|event| event.tick / bar * bar)
        .collect();
    bars.sort_unstable();
    bars.dedup();
    let mut notes = Vec::new();
    let mut next = 1_u32;
    for start in bars {
        for step in 0..8_u8 {
            let tick = start + Tick::from(step) * (PPQ / 2);
            let pitch = match step {
                0 | 4 => 36,
                2 | 6 => 38,
                _ => 42,
            };
            let velocity = if matches!(step, 0 | 4) { 112 } else { 78 };
            push(
                &mut notes,
                &mut next,
                tick,
                PPQ / 2,
                pitch,
                velocity,
                Channel::drums(),
            );
        }
    }
    let count = notes.len();
    session.apply(Command::ReplaceTrackNotes { track, notes })?;
    Ok(count)
}

/// Writes a bass, pad or arpeggio part from the chord timeline.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn write_part(
    session: &mut EditSession,
    track: TrackId,
    part: Part,
    song: &Song,
) -> Result<usize, ArrangingError> {
    let mut notes = Vec::new();
    let mut next = 1_u32;
    for (index, event) in song.chords.iter().enumerate() {
        let end = song
            .chords
            .get(index + 1)
            .map_or(event.tick + PPQ * 4, |next| next.tick)
            .max(event.tick + PPQ);
        let length = end - event.tick;
        match part {
            Part::Bass => {
                if let Some(root) = event.chord.bass_tone(2) {
                    push(
                        &mut notes,
                        &mut next,
                        event.tick,
                        length.min(PPQ * 2),
                        root.as_u8(),
                        100,
                        Channel::first(),
                    );
                }
            }
            Part::Pad => {
                for tone in event.chord.tones(3) {
                    push(
                        &mut notes,
                        &mut next,
                        event.tick,
                        length,
                        tone.as_u8(),
                        70,
                        Channel::first(),
                    );
                }
            }
            Part::Arpeggio => {
                for step in 0..4_u8 {
                    if let Some(tone) = event.chord.tones(4).get(usize::from(step % 3)) {
                        push(
                            &mut notes,
                            &mut next,
                            event.tick + Tick::from(step) * (PPQ / 2),
                            PPQ / 2,
                            tone.as_u8(),
                            88,
                            Channel::first(),
                        );
                    }
                }
            }
        }
    }
    let count = notes.len();
    session.apply(Command::ReplaceTrackNotes { track, notes })?;
    Ok(count)
}

/// Repeats the last `bars` bars of a track, appending the copy at the end.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn continue_song(
    session: &mut EditSession,
    track: TrackId,
    bars: usize,
) -> Result<usize, ArrangingError> {
    let song = session.song().clone();
    let bar = song.time_signature.bar_ticks().max(1);
    let bars = Tick::try_from(bars.max(1)).unwrap_or(1);
    let window = bar.saturating_mul(bars);
    let Some((mut notes, count)) = session
        .song()
        .tracks
        .iter()
        .find(|entry| entry.id == track)
        .map(|entry| (entry.notes.clone(), entry.notes.len()))
    else {
        return Ok(0);
    };
    let end = song.end_tick().max(window);
    let start = end.saturating_sub(window);
    let mut next = notes.iter().map(|note| note.id.get()).max().unwrap_or(0) + 1;
    let copies: Vec<Note> = notes
        .iter()
        .filter(|note| note.start >= start && note.start < end)
        .cloned()
        .collect();
    let copied = copies.len();
    for mut copy in copies {
        if let Ok(id) = NoteId::new(next) {
            next = next.saturating_add(1);
            copy.id = id;
            copy.start += window;
            notes.push(copy);
        }
    }
    notes.sort_by_key(|note| note.start);
    session.apply(Command::ReplaceTrackNotes { track, notes })?;
    let _ = count;
    Ok(copied)
}

/// Writes a seeded melody that follows the chord tones.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn melody(
    session: &mut EditSession,
    track: TrackId,
    song: &Song,
    seed: u64,
) -> Result<usize, ArrangingError> {
    let mut state = seed | 1;
    let mut notes = Vec::new();
    let mut next = 1_u32;
    for (index, event) in song.chords.iter().enumerate() {
        let end = song
            .chords
            .get(index + 1)
            .map_or(event.tick + PPQ * 4, |next| next.tick);
        let tones = event.chord.tones(5);
        if tones.is_empty() {
            continue;
        }
        let mut tick = event.tick;
        let mut step = 0_usize;
        while tick < end {
            let tone = tones[step % tones.len()];
            let lift = if step % 4 == 3 { 2 } else { 0 };
            let pitch = tone.as_u8().saturating_add(lift).min(127);
            push(
                &mut notes,
                &mut next,
                tick,
                PPQ / 2,
                pitch,
                84 + (step % 3) as u8 * 8,
                Channel::first(),
            );
            tick += PPQ / 2;
            step += 1;
        }
        let _ = &mut state;
    }
    let count = notes.len();
    session.apply(Command::ReplaceTrackNotes { track, notes })?;
    Ok(count)
}

fn jitter(state: &mut u64, amount: Tick) -> Tick {
    if amount <= 0 {
        return 0;
    }
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1);
    let span = amount.saturating_mul(2).saturating_add(1);
    Tick::try_from(*state % u64::try_from(span).unwrap_or(1)).unwrap_or(0) - amount
}

fn push(
    notes: &mut Vec<Note>,
    next: &mut u32,
    start: Tick,
    duration: Tick,
    pitch: u8,
    velocity: u8,
    channel: Channel,
) {
    let Ok(id) = NoteId::new(*next) else {
        return;
    };
    let Ok(pitch) = Pitch::new(pitch) else {
        return;
    };
    let velocity = Velocity::new(velocity).unwrap_or_else(|_| Velocity::new(80).expect("valid"));
    *next = next.saturating_add(1);
    notes.push(Note {
        id,
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
    use music_core::Chord;
    use project::song::{ChordEvent, ProjectFile, Track};

    fn session() -> EditSession {
        let mut song = Song::new("Generate", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 960,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        song.tracks
            .push(Track::new(TrackId::new(2).expect("valid"), "Drums"));
        song.tracks
            .push(Track::new(TrackId::new(3).expect("valid"), "Bass"));
        song.chords = vec![
            ChordEvent {
                tick: 0,
                chord: Chord::parse("C").expect("valid"),
            },
            ChordEvent {
                tick: 3840,
                chord: Chord::parse("F").expect("valid"),
            },
        ];
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn humanize_moves_within_the_bounds_and_undoes() {
        let mut session = session();
        let track = TrackId::new(1).expect("valid");
        let count = humanize(&mut session, track, 20, 5, 7).expect("humanize");
        assert_eq!(count, 1);
        let note = &session.song().tracks[0].notes[0];
        assert!((960 - 20..=960 + 20).contains(&note.start));
        assert!((95..=105).contains(&note.velocity.as_u8()));
        session.undo().expect("undo");
        assert_eq!(session.song().tracks[0].notes[0].start, 960);
    }

    #[test]
    fn generated_bass_is_not_routed_to_drums() {
        let mut session = session();
        let song = session.song().clone();
        write_part(
            &mut session,
            TrackId::new(3).expect("valid"),
            Part::Bass,
            &song,
        )
        .expect("bass");
        let notes = &session.song().tracks[2].notes;
        assert!(!notes.is_empty());
        assert!(notes.iter().all(|note| note.channel.zero_based() == 0));
    }

    #[test]
    fn drumify_builds_eight_steps_per_chord_bar() {
        let mut session = session();
        let song = session.song().clone();
        let count = drumify(&mut session, TrackId::new(2).expect("valid"), &song).expect("drums");
        assert_eq!(count, 16);
        let notes = &session.song().tracks[1].notes;
        assert_eq!(
            notes.iter().filter(|note| note.pitch.as_u8() == 36).count(),
            4
        );
        assert!(notes.iter().all(|note| note.channel.zero_based() == 9));
    }

    #[test]
    fn continuing_repeats_the_last_bars() {
        let mut session = session();
        let count =
            continue_song(&mut session, TrackId::new(1).expect("valid"), 1).expect("continue");
        assert_eq!(count, 1);
        let notes = &session.song().tracks[0].notes;
        assert_eq!(notes.len(), 2, "the copied note is appended");
        assert!(notes[1].start >= notes[0].start);
    }

    #[test]
    fn melody_follows_chord_tones() {
        let mut session = session();
        let song = session.song().clone();
        let count =
            melody(&mut session, TrackId::new(3).expect("valid"), &song, 9).expect("melody");
        assert!(count >= 8, "two bars of eighths: {count}");
        let notes = &session.song().tracks[2].notes;
        assert!(notes.iter().all(|note| note.pitch.as_u8() % 12 <= 11));
        assert!(notes.windows(2).all(|pair| pair[0].start <= pair[1].start));
    }

    #[test]
    fn parts_follow_the_chords() {
        let mut session = session();
        let song = session.song().clone();
        let count = write_part(
            &mut session,
            TrackId::new(3).expect("valid"),
            Part::Bass,
            &song,
        )
        .expect("bass");
        assert_eq!(count, 2);
        let notes = &session.song().tracks[2].notes;
        assert_eq!(notes[0].pitch.as_u8() % 12, 0);
        assert_eq!(notes[1].pitch.as_u8() % 12, 5);
        assert!(
            write_part(
                &mut session,
                TrackId::new(3).expect("valid"),
                Part::Pad,
                &song
            )
            .is_ok()
        );
        assert!(
            write_part(
                &mut session,
                TrackId::new(3).expect("valid"),
                Part::Arpeggio,
                &song
            )
            .is_ok()
        );
    }
}
