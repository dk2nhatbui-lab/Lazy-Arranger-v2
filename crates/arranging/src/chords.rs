//! Chord recognition written back to the timeline.
//!
//! Scans the song bar by bar, recognizes a chord from the notes that start in
//! each bar, and replaces that bar's chords in one edit per found bar.

use app_commands::{Command, EditSession};
use music_core::Tick;

use crate::ArrangingError;

/// Finds chords from the notes and writes them, returning how many bars got one.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when an edit is rejected.
pub fn find_chords(session: &mut EditSession) -> Result<usize, ArrangingError> {
    let meter = session.song().time_signature;
    let bar = meter.bar_ticks().max(1);
    let end = session.song().end_tick().max(bar);
    let mut found = Vec::new();
    let mut start: Tick = 0;
    while start < end {
        let mut pitches = Vec::new();
        let mut lowest = 127_u8;
        for track in &session.song().tracks {
            for note in &track.notes {
                if note.start >= start && note.start < start + bar {
                    pitches.push(note.pitch.as_u8());
                    lowest = lowest.min(note.pitch.as_u8());
                }
            }
        }
        if let Some(recognized) = arranger::recognize(&pitches, Some(lowest)) {
            found.push((start, recognized.chord));
        }
        start += bar;
    }
    for (tick, chord) in &found {
        let existing: Vec<Tick> = session
            .song()
            .chords
            .iter()
            .filter(|event| event.tick >= *tick && event.tick < tick + bar)
            .map(|event| event.tick)
            .collect();
        for event_tick in existing {
            session.apply(Command::RemoveChord { tick: event_tick })?;
        }
        session.apply(Command::SetChord {
            tick: *tick,
            chord: *chord,
        })?;
    }
    Ok(found.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::NoteId;
    use project::TrackId;
    use project::song::{Note, ProjectFile, Song, Track};

    #[test]
    fn a_c_major_bar_becomes_a_c_chord() {
        let mut song = Song::new("Chords", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, name) in ["C4", "E4", "G4"].into_iter().enumerate() {
            track.notes.push(Note {
                id: NoteId::new(index as u32 + 1).expect("valid"),
                start: 0,
                duration: PPQ,
                pitch: Pitch::from_name(name).expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        let mut session = EditSession::new(ProjectFile::new(song)).expect("valid");
        assert_eq!(find_chords(&mut session).expect("find"), 1);
        assert_eq!(
            session.song().chords[0]
                .chord
                .format(music_core::Spelling::Sharp),
            "C"
        );
        assert!(session.undo().is_ok());
    }
}
