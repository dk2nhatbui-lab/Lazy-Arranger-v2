//! One command path for every editor.
//!
//! GUI, CLI, automation and a future MCP server all build the same
//! [`Command`] values and hand them to [`EditSession::apply`]. Apply validates
//! the command, mutates the song, validates the result, and only then records
//! an undo step. A command that makes the song invalid is rolled back, so the
//! session never holds a broken document.
//!
//! Undo granularity is one command per step. Coalescing (for typing or a drag)
//! is deferred to ADR-006, which also covers preview and approval for
//! destructive model-proposed edits.

pub mod audio_ref;
pub mod bounce;
pub mod clipboard;
pub mod quantize;
pub mod style_import;

use music_core::{Channel, Chord, Pitch, TempoMap, Tick, Velocity};
use project::provenance;
use project::song::{BankSelect, ChordEvent, Clip, Note, Song, Track};
use project::{NoteId, ProjectError, ProjectFile, Severity, TrackId};
use thiserror::Error;

/// One edit in the shared command vocabulary.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Insert a note with a caller-chosen identifier.
    InsertNote {
        /// Owning track.
        track: TrackId,
        /// The complete note.
        note: Note,
    },
    /// Remove a note.
    RemoveNote {
        /// Owning track.
        track: TrackId,
        /// The note to remove.
        note: NoteId,
    },
    /// Change one note's duration.
    SetNoteDuration {
        /// Owning track.
        track: TrackId,
        /// The note.
        note: NoteId,
        /// New duration in ticks.
        duration: Tick,
    },
    /// Place or replace the chord at a tick.
    SetChord {
        /// Position in ticks.
        tick: Tick,
        /// The chord.
        chord: Chord,
    },
    /// Remove the chord at a tick.
    RemoveChord {
        /// Position in ticks.
        tick: Tick,
    },
    /// Set the tempo that starts at a tick.
    SetTempo {
        /// Position in ticks.
        tick: Tick,
        /// Tempo in beats per minute.
        bpm: f64,
    },
    /// Rename a track.
    RenameTrack {
        /// The track.
        track: TrackId,
        /// New name.
        name: String,
    },
    /// Set the song meter.
    SetTimeSignature {
        /// New signature.
        time_signature: music_core::TimeSignature,
    },
    /// Rename the song.
    SetSongName {
        /// New name.
        name: String,
    },
    /// Insert an empty track with a caller-chosen identifier.
    InsertTrack {
        /// The complete track.
        track: Track,
    },
    /// Remove a track and every note on it.
    RemoveTrack {
        /// The track.
        track: TrackId,
    },
    /// Set a track's mute, solo and arm flags.
    SetTrackFlags {
        /// The track.
        track: TrackId,
        /// New mute state.
        muted: bool,
        /// New solo state.
        soloed: bool,
        /// New arm state.
        armed: bool,
    },
    /// Replace a track's plugin slots.
    SetPlugins {
        /// The track.
        track: TrackId,
        /// New slots in signal order.
        plugins: Vec<project::song::PluginSlot>,
    },
    /// Replace a track's phrase slurs.
    SetSlurs {
        /// The track.
        track: TrackId,
        /// New slurs.
        slurs: Vec<project::song::Slur>,
    },
    /// Set or clear a track's SoundFont preset.
    SetSoundFont {
        /// The track.
        track: TrackId,
        /// Preset, or none to use the built-in timbre.
        soundfont: Option<project::song::SoundFontRef>,
    },
    /// Set a track's synth patch, or clear it.
    SetTrackPatch {
        /// The track.
        track: TrackId,
        /// New patch.
        patch: Option<Box<project::song::Patch>>,
    },
    /// Replace the mix buses.
    SetBuses {
        /// New buses.
        buses: Vec<project::song::Bus>,
    },
    /// Replace the master-bus plug-in chain.
    SetMasterPlugins {
        /// New master slots.
        plugins: Vec<project::song::PluginSlot>,
    },
    /// Set a track's bus routing and send.
    SetTrackRouting {
        /// The track.
        track: TrackId,
        /// Destination bus, or the master.
        bus: Option<u8>,
        /// Send bus.
        send: Option<u8>,
        /// Send level, 0..=127.
        send_level: u8,
    },
    /// Replace a track's audio reference.
    SetAudioRef {
        /// The track.
        track: TrackId,
        /// New reference, or none to remove it.
        reference: Option<project::song::AudioRef>,
    },
    /// Replace a track's active audio reference and stored takes together.
    SetAudioState {
        /// The track.
        track: TrackId,
        /// Active reference, or none.
        reference: Option<project::song::AudioRef>,
        /// Stored takes.
        takes: Vec<project::song::AudioRef>,
    },
    /// Replace a track's comped audio spans.
    SetAudioComps {
        /// The track.
        track: TrackId,
        /// Comped spans.
        comps: Vec<project::song::AudioComp>,
    },
    /// Replace a track's stored audio takes.
    SetAudioTakes {
        /// The track.
        track: TrackId,
        /// Stored takes.
        takes: Vec<project::song::AudioRef>,
    },
    /// Freeze a track: clear its notes and give it an audio reference.
    FreezeTrack {
        /// The track.
        track: TrackId,
        /// Rendered audio.
        reference: project::song::AudioRef,
    },
    /// Restore a frozen track's notes and audio reference.
    RestoreTrack {
        /// The track.
        track: TrackId,
        /// Notes to restore.
        notes: Vec<project::song::Note>,
        /// Audio reference to restore.
        audio: Option<project::song::AudioRef>,
    },
    /// Replace a track's automation lanes.
    SetAutomation {
        /// The track.
        track: TrackId,
        /// New lanes.
        lanes: Vec<project::song::Automation>,
    },
    /// Set a track's fader and pan, each 0..=127.
    SetTrackGain {
        /// The track.
        track: TrackId,
        /// Fader level.
        volume: u8,
        /// Pan position.
        pan: u8,
    },
    /// Choose the keyboard voice (GM program) for a track, or clear it.
    SetTrackProgram {
        /// The track.
        track: TrackId,
        /// Program number, or `None` to let the exporter choose.
        program: Option<u8>,
    },
    /// Choose the bank-select pair for a track's program, or clear it.
    SetTrackBank {
        /// The track.
        track: TrackId,
        /// Bank select, or `None` for the default bank.
        bank: Option<BankSelect>,
    },
    /// Replace the song's chords as one edit.
    SetChords {
        /// The new chord list, sorted by tick.
        chords: Vec<ChordEvent>,
    },
    /// Replace the song's lyric syllables as one edit.
    SetLyrics {
        /// New lyrics.
        lyrics: Vec<project::song::LyricEvent>,
    },
    /// Replace the song's score marks as one edit.
    SetMarks {
        /// New marks.
        marks: Vec<project::song::Mark>,
    },
    /// Replace the clip-grid scene names as one edit.
    SetSceneNames {
        /// Names by scene index.
        names: Vec<String>,
    },
    /// Replace the song's markers as one edit.
    SetMarkers {
        /// The new markers, sorted by tick.
        markers: Vec<project::Marker>,
    },
    /// Replace every clip region of a track.
    SetClips {
        /// The track.
        track: TrackId,
        /// The new clip list; validation rejects overlaps and bad ranges.
        clips: Vec<Clip>,
    },
    /// Replace every note of a track, keeping the track itself.
    ReplaceTrackNotes {
        /// The track.
        track: TrackId,
        /// The new note list, usually from a regeneration.
        notes: Vec<Note>,
    },
    /// Set or clear a track's generation provenance.
    SetProvenance {
        /// The track.
        track: TrackId,
        /// New provenance; `None` freezes or detaches the track.
        provenance: Option<provenance::Provenance>,
    },
    /// Restore a complete tempo map.
    ///
    /// Internal: undo builds it to put back an exact map after a tempo edit.
    /// Callers use [`Command::SetTempo`].
    ReplaceTempoMap {
        /// The map to install.
        map: TempoMap,
    },
}

impl Command {
    /// Short label used in undo descriptions.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::InsertNote { .. } => "insert note",
            Self::RemoveNote { .. } => "remove note",
            Self::SetNoteDuration { .. } => "set note duration",
            Self::SetChord { .. } => "set chord",
            Self::RemoveChord { .. } => "remove chord",
            Self::SetTempo { .. } => "set tempo",
            Self::RenameTrack { .. } => "rename track",
            Self::SetSongName { .. } => "set song name",
            Self::SetTimeSignature { .. } => "set time signature",
            Self::InsertTrack { .. } => "insert track",
            Self::RemoveTrack { .. } => "remove track",
            Self::SetChords { .. } => "set chords",
            Self::SetTrackProgram { .. } => "set track program",
            Self::SetTrackBank { .. } => "set track bank",
            Self::SetTrackFlags { .. } => "set track flags",
            Self::SetTrackGain { .. } => "set track gain",
            Self::SetAutomation { .. } => "set automation",
            Self::SetAudioRef { .. } => "set audio reference",
            Self::SetAudioState { .. } => "set audio state",
            Self::SetAudioComps { .. } => "set audio comps",
            Self::SetAudioTakes { .. } => "set audio takes",
            Self::FreezeTrack { .. } => "freeze track",
            Self::RestoreTrack { .. } => "restore track",
            Self::SetBuses { .. } => "set buses",
            Self::SetMasterPlugins { .. } => "set master plug-ins",
            Self::SetTrackPatch { .. } => "set track patch",
            Self::SetPlugins { .. } => "set plugins",
            Self::SetSoundFont { .. } => "set soundfont",
            Self::SetSlurs { .. } => "set slurs",
            Self::SetTrackRouting { .. } => "set track routing",
            Self::SetMarkers { .. } => "set markers",
            Self::SetSceneNames { .. } => "set scene names",
            Self::SetLyrics { .. } => "set lyrics",
            Self::SetMarks { .. } => "set marks",
            Self::SetClips { .. } => "set clips",
            Self::ReplaceTrackNotes { .. } => "replace track notes",
            Self::SetProvenance { .. } => "set provenance",
            Self::ReplaceTempoMap { .. } => "restore tempo map",
        }
    }
}

/// Errors raised when a command cannot be applied.
#[derive(Debug, Error, PartialEq)]
pub enum CommandError {
    /// No track holds that identifier.
    #[error("no track {0}")]
    UnknownTrack(TrackId),
    /// The track has no note with that identifier.
    #[error("track {track} has no note {note}")]
    UnknownNote {
        /// The track searched.
        track: TrackId,
        /// The note searched.
        note: NoteId,
    },
    /// A note identifier would be duplicated inside a track.
    #[error("track {track} already has note {note}")]
    DuplicateNote {
        /// The track.
        track: TrackId,
        /// The duplicated identifier.
        note: NoteId,
    },
    /// A track identifier would be duplicated in the song.
    #[error("song already has track {0}")]
    DuplicateTrack(TrackId),
    /// No chord sits at that tick.
    #[error("no chord at tick {0}")]
    UnknownChord(Tick),
    /// The tempo map rejected the change.
    #[error("tempo change rejected: {0}")]
    Tempo(String),
    /// The command would leave the song invalid.
    #[error("command would make the project invalid: {0}")]
    Validation(String),
    /// There is nothing to undo.
    #[error("nothing to undo")]
    NothingToUndo,
    /// There is nothing to redo.
    #[error("nothing to redo")]
    NothingToRedo,
}

/// What an apply or undo produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutcome {
    /// Human-readable description, for logs and the UI.
    pub description: String,
    /// Number of steps on the undo stack after the call.
    pub undo_depth: usize,
    /// Number of steps on the redo stack after the call.
    pub redo_depth: usize,
}

/// An applied command and the command that reverses it.
#[derive(Clone, Debug, PartialEq)]
struct UndoEntry {
    inverse: Command,
    description: String,
}

/// An open project plus its undo and redo history.
#[derive(Debug)]
pub struct EditSession {
    project: ProjectFile,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
}

impl EditSession {
    /// Opens a session on a project. The project is validated first.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError`] when the document is invalid.
    pub fn new(project: ProjectFile) -> Result<Self, ProjectError> {
        project.check()?;
        Ok(Self {
            project,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    /// The current project.
    #[must_use]
    pub fn project(&self) -> &ProjectFile {
        &self.project
    }

    /// The song being edited.
    #[must_use]
    pub fn song(&self) -> &Song {
        &self.project.song
    }

    /// Number of undo steps available.
    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Number of redo steps available.
    #[must_use]
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Applies one command, or leaves the song untouched.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] for an unknown target, a duplicate identifier,
    /// a rejected tempo, or a result that fails project validation.
    pub fn apply(&mut self, command: Command) -> Result<CommandOutcome, CommandError> {
        let inverse = self.inverse_for(&command)?;
        let description = describe(&command);
        let before = self.project.clone();
        if let Err(err) = self.run(&command) {
            self.project = before;
            return Err(err);
        }
        if let Err(err) = self.project.check() {
            self.project = before;
            return Err(CommandError::Validation(err.to_string()));
        }
        self.undo.push(UndoEntry {
            inverse,
            description: description.clone(),
        });
        self.redo.clear();
        Ok(self.outcome(description))
    }

    /// Reverses the most recent command.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError::NothingToUndo`] when the history is empty.
    pub fn undo(&mut self) -> Result<CommandOutcome, CommandError> {
        let Some(entry) = self.undo.pop() else {
            return Err(CommandError::NothingToUndo);
        };
        let forward = self.inverse_for(&entry.inverse)?;
        let before = self.project.clone();
        if let Err(err) = self.run(&entry.inverse) {
            self.project = before;
            self.undo.push(entry);
            return Err(err);
        }
        let description = format!("undo {}", entry.description);
        self.redo.push(UndoEntry {
            inverse: forward,
            description: entry.description,
        });
        Ok(self.outcome(description))
    }

    /// Re-applies the most recently undone command.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError::NothingToRedo`] when the history is empty.
    pub fn redo(&mut self) -> Result<CommandOutcome, CommandError> {
        let Some(entry) = self.redo.pop() else {
            return Err(CommandError::NothingToRedo);
        };
        let reverse = self.inverse_for(&entry.inverse)?;
        let before = self.project.clone();
        if let Err(err) = self.run(&entry.inverse) {
            self.project = before;
            self.redo.push(entry);
            return Err(err);
        }
        let description = format!("redo {}", entry.description);
        self.undo.push(UndoEntry {
            inverse: reverse,
            description: entry.description,
        });
        Ok(self.outcome(description))
    }

    /// Allocates the next free note identifier on a track.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError::UnknownTrack`] for an unknown track.
    pub fn next_note_id(&self, track: TrackId) -> Result<NoteId, CommandError> {
        let track = self.track(track)?;
        let highest = track
            .notes
            .iter()
            .map(|note| note.id.get())
            .max()
            .unwrap_or(0);
        NoteId::new(highest.saturating_add(1))
            .map_err(|err| CommandError::Validation(err.to_string()))
    }

    /// Adds a clip region to a track.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the track is unknown or the result is invalid.
    pub fn insert_clip(
        &mut self,
        track: TrackId,
        clip: Clip,
    ) -> Result<CommandOutcome, CommandError> {
        let mut clips = self.track(track)?.clips.clone();
        clips.push(clip);
        clips.sort_by_key(|clip| clip.start);
        self.apply(Command::SetClips { track, clips })
    }

    /// Removes a clip region by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the track or clip is unknown.
    pub fn remove_clip(
        &mut self,
        track: TrackId,
        clip: project::ClipId,
    ) -> Result<CommandOutcome, CommandError> {
        let mut clips = self.track(track)?.clips.clone();
        let before = clips.len();
        clips.retain(|candidate| candidate.id != clip);
        if clips.len() == before {
            return Err(CommandError::Validation(format!("unknown clip {clip}")));
        }
        self.apply(Command::SetClips { track, clips })
    }

    /// Moves a clip region to a new start tick.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the track or clip is unknown or the result overlaps.
    pub fn move_clip(
        &mut self,
        track: TrackId,
        clip: project::ClipId,
        start: Tick,
    ) -> Result<CommandOutcome, CommandError> {
        let mut clips = self.track(track)?.clips.clone();
        let Some(found) = clips.iter_mut().find(|candidate| candidate.id == clip) else {
            return Err(CommandError::Validation(format!("unknown clip {clip}")));
        };
        found.start = start;
        clips.sort_by_key(|clip| clip.start);
        self.apply(Command::SetClips { track, clips })
    }

    /// Changes a clip region's length.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the track or clip is unknown or the length is not positive.
    pub fn resize_clip(
        &mut self,
        track: TrackId,
        clip: project::ClipId,
        length: Tick,
    ) -> Result<CommandOutcome, CommandError> {
        if length <= 0 {
            return Err(CommandError::Validation(String::from(
                "a clip length must be positive",
            )));
        }
        let mut clips = self.track(track)?.clips.clone();
        let Some(found) = clips.iter_mut().find(|candidate| candidate.id == clip) else {
            return Err(CommandError::Validation(format!("unknown clip {clip}")));
        };
        found.length = length;
        self.apply(Command::SetClips { track, clips })
    }

    /// Splits the clip containing `tick` into two.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the track or clip is unknown or the split is empty.
    pub fn split_clip(
        &mut self,
        track: TrackId,
        tick: Tick,
        new_id: project::ClipId,
    ) -> Result<CommandOutcome, CommandError> {
        let mut clips = self.track(track)?.clips.clone();
        let Some(index) = clips.iter().position(|clip| clip.contains(tick)) else {
            return Err(CommandError::Validation(String::from(
                "no clip contains that tick",
            )));
        };
        let original = clips[index].clone();
        if tick == original.start {
            return Err(CommandError::Validation(String::from("the split is empty")));
        }
        clips[index].length = tick - original.start;
        let right_length = original.end() - tick;
        let mut right = original;
        right.id = new_id;
        right.start = tick;
        right.length = right_length;
        if right.length <= 0 {
            return Err(CommandError::Validation(String::from("the split is empty")));
        }
        clips.insert(index + 1, right);
        self.apply(Command::SetClips { track, clips })
    }

    /// Joins two clips of a track into one spanning both.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when either clip is unknown or both are the same.
    pub fn join_clips(
        &mut self,
        track: TrackId,
        first: project::ClipId,
        second: project::ClipId,
    ) -> Result<CommandOutcome, CommandError> {
        if first == second {
            return Err(CommandError::Validation(String::from(
                "a clip cannot be joined with itself",
            )));
        }
        let mut clips = self.track(track)?.clips.clone();
        let Some(a) = clips.iter().position(|clip| clip.id == first) else {
            return Err(CommandError::Validation(format!("unknown clip {first}")));
        };
        let Some(b) = clips.iter().position(|clip| clip.id == second) else {
            return Err(CommandError::Validation(format!("unknown clip {second}")));
        };
        let (left, right) = if clips[a].start <= clips[b].start {
            (a, b)
        } else {
            (b, a)
        };
        let end = clips[right].end().max(clips[left].end());
        clips[left].length = end - clips[left].start;
        clips.remove(right);
        self.apply(Command::SetClips { track, clips })
    }

    /// Allocates the next free track identifier.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError::Validation`] when the identifier space is full.
    pub fn next_track_id(&self) -> Result<TrackId, CommandError> {
        let highest = self
            .song()
            .tracks
            .iter()
            .map(|track| track.id.get())
            .max()
            .unwrap_or(0);
        TrackId::new(highest.saturating_add(1))
            .map_err(|err| CommandError::Validation(err.to_string()))
    }

    /// Convenience: insert a note, allocating its identifier.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] as [`EditSession::apply`] does.
    pub fn add_note(
        &mut self,
        track: TrackId,
        start: Tick,
        duration: Tick,
        pitch: Pitch,
        velocity: Velocity,
        channel: Channel,
    ) -> Result<CommandOutcome, CommandError> {
        let note = Note {
            id: self.next_note_id(track)?,
            start,
            duration,
            pitch,
            velocity,
            channel,
        };
        self.apply(Command::InsertNote { track, note })
    }

    /// Convenience: insert an empty named track, allocating its identifier.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] as [`EditSession::apply`] does.
    pub fn add_track(&mut self, name: impl Into<String>) -> Result<CommandOutcome, CommandError> {
        let track = Track::new(self.next_track_id()?, name);
        self.apply(Command::InsertTrack { track })
    }

    fn outcome(&self, description: String) -> CommandOutcome {
        CommandOutcome {
            description,
            undo_depth: self.undo.len(),
            redo_depth: self.redo.len(),
        }
    }

    fn track(&self, id: TrackId) -> Result<&Track, CommandError> {
        self.song()
            .tracks
            .iter()
            .find(|track| track.id == id)
            .ok_or(CommandError::UnknownTrack(id))
    }

    fn track_mut(&mut self, id: TrackId) -> Result<&mut Track, CommandError> {
        self.project
            .song
            .tracks
            .iter_mut()
            .find(|track| track.id == id)
            .ok_or(CommandError::UnknownTrack(id))
    }

    /// Computes the command that reverses `command`, before applying it.
    fn inverse_for(&self, command: &Command) -> Result<Command, CommandError> {
        Ok(match command {
            Command::InsertNote { track, note } => {
                let target = self.track(*track)?;
                if target.notes.iter().any(|existing| existing.id == note.id) {
                    return Err(CommandError::DuplicateNote {
                        track: *track,
                        note: note.id,
                    });
                }
                Command::RemoveNote {
                    track: *track,
                    note: note.id,
                }
            }
            Command::SetNoteDuration { track, note, .. } => Command::SetNoteDuration {
                track: *track,
                note: *note,
                duration: self
                    .track(*track)?
                    .notes
                    .iter()
                    .find(|entry| entry.id == *note)
                    .ok_or(CommandError::UnknownNote {
                        track: *track,
                        note: *note,
                    })?
                    .duration,
            },
            Command::RemoveNote { track, note } => {
                let target = self.track(*track)?;
                let found = target
                    .notes
                    .iter()
                    .find(|existing| existing.id == *note)
                    .cloned()
                    .ok_or(CommandError::UnknownNote {
                        track: *track,
                        note: *note,
                    })?;
                Command::InsertNote {
                    track: *track,
                    note: found,
                }
            }
            Command::SetChord { tick, .. } => match self.chord_at(*tick) {
                Some(chord) => Command::SetChord {
                    tick: *tick,
                    chord: *chord,
                },
                None => Command::RemoveChord { tick: *tick },
            },
            Command::RemoveChord { tick } => {
                let chord = self
                    .chord_at(*tick)
                    .ok_or(CommandError::UnknownChord(*tick))?;
                Command::SetChord {
                    tick: *tick,
                    chord: *chord,
                }
            }
            Command::SetTempo { bpm, .. } => {
                if !bpm.is_finite() || *bpm <= 0.0 {
                    return Err(CommandError::Tempo(format!("{bpm} bpm is not usable")));
                }
                Command::ReplaceTempoMap {
                    map: self.song().tempo_map.clone(),
                }
            }
            Command::SetSlurs { track, .. } => Command::SetSlurs {
                track: *track,
                slurs: self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == *track)
                    .map(|entry| entry.slurs.clone())
                    .unwrap_or_default(),
            },
            Command::SetSoundFont { track, .. } => Command::SetSoundFont {
                track: *track,
                soundfont: self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == *track)
                    .and_then(|entry| entry.soundfont.clone()),
            },
            Command::SetPlugins { track, .. } => Command::SetPlugins {
                track: *track,
                plugins: self.track(*track)?.plugins.clone(),
            },
            Command::SetTrackPatch { track, .. } => Command::SetTrackPatch {
                track: *track,
                patch: self.track(*track)?.patch.map(Box::new),
            },
            Command::SetBuses { .. } => Command::SetBuses {
                buses: self.song().buses.clone(),
            },
            Command::SetMasterPlugins { .. } => Command::SetMasterPlugins {
                plugins: self.song().master_plugins.clone(),
            },
            Command::SetTrackRouting { track, .. } => {
                let found = self.track(*track)?;
                Command::SetTrackRouting {
                    track: *track,
                    bus: found.bus,
                    send: found.send,
                    send_level: found.send_level,
                }
            }
            Command::FreezeTrack { track, .. } => Command::RestoreTrack {
                track: *track,
                notes: self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == *track)
                    .map(|entry| entry.notes.clone())
                    .unwrap_or_default(),
                audio: self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == *track)
                    .and_then(|entry| entry.audio.clone()),
            },
            Command::RestoreTrack { track, .. } => Command::RestoreTrack {
                track: *track,
                notes: self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == *track)
                    .map(|entry| entry.notes.clone())
                    .unwrap_or_default(),
                audio: self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == *track)
                    .and_then(|entry| entry.audio.clone()),
            },
            Command::SetAudioRef { track, .. } => Command::SetAudioRef {
                track: *track,
                reference: self.track(*track)?.audio.clone(),
            },
            Command::SetAudioState { track, .. } => Command::SetAudioState {
                track: *track,
                reference: self.track(*track)?.audio.clone(),
                takes: self.track(*track)?.takes.clone(),
            },
            Command::SetAudioComps { track, .. } => Command::SetAudioComps {
                track: *track,
                comps: self.track(*track)?.comps.clone(),
            },
            Command::SetAudioTakes { track, .. } => Command::SetAudioTakes {
                track: *track,
                takes: self.track(*track)?.takes.clone(),
            },
            Command::SetAutomation { track, .. } => Command::SetAutomation {
                track: *track,
                lanes: self.track(*track)?.automation.clone(),
            },
            Command::SetTrackGain { track, .. } => {
                let found = self.track(*track)?;
                Command::SetTrackGain {
                    track: *track,
                    volume: found.volume,
                    pan: found.pan,
                }
            }
            Command::SetTrackFlags { track, .. } => {
                let found = self.track(*track)?;
                Command::SetTrackFlags {
                    track: *track,
                    muted: found.muted,
                    soloed: found.soloed,
                    armed: found.armed,
                }
            }
            Command::SetTrackProgram { track, .. } => Command::SetTrackProgram {
                track: *track,
                program: self.track(*track)?.program,
            },
            Command::SetTrackBank { track, .. } => Command::SetTrackBank {
                track: *track,
                bank: self.track(*track)?.bank,
            },
            Command::SetChords { .. } => Command::SetChords {
                chords: self.song().chords.clone(),
            },
            Command::SetSceneNames { .. } => Command::SetSceneNames {
                names: self.song().scene_names.clone(),
            },
            Command::SetLyrics { .. } => Command::SetLyrics {
                lyrics: self.song().lyrics.clone(),
            },
            Command::SetMarks { .. } => Command::SetMarks {
                marks: self.song().marks.clone(),
            },
            Command::SetMarkers { .. } => Command::SetMarkers {
                markers: self.song().markers.clone(),
            },
            Command::SetClips { track, .. } => Command::SetClips {
                track: *track,
                clips: self.track(*track)?.clips.clone(),
            },
            Command::ReplaceTrackNotes { track, .. } => Command::ReplaceTrackNotes {
                track: *track,
                notes: self.track(*track)?.notes.clone(),
            },
            Command::SetProvenance { track, .. } => Command::SetProvenance {
                track: *track,
                provenance: self.track(*track)?.provenance.clone(),
            },
            Command::ReplaceTempoMap { .. } => Command::ReplaceTempoMap {
                map: self.song().tempo_map.clone(),
            },
            Command::RenameTrack { track, .. } => Command::RenameTrack {
                track: *track,
                name: self.track(*track)?.name.clone(),
            },
            Command::SetSongName { .. } => Command::SetSongName {
                name: self.song().name.clone(),
            },
            Command::SetTimeSignature { .. } => Command::SetTimeSignature {
                time_signature: self.song().time_signature,
            },
            Command::InsertTrack { track } => {
                if self
                    .song()
                    .tracks
                    .iter()
                    .any(|existing| existing.id == track.id)
                {
                    return Err(CommandError::DuplicateTrack(track.id));
                }
                Command::RemoveTrack { track: track.id }
            }
            Command::RemoveTrack { track } => Command::InsertTrack {
                track: self.track(*track)?.clone(),
            },
        })
    }

    fn chord_at(&self, tick: Tick) -> Option<&Chord> {
        self.song()
            .chords
            .iter()
            .find(|event| event.tick == tick)
            .map(|event| &event.chord)
    }

    fn run(&mut self, command: &Command) -> Result<(), CommandError> {
        match command {
            Command::InsertNote { track, note } => {
                let target = self.track_mut(*track)?;
                let position = target
                    .notes
                    .iter()
                    .position(|existing| existing.start > note.start)
                    .unwrap_or(target.notes.len());
                target.notes.insert(position, note.clone());
            }
            Command::SetNoteDuration {
                track,
                note,
                duration,
            } => {
                if *duration <= 0 {
                    return Err(CommandError::Validation(String::from(
                        "note duration must be positive",
                    )));
                }
                let entry = self
                    .track_mut(*track)?
                    .notes
                    .iter_mut()
                    .find(|entry| entry.id == *note)
                    .ok_or(CommandError::UnknownNote {
                        track: *track,
                        note: *note,
                    })?;
                entry.duration = *duration;
            }
            Command::RemoveNote { track, note } => {
                let target = self.track_mut(*track)?;
                target.notes.retain(|existing| existing.id != *note);
            }
            Command::SetChord { tick, chord } => {
                if *tick < 0 {
                    return Err(CommandError::Validation(format!(
                        "chord tick {tick} is negative"
                    )));
                }
                let chords = &mut self.project.song.chords;
                match chords.iter_mut().find(|event| event.tick == *tick) {
                    Some(event) => event.chord = *chord,
                    None => {
                        let position = chords
                            .iter()
                            .position(|event| event.tick > *tick)
                            .unwrap_or(chords.len());
                        chords.insert(
                            position,
                            ChordEvent {
                                tick: *tick,
                                chord: *chord,
                            },
                        );
                    }
                }
            }
            Command::RemoveChord { tick } => {
                self.project.song.chords.retain(|event| event.tick != *tick);
            }
            Command::SetTempo { tick, bpm } => {
                let map = self.song().tempo_map.clone();
                let next = if *tick == 0 {
                    if map.changes().len() == 1 {
                        TempoMap::constant(*bpm)
                    } else {
                        let mut changes = map.changes().to_vec();
                        changes[0].bpm = *bpm;
                        TempoMap::new(changes)
                            .map_err(|err| CommandError::Tempo(err.to_string()))?
                    }
                } else {
                    map.with_change(*tick, *bpm)
                        .map_err(|err| CommandError::Tempo(err.to_string()))?
                };
                self.project.song.tempo_map = next;
            }
            Command::SetAutomation { track, lanes } => {
                self.track_mut(*track)?.automation = lanes.clone();
            }
            Command::FreezeTrack { track, reference } => {
                let entry = self.track_mut(*track)?;
                entry.notes.clear();
                entry.audio = Some(reference.clone());
            }
            Command::RestoreTrack {
                track,
                notes,
                audio,
            } => {
                let entry = self.track_mut(*track)?;
                entry.notes = notes.clone();
                entry.audio = audio.clone();
            }
            Command::SetAudioRef { track, reference } => {
                self.track_mut(*track)?.audio = reference.clone();
            }
            Command::SetAudioState {
                track,
                reference,
                takes,
            } => {
                let track = self.track_mut(*track)?;
                track.audio = reference.clone();
                track.takes = takes.clone();
            }
            Command::SetAudioComps { track, comps } => {
                self.track_mut(*track)?.comps = comps.clone();
            }
            Command::SetAudioTakes { track, takes } => {
                self.track_mut(*track)?.takes = takes.clone();
            }
            Command::SetBuses { buses } => {
                self.project.song.buses = buses.clone();
            }
            Command::SetTrackPatch { track, patch } => {
                self.track_mut(*track)?.patch = patch.as_deref().copied();
            }
            Command::SetSlurs { track, slurs } => {
                self.track_mut(*track)?.slurs = slurs.clone();
            }
            Command::SetSoundFont { track, soundfont } => {
                self.track_mut(*track)?.soundfont = soundfont.clone();
            }
            Command::SetPlugins { track, plugins } => {
                self.track_mut(*track)?.plugins = plugins.clone();
            }
            Command::SetMasterPlugins { plugins } => {
                self.project.song.master_plugins = plugins.clone();
            }
            Command::SetTrackRouting {
                track,
                bus,
                send,
                send_level,
            } => {
                let target = self.track_mut(*track)?;
                target.bus = *bus;
                target.send = *send;
                target.send_level = (*send_level).min(127);
            }
            Command::SetTrackGain { track, volume, pan } => {
                let target = self.track_mut(*track)?;
                target.volume = (*volume).min(127);
                target.pan = (*pan).min(127);
            }
            Command::SetTrackFlags {
                track,
                muted,
                soloed,
                armed,
            } => {
                let target = self.track_mut(*track)?;
                target.muted = *muted;
                target.soloed = *soloed;
                target.armed = *armed;
            }
            Command::SetTrackProgram { track, program } => {
                self.track_mut(*track)?.program = *program;
            }
            Command::SetTrackBank { track, bank } => {
                self.track_mut(*track)?.bank = *bank;
            }
            Command::SetChords { chords } => {
                self.project.song.chords = chords.clone();
                self.project.song.chords.sort_by_key(|event| event.tick);
            }
            Command::SetSceneNames { names } => {
                self.project.song.scene_names = names.clone();
            }
            Command::SetLyrics { lyrics } => {
                self.project.song.lyrics = lyrics.clone();
            }
            Command::SetMarks { marks } => {
                self.project.song.marks = marks.clone();
            }
            Command::SetMarkers { markers } => {
                self.project.song.markers = markers.clone();
                self.project.song.markers.sort_by_key(|marker| marker.tick);
            }
            Command::SetClips { track, clips } => {
                self.track_mut(*track)?.clips = clips.clone();
            }
            Command::ReplaceTrackNotes { track, notes } => {
                self.track_mut(*track)?.notes = notes.clone();
            }
            Command::SetProvenance { track, provenance } => {
                self.track_mut(*track)?.provenance = provenance.clone();
            }
            Command::ReplaceTempoMap { map } => {
                self.project.song.tempo_map = map.clone();
            }
            Command::RenameTrack { track, name } => {
                self.track_mut(*track)?.name = name.clone();
            }
            Command::SetSongName { name } => {
                self.project.song.name = name.clone();
            }
            Command::SetTimeSignature { time_signature } => {
                music_core::TimeSignature::new(
                    time_signature.numerator,
                    time_signature.denominator,
                )
                .map_err(|error| CommandError::Validation(error.to_string()))?;
                self.project.song.time_signature = *time_signature;
            }
            Command::InsertTrack { track } => {
                self.project.song.tracks.push(track.clone());
            }
            Command::RemoveTrack { track } => {
                self.project
                    .song
                    .tracks
                    .retain(|existing| existing.id != *track);
            }
        }
        Ok(())
    }
}

fn describe(command: &Command) -> String {
    match command {
        Command::InsertNote { track, note } => {
            format!("insert {} on track {track} at {}", note.pitch, note.start)
        }
        Command::RemoveNote { track, note } => format!("remove note {note} from track {track}"),
        Command::SetNoteDuration {
            track,
            note,
            duration,
        } => format!("set note {note} on track {track} to {duration} ticks"),
        Command::SetChord { tick, chord } => format!("set chord {chord} at tick {tick}"),
        Command::RemoveChord { tick } => format!("remove chord at tick {tick}"),
        Command::SetTempo { tick, bpm } => format!("set tempo {bpm} bpm at tick {tick}"),
        Command::RenameTrack { track, name } => format!("rename track {track} to {name:?}"),
        Command::SetSongName { name } => format!("rename song to {name:?}"),
        Command::SetTimeSignature { time_signature } => format!(
            "set meter {}/{}",
            time_signature.numerator, time_signature.denominator
        ),
        Command::InsertTrack { track } => format!("insert track {} ({})", track.id, track.name),
        Command::RemoveTrack { track } => format!("remove track {track}"),
        Command::SetChords { chords } => format!("set {} chord(s)", chords.len()),
        Command::SetAutomation { track, lanes } => {
            format!("track {track}: {} automation lane(s)", lanes.len())
        }
        Command::SetBuses { buses } => format!("set {} bus(es)", buses.len()),
        Command::SetMasterPlugins { plugins } => {
            format!("set {} master plug-in(s)", plugins.len())
        }
        Command::SetPlugins { track, plugins } => {
            format!("track {track}: {} plugin slot(s)", plugins.len())
        }
        Command::SetSlurs { track, slurs } => {
            format!("track {track}: {} slur(s)", slurs.len())
        }
        Command::SetSoundFont { track, soundfont } => match soundfont {
            Some(font) => format!(
                "track {track}: soundfont {}:{} {}",
                font.bank, font.program, font.path
            ),
            None => format!("track {track}: clear soundfont"),
        },
        Command::SetTrackPatch { track, patch } => match patch {
            Some(patch) => format!("track {track}: {:?} patch", patch.waveform),
            None => format!("track {track}: clear patch"),
        },
        Command::SetTrackRouting {
            track,
            bus,
            send,
            send_level,
        } => format!("track {track}: bus {bus:?}, send {send:?} at {send_level}"),
        Command::FreezeTrack { reference, .. } => {
            format!("freeze track to {}", reference.path)
        }
        Command::RestoreTrack { track, .. } => format!("restore track {track}"),
        Command::SetAudioState { track, takes, .. } => {
            format!("track {track}: audio state with {} take(s)", takes.len())
        }
        Command::SetAudioComps { track, comps } => {
            format!("track {track}: {} comp span(s)", comps.len())
        }
        Command::SetAudioTakes { track, takes } => {
            format!("track {track}: {} audio take(s)", takes.len())
        }
        Command::SetAudioRef { track, reference } => match reference {
            Some(reference) => format!(
                "track {track}: audio {} from {} to {}",
                reference.path, reference.start, reference.length
            ),
            None => format!("track {track}: remove audio reference"),
        },
        Command::SetTrackGain { track, volume, pan } => {
            format!("track {track}: volume {volume}, pan {pan}")
        }
        Command::SetTrackFlags {
            track,
            muted,
            soloed,
            armed,
        } => format!("track {track}: muted {muted}, soloed {soloed}, armed {armed}"),
        Command::SetTrackProgram { track, program } => match program {
            Some(value) => format!("set track {track} program {value}"),
            None => format!("clear track {track} program"),
        },
        Command::SetTrackBank { track, bank } => match bank {
            Some(bank) => format!("set track {track} bank {}/{}", bank.msb, bank.lsb),
            None => format!("clear track {track} bank"),
        },
        Command::SetMarkers { markers } => format!("set {} marker(s)", markers.len()),
        Command::SetSceneNames { names } => format!("set {} scene name(s)", names.len()),
        Command::SetLyrics { lyrics } => format!("set {} lyric(s)", lyrics.len()),
        Command::SetMarks { marks } => format!("set {} mark(s)", marks.len()),
        Command::SetClips { track, clips } => {
            format!("set {} clip(s) on track {track}", clips.len())
        }
        Command::ReplaceTrackNotes { track, notes } => {
            format!("replace {} note(s) on track {track}", notes.len())
        }
        Command::SetProvenance { track, provenance } => {
            if provenance.is_some() {
                format!("set provenance on track {track}")
            } else {
                format!("clear provenance on track {track}")
            }
        }
        Command::ReplaceTempoMap { .. } => String::from("restore tempo map"),
    }
}

/// Checks that a project has no validation errors. Warnings are allowed.
///
/// # Errors
///
/// Returns the first error as text.
pub fn check_project(project: &ProjectFile) -> Result<(), String> {
    let issues = project.validate();
    match issues
        .iter()
        .find(|issue| issue.severity == Severity::Error)
    {
        Some(issue) => Err(issue.to_string()),
        None => Ok(()),
    }
}

impl Command {
    /// True when the command is an internal history step rather than an edit
    /// a caller should build.
    #[must_use]
    pub fn is_internal(&self) -> bool {
        matches!(self, Self::ReplaceTempoMap { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> EditSession {
        let mut song = Song::new("Commands", 120.0);
        song.tracks
            .push(Track::new(TrackId::new(1).expect("valid"), "Piano"));
        EditSession::new(ProjectFile::new(song)).expect("valid project")
    }

    fn pitch(name: &str) -> Pitch {
        Pitch::from_name(name).expect("valid pitch")
    }

    #[test]
    fn add_note_then_undo_then_redo() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.add_note(
            track,
            0,
            960,
            pitch("C4"),
            Velocity::new(100).expect("valid"),
            Channel::first(),
        )
        .expect("apply");
        assert_eq!(edit.song().tracks[0].notes.len(), 1);
        assert_eq!(edit.undo_depth(), 1);
        assert_eq!(edit.redo_depth(), 0);

        edit.undo().expect("undo");
        assert_eq!(edit.song().tracks[0].notes.len(), 0);
        assert_eq!(edit.undo_depth(), 0);
        assert_eq!(edit.redo_depth(), 1);

        edit.redo().expect("redo");
        assert_eq!(edit.song().tracks[0].notes.len(), 1);
        assert_eq!(edit.undo_depth(), 1);
        assert_eq!(edit.redo_depth(), 0);
    }

    #[test]
    fn new_command_clears_the_redo_stack() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.add_note(
            track,
            0,
            960,
            pitch("C4"),
            Velocity::new(100).expect("valid"),
            Channel::first(),
        )
        .expect("apply");
        edit.undo().expect("undo");
        assert_eq!(edit.redo_depth(), 1);
        edit.add_note(
            track,
            0,
            960,
            pitch("E4"),
            Velocity::new(100).expect("valid"),
            Channel::first(),
        )
        .expect("apply");
        assert_eq!(edit.redo_depth(), 0);
    }

    #[test]
    fn undo_and_redo_report_empty_history() {
        let mut edit = session();
        assert_eq!(edit.undo().expect_err("empty"), CommandError::NothingToUndo);
        assert_eq!(edit.redo().expect_err("empty"), CommandError::NothingToRedo);
    }

    #[test]
    fn unknown_targets_are_rejected_without_mutation() {
        let mut edit = session();
        let ghost_track = TrackId::new(99).expect("valid");
        let error = edit
            .apply(Command::RemoveNote {
                track: ghost_track,
                note: NoteId::new(1).expect("valid"),
            })
            .expect_err("unknown track");
        assert_eq!(error, CommandError::UnknownTrack(ghost_track));

        let track = TrackId::new(1).expect("valid");
        let error = edit.add_note(
            ghost_track,
            0,
            960,
            pitch("C4"),
            Velocity::new(100).expect("valid"),
            Channel::first(),
        );
        assert_eq!(
            error.expect_err("unknown track"),
            CommandError::UnknownTrack(ghost_track)
        );
        assert_eq!(track.get(), 1);
        assert_eq!(edit.undo_depth(), 0);
    }

    #[test]
    fn invalid_note_is_rolled_back() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        let note = Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 0,
            pitch: pitch("C4"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        };
        let error = edit
            .apply(Command::InsertNote { track, note })
            .expect_err("zero length note");
        assert!(matches!(error, CommandError::Validation(_)), "{error}");
        assert_eq!(edit.song().tracks[0].notes.len(), 0);
        assert_eq!(edit.undo_depth(), 0);
    }

    #[test]
    fn chords_are_inserted_in_tick_order() {
        let mut edit = session();
        edit.apply(Command::SetChord {
            tick: 3840,
            chord: Chord::parse("G7").expect("valid"),
        })
        .expect("apply");
        edit.apply(Command::SetChord {
            tick: 0,
            chord: Chord::parse("Cmaj7").expect("valid"),
        })
        .expect("apply");
        assert_eq!(edit.song().chords.len(), 2);
        assert_eq!(edit.song().chords[0].tick, 0);
        assert_eq!(edit.song().chords[1].tick, 3840);
    }

    #[test]
    fn replacing_and_removing_chords_undo_exactly() {
        let mut edit = session();
        let tick = 960;
        edit.apply(Command::SetChord {
            tick,
            chord: Chord::parse("C").expect("valid"),
        })
        .expect("apply");
        edit.apply(Command::SetChord {
            tick,
            chord: Chord::parse("Am").expect("valid"),
        })
        .expect("apply");
        assert_eq!(edit.song().chords.len(), 1);
        assert_eq!(edit.song().chords[0].chord.to_string(), "Am");

        edit.undo().expect("undo replace");
        assert_eq!(edit.song().chords[0].chord.to_string(), "C");

        edit.apply(Command::RemoveChord { tick }).expect("remove");
        assert!(edit.song().chords.is_empty());
        edit.undo().expect("undo remove");
        assert_eq!(edit.song().chords[0].chord.to_string(), "C");

        assert_eq!(
            edit.apply(Command::RemoveChord { tick: 99 })
                .expect_err("missing chord"),
            CommandError::UnknownChord(99)
        );
    }

    #[test]
    fn tempo_edits_undo_to_the_exact_map() {
        let mut edit = session();
        let original = edit.song().tempo_map.clone();
        edit.apply(Command::SetTempo { tick: 0, bpm: 90.0 })
            .expect("apply");
        assert_eq!(edit.song().tempo_map.bpm_at(0), 90.0);
        edit.apply(Command::SetTempo {
            tick: 3840,
            bpm: 60.0,
        })
        .expect("apply");
        assert_eq!(edit.song().tempo_map.changes().len(), 2);
        edit.undo().expect("undo second tempo");
        assert_eq!(edit.song().tempo_map.changes().len(), 1);
        edit.undo().expect("undo first tempo");
        assert_eq!(edit.song().tempo_map, original);
    }

    #[test]
    fn invalid_tempo_is_rejected() {
        let mut edit = session();
        for bpm in [0.0, -10.0, f64::NAN] {
            let error = edit
                .apply(Command::SetTempo { tick: 0, bpm })
                .expect_err("bad tempo");
            assert!(matches!(error, CommandError::Tempo(_)), "{error}");
        }
        assert_eq!(edit.song().tempo_map.bpm_at(0), 120.0);
    }

    #[test]
    fn track_rename_and_removal_round_trip() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.apply(Command::RenameTrack {
            track,
            name: String::from("Keys"),
        })
        .expect("rename");
        assert_eq!(edit.song().tracks[0].name, "Keys");
        edit.undo().expect("undo rename");
        assert_eq!(edit.song().tracks[0].name, "Piano");

        edit.apply(Command::RemoveTrack { track }).expect("remove");
        assert!(edit.song().tracks.is_empty());
        edit.undo().expect("undo remove");
        assert_eq!(edit.song().tracks.len(), 1);
        assert_eq!(edit.song().tracks[0].name, "Piano");
    }

    #[test]
    fn identifiers_are_allocated_above_the_highest_in_use() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.add_note(
            track,
            0,
            960,
            pitch("C4"),
            Velocity::new(100).expect("valid"),
            Channel::first(),
        )
        .expect("first");
        edit.add_note(
            track,
            960,
            960,
            pitch("D4"),
            Velocity::new(100).expect("valid"),
            Channel::first(),
        )
        .expect("second");
        assert_eq!(edit.song().tracks[0].notes[1].id.get(), 2);
        assert_eq!(edit.next_track_id().expect("valid").get(), 2);
    }

    #[test]
    fn duplicate_identifiers_are_rejected() {
        let mut edit = session();
        let track_id = TrackId::new(1).expect("valid");
        let note = Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 960,
            pitch: pitch("C4"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        };
        edit.apply(Command::InsertNote {
            track: track_id,
            note: note.clone(),
        })
        .expect("first");
        let error = edit
            .apply(Command::InsertNote {
                track: track_id,
                note,
            })
            .expect_err("duplicate");
        assert_eq!(
            error,
            CommandError::DuplicateNote {
                track: track_id,
                note: NoteId::new(1).expect("valid")
            }
        );
        assert_eq!(edit.song().tracks[0].notes.len(), 1);
    }

    fn clip(id: u32, start: Tick, length: Tick) -> Clip {
        Clip {
            id: project::ClipId::new(id).expect("valid"),
            name: format!("Clip {id}"),
            start,
            length,
        }
    }

    #[test]
    fn clip_commands_round_trip_through_undo() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.insert_clip(track, clip(1, 0, 1920)).expect("insert");
        assert_eq!(edit.song().tracks[0].clips.len(), 1);

        edit.move_clip(track, project::ClipId::new(1).expect("valid"), 960)
            .expect("move");
        assert_eq!(edit.song().tracks[0].clips[0].start, 960);

        edit.resize_clip(track, project::ClipId::new(1).expect("valid"), 480)
            .expect("resize");
        assert_eq!(edit.song().tracks[0].clips[0].length, 480);

        edit.split_clip(track, 1200, project::ClipId::new(2).expect("valid"))
            .expect("split");
        assert_eq!(edit.song().tracks[0].clips.len(), 2);
        assert_eq!(edit.song().tracks[0].clips[0].length, 240);
        assert_eq!(edit.song().tracks[0].clips[1].start, 1200);
        assert_eq!(edit.song().tracks[0].clips[1].length, 240);

        edit.remove_clip(track, project::ClipId::new(2).expect("valid"))
            .expect("remove");
        assert_eq!(edit.song().tracks[0].clips.len(), 1);

        for _ in 0..5 {
            edit.undo().expect("undo");
        }
        assert!(edit.song().tracks[0].clips.is_empty());
    }

    #[test]
    fn joining_clips_spans_both_and_undoes() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.insert_clip(track, clip(1, 0, 960)).expect("first");
        edit.insert_clip(track, clip(2, 960, 1920)).expect("second");
        edit.join_clips(
            track,
            project::ClipId::new(2).expect("valid"),
            project::ClipId::new(1).expect("valid"),
        )
        .expect("join");
        let clips = &edit.song().tracks[0].clips;
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].start, 0);
        assert_eq!(clips[0].length, 2880);
        edit.undo().expect("undo");
        assert_eq!(edit.song().tracks[0].clips.len(), 2);
        assert!(
            edit.join_clips(
                track,
                project::ClipId::new(1).expect("valid"),
                project::ClipId::new(1).expect("valid")
            )
            .is_err()
        );
    }

    #[test]
    fn plugin_slots_can_be_set_and_undone() {
        use project::song::{PluginFormat, PluginSlot};
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.apply(Command::SetPlugins {
            track,
            plugins: vec![PluginSlot {
                id: 1,
                name: String::from("Limiter"),
                format: PluginFormat::Vst3,
                path: String::from("/Library/Audio/Plug-Ins/VST3/Limiter.vst3"),
                identifier: String::new(),
                bypassed: false,
                state: String::from("state"),
            }],
        })
        .expect("plugins");
        assert_eq!(edit.song().tracks[0].plugins.len(), 1);
        assert_eq!(edit.song().tracks[0].plugins[0].name, "Limiter");
        edit.undo().expect("undo");
        assert!(edit.song().tracks[0].plugins.is_empty());
    }

    #[test]
    fn a_synth_patch_can_be_set_and_cleared() {
        use project::song::{Patch, Waveform};
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        let patch = Patch {
            waveform: Waveform::Square,
            ..Patch::default()
        };
        edit.apply(Command::SetTrackPatch {
            track,
            patch: Some(Box::new(patch)),
        })
        .expect("patch");
        assert_eq!(edit.song().tracks[0].patch, Some(patch));
        edit.apply(Command::SetTrackPatch { track, patch: None })
            .expect("clear");
        assert!(edit.song().tracks[0].patch.is_none());
        edit.undo().expect("undo clear");
        assert_eq!(edit.song().tracks[0].patch, Some(patch));
    }

    #[test]
    fn buses_and_routing_can_be_set_and_undone() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.apply(Command::SetBuses {
            buses: vec![project::song::Bus::new(1, "Reverb")],
        })
        .expect("buses");
        edit.apply(Command::SetTrackRouting {
            track,
            bus: Some(1),
            send: Some(1),
            send_level: 80,
        })
        .expect("routing");
        assert_eq!(edit.song().buses.len(), 1);
        assert_eq!(edit.song().tracks[0].bus, Some(1));
        assert_eq!(edit.song().tracks[0].send_level, 80);
        edit.undo().expect("undo routing");
        assert_eq!(edit.song().tracks[0].bus, None);
        edit.undo().expect("undo buses");
        assert!(edit.song().buses.is_empty());
    }

    #[test]
    fn an_audio_reference_can_be_set_and_cleared() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        let reference = project::song::AudioRef {
            path: String::from("/tmp/take.wav"),
            start: 0,
            length: 3840,
            offset: 0,

            fade_in: 0,
            fade_out: 0,

            stretch: false,
        };
        edit.apply(Command::SetAudioRef {
            track,
            reference: Some(reference.clone()),
        })
        .expect("set");
        assert_eq!(edit.song().tracks[0].audio.as_ref(), Some(&reference));
        edit.apply(Command::SetAudioRef {
            track,
            reference: None,
        })
        .expect("clear");
        assert!(edit.song().tracks[0].audio.is_none());
        edit.undo().expect("undo clear");
        assert!(edit.song().tracks[0].audio.is_some());
        edit.undo().expect("undo set");
        assert!(edit.song().tracks[0].audio.is_none());
    }

    #[test]
    fn automation_can_be_set_and_undone() {
        use project::song::{Automation, AutomationPoint, AutomationTarget};
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.apply(Command::SetAutomation {
            track,
            lanes: vec![Automation {
                target: AutomationTarget::Volume,
                points: vec![
                    AutomationPoint {
                        tick: 0,
                        value: 0.0,
                    },
                    AutomationPoint {
                        tick: 960,
                        value: 100.0,
                    },
                ],
            }],
        })
        .expect("automation");
        assert!((edit.song().tracks[0].volume_at(480) - 50.0).abs() < 0.01);
        assert!((edit.song().tracks[0].volume_at(0) - 0.0).abs() < 0.01);
        edit.undo().expect("undo");
        assert!((edit.song().tracks[0].volume_at(480) - 100.0).abs() < 0.01);
    }

    #[test]
    fn a_track_gain_can_be_set_and_undone() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.apply(Command::SetTrackGain {
            track,
            volume: 90,
            pan: 20,
        })
        .expect("gain");
        assert_eq!(
            (edit.song().tracks[0].volume, edit.song().tracks[0].pan),
            (90, 20)
        );
        edit.undo().expect("undo");
        assert_eq!(
            (edit.song().tracks[0].volume, edit.song().tracks[0].pan),
            (100, 64)
        );
    }

    #[test]
    fn track_flags_are_one_undo_step() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.apply(Command::SetTrackFlags {
            track,
            muted: true,
            soloed: false,
            armed: false,
        })
        .expect("mute");
        assert!(edit.song().tracks[0].muted);
        edit.apply(Command::SetTrackFlags {
            track,
            muted: true,
            soloed: true,
            armed: false,
        })
        .expect("solo");
        assert!(edit.song().tracks[0].soloed);
        edit.undo().expect("undo");
        assert!(!edit.song().tracks[0].soloed);
        assert!(edit.song().tracks[0].muted);
    }

    #[test]
    fn lyrics_and_marks_are_one_edit() {
        let mut edit = session();
        edit.apply(Command::SetLyrics {
            lyrics: vec![project::song::LyricEvent {
                tick: 0,
                text: String::from("la"),
            }],
        })
        .expect("lyrics");
        edit.apply(Command::SetMarks {
            marks: vec![project::song::Mark {
                tick: 0,
                kind: project::song::MarkKind::Dynamic(project::song::Dynamic::F),
            }],
        })
        .expect("marks");
        assert_eq!(edit.song().lyrics.len(), 1);
        assert_eq!(edit.song().marks.len(), 1);
        edit.undo().expect("undo marks");
        assert!(edit.song().marks.is_empty());
        edit.undo().expect("undo lyrics");
        assert!(edit.song().lyrics.is_empty());
    }

    #[test]
    fn slurs_are_one_edit_and_validated() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.apply(Command::SetSlurs {
            track,
            slurs: vec![project::song::Slur { start: 0, end: 960 }],
        })
        .expect("slur");
        assert_eq!(edit.song().tracks[0].slurs.len(), 1);
        edit.undo().expect("undo");
        assert!(edit.song().tracks[0].slurs.is_empty());
        assert!(
            edit.apply(Command::SetSlurs {
                track,
                slurs: vec![project::song::Slur {
                    start: 960,
                    end: 960
                }],
            })
            .is_err(),
            "an empty slur is rejected"
        );
    }

    #[test]
    fn a_soundfont_reference_is_one_edit_and_validated() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.apply(Command::SetSoundFont {
            track,
            soundfont: Some(project::song::SoundFontRef {
                path: String::from("/tmp/bank.sf2"),
                bank: 0,
                program: 4,
            }),
        })
        .expect("set");
        assert_eq!(
            edit.song().tracks[0]
                .soundfont
                .as_ref()
                .map(|font| font.program),
            Some(4)
        );
        edit.undo().expect("undo");
        assert!(edit.song().tracks[0].soundfont.is_none());
        assert!(
            edit.apply(Command::SetSoundFont {
                track,
                soundfont: Some(project::song::SoundFontRef {
                    path: String::new(),
                    bank: 0,
                    program: 0,
                }),
            })
            .is_err(),
            "an empty path is rejected"
        );
    }

    #[test]
    fn freezing_a_track_clears_notes_and_undo_restores() {
        let mut edit = session();
        edit.apply(Command::InsertNote {
            track: TrackId::new(1).expect("valid"),
            note: project::song::Note {
                id: project::NoteId::new(1).expect("valid"),
                start: 0,
                duration: music_core::PPQ,
                pitch: music_core::Pitch::from_name("C4").expect("valid"),
                velocity: music_core::Velocity::new(100).expect("valid"),
                channel: music_core::Channel::first(),
            },
        })
        .expect("note");
        edit.apply(Command::FreezeTrack {
            track: TrackId::new(1).expect("valid"),
            reference: project::song::AudioRef {
                path: String::from("/tmp/freeze.wav"),
                start: 0,
                length: music_core::PPQ,
                offset: 0,

                fade_in: 0,
                fade_out: 0,

                stretch: false,
            },
        })
        .expect("freeze");
        assert!(edit.song().tracks[0].notes.is_empty());
        assert!(edit.song().tracks[0].audio.is_some());
        edit.undo().expect("undo freeze");
        assert_eq!(edit.song().tracks[0].notes.len(), 1);
        assert!(edit.song().tracks[0].audio.is_none());
    }

    #[test]
    fn scene_names_are_one_edit() {
        let mut edit = session();
        edit.apply(Command::SetSceneNames {
            names: vec![String::from("Verse"), String::from("Chorus")],
        })
        .expect("names");
        assert_eq!(edit.song().scene_names[1], "Chorus");
        edit.undo().expect("undo");
        assert!(edit.song().scene_names.is_empty());
    }

    #[test]
    fn a_track_program_can_be_set_and_cleared() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.apply(Command::SetTrackProgram {
            track,
            program: Some(33),
        })
        .expect("set");
        assert_eq!(edit.song().tracks[0].program, Some(33));
        edit.apply(Command::SetTrackProgram {
            track,
            program: None,
        })
        .expect("clear");
        assert_eq!(edit.song().tracks[0].program, None);
        edit.undo().expect("undo");
        assert_eq!(edit.song().tracks[0].program, Some(33));
    }

    #[test]
    fn clip_commands_reject_bad_edits() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        edit.insert_clip(track, clip(1, 0, 1920)).expect("insert");
        assert!(
            edit.resize_clip(track, project::ClipId::new(1).expect("valid"), 0)
                .is_err()
        );
        assert!(
            edit.split_clip(track, 0, project::ClipId::new(2).expect("valid"))
                .is_err()
        );
        assert!(
            edit.remove_clip(track, project::ClipId::new(9).expect("valid"))
                .is_err()
        );
        edit.insert_clip(track, clip(2, 1000, 100))
            .expect_err("overlap");
        assert_eq!(edit.song().tracks[0].clips.len(), 1);
    }

    #[test]
    fn replacing_track_notes_undoes_exactly() {
        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        let first = Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 960,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        };
        edit.apply(Command::ReplaceTrackNotes {
            track,
            notes: vec![first.clone()],
        })
        .expect("replace");
        assert_eq!(edit.song().tracks[0].notes.len(), 1);

        let replacement = Note {
            start: 1920,
            ..first
        };
        edit.apply(Command::ReplaceTrackNotes {
            track,
            notes: vec![replacement],
        })
        .expect("replace");
        assert_eq!(edit.song().tracks[0].notes[0].start, 1920);
        edit.undo().expect("undo");
        assert_eq!(edit.song().tracks[0].notes[0].start, 0);
    }

    #[test]
    fn master_plugin_chain_is_undoable() {
        let mut edit = session();
        let slot = project::song::PluginSlot {
            id: 1,
            name: String::from("Master Limiter"),
            format: project::song::PluginFormat::Vst3,
            path: String::new(),
            identifier: String::new(),
            bypassed: false,
            state: String::new(),
        };
        edit.apply(Command::SetMasterPlugins {
            plugins: vec![slot.clone()],
        })
        .expect("set");
        assert_eq!(edit.song().master_plugins, vec![slot]);
        edit.undo().expect("undo");
        assert!(edit.song().master_plugins.is_empty());
    }

    #[test]
    fn provenance_can_be_set_and_cleared() {
        use project::StyleId;
        use project::provenance::Provenance;

        let mut edit = session();
        let track = TrackId::new(1).expect("valid");
        let provenance = Provenance::capture(
            edit.song(),
            StyleId::new(1).expect("valid"),
            "Main A",
            0,
            3840,
            &[],
        );
        edit.apply(Command::SetProvenance {
            track,
            provenance: Some(provenance.clone()),
        })
        .expect("set");
        assert_eq!(edit.song().tracks[0].provenance, Some(provenance));
        edit.apply(Command::SetProvenance {
            track,
            provenance: None,
        })
        .expect("clear");
        assert_eq!(edit.song().tracks[0].provenance, None);
        edit.undo().expect("undo clear");
        assert!(edit.song().tracks[0].provenance.is_some());
    }

    #[test]
    fn internal_commands_are_marked() {
        let edit = session();
        let internal = Command::ReplaceTempoMap {
            map: edit.song().tempo_map.clone(),
        };
        assert!(internal.is_internal());
        assert!(!Command::SetTempo { tick: 0, bpm: 90.0 }.is_internal());
    }

    #[test]
    fn check_project_reports_errors_only() {
        let project = ProjectFile::new(Song::default());
        // An empty song has no tracks; that is a warning, not an error.
        assert!(check_project(&project).is_ok());
    }
}
