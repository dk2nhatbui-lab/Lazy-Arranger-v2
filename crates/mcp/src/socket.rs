//! Loopback TCP JSON-RPC with a per-start secret.
//!
//! Every line carries the token; a wrong or missing token is answered with an
//! error and never reaches a tool. The listener is bound to 127.0.0.1 only.

use std::io::{BufRead, BufReader, Read, Write};

/// Refuse a single JSON-RPC line larger than this.
const MAX_LINE_BYTES: usize = 1_048_576;
use std::net::{TcpListener, TcpStream};

use app_commands::EditSession;
use serde_json::{Value, json};

/// A random hex token, 16 bytes from the operating-system CSPRNG.
///
/// # Errors
///
/// Returns a message when the platform random source fails.
pub fn new_token() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| format!("random source: {error}"))?;
    let mut token = String::with_capacity(32);
    for byte in bytes {
        token.push_str(&format!("{byte:02x}"));
    }
    Ok(token)
}

fn authorized(message: &Value, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let Some(given) = message.get("token").and_then(Value::as_str) else {
        return false;
    };
    if given.len() != token.len() {
        return false;
    }
    given
        .bytes()
        .zip(token.bytes())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

/// Handles one line, checking the token first.
#[must_use]
pub fn handle_line(session: &mut EditSession, token: &str, line: &str) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(error) => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": {"code": -32700, "message": error.to_string()},
            }));
        }
    };
    if !authorized(&message, token) {
        return Some(json!({
            "jsonrpc": "2.0",
            "id": message.get("id").cloned().unwrap_or(Value::Null),
            "error": {"code": -32001, "message": "unauthorized: token mismatch"},
        }));
    }
    crate::handle_message(session, &message)
}

/// Serves one connection until it closes.
///
/// # Errors
///
/// Returns an I/O error when reading or writing fails.
pub fn serve_stream(
    session: &mut EditSession,
    token: &str,
    stream: TcpStream,
) -> std::io::Result<()> {
    serve_stream_with(session, token, stream, |_| Ok(()))
}

/// Serves one connection until it closes, calling `after_edit` after every
/// successful `tools/call` so the caller can save the project.
///
/// Each request line is capped at 1 MiB before it is buffered, and reads time
/// out after 30 seconds.
///
/// # Errors
///
/// Returns an I/O error when reading or writing fails, or what `after_edit`
/// returns.
pub fn serve_stream_with(
    session: &mut EditSession,
    token: &str,
    stream: TcpStream,
    mut after_edit: impl FnMut(&EditSession) -> std::io::Result<()>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    loop {
        let mut bytes = Vec::new();
        let read = reader
            .by_ref()
            .take((MAX_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut bytes)?;
        if read == 0 {
            break;
        }
        if bytes.len() > MAX_LINE_BYTES {
            let reply = json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": {"code": -32600, "message": "request line is too large"},
            });
            writeln!(writer, "{reply}")?;
            writer.flush()?;
            break;
        }
        let Ok(line) = std::str::from_utf8(&bytes) else {
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        let is_call = serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|message| {
                message
                    .get("method")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .as_deref()
            == Some("tools/call");
        if let Some(reply) = handle_line(session, token, line) {
            writeln!(writer, "{reply}")?;
            writer.flush()?;
            if is_call && reply["result"]["isError"] == false {
                after_edit(session)?;
            }
        }
    }
    Ok(())
}

/// Binds the loopback listener on a port, where 0 picks a free port.
///
/// # Errors
///
/// Returns an I/O error when the address cannot be bound.
pub fn bind(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::TrackId;
    use project::song::{ProjectFile, Song, Track};

    fn session() -> EditSession {
        let mut song = Song::new("Socket", 120.0);
        song.tracks
            .push(Track::new(TrackId::new(1).expect("valid"), "Piano"));
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn tokens_are_hex_and_unique() {
        let first = new_token().expect("token");
        let second = new_token().expect("token");
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn a_wrong_token_never_reaches_a_tool() {
        let mut session = session();
        let token = String::from("good");
        let denied = handle_line(
            &mut session,
            &token,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","token":"bad"}"#,
        )
        .expect("reply");
        assert!(
            denied["error"]["message"]
                .as_str()
                .expect("message")
                .contains("unauthorized")
        );
        let allowed = handle_line(
            &mut session,
            &token,
            r#"{"jsonrpc":"2.0","id":2,"method":"initialize","token":"good"}"#,
        )
        .expect("reply");
        assert_eq!(allowed["result"]["serverInfo"]["name"], "lazy-arranger-mcp");
    }

    #[test]
    fn a_loopback_connection_round_trips() {
        let listener = bind(0).expect("bind");
        let address = listener.local_addr().expect("address");
        let token = new_token().expect("token");
        let server_token = token.clone();
        let handle = std::thread::spawn(move || {
            let mut session = session();
            if let Ok((stream, _)) = listener.accept() {
                let _ = serve_stream(&mut session, &server_token, stream);
            }
        });
        let mut client = TcpStream::connect(address).expect("connect");
        writeln!(
            client,
            r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","token":"{token}"}}"#
        )
        .expect("write");
        let mut reply = String::new();
        BufReader::new(client.try_clone().expect("clone"))
            .read_line(&mut reply)
            .expect("read");
        assert!(reply.contains("lazy-arranger-mcp"));
        drop(client);
        handle.join().expect("join");
    }
}
