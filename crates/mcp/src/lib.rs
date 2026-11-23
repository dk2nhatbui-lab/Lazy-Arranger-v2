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
            name: "undo",
            description: "Undo the last edit.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
    ]
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
pub fn call(session: &mut EditSession, name: &str, arguments: &Value) -> Result<Value, ToolError> {
    match name {
        "get_song" => Ok(song_summary(session.song())),
        "get_undo" => Ok(json!({"undo_steps": session.undo_depth()})),
        "undo" => {
            session
                .undo()
                .map_err(|error| ToolError::Command(CommandError::Validation(error.to_string())))?;
            Ok(json!({"undone": true}))
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
                        project::song::AutomationTarget::Volume => "volume",
                        project::song::AutomationTarget::Pan => "pan",
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
        "apply_groove" => apply_groove(session, arguments),
        "make_song_form" => make_song_form(session, arguments),
        "reharmonize" => reharmonize(session, arguments),
        "add_notes" => add_notes(session, arguments),
        "set_chords" => set_chords(session, arguments),
        _ => Err(ToolError::UnknownTool(String::from(name))),
    }
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
    let mut changed = 0;
    for other in others {
        let Some(entry) = session.song().tracks.iter().find(|entry| entry.id == other) else {
            continue;
        };
        let notes = arranging::groove::apply(&entry.notes, &groove, amounts);
        session.apply(Command::ReplaceTrackNotes {
            track: other,
            notes,
        })?;
        changed += 1;
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
    session.apply(Command::RemoveChord { tick })?;
    let step = bar / chords.len() as Tick;
    for (index, chord) in chords.iter().enumerate() {
        session.apply(Command::SetChord {
            tick: tick + (index as Tick) * step,
            chord: *chord,
        })?;
    }
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
    let mut notes = existing.notes.clone();
    let mut highest = notes.iter().map(|note| note.id.get()).max().unwrap_or(0);
    let mut added = 0;
    for entry in array {
        highest += 1;
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
