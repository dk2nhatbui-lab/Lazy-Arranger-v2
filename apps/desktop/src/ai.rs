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

/// Builds the chat-completions message list: system, history, then the prompt.
#[must_use]
pub fn messages(history: &[(String, String)], prompt: &str) -> Value {
    let mut messages = vec![json!({"role": "system", "content": SYSTEM})];
    for (role, content) in history
        .iter()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        if role == "user" || role == "assistant" {
            messages.push(json!({"role": role, "content": content}));
        }
    }
    messages.push(json!({"role": "user", "content": prompt}));
    Value::Array(messages)
}

/// Sends one user message and returns the assistant text.
///
/// # Errors
///
/// Returns a message when `curl` is missing, the request fails, or the reply has
/// no text content.
pub fn chat(config: &Config, prompt: &str) -> Result<String, String> {
    chat_with_history(config, &[], prompt)
}

/// Sends a prompt with earlier user/assistant turns.
///
/// # Errors
///
/// Returns a message when `curl` is missing, the request fails, or the reply has
/// no text content.
pub fn chat_with_history(
    config: &Config,
    history: &[(String, String)],
    prompt: &str,
) -> Result<String, String> {
    if !config.is_ready() {
        return Err(String::from("set an AI endpoint and model in Settings"));
    }
    let body = json!({
        "model": config.model.trim(),
        "temperature": 0.2,
        "messages": messages(history, prompt),
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
    let mut header_file = None;
    if !key.is_empty() {
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-ai-header-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|error| format!("AI key file: {error}"))?;
        use std::io::Write;
        write!(file, "Authorization: Bearer {key}")
            .map_err(|error| format!("AI key file: {error}"))?;
        command.arg("-H").arg(format!("@{}", path.display()));
        header_file = Some(path);
    }
    let output = command
        .args(["-d", &body.to_string(), config.url.trim()])
        .output();
    if let Some(path) = header_file {
        let _ = std::fs::remove_file(path);
    }
    let output = output.map_err(|error| format!("curl: {error}"))?;
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

/// Runs a local assistant CLI with the prompt substituted into `{prompt}`.
///
/// # Errors
///
/// Returns a message when the command fails or prints no text.
pub fn local_chat(command: &str, prompt: &str) -> Result<String, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err(String::from("set a local assistant command in Settings"));
    }
    let rendered = crate::shell::render(command, "{prompt}", "ARRANGER_PROMPT");
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&rendered)
        .env("ARRANGER_PROMPT", prompt)
        .output()
        .map_err(|error| format!("assistant command: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "assistant command: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(String::from("assistant command printed no text"));
    }
    Ok(text)
}

/// True when the reply is a single `tool ...` command line.
#[must_use]
pub fn tool_command(reply: &str) -> Option<&str> {
    let line = reply.trim();
    (line.starts_with("tool ") && !line.contains('\n')).then_some(line)
}

/// Tool name from a `tool <name> ...` command.
#[must_use]
pub fn tool_name(command: &str) -> Option<&str> {
    command
        .trim()
        .strip_prefix("tool ")?
        .split_whitespace()
        .next()
}

/// True when a tool command can change the project (anything not `get_*`).
#[must_use]
pub fn changes_project(command: &str) -> bool {
    tool_name(command).is_some_and(|name| !name.starts_with("get_"))
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

    #[cfg(unix)]
    #[test]
    fn local_prompts_cannot_inject_shell_commands() {
        let marker = std::env::temp_dir().join(format!(
            "lazy-arranger-assistant-injection-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&marker);
        let _ = local_chat(
            "printf '%s' {prompt}",
            &format!("'; touch {}; echo '", marker.display()),
        );
        assert!(!marker.exists());
    }

    #[test]
    fn a_local_assistant_command_returns_stdout() {
        let text = local_chat("printf '  local answer  '", "hello").expect("text");
        assert_eq!(text, "local answer");
        let text = local_chat("printf '%s' {prompt}", "quoted prompt").expect("text");
        assert_eq!(text, "quoted prompt");
        assert!(local_chat("", "x").is_err());
    }

    #[test]
    fn tool_commands_are_recognised() {
        assert!(tool_command("tool add_track {\"name\":\"Bass\"}").is_some());
        assert!(tool_command("tool add_track\nmore").is_none());
        assert!(tool_command("plain answer").is_none());
        assert_eq!(
            tool_name("tool add_track {\"name\":\"Bass\"}"),
            Some("add_track")
        );
        assert!(changes_project("tool add_track {}"));
        assert!(!changes_project("tool get_song {}"));
        assert!(!changes_project("not a tool"));
    }

    #[test]
    fn history_precedes_the_new_prompt() {
        let history = vec![
            (String::from("user"), String::from("earlier question")),
            (String::from("assistant"), String::from("earlier answer")),
        ];
        let messages = messages(&history, "now");
        let array = messages.as_array().expect("array");
        assert_eq!(array.len(), 4);
        assert_eq!(array[0]["role"], "system");
        assert_eq!(array[1]["content"], "earlier question");
        assert_eq!(array[2]["content"], "earlier answer");
        assert_eq!(array[3]["content"], "now");
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
