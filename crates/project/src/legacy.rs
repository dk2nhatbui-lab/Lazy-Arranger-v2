//! One-way import of a C++ Lazy Arranger project file (`.arranger`).
//!
//! Verified against the C++ writer's current schema 4 (`src/core/include/arranger/core/ProjectFile.h`,
//! `kVersion = 4`) and against schema 3, which the C++ build still reads.
//!
//! The C++ writer stores notes as `[offset, length, pitch, velocity]` inside
//! clips that sit at a timeline position, and puts the MIDI channel on the
//! track output. This importer flattens clips onto tracks and copies the
//! channel to every note. Audio clips become the track's audio reference
//! (several clips on one track become takes with one comp span each), and the
//! mixer's volume, pan, mute and solo plus the sound's program and bank are
//! copied. Other keys are preserved verbatim in `extra` and listed in the
//! report as deferred. Relative audio paths are kept as written; call
//! [`resolve_audio_paths`] with the project's folder to make them absolute.
//!
//! The import is one-way on purpose. Writing the C++ format back needs its own
//! golden round-trip tests, which is deferred to the parity backlog.

use std::collections::BTreeMap;

use music_core::{Channel, Chord, Pitch, TempoChange, TempoMap, TimeSignature, Velocity};
use serde_json::{Map, Value};

use crate::ProjectError;
use crate::ids::{NoteId, TrackId};
use crate::song::{ProjectFile, Song, Track};

/// The `format` tag written by the C++ application.
pub const CPP_FORMAT: &str = "arranger-midi-daw-project";

/// The newest C++ schema version this importer was verified against.
///
/// The C++ writer declares version 4 and treats that number as a floor, not a
/// schema id: keys were added without bumping it. The importer accepts 1..=4
/// and preserves every key it does not interpret.
pub const CPP_VERIFIED_VERSION: u32 = 4;

/// A problem found while importing that does not stop the import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportWarning {
    /// Path inside the source document.
    pub path: String,
    /// What was skipped or approximated.
    pub message: String,
}

/// How much of the source document was carried across.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportReport {
    /// The source `format` tag.
    pub source_format: String,
    /// The source `version` value.
    pub source_version: u32,
    /// Number of tracks imported.
    pub tracks: usize,
    /// Number of notes imported.
    pub notes: usize,
    /// Source features seen and deliberately not interpreted.
    pub deferred: BTreeMap<String, usize>,
    /// Lossy or skipped details, in document order.
    pub warnings: Vec<ImportWarning>,
}

impl ImportReport {
    /// Renders the report as indented text for a CLI or log.
    #[must_use]
    pub fn to_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "imported {} track(s), {} note(s) from {} schema {}",
            self.tracks, self.notes, self.source_format, self.source_version
        );
        if !self.deferred.is_empty() {
            out.push_str("deferred source features (preserved in extra):\n");
            for (name, count) in &self.deferred {
                let _ = writeln!(out, "  {name}: {count}");
            }
        }
        for warning in &self.warnings {
            let _ = writeln!(out, "warning: {}: {}", warning.path, warning.message);
        }
        out
    }
}

/// A project imported from a legacy file, plus its conversion report.
#[derive(Clone, Debug)]
pub struct ImportedProject {
    /// The imported project in the current schema.
    pub project: ProjectFile,
    /// What was imported, deferred and lost.
    pub report: ImportReport,
}

/// Imports a C++ `.arranger` document.
///
/// # Errors
///
/// Returns [`ProjectError::WrongFormat`] when the format tag differs,
/// [`ProjectError::NewerSchema`] for a future C++ schema, and
/// [`ProjectError::Legacy`] when a required part of the document is malformed.
pub fn import_cpp_project(text: &str) -> Result<ImportedProject, ProjectError> {
    let root: Value = serde_json::from_str(text)?;
    let object = root
        .as_object()
        .ok_or_else(|| legacy("root is not an object"))?;

    let format = string_field(object, "format")?.unwrap_or_default();
    if format != CPP_FORMAT {
        return Err(ProjectError::WrongFormat { found: format });
    }
    let version = u32::try_from(
        object
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| legacy("missing integer field \"version\""))?,
    )
    .map_err(|_| legacy("version does not fit in 32 bits"))?;
    if version > CPP_VERIFIED_VERSION {
        return Err(ProjectError::NewerSchema {
            found: version,
            supported: CPP_VERIFIED_VERSION,
        });
    }

    let song_object = object
        .get("song")
        .and_then(Value::as_object)
        .ok_or_else(|| legacy("missing object field \"song\""))?;

    let mut warnings = Vec::new();
    let mut report = ImportReport {
        source_format: format,
        source_version: version,
        tracks: 0,
        notes: 0,
        deferred: BTreeMap::new(),
        warnings: Vec::new(),
    };

    let name = string_field(song_object, "title")?.unwrap_or_default();
    let tempo_map = read_tempo_map(song_object, &mut warnings)?;
    let time_signature = read_time_signature(song_object, &mut warnings);
    let tracks = read_tracks(song_object, &tempo_map, &mut warnings, &mut report)?;
    report.tracks = tracks.len();
    let chords = read_chords(song_object, &mut warnings)?;

    let scene_names = match song_object.get("scenes") {
        Some(Value::Array(names)) => names
            .iter()
            .map(|name| name.as_str().unwrap_or_default().to_string())
            .collect(),
        _ => Vec::new(),
    };
    let mut song = Song {
        name,
        tempo_map,
        time_signature,
        key: None,
        tracks,
        chords,
        arranger: Vec::new(),
        scene_names,
        buses: Vec::new(),
        master_plugins: Vec::new(),
        marks: Vec::new(),
        lyrics: Vec::new(),
        markers: Vec::new(),
        extra: BTreeMap::new(),
    };

    let consumed = ["title", "tempo", "timeSignature", "tracks", "chords"];
    for (key, value) in song_object {
        if consumed.contains(&key.as_str()) {
            continue;
        }
        // `buses` is typed in the current schema; preserve the legacy value
        // under a distinct key so serialization has no duplicate field.
        let stored = if key == "buses" {
            String::from("legacy_buses")
        } else {
            key.clone()
        };
        *report.deferred.entry(stored.clone()).or_insert(0) += 1;
        song.extra.insert(stored, value.clone());
    }

    let mut project = ProjectFile::new(song);
    project
        .extra
        .insert(String::from("source"), Value::Object(source_stamp(version)));
    report.warnings = warnings;
    project.check()?;
    Ok(ImportedProject { project, report })
}

fn source_stamp(version: u32) -> Map<String, Value> {
    let mut stamp = Map::new();
    stamp.insert(
        String::from("application"),
        Value::String(String::from(CPP_FORMAT)),
    );
    stamp.insert(String::from("schema"), Value::from(version));
    stamp
}

#[allow(clippy::unnecessary_wraps)]
fn string_field(object: &Map<String, Value>, key: &str) -> Result<Option<String>, ProjectError> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(legacy(format!("field {key:?} is not a string"))),
    }
}

fn read_tempo_map(
    song: &Map<String, Value>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<TempoMap, ProjectError> {
    let Some(value) = song.get("tempo") else {
        warnings.push(ImportWarning {
            path: String::from("song.tempo"),
            message: String::from("no tempo list; using 120 bpm"),
        });
        return Ok(TempoMap::default());
    };
    let entries = value
        .as_array()
        .ok_or_else(|| legacy("song.tempo is not an array"))?;
    let mut changes = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let pair = entry
            .as_array()
            .ok_or_else(|| legacy(format!("song.tempo[{index}] is not an array")))?;
        if pair.len() != 2 {
            return Err(legacy(format!(
                "song.tempo[{index}] must be [tick, bpm], found {} elements",
                pair.len()
            )));
        }
        let tick = pair[0]
            .as_i64()
            .ok_or_else(|| legacy(format!("song.tempo[{index}][0] is not an integer tick")))?;
        let bpm = pair[1]
            .as_f64()
            .ok_or_else(|| legacy(format!("song.tempo[{index}][1] is not a number")))?;
        changes.push(TempoChange { tick, bpm });
    }
    if changes.is_empty() {
        warnings.push(ImportWarning {
            path: String::from("song.tempo"),
            message: String::from("empty tempo list; using 120 bpm"),
        });
        return Ok(TempoMap::default());
    }
    if changes[0].tick != 0 {
        warnings.push(ImportWarning {
            path: String::from("song.tempo"),
            message: format!(
                "first tempo change is at tick {}; the earlier range uses {} bpm",
                changes[0].tick, changes[0].bpm
            ),
        });
        changes.insert(
            0,
            TempoChange {
                tick: 0,
                bpm: changes[0].bpm,
            },
        );
    }
    TempoMap::new(changes).map_err(|err| legacy(err.to_string()))
}

fn read_time_signature(
    song: &Map<String, Value>,
    warnings: &mut Vec<ImportWarning>,
) -> TimeSignature {
    let Some(Value::Array(pair)) = song.get("timeSignature") else {
        warnings.push(ImportWarning {
            path: String::from("song.timeSignature"),
            message: String::from("missing or malformed; using 4/4"),
        });
        return TimeSignature::default();
    };
    let numerator = pair
        .first()
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok());
    let denominator = pair
        .get(1)
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok());
    match (numerator, denominator) {
        (Some(numerator), Some(denominator)) => TimeSignature::new(numerator, denominator)
            .unwrap_or_else(|_| {
                warnings.push(ImportWarning {
                    path: String::from("song.timeSignature"),
                    message: format!("{numerator}/{denominator} is invalid; using 4/4"),
                });
                TimeSignature::default()
            }),
        _ => {
            warnings.push(ImportWarning {
                path: String::from("song.timeSignature"),
                message: String::from("not [numerator, denominator]; using 4/4"),
            });
            TimeSignature::default()
        }
    }
}

fn read_tracks(
    song: &Map<String, Value>,
    tempo_map: &TempoMap,
    warnings: &mut Vec<ImportWarning>,
    report: &mut ImportReport,
) -> Result<Vec<Track>, ProjectError> {
    let mut tracks = Vec::new();
    let mut used: BTreeMap<u32, ()> = BTreeMap::new();
    let mut next_id: u32 = 1;
    let Some(Value::Array(list)) = song.get("tracks") else {
        return Ok(tracks);
    };
    for (index, value) in list.iter().enumerate() {
        let path = format!("song.tracks[{index}]");
        let object = value
            .as_object()
            .ok_or_else(|| legacy(format!("{path} is not an object")))?;
        let source_id = object
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| *id > 0);
        let id = match source_id {
            Some(id) if !used.contains_key(&id) => id,
            Some(id) => {
                warnings.push(ImportWarning {
                    path: format!("{path}.id"),
                    message: format!("duplicate track id {id}; assigned a new id"),
                });
                allocate(&mut next_id, &used)
            }
            None => allocate(&mut next_id, &used),
        };
        used.insert(id, ());
        let track_id = TrackId::new(id).map_err(|err| legacy(err.to_string()))?;
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .map_or_else(|| format!("Track {index}"), String::from);
        let channel = read_track_channel(object, &path, warnings);
        let mut track = Track::new(track_id, name);
        read_clips_into(
            object, track_id, channel, &mut track, &path, warnings, report,
        )?;
        if let Some(Value::Array(slots)) = object.get("slots") {
            track.slots = read_slots(slots, channel, &path, warnings)?;
        }
        if let Some(Value::Array(clips)) = object.get("audio") {
            read_audio_clips(clips, tempo_map, &mut track, &path, warnings);
        }
        read_mixer(object, &mut track);
        read_sound(object, &mut track);
        for (key, extra) in object {
            if matches!(
                key.as_str(),
                "id" | "name" | "clips" | "notes" | "slots" | "audio"
            ) {
                continue;
            }
            *report.deferred.entry(key.clone()).or_insert(0) += 1;
            track.extra.insert(key.clone(), extra.clone());
        }
        report.tracks += 1;
        tracks.push(track);
    }
    Ok(tracks)
}

/// Copies the C++ mixer strip (volume, pan, mute, solo) onto the track.
fn read_mixer(track: &Map<String, Value>, target: &mut Track) {
    let Some(mixer) = track.get("mixer").and_then(Value::as_object) else {
        return;
    };
    let level = |key: &str| {
        mixer
            .get(key)
            .and_then(Value::as_i64)
            .map(|value| u8::try_from(value.clamp(0, 127)).unwrap_or(127))
    };
    if let Some(volume) = level("volume") {
        target.volume = volume;
    }
    if let Some(pan) = level("pan") {
        target.pan = pan;
    }
    target.muted = mixer.get("mute").and_then(Value::as_bool).unwrap_or(false);
    target.soloed = mixer.get("solo").and_then(Value::as_bool).unwrap_or(false);
}

/// Copies the C++ sound selection (program and bank) for MIDI tracks.
fn read_sound(track: &Map<String, Value>, target: &mut Track) {
    let is_audio = track
        .get("output")
        .and_then(|output| output.get("kind"))
        .and_then(Value::as_str)
        == Some("audio");
    let Some(sound) = track.get("sound").and_then(Value::as_object) else {
        return;
    };
    if is_audio {
        return;
    }
    let byte = |key: &str| {
        sound
            .get(key)
            .and_then(Value::as_i64)
            .filter(|value| (0..=127).contains(value))
            .and_then(|value| u8::try_from(value).ok())
    };
    target.program = byte("program");
    if let Some(msb) = byte("bankMsb") {
        target.bank = Some(crate::song::BankSelect {
            msb,
            lsb: byte("bankLsb").unwrap_or(0),
        });
    }
}

/// Reads the C++ audio clips of a track.
///
/// The first clip on the timeline becomes the track's audio reference. When a
/// track has several clips, each one becomes a take with a comp span over its
/// own range, so every clip still plays where it sits. Overlapping clips are
/// trimmed so the later one wins, as in the C++ app on the same layer.
fn read_audio_clips(
    clips: &[Value],
    tempo_map: &TempoMap,
    target: &mut Track,
    path: &str,
    warnings: &mut Vec<ImportWarning>,
) {
    let mut references = Vec::new();
    for (index, clip) in clips.iter().enumerate() {
        let clip_path = format!("{path}.audio[{index}]");
        let Some(object) = clip.as_object() else {
            continue;
        };
        let file = object
            .get("file")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if file.trim().is_empty() {
            warnings.push(ImportWarning {
                path: clip_path,
                message: String::from("audio clip has no file; skipped"),
            });
            continue;
        }
        let start = object
            .get("start")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0);
        let length = object
            .get("length")
            .and_then(Value::as_i64)
            .unwrap_or(music_core::PPQ * 4)
            .max(1);
        let offset_seconds = object
            .get("offset")
            .and_then(Value::as_f64)
            .filter(|seconds| seconds.is_finite())
            .unwrap_or(0.0)
            .max(0.0);
        // The Rust renderer reads the in-point through the song tempo from
        // tick 0, so convert the C++ seconds the same way.
        let offset = if offset_seconds > 0.0 {
            tempo_map.tick_at_seconds(offset_seconds).max(0)
        } else {
            0
        };
        let fade_in = object
            .get("fadeIn")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, length);
        let fade_out = object
            .get("fadeOut")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .clamp(0, length - fade_in);
        let gain_db = object
            .get("gainDb")
            .and_then(Value::as_f64)
            .filter(|gain| gain.is_finite())
            .unwrap_or(0.0)
            .clamp(-32.0, 24.0);
        #[allow(clippy::cast_possible_truncation)]
        let gain_mb = (gain_db * 1_000.0).round() as i16;
        let layer = object.get("layer").and_then(Value::as_i64).unwrap_or(0);
        references.push((
            layer,
            crate::song::AudioRef {
                path: file.to_string(),
                start,
                length,
                offset,
                fade_in,
                fade_out,
                stretch: false,
                gain_mb,
            },
        ));
    }
    if references.is_empty() {
        return;
    }
    if references.iter().any(|(layer, _)| *layer != 0) {
        warnings.push(ImportWarning {
            path: format!("{path}.audio"),
            message: String::from(
                "clip layers are not kept; where clips overlap the later one plays",
            ),
        });
    }
    let mut references: Vec<crate::song::AudioRef> = references
        .into_iter()
        .map(|(_, reference)| reference)
        .collect();
    references.sort_by_key(|reference| reference.start);
    target.audio = Some(references[0].clone());
    if references.len() == 1 {
        return;
    }
    let mut comps = Vec::with_capacity(references.len());
    for (index, reference) in references.iter().enumerate() {
        let own_end = reference.start.saturating_add(reference.length);
        let end = references
            .get(index + 1)
            .map_or(own_end, |next| next.start.min(own_end));
        if end > reference.start {
            comps.push(crate::song::AudioComp {
                take: index,
                start: reference.start,
                length: end - reference.start,
            });
        }
    }
    target.takes = references;
    target.comps = comps;
}

/// Makes relative audio paths absolute against `dir`, the folder that holds
/// the imported C++ project. The C++ app writes paths relative to it.
pub fn resolve_audio_paths(song: &mut Song, dir: &std::path::Path) {
    let resolve = |reference: &mut crate::song::AudioRef| {
        let path = std::path::Path::new(&reference.path);
        if path.is_relative() {
            reference.path = dir.join(path).display().to_string();
        }
    };
    for track in &mut song.tracks {
        if let Some(reference) = &mut track.audio {
            resolve(reference);
        }
        for take in &mut track.takes {
            resolve(take);
        }
    }
}

fn allocate(next_id: &mut u32, used: &BTreeMap<u32, ()>) -> u32 {
    while used.contains_key(next_id) {
        *next_id += 1;
    }
    let id = *next_id;
    *next_id += 1;
    id
}

fn read_track_channel(
    track: &Map<String, Value>,
    path: &str,
    warnings: &mut Vec<ImportWarning>,
) -> Channel {
    let channel = track
        .get("output")
        .and_then(Value::as_object)
        .and_then(|output| output.get("channel"))
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok());
    match channel {
        Some(one_based) => Channel::from_one_based(one_based).unwrap_or_else(|_| {
            warnings.push(ImportWarning {
                path: format!("{path}.output.channel"),
                message: format!("channel {one_based} is outside 1..=16; using channel 1"),
            });
            Channel::first()
        }),
        None => {
            warnings.push(ImportWarning {
                path: format!("{path}.output.channel"),
                message: String::from("no output channel; using channel 1"),
            });
            Channel::first()
        }
    }
}

fn read_clips_into(
    track: &Map<String, Value>,
    _track_id: TrackId,
    channel: Channel,
    target: &mut Track,
    path: &str,
    warnings: &mut Vec<ImportWarning>,
    report: &mut ImportReport,
) -> Result<(), ProjectError> {
    let Some(Value::Array(clips)) = track.get("clips") else {
        // A track may carry bare notes in some documents; accept them at tick 0.
        if let Some(Value::Array(notes)) = track.get("notes") {
            read_notes(notes, 0, channel, target, path, warnings, report)?;
        }
        return Ok(());
    };
    for (index, clip) in clips.iter().enumerate() {
        let clip_path = format!("{path}.clips[{index}]");
        let object = clip
            .as_object()
            .ok_or_else(|| legacy(format!("{clip_path} is not an object")))?;
        let start = object
            .get("start")
            .and_then(Value::as_i64)
            .ok_or_else(|| legacy(format!("{clip_path}.start is missing or not an integer")))?;
        if start < 0 {
            return Err(legacy(format!("{clip_path}.start is negative ({start})")));
        }
        let Some(Value::Array(notes)) = object.get("notes") else {
            continue;
        };
        read_notes(notes, start, channel, target, &clip_path, warnings, report)?;
        for (key, value) in object {
            if matches!(key.as_str(), "start" | "notes" | "id" | "name" | "length") {
                continue;
            }
            *report.deferred.entry(key.clone()).or_insert(0) += 1;
            let _ = value;
        }
        *report.deferred.entry(String::from("clips")).or_insert(0) += 1;
    }
    Ok(())
}

/// Reads the Loops slots of a track: `null` or a clip per scene.
fn read_slots(
    slots: &[Value],
    channel: Channel,
    path: &str,
    warnings: &mut Vec<ImportWarning>,
) -> Result<Vec<Option<crate::song::SlotClip>>, ProjectError> {
    let mut out = Vec::with_capacity(slots.len());
    for (index, slot) in slots.iter().enumerate() {
        let Some(object) = slot.as_object() else {
            out.push(None);
            continue;
        };
        let slot_path = format!("{path}.slots[{index}]");
        let length = object
            .get("length")
            .and_then(Value::as_i64)
            .filter(|length| *length > 0);
        let Some(length) = length else {
            warnings.push(ImportWarning {
                path: slot_path,
                message: String::from("slot clip has no length; left empty"),
            });
            out.push(None);
            continue;
        };
        let mut holder = Track::new(TrackId::new(1).map_err(|err| legacy(err.to_string()))?, "");
        if let Some(Value::Array(notes)) = object.get("notes") {
            let mut scratch = ImportReport {
                source_format: String::new(),
                source_version: 0,
                tracks: 0,
                notes: 0,
                deferred: BTreeMap::new(),
                warnings: Vec::new(),
            };
            read_notes(
                notes,
                0,
                channel,
                &mut holder,
                &slot_path,
                warnings,
                &mut scratch,
            )?;
        }
        out.push(Some(crate::song::SlotClip {
            name: object
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            length,
            notes: holder.notes,
        }));
    }
    while out.last().is_some_and(Option::is_none) {
        out.pop();
    }
    Ok(out)
}

fn read_notes(
    notes: &[Value],
    offset: i64,
    channel: Channel,
    target: &mut Track,
    path: &str,
    warnings: &mut Vec<ImportWarning>,
    report: &mut ImportReport,
) -> Result<(), ProjectError> {
    for (index, note) in notes.iter().enumerate() {
        let note_path = format!("{path}.notes[{index}]");
        let tuple = note
            .as_array()
            .ok_or_else(|| legacy(format!("{note_path} is not an array")))?;
        if tuple.len() < 4 {
            return Err(legacy(format!(
                "{note_path} must be [offset, length, pitch, velocity], found {} elements",
                tuple.len()
            )));
        }
        let start = tuple[0]
            .as_i64()
            .ok_or_else(|| legacy(format!("{note_path}[0] is not an integer")))?;
        let duration = tuple[1]
            .as_i64()
            .ok_or_else(|| legacy(format!("{note_path}[1] is not an integer")))?;
        let pitch_raw = tuple[2]
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| legacy(format!("{note_path}[2] is not a MIDI note")))?;
        let velocity_raw = tuple[3]
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| legacy(format!("{note_path}[3] is not a velocity")))?;

        if duration <= 0 {
            warnings.push(ImportWarning {
                path: note_path,
                message: format!("note with length {duration} was skipped"),
            });
            continue;
        }
        let pitch = Pitch::new(pitch_raw).map_err(|err| legacy(format!("{note_path}: {err}")))?;
        let velocity =
            Velocity::new(velocity_raw).map_err(|err| legacy(format!("{note_path}: {err}")))?;
        let id = NoteId::new(u32::try_from(target.notes.len() + 1).unwrap_or(u32::MAX))
            .map_err(|err| legacy(err.to_string()))?;
        let start = offset
            .checked_add(start)
            .ok_or_else(|| legacy(format!("{note_path}: start tick overflow")))?;
        target.notes.push(crate::song::Note {
            id,
            start,
            duration,
            pitch,
            velocity,
            channel,
        });
        report.notes += 1;
    }
    Ok(())
}

fn read_chords(
    song: &Map<String, Value>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<Vec<crate::song::ChordEvent>, ProjectError> {
    let mut chords = Vec::new();
    let Some(Value::Array(list)) = song.get("chords") else {
        return Ok(chords);
    };
    for (index, value) in list.iter().enumerate() {
        let path = format!("song.chords[{index}]");
        let object = value
            .as_object()
            .ok_or_else(|| legacy(format!("{path} is not an object")))?;
        let tick = object
            .get("tick")
            .and_then(Value::as_i64)
            .ok_or_else(|| legacy(format!("{path}.tick is missing")))?;
        let symbol = object
            .get("symbol")
            .or_else(|| object.get("chord"))
            .and_then(Value::as_str)
            .ok_or_else(|| legacy(format!("{path} has no chord symbol")))?;
        match Chord::parse(symbol) {
            Ok(chord) => chords.push(crate::song::ChordEvent { tick, chord }),
            Err(err) => warnings.push(ImportWarning {
                path,
                message: format!("chord {symbol:?} was skipped: {err}"),
            }),
        }
    }
    chords.sort_by_key(|event| event.tick);
    Ok(chords)
}

fn legacy(message: impl Into<String>) -> ProjectError {
    ProjectError::Legacy(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PROJECT_FORMAT, SCHEMA_VERSION};

    const DEMO: &str = include_str!("../../../tests/fixtures/legacy-cpp-v3.arranger");
    const AUDIO_V4: &str = include_str!("../../../tests/fixtures/legacy-cpp-v4.arranger");

    #[test]
    fn imports_the_demo_project() {
        let imported = import_cpp_project(DEMO).expect("demo should import");
        let song = &imported.project.song;
        assert_eq!(song.name, "Late Night Sketch");
        assert_eq!(song.tracks.len(), 6);
        assert_eq!(song.tempo_map.bpm_at(0), 112.0);
        assert_eq!(song.time_signature.numerator, 4);
        assert_eq!(imported.report.tracks, 6);
        assert!(imported.report.notes > 100, "{}", imported.report.notes);
        assert_eq!(imported.project.version, SCHEMA_VERSION);
        assert_eq!(imported.project.format, PROJECT_FORMAT);

        let drums = song
            .tracks
            .iter()
            .find(|track| track.name == "Drums")
            .expect("drums track");
        assert_eq!(drums.notes.len(), 104, "two clips of 64 and 40 notes");
        assert!(
            drums
                .notes
                .iter()
                .all(|note| note.channel.one_based() == 10)
        );
        assert_eq!(drums.notes[0].start, 0);
        assert_eq!(drums.notes[0].duration, 90);
        assert_eq!(drums.notes[0].pitch.as_u8(), 42);
        assert_eq!(imported.report.notes, 265);

        let lead = song
            .tracks
            .iter()
            .find(|track| track.name == "Lead")
            .expect("lead track");
        assert!(lead.notes.iter().all(|note| note.channel.one_based() == 1));
        // The Lead clip starts at 15360 and carries a port name.
        assert_eq!(
            lead.extra.get("output").and_then(|value| value.get("port")),
            Some(&Value::String(String::from("Digital Keyboard")))
        );
        assert!(lead.notes.iter().all(|note| note.start >= 15_360));
    }

    #[test]
    fn clip_offsets_become_absolute_ticks() {
        let imported = import_cpp_project(DEMO).expect("demo should import");
        let strings = imported
            .project
            .song
            .tracks
            .iter()
            .find(|track| track.name == "Strings")
            .expect("strings track");
        let first = strings.notes.iter().map(|note| note.start).min();
        assert_eq!(first, Some(15_360));
    }

    #[test]
    fn loops_slots_and_scene_names_are_imported() {
        let imported = import_cpp_project(DEMO).expect("demo should import");
        let song = &imported.project.song;
        assert!(!song.scene_names.is_empty());
        assert!(song.tracks.iter().any(|track| {
            track
                .slots
                .iter()
                .flatten()
                .any(|clip| !clip.notes.is_empty())
        }));
        let json = imported.project.to_json_pretty().expect("serialize");
        let back = crate::song::ProjectFile::from_json(&json).expect("reparse");
        for (back, track) in back.song.tracks.iter().zip(&song.tracks) {
            assert_eq!(back.slots, track.slots);
        }
    }

    #[test]
    fn deferred_features_are_reported_and_preserved() {
        let imported = import_cpp_project(DEMO).expect("demo should import");
        let report = &imported.report;
        assert!(report.deferred.contains_key("loop"));
        assert!(report.deferred.contains_key("scenes"));
        assert!(report.deferred.contains_key("sound"));
        assert!(imported.project.song.extra.contains_key("loop"));
        assert_eq!(
            imported.project.song.extra.get("key"),
            Some(&Value::String(String::from("C minor")))
        );
        let text = report.to_text();
        assert!(text.contains("deferred source features"), "{text}");
        assert!(text.contains("imported 6 track(s)"), "{text}");
    }

    #[test]
    fn import_survives_json_round_trip() {
        let imported = import_cpp_project(DEMO).expect("demo should import");
        let json = imported.project.to_json_pretty().expect("serialize");
        let reparsed = ProjectFile::from_json(&json).expect("reparse");
        assert_eq!(reparsed.song.tracks.len(), 6);
        assert_eq!(reparsed.song.name, "Late Night Sketch");
        assert!(reparsed.song.extra.contains_key("loop"));
    }

    #[test]
    fn rejects_other_formats_and_future_versions() {
        assert!(matches!(
            import_cpp_project(r#"{"format":"other","version":3,"song":{}}"#),
            Err(ProjectError::WrongFormat { .. })
        ));
        assert!(matches!(
            import_cpp_project(r#"{"format":"arranger-midi-daw-project","version":9,"song":{}}"#),
            Err(ProjectError::NewerSchema {
                found: 9,
                supported: CPP_VERIFIED_VERSION
            })
        ));
        assert!(matches!(
            import_cpp_project("{}"),
            Err(ProjectError::WrongFormat { .. })
        ));
    }

    #[test]
    fn imports_a_schema_4_audio_project() {
        let imported = import_cpp_project(AUDIO_V4).expect("v4 audio project should import");
        assert_eq!(imported.report.source_version, 4);
        let song = &imported.project.song;
        assert_eq!(song.name, "Copper Lights");
        assert_eq!(song.tracks.len(), 10, "named tracks survive with no notes");
        assert_eq!(imported.report.notes, 0);
        assert!(song.tracks.iter().all(|track| track.notes.is_empty()));
        assert!(
            song.tracks
                .iter()
                .any(|track| track.name == "Supersaw Chords")
        );
        // Schema 4 keys that this importer does not interpret are preserved.
        for key in ["legacy_buses", "guitarChords", "launchQuantise"] {
            assert!(song.extra.contains_key(key), "{key} was dropped");
        }
        assert!((song.tempo_map.bpm_at(0) - 126.000_126).abs() < 1e-6);
        let drums = &song.tracks[0];
        let audio = drums
            .audio
            .as_ref()
            .expect("audio clip becomes the reference");
        assert_eq!(audio.path, "Stems/01 Drums.ogg");
        assert_eq!((audio.start, audio.length), (0, 281_478));
        assert!(song.tracks.iter().all(|track| track.audio.is_some()));
        assert!(!drums.extra.contains_key("audio"));
    }

    #[test]
    fn resolves_relative_audio_paths_against_the_project_folder() {
        let mut song = import_cpp_project(AUDIO_V4).expect("import").project.song;
        let dir = std::path::Path::new("/songs/Copper Lights");
        resolve_audio_paths(&mut song, dir);
        let path = &song.tracks[0].audio.as_ref().expect("audio").path;
        assert_eq!(
            std::path::Path::new(path),
            dir.join("Stems/01 Drums.ogg").as_path()
        );
    }

    #[test]
    fn several_audio_clips_become_comped_takes_and_mixer_is_copied() {
        let json = r#"{
            "format": "arranger-midi-daw-project",
            "version": 4,
            "song": {
                "title": "Takes",
                "tempo": [[0, 120.0]],
                "timeSignature": [4, 4],
                "tracks": [{"id": 1, "name": "Vox", "output": {"kind": "audio", "channel": 1},
                            "mixer": {"volume": 90, "pan": 30, "mute": true, "solo": false},
                            "clips": [],
                            "audio": [
                                {"file": "b.wav", "start": 7680, "length": 3840, "offset": 0.5,
                                 "gainDb": -6.0, "fadeIn": 100, "fadeOut": 100},
                                {"file": "a.wav", "start": 0, "length": 9600}
                            ]},
                           {"id": 2, "name": "Keys", "output": {"kind": "midi", "channel": 2},
                            "sound": {"program": 4, "bankMsb": 0, "bankLsb": 112},
                            "clips": []}]
            }
        }"#;
        let imported = import_cpp_project(json).expect("import");
        let song = &imported.project.song;
        let vox = &song.tracks[0];
        assert_eq!(
            (vox.volume, vox.pan, vox.muted, vox.soloed),
            (90, 30, true, false)
        );
        assert_eq!(vox.audio.as_ref().expect("audio").path, "a.wav");
        assert_eq!(vox.takes.len(), 2);
        assert_eq!(vox.takes[1].path, "b.wav");
        assert_eq!(vox.takes[1].offset, 960, "0.5 s at 120 bpm is one beat");
        assert_eq!(vox.takes[1].gain_mb, -6_000);
        let spans: Vec<(usize, i64, i64)> = vox
            .comps
            .iter()
            .map(|comp| (comp.take, comp.start, comp.length))
            .collect();
        assert_eq!(spans, vec![(0, 0, 7680), (1, 7680, 3840)]);
        let keys = &song.tracks[1];
        assert_eq!(keys.program, Some(4));
        assert_eq!(keys.bank.map(|bank| (bank.msb, bank.lsb)), Some((0, 112)));
    }

    #[test]
    fn keeps_track_ids_from_the_shared_cpp_counter() {
        let imported = import_cpp_project(AUDIO_V4).expect("import");
        let ids: Vec<u32> = imported
            .project
            .song
            .tracks
            .iter()
            .map(|t| t.id.get())
            .collect();
        assert_eq!(ids, vec![1, 3, 5, 7, 9, 11, 13, 15, 17, 19]);
    }

    #[test]
    fn malformed_note_tuples_are_errors() {
        let json = r#"{
            "format": "arranger-midi-daw-project",
            "version": 3,
            "song": {
                "title": "Broken",
                "tempo": [[0, 120.0]],
                "timeSignature": [4, 4],
                "tracks": [{"id": 1, "name": "T", "output": {"channel": 1},
                            "clips": [{"start": 0, "notes": [[0, 480]]}]}]
            }
        }"#;
        let err = import_cpp_project(json).expect_err("short tuple");
        assert!(
            err.to_string()
                .contains("must be [offset, length, pitch, velocity]"),
            "{err}"
        );
    }

    #[test]
    fn zero_length_notes_are_skipped_with_a_warning() {
        let json = r#"{
            "format": "arranger-midi-daw-project",
            "version": 3,
            "song": {
                "title": "Zeros",
                "tempo": [[0, 120.0]],
                "timeSignature": [4, 4],
                "tracks": [{"id": 1, "name": "T", "output": {"channel": 1},
                            "clips": [{"start": 0, "notes": [[0, 0, 60, 100], [0, 480, 60, 100]]}]}]
            }
        }"#;
        let imported = import_cpp_project(json).expect("import");
        assert_eq!(imported.project.song.tracks[0].notes.len(), 1);
        assert!(
            imported
                .report
                .warnings
                .iter()
                .any(|warning| warning.message.contains("was skipped"))
        );
    }
}
