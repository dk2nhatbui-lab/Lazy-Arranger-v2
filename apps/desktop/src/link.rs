//! Downloads audio from a web link with yt-dlp for Style from Audio.

use std::path::PathBuf;

/// Temporary WAV path for one link download.
#[must_use]
pub fn output_path() -> PathBuf {
    std::env::temp_dir().join(format!("lazy-arranger-link-{}.wav", std::process::id()))
}

/// Downloads the best audio and converts it to 48 kHz mono WAV.
///
/// # Errors
///
/// Returns a message when yt-dlp is missing, fails, or writes no file.
pub fn download(url: &str) -> Result<PathBuf, String> {
    let url = audio::link::clean(url);
    if url.is_empty() {
        return Err(String::from("enter an http(s) link"));
    }
    let path = output_path();
    let _ = std::fs::remove_file(&path);
    let template = path.with_extension("%(ext)s");
    let output = std::process::Command::new("yt-dlp")
        .args([
            "--no-playlist",
            "-x",
            "--audio-format",
            "wav",
            "--postprocessor-args",
            "-ac 1 -ar 48000",
            "-o",
        ])
        .arg(&template)
        .arg("--")
        .arg(&url)
        .output()
        .map_err(|error| format!("yt-dlp: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "yt-dlp: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if !path.exists() {
        return Err(String::from("yt-dlp wrote no WAV"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_link_is_rejected() {
        assert!(download("   ").is_err());
    }

    #[test]
    fn the_output_path_is_a_wav_in_temp() {
        let path = output_path();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("wav")
        );
        assert!(path.starts_with(std::env::temp_dir()));
    }
}
