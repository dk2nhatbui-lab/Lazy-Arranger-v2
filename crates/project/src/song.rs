//! The song document and its project-file envelope.

use std::collections::BTreeMap;

use music_core::{Channel, Chord, Pitch, TempoMap, Tick, TimeSignature, Velocity};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ids::{NoteId, StyleId, TrackId};
use crate::{PROJECT_FORMAT, ProjectError, SCHEMA_VERSION};

/// One note on a track.
///
/// The channel lives on the note, not on the track: an imported standard MIDI
/// file may put several channels on one track, and a drum track is simply a
/// track whose notes use channel 10.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Stable identifier inside its track.
    pub id: NoteId,
    /// Start position in ticks.
    pub start: Tick,
    /// Length in ticks; always greater than zero.
    pub duration: Tick,
    /// MIDI note number.
    pub pitch: Pitch,
    /// Note-on velocity.
    pub velocity: Velocity,
    /// Playback channel.
    pub channel: Channel,
}

impl Note {
    /// Tick where the note stops sounding.
    #[must_use]
    pub fn end(&self) -> Tick {
        self.start + self.duration
    }
}

fn default_volume() -> u8 {
    100
}

fn default_pan() -> u8 {
    64
}

fn is_default_volume(value: &u8) -> bool {
    *value == 100
}

fn is_default_pan(value: &u8) -> bool {
    *value == 64
}

/// A dynamic level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dynamic {
    /// pianissimo.
    Pp,
    /// piano.
    P,
    /// mezzo-piano.
    Mp,
    /// mezzo-forte.
    Mf,
    /// forte.
    F,
    /// fortissimo.
    Ff,
}

impl Dynamic {
    /// The written symbol.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::Pp => "pp",
            Self::P => "p",
            Self::Mp => "mp",
            Self::Mf => "mf",
            Self::F => "f",
            Self::Ff => "ff",
        }
    }
}

/// An articulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Articulation {
    /// Short and detached.
    Staccato,
    /// Stressed.
    Accent,
    /// Held full length.
    Tenuto,
    /// Strongly accented.
    Marcato,
    /// Held longer.
    Fermata,
}

/// One score mark at a tick.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Mark {
    /// Position.
    pub tick: Tick,
    /// What kind of mark.
    #[serde(flatten)]
    pub kind: MarkKind,
}

/// The kind of a score mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "mark")]
pub enum MarkKind {
    /// A dynamic.
    Dynamic(Dynamic),
    /// An articulation.
    Articulation(Articulation),
}

/// One lyric syllable tied to a note tick.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LyricEvent {
    /// Tick of the note it belongs to.
    pub tick: Tick,
    /// The syllable.
    pub text: String,
}

/// One automation lane.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Automation {
    /// What the lane controls.
    pub target: AutomationTarget,
    /// Points in tick order; values are 0..=127.
    pub points: Vec<AutomationPoint>,
}

/// What an automation lane controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationTarget {
    /// Fader.
    Volume,
    /// Pan.
    Pan,
}

/// One automation point.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AutomationPoint {
    /// Position.
    pub tick: Tick,
    /// Value, 0..=127.
    pub value: f32,
}

impl Automation {
    /// The lane's value at a tick: linear between points, held at the ends.
    #[must_use]
    pub fn value_at(&self, tick: Tick) -> Option<f32> {
        if self.points.is_empty() {
            return None;
        }
        let first = self.points.first()?;
        if tick <= first.tick {
            return Some(first.value.clamp(0.0, 127.0));
        }
        for pair in self.points.windows(2) {
            let (left, right) = (pair[0], pair[1]);
            if tick >= left.tick && tick <= right.tick {
                let span = (right.tick - left.tick).max(1) as f32;
                let ratio = (tick - left.tick) as f32 / span;
                return Some((left.value + (right.value - left.value) * ratio).clamp(0.0, 127.0));
            }
        }
        Some(self.points.last()?.value.clamp(0.0, 127.0))
    }
}

impl Track {
    /// Volume in force at a tick, from automation or the static fader.
    #[must_use]
    pub fn volume_at(&self, tick: Tick) -> f32 {
        self.automation
            .iter()
            .find(|lane| lane.target == AutomationTarget::Volume)
            .and_then(|lane| lane.value_at(tick))
            .unwrap_or(f32::from(self.volume))
    }

    /// Pan in force at a tick, from automation or the static pan.
    #[must_use]
    pub fn pan_at(&self, tick: Tick) -> f32 {
        self.automation
            .iter()
            .find(|lane| lane.target == AutomationTarget::Pan)
            .and_then(|lane| lane.value_at(tick))
            .unwrap_or(f32::from(self.pan))
    }
}

/// A reference to an audio file placed on a track.
///
/// The project stores the path and the span; the audio itself never lives in
/// the project file, matching the C++ app.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AudioRef {
    /// Path to the audio file as written when it was imported.
    pub path: String,
    /// First tick of the audio on the timeline.
    pub start: Tick,
    /// Length in ticks at the tempo when it was imported.
    pub length: Tick,
}

/// A named region of a track's timeline.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Clip {
    /// Stable identifier inside the track.
    pub id: crate::ids::ClipId,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// First tick of the region.
    pub start: Tick,
    /// Length in ticks; always positive.
    pub length: Tick,
}

impl Clip {
    /// One past the last tick.
    #[must_use]
    pub fn end(&self) -> Tick {
        self.start.saturating_add(self.length)
    }

    /// True when a tick falls inside the region.
    #[must_use]
    pub fn contains(&self, tick: Tick) -> bool {
        tick >= self.start && tick < self.end()
    }
}

/// A track: an ordered list of notes with a name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Stable identifier inside the song.
    pub id: TrackId,
    /// Display name.
    pub name: String,
    /// Clip regions, in start order. A note belongs to the clip whose range
    /// contains its start; notes outside every clip are loose notes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clips: Vec<Clip>,
    /// Notes, normally sorted by start tick.
    #[serde(default)]
    pub notes: Vec<Note>,
    /// Silence this track unless another track is soloed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub muted: bool,
    /// Play only soloed tracks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub soloed: bool,
    /// Record arm. No recording path uses it yet.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub armed: bool,
    /// Automation lanes for volume and pan.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub automation: Vec<Automation>,
    /// Fader level, 0..=127 with 100 as unity.
    #[serde(default = "default_volume", skip_serializing_if = "is_default_volume")]
    pub volume: u8,
    /// Pan, 0 = hard left, 64 = centre, 127 = hard right.
    #[serde(default = "default_pan", skip_serializing_if = "is_default_pan")]
    pub pan: u8,
    /// Reference to an audio file, for audio tracks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioRef>,
    /// Keyboard voice (General MIDI program) chosen for this track.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<u8>,
    /// Where the notes came from, when they were generated by the arranger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<crate::provenance::Provenance>,
    /// Fields this build does not understand, kept verbatim on save.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl Track {
    /// The clip that contains a tick, if any.
    #[must_use]
    pub fn clip_at(&self, tick: Tick) -> Option<&Clip> {
        self.clips.iter().find(|clip| clip.contains(tick))
    }

    /// The notes whose start falls inside a clip, in track order.
    #[must_use]
    pub fn notes_in(&self, clip: &Clip) -> Vec<&Note> {
        self.notes
            .iter()
            .filter(|note| clip.contains(note.start))
            .collect()
    }

    /// Splits the clip containing `tick` into two at that tick.
    ///
    /// Returns the identifier of the new right-hand clip, or `None` when no
    /// clip contains the tick or the split would leave a zero-length piece.
    pub fn split_clip(
        &mut self,
        tick: Tick,
        next_id: crate::ids::ClipId,
    ) -> Option<crate::ids::ClipId> {
        let index = self.clips.iter().position(|clip| clip.contains(tick))?;
        let original = self.clips[index].clone();
        if tick == original.start {
            return None;
        }
        self.clips[index].length = tick - original.start;
        let end = original.end();
        let mut right = original;
        right.id = next_id;
        right.start = tick;
        right.length = end - tick;
        self.clips.insert(index + 1, right);
        Some(next_id)
    }

    /// Creates an empty track.
    #[must_use]
    pub fn new(id: TrackId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            clips: Vec::new(),
            notes: Vec::new(),
            muted: false,
            soloed: false,
            armed: false,
            automation: Vec::new(),
            volume: 100,
            pan: 64,
            audio: None,
            program: None,
            provenance: None,
            extra: BTreeMap::new(),
        }
    }
}

/// A chord symbol at a tick.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChordEvent {
    /// Position in ticks.
    pub tick: Tick,
    /// The chord in force from `tick` until the next chord event.
    pub chord: Chord,
}

/// A change in arranger intent, recorded alongside the rendered MIDI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ArrangerEvent {
    /// Choose a style.
    SelectStyle {
        /// Position in ticks.
        tick: Tick,
        /// The style to play.
        style_id: StyleId,
    },
    /// Choose a style section or variation.
    SelectVariation {
        /// Position in ticks.
        tick: Tick,
        /// Variation number, as printed in the style.
        variation: u8,
    },
    /// Trigger a fill.
    TriggerFill {
        /// Position in ticks.
        tick: Tick,
        /// Fill number.
        fill: u8,
    },
    /// Start accompaniment.
    Start {
        /// Position in ticks.
        tick: Tick,
    },
    /// Stop accompaniment.
    Stop {
        /// Position in ticks.
        tick: Tick,
    },
}

impl ArrangerEvent {
    /// Position of the event in ticks.
    #[must_use]
    pub const fn tick(&self) -> Tick {
        match self {
            Self::SelectStyle { tick, .. }
            | Self::SelectVariation { tick, .. }
            | Self::TriggerFill { tick, .. }
            | Self::Start { tick }
            | Self::Stop { tick } => *tick,
        }
    }
}

/// The song: everything needed to play and edit an arrangement, and nothing
/// that belongs to a device or a window.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Song {
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Tempo over the timeline.
    pub tempo_map: TempoMap,
    /// Meter of the song. Schema version 1 allows one signature.
    #[serde(default)]
    pub time_signature: TimeSignature,
    /// Tracks in display order.
    #[serde(default)]
    pub tracks: Vec<Track>,
    /// Chord changes, sorted by tick.
    #[serde(default)]
    pub chords: Vec<ChordEvent>,
    /// Arranger intent, sorted by tick.
    #[serde(default)]
    pub arranger: Vec<ArrangerEvent>,
    /// Score marks: dynamics and articulations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Mark>,
    /// Lyric syllables, matched to notes by tick.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lyrics: Vec<LyricEvent>,
    /// Names for clip-grid scenes, by scene index. Missing entries use
    /// "Scene N".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scene_names: Vec<String>,
    /// Named timeline positions.
    #[serde(default)]
    pub markers: Vec<crate::ids::Marker>,
    /// Fields this build does not understand, kept verbatim on save.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl Song {
    /// Creates an empty song with a name and a tempo.
    #[must_use]
    pub fn new(name: impl Into<String>, bpm: f64) -> Self {
        Self {
            name: name.into(),
            tempo_map: TempoMap::constant(bpm),
            ..Self::default()
        }
    }

    /// Returns the chord in force at `tick`, if a chord has been placed.
    #[must_use]
    pub fn chord_at(&self, tick: Tick) -> Option<&Chord> {
        self.chords
            .iter()
            .rev()
            .find(|event| event.tick <= tick)
            .map(|event| &event.chord)
    }

    /// True when the track sounds: any solo makes only soloed tracks audible,
    /// otherwise muted tracks are silent.
    #[must_use]
    pub fn is_audible(&self, track: &Track) -> bool {
        let any_solo = self.tracks.iter().any(|entry| entry.soloed);
        if any_solo { track.soloed } else { !track.muted }
    }

    /// Total length in ticks: the last note end, chord or marker.
    #[must_use]
    pub fn end_tick(&self) -> Tick {
        let note_end = self
            .tracks
            .iter()
            .flat_map(|track| track.notes.iter())
            .map(Note::end)
            .max()
            .unwrap_or(0);
        let chord_end = self.chords.last().map_or(0, |event| event.tick);
        let marker_end = self.markers.last().map_or(0, |marker| marker.tick);
        note_end.max(chord_end).max(marker_end)
    }
}

/// The project file envelope: format tag, schema version, song.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectFile {
    /// Must equal [`PROJECT_FORMAT`].
    pub format: String,
    /// Schema version of this document.
    pub version: u32,
    /// The song.
    pub song: Song,
    /// Envelope fields this build does not understand.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for ProjectFile {
    fn default() -> Self {
        Self::new(Song::default())
    }
}

impl ProjectFile {
    /// Wraps a song in the current envelope.
    #[must_use]
    pub fn new(song: Song) -> Self {
        Self {
            format: String::from(PROJECT_FORMAT),
            version: SCHEMA_VERSION,
            song,
            extra: BTreeMap::new(),
        }
    }

    /// Parses and validates a project document.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Json`] for malformed JSON,
    /// [`ProjectError::WrongFormat`] when the format tag differs,
    /// [`ProjectError::NewerSchema`] for a newer version, and
    /// [`ProjectError::Invalid`] when validation finds an error.
    pub fn from_json(text: &str) -> Result<Self, ProjectError> {
        let file: Self = serde_json::from_str(text)?;
        if file.format != PROJECT_FORMAT {
            return Err(ProjectError::WrongFormat { found: file.format });
        }
        if file.version > SCHEMA_VERSION {
            return Err(ProjectError::NewerSchema {
                found: file.version,
                supported: SCHEMA_VERSION,
            });
        }
        let mut file = file;
        file.migrate()?;
        file.check()?;
        Ok(file)
    }

    /// Serializes the project as pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Json`] if serialization fails.
    pub fn to_json_pretty(&self) -> Result<String, ProjectError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Validates the song and returns every issue found.
    #[must_use]
    pub fn validate(&self) -> Vec<crate::ValidationIssue> {
        crate::validate::validate(self)
    }

    /// Validates and fails on the first error.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Invalid`] with the first error message.
    pub fn check(&self) -> Result<(), ProjectError> {
        let issues = self.validate();
        match issues
            .iter()
            .find(|issue| issue.severity == crate::Severity::Error)
        {
            Some(issue) => Err(ProjectError::Invalid(issue.to_string())),
            None => Ok(()),
        }
    }

    /// Runs schema migrations up to [`SCHEMA_VERSION`].
    ///
    /// Version 1 is the first schema, so there is nothing to migrate yet. The
    /// function exists so version 2 can add steps without changing callers.
    /// Version 2 to 3: each track with notes and no clips gets one clip from
    /// zero to the end of its notes, at least one bar long.
    fn wrap_notes_in_clips(&mut self) {
        let bar = self.song.time_signature.bar_ticks().max(1);
        for track in &mut self.song.tracks {
            if track.notes.is_empty() || !track.clips.is_empty() {
                continue;
            }
            let end = track
                .notes
                .iter()
                .map(|note| note.start.saturating_add(note.duration.max(0)))
                .max()
                .unwrap_or(0)
                .max(bar);
            if let Ok(id) = crate::ids::ClipId::new(1) {
                track.clips.push(crate::song::Clip {
                    id,
                    name: String::from("Clip 1"),
                    start: 0,
                    length: end,
                });
            }
        }
    }

    fn migrate(&mut self) -> Result<(), ProjectError> {
        while self.version < SCHEMA_VERSION {
            match self.version {
                // Version 2 adds track provenance. A version 1 file has no
                // provenance, which is exactly the version 2 default.
                1 => self.version = 2,
                2 => {
                    self.wrap_notes_in_clips();
                    self.version = 3;
                }
                3 => {
                    // Version 4 adds an optional per-track program; None is
                    // exactly the version 3 meaning.
                    self.version = 4;
                }
                4 => {
                    // Version 5 adds mute, solo and arm flags, all false by
                    // default, which is the version 4 behaviour.
                    self.version = 5;
                }
                5 => {
                    // Version 6 adds clip-grid scene names; an empty list means
                    // the default "Scene N" names, which is the version 5 state.
                    self.version = 6;
                }
                6 => {
                    // Version 7 adds optional audio references on tracks; None
                    // is the version 6 behaviour.
                    self.version = 7;
                }
                7 => {
                    // Version 8 adds volume and pan; unity and centre are the
                    // version 7 behaviour.
                    self.version = 8;
                }
                8 => {
                    // Version 9 adds automation lanes; none means the static
                    // fader and pan of version 8.
                    self.version = 9;
                }
                9 => {
                    // Version 10 adds lyric syllables; none is the version 9
                    // behaviour.
                    self.version = 10;
                }
                10 => {
                    // Version 11 adds score marks; none is the version 10
                    // behaviour.
                    self.version = 11;
                }
                from => {
                    return Err(ProjectError::Migration {
                        from,
                        to: SCHEMA_VERSION,
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Chord, Pitch, Spelling};

    fn note(id: u32, start: i64, duration: i64, name: &str) -> Note {
        Note {
            id: NoteId::new(id).expect("valid id"),
            start,
            duration,
            pitch: Pitch::from_name(name).expect("valid pitch"),
            velocity: Velocity::new(100).expect("valid velocity"),
            channel: Channel::first(),
        }
    }

    #[test]
    fn empty_song_round_trips_through_json() {
        let project = ProjectFile::new(Song::default());
        let json = project.to_json_pretty().expect("serialize");
        let back = ProjectFile::from_json(&json).expect("parse");
        assert_eq!(back, project);
    }

    #[test]
    fn model_round_trips_with_content() {
        let mut song = Song::new("Four bars", 120.0);
        song.time_signature = TimeSignature::default();
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(note(1, 0, 960, "C4"));
        track.notes.push(note(2, 960, 960, "E4"));
        song.tracks.push(track);
        song.chords.push(ChordEvent {
            tick: 0,
            chord: Chord::parse("Cmaj7").expect("valid"),
        });
        song.chords.push(ChordEvent {
            tick: 3840,
            chord: Chord::parse("Am7").expect("valid"),
        });
        song.arranger.push(ArrangerEvent::SelectVariation {
            tick: 3840,
            variation: 2,
        });
        song.markers.push(crate::ids::Marker {
            tick: 7680,
            name: String::from("Chorus"),
        });

        let project = ProjectFile::new(song);
        let json = project.to_json_pretty().expect("serialize");
        let back = ProjectFile::from_json(&json).expect("parse");
        assert_eq!(back.song.name, "Four bars");
        assert_eq!(back.song.chords.len(), 2);
        assert_eq!(
            back.song.chord_at(0).map(|c| c.format(Spelling::Sharp)),
            Some(String::from("Cmaj7"))
        );
        assert_eq!(
            back.song.chord_at(3839).map(|c| c.format(Spelling::Sharp)),
            Some(String::from("Cmaj7"))
        );
        assert_eq!(
            back.song.chord_at(3840).map(|c| c.format(Spelling::Sharp)),
            Some(String::from("Am7"))
        );
        assert_eq!(back.song.end_tick(), 7680);
        assert_eq!(back.song.arranger[0].tick(), 3840);
        assert_eq!(
            back.validate()
                .iter()
                .filter(|i| i.severity == crate::Severity::Error)
                .count(),
            0
        );
    }

    #[test]
    fn arranger_events_use_a_readable_tag() {
        let event = ArrangerEvent::TriggerFill {
            tick: 1920,
            fill: 1,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert_eq!(json, r#"{"type":"trigger-fill","tick":1920,"fill":1}"#);
        let back: ArrangerEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, event);
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let json = r#"{
            "format": "lazy-arranger-project",
            "version": 1,
            "future_envelope": {"a": 1},
            "song": {
                "name": "Legacy",
                "tempo_map": {"changes": [{"tick": 0, "bpm": 100.0}]},
                "time_signature": {"numerator": 4, "denominator": 4},
                "future_song_field": [1, 2, 3],
                "tracks": [
                    {
                        "id": 1,
                        "name": "Imported",
                        "cc_automation": [{"controller": 7, "value": 100}],
                        "notes": []
                    }
                ]
            }
        }"#;
        let project = ProjectFile::from_json(json).expect("parse");
        assert_eq!(
            project.song.extra.get("future_song_field"),
            Some(&serde_json::json!([1, 2, 3]))
        );
        assert!(project.song.tracks[0].extra.contains_key("cc_automation"));
        let text = project.to_json_pretty().expect("serialize");
        assert!(text.contains("future_song_field"));
        assert!(text.contains("cc_automation"));
        assert!(text.contains("future_envelope"));
    }

    #[test]
    fn rejects_wrong_format_and_newer_schema() {
        let wrong = r#"{"format":"other-app","version":1,
            "song":{"tempo_map":{"changes":[{"tick":0,"bpm":120.0}]}}}"#;
        assert!(matches!(
            ProjectFile::from_json(wrong),
            Err(ProjectError::WrongFormat { .. })
        ));

        let newer = r#"{"format":"lazy-arranger-project","version":99,
            "song":{"tempo_map":{"changes":[{"tick":0,"bpm":120.0}]}}}"#;
        assert!(matches!(
            ProjectFile::from_json(newer),
            Err(ProjectError::NewerSchema {
                found: 99,
                supported: SCHEMA_VERSION
            })
        ));
    }

    #[test]
    fn rejects_invalid_song_content() {
        let json = r#"{
            "format": "lazy-arranger-project",
            "version": 1,
            "song": {
                "tempo_map": {"changes": [{"tick": 0, "bpm": 120.0}]},
                "time_signature": {"numerator": 4, "denominator": 3},
                "tracks": [
                    {"id": 1, "name": "Bad", "notes": [
                        {"id": 1, "start": -5, "duration": 960,
                         "pitch": 60, "velocity": 100, "channel": 0}
                    ]}
                ]
            }
        }"#;
        let project = ProjectFile::from_json(json);
        assert!(matches!(project, Err(ProjectError::Invalid(_))));
    }

    #[test]
    fn clips_select_their_notes_and_split_at_a_tick() {
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Clip test");
        track.clips.push(crate::song::Clip {
            id: crate::ids::ClipId::new(1).expect("valid"),
            name: String::from("A"),
            start: 0,
            length: 1920,
        });
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 960,
            pitch: music_core::Pitch::from_name("C4").expect("valid"),
            velocity: music_core::Velocity::new(100).expect("valid"),
            channel: music_core::Channel::first(),
        });
        track.notes.push(Note {
            id: NoteId::new(2).expect("valid"),
            start: 1920,
            duration: 960,
            pitch: music_core::Pitch::from_name("E4").expect("valid"),
            velocity: music_core::Velocity::new(100).expect("valid"),
            channel: music_core::Channel::first(),
        });
        assert!(track.clip_at(0).is_some());
        assert!(
            track.clip_at(1920).is_none(),
            "second note is outside clip A"
        );
        assert_eq!(track.notes_in(&track.clips[0].clone()).len(), 1);
        let split = track
            .split_clip(960, crate::ids::ClipId::new(2).expect("valid"))
            .expect("split");
        assert_eq!(track.clips.len(), 2);
        assert_eq!(track.clips[0].length, 960);
        assert_eq!(track.clips[1].start, 960);
        assert_eq!(track.clips[1].length, 960);
        assert_eq!(track.clips[1].id, split);
        assert_eq!(
            track.split_clip(0, crate::ids::ClipId::new(3).expect("valid")),
            None
        );
    }

    #[test]
    fn solo_overrides_mute_and_defaults_to_audible() {
        let mut song = Song::new("Mix", 120.0);
        let mut first = Track::new(TrackId::new(1).expect("valid"), "A");
        let mut second = Track::new(TrackId::new(2).expect("valid"), "B");
        song.tracks.push(first.clone());
        song.tracks.push(second.clone());
        assert!(song.is_audible(&first) && song.is_audible(&second));
        first.muted = true;
        assert!(!song.is_audible(&first));
        second.soloed = true;
        song.tracks[0] = first;
        song.tracks[1] = second.clone();
        assert!(
            !song.is_audible(&song.tracks[0]),
            "mute loses to another track's solo"
        );
        assert!(song.is_audible(&second));
        let mut third = Track::new(TrackId::new(3).expect("valid"), "C");
        third.muted = true;
        song.tracks.push(third);
        second.soloed = true;
        assert!(!song.is_audible(&song.tracks[2]));
    }

    #[test]
    fn mute_solo_and_arm_default_false_and_round_trip() {
        let json = r#"{"format":"lazy-arranger-project","version":5,"song":{
            "name":"Flags","tempo_map":{"changes":[{"tick":0,"bpm":120.0}]},
            "time_signature":{"numerator":4,"denominator":4},
            "tracks":[{"id":1,"name":"A","muted":true,"soloed":true,"armed":true}]}}"#;
        let project = ProjectFile::from_json(json).expect("parse");
        let track = &project.song.tracks[0];
        assert!(track.muted && track.soloed && track.armed);
        let text = project.to_json_pretty().expect("serialize");
        assert!(text.contains("\"muted\": true"));
    }

    #[test]
    fn marks_round_trip_and_default_empty() {
        use crate::song::{Articulation, Dynamic, Mark, MarkKind};
        let mut song = Song::new("Marks", 120.0);
        song.marks.push(Mark {
            tick: 960,
            kind: MarkKind::Dynamic(Dynamic::Mf),
        });
        let text = ProjectFile::new(song).to_json_pretty().expect("serialize");
        let back = ProjectFile::from_json(&text).expect("parse");
        assert_eq!(back.song.marks.len(), 1);
        assert!(matches!(
            back.song.marks[0].kind,
            MarkKind::Dynamic(Dynamic::Mf)
        ));
        let empty = ProjectFile::new(Song::new("Empty", 120.0));
        assert!(!empty.to_json_pretty().expect("serialize").contains("marks"));
        let _ = Articulation::Fermata;
    }

    #[test]
    fn lyrics_round_trip_and_default_empty() {
        let mut song = Song::new("Lyrics", 120.0);
        song.lyrics.push(crate::song::LyricEvent {
            tick: 960,
            text: String::from("la"),
        });
        let text = ProjectFile::new(song).to_json_pretty().expect("serialize");
        let back = ProjectFile::from_json(&text).expect("parse");
        assert_eq!(back.song.lyrics[0].tick, 960);
        assert_eq!(back.song.lyrics[0].text, "la");
        let empty = ProjectFile::new(Song::new("Empty", 120.0));
        assert!(
            !empty
                .to_json_pretty()
                .expect("serialize")
                .contains("lyrics")
        );
    }

    #[test]
    fn automation_interpolates_and_holds() {
        use crate::song::{Automation, AutomationPoint, AutomationTarget};
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Auto");
        track.automation.push(Automation {
            target: AutomationTarget::Volume,
            points: vec![
                AutomationPoint {
                    tick: 0,
                    value: 0.0,
                },
                AutomationPoint {
                    tick: 1000,
                    value: 100.0,
                },
            ],
        });
        assert!((track.volume_at(-10) - 0.0).abs() < 0.01);
        assert!((track.volume_at(500) - 50.0).abs() < 0.01);
        assert!((track.volume_at(2000) - 100.0).abs() < 0.01);
        assert!(
            (track.pan_at(500) - 64.0).abs() < 0.01,
            "no pan lane uses the static pan"
        );
        let song = Song {
            tracks: vec![track.clone()],
            ..Song::new("Auto", 120.0)
        };
        let text = ProjectFile::new(song).to_json_pretty().expect("serialize");
        let back = ProjectFile::from_json(&text).expect("parse");
        assert_eq!(back.song.tracks[0].automation.len(), 1);
    }

    #[test]
    fn volume_and_pan_default_to_unity_and_round_trip() {
        let mut song = Song::new("Mixer", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        song.tracks.push(track.clone());
        let text = ProjectFile::new(song.clone())
            .to_json_pretty()
            .expect("serialize");
        assert!(
            !text.contains("volume") && !text.contains("\"pan\""),
            "defaults are omitted"
        );
        track.volume = 80;
        track.pan = 20;
        song.tracks[0] = track;
        let text = ProjectFile::new(song).to_json_pretty().expect("serialize");
        assert!(text.contains("\"volume\": 80"));
        assert!(text.contains("\"pan\": 20"));
        let back = ProjectFile::from_json(&text).expect("parse");
        assert_eq!(
            (back.song.tracks[0].volume, back.song.tracks[0].pan),
            (80, 20)
        );
    }

    #[test]
    fn audio_references_round_trip_and_default_none() {
        let mut song = Song::new("Audio", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Take");
        track.audio = Some(AudioRef {
            path: String::from("/tmp/take.wav"),
            start: 0,
            length: 3840,
        });
        song.tracks.push(track);
        let text = ProjectFile::new(song).to_json_pretty().expect("serialize");
        let back = ProjectFile::from_json(&text).expect("parse");
        let audio = back.song.tracks[0].audio.clone().expect("audio");
        assert_eq!(audio.path, "/tmp/take.wav");
        assert_eq!(audio.length, 3840);
        assert_eq!(back.version, SCHEMA_VERSION);
        let empty = ProjectFile::new(Song::new("Empty", 120.0));
        assert!(!empty.to_json_pretty().expect("serialize").contains("audio"));
    }

    #[test]
    fn scene_names_round_trip_and_default_empty() {
        let mut song = Song::new("Scenes", 120.0);
        song.scene_names = vec![String::from("Verse"), String::from("Chorus")];
        let project = ProjectFile::new(song);
        let text = project.to_json_pretty().expect("serialize");
        let back = ProjectFile::from_json(&text).expect("parse");
        assert_eq!(back.song.scene_names, vec!["Verse", "Chorus"]);
        assert_eq!(back.version, SCHEMA_VERSION);
        let empty = ProjectFile::new(Song::new("Empty", 120.0));
        assert!(
            !empty
                .to_json_pretty()
                .expect("serialize")
                .contains("scene_names")
        );
    }

    #[test]
    fn a_version_3_document_gains_an_empty_program() {
        let json = r#"{
            "format": "lazy-arranger-project",
            "version": 3,
            "song": {
                "name": "Old file",
                "tempo_map": {"changes": [{"tick": 0, "bpm": 120.0}]},
                "time_signature": {"numerator": 4, "denominator": 4},
                "tracks": [{"id": 1, "name": "Piano",
                            "clips": [{"id": 1, "name": "Clip 1", "start": 0, "length": 3840}],
                            "notes": []}]
            }
        }"#;
        let project = ProjectFile::from_json(json).expect("migrates");
        assert_eq!(project.version, SCHEMA_VERSION);
        assert_eq!(project.song.tracks[0].program, None);
        assert_eq!(project.song.tracks[0].clips.len(), 1);
    }

    #[test]
    fn a_version_2_document_gets_one_clip_per_track() {
        let json = r#"{
            "format": "lazy-arranger-project",
            "version": 2,
            "song": {
                "name": "Old file",
                "tempo_map": {"changes": [{"tick": 0, "bpm": 120.0}]},
                "time_signature": {"numerator": 4, "denominator": 4},
                "tracks": [{"id": 1, "name": "Piano", "notes": [
                    {"id": 1, "start": 0, "duration": 960,
                     "pitch": 60, "velocity": 100, "channel": 0},
                    {"id": 2, "start": 1920, "duration": 480,
                     "pitch": 64, "velocity": 100, "channel": 0}
                ]}]
            }
        }"#;
        let project = ProjectFile::from_json(json).expect("migrates");
        assert_eq!(project.version, SCHEMA_VERSION);
        let track = &project.song.tracks[0];
        assert_eq!(track.clips.len(), 1);
        assert_eq!(track.clips[0].start, 0);
        assert_eq!(track.clips[0].length, 3840, "one bar minimum");
        assert!(track.clips[0].contains(1920));
    }

    #[test]
    fn a_version_1_document_migrates_to_version_2() {
        let json = r#"{
            "format": "lazy-arranger-project",
            "version": 1,
            "song": {
                "name": "Old file",
                "tempo_map": {"changes": [{"tick": 0, "bpm": 120.0}]},
                "time_signature": {"numerator": 4, "denominator": 4},
                "tracks": [{"id": 1, "name": "Piano", "notes": [
                    {"id": 1, "start": 0, "duration": 960,
                     "pitch": 60, "velocity": 100, "channel": 0}
                ]}]
            }
        }"#;
        let project = ProjectFile::from_json(json).expect("version 1 migrates");
        assert_eq!(project.version, SCHEMA_VERSION);
        assert_eq!(project.song.tracks[0].notes.len(), 1);
        assert_eq!(project.song.tracks[0].provenance, None);
        let text = project.to_json_pretty().expect("serialize");
        assert!(!text.contains("provenance"), "manual tracks omit the field");
    }

    #[test]
    fn rejects_duplicate_note_ids_and_unsorted_chords() {
        let mut song = Song::new("x", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "t");
        track.notes.push(note(1, 0, 10, "C4"));
        track.notes.push(note(1, 20, 10, "D4"));
        song.tracks.push(track);
        song.chords.push(ChordEvent {
            tick: 100,
            chord: Chord::parse("C").expect("valid"),
        });
        song.chords.push(ChordEvent {
            tick: 50,
            chord: Chord::parse("G").expect("valid"),
        });
        let issues = ProjectFile::new(song).validate();
        let errors: Vec<&str> = issues
            .iter()
            .filter(|issue| issue.severity == crate::Severity::Error)
            .map(|issue| issue.path.as_str())
            .collect();
        assert!(
            errors.iter().any(|path| path.contains("notes[1].id")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|path| path.contains("chords[1].tick")),
            "{errors:?}"
        );
    }
}
