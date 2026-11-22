//! Edits from the shell go through `app-commands`, so undo/redo and validation
//! are the same as everywhere else.

use std::path::Path;

use app_commands::{Command, CommandError, EditSession};
use music_core::{PPQ, Pitch, Tick};
use project::song::{Note, ProjectFile, Song};
use project::{NoteId, ProjectError, TrackId};

/// A project plus its undo history and a dirty flag.
#[derive(Debug)]
pub struct Editor {
    session: EditSession,
    dirty: bool,
}

impl Editor {
    /// Opens a project for editing.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the project is invalid.
    pub fn new(project: ProjectFile) -> Result<Self, ProjectError> {
        Ok(Self {
            session: EditSession::new(project)?,
            dirty: false,
        })
    }

    /// The song being edited.
    #[must_use]
    pub fn song(&self) -> &Song {
        self.session.song()
    }

    /// The project, for saving and validation.
    #[must_use]
    pub fn project(&self) -> &ProjectFile {
        self.session.project()
    }

    /// True when there are unsaved edits.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Number of undo steps.
    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.session.undo_depth()
    }

    /// Inserts one note of one beat at a snapped tick.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the track is unknown or the edit is invalid.
    pub fn insert(
        &mut self,
        track: TrackId,
        start: Tick,
        pitch: Pitch,
    ) -> Result<NoteId, CommandError> {
        let id = self.session.next_note_id(track)?;
        self.session.add_note(
            track,
            start,
            PPQ,
            pitch,
            music_core::Velocity::new(100).expect("valid"),
            music_core::Channel::first(),
        )?;
        self.dirty = true;
        Ok(id)
    }

    /// Removes one note.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the note is unknown.
    pub fn delete(&mut self, track: TrackId, note: NoteId) -> Result<(), CommandError> {
        self.session.apply(Command::RemoveNote { track, note })?;
        self.dirty = true;
        Ok(())
    }

    /// Moves one note to a new start and pitch.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the note is unknown or the result is invalid.
    pub fn move_note(
        &mut self,
        track: TrackId,
        note: NoteId,
        start: Tick,
        pitch: Pitch,
    ) -> Result<(), CommandError> {
        let Some(existing) = self
            .song()
            .tracks
            .iter()
            .find(|candidate| candidate.id == track)
            .and_then(|candidate| candidate.notes.iter().find(|entry| entry.id == note))
            .cloned()
        else {
            return Err(CommandError::UnknownNote { track, note });
        };
        let mut notes = self
            .song()
            .tracks
            .iter()
            .find(|candidate| candidate.id == track)
            .map(|candidate| candidate.notes.clone())
            .unwrap_or_default();
        let Some(slot) = notes.iter_mut().find(|entry| entry.id == note) else {
            return Err(CommandError::UnknownNote { track, note });
        };
        *slot = Note {
            start: start.max(0),
            pitch,
            ..existing
        };
        self.session
            .apply(Command::ReplaceTrackNotes { track, notes })?;
        self.dirty = true;
        Ok(())
    }

    /// Adds a clip region.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the track is unknown or the clips overlap.
    pub fn insert_clip(
        &mut self,
        track: TrackId,
        clip: project::song::Clip,
    ) -> Result<(), CommandError> {
        self.session.insert_clip(track, clip)?;
        self.dirty = true;
        Ok(())
    }

    /// Moves a clip region.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the clip is unknown or the move overlaps.
    pub fn move_clip(
        &mut self,
        track: TrackId,
        clip: project::ClipId,
        start: Tick,
    ) -> Result<(), CommandError> {
        self.session.move_clip(track, clip, start)?;
        self.dirty = true;
        Ok(())
    }

    /// Removes a clip region; its notes stay in the track.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the clip is unknown.
    pub fn remove_clip(
        &mut self,
        track: TrackId,
        clip: project::ClipId,
    ) -> Result<(), CommandError> {
        self.session.remove_clip(track, clip)?;
        self.dirty = true;
        Ok(())
    }

    /// Resizes a clip region.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the clip is unknown or the length is not positive.
    pub fn resize_clip(
        &mut self,
        track: TrackId,
        clip: project::ClipId,
        length: Tick,
    ) -> Result<(), CommandError> {
        self.session.resize_clip(track, clip, length)?;
        self.dirty = true;
        Ok(())
    }

    /// Joins two clip regions.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when either clip is unknown.
    pub fn join_clips(
        &mut self,
        track: TrackId,
        first: project::ClipId,
        second: project::ClipId,
    ) -> Result<(), CommandError> {
        self.session.join_clips(track, first, second)?;
        self.dirty = true;
        Ok(())
    }

    /// Replaces the chords from `start` with a typed progression.
    ///
    /// Returns the number of chords placed.
    ///
    /// # Errors
    ///
    /// Returns an error for an unreadable chord symbol or a rejected command.
    pub fn apply_progression(&mut self, text: &str, start: Tick) -> Result<usize, String> {
        let meter = self.song().time_signature;
        let placed = music_core::progression::parse(text, start, meter)
            .map_err(|error| error.to_string())?;
        if placed.is_empty() {
            return Err(String::from("no chords in that text"));
        }
        let end = placed.last().map(|entry| entry.tick).unwrap_or(start) + meter.bar_ticks().max(1);
        let existing: Vec<Tick> = self
            .song()
            .chords
            .iter()
            .filter(|event| event.tick >= start && event.tick < end)
            .map(|event| event.tick)
            .collect();
        for tick in existing {
            self.session
                .apply(Command::RemoveChord { tick })
                .map_err(|error| error.to_string())?;
        }
        for entry in &placed {
            self.session
                .apply(Command::SetChord {
                    tick: entry.tick,
                    chord: entry.chord,
                })
                .map_err(|error| error.to_string())?;
        }
        self.dirty = true;
        Ok(placed.len())
    }

    /// Undoes one step; true when something was undone.
    pub fn undo(&mut self) -> bool {
        if self.session.undo().is_ok() {
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Redoes one step; true when something was redone.
    pub fn redo(&mut self) -> bool {
        if self.session.redo().is_ok() {
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Saves the project as JSON.
    ///
    /// # Errors
    ///
    /// Returns a string describing the failure.
    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        let json = self
            .session
            .project()
            .to_json_pretty()
            .map_err(|error| error.to_string())?;
        std::fs::write(path, json).map_err(|error| format!("{}: {error}", path.display()))?;
        self.dirty = false;
        Ok(())
    }
}

/// Snaps a tick down to the nearest grid line.
#[must_use]
pub fn snap(tick: Tick, grid: Tick) -> Tick {
    if grid <= 0 {
        return tick.max(0);
    }
    tick.div_euclid(grid) * grid
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Chord, Velocity};
    use project::song::Track;

    fn editor() -> Editor {
        let mut song = Song::new("Edit", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: music_core::Channel::first(),
        });
        song.tracks.push(track);
        song.chords.push(project::ChordEvent {
            tick: 0,
            chord: Chord::parse("C").expect("valid"),
        });
        Editor::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn insert_and_undo_round_trip() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let id = editor
            .insert(track, 1920, Pitch::from_name("E4").expect("valid"))
            .expect("insert");
        assert_eq!(editor.song().tracks[0].notes.len(), 2);
        assert!(editor.dirty());
        assert_eq!(editor.song().tracks[0].notes[1].id, id);
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].notes.len(), 1);
        assert!(editor.redo());
        assert_eq!(editor.song().tracks[0].notes.len(), 2);
    }

    #[test]
    fn delete_and_undo_round_trip() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let note = NoteId::new(1).expect("valid");
        editor.delete(track, note).expect("delete");
        assert!(editor.song().tracks[0].notes.is_empty());
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].notes.len(), 1);
    }

    #[test]
    fn moving_a_note_changes_start_and_pitch_and_undoes() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let note = NoteId::new(1).expect("valid");
        editor
            .move_note(track, note, 960, Pitch::from_name("G4").expect("valid"))
            .expect("move");
        let moved = &editor.song().tracks[0].notes[0];
        assert_eq!(moved.start, 960);
        assert_eq!(moved.pitch.as_u8(), 67);
        assert!(editor.undo());
        let back = &editor.song().tracks[0].notes[0];
        assert_eq!((back.start, back.pitch.as_u8()), (0, 60));
    }

    #[test]
    fn edits_keep_the_project_valid() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        for tick in [0, 960, 1920, 2880] {
            editor
                .insert(track, tick, Pitch::from_name("C5").expect("valid"))
                .expect("insert");
        }
        assert!(editor.project().check().is_ok());
    }

    #[test]
    fn snapping_uses_floor_division() {
        assert_eq!(snap(0, PPQ), 0);
        assert_eq!(snap(959, PPQ), 0);
        assert_eq!(snap(960, PPQ), 960);
        assert_eq!(snap(-5, PPQ), -960, "negative ticks floor like the oracle");
        assert_eq!(snap(1234, 0), 1234);
    }

    #[test]
    fn clip_edits_are_undoable_and_keep_notes() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let clip = project::song::Clip {
            id: project::ClipId::new(1).expect("valid"),
            name: String::from("Clip 1"),
            start: 0,
            length: 1920,
        };
        editor.insert_clip(track, clip).expect("insert");
        assert_eq!(editor.song().tracks[0].clips.len(), 1);
        editor
            .move_clip(track, project::ClipId::new(1).expect("valid"), 960)
            .expect("move");
        assert_eq!(editor.song().tracks[0].clips[0].start, 960);
        editor
            .remove_clip(track, project::ClipId::new(1).expect("valid"))
            .expect("remove");
        assert!(editor.song().tracks[0].clips.is_empty());
        assert_eq!(editor.song().tracks[0].notes.len(), 1, "notes stay");
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].clips.len(), 1);
        assert!(editor.dirty());
    }

    #[test]
    fn a_typed_progression_replaces_chords_and_undoes() {
        let mut editor = editor();
        let placed = editor
            .apply_progression("Am F | C G", 0)
            .expect("progression");
        assert_eq!(placed, 4);
        let ticks: Vec<Tick> = editor
            .song()
            .chords
            .iter()
            .map(|event| event.tick)
            .collect();
        assert_eq!(ticks, vec![0, 1920, 3840, 5760]);
        assert!(editor.dirty());
        for _ in 0..5 {
            let _ = editor.undo();
        }
        assert_eq!(
            editor.song().chords.len(),
            1,
            "the original C chord returns"
        );
        assert_eq!(editor.song().chords[0].tick, 0);
    }

    #[test]
    fn saving_clears_the_dirty_flag() {
        let mut editor = editor();
        let path = std::env::temp_dir().join("lazy-arranger-editor-test.json");
        editor
            .insert(
                TrackId::new(1).expect("valid"),
                1920,
                Pitch::from_name("D4").expect("valid"),
            )
            .expect("insert");
        assert!(editor.dirty());
        editor.save(&path).expect("save");
        assert!(!editor.dirty());
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(ProjectFile::from_json(&text).is_ok());
        let _ = std::fs::remove_file(path);
    }
}
