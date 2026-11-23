//! A local assistant: a small command language over the tool registry.
//!
//! No model or network is involved. `tool <name> <json>` runs one registered
//! tool, and a few aliases cover the common questions.

use app_commands::EditSession;
use serde_json::{Value, json};

/// One parsed request.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Show the command list.
    Help,
    /// Summarise the song.
    Song,
    /// Undo the last edit.
    Undo,
    /// Call one tool with JSON arguments.
    Tool {
        /// Tool name.
        name: String,
        /// Arguments object.
        arguments: Value,
    },
}

/// The assistant's answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    /// Text shown in the panel.
    pub text: String,
    /// JSON result, when the tool returned one.
    pub json: Option<Value>,
    /// True when the call changed the project.
    pub changed: bool,
}

/// Parses one command.
///
/// # Errors
///
/// Returns a message for malformed JSON or an incomplete command.
pub fn parse(input: &str) -> Result<Request, String> {
    let input = input.trim();
    if input.is_empty() || input == "help" || input == "?" {
        return Ok(Request::Help);
    }
    match input {
        "song" | "summary" => return Ok(Request::Song),
        "undo" => return Ok(Request::Undo),
        _ => {}
    }
    let Some(rest) = input.strip_prefix("tool ") else {
        return Err(String::from(
            "commands: help, song, undo, tool <name> {json}",
        ));
    };
    let (name, arguments) = match rest.split_once(char::is_whitespace) {
        Some((name, json_text)) => {
            let arguments: Value = serde_json::from_str(json_text.trim())
                .map_err(|error| format!("bad JSON: {error}"))?;
            (name, arguments)
        }
        None => (rest, json!({})),
    };
    if name.is_empty() {
        return Err(String::from("tool name is missing"));
    }
    Ok(Request::Tool {
        name: name.to_string(),
        arguments,
    })
}

/// Runs one command against the session.
#[must_use]
pub fn respond(session: &mut EditSession, input: &str) -> Reply {
    let request = match parse(input) {
        Ok(request) => request,
        Err(error) => {
            return Reply {
                text: error,
                json: None,
                changed: false,
            };
        }
    };
    let (name, arguments) = match request {
        Request::Help => {
            return Reply {
                text: String::from(
                    "help, song, undo, tool <name> {json}. Tools include get_song, add_track, set_chords, arrangement_doctor, generate, set_automation, get_lyrics, theory.",
                ),
                json: None,
                changed: false,
            };
        }
        Request::Song => (String::from("get_song"), json!({})),
        Request::Undo => (String::from("undo"), json!({})),
        Request::Tool { name, arguments } => (name, arguments),
    };
    let read_only = name.starts_with("get_");
    match crate::call(session, &name, &arguments) {
        Ok(result) => {
            let summary = if read_only {
                String::from_utf8_lossy(&serde_json::to_vec_pretty(&result).unwrap_or_default())
                    .to_string()
            } else {
                format!("{name}: done")
            };
            Reply {
                text: summary,
                json: Some(result),
                changed: !read_only,
            }
        }
        Err(error) => Reply {
            text: format!("{name}: {error}"),
            json: None,
            changed: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::TrackId;
    use project::song::{ProjectFile, Song, Track};

    fn session() -> EditSession {
        let mut song = Song::new("Assistant", 120.0);
        song.tracks
            .push(Track::new(TrackId::new(1).expect("valid"), "Piano"));
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn commands_parse() {
        assert_eq!(parse("help").expect("help"), Request::Help);
        assert_eq!(parse("song").expect("song"), Request::Song);
        assert_eq!(parse("undo").expect("undo"), Request::Undo);
        let request = parse(r#"tool add_track {"name":"Bass"}"#).expect("tool");
        assert!(matches!(request, Request::Tool { name, .. } if name == "add_track"));
        assert!(parse("tool add_track {oops").is_err());
        assert!(parse("fly").is_err());
        assert!(parse("tool").is_err());
    }

    #[test]
    fn tools_change_the_session_and_undo_reverses() {
        let mut session = session();
        let reply = respond(&mut session, r#"tool add_track {"name":"Bass"}"#);
        assert!(reply.changed);
        assert_eq!(session.song().tracks.len(), 2);
        let undo = respond(&mut session, "undo");
        assert!(undo.changed);
        assert_eq!(session.song().tracks.len(), 1);
        let song = respond(&mut session, "song");
        assert!(!song.changed);
        assert!(song.text.contains("Assistant"));
        let bad = respond(&mut session, "tool nope {}");
        assert!(bad.text.contains("unknown tool"));
    }
}
