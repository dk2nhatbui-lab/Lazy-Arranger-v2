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
    clipboard: app_commands::clipboard::Clipboard,
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
            clipboard: app_commands::clipboard::Clipboard::default(),
            dirty: false,
        })
    }

    /// The song being edited.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn session_for_test(&mut self) -> &mut app_commands::EditSession {
        &mut self.session
    }

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
        self.insert_length(track, start, pitch, PPQ)
    }

    /// Inserts one note of the given length.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the track is unknown or the edit is invalid.
    pub fn insert_length(
        &mut self,
        track: TrackId,
        start: Tick,
        pitch: Pitch,
        length: Tick,
    ) -> Result<NoteId, CommandError> {
        let id = self.session.next_note_id(track)?;
        self.session.add_note(
            track,
            start,
            length.max(1),
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

    /// Deletes several notes of one track as one edit.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn delete_notes(
        &mut self,
        track: TrackId,
        notes: &[project::NoteId],
    ) -> Result<usize, String> {
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return Ok(0);
        };
        let before = entry.notes.len();
        let kept: Vec<project::song::Note> = entry
            .notes
            .iter()
            .filter(|note| !notes.contains(&note.id))
            .cloned()
            .collect();
        let removed = before - kept.len();
        if removed > 0 {
            self.session
                .apply(app_commands::Command::ReplaceTrackNotes { track, notes: kept })
                .map_err(|error| error.to_string())?;
            self.dirty = true;
        }
        Ok(removed)
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

    /// Adds a clip called `name` at `start` with `length`, choosing the next id.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the clips overlap.
    pub fn add_named_clip(
        &mut self,
        track: TrackId,
        start: Tick,
        length: Tick,
        name: &str,
    ) -> Result<(), String> {
        let entry = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .ok_or_else(|| String::from("unknown track"))?;
        let id = entry
            .clips
            .iter()
            .map(|clip| clip.id.get())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.insert_clip(
            track,
            project::song::Clip {
                id: project::ClipId::new(id).map_err(|error| error.to_string())?,
                name: name.to_string(),
                start,
                length,
            },
        )
        .map_err(|error| error.to_string())
    }

    /// Adds a named clip at `start` with `length`, choosing the next id.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the clips overlap.
    pub fn add_clip(&mut self, track: TrackId, start: Tick, length: Tick) -> Result<(), String> {
        let entry = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .ok_or_else(|| String::from("unknown track"))?;
        let id = entry
            .clips
            .iter()
            .map(|clip| clip.id.get())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.insert_clip(
            track,
            project::song::Clip {
                id: project::ClipId::new(id).map_err(|error| error.to_string())?,
                name: String::from("Clip"),
                start,
                length,
            },
        )
        .map_err(|error| error.to_string())
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

    /// Runs a rule-based generator on one track through the generate tool.
    ///
    /// # Errors
    ///
    /// Returns a message when the generator or edit is rejected.
    pub fn generate_part(
        &mut self,
        track: project::TrackId,
        op: &str,
        seed: u64,
    ) -> Result<String, String> {
        let result = mcp::call(
            &mut self.session,
            "generate",
            &serde_json::json!({"track": track.get(), "op": op, "bars": 4, "seed": seed}),
        )
        .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(result.to_string())
    }

    /// Replaces the song chords with the given events.
    ///
    /// # Errors
    ///
    /// Returns a message when the command is rejected.
    pub fn set_chords(&mut self, mut chords: Vec<project::song::ChordEvent>) -> Result<(), String> {
        chords.sort_by_key(|event| event.tick);
        self.session
            .apply(app_commands::Command::SetChords { chords })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Writes one parsed chord at a tick.
    ///
    /// # Errors
    ///
    /// Returns a message for an unreadable chord or a rejected command.
    pub fn set_chord(&mut self, tick: Tick, chord: &str) -> Result<(), String> {
        let chord = music_core::Chord::parse(chord).map_err(|error| error.to_string())?;
        self.session
            .apply(app_commands::Command::SetChord { tick, chord })
            .map_err(|error| error.to_string())?;
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
        let found =
            arranging::chords::find_chords(&mut self.session).map_err(|error| error.to_string())?;
        self.dirty |= found > 0;
        Ok(found)
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

    /// Applies a stored groove to one track.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is missing or the command is rejected.
    pub fn apply_stored_groove(
        &mut self,
        track: TrackId,
        groove: &arranging::groove::Groove,
        amounts: arranging::groove::Amounts,
    ) -> Result<usize, String> {
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return Err(String::from("track not found"));
        };
        let notes = arranging::groove::apply(&entry.notes, groove, amounts);
        let changed = notes.len();
        self.session
            .apply(Command::ReplaceTrackNotes { track, notes })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(changed)
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

    /// Sections read from the song's markers.
    #[must_use]
    pub fn sections(&self) -> Vec<arranging::forms::Section> {
        arranging::forms::sections(self.song())
    }

    /// Joins a marker section to itself, moving later material.
    ///
    /// # Errors
    ///
    /// Returns a message when the section is missing or the edit is rejected.
    pub fn join_loop(
        &mut self,
        section: &str,
        total: usize,
    ) -> Result<arranging::forms::LoopOutcome, String> {
        let outcome = arranging::forms::join_loop(&mut self.session, section, total)
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(outcome)
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

    /// Sets a track's mute, solo and arm flags.
    ///
    /// # Errors
    ///
    /// Returns a message when the command is rejected.
    pub fn set_track_flags(
        &mut self,
        track: TrackId,
        muted: bool,
        soloed: bool,
        armed: bool,
    ) -> Result<(), String> {
        self.session
            .apply(Command::SetTrackFlags {
                track,
                muted,
                soloed,
                armed,
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Reads the drum pattern of one bar from a track.
    #[must_use]
    pub fn beat_pattern(
        &self,
        track: TrackId,
        start: Tick,
    ) -> Option<(Tick, arranging::beat::BeatPattern)> {
        self.beat_pattern_bars(track, start, 1)
    }

    /// Reads a beat pattern covering `bars` bars.
    #[must_use]
    pub fn beat_pattern_bars(
        &self,
        track: TrackId,
        start: Tick,
        bars: usize,
    ) -> Option<(Tick, arranging::beat::BeatPattern)> {
        let found = self.song().tracks.iter().find(|entry| entry.id == track)?;
        let bar = self.song().time_signature.bar_ticks().max(1);
        Some((
            bar,
            arranging::beat::BeatPattern::from_track_bars(found, start, bar, bars),
        ))
    }

    /// Writes a beat pattern over one bar.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn apply_beat(
        &mut self,
        track: TrackId,
        start: Tick,
        bar: Tick,
        pattern: &arranging::beat::BeatPattern,
    ) -> Result<usize, String> {
        let added = arranging::beat::apply(&mut self.session, track, start, bar, pattern)
            .map_err(|error| error.to_string())?;
        self.dirty |= added > 0;
        Ok(added)
    }

    /// Writes the pattern into `total_bars` bars as one edit.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn put_in_bars(
        &mut self,
        track: TrackId,
        start: Tick,
        bar: Tick,
        pattern: &arranging::beat::BeatPattern,
        total_bars: usize,
    ) -> Result<usize, String> {
        let added =
            arranging::beat::put_in_bars(&mut self.session, track, start, bar, pattern, total_bars)
                .map_err(|error| error.to_string())?;
        self.dirty |= added > 0;
        Ok(added)
    }

    /// Fills the Loops slots from the timeline in scenes of `bars` bars.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn loops_from_timeline(&mut self, bars: u32) -> Result<usize, String> {
        let length = arranging::loops::bars_to_ticks(self.song(), bars);
        let scenes = arranging::loops::fill_from_timeline(&mut self.session, length)
            .map_err(|error| error.to_string())?;
        self.dirty |= scenes > 0;
        Ok(scenes)
    }

    /// Writes a Loops jam to the timeline.
    ///
    /// # Errors
    ///
    /// Returns a message when an edit is rejected.
    pub fn record_loops(
        &mut self,
        changes: &[arranging::loops::Change],
        end: Tick,
    ) -> Result<usize, String> {
        let written = arranging::loops::record_to_timeline(&mut self.session, changes, end)
            .map_err(|error| error.to_string())?;
        self.dirty |= written > 0;
        Ok(written)
    }

    /// Renames one Loops scene.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn rename_scene(&mut self, index: usize, name: &str) -> Result<(), String> {
        arranging::loops::rename_scene(&mut self.session, index, name)
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Puts a clip in one Loops slot, or empties it.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_slot(
        &mut self,
        scene: usize,
        track: TrackId,
        clip: Option<project::song::SlotClip>,
    ) -> Result<(), String> {
        arranging::loops::set_slot(&mut self.session, track, scene, clip)
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Makes an empty clip in one Loops slot.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn new_slot(&mut self, scene: usize, track: TrackId, length: Tick) -> Result<(), String> {
        arranging::loops::new_slot(&mut self.session, track, scene, length)
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Replaces every note of a track as one edit.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn replace_track_notes(
        &mut self,
        track: TrackId,
        notes: Vec<project::song::Note>,
    ) -> Result<(), String> {
        self.session
            .apply(Command::ReplaceTrackNotes { track, notes })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Replaces the notes of one Loops slot clip.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_slot_notes(
        &mut self,
        scene: usize,
        track: TrackId,
        notes: Vec<project::song::Note>,
    ) -> Result<(), String> {
        arranging::loops::set_slot_notes(&mut self.session, track, scene, notes)
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Renames one Loops slot clip.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn rename_slot(&mut self, scene: usize, track: TrackId, name: &str) -> Result<(), String> {
        arranging::loops::rename_slot(&mut self.session, track, scene, name)
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets a track's fader and pan.
    ///
    /// # Errors
    ///
    /// Returns a message when the command is rejected.
    pub fn set_track_gain(&mut self, track: TrackId, volume: u8, pan: u8) -> Result<(), String> {
        self.session
            .apply(Command::SetTrackGain { track, volume, pan })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Replaces a track's automation lanes.
    ///
    /// # Errors
    ///
    /// Returns a message when the command is rejected.
    pub fn set_automation(
        &mut self,
        track: TrackId,
        lanes: Vec<project::song::Automation>,
    ) -> Result<(), String> {
        self.session
            .apply(Command::SetAutomation { track, lanes })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Stores opaque state on one plug-in slot.
    ///
    /// # Errors
    ///
    /// Returns a message when the track or slot is unknown.
    pub fn set_plugin_state(
        &mut self,
        track: TrackId,
        slot: u32,
        state: &str,
    ) -> Result<(), String> {
        let plugins = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| entry.plugins.clone())
            .ok_or_else(|| String::from("unknown track"))?;
        let mut plugins = plugins;
        let entry = plugins
            .iter_mut()
            .find(|entry| entry.id == slot)
            .ok_or_else(|| String::from("unknown plug-in slot"))?;
        entry.state = state.to_string();
        self.session
            .apply(app_commands::Command::SetPlugins { track, plugins })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Replaces the master-bus plug-in chain.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_master_plugins(
        &mut self,
        plugins: Vec<project::song::PluginSlot>,
    ) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetMasterPlugins { plugins })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Moves a plug-in slot earlier or later on its track.
    ///
    /// # Errors
    ///
    /// Returns a message when the track or slot is unknown.
    pub fn move_plugin(&mut self, track: TrackId, id: u32, earlier: bool) -> Result<(), String> {
        let mut plugins = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| entry.plugins.clone())
            .ok_or_else(|| String::from("unknown track"))?;
        let index = plugins
            .iter()
            .position(|entry| entry.id == id)
            .ok_or_else(|| String::from("unknown plug-in slot"))?;
        let target = if earlier {
            index.checked_sub(1)
        } else {
            (index + 1 < plugins.len()).then_some(index + 1)
        };
        let Some(target) = target else {
            return Ok(());
        };
        plugins.swap(index, target);
        self.session
            .apply(app_commands::Command::SetPlugins { track, plugins })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets or clears a track's synth patch.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the edit is rejected.
    pub fn set_track_patch(
        &mut self,
        track: TrackId,
        patch: Option<project::song::Patch>,
    ) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetTrackPatch {
                track,
                patch: patch.map(Box::new),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Adds a plug-in slot to a track and returns its id.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the edit is rejected.
    pub fn add_plugin(
        &mut self,
        track: TrackId,
        name: &str,
        format: project::song::PluginFormat,
        path: &str,
        identifier: &str,
    ) -> Result<u32, String> {
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return Err(String::from("unknown track"));
        };
        let mut plugins = entry.plugins.clone();
        let id = plugins.iter().map(|plugin| plugin.id).max().unwrap_or(0) + 1;
        plugins.push(project::song::PluginSlot {
            id,
            name: name.to_string(),
            format,
            path: path.to_string(),
            identifier: identifier.to_string(),
            bypassed: false,
            state: String::new(),
        });
        self.session
            .apply(app_commands::Command::SetPlugins { track, plugins })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(id)
    }

    /// Removes a plug-in slot.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn remove_plugin(&mut self, track: TrackId, id: u32) -> Result<(), String> {
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return Err(String::from("unknown track"));
        };
        let plugins: Vec<project::song::PluginSlot> = entry
            .plugins
            .iter()
            .filter(|plugin| plugin.id != id)
            .cloned()
            .collect();
        self.session
            .apply(app_commands::Command::SetPlugins { track, plugins })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Toggles one slot's bypass flag.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn toggle_plugin(&mut self, track: TrackId, id: u32) -> Result<(), String> {
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return Err(String::from("unknown track"));
        };
        let mut plugins = entry.plugins.clone();
        let Some(slot) = plugins.iter_mut().find(|plugin| plugin.id == id) else {
            return Err(String::from("unknown plug-in"));
        };
        slot.bypassed = !slot.bypassed;
        self.session
            .apply(app_commands::Command::SetPlugins { track, plugins })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Replaces the score marks.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_marks(&mut self, mut marks: Vec<project::song::Mark>) -> Result<(), String> {
        marks.sort_by_key(|mark| mark.tick);
        self.session
            .apply(app_commands::Command::SetMarks { marks })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets or clears the song key signature.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_song_key(&mut self, key: Option<project::song::KeySignature>) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetSongKey { key })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Adds one dynamic or articulation at a tick.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn add_mark(&mut self, tick: Tick, kind: project::song::MarkKind) -> Result<(), String> {
        let mut marks = self.song().marks.clone();
        marks.push(project::song::Mark {
            tick: tick.max(0),
            kind,
        });
        marks.sort_by_key(|mark| mark.tick);
        self.set_marks(marks)
    }

    /// Removes one mark by its index in tick order.
    ///
    /// # Errors
    ///
    /// Returns a message when the index or edit is invalid.
    pub fn remove_mark(&mut self, index: usize) -> Result<(), String> {
        let mut marks = self.song().marks.clone();
        if index >= marks.len() {
            return Err(String::from("unknown score mark"));
        }
        marks.remove(index);
        self.set_marks(marks)
    }

    /// Replaces every marker in one edit.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_markers(&mut self, mut markers: Vec<project::Marker>) -> Result<(), String> {
        markers.sort_by_key(|marker| marker.tick);
        self.session
            .apply(app_commands::Command::SetMarkers { markers })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Adds or replaces the marker at a tick.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn add_marker(&mut self, tick: Tick, name: &str) -> Result<(), String> {
        let mut markers: Vec<project::Marker> = self
            .song()
            .markers
            .iter()
            .filter(|marker| marker.tick != tick)
            .cloned()
            .collect();
        markers.push(project::Marker {
            tick,
            name: name.to_string(),
        });
        self.set_markers(markers)
    }

    /// Removes the marker at a tick.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn remove_marker(&mut self, tick: Tick) -> Result<(), String> {
        let markers: Vec<project::Marker> = self
            .song()
            .markers
            .iter()
            .filter(|marker| marker.tick != tick)
            .cloned()
            .collect();
        self.set_markers(markers)
    }

    /// Replaces all lyric syllables in one edit.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_lyrics(&mut self, lyrics: Vec<project::song::LyricEvent>) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetLyrics { lyrics })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Adds or replaces the syllable at a tick.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn add_lyric(&mut self, tick: Tick, text: &str) -> Result<(), String> {
        let mut lyrics: Vec<project::song::LyricEvent> = self
            .song()
            .lyrics
            .iter()
            .filter(|event| event.tick != tick)
            .cloned()
            .collect();
        lyrics.push(project::song::LyricEvent {
            tick,
            text: text.to_string(),
        });
        lyrics.sort_by_key(|event| event.tick);
        self.set_lyrics(lyrics)
    }

    /// Removes the syllable at a tick.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn remove_lyric(&mut self, tick: Tick) -> Result<(), String> {
        let lyrics: Vec<project::song::LyricEvent> = self
            .song()
            .lyrics
            .iter()
            .filter(|event| event.tick != tick)
            .cloned()
            .collect();
        self.set_lyrics(lyrics)
    }

    /// Inserts recorded notes as a new track with fresh note ids.
    ///
    /// # Errors
    ///
    /// Returns a message when a track edit is rejected.
    pub fn insert_recorded(
        &mut self,
        name: &str,
        notes: &[project::song::Note],
    ) -> Result<TrackId, String> {
        let track_id = self
            .session
            .next_track_id()
            .map_err(|error| error.to_string())?;
        let mut track = project::song::Track::new(track_id, name);
        track.notes = notes.to_vec();
        for (index, note) in track.notes.iter_mut().enumerate() {
            note.id = project::NoteId::new(index as u32 + 1).map_err(|error| error.to_string())?;
        }
        if let Some(start) = track.notes.iter().map(|note| note.start).min() {
            let bar = self.song().time_signature.bar_ticks().max(1);
            let end = track
                .notes
                .iter()
                .map(project::song::Note::end)
                .max()
                .unwrap_or(start);
            let start = start.div_euclid(bar) * bar;
            let length = ((end.saturating_sub(start).max(1) + bar - 1) / bar) * bar;
            track.clips.push(project::song::Clip {
                id: project::ClipId::new(1).expect("valid"),
                name: String::from("Recorded"),
                start,
                length,
            });
        }
        self.session
            .apply(app_commands::Command::InsertTrack { track })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(track_id)
    }

    /// Appends recorded notes to an existing track with fresh ids.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the edit is rejected.
    pub fn append_recorded(
        &mut self,
        track: TrackId,
        notes: &[project::song::Note],
    ) -> Result<(), String> {
        let entry = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .ok_or_else(|| String::from("unknown track"))?;
        let mut merged = entry.notes.clone();
        let existing_clips = entry.clips.clone();
        let start = merged.iter().map(|note| note.id.get()).max().unwrap_or(0) + 1;
        for (offset, note) in notes.iter().enumerate() {
            let mut note = note.clone();
            note.id =
                project::NoteId::new(start + offset as u32).map_err(|error| error.to_string())?;
            merged.push(note);
        }
        let mut clips = existing_clips;
        if let Some(first) = notes.iter().map(|note| note.start).min() {
            let bar = self.song().time_signature.bar_ticks().max(1);
            let end = notes
                .iter()
                .map(project::song::Note::end)
                .max()
                .unwrap_or(first);
            let clip_start = first.div_euclid(bar) * bar;
            let overlaps = clips.iter().any(|clip| {
                clip.start < end && clip_start < clip.start.saturating_add(clip.length)
            });
            if !overlaps {
                let id = clips
                    .iter()
                    .map(|clip| clip.id.get())
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1);
                let length = ((end.saturating_sub(clip_start).max(1) + bar - 1) / bar) * bar;
                clips.push(project::song::Clip {
                    id: project::ClipId::new(id).map_err(|error| error.to_string())?,
                    name: String::from("Recorded"),
                    start: clip_start,
                    length,
                });
            }
        }
        self.session
            .apply(app_commands::Command::ReplaceTrackNotesAndClips {
                track,
                notes: merged,
                clips,
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Moves a track to a new zero-based position.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the edit is rejected.
    pub fn move_track(&mut self, track: TrackId, index: usize) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::MoveTrack { track, index })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Removes a track and every note on it.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the edit is rejected.
    pub fn remove_track(&mut self, track: TrackId) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::RemoveTrack { track })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Duplicates a track, with fresh note and clip identifiers.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the insert is rejected.
    pub fn duplicate_track(&mut self, track: TrackId) -> Result<TrackId, String> {
        let source = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .cloned()
            .ok_or_else(|| String::from("unknown track"))?;
        let id = self
            .session
            .next_track_id()
            .map_err(|error| error.to_string())?;
        let mut copy = source;
        copy.id = id;
        copy.name = format!("{} copy", copy.name);
        for (index, note) in copy.notes.iter_mut().enumerate() {
            note.id = project::NoteId::new(index as u32 + 1).map_err(|error| error.to_string())?;
        }
        for (index, clip) in copy.clips.iter_mut().enumerate() {
            clip.id = project::ClipId::new(index as u32 + 1).map_err(|error| error.to_string())?;
        }
        self.session
            .apply(app_commands::Command::InsertTrack { track: copy })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(id)
    }

    /// Duplicates an audio track, including takes and comps, as a new track.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the insert is rejected.
    pub fn duplicate_audio_track(&mut self, track: TrackId) -> Result<TrackId, String> {
        self.duplicate_track(track)
    }

    /// Sets the song meter.
    ///
    /// # Errors
    ///
    /// Returns a message when the signature or edit is rejected.
    pub fn set_time_signature(&mut self, numerator: u8, denominator: u8) -> Result<(), String> {
        let time_signature = music_core::TimeSignature::new(numerator, denominator)
            .map_err(|error| error.to_string())?;
        self.session
            .apply(app_commands::Command::SetTimeSignature { time_signature })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets one note's duration.
    ///
    /// # Errors
    ///
    /// Returns a message when the note or edit is rejected.
    pub fn set_note_duration(
        &mut self,
        track: TrackId,
        note: project::NoteId,
        duration: Tick,
    ) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetNoteDuration {
                track,
                note,
                duration,
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Renames the song.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn rename_song(&mut self, name: &str) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetSongName {
                name: name.to_string(),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Renames a track.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn rename_track(&mut self, track: TrackId, name: &str) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::RenameTrack {
                track,
                name: name.to_string(),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets or clears a track's bank select.
    ///
    /// # Errors
    ///
    /// Returns a message when the command is rejected.
    pub fn set_track_bank(
        &mut self,
        track: TrackId,
        bank: Option<project::song::BankSelect>,
    ) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetTrackBank { track, bank })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets a track's program and bank together in one undo step.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_track_voice(
        &mut self,
        track: TrackId,
        program: Option<u8>,
        bank: Option<project::song::BankSelect>,
    ) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetTrackVoice {
                track,
                program,
                bank,
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets or clears a track's keyboard program.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_track_program(&mut self, track: TrackId, program: Option<u8>) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetTrackProgram { track, program })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets or clears a track's SoundFont preset.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_soundfont(
        &mut self,
        track: TrackId,
        soundfont: Option<project::song::SoundFontRef>,
    ) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetSoundFont { track, soundfont })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Replaces one automation lane, leaving the track's other lanes alone.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn merge_automation(
        &mut self,
        track: TrackId,
        lane: project::song::Automation,
    ) -> Result<(), String> {
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return Err(String::from("unknown track"));
        };
        let mut lanes: Vec<project::song::Automation> = entry
            .automation
            .iter()
            .filter(|existing| existing.target != lane.target)
            .cloned()
            .collect();
        if !lane.points.is_empty() {
            lanes.push(lane);
        }
        self.set_automation(track, lanes)
    }

    /// Copies several notes of one track into the clipboard.
    pub fn copy_notes(&mut self, track: TrackId, notes: &[NoteId]) {
        self.clipboard = app_commands::clipboard::Clipboard::from_notes(self.song(), track, notes);
    }

    /// Copies then deletes several notes; returns how many were removed.
    ///
    /// # Errors
    ///
    /// Returns a message when the delete is rejected.
    pub fn cut_notes(&mut self, track: TrackId, notes: &[NoteId]) -> Result<usize, String> {
        self.copy_notes(track, notes);
        self.delete_notes(track, notes)
    }

    /// Copies one note into the clipboard.
    pub fn copy_note(&mut self, track: TrackId, note: NoteId) {
        self.clipboard =
            app_commands::clipboard::Clipboard::from_notes(self.song(), track, &[note]);
    }

    /// Copies then deletes one note; true when it was copied.
    pub fn cut_note(&mut self, track: TrackId, note: NoteId) -> bool {
        self.copy_note(track, note);
        let copied = !self.clipboard.is_empty();
        if copied {
            let _ = self.delete(track, note);
        }
        copied
    }

    /// Pastes copied notes at a tick.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn paste_notes(&mut self, track: TrackId, at: Tick) -> Result<usize, String> {
        let copied = self
            .clipboard
            .paste_notes(&mut self.session, track, at)
            .map_err(|error| error.to_string())?;
        self.dirty |= copied > 0;
        Ok(copied)
    }

    /// Replaces the project with the Kids Mode demo.
    ///
    /// # Errors
    ///
    /// Returns a message when the demo project is invalid.
    pub fn load_kids_demo(&mut self) -> Result<(), String> {
        self.session = app_commands::EditSession::new(project::song::ProjectFile::new(
            arranging::demo::kids_song(),
        ))
        .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Imports a style file's tracks with a name prefix.
    ///
    /// # Errors
    ///
    /// Returns a message when the style cannot be imported.
    pub fn import_style_tracks(
        &mut self,
        path: &std::path::Path,
        prefix: &str,
    ) -> Result<app_commands::style_import::Imported, String> {
        let imported = app_commands::style_import::import_file(&mut self.session, path, prefix)?;
        self.dirty |= imported.tracks > 0;
        Ok(imported)
    }

    /// Sets the tempo at tick zero.
    ///
    /// # Errors
    ///
    /// Returns a message when the command is rejected.
    /// Moves every note on a track to one MIDI channel.
    ///
    /// # Errors
    ///
    /// Returns a message when the track edit is rejected.
    pub fn set_track_channel(
        &mut self,
        track: TrackId,
        channel: music_core::Channel,
    ) -> Result<(), String> {
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return Err(String::from("track not found"));
        };
        let notes: Vec<project::song::Note> = entry
            .notes
            .iter()
            .cloned()
            .map(|mut note| {
                note.channel = channel;
                note
            })
            .collect();
        self.session
            .apply(app_commands::Command::ReplaceTrackNotes { track, notes })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Adds an empty MIDI track.
    ///
    /// # Errors
    ///
    /// Returns a message when the track id or command is rejected.
    pub fn add_empty_track(&mut self) -> Result<TrackId, String> {
        let track_id = self
            .session
            .next_track_id()
            .map_err(|error| error.to_string())?;
        let track = project::song::Track::new(track_id, format!("Track {track_id}"));
        self.session
            .apply(app_commands::Command::InsertTrack { track })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(track_id)
    }

    /// Imports every L6max take as an audio track at `start`.
    ///
    /// # Errors
    ///
    /// Returns a message when a track edit is rejected.
    pub fn import_l6max(
        &mut self,
        plan: &audio::l6max::Plan,
        start: Tick,
    ) -> Result<usize, String> {
        let bpm = self.song().tempo_map.bpm_at(start);
        let mut imported = 0;
        for take in &plan.takes {
            let track_id = self
                .session
                .next_track_id()
                .map_err(|error| error.to_string())?;
            let mut track = project::song::Track::new(track_id, take.name.clone());
            let lower = take.name.to_ascii_lowercase();
            track.muted = lower.contains("master") || lower.contains("mix");
            track.audio = Some(project::song::AudioRef {
                path: take.path.display().to_string(),
                start,
                length: audio::chords::to_ticks(take.seconds, bpm).max(1),
                offset: 0,

                fade_in: 0,
                fade_out: 0,

                stretch: false,
                gain_mb: 0,
            });
            self.session
                .apply(app_commands::Command::InsertTrack { track })
                .map_err(|error| error.to_string())?;
            imported += 1;
        }
        self.dirty |= imported > 0;
        Ok(imported)
    }

    /// Imports a WAV as a new audio track starting at a tick.
    ///
    /// # Errors
    ///
    /// Returns a message when the file is unreadable, not a WAV, or the track
    /// cannot be inserted.
    pub fn import_audio(&mut self, wav: &std::path::Path, start: Tick) -> Result<TrackId, String> {
        let bytes = std::fs::read(wav)
            .map_err(|error| format!("could not read {}: {error}", wav.display()))?;
        let seconds = if bytes.starts_with(b"RIFF") {
            audio::wav::info(&bytes)?.seconds
        } else {
            let decoded = audio::decode::decode_mono(wav)?;
            decoded.samples.len() as f64 / f64::from(decoded.sample_rate)
        };
        let bpm = self.song().tempo_map.bpm_at(start);
        let length = audio::chords::to_ticks(seconds, bpm).max(1);
        let track_id = self
            .session
            .next_track_id()
            .map_err(|error| error.to_string())?;
        let mut track = project::song::Track::new(
            track_id,
            wav.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Audio")
                .to_string(),
        );
        track.audio = Some(project::song::AudioRef {
            path: wav.display().to_string(),
            start,
            length,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        self.session
            .apply(app_commands::Command::InsertTrack { track })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(track_id)
    }

    /// Writes a reversed copy beside the source and points the reference at it.
    ///
    /// # Errors
    ///
    /// Returns a message when the track or source cannot be read or written.
    pub fn reverse_audio(&mut self, track: TrackId) -> Result<std::path::PathBuf, String> {
        let Some(reference) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.audio.clone())
        else {
            return Err(String::from("track has no audio reference"));
        };
        let source = std::path::PathBuf::from(&reference.path);
        let bytes = std::fs::read(&source)
            .map_err(|error| format!("could not read {}: {error}", source.display()))?;
        let audio = audio::wav::decode_stereo(&bytes)?;
        let mut reversed = audio;
        audio::edit::reverse(&mut reversed);
        let stem = source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("audio");
        let out = source.with_file_name(format!("{stem}-reversed.wav"));
        std::fs::write(&out, reversed.to_wav32f())
            .map_err(|error| format!("{}: {error}", out.display()))?;
        let mut updated = reference;
        updated.path = out.display().to_string();
        updated.offset = 0;
        self.session
            .apply(app_commands::Command::SetAudioRef {
                track,
                reference: Some(updated),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(out)
    }

    /// Writes a normalized copy beside the source and points the reference at it.
    ///
    /// Returns the new path. Undo restores the reference but leaves the file.
    ///
    /// # Errors
    ///
    /// Returns a message when the track has no audio, the file cannot be read
    /// or the normalized copy cannot be written.
    pub fn normalize_audio(
        &mut self,
        track: TrackId,
        target: f32,
    ) -> Result<std::path::PathBuf, String> {
        let Some(reference) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.audio.clone())
        else {
            return Err(String::from("track has no audio reference"));
        };
        let source = std::path::PathBuf::from(&reference.path);
        let bytes = std::fs::read(&source)
            .map_err(|error| format!("could not read {}: {error}", source.display()))?;
        let mut audio = audio::wav::decode_stereo(&bytes)?;
        audio::edit::normalize_peak(&mut audio, target);
        let stem = source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("audio");
        let out = source.with_file_name(format!("{stem}-normalized.wav"));
        std::fs::write(&out, audio.to_wav32f())
            .map_err(|error| format!("{}: {error}", out.display()))?;
        let mut updated = reference;
        updated.path = out.display().to_string();
        self.session
            .apply(app_commands::Command::SetAudioRef {
                track,
                reference: Some(updated),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(out)
    }

    /// Adds a comp span that plays a stored take.
    ///
    /// # Errors
    ///
    /// Returns a message when the take or track is unknown.
    pub fn add_audio_comp(
        &mut self,
        track: TrackId,
        take: usize,
        start: Tick,
        length: Tick,
    ) -> Result<(), String> {
        let mut comps = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| entry.comps.clone())
            .ok_or_else(|| String::from("unknown track"))?;
        if self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .is_none_or(|entry| take >= entry.takes.len())
        {
            return Err(String::from("unknown audio take"));
        }
        if length <= 0 {
            return Err(String::from("comp length must be positive"));
        }
        let end = start.max(0).saturating_add(length);
        if comps
            .iter()
            .any(|comp| start.max(0) < comp.start.saturating_add(comp.length) && comp.start < end)
        {
            return Err(String::from("comp span overlaps another span"));
        }
        comps.push(project::song::AudioComp {
            take,
            start: start.max(0),
            length,
        });
        self.session
            .apply(app_commands::Command::SetAudioComps { track, comps })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Changes one comp span's start and length.
    ///
    /// # Errors
    ///
    /// Returns a message when the span is unknown, empty or overlaps another.
    pub fn set_audio_comp(
        &mut self,
        track: TrackId,
        index: usize,
        take: usize,
        start: Tick,
        length: Tick,
    ) -> Result<(), String> {
        let mut comps = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| entry.comps.clone())
            .ok_or_else(|| String::from("unknown track"))?;
        if index >= comps.len() {
            return Err(String::from("unknown comp span"));
        }
        if self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .is_none_or(|entry| take >= entry.takes.len())
        {
            return Err(String::from("unknown audio take"));
        }
        if length <= 0 {
            return Err(String::from("comp length must be positive"));
        }
        let start = start.max(0);
        let end = start.saturating_add(length);
        if comps.iter().enumerate().any(|(other, span)| {
            other != index && start < span.start.saturating_add(span.length) && span.start < end
        }) {
            return Err(String::from("comp span overlaps another span"));
        }
        comps[index].take = take;
        comps[index].start = start;
        comps[index].length = length;
        self.session
            .apply(app_commands::Command::SetAudioComps { track, comps })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Removes one comp span.
    ///
    /// # Errors
    ///
    /// Returns a message when the track or index is unknown.
    pub fn remove_audio_comp(&mut self, track: TrackId, index: usize) -> Result<(), String> {
        let mut comps = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| entry.comps.clone())
            .ok_or_else(|| String::from("unknown track"))?;
        if index >= comps.len() {
            return Err(String::from("unknown comp span"));
        }
        comps.remove(index);
        self.session
            .apply(app_commands::Command::SetAudioComps { track, comps })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Stores the active audio reference as an alternate take.
    ///
    /// # Errors
    ///
    /// Returns a message when the track has no active audio or the edit fails.
    pub fn store_audio_take(&mut self, track: TrackId) -> Result<usize, String> {
        let Some(reference) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.audio.clone())
        else {
            return Err(String::from("track has no audio reference"));
        };
        let mut takes = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| entry.takes.clone())
            .unwrap_or_default();
        if takes
            .iter()
            .any(|take| take.path == reference.path && take.start == reference.start)
        {
            return Ok(takes.len());
        }
        takes.push(reference);
        self.session
            .apply(app_commands::Command::SetAudioTakes {
                track,
                takes: takes.clone(),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(takes.len())
    }

    /// Makes one stored take the active audio reference.
    ///
    /// # Errors
    ///
    /// Returns a message when the track or take is unknown.
    pub fn use_audio_take(&mut self, track: TrackId, index: usize) -> Result<(), String> {
        let take = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.takes.get(index))
            .cloned()
            .ok_or_else(|| String::from("unknown audio take"))?;
        self.session
            .apply(app_commands::Command::SetAudioRef {
                track,
                reference: Some(take),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Removes one stored take.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the edit fails.
    pub fn remove_audio_take(&mut self, track: TrackId, index: usize) -> Result<(), String> {
        let mut takes = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| entry.takes.clone())
            .ok_or_else(|| String::from("unknown track"))?;
        if index >= takes.len() {
            return Err(String::from("unknown audio take"));
        }
        takes.remove(index);
        self.session
            .apply(app_commands::Command::SetAudioTakes { track, takes })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Enables or disables resampling a track's audio to its reference length.
    ///
    /// # Errors
    ///
    /// Returns a message when the track has no audio or the edit is rejected.
    /// Replaces or creates the audio reference of an existing track.
    ///
    /// # Errors
    ///
    /// Returns a message when the file cannot be read or the track is unknown.
    pub fn record_audio_into(
        &mut self,
        track: TrackId,
        wav: &std::path::Path,
        start: Tick,
    ) -> Result<(), String> {
        if !self.song().tracks.iter().any(|entry| entry.id == track) {
            return Err(String::from("unknown track"));
        }
        let bytes = std::fs::read(wav)
            .map_err(|error| format!("could not read {}: {error}", wav.display()))?;
        let info = audio::wav::info(&bytes)?;
        let bpm = self.song().tempo_map.bpm_at(start);
        let length = audio::chords::to_ticks(info.seconds, bpm).max(1);
        let mut takes = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| entry.takes.clone())
            .unwrap_or_default();
        if let Some(previous) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.audio.clone())
            && !takes
                .iter()
                .any(|take| take.path == previous.path && take.start == previous.start)
        {
            takes.push(previous);
        }
        self.session
            .apply(app_commands::Command::SetAudioState {
                track,
                reference: Some(project::song::AudioRef {
                    path: wav.display().to_string(),
                    start,
                    length,
                    offset: 0,
                    fade_in: 0,
                    fade_out: 0,
                    stretch: false,
                    gain_mb: 0,
                }),
                takes,
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    pub fn set_audio_stretch(&mut self, track: TrackId, stretch: bool) -> Result<(), String> {
        let Some(reference) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.audio.clone())
        else {
            return Err(String::from("track has no audio reference"));
        };
        let mut updated = reference;
        updated.stretch = stretch;
        self.session
            .apply(app_commands::Command::SetAudioRef {
                track,
                reference: Some(updated),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets the fade lengths of a track's audio reference.
    ///
    /// # Errors
    ///
    /// Returns a message when the track has no audio or the edit is rejected.
    pub fn set_audio_fades(
        &mut self,
        track: TrackId,
        fade_in: Tick,
        fade_out: Tick,
    ) -> Result<(), String> {
        let Some(reference) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.audio.clone())
        else {
            return Err(String::from("track has no audio reference"));
        };
        let mut updated = reference;
        updated.fade_in = fade_in.max(0);
        updated.fade_out = fade_out.max(0);
        self.session
            .apply(app_commands::Command::SetAudioRef {
                track,
                reference: Some(updated),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Freezes a track: renders it alone to `path`, clears its notes and gives
    /// it the audio reference in one command.
    ///
    /// # Errors
    ///
    /// Returns a message when the render cannot be written or the edit is
    /// rejected.
    pub fn freeze_track(&mut self, track: TrackId, path: &std::path::Path) -> Result<(), String> {
        let mut song = self.song().clone();
        song.tracks.retain(|entry| entry.id == track);
        song.buses.clear();
        let rendered = audio::render(&song, audio::SAMPLE_RATE);
        std::fs::write(path, rendered.to_wav())
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let length = self.song().end_tick().max(1);
        self.session
            .apply(app_commands::Command::FreezeTrack {
                track,
                reference: project::song::AudioRef {
                    path: path.display().to_string(),
                    start: 0,
                    length,
                    offset: 0,

                    fade_in: 0,
                    fade_out: 0,

                    stretch: false,
                    gain_mb: 0,
                },
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets a tempo change at a tick.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_tempo_at(&mut self, tick: Tick, bpm: f64) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetTempo { tick, bpm })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    pub fn set_tempo(&mut self, bpm: f64) -> Result<(), String> {
        self.session
            .apply(Command::SetTempo { tick: 0, bpm })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Runs one assistant command against the session.
    pub fn assistant(&mut self, input: &str) -> mcp::assistant::Reply {
        let reply = mcp::assistant::respond(&mut self.session, input);
        if reply.changed {
            self.dirty = true;
        }
        reply
    }

    /// Replaces the mix buses.
    ///
    /// # Errors
    ///
    /// Returns a message when the command is rejected.
    pub fn set_buses(&mut self, buses: Vec<project::song::Bus>) -> Result<(), String> {
        self.session
            .apply(Command::SetBuses { buses })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets a track's bus routing and send.
    ///
    /// # Errors
    ///
    /// Returns a message when the command is rejected.
    pub fn set_track_routing(
        &mut self,
        track: TrackId,
        bus: Option<u8>,
        send: Option<u8>,
        send_level: u8,
    ) -> Result<(), String> {
        self.session
            .apply(Command::SetTrackRouting {
                track,
                bus,
                send,
                send_level,
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Resizes a note to a new duration.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn resize_note(
        &mut self,
        track: TrackId,
        note: project::NoteId,
        duration: Tick,
    ) -> Result<(), String> {
        self.edit_note(track, note, |target| {
            target.duration = duration.max(1);
        })
    }

    fn edit_note(
        &mut self,
        track: TrackId,
        note: project::NoteId,
        change: impl FnOnce(&mut project::song::Note),
    ) -> Result<(), String> {
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return Err(String::from("unknown track"));
        };
        let mut notes = entry.notes.clone();
        let Some(target) = notes.iter_mut().find(|candidate| candidate.id == note) else {
            return Err(String::from("unknown note"));
        };
        change(target);
        notes.sort_by_key(|note| note.start);
        self.session
            .apply(app_commands::Command::ReplaceTrackNotes { track, notes })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Snaps every note start in every track to a grid.
    ///
    /// # Errors
    ///
    /// Returns a message when a track edit is rejected.
    pub fn quantize(&mut self, grid: Tick) -> Result<usize, String> {
        let changed = app_commands::quantize::quantize(&mut self.session, grid)?;
        self.dirty |= changed > 0;
        Ok(changed)
    }

    /// Sets one note's velocity.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_velocity(
        &mut self,
        track: TrackId,
        note: project::NoteId,
        velocity: u8,
    ) -> Result<(), String> {
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return Err(String::from("unknown track"));
        };
        let mut notes = entry.notes.clone();
        let Some(target) = notes.iter_mut().find(|candidate| candidate.id == note) else {
            return Err(String::from("unknown note"));
        };
        target.velocity =
            music_core::Velocity::new(velocity.clamp(1, 127)).map_err(|error| error.to_string())?;
        self.session
            .apply(app_commands::Command::ReplaceTrackNotes { track, notes })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Empties one Loops slot.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn clear_slot(&mut self, scene: usize, track: TrackId) -> Result<(), String> {
        self.set_slot(scene, track, None)
    }

    /// Sets one Loops slot's loop length.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn set_slot_length(
        &mut self,
        scene: usize,
        track: TrackId,
        length: Tick,
    ) -> Result<(), String> {
        arranging::loops::set_slot_length(&mut self.session, track, scene, length)
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets an audio reference's clip gain in millibels.
    ///
    /// # Errors
    ///
    /// Returns a message when the track has no audio or the edit is rejected.
    pub fn set_audio_gain(&mut self, track: TrackId, gain_mb: i16) -> Result<(), String> {
        let mut reference = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.audio.clone())
            .ok_or_else(|| String::from("track has no audio reference"))?;
        reference.gain_mb = gain_mb;
        self.session
            .apply(app_commands::Command::SetAudioRef {
                track,
                reference: Some(reference),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Moves an audio reference along the timeline by `delta` ticks.
    ///
    /// # Errors
    ///
    /// Returns a message when the track has no audio or the edit is rejected.
    pub fn move_audio(&mut self, track: TrackId, delta: Tick) -> Result<(), String> {
        let reference = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.audio.clone())
            .ok_or_else(|| String::from("track has no audio reference"))?;
        let mut reference = reference;
        reference.start = reference.start.saturating_add(delta).max(0);
        self.session
            .apply(app_commands::Command::SetAudioRef {
                track,
                reference: Some(reference),
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Splits an audio reference or one of its comps at `at`.
    ///
    /// # Errors
    ///
    /// Returns a message when the track has no audio or `at` is outside it.
    pub fn split_audio(&mut self, track: TrackId, at: Tick) -> Result<(), String> {
        app_commands::audio_ref::split(&mut self.session, track, at)
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Removes a track's audio reference.
    ///
    /// # Errors
    ///
    /// Returns a message when the track is unknown or the edit is rejected.
    pub fn remove_audio(&mut self, track: TrackId) -> Result<(), String> {
        self.session
            .apply(app_commands::Command::SetAudioRef {
                track,
                reference: None,
            })
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Trims the front of a track's audio reference.
    ///
    /// # Errors
    ///
    /// Returns a message when the trim is rejected.
    pub fn trim_audio_start(&mut self, track: TrackId, delta: Tick) -> Result<(), String> {
        app_commands::audio_ref::trim_start(&mut self.session, track, delta)
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Sets the end of a track's audio reference.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn trim_audio_length(&mut self, track: TrackId, length: Tick) -> Result<(), String> {
        app_commands::audio_ref::set_length(&mut self.session, track, length)
            .map_err(|error| error.to_string())?;
        self.dirty = true;
        Ok(())
    }

    /// Removes chords inside a range.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn remove_chords(&mut self, from: Tick, to: Tick) -> Result<usize, String> {
        let kept: Vec<project::song::ChordEvent> = self
            .song()
            .chords
            .iter()
            .filter(|event| event.tick < from || event.tick >= to)
            .cloned()
            .collect();
        let removed = self.song().chords.len() - kept.len();
        if removed > 0 {
            self.session
                .apply(app_commands::Command::SetChords { chords: kept })
                .map_err(|error| error.to_string())?;
            self.dirty = true;
        }
        Ok(removed)
    }

    /// Copies chords inside a range.
    pub fn copy_chords(&mut self, start: Tick, end: Tick) {
        self.clipboard = app_commands::clipboard::Clipboard::from_chords(self.song(), start, end);
    }

    /// Pastes copied chords at a tick.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    /// Pastes the chord range repeatedly as one edit.
    ///
    /// # Errors
    ///
    /// Returns a message when the edit is rejected.
    pub fn paste_chords_repeated(
        &mut self,
        at: Tick,
        count: usize,
        spacing: Tick,
    ) -> Result<usize, String> {
        let copied = self
            .clipboard
            .paste_chords_repeated(&mut self.session, at, count, spacing)
            .map_err(|error| error.to_string())?;
        self.dirty |= copied > 0;
        Ok(copied)
    }

    pub fn paste_chords(&mut self, at: Tick) -> Result<usize, String> {
        let copied = self
            .clipboard
            .paste_chords(&mut self.session, at)
            .map_err(|error| error.to_string())?;
        self.dirty |= copied > 0;
        Ok(copied)
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
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        std::fs::write(&temporary, json)
            .map_err(|error| format!("{}: {error}", temporary.display()))?;
        #[cfg(unix)]
        std::fs::rename(&temporary, path).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            format!("{}: {error}", path.display())
        })?;
        #[cfg(not(unix))]
        {
            let _ = std::fs::remove_file(path);
            std::fs::rename(&temporary, path).map_err(|error| {
                let _ = std::fs::remove_file(&temporary);
                format!("{}: {error}", path.display())
            })?;
        }
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
    fn a_section_can_be_joined_to_itself() {
        let mut editor = editor();
        editor.add_marker(0, "A").expect("A");
        editor.add_marker(3840, "B").expect("B");
        let outcome = editor.join_loop("A", 2).expect("join");
        assert_eq!(outcome.copies, 1);
        assert_eq!(outcome.section, "A");
        assert!(editor.undo());
        assert!(
            editor
                .song()
                .markers
                .iter()
                .any(|marker| marker.name == "A")
        );
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
    fn a_scene_can_be_renamed_and_undone() {
        let mut editor = editor();
        editor.rename_scene(0, "Verse").expect("rename");
        assert_eq!(editor.song().scene_names[0], "Verse");
        assert_eq!(editor.song().scene_name(0), "Verse");
        assert!(editor.undo());
        assert!(editor.song().scene_names.is_empty());
    }

    #[test]
    fn chords_copy_cut_and_paste_a_range() {
        let mut editor = editor();
        editor.apply_progression("C F G", 0).expect("progression");
        let before = editor.song().chords.len();
        assert!(before >= 2);
        let first = editor.song().chords[0].tick;
        let second = editor.song().chords[1].tick;
        editor.copy_chords(first, second);
        let removed = editor.remove_chords(first, second).expect("remove");
        assert!(removed >= 1);
        let pasted = editor.paste_chords(15360).expect("paste");
        assert!(pasted >= 1);
        assert!(editor.song().chords.iter().any(|event| event.tick == 15360));
        assert!(editor.undo());
    }

    #[test]
    fn note_duration_change_is_undoable() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let note = editor.song().tracks[0].notes[0].id;
        editor
            .set_note_duration(track, note, 1920)
            .expect("duration");
        assert_eq!(editor.song().tracks[0].notes[0].duration, 1920);
        assert!(editor.set_note_duration(track, note, 0).is_err());
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].notes[0].duration, PPQ);
    }

    #[test]
    fn note_entry_uses_the_requested_length() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("track");
        let id = editor
            .insert_length(
                track,
                960,
                Pitch::from_name("C5").expect("pitch"),
                music_core::PPQ / 2,
            )
            .expect("insert");
        let note = editor.song().tracks[0]
            .notes
            .iter()
            .find(|note| note.id == id)
            .expect("note");
        assert_eq!(note.duration, music_core::PPQ / 2);
    }

    #[test]
    fn audio_comps_add_and_remove() {
        let mut song = Song::new("Comps", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        track.takes.push(project::song::AudioRef {
            path: String::from("take.wav"),
            start: 0,
            length: music_core::PPQ,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("editor");
        editor
            .add_audio_comp(TrackId::new(1).expect("track"), 0, 0, music_core::PPQ)
            .expect("add");
        assert_eq!(editor.song().tracks[0].comps.len(), 1);
        assert!(
            editor
                .add_audio_comp(
                    TrackId::new(1).expect("track"),
                    0,
                    music_core::PPQ / 2,
                    music_core::PPQ
                )
                .is_err(),
            "overlapping comps are rejected"
        );
        editor
            .set_audio_comp(
                TrackId::new(1).expect("track"),
                0,
                0,
                music_core::PPQ,
                music_core::PPQ / 2,
            )
            .expect("set");
        assert_eq!(editor.song().tracks[0].comps[0].start, music_core::PPQ);
        editor
            .remove_audio_comp(TrackId::new(1).expect("track"), 0)
            .expect("remove");
        assert!(editor.song().tracks[0].comps.is_empty());
    }

    #[test]
    fn audio_takes_store_use_and_remove() {
        let mut song = Song::new("Takes", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        track.audio = Some(project::song::AudioRef {
            path: String::from("first.wav"),
            start: 0,
            length: music_core::PPQ,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("editor");
        assert_eq!(
            editor.store_audio_take(TrackId::new(1).expect("track")),
            Ok(1)
        );
        editor
            .session
            .apply(app_commands::Command::SetAudioRef {
                track: TrackId::new(1).expect("track"),
                reference: Some(project::song::AudioRef {
                    path: String::from("second.wav"),
                    start: 0,
                    length: music_core::PPQ,
                    offset: 0,
                    fade_in: 0,
                    fade_out: 0,
                    stretch: false,
                    gain_mb: 0,
                }),
            })
            .expect("second take");
        editor
            .use_audio_take(TrackId::new(1).expect("track"), 0)
            .expect("use");
        assert_eq!(
            editor.song().tracks[0].audio.as_ref().expect("audio").path,
            "first.wav"
        );
        editor
            .remove_audio_take(TrackId::new(1).expect("track"), 0)
            .expect("remove");
        assert!(editor.song().tracks[0].takes.is_empty());
    }

    #[test]
    fn recording_stores_the_replaced_take() {
        let mut song = Song::new("Record", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        track.audio = Some(project::song::AudioRef {
            path: String::from("first.wav"),
            start: 0,
            length: music_core::PPQ,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("editor");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-record-take-{}.wav",
            std::process::id()
        ));
        let wav = audio::RenderedAudio {
            samples: vec![0.1; 4800 * 2],
            sample_rate: 48_000,
        };
        std::fs::write(&path, wav.to_wav()).expect("wav");
        editor
            .record_audio_into(TrackId::new(1).expect("track"), &path, 0)
            .expect("record");
        let _ = std::fs::remove_file(&path);
        assert_eq!(editor.song().tracks[0].takes.len(), 1);
        assert_eq!(editor.song().tracks[0].takes[0].path, "first.wav");
        assert!(
            editor.song().tracks[0]
                .audio
                .as_ref()
                .expect("audio")
                .path
                .contains("lazy-arranger-record-take")
        );
    }

    #[test]
    fn moving_an_audio_reference_is_undoable() {
        let mut song = Song::new("Move", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        track.audio = Some(project::song::AudioRef {
            path: String::from("take.wav"),
            start: 0,
            length: 4 * music_core::PPQ,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("editor");
        let track = TrackId::new(1).expect("track");
        editor.move_audio(track, music_core::PPQ).expect("move");
        assert_eq!(
            editor.song().tracks[0]
                .audio
                .as_ref()
                .map(|audio| audio.start),
            Some(music_core::PPQ)
        );
        assert!(editor.undo());
        assert_eq!(
            editor.song().tracks[0]
                .audio
                .as_ref()
                .map(|audio| audio.start),
            Some(0)
        );
        editor
            .move_audio(track, -music_core::PPQ)
            .expect("move back");
        assert_eq!(
            editor.song().tracks[0]
                .audio
                .as_ref()
                .map(|audio| audio.start),
            Some(0)
        );
    }

    #[test]
    fn audio_clip_gain_is_undoable() {
        let mut song = Song::new("Gain", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        track.audio = Some(project::song::AudioRef {
            path: String::from("take.wav"),
            start: 0,
            length: 4 * music_core::PPQ,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("editor");
        let track = TrackId::new(1).expect("track");
        editor.set_audio_gain(track, -3_000).expect("gain");
        assert_eq!(
            editor.song().tracks[0]
                .audio
                .as_ref()
                .map(|audio| audio.gain_mb),
            Some(-3_000)
        );
        assert!(editor.undo());
        assert_eq!(
            editor.song().tracks[0]
                .audio
                .as_ref()
                .map(|audio| audio.gain_mb),
            Some(0)
        );
    }

    #[test]
    fn splitting_audio_creates_two_comps() {
        let mut song = Song::new("Split", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        track.audio = Some(project::song::AudioRef {
            path: String::from("take.wav"),
            start: 0,
            length: 4 * music_core::PPQ,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("editor");
        let track = TrackId::new(1).expect("track");
        editor
            .split_audio(track, 2 * music_core::PPQ)
            .expect("split");
        let entry = &editor.song().tracks[0];
        assert_eq!(entry.takes.len(), 2);
        assert_eq!(entry.comps.len(), 2);
        assert_eq!(entry.comps[0].start, 0);
        assert_eq!(entry.comps[0].length, 2 * music_core::PPQ);
        assert_eq!(entry.comps[1].start, 2 * music_core::PPQ);
        assert_eq!(entry.comps[1].length, 2 * music_core::PPQ);
        assert_eq!(entry.takes[1].offset, 2 * music_core::PPQ);
        editor
            .split_audio(track, music_core::PPQ)
            .expect("second split");
        let entry = &editor.song().tracks[0];
        assert_eq!(entry.comps.len(), 3);
        assert_eq!(entry.takes.len(), 3);
        assert_eq!(entry.takes[2].offset, music_core::PPQ);
        assert_eq!(entry.comps[0].length, music_core::PPQ);
        assert_eq!(entry.comps[1].start, music_core::PPQ);
        assert!(editor.undo());
        let entry = &editor.song().tracks[0];
        assert_eq!(entry.comps.len(), 2);
        assert!(editor.undo());
        assert!(editor.song().tracks[0].comps.is_empty());
        assert!(editor.song().tracks[0].takes.is_empty());
    }

    #[test]
    fn removing_an_audio_reference_is_undoable() {
        let mut song = Song::new("Remove", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        track.audio = Some(project::song::AudioRef {
            path: String::from("take.wav"),
            start: 0,
            length: 4 * music_core::PPQ,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("editor");
        let track = TrackId::new(1).expect("track");
        editor.remove_audio(track).expect("remove");
        assert!(editor.song().tracks[0].audio.is_none());
        assert!(editor.undo());
        assert!(editor.song().tracks[0].audio.is_some());
    }

    #[test]
    fn trimming_an_audio_end_is_undoable() {
        let mut song = Song::new("Trim", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        track.audio = Some(project::song::AudioRef {
            path: String::from("take.wav"),
            start: 0,
            length: 4 * music_core::PPQ,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let mut editor = Editor::new(ProjectFile::new(song)).expect("editor");
        editor
            .trim_audio_length(TrackId::new(1).expect("track"), music_core::PPQ)
            .expect("trim");
        assert_eq!(
            editor.song().tracks[0]
                .audio
                .as_ref()
                .expect("audio")
                .length,
            music_core::PPQ
        );
        editor.undo();
        assert_eq!(
            editor.song().tracks[0]
                .audio
                .as_ref()
                .expect("audio")
                .length,
            4 * music_core::PPQ
        );
    }

    #[test]
    fn score_marks_add_and_remove() {
        let mut editor = editor();
        editor
            .add_mark(
                music_core::PPQ,
                project::song::MarkKind::Dynamic(project::song::Dynamic::F),
            )
            .expect("add");
        assert_eq!(editor.song().marks.len(), 1);
        editor.remove_mark(0).expect("remove");
        assert!(editor.song().marks.is_empty());
        editor.undo();
        assert_eq!(editor.song().marks.len(), 1);
    }

    #[test]
    fn generating_a_part_is_one_undo_step() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("track");
        editor
            .generate_part(track, "humanize", 7)
            .expect("generate");
        editor.undo();
        assert!(!editor.song().tracks.is_empty());
    }

    #[test]
    fn replacing_all_chords_is_undoable() {
        let mut editor = editor();
        let chords = vec![
            project::song::ChordEvent {
                tick: 0,
                chord: Chord::parse("C").expect("valid"),
            },
            project::song::ChordEvent {
                tick: 960,
                chord: Chord::parse("G").expect("valid"),
            },
        ];
        let before = editor.song().chords.len();
        editor.set_chords(chords).expect("chords");
        assert_eq!(editor.song().chords.len(), 2);
        editor.undo();
        assert_eq!(editor.song().chords.len(), before);
    }

    #[test]
    fn setting_one_chord_is_undoable() {
        let mut editor = editor();
        let expected = Chord::parse("G7").expect("valid");
        editor.set_chord(960, "G7").expect("chord");
        assert!(
            editor
                .song()
                .chords
                .iter()
                .any(|event| event.tick == 960 && event.chord == expected)
        );
        editor.undo();
        assert!(
            !editor
                .song()
                .chords
                .iter()
                .any(|event| event.tick == 960 && event.chord == expected)
        );
    }

    #[test]
    fn meter_change_is_validated_and_undoable() {
        let mut editor = editor();
        editor.set_time_signature(3, 4).expect("3/4");
        assert_eq!(editor.song().time_signature.numerator, 3);
        assert!(editor.set_time_signature(0, 4).is_err());
        assert!(editor.set_time_signature(4, 3).is_err());
        assert!(editor.undo());
        assert_eq!(editor.song().time_signature.numerator, 4);
    }

    #[test]
    fn markers_add_replace_remove_and_undo() {
        let mut editor = editor();
        editor.add_marker(0, "Intro").expect("add");
        editor.add_marker(3840, "Verse").expect("add");
        assert_eq!(editor.song().markers.len(), 2);
        editor.add_marker(0, "Start").expect("replace");
        assert_eq!(editor.song().markers.len(), 2);
        assert_eq!(editor.song().markers[0].name, "Start");
        editor.remove_marker(3840).expect("remove");
        assert_eq!(editor.song().markers.len(), 1);
        assert!(editor.undo());
        assert_eq!(editor.song().markers.len(), 2);
    }

    #[test]
    fn song_rename_is_undoable() {
        let mut editor = editor();
        let before = editor.song().name.clone();
        editor.rename_song("New title").expect("rename");
        assert_eq!(editor.song().name, "New title");
        assert!(editor.undo());
        assert_eq!(editor.song().name, before);
    }

    #[test]
    fn lyrics_add_replace_and_remove() {
        let mut editor = editor();
        editor.add_lyric(0, "la").expect("add");
        editor.add_lyric(960, "da").expect("add");
        assert_eq!(editor.song().lyrics.len(), 2);
        editor.add_lyric(0, "lo").expect("replace");
        assert_eq!(editor.song().lyrics.len(), 2);
        assert_eq!(editor.song().lyrics[0].text, "lo");
        editor.remove_lyric(960).expect("remove");
        assert_eq!(editor.song().lyrics.len(), 1);
        assert!(editor.undo());
        assert_eq!(editor.song().lyrics.len(), 2);
    }

    #[test]
    fn removing_a_track_is_undoable() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        editor.remove_track(track).expect("remove");
        assert!(editor.song().tracks.iter().all(|entry| entry.id != track));
        assert!(editor.undo());
        assert!(editor.song().tracks.iter().any(|entry| entry.id == track));
    }

    #[test]
    fn a_midi_track_can_be_duplicated() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let before = editor.song().tracks[0].notes.len();
        let id = editor.duplicate_track(track).expect("duplicate");
        let copy = editor
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == id)
            .expect("copy");
        assert_eq!(copy.notes.len(), before);
        assert!(copy.notes.iter().all(|note| note.id.get() >= 1));
        assert!(copy.name.ends_with("copy"));
        assert!(editor.undo());
        assert!(editor.song().tracks.iter().all(|entry| entry.id != id));
    }

    #[test]
    fn audio_can_be_reversed_to_a_new_source() {
        let mut editor = editor();
        let path =
            std::env::temp_dir().join(format!("lazy-arranger-reverse-{}.wav", std::process::id()));
        let audio = audio::RenderedAudio {
            samples: vec![0.1, 0.1, 0.2, 0.2, 0.3, 0.3, 0.4, 0.4],
            sample_rate: 48_000,
        };
        std::fs::write(&path, audio.to_wav32f()).expect("wav");
        let track = editor.import_audio(&path, 0).expect("import");
        let out = editor.reverse_audio(track).expect("reverse");
        let bytes = std::fs::read(&out).expect("read");
        let reversed = audio::wav::decode_stereo(&bytes).expect("decode");
        assert!((reversed.samples[0] - 0.4).abs() < 1.0e-4);
        assert!((reversed.samples[6] - 0.1).abs() < 1.0e-4);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn an_audio_track_can_be_duplicated() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let id = editor.duplicate_audio_track(track).expect("duplicate");
        let copy = editor
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == id)
            .expect("copy");
        let source = editor
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .expect("source");
        assert_eq!(copy.audio, source.audio);
        assert_eq!(copy.takes, source.takes);
        assert_eq!(copy.comps, source.comps);
        assert!(copy.name.ends_with("copy"));
        assert!(editor.undo());
        assert!(editor.song().tracks.iter().all(|entry| entry.id != id));
    }

    #[test]
    fn a_clip_can_be_added_at_the_playhead() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        editor.add_clip(track, 960, 960).expect("clip");
        let clip = &editor.song().tracks[0].clips[0];
        assert_eq!((clip.start, clip.length), (960, 960));
        assert!(editor.undo());
        assert!(editor.song().tracks[0].clips.is_empty());
    }

    #[test]
    fn recorded_notes_can_append_to_an_armed_track() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let before = editor.song().tracks[0].notes.len();
        let notes = vec![editor.song().tracks[0].notes[0].clone()];
        editor.append_recorded(track, &notes).expect("append");
        assert_eq!(editor.song().tracks[0].notes.len(), before + 1);
        assert!(
            editor.song().tracks[0]
                .clips
                .iter()
                .any(|clip| clip.name == "Recorded")
        );
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].notes.len(), before);
    }

    #[test]
    fn recorded_notes_become_a_new_track() {
        let mut editor = editor();
        let notes = vec![project::song::Note {
            id: project::NoteId::new(99).expect("valid"),
            start: 0,
            duration: music_core::PPQ,
            pitch: music_core::Pitch::from_name("D4").expect("valid"),
            velocity: music_core::Velocity::new(90).expect("valid"),
            channel: music_core::Channel::first(),
        }];
        let track = editor.insert_recorded("Recorded", &notes).expect("insert");
        let recorded = editor
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .expect("track");
        assert_eq!(recorded.name, "Recorded");
        assert_eq!(recorded.notes.len(), 1);
        assert_eq!(recorded.notes[0].id.get(), 1, "ids are reassigned");
        assert_eq!(recorded.clips.len(), 1);
        let bar = music_core::TimeSignature::new(4, 4)
            .expect("meter")
            .bar_ticks();
        assert_eq!(recorded.clips[0].length % bar, 0);
        assert!(editor.undo());
        assert!(editor.song().tracks.iter().all(|entry| entry.id != track));
    }

    #[test]
    fn renaming_and_program_changes_are_undoable() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        editor.rename_track(track, "Strings").expect("rename");
        assert_eq!(editor.song().tracks[0].name, "Strings");
        editor.set_track_program(track, Some(48)).expect("program");
        assert_eq!(editor.song().tracks[0].program, Some(48));
        editor.set_track_program(track, None).expect("clear");
        assert!(editor.song().tracks[0].program.is_none());
        editor
            .set_track_bank(track, Some(project::song::BankSelect { msb: 63, lsb: 1 }))
            .expect("bank");
        assert_eq!(
            editor.song().tracks[0].bank,
            Some(project::song::BankSelect { msb: 63, lsb: 1 })
        );
        editor.set_track_bank(track, None).expect("clear bank");
        assert!(editor.song().tracks[0].bank.is_none());
        assert!(editor.undo());
        assert_eq!(
            editor.song().tracks[0].bank,
            Some(project::song::BankSelect { msb: 63, lsb: 1 })
        );
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].program, None);
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].program, Some(48));
    }

    #[test]
    fn moving_a_track_to_a_channel_is_undoable() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let channel = music_core::Channel::from_one_based(10).expect("channel");
        editor.set_track_channel(track, channel).expect("channel");
        assert!(
            editor.song().tracks[0]
                .notes
                .iter()
                .all(|note| note.channel == channel)
        );
        assert!(editor.undo());
    }

    #[test]
    fn plugins_add_toggle_and_remove() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let first = editor
            .add_plugin(
                track,
                "Reverb",
                project::song::PluginFormat::Vst3,
                "/p/Reverb.vst3",
                "",
            )
            .expect("add");
        let second = editor
            .add_plugin(
                track,
                "Comp",
                project::song::PluginFormat::AudioUnit,
                "/p/Comp.component",
                "",
            )
            .expect("add");
        assert_ne!(first, second);
        assert_eq!(editor.song().tracks[0].plugins.len(), 2);
        editor.toggle_plugin(track, first).expect("bypass");
        assert!(editor.song().tracks[0].plugins[0].bypassed);
        editor.remove_plugin(track, second).expect("remove");
        assert_eq!(editor.song().tracks[0].plugins.len(), 1);
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].plugins.len(), 2);
        editor.move_plugin(track, second, true).expect("move up");
        assert_eq!(editor.song().tracks[0].plugins[0].id, second);
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].plugins[0].id, first);
    }

    #[test]
    fn moving_the_first_plugin_earlier_is_a_no_op() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let first = editor
            .add_plugin(
                track,
                "First",
                project::song::PluginFormat::Vst3,
                "/p/First.vst3",
                "",
            )
            .expect("add");
        editor
            .add_plugin(
                track,
                "Second",
                project::song::PluginFormat::Vst3,
                "/p/Second.vst3",
                "",
            )
            .expect("add");
        editor.move_plugin(track, first, true).expect("bounded");
        assert_eq!(editor.song().tracks[0].plugins[0].id, first);
        editor.move_plugin(track, first, false).expect("move down");
        assert_eq!(editor.song().tracks[0].plugins[1].id, first);
    }

    #[test]
    fn a_plugin_state_is_stored_and_undoable() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let slot = editor
            .add_plugin(
                track,
                "Dynamics",
                project::song::PluginFormat::AudioUnit,
                "",
                "appl:dcmp",
            )
            .expect("add");
        editor
            .set_plugin_state(track, slot, "0=0.5;")
            .expect("state");
        assert_eq!(editor.song().tracks[0].plugins[0].state, "0=0.5;");
        assert!(editor.undo());
        assert!(editor.song().tracks[0].plugins[0].state.is_empty());
    }

    #[test]
    fn a_synth_patch_is_undoable() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let patch = project::song::Patch {
            waveform: project::song::Waveform::Saw,
            attack: 0.02,
            decay: 0.3,
            sustain: 0.5,
            release: 0.4,
            brightness: 0.8,
        };
        editor
            .set_track_patch(track, Some(patch))
            .expect("set patch");
        assert_eq!(editor.song().tracks[0].patch, Some(patch));
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].patch, None);
        editor
            .set_track_patch(track, Some(patch))
            .expect("set patch");
        editor.set_track_patch(track, None).expect("clear patch");
        assert_eq!(editor.song().tracks[0].patch, None);
    }

    #[test]
    fn recorded_audio_can_replace_an_armed_tracks_reference() {
        let path = std::env::temp_dir().join("lazy-arranger-record-target.wav");
        let audio = audio::RenderedAudio {
            samples: vec![0.1; 4_800 * 2],
            sample_rate: 48_000,
        };
        std::fs::write(&path, audio.to_wav()).expect("write");
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        editor
            .record_audio_into(track, &path, 0)
            .expect("record into");
        let reference = editor.song().tracks[0].audio.as_ref().expect("reference");
        assert_eq!(reference.path, path.display().to_string());
        assert!(reference.length > 0);
        assert!(editor.undo());
        assert!(editor.song().tracks[0].audio.is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn normalize_audio_writes_a_copy_and_switches_the_reference() {
        let source = std::env::temp_dir().join("lazy-arranger-normalize-source.wav");
        let audio = audio::RenderedAudio {
            samples: vec![0.2, -0.4, 0.1, -0.2],
            sample_rate: 48_000,
        };
        std::fs::write(&source, audio.to_wav()).expect("write");
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        editor
            .session
            .apply(app_commands::Command::SetAudioRef {
                track,
                reference: Some(project::song::AudioRef {
                    path: source.display().to_string(),
                    start: 0,
                    length: 3840,
                    offset: 0,
                    fade_in: 0,
                    fade_out: 0,
                    stretch: false,
                    gain_mb: 0,
                }),
            })
            .expect("audio");
        let out = editor.normalize_audio(track, 0.9).expect("normalize");
        let reference = editor.song().tracks[0].audio.as_ref().expect("audio");
        assert_eq!(reference.path, out.display().to_string());
        let bytes = std::fs::read(&out).expect("read normalized");
        let decoded = audio::wav::decode_stereo(&bytes).expect("decode");
        let peak = decoded
            .samples
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0_f32, f32::max);
        assert!((peak - 0.9).abs() < 0.001);
        assert!(editor.undo());
        assert_eq!(
            editor.song().tracks[0].audio.as_ref().expect("audio").path,
            source.display().to_string()
        );
        let _ = std::fs::remove_file(source);
        let _ = std::fs::remove_file(out);
    }

    #[test]
    fn audio_fades_apply_and_undo() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        editor
            .session
            .apply(app_commands::Command::SetAudioRef {
                track,
                reference: Some(project::song::AudioRef {
                    path: String::from("/tmp/take.wav"),
                    start: 0,
                    length: 3840,
                    offset: 0,
                    fade_in: 0,
                    fade_out: 0,

                    stretch: false,
                    gain_mb: 0,
                }),
            })
            .expect("audio");
        editor.set_audio_fades(track, 960, 480).expect("fades");
        let reference = editor.song().tracks[0].audio.as_ref().expect("audio");
        assert_eq!((reference.fade_in, reference.fade_out), (960, 480));
        editor.set_audio_stretch(track, true).expect("stretch");
        assert!(
            editor.song().tracks[0]
                .audio
                .as_ref()
                .expect("audio")
                .stretch
        );
        assert!(editor.undo());
        assert!(
            !editor.song().tracks[0]
                .audio
                .as_ref()
                .expect("audio")
                .stretch
        );
        assert!(editor.undo());
        assert_eq!(
            editor.song().tracks[0]
                .audio
                .as_ref()
                .expect("audio")
                .fade_in,
            0
        );
    }

    #[test]
    fn the_editor_sets_and_clears_a_soundfont() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        editor
            .set_soundfont(
                track,
                Some(project::song::SoundFontRef {
                    path: String::from("/tmp/bank.sf2"),
                    bank: 1,
                    program: 2,
                }),
            )
            .expect("set");
        assert_eq!(
            editor.song().tracks[0]
                .soundfont
                .as_ref()
                .map(|font| font.bank),
            Some(1)
        );
        editor.set_soundfont(track, None).expect("clear");
        assert!(editor.song().tracks[0].soundfont.is_none());
        assert!(editor.undo());
        assert!(editor.song().tracks[0].soundfont.is_some());
    }

    #[test]
    fn a_wav_imports_as_an_audio_track() {
        let path = std::env::temp_dir().join("lazy-arranger-import-audio.wav");
        let rendered = audio::RenderedAudio {
            samples: vec![0.1; 48_000 * 2],
            sample_rate: 48_000,
        };
        std::fs::write(&path, rendered.to_wav()).expect("write");
        let mut editor = editor();
        let track = editor.import_audio(&path, 0).expect("import");
        let imported = editor
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .expect("track");
        let reference = imported.audio.as_ref().expect("audio");
        assert_eq!(reference.path, path.display().to_string());
        assert!(reference.length > 0);
        assert!(editor.undo());
        assert_eq!(editor.song().tracks.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn l6max_takes_become_audio_tracks() {
        let mut editor = editor();
        let plan = audio::l6max::Plan {
            takes: vec![audio::l6max::Take {
                path: std::path::PathBuf::from("/tmp/take-01.wav"),
                name: String::from("Take 01"),
                seconds: 12.5,
                channels: 2,
                sample_rate: 48_000,
                bits: 32,
                format_tag: 3,
            }],
            problems: Vec::new(),
        };
        assert_eq!(editor.import_l6max(&plan, 0).expect("import"), 1);
        let imported = &editor.song().tracks[1];
        assert_eq!(imported.name, "Take 01");
        assert!(imported.audio.is_some());
        assert!(editor.undo());
        assert_eq!(editor.song().tracks.len(), 1);
    }

    #[test]
    fn freezing_renders_audio_and_undo_restores_notes() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let path = std::env::temp_dir().join("lazy-arranger-freeze-test.wav");
        editor.freeze_track(track, &path).expect("freeze");
        assert!(editor.song().tracks[0].notes.is_empty());
        assert!(editor.song().tracks[0].audio.is_some());
        assert!(std::fs::metadata(&path).expect("file").len() > 44);
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].notes.len(), 1);
        assert!(editor.song().tracks[0].audio.is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn tempo_changes_accumulate_on_the_map() {
        let mut editor = editor();
        editor.set_tempo_at(0, 100.0).expect("first");
        editor.set_tempo_at(3840, 140.0).expect("second");
        assert_eq!(editor.song().tempo_map.changes().len(), 2);
        assert!((editor.song().tempo_map.bpm_at(0) - 100.0).abs() < 0.01);
        assert!((editor.song().tempo_map.bpm_at(3840) - 140.0).abs() < 0.01);
        assert!(editor.undo());
        assert_eq!(editor.song().tempo_map.changes().len(), 1);
    }

    #[test]
    fn merging_one_lane_keeps_the_others() {
        use project::song::{Automation, AutomationPoint, AutomationTarget};
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        editor
            .set_automation(
                track,
                vec![
                    Automation {
                        target: AutomationTarget::Volume,
                        points: vec![AutomationPoint {
                            tick: 0,
                            value: 10.0,
                        }],
                    },
                    Automation {
                        target: AutomationTarget::Pan,
                        points: vec![AutomationPoint {
                            tick: 0,
                            value: 90.0,
                        }],
                    },
                ],
            )
            .expect("lanes");
        editor
            .merge_automation(
                track,
                Automation {
                    target: AutomationTarget::Volume,
                    points: vec![AutomationPoint {
                        tick: 480,
                        value: 50.0,
                    }],
                },
            )
            .expect("merge");
        let lanes = &editor.song().tracks[0].automation;
        assert_eq!(lanes.len(), 2);
        assert!(
            lanes
                .iter()
                .any(|lane| lane.target == AutomationTarget::Pan)
        );
        assert!(
            lanes
                .iter()
                .find(|lane| lane.target == AutomationTarget::Volume)
                .is_some_and(|lane| lane.points[0].tick == 480)
        );
    }

    #[test]
    fn several_notes_copy_and_cut() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        editor
            .insert(
                track,
                960,
                music_core::Pitch::from_name("E4").expect("valid"),
            )
            .expect("insert");
        let ids: Vec<project::NoteId> = editor.song().tracks[0]
            .notes
            .iter()
            .map(|note| note.id)
            .collect();
        editor.copy_notes(track, &ids);
        let pasted = editor.paste_notes(track, 3840).expect("paste");
        assert_eq!(pasted, 2);
        assert_eq!(editor.song().tracks[0].notes.len(), 4);
        let removed = editor.cut_notes(track, &ids).expect("cut");
        assert_eq!(removed, 2);
        assert_eq!(editor.song().tracks[0].notes.len(), 2);
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].notes.len(), 4);
    }

    #[test]
    fn several_notes_delete_in_one_edit() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        editor
            .insert(
                track,
                960,
                music_core::Pitch::from_name("E4").expect("valid"),
            )
            .expect("insert");
        let ids: Vec<project::NoteId> = editor.song().tracks[0]
            .notes
            .iter()
            .map(|note| note.id)
            .collect();
        assert_eq!(editor.delete_notes(track, &ids).expect("delete"), 2);
        assert!(editor.song().tracks[0].notes.is_empty());
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].notes.len(), 2);
    }

    #[test]
    fn notes_move_and_resize_through_commands() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let note = editor.song().tracks[0].notes[0].id;
        editor
            .move_note(
                track,
                note,
                1920,
                music_core::Pitch::new(67).expect("valid"),
            )
            .expect("move");
        let moved = &editor.song().tracks[0].notes[0];
        assert_eq!((moved.start, moved.pitch.as_u8()), (1920, 67));
        editor.resize_note(track, note, 480).expect("resize");
        assert_eq!(editor.song().tracks[0].notes[0].duration, 480);
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].notes[0].duration, music_core::PPQ);
    }

    #[test]
    fn quantise_snaps_starts_and_sets_velocity() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let note = editor.song().tracks[0].notes[0].id;
        editor.set_velocity(track, note, 70).expect("velocity");
        assert_eq!(editor.song().tracks[0].notes[0].velocity.as_u8(), 70);
        editor
            .insert(
                track,
                125,
                music_core::Pitch::from_name("E4").expect("valid"),
            )
            .expect("insert");
        let changed = editor.quantize(music_core::PPQ / 4).expect("quantise");
        assert!(changed >= 1);
        assert!(
            editor.song().tracks[0]
                .notes
                .iter()
                .all(|note| note.start % (music_core::PPQ / 4) == 0)
        );
        assert!(editor.undo());
    }

    #[test]
    fn slot_wrappers_clear_and_resize_through_commands() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        assert_eq!(editor.loops_from_timeline(4).expect("fill"), 1);
        assert!(editor.song().tracks[0].slot(0).is_some());
        editor.set_slot_length(0, track, 1920).expect("length");
        assert_eq!(
            editor.song().tracks[0].slot(0).map(|clip| clip.length),
            Some(1920)
        );
        editor.clear_slot(0, track).expect("clear");
        assert!(editor.song().tracks[0].slot(0).is_none());
        assert!(editor.undo());
        assert!(editor.song().tracks[0].slot(0).is_some());
    }

    #[test]
    fn a_note_can_be_copied_and_pasted() {
        let mut editor = editor();
        let track = TrackId::new(1).expect("valid");
        let note = editor.song().tracks[0].notes[0].id;
        editor.copy_note(track, note);
        let pasted = editor.paste_notes(track, 3840).expect("paste");
        assert_eq!(pasted, 1);
        assert_eq!(editor.song().tracks[0].notes.len(), 2);
        assert!(editor.undo());
        assert_eq!(editor.song().tracks[0].notes.len(), 1);
        assert!(editor.cut_note(track, note));
        assert!(editor.song().tracks[0].notes.is_empty());
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
