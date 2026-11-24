//! Microphone dictation through a user-supplied Whisper command.

use std::path::{Path, PathBuf};

/// Records a short microphone take to a temporary WAV file.
///
/// # Errors
///
/// Returns a message when the input cannot be recorded or written.
pub fn record_to_wav(seconds: f64) -> Result<PathBuf, String> {
    let audio = audio::record::record_seconds(seconds)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "lazy-arranger-dictate-{}-{stamp}.wav",
        std::process::id()
    ));
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        file.write_all(&audio.to_wav())
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, audio.to_wav()).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Runs a command template with `{file}` replaced by the WAV path.
///
/// # Errors
///
/// Returns a message when the command fails or prints no text.
pub fn transcribe(wav: &Path, command: &str) -> Result<String, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err(String::from("set a Whisper command in Settings"));
    }
    let rendered = crate::shell::render(command, "{file}", "ARRANGER_FILE");
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&rendered)
        .env("ARRANGER_FILE", wav)
        .output()
        .map_err(|error| format!("speech command: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "speech command: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(String::from("speech command printed no text"));
    }
    Ok(text)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_command_template_returns_stdout() {
        let wav = std::env::temp_dir().join("lazy-arranger-dictate-command.wav");
        std::fs::write(&wav, b"RIFF").expect("wav");
        let text = transcribe(&wav, "printf '  hello world  '").expect("text");
        assert_eq!(text, "hello world");
        assert!(transcribe(&wav, "printf ''").is_err());
        assert!(transcribe(&wav, "").is_err());
        let _ = std::fs::remove_file(wav);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_placeholder_is_quoted() {
        let wav = std::env::temp_dir().join("lazy-arranger-dictate-q.wav");
        std::fs::write(&wav, b"RIFF").expect("wav");
        let text = transcribe(&wav, "test -f {file} && echo present").expect("present");
        assert_eq!(text, "present");
        let _ = std::fs::remove_file(wav);
    }
}
