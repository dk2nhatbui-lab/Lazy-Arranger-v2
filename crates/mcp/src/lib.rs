//! Tool registry and JSON dispatch for AI assistants.
//!
//! This is the testable core of the MCP server: tool definitions, argument
//! parsing and dispatch onto [`EditSession`]. It never touches a socket, so the
//! same layer can later serve stdio, a local socket or a test harness.

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
        "add_notes" => add_notes(session, arguments),
        "set_chords" => set_chords(session, arguments),
        _ => Err(ToolError::UnknownTool(String::from(name))),
    }
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
