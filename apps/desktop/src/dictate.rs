//! Microphone dictation through a user-supplied Whisper command.

use std::path::{Path, PathBuf};

/// Records a short microphone take to a temporary WAV file.
///
/// # Errors
///
/// Returns a message when the input cannot be recorded or written.
pub fn record_to_wav(seconds: f64) -> Result<PathBuf, String> {
    let audio = audio::record::record_seconds(seconds)?;
    let path =
        std::env::temp_dir().join(format!("lazy-arranger-dictate-{}.wav", std::process::id()));
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
    let quoted = format!("'{}'", wav.display().to_string().replace('\'', "'\\''"));
    let rendered = if command.contains("{file}") {
        command.replace("{file}", &quoted)
    } else {
        format!("{command} {quoted}")
    };
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&rendered)
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

#[cfg(test)]
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
