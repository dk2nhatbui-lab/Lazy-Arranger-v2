//! Tool registry and JSON dispatch for AI assistants.
//!
//! This is the testable core of the MCP server: tool definitions, argument
//! parsing and dispatch onto [`EditSession`]. It never touches a socket, so the
//! same layer can later serve stdio, a local socket or a test harness.

pub mod assistant;

pub mod socket;

use app_commands::{Command, CommandError, EditSession};
use music_core::{Channel, Chord, Pitch, Tick, Velocity};
use project::song::{ChordEvent, Note, ProjectFile};
use project::{NoteId, TrackId};
use serde_json::{Value, json};
use thiserror::Error;

/// Errors a tool call can return.
#[derive(Debug, Error)]
pub enum ToolError {
    /// No tool has that name.
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    /// An argument is missing or malformed.
    #[error("bad arguments: {0}")]
    Arguments(String),
    /// The edit was rejected.
    #[error("edit rejected: {0}")]
    Command(#[from] CommandError),
}

impl From<music_core::MusicError> for ToolError {
    fn from(error: music_core::MusicError) -> Self {
        Self::Arguments(error.to_string())
    }
}

impl From<arranging::ArrangingError> for ToolError {
    fn from(error: arranging::ArrangingError) -> Self {
        Self::Command(CommandError::Validation(error.to_string()))
    }
}

/// One tool definition.
#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    /// Name used in a call.
    pub name: &'static str,
    /// One-line description.
    pub description: &'static str,
    /// JSON schema for the arguments.
    pub input_schema: Value,
}

/// The tool list, as JSON for a client.
#[must_use]
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "get_track",
            description: "Read one track.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "get_note",
            description: "Read one note from a track.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "note": {"type": "integer"}
                },
                "required": ["track", "note"]
            }),
        },
        Tool {
            name: "get_audio_ref",
            description: "Read a track's audio reference, when present.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "get_song_name",
            description: "Read the song name.",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "set_song_name",
            description: "Rename the song.",
            input_schema: json!({
                "type": "object",
                "properties": {"name": {"type": "string"}},
                "required": ["name"]
            }),
        },
        Tool {
            name: "get_song",
            description: "Return the song summary: tempo, meter, tracks, chords and markers.",
            input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
        Tool {
            name: "add_track",
            description: "Add an empty MIDI track and return its id.",
            input_schema: json!({
                "type": "object",
                "properties": {"name": {"type": "string"}},
                "required": ["name"]
            }),
        },
        Tool {
            name: "add_notes",
            description: "Add notes to a track in one edit. Each note needs start, duration and pitch.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "notes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "start": {"type": "integer"},
                                "duration": {"type": "integer"},
                                "pitch": {"type": "integer"},
                                "velocity": {"type": "integer"},
                                "channel": {"type": "integer"}
                            },
                            "required": ["start", "duration", "pitch"]
                        }
                    }
                },
                "required": ["track", "notes"]
            }),
        },
        Tool {
            name: "set_chords",
            description: "Replace the chord timeline with tick and chord-symbol pairs.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "chords": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "tick": {"type": "integer"},
                                "chord": {"type": "string"}
                            },
                            "required": ["tick", "chord"]
                        }
                    }
                },
                "required": ["chords"]
            }),
        },
        Tool {
            name: "get_clip_grid",
            description: "Return the clip grid: scenes of N bars (default 2) and a slot per track.",
            input_schema: json!({
                "type": "object",
                "properties": {"bars": {"type": "integer"}},
                "required": []
            }),
        },
        Tool {
            name: "korg_scan",
            description: "Recognise a Korg PCG container (decoding is not implemented).",
            input_schema: json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        },
        Tool {
            name: "set_tempo",
            description: "Set a tempo change at a tick.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tick": {"type": "integer"},
                    "bpm": {"type": "number"}
                },
                "required": ["tick", "bpm"]
            }),
        },
        Tool {
            name: "get_tempo",
            description: "List the tempo map changes.",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "quantize",
            description: "Snap every note start to a grid, one edit per changed track.",
            input_schema: json!({
                "type": "object",
                "properties": {"grid": {"type": "integer"}},
                "required": ["grid"]
            }),
        },
        Tool {
            name: "set_velocity",
            description: "Set one note's velocity.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "note": {"type": "integer"},
                    "velocity": {"type": "integer"}
                },
                "required": ["track", "note", "velocity"]
            }),
        },
        Tool {
            name: "get_plugins",
            description: "Read a track's plugin slots (state only, not hosted).",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "set_plugins",
            description: "Replace a track's plugin slots.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "plugins": {"type": "array"}
                },
                "required": ["track", "plugins"]
            }),
        },
        Tool {
            name: "add_note",
            description: "Insert one note on a track.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "start": {"type": "integer"},
                    "pitch": {"type": "integer"},
                    "duration": {"type": "integer"},
                    "velocity": {"type": "integer"}
                },
                "required": ["track", "start", "pitch"]
            }),
        },
        Tool {
            name: "remove_note",
            description: "Remove one note from a track.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "note": {"type": "integer"}
                },
                "required": ["track", "note"]
            }),
        },
        Tool {
            name: "set_chord",
            description: "Set one chord symbol at a tick.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tick": {"type": "integer"},
                    "symbol": {"type": "string"}
                },
                "required": ["tick", "symbol"]
            }),
        },
        Tool {
            name: "remove_chord",
            description: "Remove the chord at a tick.",
            input_schema: json!({
                "type": "object",
                "properties": {"tick": {"type": "integer"}},
                "required": ["tick"]
            }),
        },
        Tool {
            name: "add_marker",
            description: "Add or replace a named timeline marker.",
            input_schema: json!({
                "type": "object",
                "properties": {"tick": {"type": "integer"}, "name": {"type": "string"}},
                "required": ["tick", "name"]
            }),
        },
        Tool {
            name: "remove_marker",
            description: "Remove the marker at a tick.",
            input_schema: json!({
                "type": "object",
                "properties": {"tick": {"type": "integer"}},
                "required": ["tick"]
            }),
        },
        Tool {
            name: "add_lyric",
            description: "Add or replace the syllable at a tick.",
            input_schema: json!({
                "type": "object",
                "properties": {"tick": {"type": "integer"}, "text": {"type": "string"}},
                "required": ["tick", "text"]
            }),
        },
        Tool {
            name: "remove_lyric",
            description: "Remove the syllable at a tick.",
            input_schema: json!({
                "type": "object",
                "properties": {"tick": {"type": "integer"}},
                "required": ["tick"]
            }),
        },
        Tool {
            name: "get_chord_at",
            description: "Read the chord exactly at a tick, or null.",
            input_schema: json!({
                "type": "object",
                "properties": {"tick": {"type": "integer"}},
                "required": ["tick"]
            }),
        },
        Tool {
            name: "get_note_at",
            description: "Read the note covering or nearest a tick on a track.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}, "tick": {"type": "integer"}},
                "required": ["track", "tick"]
            }),
        },
        Tool {
            name: "set_time_signature",
            description: "Set the song meter (numerator and power-of-two denominator).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "numerator": {"type": "integer"},
                    "denominator": {"type": "integer"}
                },
                "required": ["numerator", "denominator"]
            }),
        },
        Tool {
            name: "get_time_signature",
            description: "Read the song time signature and bar ticks.",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "set_song_key",
            description: "Set the song key (tonic 0..=11 and minor), or clear it with null.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tonic": {"type": ["integer", "null"]},
                    "minor": {"type": "boolean"}
                },
                "required": ["tonic"]
            }),
        },
        Tool {
            name: "set_lyrics",
            description: "Replace all lyric syllables.",
            input_schema: json!({
                "type": "object",
                "properties": {"lyrics": {"type": "array"}},
                "required": ["lyrics"]
            }),
        },
        Tool {
            name: "get_marks",
            description: "List score marks (dynamics and articulations).",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "set_marks",
            description: "Replace score marks; kind is a dynamic or articulation name.",
            input_schema: json!({
                "type": "object",
                "properties": {"marks": {"type": "array"}},
                "required": ["marks"]
            }),
        },
        Tool {
            name: "get_patch",
            description: "Read a track's synth patch.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "set_patch",
            description: "Set or clear a track's synth patch.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "patch": {"type": ["object", "null"]}
                },
                "required": ["track"]
            }),
        },
        Tool {
            name: "export_pdf",
            description: "Write the engraved score as a vector PDF.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "track": {"type": "integer"},
                    "title": {"type": "string"}
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "get_clip",
            description: "Read one clip region.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}, "clip": {"type": "integer"}},
                "required": ["track", "clip"]
            }),
        },
        Tool {
            name: "add_clip",
            description: "Add a clip region to a track.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "start": {"type": "integer"},
                    "length": {"type": "integer"},
                    "name": {"type": "string"}
                },
                "required": ["track", "start", "length"]
            }),
        },
        Tool {
            name: "move_clip",
            description: "Move a clip region to a new start.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "clip": {"type": "integer"},
                    "start": {"type": "integer"}
                },
                "required": ["track", "clip", "start"]
            }),
        },
        Tool {
            name: "remove_clip",
            description: "Remove one clip region.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}, "clip": {"type": "integer"}},
                "required": ["track", "clip"]
            }),
        },
        Tool {
            name: "add_bus",
            description: "Add a mixer bus.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "volume": {"type": "integer"},
                    "pan": {"type": "integer"},
                    "muted": {"type": "boolean"},
                    "soloed": {"type": "boolean"}
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "remove_bus",
            description: "Remove a mixer bus by id.",
            input_schema: json!({
                "type": "object",
                "properties": {"bus": {"type": "integer"}},
                "required": ["bus"]
            }),
        },
        Tool {
            name: "add_plugin",
            description: "Add a plugin slot (format vst3 or au).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "format": {"type": "string"},
                    "name": {"type": "string"},
                    "path": {"type": "string"},
                    "identifier": {"type": "string"}
                },
                "required": ["track", "format"]
            }),
        },
        Tool {
            name: "remove_plugin",
            description: "Remove a plugin slot by id.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "plugin": {"type": "integer"}
                },
                "required": ["track", "plugin"]
            }),
        },
        Tool {
            name: "get_plugin_slots",
            description: "List a track's plugin slots.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "get_scene_name",
            description: "Read a scene name.",
            input_schema: json!({
                "type": "object",
                "properties": {"scene": {"type": "integer"}},
                "required": ["scene"]
            }),
        },
        Tool {
            name: "set_scene_name",
            description: "Set a scene name.",
            input_schema: json!({
                "type": "object",
                "properties": {"scene": {"type": "integer"}, "name": {"type": "string"}},
                "required": ["scene", "name"]
            }),
        },
        Tool {
            name: "get_marker_at",
            description: "Read the marker exactly at a tick, or null.",
            input_schema: json!({
                "type": "object",
                "properties": {"tick": {"type": "integer"}},
                "required": ["tick"]
            }),
        },
        Tool {
            name: "save_project",
            description: "Write the project JSON to a path.",
            input_schema: json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        },
        Tool {
            name: "export_midi",
            description: "Write a keyboard MIDI file (setup bar, programs, drums).",
            input_schema: json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        },
        Tool {
            name: "export_musicxml",
            description: "Write a MusicXML score.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "title": {"type": "string"}
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "render_wav",
            description: "Render the song to a WAV (16, 24 or 32-bit float).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "sample_rate": {"type": "integer"},
                    "bits": {"type": "integer"}
                },
                "required": ["path"]
            }),
        },
        Tool {
            name: "export_stems",
            description: "Render each audible track to a folder with a manifest.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "directory": {"type": "string"},
                    "sample_rate": {"type": "integer"}
                },
                "required": ["directory"]
            }),
        },
        Tool {
            name: "get_slurs",
            description: "List a track's phrase slurs.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "set_slurs",
            description: "Replace a track's phrase slurs.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "slurs": {"type": "array"}
                },
                "required": ["track", "slurs"]
            }),
        },
        Tool {
            name: "set_track_name",
            description: "Rename a track.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "name": {"type": "string"}
                },
                "required": ["track", "name"]
            }),
        },
        Tool {
            name: "set_track_gain",
            description: "Set a track's volume and pan.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "volume": {"type": "integer"},
                    "pan": {"type": "integer"}
                },
                "required": ["track", "volume", "pan"]
            }),
        },
        Tool {
            name: "set_track_flags",
            description: "Set a track's mute, solo and arm flags.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "muted": {"type": "boolean"},
                    "soloed": {"type": "boolean"},
                    "armed": {"type": "boolean"}
                },
                "required": ["track"]
            }),
        },
        Tool {
            name: "set_audio_ref",
            description: "Set or clear a track's audio reference.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "path": {"type": "string"},
                    "start": {"type": "integer"},
                    "length": {"type": "integer"},
                    "offset": {"type": "integer"},
                    "fade_in": {"type": "integer"},
                    "fade_out": {"type": "integer"},
                    "stretch": {"type": "boolean"},
                    "takes": {"type": "array"},
                    "comps": {"type": "array"}
                },
                "required": ["track"]
            }),
        },
        Tool {
            name: "get_tracks",
            description: "List tracks with mix and routing state.",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "get_notes",
            description: "List a track's notes.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "get_chords",
            description: "List chord events.",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "get_markers",
            description: "List section markers.",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "set_markers",
            description: "Replace section markers.",
            input_schema: json!({
                "type": "object",
                "properties": {"markers": {"type": "array"}},
                "required": ["markers"]
            }),
        },
        Tool {
            name: "get_clips",
            description: "List a track's clips.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "set_clips",
            description: "Replace a track's clips; ids are optional.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "clips": {"type": "array"}
                },
                "required": ["track", "clips"]
            }),
        },
        Tool {
            name: "get_scene_names",
            description: "List clip-grid scene names.",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "set_scene_names",
            description: "Replace clip-grid scene names.",
            input_schema: json!({
                "type": "object",
                "properties": {"names": {"type": "array"}},
                "required": ["names"]
            }),
        },
        Tool {
            name: "plugin_scan",
            description: "List installed VST3 and Audio Unit bundles (not hosted).",
            input_schema: json!({
                "type": "object",
                "properties": {"directories": {"type": "array"}}
            }),
        },
        Tool {
            name: "get_soundfonts",
            description: "Read SoundFont presets assigned to tracks.",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "set_soundfont",
            description: "Assign or clear a track's SoundFont preset.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "path": {"type": "string"},
                    "bank": {"type": "integer"},
                    "program": {"type": "integer"}
                },
                "required": ["track"]
            }),
        },
        Tool {
            name: "get_buses",
            description: "List mix buses.",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "set_buses",
            description: "Replace mix buses.",
            input_schema: json!({
                "type": "object",
                "properties": {"buses": {"type": "array"}},
                "required": ["buses"]
            }),
        },
        Tool {
            name: "set_track_routing",
            description: "Set a track's bus, send bus and send level.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "bus": {"type": ["integer", "null"]},
                    "send": {"type": ["integer", "null"]},
                    "send_level": {"type": "integer"}
                },
                "required": ["track"]
            }),
        },
        Tool {
            name: "l6max_scan",
            description: "Scan a folder of L6max WAV takes and report the import plan.",
            input_schema: json!({
                "type": "object",
                "properties": {"folder": {"type": "string"}},
                "required": ["folder"]
            }),
        },
        Tool {
            name: "generate",
            description: "Rule-based generation: humanize, drumify, bass, pad, arp, continue, melody.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "op": {"type": "string"},
                    "timing": {"type": "integer"},
                    "velocity": {"type": "integer"},
                    "seed": {"type": "integer"},
                    "bars": {"type": "integer"}
                },
                "required": ["track", "op"]
            }),
        },
        Tool {
            name: "get_automation",
            description: "Read a track's volume and pan automation lanes.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "set_automation",
            description: "Replace a track's automation lanes; points are (tick, value 0..127).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "lanes": {"type": "array"}
                },
                "required": ["track", "lanes"]
            }),
        },
        Tool {
            name: "get_lyrics",
            description: "Read the song's lyrics and score marks.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "style_catalog",
            description: "List numbered style files under a folder.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "root": {"type": "string"},
                    "limit": {"type": "integer"}
                },
                "required": ["root"]
            }),
        },
        Tool {
            name: "style_from_clips",
            description: "Write a Yamaha SFF style from the project's tracks, optionally split at markers.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "out": {"type": "string"},
                    "section": {"type": "string"},
                    "chord": {"type": "string"},
                    "length": {"type": "integer"},
                    "from_markers": {"type": "boolean"},
                    "version": {"type": "string"}
                },
                "required": ["out"]
            }),
        },
        Tool {
            name: "audio_to_midi",
            description: "Transcribe monophonic audio to melody or drum notes.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "audio": {"type": "string"},
                    "bpm": {"type": "number"},
                    "mode": {"type": "string"},
                    "level": {"type": "integer"}
                },
                "required": ["audio"]
            }),
        },
        Tool {
            name: "audio_tags",
            description: "Guess an instrument family from monophonic audio.",
            input_schema: json!({
                "type": "object",
                "properties": {"audio": {"type": "string"}},
                "required": ["audio"]
            }),
        },
        Tool {
            name: "groove_from_audio",
            description: "Extract a groove template from monophonic audio.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "audio": {"type": "string"},
                    "dir": {"type": "string"},
                    "name": {"type": "string"},
                    "bpm": {"type": "number"},
                    "detect_tempo": {"type": "boolean"},
                    "grid": {"type": "integer"}
                },
                "required": ["audio", "name"]
            }),
        },
        Tool {
            name: "style_from_audio",
            description: "Write a Yamaha SFF style from monophonic audio analysis.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "audio": {"type": "string"},
                    "out": {"type": "string"},
                    "bpm": {"type": "number"},
                    "section": {"type": "string"},
                    "chord": {"type": "string"},
                    "split_parts": {"type": "boolean"},
                    "section_bars": {"type": "integer"},
                    "drums": {"type": "boolean"},
                    "detect_tempo": {"type": "boolean"},
                    "version": {"type": "string"}
                },
                "required": ["audio", "out"]
            }),
        },
        Tool {
            name: "theory",
            description: "Scale names, roman labels, cadence or voice-leading cost.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "op": {"type": "string"},
                    "tonic": {"type": "string"},
                    "mode": {"type": "string"},
                    "chords": {"type": "array"},
                    "to": {"type": "string"}
                },
                "required": ["op"]
            }),
        },
        Tool {
            name: "edit_slot",
            description: "Edit one clip-grid slot: clear it, copy it to a tick, or set its length.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "scene": {"type": "integer"},
                    "track": {"type": "integer"},
                    "action": {"type": "string"},
                    "tick": {"type": "integer"},
                    "length": {"type": "integer"}
                },
                "required": ["scene", "track", "action"]
            }),
        },
        Tool {
            name: "find_chords",
            description: "Recognize a chord per bar from the notes and write the chord timeline.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "arrangement_doctor",
            description: "Return Arrangement Doctor proposals (crowded bars, literal repeats).",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "extract_groove",
            description: "Return the groove (1/16 timing offsets and levels) of a track's first bar.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "list_grooves",
            description: "List groove files in a directory.",
            input_schema: json!({
                "type": "object",
                "properties": {"directory": {"type": "string"}},
                "required": ["directory"]
            }),
        },
        Tool {
            name: "save_groove",
            description: "Extract one bar of a track to a groove file.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "name": {"type": "string"},
                    "directory": {"type": "string"}
                },
                "required": ["track", "name", "directory"]
            }),
        },
        Tool {
            name: "apply_groove_file",
            description: "Load a groove by name and apply it to a track.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "name": {"type": "string"},
                    "directory": {"type": "string"},
                    "timing": {"type": "number"},
                    "accents": {"type": "number"}
                },
                "required": ["track", "name", "directory"]
            }),
        },
        Tool {
            name: "apply_groove",
            description: "Apply one track's groove to the other tracks.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "timing": {"type": "number"},
                    "accents": {"type": "number"}
                },
                "required": ["track"]
            }),
        },
        Tool {
            name: "make_song_form",
            description: "Write a preset song form to the timeline: short, gig, long_intro or instrumental.",
            input_schema: json!({
                "type": "object",
                "properties": {"preset": {"type": "string"}},
                "required": ["preset"]
            }),
        },
        Tool {
            name: "reharmonize",
            description: "Replace the chord at a tick with a tritone, borrowed, two_five or relative rule.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tick": {"type": "integer"},
                    "kind": {"type": "string"}
                },
                "required": ["tick", "kind"]
            }),
        },
        Tool {
            name: "set_track_program",
            description: "Choose the keyboard voice (GM program) for a track, or clear it with null.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "program": {"type": ["integer", "null"]}
                },
                "required": ["track", "program"]
            }),
        },
        Tool {
            name: "get_undo",
            description: "Return how many undo steps are available.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "freeze_track",
            description: "Freeze a track to an audio reference, clearing its notes.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "path": {"type": "string"},
                    "start": {"type": "integer"},
                    "length": {"type": "integer"},
                    "offset": {"type": "integer"},
                    "fade_in": {"type": "integer"},
                    "fade_out": {"type": "integer"},
                    "stretch": {"type": "boolean"}
                },
                "required": ["track", "path", "length"]
            }),
        },
        Tool {
            name: "restore_track",
            description: "Restore a frozen track's notes and optional audio reference.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "notes": {"type": "array"},
                    "audio": {"type": ["object", "null"]}
                },
                "required": ["track", "notes"]
            }),
        },
        Tool {
            name: "redo",
            description: "Redo the last undone edit.",
            input_schema: json!({"type": "object"}),
        },
        Tool {
            name: "remove_track",
            description: "Remove a track and every note on it.",
            input_schema: json!({
                "type": "object",
                "properties": {"track": {"type": "integer"}},
                "required": ["track"]
            }),
        },
        Tool {
            name: "set_track_bank",
            description: "Set a track's bank-select pair, or clear it with null.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "track": {"type": "integer"},
                    "msb": {"type": ["integer", "null"]},
                    "lsb": {"type": ["integer", "null"]}
                },
                "required": ["track"]
            }),
        },
        Tool {
            name: "undo",
            description: "Undo the last edit.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
    ]
}

static OUTPUT_ROOT: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();

/// Restricts MCP export/save tools to one directory tree.
///
/// # Errors
///
/// Returns a message when the root cannot be canonicalized or was already set.
pub fn set_output_root(root: &std::path::Path) -> Result<(), String> {
    let root = std::fs::canonicalize(root)
        .map_err(|error| format!("output root {}: {error}", root.display()))?;
    OUTPUT_ROOT
        .set(Some(root))
        .map_err(|_| String::from("MCP output root is already set"))
}

fn checked_output(path: &std::path::Path) -> Result<std::path::PathBuf, ToolError> {
    let Some(Some(root)) = OUTPUT_ROOT.get() else {
        return Ok(path.to_path_buf());
    };
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let parent = std::fs::canonicalize(parent).map_err(|error| {
        ToolError::Arguments(format!("output directory {}: {error}", parent.display()))
    })?;
    if !parent.starts_with(root) {
        return Err(ToolError::Arguments(format!(
            "output path must be under {}",
            root.display()
        )));
    }
    Ok(parent.join(path.file_name().unwrap_or_default()))
}

/// The tool list as a JSON array of `{name, description, inputSchema}`.
#[must_use]
pub fn tool_list_json() -> Value {
    Value::Array(
        tools()
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                })
            })
            .collect(),
    )
}

/// Calls one tool by name.
///
/// # Errors
///
/// Returns [`ToolError`] for an unknown tool, bad arguments or a rejected edit.
fn mark_label(kind: project::song::MarkKind) -> &'static str {
    use project::song::{Articulation, Dynamic, MarkKind};
    match kind {
        MarkKind::Dynamic(Dynamic::Pp) => "pp",
        MarkKind::Dynamic(Dynamic::P) => "p",
        MarkKind::Dynamic(Dynamic::Mp) => "mp",
        MarkKind::Dynamic(Dynamic::Mf) => "mf",
        MarkKind::Dynamic(Dynamic::F) => "f",
        MarkKind::Dynamic(Dynamic::Ff) => "ff",
        MarkKind::Articulation(Articulation::Staccato) => "staccato",
        MarkKind::Articulation(Articulation::Accent) => "accent",
        MarkKind::Articulation(Articulation::Tenuto) => "tenuto",
        MarkKind::Articulation(Articulation::Marcato) => "marcato",
        MarkKind::Articulation(Articulation::Fermata) => "fermata",
    }
}

fn patch_waveform(waveform: project::song::Waveform) -> &'static str {
    match waveform {
        project::song::Waveform::Sine => "sine",
        project::song::Waveform::Saw => "saw",
        project::song::Waveform::Square => "square",
        project::song::Waveform::Triangle => "triangle",
    }
}

pub fn call(session: &mut EditSession, name: &str, arguments: &Value) -> Result<Value, ToolError> {
    match name {
        "get_track" => {
            let track_id = track_argument(arguments)?;
            let track = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track_id)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?;
            Ok(json!({
                "id": track.id.get(),
                "name": track.name,
                "volume": track.volume,
                "pan": track.pan,
                "muted": track.muted,
                "soloed": track.soloed,
                "armed": track.armed,
                "program": track.program,
                "bus": track.bus,
                "send": track.send,
                "send_level": track.send_level,
                "soundfont": track.soundfont,
                "plugins": track.plugins.len(),
                "clips": track.clips.len(),
                "notes": track.notes.len(),
                "automation": track.automation.len(),
                "slurs": track.slurs.len()
            }))
        }
        "get_note" => {
            let track_id = track_argument(arguments)?;
            let note_id = arguments
                .get("note")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("note is required")))?;
            let note_id = project::NoteId::new(u32::try_from(note_id).unwrap_or(0))
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            let note = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track_id)
                .and_then(|entry| entry.notes.iter().find(|note| note.id == note_id))
                .ok_or_else(|| ToolError::Arguments(String::from("unknown note")))?;
            Ok(json!({
                "id": note.id.get(),
                "start": note.start,
                "duration": note.duration,
                "pitch": note.pitch.as_u8(),
                "velocity": note.velocity.as_u8(),
                "channel": note.channel.zero_based()
            }))
        }
        "get_song_name" => Ok(json!({"name": session.song().name})),
        "set_song_name" => {
            let name = arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("name is required")))?;
            session.apply(Command::SetSongName {
                name: name.to_string(),
            })?;
            Ok(json!({"name": name}))
        }
        "get_song" => Ok(song_summary(session.song())),
        "get_undo" => Ok(json!({"undo_steps": session.undo_depth()})),
        "undo" => {
            session
                .undo()
                .map_err(|error| ToolError::Command(CommandError::Validation(error.to_string())))?;
            Ok(json!({"undone": true}))
        }
        "redo" => {
            session
                .redo()
                .map_err(|error| ToolError::Command(CommandError::Validation(error.to_string())))?;
            Ok(json!({"redone": true}))
        }
        "remove_track" => {
            let track = track_argument(arguments)?;
            session.apply(Command::RemoveTrack { track })?;
            Ok(json!({"removed": track.get()}))
        }
        "set_track_bank" => {
            let track = track_argument(arguments)?;
            let msb = arguments
                .get("msb")
                .and_then(Value::as_u64)
                .map(u8::try_from)
                .transpose()
                .map_err(|_| ToolError::Arguments(String::from("msb must be 0..=127")))?;
            let lsb = arguments
                .get("lsb")
                .and_then(Value::as_u64)
                .map(u8::try_from)
                .transpose()
                .map_err(|_| ToolError::Arguments(String::from("lsb must be 0..=127")))?;
            if msb.is_some_and(|value| value > 127) || lsb.is_some_and(|value| value > 127) {
                return Err(ToolError::Arguments(String::from(
                    "bank select values must be 0..=127",
                )));
            }
            let bank = match (msb, lsb) {
                (Some(msb), Some(lsb)) => Some(project::song::BankSelect { msb, lsb }),
                (None, None) => None,
                _ => {
                    return Err(ToolError::Arguments(String::from(
                        "set both msb and lsb, or neither",
                    )));
                }
            };
            session.apply(Command::SetTrackBank { track, bank })?;
            Ok(
                json!({"track": track.get(), "bank": bank.map(|bank| json!({"msb": bank.msb, "lsb": bank.lsb}))}),
            )
        }
        "freeze_track" => {
            let track = track_argument(arguments)?;
            let reference = audio_ref_from_value(arguments)
                .ok_or_else(|| ToolError::Arguments(String::from("path is required")))?;
            if reference.length <= 0 {
                return Err(ToolError::Arguments(String::from(
                    "length must be positive",
                )));
            }
            session.apply(Command::FreezeTrack { track, reference })?;
            Ok(json!({"track": track.get(), "frozen": true}))
        }
        "restore_track" => {
            let track = track_argument(arguments)?;
            let array = arguments
                .get("notes")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Arguments(String::from("notes must be an array")))?;
            if array.len() > 100_000 {
                return Err(ToolError::Arguments(String::from("too many notes")));
            }
            let mut notes = Vec::new();
            for (index, note) in array.iter().enumerate() {
                let start = note
                    .get("start")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| ToolError::Arguments(String::from("start is required")))?;
                let duration = note
                    .get("duration")
                    .and_then(Value::as_i64)
                    .filter(|duration| *duration > 0)
                    .ok_or_else(|| {
                        ToolError::Arguments(String::from("duration must be positive"))
                    })?;
                let pitch = note
                    .get("pitch")
                    .and_then(Value::as_u64)
                    .and_then(|value| u8::try_from(value).ok())
                    .ok_or_else(|| ToolError::Arguments(String::from("pitch is required")))?;
                let velocity =
                    u8::try_from(note.get("velocity").and_then(Value::as_u64).unwrap_or(100))
                        .unwrap_or(100);
                let channel =
                    u8::try_from(note.get("channel").and_then(Value::as_u64).unwrap_or(0))
                        .unwrap_or(0);
                notes.push(project::song::Note {
                    id: project::NoteId::new(u32::try_from(index + 1).unwrap_or(u32::MAX))
                        .map_err(|error| ToolError::Arguments(error.to_string()))?,
                    start,
                    duration,
                    pitch: music_core::Pitch::new(pitch)
                        .map_err(|error| ToolError::Arguments(error.to_string()))?,
                    velocity: music_core::Velocity::new(velocity)
                        .map_err(|error| ToolError::Arguments(error.to_string()))?,
                    channel: music_core::Channel::new(channel)
                        .map_err(|error| ToolError::Arguments(error.to_string()))?,
                });
            }
            let audio = arguments
                .get("audio")
                .filter(|value| !value.is_null())
                .and_then(audio_ref_from_value);
            session.apply(Command::RestoreTrack {
                track,
                notes,
                audio,
            })?;
            Ok(json!({"track": track.get(), "restored": true}))
        }
        "add_track" => {
            let track_name = arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("name is required")))?;
            let id = session.next_track_id()?;
            let track = project::song::Track::new(id, track_name);
            session.apply(Command::InsertTrack { track })?;
            Ok(json!({"track": id.get()}))
        }
        "set_track_program" => {
            let track = track_argument(arguments)?;
            let program = arguments
                .get("program")
                .and_then(Value::as_u64)
                .map(u8::try_from)
                .transpose()
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            if let Some(value) = program
                && value > 127
            {
                return Err(ToolError::Arguments(String::from(
                    "a program must be 0..=127",
                )));
            }
            session.apply(Command::SetTrackProgram { track, program })?;
            Ok(json!({"track": track.get(), "program": program}))
        }
        "get_clip_grid" => {
            let bars = arguments
                .get("bars")
                .and_then(Value::as_u64)
                .unwrap_or(2)
                .max(1);
            let grid =
                arranging::grid::from_timeline(session.song(), usize::try_from(bars).unwrap_or(1));
            let scenes: Vec<Value> = grid
                .rows
                .iter()
                .map(|row| {
                    json!({
                        "name": row.scene.name,
                        "start": row.scene.start,
                        "length": row.scene.length,
                        "slots": row.slots.iter().map(|slot| slot.clip.map(|clip| clip.get())).collect::<Vec<_>>(),
                    })
                })
                .collect();
            Ok(json!({
                "tracks": grid.tracks.iter().map(|track| track.get()).collect::<Vec<_>>(),
                "scenes": scenes,
            }))
        }
        "korg_scan" => {
            let path = arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("path is required")))?;
            let bytes = std::fs::read(path)
                .map_err(|error| ToolError::Arguments(format!("{path}: {error}")))?;
            let scan = style_formats::korg::scan(&bytes);
            Ok(json!({
                "korg_marker": scan.korg_marker,
                "model": scan.model,
                "offset": scan.offset,
                "size": scan.size,
                "description": style_formats::korg::describe(&scan)
            }))
        }
        "set_tempo" => {
            let tick = arguments
                .get("tick")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
            let bpm = arguments
                .get("bpm")
                .and_then(Value::as_f64)
                .ok_or_else(|| ToolError::Arguments(String::from("bpm is required")))?;
            session.apply(Command::SetTempo { tick, bpm })?;
            Ok(json!({"tick": tick, "bpm": bpm}))
        }
        "get_tempo" => Ok(json!({
            "changes": session
                .song()
                .tempo_map
                .changes()
                .iter()
                .map(|change| json!({"tick": change.tick, "bpm": change.bpm}))
                .collect::<Vec<_>>()
        })),
        "quantize" => {
            let grid = arguments
                .get("grid")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("grid is required")))?;
            let changed =
                app_commands::quantize::quantize(session, grid).map_err(ToolError::Arguments)?;
            Ok(json!({"tracks_changed": changed}))
        }
        "set_velocity" => {
            use project::NoteId;
            let track = track_argument(arguments)?;
            let note = arguments
                .get("note")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("note is required")))?;
            let note = NoteId::new(u32::try_from(note).unwrap_or(0))
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            let velocity = arguments
                .get("velocity")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("velocity is required")))?
                as u8;
            let Some(entry) = session.song().tracks.iter().find(|entry| entry.id == track) else {
                return Err(ToolError::Arguments(String::from("unknown track")));
            };
            let mut notes = entry.notes.clone();
            let Some(target) = notes.iter_mut().find(|candidate| candidate.id == note) else {
                return Err(ToolError::Arguments(String::from("unknown note")));
            };
            target.velocity = music_core::Velocity::new(velocity.clamp(1, 127))?;
            session.apply(Command::ReplaceTrackNotes { track, notes })?;
            Ok(json!({"velocity": velocity.clamp(1, 127)}))
        }
        "add_note" => {
            let track = track_argument(arguments)?;
            let note_id = session.next_note_id(track)?;
            let start = arguments
                .get("start")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("start is required")))?;
            let pitch = arguments
                .get("pitch")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("pitch is required")))?;
            let pitch = u8::try_from(pitch)
                .ok()
                .and_then(|value| music_core::Pitch::new(value).ok())
                .ok_or_else(|| ToolError::Arguments(String::from("pitch must be 0..=127")))?;
            let duration = arguments
                .get("duration")
                .and_then(Value::as_i64)
                .unwrap_or(music_core::PPQ)
                .max(1);
            let velocity = arguments
                .get("velocity")
                .and_then(Value::as_u64)
                .unwrap_or(100)
                .clamp(1, 127) as u8;
            session.apply(Command::InsertNote {
                track,
                note: project::song::Note {
                    id: note_id,
                    start,
                    duration,
                    pitch,
                    velocity: music_core::Velocity::new(velocity)
                        .map_err(|error| ToolError::Arguments(error.to_string()))?,
                    channel: music_core::Channel::first(),
                },
            })?;
            Ok(json!({"note": note_id.get()}))
        }
        "remove_note" => {
            let track = track_argument(arguments)?;
            let note = arguments
                .get("note")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("note is required")))?;
            let note = project::NoteId::new(u32::try_from(note).unwrap_or(0))
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            session.apply(Command::RemoveNote { track, note })?;
            Ok(json!({"removed": note.get()}))
        }
        "set_chord" => {
            let tick = arguments
                .get("tick")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
            let symbol = arguments
                .get("symbol")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("symbol is required")))?;
            let chord = music_core::Chord::parse(symbol)
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            session.apply(Command::SetChord { tick, chord })?;
            Ok(json!({"tick": tick, "symbol": symbol}))
        }
        "remove_chord" => {
            let tick = arguments
                .get("tick")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
            session.apply(Command::RemoveChord { tick })?;
            Ok(json!({"removed": tick}))
        }
        "add_marker" => {
            let tick = arguments
                .get("tick")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
            let name = arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("name is required")))?;
            let mut markers: Vec<project::Marker> = session
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
            markers.sort_by_key(|marker| marker.tick);
            session.apply(Command::SetMarkers { markers })?;
            Ok(json!({"tick": tick, "name": name}))
        }
        "remove_marker" => {
            let tick = arguments
                .get("tick")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
            let markers: Vec<project::Marker> = session
                .song()
                .markers
                .iter()
                .filter(|marker| marker.tick != tick)
                .cloned()
                .collect();
            session.apply(Command::SetMarkers { markers })?;
            Ok(json!({"removed": tick}))
        }
        "add_lyric" => {
            let tick = arguments
                .get("tick")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
            let text = arguments
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("text is required")))?;
            let mut lyrics: Vec<project::song::LyricEvent> = session
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
            session.apply(Command::SetLyrics { lyrics })?;
            Ok(json!({"tick": tick, "text": text}))
        }
        "remove_lyric" => {
            let tick = arguments
                .get("tick")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
            let lyrics: Vec<project::song::LyricEvent> = session
                .song()
                .lyrics
                .iter()
                .filter(|event| event.tick != tick)
                .cloned()
                .collect();
            session.apply(Command::SetLyrics { lyrics })?;
            Ok(json!({"removed": tick}))
        }
        "get_chord_at" => {
            let tick = arguments
                .get("tick")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
            let chord = session
                .song()
                .chords
                .iter()
                .find(|event| event.tick == tick)
                .map(|event| event.chord.format(music_core::Spelling::Sharp));
            Ok(json!({"chord": chord}))
        }
        "get_note_at" => {
            let track = track_argument(arguments)?;
            let tick = arguments
                .get("tick")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
            let note = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .and_then(|entry| {
                    entry
                        .notes
                        .iter()
                        .find(|note| tick >= note.start && tick < note.start + note.duration.max(1))
                        .or_else(|| {
                            entry
                                .notes
                                .iter()
                                .min_by_key(|note| (note.start - tick).abs())
                        })
                })
                .map(|note| {
                    json!({
                        "id": note.id.get(),
                        "start": note.start,
                        "duration": note.duration,
                        "pitch": note.pitch.as_u8()
                    })
                });
            Ok(json!({"note": note}))
        }
        "set_time_signature" => {
            let numerator = arguments
                .get("numerator")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("numerator is required")))?;
            let denominator = arguments
                .get("denominator")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("denominator is required")))?;
            let numerator = u8::try_from(numerator)
                .map_err(|_| ToolError::Arguments(String::from("numerator too large")))?;
            let denominator = u8::try_from(denominator)
                .map_err(|_| ToolError::Arguments(String::from("denominator too large")))?;
            let time_signature = music_core::TimeSignature::new(numerator, denominator)
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            session.apply(Command::SetTimeSignature { time_signature })?;
            Ok(json!({"numerator": numerator, "denominator": denominator}))
        }
        "get_time_signature" => {
            let meter = session.song().time_signature;
            Ok(json!({
                "numerator": meter.numerator,
                "denominator": meter.denominator,
                "bar_ticks": meter.bar_ticks()
            }))
        }
        "set_song_key" => {
            let key = match arguments.get("tonic").filter(|value| !value.is_null()) {
                None => None,
                Some(value) => {
                    let tonic = value
                        .as_u64()
                        .and_then(|value| u8::try_from(value).ok())
                        .filter(|value| *value < 12)
                        .ok_or_else(|| {
                            ToolError::Arguments(String::from("tonic must be 0..=11"))
                        })?;
                    Some(project::song::KeySignature {
                        tonic: music_core::PitchClass::new(i32::from(tonic)),
                        minor: arguments
                            .get("minor")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                }
            };
            session.apply(Command::SetSongKey { key })?;
            Ok(json!({
                "key": key.map(|key| json!({
                    "tonic": key.tonic.as_u8(),
                    "minor": key.minor
                }))
            }))
        }
        "set_lyrics" => {
            let lyrics = arguments
                .get("lyrics")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Arguments(String::from("lyrics must be an array")))?;
            let mut parsed = Vec::new();
            for lyric in lyrics {
                parsed.push(project::song::LyricEvent {
                    tick: lyric
                        .get("tick")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| ToolError::Arguments(String::from("lyric needs a tick")))?,
                    text: lyric
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                });
            }
            parsed.sort_by_key(|event| event.tick);
            session.apply(Command::SetLyrics {
                lyrics: parsed.clone(),
            })?;
            Ok(json!({"lyrics": parsed.len()}))
        }
        "get_marks" => Ok(json!({
            "marks": session.song().marks.iter().map(|mark| json!({
                "tick": mark.tick,
                "kind": mark_label(mark.kind),
            })).collect::<Vec<_>>()
        })),
        "set_marks" => {
            use project::song::{Articulation, Dynamic, Mark, MarkKind};
            let marks = arguments
                .get("marks")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Arguments(String::from("marks must be an array")))?;
            let mut parsed = Vec::new();
            for mark in marks {
                let tick = mark
                    .get("tick")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| ToolError::Arguments(String::from("mark needs a tick")))?;
                let kind = match mark.get("kind").and_then(Value::as_str) {
                    Some("pp") => MarkKind::Dynamic(Dynamic::Pp),
                    Some("p") => MarkKind::Dynamic(Dynamic::P),
                    Some("mp") => MarkKind::Dynamic(Dynamic::Mp),
                    Some("mf") => MarkKind::Dynamic(Dynamic::Mf),
                    Some("f") => MarkKind::Dynamic(Dynamic::F),
                    Some("ff") => MarkKind::Dynamic(Dynamic::Ff),
                    Some("staccato") => MarkKind::Articulation(Articulation::Staccato),
                    Some("accent") => MarkKind::Articulation(Articulation::Accent),
                    Some("tenuto") => MarkKind::Articulation(Articulation::Tenuto),
                    Some("marcato") => MarkKind::Articulation(Articulation::Marcato),
                    Some("fermata") => MarkKind::Articulation(Articulation::Fermata),
                    _ => {
                        return Err(ToolError::Arguments(String::from("unknown mark kind")));
                    }
                };
                parsed.push(Mark { tick, kind });
            }
            parsed.sort_by_key(|mark| mark.tick);
            session.apply(Command::SetMarks {
                marks: parsed.clone(),
            })?;
            Ok(json!({"marks": parsed.len()}))
        }
        "get_patch" => {
            let track = track_argument(arguments)?;
            let found = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?;
            Ok(json!({"patch": found.patch.map(|patch| json!({
                "waveform": patch_waveform(patch.waveform),
                "attack": patch.attack,
                "decay": patch.decay,
                "sustain": patch.sustain,
                "release": patch.release,
                "brightness": patch.brightness
            }))}))
        }
        "set_patch" => {
            use project::song::{Patch, Waveform};
            let track = track_argument(arguments)?;
            let patch = arguments.get("patch").filter(|value| !value.is_null());
            let parsed = match patch {
                None => None,
                Some(patch) => {
                    let waveform = match patch.get("waveform").and_then(Value::as_str) {
                        Some("saw") => Waveform::Saw,
                        Some("square") => Waveform::Square,
                        Some("triangle") => Waveform::Triangle,
                        Some("sine") | None => Waveform::Sine,
                        Some(other) => {
                            return Err(ToolError::Arguments(format!("unknown waveform {other}")));
                        }
                    };
                    Some(Box::new(Patch {
                        waveform,
                        attack: patch.get("attack").and_then(Value::as_f64).unwrap_or(0.01) as f32,
                        decay: patch.get("decay").and_then(Value::as_f64).unwrap_or(0.1) as f32,
                        sustain: patch.get("sustain").and_then(Value::as_f64).unwrap_or(0.8) as f32,
                        release: patch.get("release").and_then(Value::as_f64).unwrap_or(0.1) as f32,
                        brightness: patch
                            .get("brightness")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.5) as f32,
                    }))
                }
            };
            session.apply(Command::SetTrackPatch {
                track,
                patch: parsed.clone(),
            })?;
            Ok(json!({"patch": parsed}))
        }
        "export_pdf" => {
            let raw = arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("path is required")))?;
            let path = checked_output(std::path::Path::new(raw))?;
            let title = arguments
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Lazy Arranger score");
            let track = match arguments.get("track").and_then(Value::as_u64) {
                Some(value) => project::TrackId::new(u32::try_from(value).unwrap_or(0))
                    .map_err(|error| ToolError::Arguments(error.to_string()))?,
                None => session
                    .song()
                    .tracks
                    .iter()
                    .find(|track| !track.notes.is_empty())
                    .map(|track| track.id)
                    .ok_or_else(|| ToolError::Arguments(String::from("no track to engrave")))?,
            };
            let all_tracks = arguments
                .get("all_tracks")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let settings = score::Settings::default();
            let (pdf, pages) = if all_tracks {
                let tracks: Vec<project::TrackId> =
                    session.song().tracks.iter().map(|track| track.id).collect();
                let pdf = score::pdf::to_full_pdf(session.song(), settings, title, &tracks);
                (pdf, tracks.len().max(1))
            } else {
                let engraved = score::engrave(session.song(), track, settings);
                let pages = engraved.pages.len();
                (score::pdf::to_pdf(&engraved, settings, title), pages)
            };
            std::fs::write(&path, &pdf)
                .map_err(|error| ToolError::Arguments(format!("{}: {error}", path.display())))?;
            Ok(json!({"path": path.display().to_string(), "bytes": pdf.len(), "pages": pages}))
        }
        "get_clip" => {
            let track = track_argument(arguments)?;
            let clip_id = clip_argument(arguments)?;
            let found = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .and_then(|entry| entry.clips.iter().find(|clip| clip.id == clip_id))
                .ok_or_else(|| ToolError::Arguments(String::from("unknown clip")))?;
            Ok(json!({
                "id": found.id.get(),
                "name": found.name,
                "start": found.start,
                "length": found.length
            }))
        }
        "add_clip" => {
            let track = track_argument(arguments)?;
            let start = arguments
                .get("start")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("start is required")))?;
            let length = arguments
                .get("length")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("length is required")))?;
            if length <= 0 {
                return Err(ToolError::Arguments(String::from(
                    "length must be positive",
                )));
            }
            let mut clips = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?
                .clips
                .clone();
            let next = clips
                .iter()
                .map(|clip| clip.id.get())
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            let id = project::ClipId::new(next)
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            clips.push(project::song::Clip {
                id,
                name: arguments
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                start,
                length,
            });
            session.apply(Command::SetClips { track, clips })?;
            Ok(json!({"clip": id.get()}))
        }
        "move_clip" => {
            let track = track_argument(arguments)?;
            let clip = clip_argument(arguments)?;
            let start = arguments
                .get("start")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("start is required")))?;
            let mut clips = track_clips(session, track)?;
            let found = clips
                .iter_mut()
                .find(|entry| entry.id == clip)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown clip")))?;
            found.start = start;
            session.apply(Command::SetClips { track, clips })?;
            Ok(json!({"start": start}))
        }
        "remove_clip" => {
            let track = track_argument(arguments)?;
            let clip = clip_argument(arguments)?;
            let mut clips = track_clips(session, track)?;
            clips.retain(|entry| entry.id != clip);
            session.apply(Command::SetClips { track, clips })?;
            Ok(json!({"removed": clip.get()}))
        }
        "add_bus" => {
            let name = arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("name is required")))?;
            let mut buses = session.song().buses.clone();
            let id = buses
                .iter()
                .map(|bus| bus.id)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            buses.push(project::song::Bus {
                id,
                name: name.to_string(),
                volume: arguments
                    .get("volume")
                    .and_then(Value::as_u64)
                    .unwrap_or(100)
                    .clamp(0, 127) as u8,
                pan: arguments
                    .get("pan")
                    .and_then(Value::as_u64)
                    .unwrap_or(64)
                    .clamp(0, 127) as u8,
                muted: arguments
                    .get("muted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                soloed: arguments
                    .get("soloed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                plugins: Vec::new(),
            });
            session.apply(Command::SetBuses { buses })?;
            Ok(json!({"bus": id}))
        }
        "remove_bus" => {
            let bus = arguments
                .get("bus")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("bus is required")))?;
            let bus = u8::try_from(bus)
                .map_err(|_| ToolError::Arguments(String::from("bus must be 0..=255")))?;
            let mut buses = session.song().buses.clone();
            buses.retain(|entry| entry.id != bus);
            session.apply(Command::SetBuses { buses })?;
            Ok(json!({"removed": bus}))
        }
        "add_plugin" => {
            use project::song::{PluginFormat, PluginSlot};
            let track = track_argument(arguments)?;
            let format = match arguments.get("format").and_then(Value::as_str) {
                Some("vst3") => PluginFormat::Vst3,
                Some("au") | Some("audio_unit") => PluginFormat::AudioUnit,
                _ => {
                    return Err(ToolError::Arguments(String::from(
                        "format must be vst3 or au",
                    )));
                }
            };
            let mut plugins = track_plugins(session, track)?;
            let id = plugins
                .iter()
                .map(|slot| slot.id)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            let slot_path = arguments
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !slot_path.is_empty() && !plugin_path_allowed(&slot_path, format) {
                return Err(ToolError::Arguments(String::from(
                    "plugin path is outside the standard plug-in folders",
                )));
            }
            plugins.push(PluginSlot {
                id,
                name: arguments
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Plugin")
                    .to_string(),
                format,
                path: slot_path,
                identifier: arguments
                    .get("identifier")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                bypassed: false,
                state: String::new(),
            });
            session.apply(Command::SetPlugins { track, plugins })?;
            Ok(json!({"plugin": id}))
        }
        "remove_plugin" => {
            let track = track_argument(arguments)?;
            let plugin = arguments
                .get("plugin")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("plugin is required")))?;
            let plugin = u32::try_from(plugin)
                .map_err(|_| ToolError::Arguments(String::from("plugin id too large")))?;
            let mut plugins = track_plugins(session, track)?;
            plugins.retain(|slot| slot.id != plugin);
            session.apply(Command::SetPlugins { track, plugins })?;
            Ok(json!({"removed": plugin}))
        }
        "get_plugin_slots" => {
            let track = track_argument(arguments)?;
            let plugins = track_plugins(session, track)?;
            Ok(json!({
                "plugins": plugins.iter().map(|slot| json!({
                    "id": slot.id,
                    "name": slot.name,
                    "format": match slot.format {
                        project::song::PluginFormat::Vst3 => "vst3",
                        project::song::PluginFormat::AudioUnit => "au",
                    },
                    "path": slot.path,
                    "bypassed": slot.bypassed
                })).collect::<Vec<_>>()
            }))
        }
        "get_scene_name" => {
            let index = arguments
                .get("scene")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("scene is required")))?;
            Ok(json!({
                "name": session.song().scene_names.get(index as usize)
            }))
        }
        "set_scene_name" => {
            let index = arguments
                .get("scene")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("scene is required")))?
                as usize;
            let name = arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("name is required")))?;
            if index >= 4096 {
                return Err(ToolError::Arguments(String::from(
                    "scene index must be below 4096",
                )));
            }
            let mut names = session.song().scene_names.clone();
            if names.len() <= index {
                names.resize(index + 1, String::new());
            }
            names[index] = name.to_string();
            session.apply(Command::SetSceneNames { names })?;
            Ok(json!({"scene": index, "name": name}))
        }
        "get_marker_at" => {
            let tick = arguments
                .get("tick")
                .and_then(Value::as_i64)
                .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
            let marker = session
                .song()
                .markers
                .iter()
                .find(|marker| marker.tick == tick)
                .map(|marker| json!({"tick": marker.tick, "name": marker.name}));
            Ok(json!({"marker": marker}))
        }
        "save_project" => {
            let raw = arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("path is required")))?;
            let path = checked_output(std::path::Path::new(raw))?;
            let json = session
                .project()
                .to_json_pretty()
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            std::fs::write(&path, json)
                .map_err(|error| ToolError::Arguments(format!("{}: {error}", path.display())))?;
            Ok(json!({"path": path.display().to_string()}))
        }
        "export_midi" => {
            use smf::keyboard::Voice;
            let raw = arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("path is required")))?;
            let path = checked_output(std::path::Path::new(raw))?;
            let voices: Vec<Voice> = session
                .song()
                .tracks
                .iter()
                .enumerate()
                .map(|(index, track)| {
                    let drums = track
                        .notes
                        .iter()
                        .any(|note| note.channel.zero_based() == 9);
                    Voice {
                        channel: if drums {
                            9
                        } else {
                            u8::try_from(index).unwrap_or(0) & 0x0f
                        },
                        program: track.program.unwrap_or(0),
                        volume: track.volume.min(127),
                        pan: track.pan.min(127),
                        bank: None,
                    }
                })
                .collect();
            let (bytes, report) = smf::keyboard::export(session.song(), &voices, &[]);
            std::fs::write(&path, &bytes)
                .map_err(|error| ToolError::Arguments(format!("{}: {error}", path.display())))?;
            Ok(
                json!({"path": path.display().to_string(), "bytes": bytes.len(), "tracks": report.tracks}),
            )
        }
        "export_musicxml" => {
            let raw = arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("path is required")))?;
            let path = checked_output(std::path::Path::new(raw))?;
            let title = arguments
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Lazy Arranger score");
            let xml = musicxml::export(session.song(), title);
            std::fs::write(&path, xml.as_bytes())
                .map_err(|error| ToolError::Arguments(format!("{}: {error}", path.display())))?;
            Ok(json!({"path": path.display().to_string(), "bytes": xml.len()}))
        }
        "render_wav" => {
            let raw = arguments
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("path is required")))?;
            let path = checked_output(std::path::Path::new(raw))?;
            let sample_rate = arguments
                .get("sample_rate")
                .and_then(Value::as_u64)
                .unwrap_or(48_000)
                .clamp(8_000, 192_000) as u32;
            let bits = arguments.get("bits").and_then(Value::as_u64).unwrap_or(16);
            let rendered = audio::render(session.song(), sample_rate);
            let problems = audio::mix::check_audio_references(session.song());
            let wav = match bits {
                24 => rendered.to_wav24(),
                32 => rendered.to_wav32f(),
                16 => rendered.to_wav(),
                other => {
                    return Err(ToolError::Arguments(format!(
                        "bits must be 16, 24 or 32, found {other}"
                    )));
                }
            };
            std::fs::write(&path, wav)
                .map_err(|error| ToolError::Arguments(format!("{}: {error}", path.display())))?;
            Ok(json!({
                "path": path.display().to_string(),
                "seconds": rendered.duration_seconds(),
                "problems": problems.len(),
                "bits": bits
            }))
        }
        "export_stems" => {
            let raw = arguments
                .get("directory")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("directory is required")))?;
            let directory = checked_output(std::path::Path::new(raw))?;
            let sample_rate = arguments
                .get("sample_rate")
                .and_then(Value::as_u64)
                .unwrap_or(48_000)
                .clamp(8_000, 192_000) as u32;
            let stems = audio::render_stems(session.song(), sample_rate);
            std::fs::create_dir_all(&directory).map_err(|error| {
                ToolError::Arguments(format!("{}: {error}", directory.display()))
            })?;
            let mut manifest = String::new();
            for stem in &stems {
                let slug: String = stem
                    .name
                    .chars()
                    .map(|character| {
                        if character.is_ascii_alphanumeric() {
                            character.to_ascii_lowercase()
                        } else {
                            '-'
                        }
                    })
                    .collect();
                let file = format!("{}-{slug}.wav", stem.track.get());
                std::fs::write(directory.join(&file), stem.audio.to_wav()).map_err(|error| {
                    ToolError::Arguments(format!("{}: {error}", directory.display()))
                })?;
                manifest.push_str(&format!(
                    "{}\t{file}\t{:.2}s\n",
                    stem.name,
                    stem.audio.duration_seconds()
                ));
            }
            std::fs::write(directory.join("stems.txt"), manifest).map_err(|error| {
                ToolError::Arguments(format!("{}: {error}", directory.display()))
            })?;
            Ok(json!({"directory": directory.display().to_string(), "stems": stems.len()}))
        }
        "get_slurs" => {
            let track = track_argument(arguments)?;
            let found = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?;
            Ok(json!({
                "slurs": found.slurs.iter().map(|slur| json!({
                    "start": slur.start,
                    "end": slur.end
                })).collect::<Vec<_>>()
            }))
        }
        "set_slurs" => {
            use project::song::Slur;
            let track = track_argument(arguments)?;
            let slurs = arguments
                .get("slurs")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Arguments(String::from("slurs must be an array")))?;
            let mut parsed = Vec::new();
            for slur in slurs {
                parsed.push(Slur {
                    start: slur
                        .get("start")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| ToolError::Arguments(String::from("slur needs a start")))?,
                    end: slur
                        .get("end")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| ToolError::Arguments(String::from("slur needs an end")))?,
                });
            }
            session.apply(Command::SetSlurs {
                track,
                slurs: parsed.clone(),
            })?;
            Ok(json!({"slurs": parsed.len()}))
        }
        "set_track_name" => {
            let track = track_argument(arguments)?;
            let name = arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("name is required")))?;
            session.apply(Command::RenameTrack {
                track,
                name: name.to_string(),
            })?;
            Ok(json!({"name": name}))
        }
        "set_track_gain" => {
            let track = track_argument(arguments)?;
            let volume = arguments
                .get("volume")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("volume is required")))?
                .min(127) as u8;
            let pan = arguments
                .get("pan")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("pan is required")))?
                .min(127) as u8;
            session.apply(Command::SetTrackGain { track, volume, pan })?;
            Ok(json!({"volume": volume, "pan": pan}))
        }
        "set_track_flags" => {
            let track = track_argument(arguments)?;
            let muted = arguments
                .get("muted")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let soloed = arguments
                .get("soloed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let armed = arguments
                .get("armed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            session.apply(Command::SetTrackFlags {
                track,
                muted,
                soloed,
                armed,
            })?;
            Ok(json!({"muted": muted, "soloed": soloed, "armed": armed}))
        }
        "set_audio_ref" => {
            let track = track_argument(arguments)?;
            let current = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?;
            let reference = arguments
                .get("path")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
                .map(|_| audio_ref_from_value(arguments))
                .unwrap_or(None);
            let takes = arguments
                .get("takes")
                .and_then(Value::as_array)
                .map(|takes| {
                    takes
                        .iter()
                        .filter_map(audio_ref_from_value)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| current.takes.clone());
            let comps = arguments
                .get("comps")
                .and_then(Value::as_array)
                .map(|comps| {
                    comps
                        .iter()
                        .filter_map(audio_comp_from_value)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| current.comps.clone());
            session.apply(Command::SetAudioStateFull {
                track,
                reference: reference.clone(),
                takes,
                comps,
            })?;
            Ok(json!({"audio": reference.map(|reference| audio_ref_json(&reference))}))
        }
        "get_audio_ref" => {
            let track_id = track_argument(arguments)?;
            let track = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track_id)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?;
            Ok(json!({
                "audio": track.audio.as_ref().map(audio_ref_json),
                "takes": track.takes.iter().map(audio_ref_json).collect::<Vec<_>>(),
                "comps": track.comps.iter().map(audio_comp_json).collect::<Vec<_>>()
            }))
        }
        "get_tracks" => Ok(json!({
            "tracks": session.song().tracks.iter().map(|track| json!({
                "id": track.id.get(),
                "name": track.name,
                "volume": track.volume,
                "pan": track.pan,
                "muted": track.muted,
                "soloed": track.soloed,
                "armed": track.armed,
                "bus": track.bus,
                "send": track.send,
                "send_level": track.send_level,
                "program": track.program,
                "bank": track.bank.map(|bank| json!({"msb": bank.msb, "lsb": bank.lsb})),
                "notes": track.notes.len(),
                "clips": track.clips.len(),
                "audio": track.audio.as_ref().map(|reference| reference.path.clone())
            })).collect::<Vec<_>>()
        })),
        "get_notes" => {
            let track = track_argument(arguments)?;
            let found = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?;
            Ok(json!({
                "notes": found.notes.iter().map(|note| json!({
                    "id": note.id.get(),
                    "start": note.start,
                    "duration": note.duration,
                    "pitch": note.pitch.as_u8(),
                    "velocity": note.velocity.as_u8(),
                    "channel": note.channel.zero_based()
                })).collect::<Vec<_>>()
            }))
        }
        "get_chords" => Ok(json!({
            "chords": session.song().chords.iter().map(|event| json!({
                "tick": event.tick,
                "symbol": event.chord.format(music_core::Spelling::Sharp)
            })).collect::<Vec<_>>()
        })),
        "get_markers" => Ok(json!({
            "markers": session.song().markers.iter().map(|marker| json!({
                "tick": marker.tick,
                "name": marker.name
            })).collect::<Vec<_>>()
        })),
        "set_markers" => {
            use project::Marker;
            let markers = arguments
                .get("markers")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Arguments(String::from("markers must be an array")))?;
            let mut parsed = Vec::new();
            for marker in markers {
                parsed.push(Marker {
                    tick: marker
                        .get("tick")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| ToolError::Arguments(String::from("marker needs a tick")))?,
                    name: marker
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or_else(|| ToolError::Arguments(String::from("marker needs a name")))?
                        .to_string(),
                });
            }
            parsed.sort_by_key(|marker| marker.tick);
            session.apply(Command::SetMarkers {
                markers: parsed.clone(),
            })?;
            Ok(json!({"markers": parsed.len()}))
        }
        "get_clips" => {
            let track = track_argument(arguments)?;
            let found = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?;
            Ok(json!({
                "clips": found.clips.iter().map(|clip| json!({
                    "id": clip.id.get(),
                    "name": clip.name,
                    "start": clip.start,
                    "length": clip.length
                })).collect::<Vec<_>>()
            }))
        }
        "set_clips" => {
            use project::song::Clip;
            let track = track_argument(arguments)?;
            let clips = arguments
                .get("clips")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Arguments(String::from("clips must be an array")))?;
            let mut parsed = Vec::new();
            let mut next = 1_u32;
            for clip in clips {
                let id = clip
                    .get("id")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or(next);
                next = next.max(id + 1);
                parsed.push(Clip {
                    id: project::ClipId::new(id)
                        .map_err(|error| ToolError::Arguments(error.to_string()))?,
                    name: clip
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("Clip")
                        .to_string(),
                    start: clip.get("start").and_then(Value::as_i64).unwrap_or(0),
                    length: clip.get("length").and_then(Value::as_i64).unwrap_or(3840),
                });
            }
            parsed.sort_by_key(|clip| clip.start);
            session.apply(Command::SetClips {
                track,
                clips: parsed.clone(),
            })?;
            Ok(json!({"clips": parsed.len()}))
        }
        "get_scene_names" => Ok(json!({"names": session.song().scene_names})),
        "set_scene_names" => {
            let names = arguments
                .get("names")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Arguments(String::from("names must be an array")))?;
            let parsed: Vec<String> = names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            session.apply(Command::SetSceneNames {
                names: parsed.clone(),
            })?;
            Ok(json!({"names": parsed}))
        }
        "plugin_scan" => {
            let roots = arguments
                .get("directories")
                .and_then(Value::as_array)
                .map(|directories| {
                    directories
                        .iter()
                        .filter_map(Value::as_str)
                        .flat_map(|directory| {
                            [
                                plugins::root(plugins::Format::Vst3, directory),
                                plugins::root(plugins::Format::AudioUnit, directory),
                            ]
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(plugins::default_roots);
            let found = plugins::scan_roots(&roots);
            Ok(json!({
                "plugins": found.iter().map(|plugin| json!({
                    "name": plugin.name,
                    "path": plugin.path.display().to_string(),
                    "format": plugin.format.label()
                })).collect::<Vec<_>>()
            }))
        }
        "get_soundfonts" => Ok(json!({
            "tracks": session
                .song()
                .tracks
                .iter()
                .map(|track| json!({
                    "track": track.id.get(),
                    "soundfont": track.soundfont.as_ref().map(|font| json!({
                        "path": font.path,
                        "bank": font.bank,
                        "program": font.program
                    }))
                }))
                .collect::<Vec<_>>()
        })),
        "set_soundfont" => {
            use project::song::SoundFontRef;
            let track = track_argument(arguments)?;
            let path = arguments
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let soundfont = if path.is_empty() {
                None
            } else {
                Some(SoundFontRef {
                    path,
                    bank: arguments.get("bank").and_then(Value::as_u64).unwrap_or(0) as u16,
                    program: arguments
                        .get("program")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as u16,
                })
            };
            session.apply(Command::SetSoundFont {
                track,
                soundfont: soundfont.clone(),
            })?;
            Ok(json!({"soundfont": soundfont}))
        }
        "get_buses" => Ok(json!({
            "buses": session
                .song()
                .buses
                .iter()
                .map(|bus| json!({
                    "id": bus.id,
                    "name": bus.name,
                    "volume": bus.volume,
                    "pan": bus.pan,
                    "muted": bus.muted,
                    "soloed": bus.soloed
                }))
                .collect::<Vec<_>>()
        })),
        "set_buses" => {
            use project::song::Bus;
            let buses = arguments
                .get("buses")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Arguments(String::from("buses must be an array")))?;
            let mut parsed = Vec::new();
            for bus in buses {
                let id = bus
                    .get("id")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| ToolError::Arguments(String::from("bus needs an id")))?;
                let name = bus
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::Arguments(String::from("bus needs a name")))?;
                parsed.push(Bus {
                    id: u8::try_from(id).map_err(|_| {
                        ToolError::Arguments(String::from("bus id must be 0..=255"))
                    })?,
                    name: name.to_string(),
                    volume: bus.get("volume").and_then(Value::as_u64).unwrap_or(100) as u8,
                    pan: bus.get("pan").and_then(Value::as_u64).unwrap_or(64) as u8,
                    muted: bus.get("muted").and_then(Value::as_bool).unwrap_or(false),
                    soloed: bus.get("soloed").and_then(Value::as_bool).unwrap_or(false),
                    plugins: Vec::new(),
                });
            }
            for bus in &mut parsed {
                if let Some(existing) = session.song().buses.iter().find(|entry| entry.id == bus.id)
                {
                    bus.plugins = existing.plugins.clone();
                }
            }
            session.apply(Command::SetBuses {
                buses: parsed.clone(),
            })?;
            Ok(json!({"buses": parsed.len()}))
        }
        "set_track_routing" => {
            let track = track_argument(arguments)?;
            let bus = arguments
                .get("bus")
                .and_then(Value::as_u64)
                .map(u8::try_from)
                .transpose()
                .map_err(|_| ToolError::Arguments(String::from("bus must be 0..=255")))?;
            let send = arguments
                .get("send")
                .and_then(Value::as_u64)
                .map(u8::try_from)
                .transpose()
                .map_err(|_| ToolError::Arguments(String::from("send must be 0..=255")))?;
            let send_level = arguments
                .get("send_level")
                .and_then(Value::as_u64)
                .unwrap_or(100)
                .min(127) as u8;
            session.apply(Command::SetTrackRouting {
                track,
                bus,
                send,
                send_level,
            })?;
            Ok(json!({"bus": bus, "send": send, "send_level": send_level}))
        }
        "get_plugins" => {
            let track = track_argument(arguments)?;
            let found = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?;
            Ok(json!({
                "plugins": found.plugins.iter().map(|plugin| json!({
                    "id": plugin.id,
                    "name": plugin.name,
                    "format": match plugin.format {
                        project::song::PluginFormat::Vst3 => "vst3",
                        project::song::PluginFormat::AudioUnit => "audio_unit",
                    },
                    "path": plugin.path,
                    "bypassed": plugin.bypassed,
                    "state": plugin.state
                })).collect::<Vec<_>>()
            }))
        }
        "set_plugins" => {
            use project::song::{PluginFormat, PluginSlot};
            let track = track_argument(arguments)?;
            let plugins = arguments
                .get("plugins")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Arguments(String::from("plugins must be an array")))?;
            let mut slots = Vec::new();
            for plugin in plugins {
                let name = plugin
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::Arguments(String::from("plugin needs a name")))?;
                let format = match plugin.get("format").and_then(Value::as_str) {
                    Some("audio_unit") | Some("au") => PluginFormat::AudioUnit,
                    Some("vst3") | None => PluginFormat::Vst3,
                    Some(other) => {
                        return Err(ToolError::Arguments(format!(
                            "unknown plugin format {other}"
                        )));
                    }
                };
                let id = plugin
                    .get("id")
                    .and_then(Value::as_u64)
                    .unwrap_or((slots.len() as u64) + 1) as u32;
                slots.push(PluginSlot {
                    id,
                    name: name.to_string(),
                    format,
                    path: plugin
                        .get("path")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    identifier: plugin
                        .get("identifier")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    bypassed: plugin
                        .get("bypassed")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    state: plugin
                        .get("state")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                });
            }
            session.apply(Command::SetPlugins {
                track,
                plugins: slots.clone(),
            })?;
            Ok(json!({"plugins": slots.len()}))
        }
        "l6max_scan" => {
            let folder = arguments
                .get("folder")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("folder is required")))?;
            let plan =
                audio::l6max::scan(std::path::Path::new(folder)).map_err(ToolError::Arguments)?;
            Ok(json!({
                "takes": plan.takes.iter().map(|take| json!({
                    "name": take.name,
                    "seconds": take.seconds,
                    "channels": take.channels,
                    "sample_rate": take.sample_rate,
                    "bits": take.bits,
                    "float32": take.is_float32()
                })).collect::<Vec<_>>(),
                "total_seconds": plan.seconds(),
                "float32": plan.float32(),
                "problems": plan.problems.iter().map(|(path, reason)| json!({
                    "path": path.display().to_string(),
                    "reason": reason
                })).collect::<Vec<_>>()
            }))
        }
        "generate" => {
            use arranging::generate::Part;
            let track = track_argument(arguments)?;
            let op = arguments
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("op is required")))?;
            let seed = arguments.get("seed").and_then(Value::as_u64).unwrap_or(1);
            let timing = arguments.get("timing").and_then(Value::as_i64).unwrap_or(8);
            let velocity = arguments
                .get("velocity")
                .and_then(Value::as_u64)
                .unwrap_or(4) as u8;
            let bars = arguments.get("bars").and_then(Value::as_u64).unwrap_or(1) as usize;
            let song = session.song().clone();
            let count = match op {
                "humanize" => {
                    arranging::generate::humanize(session, track, timing, velocity, seed)?
                }
                "drumify" => arranging::generate::drumify(session, track, &song)?,
                "bass" => arranging::generate::write_part(session, track, Part::Bass, &song)?,
                "pad" => arranging::generate::write_part(session, track, Part::Pad, &song)?,
                "arp" => arranging::generate::write_part(session, track, Part::Arpeggio, &song)?,
                "continue" => arranging::generate::continue_song(session, track, bars)?,
                "melody" => arranging::generate::melody(session, track, &song, seed)?,
                other => return Err(ToolError::Arguments(format!("unknown op {other}"))),
            };
            Ok(json!({"op": op, "notes": count}))
        }
        "get_automation" => {
            let track = track_argument(arguments)?;
            let found = session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?;
            Ok(json!({
                "volume": found.volume,
                "pan": found.pan,
                "lanes": found.automation.iter().map(|lane| json!({
                    "target": match lane.target {
                        project::song::AutomationTarget::Volume => String::from("volume"),
                        project::song::AutomationTarget::Pan => String::from("pan"),
                        project::song::AutomationTarget::Send => String::from("send"),
                        project::song::AutomationTarget::Brightness => String::from("brightness"),
                        project::song::AutomationTarget::PluginParam { slot, parameter } => {
                            format!("plugin:{slot}:{parameter}")
                        }
                    },
                    "points": lane.points.iter().map(|point| json!({"tick": point.tick, "value": point.value})).collect::<Vec<_>>()
                })).collect::<Vec<_>>()
            }))
        }
        "set_automation" => {
            use project::song::{Automation, AutomationPoint, AutomationTarget};
            let track = track_argument(arguments)?;
            let lanes = arguments
                .get("lanes")
                .and_then(Value::as_array)
                .ok_or_else(|| ToolError::Arguments(String::from("lanes must be an array")))?;
            let mut parsed = Vec::new();
            for lane in lanes {
                let target = match lane.get("target").and_then(Value::as_str) {
                    Some("volume") => AutomationTarget::Volume,
                    Some("pan") => AutomationTarget::Pan,
                    Some("send") => AutomationTarget::Send,
                    Some("brightness") => AutomationTarget::Brightness,
                    Some(value) if value.starts_with("plugin:") => {
                        let mut parts = value.split(':').skip(1);
                        let slot = parts.next().and_then(|part| part.parse().ok());
                        let parameter = parts.next().and_then(|part| part.parse().ok());
                        match (slot, parameter) {
                            (Some(slot), Some(parameter)) => {
                                AutomationTarget::PluginParam { slot, parameter }
                            }
                            _ => {
                                return Err(ToolError::Arguments(format!(
                                    "bad plug-in target {value:?}; use plugin:slot:param"
                                )));
                            }
                        }
                    }
                    _ => {
                        return Err(ToolError::Arguments(String::from(
                            "lane target must be volume or pan",
                        )));
                    }
                };
                let points = lane
                    .get("points")
                    .and_then(Value::as_array)
                    .ok_or_else(|| ToolError::Arguments(String::from("lane needs points")))?;
                let mut points: Vec<AutomationPoint> = points
                    .iter()
                    .map(|point| {
                        let tick = point.get("tick").and_then(Value::as_i64).unwrap_or(0);
                        let value = point.get("value").and_then(Value::as_f64).unwrap_or(0.0);
                        AutomationPoint {
                            tick: tick.max(0),
                            value: value.clamp(0.0, 127.0) as f32,
                        }
                    })
                    .collect();
                points.sort_by_key(|point| point.tick);
                parsed.push(Automation { target, points });
            }
            session.apply(Command::SetAutomation {
                track,
                lanes: parsed.clone(),
            })?;
            Ok(json!({"lanes": parsed.len()}))
        }
        "get_lyrics" => Ok(json!({
            "lyrics": session.song().lyrics.iter().map(|event| json!({"tick": event.tick, "text": event.text})).collect::<Vec<_>>(),
            "marks": session.song().marks.iter().map(|mark| json!({"tick": mark.tick, "kind": format!("{:?}", mark.kind)})).collect::<Vec<_>>()
        })),
        "style_catalog" => {
            let root = arguments
                .get("root")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("root is required")))?;
            let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
            let entries = style_formats::catalog::catalog(std::path::Path::new(root))
                .map_err(ToolError::Arguments)?;
            Ok(json!({
                "total": entries.len(),
                "entries": entries.iter().take(limit).map(|entry| json!({
                    "number": entry.number,
                    "name": entry.name,
                    "path": entry.path.display().to_string()
                })).collect::<Vec<_>>()
            }))
        }
        "style_from_clips" => {
            let raw = arguments
                .get("out")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("out is required")))?;
            let out = checked_output(std::path::Path::new(raw))?;
            let section = arguments
                .get("section")
                .and_then(Value::as_str)
                .unwrap_or("Main A");
            let chord = arguments
                .get("chord")
                .and_then(Value::as_str)
                .unwrap_or("C");
            let length = arguments
                .get("length")
                .and_then(Value::as_u64)
                .unwrap_or(3840)
                .max(1) as i64;
            let from_markers = arguments
                .get("from_markers")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let version = match arguments
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("sff2")
            {
                "sff1" => style_formats::casm::SffVersion::Sff1,
                "sff2" => style_formats::casm::SffVersion::Sff2,
                other => {
                    return Err(ToolError::Arguments(format!(
                        "version must be sff1 or sff2, found {other:?}"
                    )));
                }
            };
            let parsed =
                Chord::parse(chord).map_err(|error| ToolError::Arguments(error.to_string()))?;
            let source = arranger::chord_type::ArrangerChord::try_from(&parsed)
                .map_err(|_| ToolError::Arguments(format!("unsupported style chord {chord:?}")))?;
            let mut style = if from_markers {
                arranger::style::from_song_sections(session.song(), &session.song().name, source)
            } else {
                arranger::style::from_song(
                    session.song(),
                    &session.song().name,
                    source,
                    Tick::from(length),
                )
            };
            if !from_markers && let Some(first) = style.sections.first_mut() {
                first.name = section.to_string();
            }
            let bytes = style_formats::write_native_style(&style, version)
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            std::fs::write(&out, &bytes)
                .map_err(|error| ToolError::Arguments(format!("{}: {error}", out.display())))?;
            Ok(json!({
                "out": out.display().to_string(),
                "bytes": bytes.len(),
                "sections": style.sections.iter().map(|entry| entry.name.clone()).collect::<Vec<_>>(),
                "parts": style.sections.first().map_or(0, |entry| entry.parts.len())
            }))
        }
        "audio_to_midi" => {
            let audio_path = arguments
                .get("audio")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("audio is required")))?;
            let bpm = arguments
                .get("bpm")
                .and_then(Value::as_f64)
                .unwrap_or_else(|| session.song().tempo_map.bpm_at(0))
                .max(1.0);
            let drums = arguments
                .get("mode")
                .and_then(Value::as_str)
                .is_some_and(|mode| mode == "drums");
            let minimum = arguments
                .get("level")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let decoded = audio::decode::decode_mono(std::path::Path::new(audio_path))
                .or_else(|_| {
                    let bytes = std::fs::read(audio_path)
                        .map_err(|error| format!("{audio_path}: {error}"))?;
                    audio::wav::decode_mono(&bytes)
                })
                .map_err(ToolError::Arguments)?;
            let detected = if drums {
                audio::analysis::to_drums(&decoded.samples, decoded.sample_rate)
            } else {
                audio::analysis::to_midi(&decoded.samples, decoded.sample_rate)
            };
            let channel = if drums { 9 } else { 0 };
            let notes: Vec<Value> = detected
                .iter()
                .filter(|note| f64::from(note.level) >= minimum)
                .map(|note| {
                    let (start, duration) = audio::analysis::as_ticks(note, bpm);
                    json!({
                        "start": start,
                        "duration": duration.max(1),
                        "pitch": note.pitch,
                        "channel": channel
                    })
                })
                .collect();
            Ok(json!({"notes": notes, "bpm": bpm, "mode": if drums { "drums" } else { "melody" }}))
        }
        "audio_tags" => {
            let audio_path = arguments
                .get("audio")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("audio is required")))?;
            let path = std::path::Path::new(audio_path);
            let decoded = match audio::decode::decode_mono(path) {
                Ok(decoded) => decoded,
                Err(_) => {
                    let bytes = std::fs::read(path).map_err(|error| {
                        ToolError::Arguments(format!("{}: {error}", path.display()))
                    })?;
                    audio::wav::decode_mono(&bytes).map_err(ToolError::Arguments)?
                }
            };
            match audio::analysis::tag_instrument(&decoded.samples, decoded.sample_rate) {
                Some(tag) => Ok(json!({
                    "instrument": tag.instrument.label(),
                    "confidence": tag.confidence
                })),
                None => Ok(json!({"instrument": null, "confidence": 0.0})),
            }
        }
        "groove_from_audio" => {
            let audio_path = arguments
                .get("audio")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("audio is required")))?;
            let name = arguments
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("name is required")))?;
            let dir = arguments
                .get("dir")
                .and_then(Value::as_str)
                .unwrap_or("grooves");
            let mut bpm = arguments
                .get("bpm")
                .and_then(Value::as_f64)
                .unwrap_or(120.0);
            let detect_tempo = arguments
                .get("detect_tempo")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let grid = arguments.get("grid").and_then(Value::as_u64).unwrap_or(16);
            let path = std::path::Path::new(audio_path);
            let decoded = match audio::decode::decode_mono(path) {
                Ok(decoded) => decoded,
                Err(_) => {
                    let bytes = std::fs::read(path).map_err(|error| {
                        ToolError::Arguments(format!("{}: {error}", path.display()))
                    })?;
                    audio::wav::decode_mono(&bytes).map_err(ToolError::Arguments)?
                }
            };
            if detect_tempo {
                bpm = audio::analysis::estimate_tempo(&decoded.samples, decoded.sample_rate)
                    .unwrap_or(bpm);
            }
            let detected = audio::analysis::to_midi(&decoded.samples, decoded.sample_rate);
            let channel = music_core::Channel::from_one_based(1)
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            let mut notes = Vec::new();
            for (index, note) in detected.iter().enumerate() {
                let (start, duration) = audio::analysis::as_ticks(note, bpm);
                if duration <= 0 {
                    continue;
                }
                notes.push(project::song::Note {
                    id: project::NoteId::new(index as u32 + 1)
                        .map_err(|error| ToolError::Arguments(error.to_string()))?,
                    start,
                    duration,
                    pitch: music_core::Pitch::new(note.pitch)
                        .map_err(|error| ToolError::Arguments(error.to_string()))?,
                    velocity: music_core::Velocity::new(
                        (note.level * 127.0).clamp(1.0, 127.0) as u8
                    )
                    .map_err(|error| ToolError::Arguments(error.to_string()))?,
                    channel,
                });
            }
            let bar = 4 * music_core::PPQ;
            let grid = (4 * music_core::PPQ / grid.max(1) as i64).max(1);
            let groove = arranging::groove::extract(&notes, bar, grid)
                .ok_or_else(|| ToolError::Arguments(String::from("no groove")))?;
            let path = arranging::groove_file::save(std::path::Path::new(dir), name, &groove)
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            Ok(json!({
                "path": path,
                "bpm": bpm,
                "notes": notes.len(),
                "slots": groove.offsets.len(),
                "offsets": groove.offsets
            }))
        }
        "style_from_audio" => {
            let audio_path = arguments
                .get("audio")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("audio is required")))?;
            let raw = arguments
                .get("out")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("out is required")))?;
            let out = checked_output(std::path::Path::new(raw))?;
            let bpm = arguments
                .get("bpm")
                .and_then(Value::as_f64)
                .unwrap_or(120.0);
            let section = arguments
                .get("section")
                .and_then(Value::as_str)
                .unwrap_or("Main A");
            let chord = arguments
                .get("chord")
                .and_then(Value::as_str)
                .unwrap_or("auto");
            let split_parts = arguments
                .get("split_parts")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let section_bars = arguments
                .get("section_bars")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            let drums = arguments
                .get("drums")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let detect_tempo = arguments
                .get("detect_tempo")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let version = match arguments
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("sff2")
            {
                "sff1" => style_formats::casm::SffVersion::Sff1,
                "sff2" => style_formats::casm::SffVersion::Sff2,
                other => {
                    return Err(ToolError::Arguments(format!(
                        "version must be sff1 or sff2, found {other:?}"
                    )));
                }
            };
            let path = std::path::Path::new(audio_path);
            let decoded = match audio::decode::decode_mono(path) {
                Ok(decoded) => decoded,
                Err(_) => {
                    let bytes = std::fs::read(path).map_err(|error| {
                        ToolError::Arguments(format!("{}: {error}", path.display()))
                    })?;
                    audio::wav::decode_mono(&bytes).map_err(ToolError::Arguments)?
                }
            };
            let source = if chord.eq_ignore_ascii_case("auto") {
                None
            } else {
                let parsed =
                    Chord::parse(chord).map_err(|error| ToolError::Arguments(error.to_string()))?;
                Some(
                    arranger::chord_type::ArrangerChord::try_from(&parsed).map_err(|_| {
                        ToolError::Arguments(format!("unsupported chord {chord:?}"))
                    })?,
                )
            };
            let built = style_formats::style_from_audio(
                &decoded,
                style_formats::AudioStyleOptions {
                    bpm,
                    section,
                    source_chord: source,
                    split_parts,
                    section_bars,
                    include_drums: drums,
                    detect_tempo,
                },
            )
            .map_err(ToolError::Arguments)?;
            let bytes = style_formats::write_native_style(&built.style, version)
                .map_err(|error| ToolError::Arguments(error.to_string()))?;
            std::fs::write(&out, &bytes)
                .map_err(|error| ToolError::Arguments(format!("{}: {error}", out.display())))?;
            Ok(json!({
                "out": out.display().to_string(),
                "source": built.source_chord.name(),
                "notes": built.notes,
                "sections": built.style.sections.iter().map(|entry| entry.name.clone()).collect::<Vec<_>>(),
                "bytes": bytes.len()
            }))
        }
        "theory" => {
            let op = arguments
                .get("op")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("op is required")))?;
            match op {
                "scale" => {
                    let tonic = parse_tonic_argument(arguments)?;
                    let mode = arguments
                        .get("mode")
                        .and_then(Value::as_str)
                        .unwrap_or("major");
                    let mode = music_core::theory::Mode::parse(mode)?;
                    Ok(json!({
                        "tonic": tonic.name_sharp(),
                        "mode": mode.name(),
                        "notes": music_core::theory::scale(tonic, mode).iter().map(|pitch| pitch.name_sharp()).collect::<Vec<_>>()
                    }))
                }
                "label" => {
                    let tonic = parse_tonic_argument(arguments)?;
                    let chords = chord_arguments(arguments)?;
                    Ok(json!({
                        "labels": chords.iter().map(|chord| music_core::theory::roman_label(*chord, tonic)).collect::<Vec<_>>(),
                        "cadence": format!("{:?}", music_core::theory::cadence(&chords, tonic))
                    }))
                }
                "voice_leading" => {
                    let from = arguments
                        .get("tonic")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            ToolError::Arguments(String::from("tonic must name the first chord"))
                        })?;
                    let to = arguments
                        .get("to")
                        .and_then(Value::as_str)
                        .ok_or_else(|| ToolError::Arguments(String::from("to is required")))?;
                    let from = music_core::Chord::parse(from)?;
                    let to = music_core::Chord::parse(to)?;
                    Ok(json!({"cost": music_core::theory::voice_leading_cost(from, to)}))
                }
                other => Err(ToolError::Arguments(format!("unknown theory op {other}"))),
            }
        }
        "edit_slot" => {
            let scene = arguments
                .get("scene")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Arguments(String::from("scene is required")))?;
            let track = track_argument(arguments)?;
            let action = arguments
                .get("action")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Arguments(String::from("action is required")))?;
            let grid = arranging::grid::from_timeline(session.song(), 2);
            let index = usize::try_from(scene).unwrap_or(0);
            match action {
                "clear" => {
                    arranging::grid::clear_slot(session, &grid, index, track)?;
                    Ok(json!({"action": "clear"}))
                }
                "copy" => {
                    let tick = arguments
                        .get("tick")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
                    let copied = arranging::grid::copy_slot(session, &grid, index, track, tick)?;
                    Ok(json!({"copied": copied}))
                }
                "length" => {
                    let length = arguments
                        .get("length")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| ToolError::Arguments(String::from("length is required")))?;
                    arranging::grid::set_slot_length(session, &grid, index, track, length)?;
                    Ok(json!({"length": length}))
                }
                other => Err(ToolError::Arguments(format!("unknown action {other}"))),
            }
        }
        "find_chords" => Ok(json!({"chords": arranging::chords::find_chords(session)?})),
        "arrangement_doctor" => {
            let proposals: Vec<Value> = arranging::analyze(session.song())
                .into_iter()
                .map(|proposal| {
                    json!({
                        "title": proposal.title,
                        "detail": proposal.detail,
                        "bar": proposal.bar,
                        "edits": proposal.edits.len(),
                    })
                })
                .collect();
            Ok(json!({"proposals": proposals}))
        }
        "extract_groove" => extract_groove(session, arguments),
        "list_grooves" => list_grooves(arguments),
        "save_groove" => save_groove(session, arguments),
        "apply_groove_file" => apply_groove_file(session, arguments),
        "apply_groove" => apply_groove(session, arguments),
        "make_song_form" => make_song_form(session, arguments),
        "reharmonize" => reharmonize(session, arguments),
        "add_notes" => add_notes(session, arguments),
        "set_chords" => set_chords(session, arguments),
        _ => Err(ToolError::UnknownTool(String::from(name))),
    }
}

fn plugin_path_allowed(path: &str, format: project::song::PluginFormat) -> bool {
    let Ok(path) = std::fs::canonicalize(path) else {
        return false;
    };
    let wanted = match format {
        project::song::PluginFormat::Vst3 => plugins::Format::Vst3,
        project::song::PluginFormat::AudioUnit => plugins::Format::AudioUnit,
    };
    plugins::default_roots().iter().any(|root| {
        root.format == wanted
            && std::fs::canonicalize(&root.path).is_ok_and(|base| path.starts_with(base))
    })
}

fn parse_tonic_argument(arguments: &Value) -> Result<music_core::PitchClass, ToolError> {
    let name = arguments
        .get("tonic")
        .and_then(Value::as_str)
        .unwrap_or("C");
    let with_octave = if name
        .chars()
        .last()
        .is_some_and(|last| last.is_ascii_digit())
    {
        name.to_string()
    } else {
        format!("{name}4")
    };
    let pitch = music_core::Pitch::from_name(&with_octave)?;
    Ok(music_core::PitchClass::new(i32::from(pitch.as_u8() % 12)))
}

fn chord_arguments(arguments: &Value) -> Result<Vec<music_core::Chord>, ToolError> {
    let chords = arguments
        .get("chords")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::Arguments(String::from("chords must be an array")))?;
    chords
        .iter()
        .map(|value| {
            let symbol = value
                .as_str()
                .ok_or_else(|| ToolError::Arguments(String::from("chords must be strings")))?;
            Ok(music_core::Chord::parse(symbol)?)
        })
        .collect()
}

fn track_plugins(
    session: &EditSession,
    track: TrackId,
) -> Result<Vec<project::song::PluginSlot>, ToolError> {
    Ok(session
        .song()
        .tracks
        .iter()
        .find(|entry| entry.id == track)
        .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?
        .plugins
        .clone())
}

fn clip_argument(arguments: &Value) -> Result<project::ClipId, ToolError> {
    let value = arguments
        .get("clip")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolError::Arguments(String::from("clip is required")))?;
    project::ClipId::new(u32::try_from(value).unwrap_or(0))
        .map_err(|error| ToolError::Arguments(error.to_string()))
}

fn track_clips(
    session: &EditSession,
    track: TrackId,
) -> Result<Vec<project::song::Clip>, ToolError> {
    Ok(session
        .song()
        .tracks
        .iter()
        .find(|entry| entry.id == track)
        .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?
        .clips
        .clone())
}

fn audio_comp_json(comp: &project::song::AudioComp) -> Value {
    json!({"take": comp.take, "start": comp.start, "length": comp.length})
}

fn audio_comp_from_value(value: &Value) -> Option<project::song::AudioComp> {
    Some(project::song::AudioComp {
        take: usize::try_from(value.get("take").and_then(Value::as_u64)?).ok()?,
        start: value.get("start").and_then(Value::as_i64).unwrap_or(0),
        length: value.get("length").and_then(Value::as_i64).unwrap_or(0),
    })
}

fn audio_ref_json(reference: &project::song::AudioRef) -> Value {
    json!({
        "path": reference.path,
        "start": reference.start,
        "length": reference.length,
        "offset": reference.offset,
        "fade_in": reference.fade_in,
        "fade_out": reference.fade_out,
        "stretch": reference.stretch
    })
}

fn audio_ref_from_value(value: &Value) -> Option<project::song::AudioRef> {
    let path = value.get("path").and_then(Value::as_str)?.to_string();
    if path.is_empty() {
        return None;
    }
    Some(project::song::AudioRef {
        path,
        start: value.get("start").and_then(Value::as_i64).unwrap_or(0),
        length: value.get("length").and_then(Value::as_i64).unwrap_or(0),
        offset: value.get("offset").and_then(Value::as_i64).unwrap_or(0),
        fade_in: value.get("fade_in").and_then(Value::as_i64).unwrap_or(0),
        fade_out: value.get("fade_out").and_then(Value::as_i64).unwrap_or(0),
        stretch: value
            .get("stretch")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn track_argument(arguments: &Value) -> Result<TrackId, ToolError> {
    let value = arguments
        .get("track")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolError::Arguments(String::from("track is required")))?;
    TrackId::new(u32::try_from(value).unwrap_or(0))
        .map_err(|error| ToolError::Arguments(error.to_string()))
}

fn extract_groove(session: &mut EditSession, arguments: &Value) -> Result<Value, ToolError> {
    let track = track_argument(arguments)?;
    let Some(source) = session.song().tracks.iter().find(|entry| entry.id == track) else {
        return Err(ToolError::Arguments(format!("unknown track {track}")));
    };
    let bar = session.song().time_signature.bar_ticks().max(1);
    let groove = arranging::groove::extract(&source.notes, bar, music_core::PPQ / 4)
        .ok_or_else(|| ToolError::Arguments(String::from("no groove")))?;
    Ok(json!({"grid": groove.grid, "offsets": groove.offsets, "levels": groove.levels}))
}

fn list_grooves(arguments: &Value) -> Result<Value, ToolError> {
    let directory = arguments
        .get("directory")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Arguments(String::from("directory is required")))?;
    let directory = checked_output(std::path::Path::new(&directory))?;
    let entries = arranging::groove_file::list(std::path::Path::new(&directory))
        .map_err(ToolError::Arguments)?;
    Ok(json!({
        "grooves": entries.iter().map(|(name, path)| json!({
            "name": name,
            "path": path.display().to_string()
        })).collect::<Vec<_>>()
    }))
}

fn save_groove(session: &mut EditSession, arguments: &Value) -> Result<Value, ToolError> {
    let track = track_argument(arguments)?;
    let name = arguments
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Arguments(String::from("name is required")))?;
    let directory = arguments
        .get("directory")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Arguments(String::from("directory is required")))?;
    let directory = checked_output(std::path::Path::new(&directory))?;
    let entry = session
        .song()
        .tracks
        .iter()
        .find(|entry| entry.id == track)
        .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?;
    let bar = session.song().time_signature.bar_ticks().max(1);
    let groove = arranging::groove::extract(&entry.notes, bar, music_core::PPQ / 4)
        .ok_or_else(|| ToolError::Arguments(String::from("track has no notes")))?;
    let path = arranging::groove_file::save(std::path::Path::new(&directory), name, &groove)
        .map_err(ToolError::Arguments)?;
    Ok(json!({"path": path.display().to_string()}))
}

fn apply_groove_file(session: &mut EditSession, arguments: &Value) -> Result<Value, ToolError> {
    let track = track_argument(arguments)?;
    let name = arguments
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Arguments(String::from("name is required")))?;
    let directory = arguments
        .get("directory")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Arguments(String::from("directory is required")))?;
    let directory = checked_output(std::path::Path::new(&directory))?;
    let timing = arguments
        .get("timing")
        .and_then(Value::as_f64)
        .unwrap_or(1.0) as f32;
    let accents = arguments
        .get("accents")
        .and_then(Value::as_f64)
        .unwrap_or(1.0) as f32;
    let entries = arranging::groove_file::list(std::path::Path::new(&directory))
        .map_err(ToolError::Arguments)?;
    let (_, path) = entries
        .into_iter()
        .find(|(found, _)| found == name)
        .ok_or_else(|| {
            ToolError::Arguments(format!(
                "no groove named {name:?} in {}",
                directory.display()
            ))
        })?;
    let (_, groove) = arranging::groove_file::load(&path).map_err(ToolError::Arguments)?;
    let notes = session
        .song()
        .tracks
        .iter()
        .find(|entry| entry.id == track)
        .ok_or_else(|| ToolError::Arguments(String::from("unknown track")))?
        .notes
        .clone();
    let applied = arranging::groove::apply(
        &notes,
        &groove,
        arranging::groove::Amounts { timing, accents },
    );
    let count = applied.len();
    session.apply(Command::ReplaceTrackNotes {
        track,
        notes: applied,
    })?;
    Ok(json!({"applied": count}))
}

fn apply_groove(session: &mut EditSession, arguments: &Value) -> Result<Value, ToolError> {
    let track = track_argument(arguments)?;
    let amounts = arranging::groove::Amounts {
        timing: arguments
            .get("timing")
            .and_then(Value::as_f64)
            .unwrap_or(1.0) as f32,
        accents: arguments
            .get("accents")
            .and_then(Value::as_f64)
            .unwrap_or(1.0) as f32,
    };
    let Some(groove) = extract_groove(session, arguments).ok().and_then(|_| {
        let source = session
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)?;
        let bar = session.song().time_signature.bar_ticks().max(1);
        arranging::groove::extract(&source.notes, bar, music_core::PPQ / 4)
    }) else {
        return Err(ToolError::Arguments(String::from(
            "no groove in that track",
        )));
    };
    let others: Vec<TrackId> = session
        .song()
        .tracks
        .iter()
        .filter(|entry| entry.id != track && !entry.notes.is_empty())
        .map(|entry| entry.id)
        .collect();
    let mut tracks = Vec::new();
    for other in others {
        let Some(entry) = session.song().tracks.iter().find(|entry| entry.id == other) else {
            continue;
        };
        tracks.push((
            other,
            arranging::groove::apply(&entry.notes, &groove, amounts),
        ));
    }
    let changed = tracks.len();
    if !tracks.is_empty() {
        session.apply(Command::ReplaceTrackNotesMany { tracks })?;
    }
    Ok(json!({"tracks": changed}))
}

fn make_song_form(session: &mut EditSession, arguments: &Value) -> Result<Value, ToolError> {
    let preset = arguments
        .get("preset")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Arguments(String::from("preset is required")))?;
    let sections = arranging::forms::sections(session.song());
    let form = match preset {
        "short" => arranging::forms::short(&sections),
        "gig" => arranging::forms::gig(&sections),
        "long_intro" => arranging::forms::long_intro(&sections),
        "instrumental" => arranging::forms::instrumental(&sections),
        other => return Err(ToolError::Arguments(format!("unknown preset {other}"))),
    };
    let copied = arranging::forms::apply(session, &sections, &form)?;
    Ok(json!({"form": form.name, "notes": copied}))
}

fn reharmonize(session: &mut EditSession, arguments: &Value) -> Result<Value, ToolError> {
    let tick = arguments
        .get("tick")
        .and_then(Value::as_i64)
        .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
    let kind_name = arguments
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Arguments(String::from("kind is required")))?;
    let kind = match kind_name {
        "tritone" => music_core::reharmonize::Kind::TritoneSubstitute,
        "borrowed" => music_core::reharmonize::Kind::Borrowed,
        "two_five" => music_core::reharmonize::Kind::TwoFive,
        "relative" => music_core::reharmonize::Kind::Relative,
        other => return Err(ToolError::Arguments(format!("unknown kind {other}"))),
    };
    let Some(original) = session
        .song()
        .chords
        .iter()
        .find(|event| event.tick == tick)
        .map(|event| event.chord)
    else {
        return Err(ToolError::Arguments(String::from("no chord at that tick")));
    };
    let chords = music_core::reharmonize::apply(original, kind);
    let bar = session.song().time_signature.bar_ticks().max(1);
    let step = bar / chords.len() as Tick;
    let mut events = session.song().chords.clone();
    if let Some(index) = events.iter().position(|event| event.tick == tick) {
        events.remove(index);
    }
    for (index, chord) in chords.iter().enumerate() {
        events.push(project::song::ChordEvent {
            tick: tick + (index as Tick) * step,
            chord: *chord,
        });
    }
    events.sort_by_key(|event| event.tick);
    session.apply(Command::SetChords { chords: events })?;
    Ok(json!({"chords": chords.len()}))
}

fn song_summary(song: &project::song::Song) -> Value {
    let tracks: Vec<Value> = song
        .tracks
        .iter()
        .map(|track| {
            json!({
                "id": track.id.get(),
                "name": track.name,
                "notes": track.notes.len(),
                "clips": track.clips.len(),
            })
        })
        .collect();
    json!({
        "name": song.name,
        "tempo": song.tempo_map.bpm_at(0),
        "meter": [song.time_signature.numerator, song.time_signature.denominator],
        "key": song.key.map(|key| json!({"tonic": key.tonic.as_u8(), "minor": key.minor})),
        "tracks": tracks,
        "chords": song.chords.len(),
        "markers": song.markers.len(),
        "end_tick": song.end_tick(),
    })
}

fn add_notes(session: &mut EditSession, arguments: &Value) -> Result<Value, ToolError> {
    let track_value = arguments
        .get("track")
        .and_then(Value::as_u64)
        .ok_or_else(|| ToolError::Arguments(String::from("track is required")))?;
    let track = TrackId::new(u32::try_from(track_value).unwrap_or(0))
        .map_err(|error| ToolError::Arguments(error.to_string()))?;
    let Some(existing) = session.song().tracks.iter().find(|entry| entry.id == track) else {
        return Err(ToolError::Arguments(format!("unknown track {track}")));
    };
    let array = arguments
        .get("notes")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::Arguments(String::from("notes must be an array")))?;
    if array.len() > 100_000 {
        return Err(ToolError::Arguments(String::from(
            "too many notes in one call",
        )));
    }
    let mut notes = existing.notes.clone();
    let mut highest = notes.iter().map(|note| note.id.get()).max().unwrap_or(0);
    let mut added = 0;
    for entry in array {
        highest = highest
            .checked_add(1)
            .ok_or_else(|| ToolError::Arguments(String::from("note id overflow")))?;
        let start = entry
            .get("start")
            .and_then(Value::as_i64)
            .ok_or_else(|| ToolError::Arguments(String::from("start is required")))?;
        let duration = entry
            .get("duration")
            .and_then(Value::as_i64)
            .filter(|duration| *duration > 0)
            .ok_or_else(|| ToolError::Arguments(String::from("duration must be positive")))?;
        let pitch = entry
            .get("pitch")
            .and_then(Value::as_u64)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| ToolError::Arguments(String::from("pitch is required")))?;
        let velocity = u8::try_from(entry.get("velocity").and_then(Value::as_u64).unwrap_or(100))
            .unwrap_or(100);
        let channel =
            u8::try_from(entry.get("channel").and_then(Value::as_u64).unwrap_or(0)).unwrap_or(0);
        notes.push(Note {
            id: NoteId::new(highest).map_err(|error| ToolError::Arguments(error.to_string()))?,
            start,
            duration,
            pitch: Pitch::new(pitch).map_err(|error| ToolError::Arguments(error.to_string()))?,
            velocity: Velocity::new(velocity)
                .map_err(|error| ToolError::Arguments(error.to_string()))?,
            channel: Channel::new(channel)
                .map_err(|error| ToolError::Arguments(error.to_string()))?,
        });
        added += 1;
    }
    session.apply(Command::ReplaceTrackNotes { track, notes })?;
    Ok(json!({"added": added}))
}

fn set_chords(session: &mut EditSession, arguments: &Value) -> Result<Value, ToolError> {
    let array = arguments
        .get("chords")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::Arguments(String::from("chords must be an array")))?;
    let mut chords = Vec::new();
    for entry in array {
        let tick: Tick = entry
            .get("tick")
            .and_then(Value::as_i64)
            .ok_or_else(|| ToolError::Arguments(String::from("tick is required")))?;
        let symbol = entry
            .get("chord")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Arguments(String::from("chord is required")))?;
        chords.push(ChordEvent {
            tick,
            chord: Chord::parse(symbol).map_err(|error| ToolError::Arguments(error.to_string()))?,
        });
    }
    let count = chords.len();
    session.apply(Command::SetChords { chords })?;
    Ok(json!({"chords": count}))
}

/// Opens a project for tool calls.
///
/// # Errors
///
/// Returns a message when the project is invalid.
pub fn session(project: ProjectFile) -> Result<EditSession, String> {
    EditSession::new(project).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::PPQ;

    fn session_with_track() -> EditSession {
        let mut song = project::song::Song::new("Tools", 120.0);
        song.tracks.push(project::song::Track::new(
            TrackId::new(1).expect("valid"),
            "Piano",
        ));
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn the_tool_list_is_json_with_schemas() {
        let list = tool_list_json();
        let names: Vec<&str> = list
            .as_array()
            .expect("array")
            .iter()
            .map(|tool| tool["name"].as_str().expect("name"))
            .collect();
        assert!(names.contains(&"get_song"));
        assert!(names.contains(&"add_notes"));
        assert!(list[0]["inputSchema"].is_object());
    }

    #[test]
    fn automation_is_read_and_written_through_tools() {
        let mut session = session_with_track();
        let written = call(
            &mut session,
            "set_automation",
            &json!({
                "track": 1,
                "lanes": [{
                    "target": "volume",
                    "points": [
                        {"tick": 960, "value": 100.0},
                        {"tick": 0, "value": 0.0}
                    ]
                }]
            }),
        )
        .expect("set");
        assert_eq!(written["lanes"], 1);
        let read = call(&mut session, "get_automation", &json!({"track": 1})).expect("get");
        assert_eq!(read["lanes"][0]["target"], "volume");
        assert_eq!(
            read["lanes"][0]["points"][0]["tick"], 0,
            "points are sorted"
        );
        assert!(
            call(
                &mut session,
                "set_automation",
                &json!({"track": 1, "lanes": [{"target": "tempo"}]})
            )
            .is_err()
        );
    }

    #[test]
    fn korg_scan_tool_recognises_a_marker() {
        let path = std::env::temp_dir().join("lazy-arranger-korg-scan.pcg");
        std::fs::write(&path, b"KORG\0\0KRONOS").expect("write");
        let mut session = session_with_track();
        let result = call(
            &mut session,
            "korg_scan",
            &json!({"path": path.display().to_string()}),
        )
        .expect("scan");
        assert_eq!(result["korg_marker"], true);
        assert_eq!(result["model"], "KRONOS");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_registry_has_the_documented_tool_count() {
        assert_eq!(tools().len(), 104);
        let names: Vec<&str> = tools().iter().map(|tool| tool.name).collect();
        assert!(names.contains(&"get_song"));
        assert!(names.contains(&"set_time_signature"));
    }

    #[test]
    fn every_registered_tool_has_a_handler() {
        let mut session = session_with_track();
        for tool in tools() {
            if let Err(ToolError::UnknownTool(name)) = call(&mut session, tool.name, &json!({})) {
                panic!("{name} has no handler");
            }
        }
    }

    #[test]
    fn single_object_read_tools_work() {
        let mut session = session_with_track();
        let track = call(&mut session, "get_track", &json!({"track": 1})).expect("track");
        assert_eq!(track["name"], "Piano");
        let added = call(
            &mut session,
            "add_note",
            &json!({"track": 1, "start": 0, "pitch": 67}),
        )
        .expect("note");
        let note = call(
            &mut session,
            "get_note",
            &json!({"track": 1, "note": added["note"]}),
        )
        .expect("get note");
        assert_eq!(note["pitch"], 67);
        call(
            &mut session,
            "set_audio_ref",
            &json!({"track": 1, "path": "/tmp/take.wav", "length": 960, "stretch": true,
                "takes": [{"path": "/tmp/old.wav", "length": 960}],
                "comps": [{"take": 0, "start": 0, "length": 480}]}),
        )
        .expect("audio");
        let audio = call(&mut session, "get_audio_ref", &json!({"track": 1})).expect("audio ref");
        assert_eq!(audio["audio"]["stretch"], true);
        assert_eq!(audio["takes"][0]["path"], "/tmp/old.wav");
        assert_eq!(audio["comps"][0]["length"], 480);
        assert!(call(&mut session, "get_note", &json!({"track": 1, "note": 99})).is_err());
    }

    #[test]
    fn note_and_chord_edit_tools_work() {
        let mut session = session_with_track();
        let added = call(
            &mut session,
            "add_note",
            &json!({"track": 1, "start": 0, "pitch": 64, "velocity": 90}),
        )
        .expect("add note");
        let note = added["note"].as_u64().expect("id");
        assert_eq!(session.song().tracks[0].notes.len(), 1);
        call(
            &mut session,
            "remove_note",
            &json!({"track": 1, "note": note}),
        )
        .expect("remove note");
        assert!(session.song().tracks[0].notes.is_empty());
        call(
            &mut session,
            "set_chord",
            &json!({"tick": 0, "symbol": "Am"}),
        )
        .expect("chord");
        assert_eq!(
            session.song().chords[0]
                .chord
                .format(music_core::Spelling::Sharp),
            "Am"
        );
        call(&mut session, "remove_chord", &json!({"tick": 0})).expect("remove chord");
        assert!(session.song().chords.is_empty());
        assert!(
            call(
                &mut session,
                "set_chord",
                &json!({"tick": 0, "symbol": "H"})
            )
            .is_err()
        );
    }

    #[test]
    fn time_signature_tool_changes_meter() {
        let mut session = session_with_track();
        call(
            &mut session,
            "set_time_signature",
            &json!({"numerator": 6, "denominator": 8}),
        )
        .expect("meter");
        let meter = call(&mut session, "get_time_signature", &json!({})).expect("get");
        assert_eq!(meter["numerator"], 6);
        assert_eq!(meter["bar_ticks"], 2880);
        assert!(
            call(
                &mut session,
                "set_time_signature",
                &json!({"numerator": 4, "denominator": 3})
            )
            .is_err()
        );
    }

    #[test]
    fn song_name_tools_work() {
        let mut session = session_with_track();
        call(&mut session, "set_song_name", &json!({"name": "My Song"})).expect("rename");
        let name = call(&mut session, "get_song_name", &json!({})).expect("name");
        assert_eq!(name["name"], "My Song");
        call(&mut session, "undo", &json!({})).expect("undo");
        assert_ne!(session.song().name, "My Song");
    }

    #[test]
    fn bus_plugin_and_scene_tools_work() {
        let mut session = session_with_track();
        let bus = call(&mut session, "add_bus", &json!({"name": "Reverb"})).expect("bus");
        let bus_id = bus["bus"].as_u64().expect("id");
        assert_eq!(session.song().buses.len(), 1);
        call(&mut session, "remove_bus", &json!({"bus": bus_id})).expect("remove bus");
        assert!(session.song().buses.is_empty());
        let plugin = call(
            &mut session,
            "add_plugin",
            &json!({"track": 1, "format": "vst3", "name": "R0Verb"}),
        )
        .expect("plugin");
        assert!(
            call(
                &mut session,
                "add_plugin",
                &json!({
                    "track": 1,
                    "format": "vst3",
                    "path": "/tmp/not-a-plugin.vst3"
                }),
            )
            .is_err()
        );
        let plugin_id = plugin["plugin"].as_u64().expect("id");
        let slots = call(&mut session, "get_plugin_slots", &json!({"track": 1})).expect("slots");
        assert_eq!(slots["plugins"][0]["format"], "vst3");
        call(
            &mut session,
            "remove_plugin",
            &json!({"track": 1, "plugin": plugin_id}),
        )
        .expect("remove plugin");
        call(
            &mut session,
            "set_scene_name",
            &json!({"scene": 1, "name": "Verse"}),
        )
        .expect("scene");
        let name = call(&mut session, "get_scene_name", &json!({"scene": 1})).expect("get scene");
        assert_eq!(name["name"], "Verse");
        call(
            &mut session,
            "add_marker",
            &json!({"tick": 960, "name": "Chorus"}),
        )
        .expect("marker");
        let marker = call(&mut session, "get_marker_at", &json!({"tick": 960})).expect("marker at");
        assert_eq!(marker["marker"]["name"], "Chorus");
    }

    #[test]
    fn clip_and_pdf_tools_work() {
        let directory = std::env::temp_dir().join("lazy-arranger-mcp-clips");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("dir");
        let mut session = session_with_track();
        call(
            &mut session,
            "add_note",
            &json!({"track": 1, "start": 0, "pitch": 60}),
        )
        .expect("note");
        let added = call(
            &mut session,
            "add_clip",
            &json!({"track": 1, "start": 0, "length": 1920}),
        )
        .expect("add clip");
        let clip = added["clip"].as_u64().expect("id");
        let read =
            call(&mut session, "get_clip", &json!({"track": 1, "clip": clip})).expect("get clip");
        assert_eq!(read["length"], 1920);
        call(
            &mut session,
            "move_clip",
            &json!({"track": 1, "clip": clip, "start": 960}),
        )
        .expect("move clip");
        call(
            &mut session,
            "remove_clip",
            &json!({"track": 1, "clip": clip}),
        )
        .expect("remove clip");
        let pdf = directory.join("score.pdf");
        let exported = call(
            &mut session,
            "export_pdf",
            &json!({"path": pdf.display().to_string(), "title": "Test"}),
        )
        .expect("pdf");
        assert!(exported["bytes"].as_u64().unwrap_or(0) > 0);
        assert!(std::fs::metadata(&pdf).is_ok_and(|meta| meta.len() > 0));
        let full = directory.join("full.pdf");
        let full_export = call(
            &mut session,
            "export_pdf",
            &json!({"path": full.display().to_string(), "all_tracks": true}),
        )
        .expect("full pdf");
        assert!(full_export["pages"].as_u64().unwrap_or(0) >= 1);
        assert!(std::fs::metadata(&full).is_ok_and(|meta| meta.len() > 0));
        let selected = directory.join("selected.pdf");
        call(
            &mut session,
            "export_pdf",
            &json!({
                "path": selected.display().to_string(),
                "all_tracks": true,
                "tracks": [1]
            }),
        )
        .expect("selected full pdf");
        assert!(std::fs::metadata(&selected).is_ok_and(|meta| meta.len() > 0));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn marker_lyric_and_query_tools_work() {
        let mut session = session_with_track();
        call(
            &mut session,
            "add_marker",
            &json!({"tick": 0, "name": "Intro"}),
        )
        .expect("marker");
        assert_eq!(session.song().markers[0].name, "Intro");
        call(&mut session, "remove_marker", &json!({"tick": 0})).expect("remove marker");
        assert!(session.song().markers.is_empty());
        call(&mut session, "add_lyric", &json!({"tick": 0, "text": "la"})).expect("lyric");
        assert_eq!(session.song().lyrics.len(), 1);
        call(&mut session, "remove_lyric", &json!({"tick": 0})).expect("remove lyric");
        call(
            &mut session,
            "set_chord",
            &json!({"tick": 0, "symbol": "Dm7"}),
        )
        .expect("chord");
        let chord = call(&mut session, "get_chord_at", &json!({"tick": 0})).expect("chord query");
        assert_eq!(chord["chord"], "Dm7");
        call(
            &mut session,
            "add_note",
            &json!({"track": 1, "start": 0, "pitch": 64}),
        )
        .expect("note");
        let note = call(
            &mut session,
            "get_note_at",
            &json!({"track": 1, "tick": 10}),
        )
        .expect("note query");
        assert_eq!(note["note"]["pitch"], 64);
        let meter = call(&mut session, "get_time_signature", &json!({})).expect("meter");
        assert_eq!(meter["numerator"], 4);
    }

    #[test]
    fn groove_file_tools_round_trip() {
        let directory = std::env::temp_dir().join("lazy-arranger-mcp-grooves");
        let _ = std::fs::remove_dir_all(&directory);
        let mut session = session_with_track();
        call(
            &mut session,
            "add_note",
            &json!({"track": 1, "start": 0, "pitch": 60}),
        )
        .expect("note");
        call(
            &mut session,
            "save_groove",
            &json!({"track": 1, "name": "Test", "directory": directory.display().to_string()}),
        )
        .expect("save");
        let listed = call(
            &mut session,
            "list_grooves",
            &json!({"directory": directory.display().to_string()}),
        )
        .expect("list");
        assert_eq!(listed["grooves"][0]["name"], "Test");
        call(
            &mut session,
            "apply_groove_file",
            &json!({"track": 1, "name": "Test", "directory": directory.display().to_string()}),
        )
        .expect("apply");
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn lyrics_marks_and_patch_tools_work() {
        let mut session = session_with_track();
        call(
            &mut session,
            "set_lyrics",
            &json!({"lyrics": [{"tick": 0, "text": "la"}, {"tick": 960, "text": "da"}]}),
        )
        .expect("lyrics");
        assert_eq!(session.song().lyrics.len(), 2);
        call(
            &mut session,
            "set_marks",
            &json!({"marks": [{"tick": 0, "kind": "ff"}, {"tick": 960, "kind": "staccato"}]}),
        )
        .expect("marks");
        let marks = call(&mut session, "get_marks", &json!({})).expect("get marks");
        assert_eq!(marks["marks"][0]["kind"], "ff");
        call(
            &mut session,
            "set_patch",
            &json!({"track": 1, "patch": {"waveform": "square", "brightness": 0.7}}),
        )
        .expect("patch");
        let patch = call(&mut session, "get_patch", &json!({"track": 1})).expect("get patch");
        assert_eq!(patch["patch"]["waveform"], "square");
        call(&mut session, "set_patch", &json!({"track": 1})).expect("clear patch");
        assert!(
            call(
                &mut session,
                "set_marks",
                &json!({"marks": [{"tick": 0, "kind": "loud"}]})
            )
            .is_err()
        );
    }

    #[test]
    fn export_tools_write_files() {
        let directory = std::env::temp_dir().join("lazy-arranger-mcp-exports");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("dir");
        let mut session = session_with_track();
        let project = directory.join("project.json");
        call(
            &mut session,
            "save_project",
            &json!({"path": project.display().to_string()}),
        )
        .expect("save");
        let midi = directory.join("song.mid");
        let saved = call(
            &mut session,
            "export_midi",
            &json!({"path": midi.display().to_string()}),
        )
        .expect("midi");
        assert!(saved["bytes"].as_u64().unwrap_or(0) > 0);
        let xml = directory.join("song.xml");
        call(
            &mut session,
            "export_musicxml",
            &json!({"path": xml.display().to_string(), "title": "Test"}),
        )
        .expect("xml");
        let wav = directory.join("song.wav");
        call(
            &mut session,
            "render_wav",
            &json!({"path": wav.display().to_string(), "sample_rate": 8000}),
        )
        .expect("wav");
        let stems = directory.join("stems");
        let exported = call(
            &mut session,
            "export_stems",
            &json!({"directory": stems.display().to_string(), "sample_rate": 8000}),
        )
        .expect("stems");
        for path in [&project, &midi, &xml, &wav, &stems.join("stems.txt")] {
            assert!(
                std::fs::metadata(path).is_ok_and(|metadata| metadata.len() > 0),
                "{}",
                path.display()
            );
        }
        assert_eq!(exported["stems"], 1);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn audio_to_midi_tool_returns_melody_notes() {
        let mut session = session_with_track();
        let path =
            std::env::temp_dir().join(format!("lazy-arranger-mcp-midi-{}.wav", std::process::id()));
        let rate = 48_000_u32;
        let mut samples = vec![0.0_f32; rate as usize];
        for (index, sample) in samples.iter_mut().enumerate() {
            let time = index as f32 / rate as f32;
            *sample = (std::f32::consts::TAU * 440.0 * time).sin() * 0.5;
        }
        let wav = audio::RenderedAudio {
            samples,
            sample_rate: rate,
        }
        .to_wav();
        std::fs::write(&path, wav).expect("wav");
        let result = call(
            &mut session,
            "audio_to_midi",
            &json!({"audio": path.display().to_string(), "bpm": 120.0}),
        )
        .expect("transcribe");
        let _ = std::fs::remove_file(&path);
        assert_eq!(result["mode"], "melody");
        assert!(
            result["notes"]
                .as_array()
                .is_some_and(|notes| !notes.is_empty())
        );
    }

    #[test]
    fn slur_tools_work() {
        let mut session = session_with_track();
        call(
            &mut session,
            "set_slurs",
            &json!({"track": 1, "slurs": [{"start": 0, "end": 960}]}),
        )
        .expect("slurs");
        let slurs = call(&mut session, "get_slurs", &json!({"track": 1})).expect("get");
        assert_eq!(slurs["slurs"][0]["end"], 960);
        assert_eq!(session.song().tracks[0].slurs.len(), 1);
    }

    #[test]
    fn track_edit_tools_work() {
        let mut session = session_with_track();
        let track = TrackId::new(1).expect("valid");
        call(
            &mut session,
            "set_track_name",
            &json!({"track": 1, "name": "Strings"}),
        )
        .expect("name");
        assert_eq!(session.song().tracks[0].name, "Strings");
        call(
            &mut session,
            "set_track_gain",
            &json!({"track": 1, "volume": 80, "pan": 30}),
        )
        .expect("gain");
        assert_eq!(session.song().tracks[0].volume, 80);
        call(
            &mut session,
            "set_track_flags",
            &json!({"track": 1, "muted": true, "soloed": false, "armed": true}),
        )
        .expect("flags");
        assert!(session.song().tracks[0].muted && session.song().tracks[0].armed);
        call(
            &mut session,
            "set_track_program",
            &json!({"track": 1, "program": 48}),
        )
        .expect("program");
        assert_eq!(session.song().tracks[0].program, Some(48));
        let before = session.undo_depth();
        call(
            &mut session,
            "set_audio_ref",
            &json!({
                "track": 1,
                "path": "/tmp/take.wav",
                "length": 960,
                "fade_in": 120,
                "takes": [{"path": "/tmp/take.wav", "length": 960}],
                "comps": [{"take": 0, "start": 0, "length": 480}]
            }),
        )
        .expect("audio");
        assert_eq!(session.undo_depth(), before + 1);
        let (fade_in, length, takes, comps) = {
            let track = &session.song().tracks[0];
            let reference = track.audio.as_ref().expect("audio");
            (
                reference.fade_in,
                reference.length,
                track.takes.len(),
                track.comps.len(),
            )
        };
        assert_eq!(fade_in, 120);
        assert_eq!(length, 960);
        assert_eq!(takes, 1);
        assert_eq!(comps, 1);
        call(&mut session, "set_audio_ref", &json!({"track": 1})).expect("clear");
        assert!(session.song().tracks[0].audio.is_none());
        let _ = track;
    }

    #[test]
    fn freeze_and_restore_tools_work() {
        let mut session = session_with_track();
        call(
            &mut session,
            "add_note",
            &json!({"track": 1, "start": 0, "pitch": 60}),
        )
        .expect("note");
        call(
            &mut session,
            "freeze_track",
            &json!({"track": 1, "path": "/tmp/frozen.wav", "length": 960}),
        )
        .expect("freeze");
        assert!(session.song().tracks[0].notes.is_empty());
        assert!(session.song().tracks[0].audio.is_some());
        call(
            &mut session,
            "restore_track",
            &json!({
                "track": 1,
                "notes": [{"start": 0, "duration": 960, "pitch": 60}],
                "audio": null
            }),
        )
        .expect("restore");
        assert_eq!(session.song().tracks[0].notes.len(), 1);
        assert!(session.song().tracks[0].audio.is_none());
    }

    #[test]
    fn song_key_tools_work() {
        let mut session = session_with_track();
        call(
            &mut session,
            "set_song_key",
            &json!({"tonic": 9, "minor": true}),
        )
        .expect("set key");
        let song = call(&mut session, "get_song", &json!({})).expect("song");
        assert_eq!(song["key"]["tonic"], 9);
        assert_eq!(song["key"]["minor"], true);
        call(&mut session, "set_song_key", &json!({"tonic": null})).expect("clear");
        let song = call(&mut session, "get_song", &json!({})).expect("song");
        assert!(song["key"].is_null());
    }

    #[test]
    fn bank_remove_and_redo_tools_work() {
        let mut session = session_with_track();
        call(
            &mut session,
            "set_track_bank",
            &json!({"track": 1, "msb": 63, "lsb": 1}),
        )
        .expect("bank");
        let tracks = call(&mut session, "get_tracks", &json!({})).expect("tracks");
        assert_eq!(tracks["tracks"][0]["bank"]["msb"], 63);
        call(
            &mut session,
            "add_note",
            &json!({"track": 1, "start": 0, "pitch": 60}),
        )
        .expect("note");
        call(&mut session, "undo", &json!({})).expect("undo");
        call(&mut session, "redo", &json!({})).expect("redo");
        assert_eq!(session.song().tracks[0].notes.len(), 1);
        call(&mut session, "remove_track", &json!({"track": 1})).expect("remove");
        assert!(session.song().tracks.is_empty());
        call(&mut session, "undo", &json!({})).expect("restore");
        assert_eq!(session.song().tracks.len(), 1);
    }

    #[test]
    fn track_note_chord_marker_clip_and_scene_tools_work() {
        use project::Marker;
        let mut session = session_with_track();
        session
            .apply(Command::SetMarkers {
                markers: vec![Marker {
                    tick: 0,
                    name: String::from("Intro"),
                }],
            })
            .expect("markers");
        let tracks = call(&mut session, "get_tracks", &json!({})).expect("tracks");
        assert_eq!(tracks["tracks"][0]["name"], "Piano");
        session
            .apply(Command::InsertNote {
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
        let notes = call(&mut session, "get_notes", &json!({"track": 1})).expect("notes");
        assert_eq!(notes["notes"][0]["pitch"], 60);
        session
            .apply(Command::SetChords {
                chords: vec![project::song::ChordEvent {
                    tick: 0,
                    chord: music_core::Chord::parse("C").expect("valid"),
                }],
            })
            .expect("chord");
        let chords = call(&mut session, "get_chords", &json!({})).expect("chords");
        assert_eq!(chords["chords"][0]["symbol"], "C");
        let markers = call(&mut session, "get_markers", &json!({})).expect("markers");
        assert_eq!(markers["markers"][0]["name"], "Intro");
        call(
            &mut session,
            "set_clips",
            &json!({"track": 1, "clips": [{"name": "A", "start": 0, "length": 3840}]}),
        )
        .expect("clips");
        let clips = call(&mut session, "get_clips", &json!({"track": 1})).expect("get clips");
        assert_eq!(clips["clips"][0]["name"], "A");
        call(
            &mut session,
            "set_scene_names",
            &json!({"names": ["Verse"]}),
        )
        .expect("scenes");
        let scenes = call(&mut session, "get_scene_names", &json!({})).expect("get scenes");
        assert_eq!(scenes["names"][0], "Verse");
    }

    #[test]
    fn plugin_scan_finds_synthetic_bundles() {
        let directory = std::env::temp_dir().join("lazy-arranger-mcp-plugins");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("Demo.vst3")).expect("bundle");
        let mut session = session_with_track();
        let result = call(
            &mut session,
            "plugin_scan",
            &json!({"directories": [directory.display().to_string()]}),
        )
        .expect("scan");
        assert_eq!(result["plugins"][0]["name"], "Demo");
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn soundfont_bus_and_routing_tools_work() {
        let mut session = session_with_track();
        call(
            &mut session,
            "set_buses",
            &json!({"buses": [{"id": 1, "name": "Reverb", "volume": 90, "pan": 64, "muted": true, "soloed": true}]}),
        )
        .expect("buses");
        let buses = call(&mut session, "get_buses", &json!({})).expect("get buses");
        assert_eq!(buses["buses"][0]["name"], "Reverb");
        assert_eq!(buses["buses"][0]["muted"], true);
        assert_eq!(buses["buses"][0]["soloed"], true);
        call(
            &mut session,
            "set_track_routing",
            &json!({"track": 1, "bus": 1, "send": 1, "send_level": 80}),
        )
        .expect("routing");
        assert_eq!(session.song().tracks[0].bus, Some(1));
        assert_eq!(session.song().tracks[0].send_level, 80);
        call(
            &mut session,
            "set_soundfont",
            &json!({"track": 1, "path": "/tmp/bank.sf2", "bank": 0, "program": 4}),
        )
        .expect("soundfont");
        let fonts = call(&mut session, "get_soundfonts", &json!({})).expect("fonts");
        assert_eq!(fonts["tracks"][0]["soundfont"]["program"], 4);
        call(&mut session, "set_soundfont", &json!({"track": 1})).expect("clear");
        assert!(session.song().tracks[0].soundfont.is_none());
    }

    #[test]
    fn tempo_tools_work() {
        let mut session = session_with_track();
        call(&mut session, "set_tempo", &json!({"tick": 0, "bpm": 90.0})).expect("tempo");
        let changes = call(&mut session, "get_tempo", &json!({})).expect("tempo map");
        assert_eq!(changes["changes"].as_array().expect("array").len(), 1);
        assert!((session.song().tempo_map.bpm_at(0) - 90.0).abs() < 0.01);
        assert!(call(&mut session, "set_tempo", &json!({"tick": 0})).is_err());
    }

    #[test]
    fn quantise_and_velocity_tools_work() {
        let mut session = session_with_track();
        session
            .apply(Command::InsertNote {
                track: TrackId::new(1).expect("valid"),
                note: project::song::Note {
                    id: project::NoteId::new(1).expect("valid"),
                    start: 130,
                    duration: music_core::PPQ,
                    pitch: music_core::Pitch::from_name("C4").expect("valid"),
                    velocity: music_core::Velocity::new(100).expect("valid"),
                    channel: music_core::Channel::first(),
                },
            })
            .expect("note");
        let quantised = call(&mut session, "quantize", &json!({"grid": 240})).expect("quantise");
        assert_eq!(quantised["tracks_changed"], 1);
        assert_eq!(session.song().tracks[0].notes[0].start, 240);
        let velocity = call(
            &mut session,
            "set_velocity",
            &json!({"track": 1, "note": 1, "velocity": 70}),
        )
        .expect("velocity");
        assert_eq!(velocity["velocity"], 70);
        assert!(call(&mut session, "quantize", &json!({})).is_err());
    }

    #[test]
    fn plugin_and_l6max_tools_work() {
        let mut session = session_with_track();
        let written = call(
            &mut session,
            "set_plugins",
            &json!({"track": 1, "plugins": [{"id": 1, "name": "Limiter", "format": "vst3"}]}),
        )
        .expect("set plugins");
        assert_eq!(written["plugins"], 1);
        let read = call(&mut session, "get_plugins", &json!({"track": 1})).expect("get plugins");
        assert_eq!(read["plugins"][0]["name"], "Limiter");
        assert_eq!(read["plugins"][0]["format"], "vst3");
        assert!(
            call(
                &mut session,
                "set_plugins",
                &json!({"track": 1, "plugins": [{"id": 1, "name": "X", "format": "clap"}]})
            )
            .is_err()
        );

        let folder = std::env::temp_dir().join("lazy-arranger-mcp-l6max");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("dir");
        let wav = audio::RenderedAudio {
            samples: vec![0.0; 48_000],
            sample_rate: 48_000,
        };
        std::fs::write(folder.join("take.wav"), wav.to_wav()).expect("write");
        let scan = call(
            &mut session,
            "l6max_scan",
            &json!({"folder": folder.display().to_string()}),
        )
        .expect("scan");
        assert_eq!(scan["takes"][0]["name"], "take");
        assert_eq!(scan["takes"][0]["float32"], false);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn generation_tools_drive_the_generators() {
        let mut session = session_with_track();
        let result = call(
            &mut session,
            "generate",
            &json!({"track": 1, "op": "melody", "seed": 3}),
        )
        .expect("melody without chords writes nothing");
        assert_eq!(result["notes"], 0);
        assert!(
            call(
                &mut session,
                "generate",
                &json!({"track": 1, "op": "explode"})
            )
            .is_err()
        );
    }

    #[test]
    fn lyrics_marks_style_catalog_and_theory_are_available() {
        let mut song = project::song::Song::new("Tools", 120.0);
        song.tracks.push(project::song::Track::new(
            TrackId::new(1).expect("valid"),
            "Piano",
        ));
        song.lyrics.push(project::song::LyricEvent {
            tick: 0,
            text: String::from("la"),
        });
        let mut session = EditSession::new(project::song::ProjectFile::new(song)).expect("valid");
        let lyrics = call(&mut session, "get_lyrics", &json!({})).expect("lyrics");
        assert_eq!(lyrics["lyrics"][0]["text"], "la");

        let directory = std::env::temp_dir().join("lazy-arranger-mcp-catalog");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("dir");
        std::fs::write(directory.join("one.sty"), b"x").expect("write");
        let catalog = call(
            &mut session,
            "style_catalog",
            &json!({"root": directory.display().to_string()}),
        )
        .expect("catalog");
        assert_eq!(catalog["total"], 1);
        assert_eq!(catalog["entries"][0]["number"], 1);
        let _ = std::fs::remove_dir_all(&directory);

        let scale = call(
            &mut session,
            "theory",
            &json!({"op": "scale", "tonic": "C", "mode": "major"}),
        )
        .expect("scale");
        assert_eq!(scale["notes"], json!(["C", "D", "E", "F", "G", "A", "B"]));
        let labels = call(
            &mut session,
            "theory",
            &json!({"op": "label", "tonic": "C", "chords": ["C", "Am", "F", "G"]}),
        )
        .expect("labels");
        assert_eq!(labels["labels"][1], "VIm");
        let cost = call(
            &mut session,
            "theory",
            &json!({"op": "voice_leading", "tonic": "C", "to": "F"}),
        )
        .expect("cost");
        assert_eq!(cost["cost"], 3);
    }

    #[test]
    fn get_song_reports_tracks_and_counts() {
        let mut session = session_with_track();
        let summary = call(&mut session, "get_song", &json!({})).expect("call");
        assert_eq!(summary["name"], "Tools");
        assert_eq!(summary["tracks"][0]["name"], "Piano");
        assert_eq!(summary["tracks"][0]["notes"], 0);
        assert_eq!(summary["meter"], json!([4, 4]));
    }

    #[test]
    fn add_track_and_add_notes_edit_and_undo() {
        let mut session = session_with_track();
        let created = call(&mut session, "add_track", &json!({"name": "Bass"})).expect("track");
        let id = created["track"].as_u64().expect("id");
        let added = call(
            &mut session,
            "add_notes",
            &json!({
                "track": 1,
                "notes": [
                    {"start": 0, "duration": PPQ, "pitch": 60},
                    {"start": PPQ, "duration": PPQ, "pitch": 64, "velocity": 80}
                ]
            }),
        )
        .expect("notes");
        assert_eq!(added["added"], 2);
        assert_eq!(session.song().tracks[1].id.get() as u64, id);
        assert_eq!(session.song().tracks[0].notes.len(), 2);
        assert_eq!(session.undo_depth(), 2);
        call(&mut session, "undo", &json!({})).expect("undo");
        assert!(session.song().tracks[0].notes.is_empty());
    }

    #[test]
    fn set_chords_replaces_the_timeline() {
        let mut session = session_with_track();
        let result = call(
            &mut session,
            "set_chords",
            &json!({"chords": [{"tick": 0, "chord": "Am"}, {"tick": 1920, "chord": "F"}]}),
        )
        .expect("chords");
        assert_eq!(result["chords"], 2);
        assert_eq!(session.song().chords.len(), 2);
        assert_eq!(
            session.song().chords[1]
                .chord
                .format(music_core::Spelling::Sharp),
            "F"
        );
    }

    #[test]
    fn bad_calls_are_errors() {
        let mut session = session_with_track();
        assert!(matches!(
            call(&mut session, "nope", &json!({})),
            Err(ToolError::UnknownTool(_))
        ));
        assert!(call(&mut session, "add_notes", &json!({"track": 1, "notes": []})).is_ok());
        assert!(matches!(
            call(&mut session, "add_notes", &json!({"notes": []})),
            Err(ToolError::Arguments(_))
        ));
        assert!(matches!(
            call(&mut session, "set_chords", &json!({})),
            Err(ToolError::Arguments(_))
        ));
    }
}

/// Handles one JSON-RPC message, returning the response for requests.
///
/// Supports `initialize`, `ping`, `tools/list` and `tools/call`. Notifications
/// return `None`. Unknown methods return a JSON-RPC error object.
pub fn handle_message(session: &mut EditSession, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    id.as_ref()?;
    let reply = |result: Value| json!({"jsonrpc": "2.0", "id": id.clone(), "result": result});
    match method {
        "initialize" => Some(reply(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "lazy-arranger-mcp", "version": env!("CARGO_PKG_VERSION")},
        }))),
        "ping" => Some(reply(json!({}))),
        "tools/list" => Some(reply(json!({"tools": tool_list_json()}))),
        "tools/call" => {
            let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            Some(reply(match call(session, name, &arguments) {
                Ok(value) => json!({
                    "content": [{"type": "text", "text": value.to_string()}],
                    "isError": false,
                }),
                Err(error) => json!({
                    "content": [{"type": "text", "text": error.to_string()}],
                    "isError": true,
                }),
            }))
        }
        other => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": format!("unknown method: {other}")},
        })),
    }
}

/// Serves newline-delimited JSON-RPC until the input ends.
///
/// # Errors
///
/// Returns an error when reading or writing fails. Malformed JSON gets a parse
/// error reply and does not stop the loop.
pub fn serve<R: std::io::BufRead, W: std::io::Write>(
    session: &mut EditSession,
    mut input: R,
    mut output: W,
) -> std::io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(trimmed) {
            Ok(message) => handle_message(session, &message),
            Err(error) => Some(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": {"code": -32700, "message": error.to_string()},
            })),
        };
        if let Some(response) = response {
            writeln!(output, "{response}")?;
            output.flush()?;
        }
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use music_core::PPQ;
    use std::io::Cursor;

    fn session() -> EditSession {
        let mut song = project::song::Song::new("Serve", 120.0);
        song.tracks.push(project::song::Track::new(
            TrackId::new(1).expect("valid"),
            "Piano",
        ));
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn initialize_lists_tools_and_calls_them() {
        let mut session = session();
        let init = handle_message(
            &mut session,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}),
        )
        .expect("reply");
        assert_eq!(init["result"]["serverInfo"]["name"], "lazy-arranger-mcp");

        let list = handle_message(
            &mut session,
            &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        )
        .expect("reply");
        assert!(list["result"]["tools"].as_array().expect("tools").len() >= 6);

        let call = handle_message(
            &mut session,
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "add_notes",
                    "arguments": {"track": 1, "notes": [{"start": 0, "duration": PPQ, "pitch": 64}]}
                }
            }),
        )
        .expect("reply");
        assert_eq!(call["result"]["isError"], false);
        assert_eq!(session.song().tracks[0].notes.len(), 1);
    }

    #[test]
    fn unknown_methods_are_jsonrpc_errors_and_notifications_are_silent() {
        let mut session = session();
        let reply = handle_message(
            &mut session,
            &json!({"jsonrpc": "2.0", "id": 7, "method": "no/such"}),
        )
        .expect("reply");
        assert_eq!(reply["error"]["code"], -32601);
        assert!(
            handle_message(
                &mut session,
                &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
            )
            .is_none()
        );
    }

    #[test]
    fn a_tool_error_is_reported_in_the_result() {
        let mut session = session();
        let reply = handle_message(
            &mut session,
            &json!({
                "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": {"name": "get_song", "arguments": {}}
            }),
        )
        .expect("reply");
        assert_eq!(reply["result"]["isError"], false);
        let bad = handle_message(
            &mut session,
            &json!({
                "jsonrpc": "2.0", "id": 5, "method": "tools/call",
                "params": {"name": "add_notes", "arguments": {"track": 99, "notes": []}}
            }),
        )
        .expect("reply");
        assert_eq!(bad["result"]["isError"], true);
    }

    #[test]
    fn the_server_reads_lines_and_writes_responses() {
        let mut session = session();
        let input = Cursor::new(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n\
             not json\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        );
        let mut output = Vec::new();
        serve(&mut session, input, &mut output).expect("serve");
        let text = String::from_utf8(output).expect("utf8");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "initialize, parse error, tools/list");
        assert!(lines[0].contains("\"result\""));
        assert!(lines[1].contains("-32700"));
        assert!(lines[2].contains("\"tools\""));
    }
}

#[cfg(test)]
mod arranging_tool_tests {
    use super::*;
    use music_core::{Chord, PPQ, Pitch, Velocity};
    use project::Marker;
    use project::song::{Note, Track};

    fn session() -> EditSession {
        let mut song = project::song::Song::new("Tools", 120.0);
        let mut lead = Track::new(TrackId::new(1).expect("valid"), "Lead");
        lead.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C6").expect("valid"),
            velocity: Velocity::new(110).expect("valid"),
            channel: Channel::first(),
        });
        let mut band = Track::new(TrackId::new(2).expect("valid"), "Band");
        for id in 1..=12 {
            band.notes.push(Note {
                id: NoteId::new(id).expect("valid"),
                start: (id as Tick) * 10,
                duration: PPQ,
                pitch: Pitch::from_name(["C4", "E4", "G4"][(id as usize - 1) % 3]).expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(lead);
        song.tracks.push(band);
        song.markers.push(Marker {
            tick: 0,
            name: String::from("Main A"),
        });
        song.chords.push(ChordEvent {
            tick: 0,
            chord: Chord::parse("C").expect("valid"),
        });
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn the_doctor_returns_proposals() {
        let mut session = session();
        let result = call(&mut session, "arrangement_doctor", &json!({})).expect("call");
        assert!(!result["proposals"].as_array().expect("array").is_empty());
    }

    #[test]
    fn a_program_can_be_set_through_tools() {
        let mut session = session();
        let result = call(
            &mut session,
            "set_track_program",
            &json!({"track": 1, "program": 48}),
        )
        .expect("program");
        assert_eq!(result["program"], 48);
        assert_eq!(session.song().tracks[0].program, Some(48));
        assert!(
            call(
                &mut session,
                "set_track_program",
                &json!({"track": 1, "program": 200})
            )
            .is_err()
        );
    }

    #[test]
    fn the_clip_grid_is_reported() {
        let mut session = session();
        session
            .apply(Command::SetClips {
                track: TrackId::new(1).expect("valid"),
                clips: vec![project::song::Clip {
                    id: project::ClipId::new(1).expect("valid"),
                    name: String::from("A"),
                    start: 0,
                    length: 3840,
                }],
            })
            .expect("clips");
        let result = call(&mut session, "get_clip_grid", &json!({"bars": 1})).expect("grid");
        assert_eq!(result["scenes"].as_array().expect("scenes").len(), 1);
        assert_eq!(result["scenes"][0]["slots"][0], 1);
        assert_eq!(result["tracks"].as_array().expect("tracks").len(), 2);
    }

    #[test]
    fn slots_can_be_cleared_and_lengths_changed() {
        let mut session = session();
        session
            .apply(Command::SetClips {
                track: TrackId::new(1).expect("valid"),
                clips: vec![project::song::Clip {
                    id: project::ClipId::new(1).expect("valid"),
                    name: String::from("A"),
                    start: 0,
                    length: 3840,
                }],
            })
            .expect("clips");
        call(
            &mut session,
            "edit_slot",
            &json!({"scene": 0, "track": 1, "action": "length", "length": 1920}),
        )
        .expect("length");
        assert_eq!(session.song().tracks[0].clips[0].length, 1920);
        call(
            &mut session,
            "edit_slot",
            &json!({"scene": 0, "track": 1, "action": "clear"}),
        )
        .expect("clear");
        assert!(session.song().tracks[0].clips.is_empty());
        assert!(
            call(
                &mut session,
                "edit_slot",
                &json!({"scene": 0, "track": 1, "action": "nope"})
            )
            .is_err()
        );
    }

    #[test]
    fn find_chords_writes_the_timeline() {
        let mut session = session();
        let result = call(&mut session, "find_chords", &json!({})).expect("call");
        assert!(result["chords"].as_u64().expect("count") >= 1);
        assert!(!session.song().chords.is_empty());
    }

    #[test]
    fn grooves_can_be_extracted_and_applied() {
        let mut session = session();
        let result = call(&mut session, "extract_groove", &json!({"track": 1})).expect("groove");
        assert_eq!(result["grid"], PPQ / 4);
        assert!(result["offsets"].is_array());
        let applied = call(&mut session, "apply_groove", &json!({"track": 1})).expect("apply");
        assert!(applied["tracks"].as_u64().unwrap_or(0) >= 1);
    }

    #[test]
    fn a_form_and_a_reharmonization_apply() {
        let mut session = session();
        let form = call(&mut session, "make_song_form", &json!({"preset": "gig"})).expect("form");
        assert!(form["notes"].as_u64().expect("notes") > 0);
        let reharm = call(
            &mut session,
            "reharmonize",
            &json!({"tick": 0, "kind": "two_five"}),
        )
        .expect("reharmonize");
        assert_eq!(reharm["chords"], 2);
        assert!(call(&mut session, "make_song_form", &json!({"preset": "nope"})).is_err());
    }
}
