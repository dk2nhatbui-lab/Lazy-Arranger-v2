//! Clipboard for notes and chords.
//!
//! Items keep their offset from the earliest copied event, so a paste lands
//! where the caller asks and a cut can be undone like any other edit. Each
//! paste is one undo step.

use music_core::{Chord, Tick};
use project::TrackId;
use project::song::{Note, Song};

use crate::{Command, EditSession};

/// A note and its offset from the clipboard origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteItem {
    /// Offset from the earliest copied note.
    pub offset: Tick,
    /// The note, with its original id and start kept for reference.
    pub note: Note,
}

/// A chord and its offset from the clipboard origin.
#[derive(Clone, Debug, PartialEq)]
pub struct ChordItem {
    /// Offset from the earliest copied chord.
    pub offset: Tick,
    /// The chord.
    pub chord: Chord,
}

/// Copied events.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clipboard {
    /// Notes in start order.
    pub notes: Vec<NoteItem>,
    /// Chords in tick order.
    pub chords: Vec<ChordItem>,
}

impl Clipboard {
    /// True when nothing was copied.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty() && self.chords.is_empty()
    }

    /// Copies the given notes of one track.
    #[must_use]
    pub fn from_notes(song: &Song, track: TrackId, ids: &[project::NoteId]) -> Self {
        let Some(entry) = song.tracks.iter().find(|entry| entry.id == track) else {
            return Self::default();
        };
        let mut chosen: Vec<&Note> = entry
            .notes
            .iter()
            .filter(|note| ids.contains(&note.id))
            .collect();
        chosen.sort_by_key(|note| note.start);
        let origin = chosen.first().map_or(0, |note| note.start);
        Self {
            notes: chosen
                .into_iter()
                .map(|note| NoteItem {
                    offset: note.start - origin,
                    note: note.clone(),
                })
                .collect(),
            chords: Vec::new(),
        }
    }

    /// Copies notes that start inside `start..end`.
    #[must_use]
    pub fn from_range(song: &Song, track: TrackId, start: Tick, end: Tick) -> Self {
        let Some(entry) = song.tracks.iter().find(|entry| entry.id == track) else {
            return Self::default();
        };
        let ids: Vec<project::NoteId> = entry
            .notes
            .iter()
            .filter(|note| note.start >= start && note.start < end)
            .map(|note| note.id)
            .collect();
        Self::from_notes(song, track, &ids)
    }

    /// Copies chords inside `start..end`.
    #[must_use]
    pub fn from_chords(song: &Song, start: Tick, end: Tick) -> Self {
        let mut chosen: Vec<(Tick, Chord)> = song
            .chords
            .iter()
            .filter(|event| event.tick >= start && event.tick < end)
            .map(|event| (event.tick, event.chord))
            .collect();
        chosen.sort_by_key(|(tick, _)| *tick);
        let origin = chosen.first().map_or(0, |(tick, _)| *tick);
        Self {
            notes: Vec::new(),
            chords: chosen
                .into_iter()
                .map(|(tick, chord)| ChordItem {
                    offset: tick - origin,
                    chord,
                })
                .collect(),
        }
    }

    /// Pastes copied notes into a track, returning how many were added.
    ///
    /// # Errors
    ///
    /// Returns a rejection when the edit cannot be applied.
    pub fn paste_notes(
        &self,
        session: &mut EditSession,
        track: TrackId,
        at: Tick,
    ) -> Result<usize, crate::CommandError> {
        if self.notes.is_empty() {
            return Ok(0);
        }
        let Some(entry) = session.song().tracks.iter().find(|entry| entry.id == track) else {
            return Ok(0);
        };
        let mut notes = entry.notes.clone();
        let highest = notes.iter().map(|note| note.id.get()).max().unwrap_or(0);
        let fits = u32::try_from(self.notes.len())
            .ok()
            .and_then(|count| highest.checked_add(count))
            .is_some();
        if !fits {
            return Err(crate::CommandError::Validation(String::from(
                "no note ids left",
            )));
        }
        let mut next = highest + 1;
        for item in &self.notes {
            let Some(id) = project::NoteId::new(next).ok() else {
                break;
            };
            next = next.saturating_add(1);
            let mut note = item.note.clone();
            note.id = id;
            note.start = at.saturating_add(item.offset);
            notes.push(note);
        }
        notes.sort_by_key(|note| note.start);
        let added = self.notes.len();
        session.apply(Command::ReplaceTrackNotes { track, notes })?;
        Ok(added)
    }

    /// Pastes copied chords, returning how many were added.
    ///
    /// # Errors
    ///
    /// Returns a rejection when the edit cannot be applied.
    /// Pastes the chord range `count` times, each `spacing` ticks after the
    /// previous one, as one edit.
    ///
    /// # Errors
    ///
    /// Returns the rejection when the result is invalid, such as a duplicate
    /// chord tick.
    pub fn paste_chords_repeated(
        &self,
        session: &mut EditSession,
        at: Tick,
        count: usize,
        spacing: Tick,
    ) -> Result<usize, crate::CommandError> {
        if self.chords.is_empty() || count == 0 {
            return Ok(0);
        }
        let mut chords = session.song().chords.clone();
        let mut added = 0;
        for copy in 0..count {
            let offset = Tick::try_from(copy)
                .unwrap_or(0)
                .saturating_mul(spacing.max(1));
            for item in &self.chords {
                chords.push(project::song::ChordEvent {
                    tick: at.saturating_add(offset).saturating_add(item.offset),
                    chord: item.chord,
                });
                added += 1;
            }
        }
        chords.sort_by_key(|event| event.tick);
        session.apply(crate::Command::SetChords { chords })?;
        Ok(added)
    }

    pub fn paste_chords(
        &self,
        session: &mut EditSession,
        at: Tick,
    ) -> Result<usize, crate::CommandError> {
        if self.chords.is_empty() {
            return Ok(0);
        }
        let mut chords = session.song().chords.clone();
        for item in &self.chords {
            chords.push(project::song::ChordEvent {
                tick: at.saturating_add(item.offset),
                chord: item.chord,
            });
        }
        chords.sort_by_key(|event| event.tick);
        let added = self.chords.len();
        session.apply(Command::SetChords { chords })?;
        Ok(added)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::NoteId;
    use project::song::{ProjectFile, Track};

    fn session() -> EditSession {
        let mut song = Song::new("Clipboard", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, start) in [(1_u32, 0_i64), (2, 480)] {
            track.notes.push(Note {
                id: NoteId::new(index).expect("valid"),
                start,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn notes_copy_with_offsets_and_paste_at_the_requested_tick() {
        let mut session = session();
        let clipboard =
            Clipboard::from_range(session.song(), TrackId::new(1).expect("valid"), 0, 960);
        assert_eq!(clipboard.notes.len(), 2);
        assert_eq!(clipboard.notes[1].offset, 480);
        let added = clipboard
            .paste_notes(&mut session, TrackId::new(1).expect("valid"), 1920)
            .expect("paste");
        assert_eq!(added, 2);
        let notes = &session.song().tracks[0].notes;
        assert_eq!(notes.len(), 4);
        assert_eq!(notes[2].start, 1920);
        assert_eq!(notes[3].start, 2400);
        let ids: Vec<u32> = notes.iter().map(|note| note.id.get()).collect();
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(ids.len(), unique.len(), "pasted ids are new");
        session.undo().expect("undo");
        assert_eq!(session.song().tracks[0].notes.len(), 2);
    }

    #[test]
    fn chords_copy_and_paste_as_one_step() {
        let mut session = session();
        session
            .apply(Command::SetChords {
                chords: vec![
                    project::song::ChordEvent {
                        tick: 0,
                        chord: Chord::parse("C").expect("valid"),
                    },
                    project::song::ChordEvent {
                        tick: 960,
                        chord: Chord::parse("G").expect("valid"),
                    },
                ],
            })
            .expect("chords");
        let clipboard = Clipboard::from_chords(session.song(), 0, 1920);
        assert_eq!(clipboard.chords.len(), 2);
        let added = clipboard.paste_chords(&mut session, 3840).expect("paste");
        assert_eq!(added, 2);
        let ticks: Vec<Tick> = session
            .song()
            .chords
            .iter()
            .map(|event| event.tick)
            .collect();
        assert!(ticks.contains(&3840) && ticks.contains(&4800));
        session.undo().expect("undo paste");
        assert_eq!(session.song().chords.len(), 2);
    }

    #[test]
    fn chord_ranges_paste_repeatedly() {
        let mut session = session();
        session
            .apply(Command::SetChords {
                chords: vec![
                    project::song::ChordEvent {
                        tick: 0,
                        chord: Chord::parse("C").expect("valid"),
                    },
                    project::song::ChordEvent {
                        tick: 960,
                        chord: Chord::parse("G").expect("valid"),
                    },
                ],
            })
            .expect("chords");
        let clipboard = Clipboard::from_chords(session.song(), 0, 1920);
        let added = clipboard
            .paste_chords_repeated(&mut session, 3840, 3, 1920)
            .expect("repeat");
        assert_eq!(added, 6);
        assert_eq!(session.song().chords.len(), 8);
        assert!(
            clipboard
                .paste_chords_repeated(&mut session, 0, 1, 1920)
                .is_err(),
            "a duplicate chord tick is rejected"
        );
        session.undo().expect("undo repeat");
        assert_eq!(session.song().chords.len(), 2);
    }

    #[test]
    fn an_empty_clipboard_pastes_zero() {
        let mut session = session();
        let clipboard = Clipboard::default();
        assert!(clipboard.is_empty());
        assert_eq!(
            clipboard
                .paste_notes(&mut session, TrackId::new(1).expect("valid"), 0)
                .expect("paste"),
            0
        );
        assert_eq!(clipboard.paste_chords(&mut session, 0).expect("paste"), 0);
    }
}
