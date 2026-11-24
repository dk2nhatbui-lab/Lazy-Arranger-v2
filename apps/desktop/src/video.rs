//! Extracts an audio track from a video file with FFmpeg.

use std::path::{Path, PathBuf};

/// True for a video container the importer accepts.
#[must_use]
pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "mpeg" | "mpg"
            )
        })
}

/// Writes a temporary mono 48 kHz WAV from a video's audio track.
///
/// # Errors
///
/// Returns a message when FFmpeg fails or produces no file.
pub fn extract_audio(path: &Path) -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let mut folder = crate::settings::Settings::data_dir().join("audio");
    if std::fs::create_dir_all(&folder).is_err() {
        folder = std::env::temp_dir();
    }
    let output = folder.join(format!(
        "video-{}-{stamp}-{}.wav",
        std::process::id(),
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("audio")
    ));
    let result = std::process::Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", "48000"])
        .arg(&output)
        .output()
        .map_err(|error| format!("ffmpeg: {error}"))?;
    if !result.status.success() {
        let _ = std::fs::remove_file(&output);
        return Err(format!(
            "ffmpeg: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    if !output.exists() {
        return Err(String::from("ffmpeg produced no audio file"));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_extensions_are_recognised() {
        assert!(is_video(Path::new("take.MP4")));
        assert!(is_video(Path::new("take.webm")));
        assert!(!is_video(Path::new("take.wav")));
        assert!(!is_video(Path::new("take")));
    }

    #[test]
    fn ffmpeg_extracts_a_temporary_wav() {
        if std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        let video = std::env::temp_dir().join("lazy-arranger-video-test.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i"])
            .arg("sine=frequency=440:duration=1")
            .args(["-f", "lavfi", "-i", "color=c=black:s=64x64:d=1"])
            .args(["-shortest", "-c:v", "mpeg4"])
            .arg(&video)
            .status();
        if !made.is_ok_and(|status| status.success()) {
            let _ = std::fs::remove_file(&video);
            return;
        }
        let wav = extract_audio(&video).expect("audio");
        assert!(std::fs::metadata(&wav).expect("metadata").len() > 44);
        let _ = std::fs::remove_file(&wav);
        let _ = std::fs::remove_file(&video);
    }
}
