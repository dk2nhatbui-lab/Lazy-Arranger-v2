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
        self.apply_progression_in_key(text, start, music_core::PitchClass::new(0))
    }

    /// Replaces the chords with a progression that may use roman numerals.
    ///
    /// # Errors
    ///
    /// Returns an error for an unreadable token or a rejected command.
    pub fn apply_progression_in_key(
        &mut self,
        text: &str,
        start: Tick,
        key: music_core::PitchClass,
    ) -> Result<usize, String> {
        let meter = self.song().time_signature;
        let placed = music_core::progression::parse_in_key(text, start, meter, key)
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

    /// Replaces the chord at `tick` with a reharmonization over one bar.
    ///
    /// # Errors
    ///
    /// Returns a message when the sequence is empty or a command is rejected.
    pub fn reharmonize(&mut self, tick: Tick, chords: &[music_core::Chord]) -> Result<(), String> {
        if chords.is_empty() {
            return Err(String::from("no reharmonization chords"));
        }
        let bar = self.song().time_signature.bar_ticks().max(1);
        if self.song().chords.iter().any(|event| event.tick == tick) {
            self.session
                .apply(Command::RemoveChord { tick })
                .map_err(|error| error.to_string())?;
        }
        let step = bar / chords.len() as Tick;
        for (index, chord) in chords.iter().enumerate() {
            self.session
                .apply(Command::SetChord {
                    tick: tick + (index as Tick) * step,
                    chord: *chord,
                })
                .map_err(|error| error.to_string())?;
        }
        self.dirty = true;
        Ok(())
    }

    /// Finds chords from the notes, one per bar, and writes them.
    ///
    /// Returns how many bars got a chord. Bars whose notes match nothing are
    /// left unchanged.
    ///
    /// # Errors
    ///
    /// Returns a message when a command is rejected.
    pub fn find_chords(&mut self) -> Result<usize, String> {
        let meter = self.song().time_signature;
        let bar = meter.bar_ticks().max(1);
        let end = self.song().end_tick().max(bar);
        let mut found = Vec::new();
        let mut start = 0;
        while start < end {
            let mut pitches = Vec::new();
            let mut lowest = 127_u8;
            for track in &self.song().tracks {
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
            let existing: Vec<Tick> = self
                .song()
                .chords
                .iter()
                .filter(|event| event.tick >= *tick && event.tick < tick + bar)
                .map(|event| event.tick)
                .collect();
            for event_tick in existing {
                self.session
                    .apply(Command::RemoveChord { tick: event_tick })
                    .map_err(|error| error.to_string())?;
            }
            self.session
                .apply(Command::SetChord {
                    tick: *tick,
                    chord: *chord,
                })
                .map_err(|error| error.to_string())?;
        }
        self.dirty |= !found.is_empty();
        Ok(found.len())
    }

    /// The Arrangement Doctor's proposals for the current song.
    #[must_use]
    pub fn proposals(&self) -> Vec<arranging::Proposal> {
        arranging::analyze(self.song())
    }

    /// Fit to My Synths proposals for a synth's limits.
    #[must_use]
    pub fn fit_proposals(&self, spec: arranging::SynthSpec) -> Vec<arranging::Proposal> {
        arranging::fit_to_synth(self.song(), spec)
    }

    /// Extracts a groove from one track over the first bar.
    #[must_use]
    pub fn extract_groove(&self, track: TrackId) -> Option<arranging::groove::Groove> {
        let source = self.song().tracks.iter().find(|entry| entry.id == track)?;
        let bar = self.song().time_signature.bar_ticks().max(1);
        arranging::groove::extract(&source.notes, bar, music_core::PPQ / 4)
    }

    /// Applies a track's groove to every other track.
    ///
    /// # Errors
    ///
    /// Returns a message when a command is rejected.
    pub fn apply_groove(
        &mut self,
        source: TrackId,
        amounts: arranging::groove::Amounts,
    ) -> Result<usize, String> {
        let Some(groove) = self.extract_groove(source) else {
            return Err(String::from("no groove in that track"));
        };
        let others: Vec<TrackId> = self
            .song()
            .tracks
            .iter()
            .filter(|track| track.id != source && !track.notes.is_empty())
            .map(|track| track.id)
            .collect();
        let mut changed = 0;
        for track in others {
            let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
                continue;
            };
            let notes = arranging::groove::apply(&entry.notes, &groove, amounts);
            self.session
                .apply(Command::ReplaceTrackNotes { track, notes })
                .map_err(|error| error.to_string())?;
            changed += 1;
        }
        self.dirty |= changed > 0;
        Ok(changed)
    }

    /// Sections read from the song's markers.
    #[must_use]
    pub fn sections(&self) -> Vec<arranging::forms::Section> {
        arranging::forms::sections(self.song())
    }

    /// Writes a song form to the timeline.
    ///
    /// # Errors
    ///
    /// Returns a message when a command is rejected.
    pub fn apply_form(&mut self, form: &arranging::forms::Form) -> Result<usize, String> {
        let sections = self.sections();
        let copied = arranging::forms::apply(&mut self.session, &sections, form)
            .map_err(|error| error.to_string())?;
        self.dirty |= copied > 0;
        Ok(copied)
    }

    /// Adds a transition ending at `join_tick`.
    ///
    /// # Errors
    ///
    /// Returns a message when a command is rejected.
    pub fn apply_transition(
        &mut self,
        join_tick: Tick,
        kind: arranging::forms::Transition,
    ) -> Result<usize, String> {
        let plan = arranging::forms::transition(self.song(), join_tick, kind);
        let added = arranging::forms::apply_transition(&mut self.session, &plan)
            .map_err(|error| error.to_string())?;
        self.dirty |= added > 0;
        Ok(added)
    }

    /// Applies one Arrangement Doctor proposal.
    ///
    /// # Errors
    ///
    /// Returns a message when a command is rejected.
    pub fn apply_proposal(&mut self, proposal: &arranging::Proposal) -> Result<usize, String> {
        let changed =
            arranging::apply(&mut self.session, proposal).map_err(|error| error.to_string())?;
        self.dirty |= changed > 0;
        Ok(changed)
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
    fn roman_progressions_use_the_chosen_key() {
        let mut editor = editor();
        editor
            .apply_progression_in_key("vi IV I V", 0, music_core::PitchClass::new(7))
            .expect("progression");
        let roots: Vec<u8> = editor
            .song()
            .chords
            .iter()
            .map(|event| event.chord.root.as_u8())
            .collect();
        assert_eq!(roots, vec![4, 0, 7, 2], "Em C G D");
    }

    #[test]
    fn reharmonizing_replaces_one_chord_with_a_sequence() {
        let mut editor = editor();
        let two_five = music_core::reharmonize::apply(
            music_core::Chord::parse("C").expect("valid"),
            music_core::reharmonize::Kind::TwoFive,
        );
        editor.reharmonize(0, &two_five).expect("reharmonize");
        let ticks: Vec<Tick> = editor
            .song()
            .chords
            .iter()
            .map(|event| event.tick)
            .collect();
        assert_eq!(ticks, vec![0, 1920]);
        assert_eq!(
            editor.song().chords[0]
                .chord
                .format(music_core::Spelling::Sharp),
            "Dm7"
        );
        for _ in 0..3 {
            let _ = editor.undo();
        }
        assert_eq!(editor.song().chords.len(), 1);
        assert_eq!(
            editor.song().chords[0]
                .chord
                .format(music_core::Spelling::Sharp),
            "C"
        );
    }

    #[test]
    fn chords_are_found_from_the_notes() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        for (index, name) in [(2_u32, "E4"), (3, "G4")] {
            editor
                .insert(
                    track,
                    PPQ * i64::from(index - 1),
                    Pitch::from_name(name).expect("valid"),
                )
                .expect("insert");
        }
        let found = editor.find_chords().expect("find");
        assert_eq!(found, 1);
        assert_eq!(
            editor.song().chords[0]
                .chord
                .format(music_core::Spelling::Sharp),
            "C"
        );
        assert!(editor.undo());
    }

    #[test]
    fn doctor_proposals_can_be_applied_and_undone() {
        let mut song = Song::new("Doctor", 120.0);
        let mut band = Track::new(TrackId::new(1).expect("valid"), "Band");
        let mut lead = Track::new(TrackId::new(2).expect("valid"), "Lead");
        lead.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C6").expect("valid"),
            velocity: Velocity::new(110).expect("valid"),
            channel: music_core::Channel::first(),
        });
        for id in 1..=14 {
            band.notes.push(Note {
                id: NoteId::new(id).expect("valid"),
                start: 0,
                duration: PPQ,
                pitch: Pitch::from_name("C3").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: music_core::Channel::first(),
            });
        }
        song.tracks.push(band);
        song.tracks.push(lead);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("valid");
        let proposals = editor.proposals();
        assert!(!proposals.is_empty());
        let changed = editor.apply_proposal(&proposals[0]).expect("apply");
        assert!(changed > 0);
        assert!(editor.undo());
        assert!(
            editor.song().tracks[0]
                .notes
                .iter()
                .all(|note| note.velocity.as_u8() == 100)
        );
    }

    #[test]
    fn a_groove_moves_the_other_tracks_and_undoes() {
        let mut song = Song::new("Groove", 120.0);
        let mut swung = Track::new(TrackId::new(1).expect("valid"), "Swung");
        let mut straight = Track::new(TrackId::new(2).expect("valid"), "Straight");
        for index in 0..4 {
            let start = i64::from(index) * PPQ;
            let offset = if index % 2 == 1 { 40 } else { 0 };
            swung.notes.push(Note {
                id: NoteId::new(index + 1).expect("valid"),
                start: start + offset,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: music_core::Channel::first(),
            });
            straight.notes.push(Note {
                id: NoteId::new(index + 1).expect("valid"),
                start,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: music_core::Channel::first(),
            });
        }
        song.tracks.push(swung);
        song.tracks.push(straight);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("valid");
        let mut original = vec![];
        let mut edited = vec![];
        for track in &editor.song().tracks {
            if track.id == TrackId::new(2).expect("valid") {
                original = track.notes.iter().map(|note| note.start).collect();
            }
        }
        editor
            .apply_groove(
                TrackId::new(1).expect("valid"),
                arranging::groove::Amounts::default(),
            )
            .expect("apply");
        for track in &editor.song().tracks {
            if track.id == TrackId::new(2).expect("valid") {
                edited = track.notes.iter().map(|note| note.start).collect();
            }
        }
        assert_ne!(original, edited);
        assert!(editor.undo());
        let mut after = vec![];
        for track in &editor.song().tracks {
            if track.id == TrackId::new(2).expect("valid") {
                after = track.notes.iter().map(|note| note.start).collect();
            }
        }
        assert_eq!(after, original);
    }

    #[test]
    fn a_song_form_is_written_and_undoes() {
        let mut song = Song::new("Forms", 120.0);
        song.markers.push(project::Marker {
            tick: 0,
            name: String::from("Main A"),
        });
        song.markers.push(project::Marker {
            tick: 3840,
            name: String::from("Ending"),
        });
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for index in 0..2_i64 {
            track.notes.push(Note {
                id: NoteId::new(index as u32 + 1).expect("valid"),
                start: index * 3840,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: music_core::Channel::first(),
            });
        }
        song.tracks.push(track);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("valid");
        let form = arranging::forms::Form {
            name: String::from("Twice"),
            parts: vec![arranging::forms::FormPart {
                section: String::from("Main A"),
                repeats: 2,
                no_melody: false,
            }],
        };
        let copied = editor.apply_form(&form).expect("apply");
        assert_eq!(copied, 2);
        assert_eq!(editor.song().markers.len(), 2);
        assert!(editor.dirty());
        while editor.undo() {}
        assert_eq!(editor.song().tracks[0].notes.len(), 2);
    }

    #[test]
    fn a_transition_is_added_and_undone() {
        let mut song = Song::new("Fill", 120.0);
        let mut drums = Track::new(TrackId::new(1).expect("valid"), "Drums");
        drums.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::new(36).expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: music_core::Channel::new(9).expect("valid"),
        });
        song.tracks.push(drums);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("valid");
        let added = editor
            .apply_transition(3840, arranging::forms::Transition::DrumFill)
            .expect("transition");
        assert_eq!(added, 4);
        assert_eq!(editor.song().tracks[0].notes.len(), 5);
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].notes.len(), 1);
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
