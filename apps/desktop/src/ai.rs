//! Optional OpenAI-compatible chat endpoint for the assistant.
//!
//! The request is sent with the system `curl` command so the app does not
//! vendor a TLS stack. The endpoint, model and key come from Settings.

use serde_json::{Value, json};

/// One endpoint configuration.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// Full chat-completions URL, e.g. `http://localhost:11434/v1/chat/completions`.
    pub url: String,
    /// Model name.
    pub model: String,
    /// Bearer token; empty for local endpoints.
    pub api_key: String,
}

impl Config {
    /// True when a URL and model are both set.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        !self.url.trim().is_empty() && !self.model.trim().is_empty()
    }
}

const SYSTEM: &str = "You are the assistant inside the Lazy Arranger music app. \
Answer briefly. When the user asks for a change, reply with exactly one line \
`tool <tool_name> {json}` using a tool from the app, otherwise answer normally.";

/// Sends one user message and returns the assistant text.
///
/// # Errors
///
/// Returns a message when `curl` is missing, the request fails, or the reply has
/// no text content.
pub fn chat(config: &Config, prompt: &str) -> Result<String, String> {
    if !config.is_ready() {
        return Err(String::from("set an AI endpoint and model in Settings"));
    }
    let body = json!({
        "model": config.model.trim(),
        "temperature": 0.2,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": prompt}
        ]
    });
    let mut command = std::process::Command::new("curl");
    command.args([
        "--silent",
        "--show-error",
        "--max-time",
        "45",
        "-H",
        "Content-Type: application/json",
    ]);
    let key = config.api_key.trim();
    if !key.is_empty() {
        command.args(["-H", &format!("Authorization: Bearer {key}")]);
    }
    let output = command
        .args(["-d", &body.to_string(), config.url.trim()])
        .output()
        .map_err(|error| format!("curl: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "endpoint: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_reply(&String::from_utf8_lossy(&output.stdout))
}

/// Extracts `choices[0].message.content` from a chat-completions reply.
///
/// # Errors
///
/// Returns a message for malformed JSON or a reply without text.
pub fn parse_reply(body: &str) -> Result<String, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|error| format!("reply JSON: {error}"))?;
    value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| String::from("reply has no text content"))
}

/// True when the reply is a single `tool ...` command line.
#[must_use]
pub fn tool_command(reply: &str) -> Option<&str> {
    let line = reply.trim();
    (line.starts_with("tool ") && !line.contains('\n')).then_some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_is_read_from_the_first_choice() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"  hello  "}}]}"#;
        assert_eq!(parse_reply(body).expect("text"), "hello");
        assert!(parse_reply("{}").is_err());
        assert!(parse_reply("not json").is_err());
    }

    #[test]
    fn tool_commands_are_recognised() {
        assert!(tool_command("tool add_track {\"name\":\"Bass\"}").is_some());
        assert!(tool_command("tool add_track\nmore").is_none());
        assert!(tool_command("plain answer").is_none());
    }

    #[test]
    fn a_local_endpoint_returns_text() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            use std::io::{Read, Write};
            let mut buffer = [0_u8; 8192];
            let _ = stream.read(&mut buffer);
            let body = r#"{"choices":[{"message":{"content":"from the test endpoint"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write");
        });
        if std::process::Command::new("curl")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let config = Config {
            url: format!("http://{address}/v1/chat/completions"),
            model: String::from("test"),
            api_key: String::new(),
        };
        let reply = chat(&config, "hello").expect("reply");
        assert_eq!(reply, "from the test endpoint");
        server.join().expect("server");
    }
}
