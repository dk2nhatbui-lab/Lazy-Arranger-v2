//! YouTube (and other yt-dlp) links: validation, arguments and output parsing.
//!
//! The app never downloads by itself; it runs yt-dlp and ffmpeg, which the user
//! installs. These helpers are pure, so they are testable without a network.

use std::path::PathBuf;

/// True for an http(s) link with a host and no spaces.
#[must_use]
pub fn is_link(text: &str) -> bool {
    let text = text.trim();
    if text.contains(char::is_whitespace) {
        return false;
    }
    let rest = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"));
    rest.is_some_and(|rest| rest.contains('.') && !rest.starts_with('.'))
}

/// The link without spaces, with `https://` added for `www.` or `youtu`.
/// Empty when it is not a link.
#[must_use]
pub fn clean(text: &str) -> String {
    let text = text.trim();
    let lower = text.to_lowercase();
    let candidate = if lower.starts_with("www.") || lower.starts_with("youtu") {
        format!("https://{text}")
    } else {
        text.to_string()
    };
    if is_link(&candidate) {
        candidate
    } else {
        String::new()
    }
}

/// Arguments for yt-dlp (after the program): best audio of one video as WAV,
/// named after the title, with its path printed as `FILE <path>`.
#[must_use]
pub fn ytdlp_args(
    link: &str,
    folder: &std::path::Path,
    ffmpeg_folder: &std::path::Path,
) -> Vec<String> {
    let mut args = vec![
        String::from("--no-playlist"),
        String::from("-x"),
        String::from("--audio-format"),
        String::from("wav"),
        String::from("--audio-quality"),
        String::from("0"),
        String::from("--print"),
        String::from("after_move:FILE %(filepath)s"),
        String::from("-o"),
        format!("{}/%(title)s.%(ext)s", folder.display()),
    ];
    if !ffmpeg_folder.as_os_str().is_empty() {
        args.push(String::from("--ffmpeg-location"));
        args.push(ffmpeg_folder.display().to_string());
    }
    args.push(String::from("--"));
    args.push(link.to_string());
    args
}

/// Temporary WAV path for one link download.
#[must_use]
pub fn temp_wav_path() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "lazy-arranger-link-{}-{stamp}.wav",
        std::process::id()
    ))
}

/// Downloads the best audio and converts it to a temporary 48 kHz mono WAV.
///
/// # Errors
///
/// Returns a message when `url` is invalid or `yt-dlp` is missing, fails, or writes no file.
pub fn download_mono_wav(url: &str) -> Result<PathBuf, String> {
    let url = clean(url);
    if url.is_empty() {
        return Err(String::from("enter an http(s) link"));
    }
    let path = temp_wav_path();
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

/// Parses the `FILE <path>` line yt-dlp prints.
#[must_use]
pub fn parse_file(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("FILE "))
        .map(PathBuf::from)
}

/// Runs yt-dlp and returns the downloaded WAV path.
///
/// # Errors
///
/// Returns a message when yt-dlp is missing, fails, or does not print a file.
pub fn download(
    link: &str,
    folder: &std::path::Path,
    ffmpeg_folder: &std::path::Path,
) -> Result<PathBuf, String> {
    let cleaned = clean(link);
    if cleaned.is_empty() {
        return Err(String::from("not an http(s) link"));
    }
    let output = std::process::Command::new("yt-dlp")
        .args(ytdlp_args(&cleaned, folder, ffmpeg_folder))
        .output()
        .map_err(|error| {
            format!("could not run yt-dlp: {error}. Install yt-dlp and ffmpeg first")
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!("yt-dlp failed: {}", stderr.trim()));
    }
    parse_file(&stdout).ok_or_else(|| format!("yt-dlp did not report a file: {stderr}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn links_are_recognised_and_cleaned() {
        assert!(is_link("https://youtu.be/abc"));
        assert!(!is_link("not a link"));
        assert!(!is_link("https://"));
        assert_eq!(clean(" https://youtu.be/abc "), "https://youtu.be/abc");
        assert_eq!(
            clean("www.youtube.com/watch?v=x"),
            "https://www.youtube.com/watch?v=x"
        );
        assert_eq!(clean("youtu.be/abc"), "https://youtu.be/abc");
        assert_eq!(clean("nonsense"), "");
    }

    #[test]
    fn arguments_ask_for_one_wav_and_a_file_line() {
        let args = ytdlp_args("https://youtu.be/x", Path::new("/tmp/audio"), Path::new(""));
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "-x" && pair[1] == "--audio-format")
        );
        assert!(args.iter().any(|arg| arg == "--no-playlist"));
        assert!(args.iter().any(|arg| arg.contains("%(title)s")));
        assert!(!args.iter().any(|arg| arg == "--ffmpeg-location"));
        assert_eq!(args[args.len() - 2], "--");
        assert_eq!(args.last().map(String::as_str), Some("https://youtu.be/x"));
    }

    #[test]
    fn empty_links_are_rejected_and_temp_paths_are_wav() {
        assert!(download_mono_wav("   ").is_err());
        let path = temp_wav_path();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("wav")
        );
        assert!(path.starts_with(std::env::temp_dir()));
    }

    #[test]
    fn the_file_line_is_parsed() {
        let output = "[youtube] abc\nFILE /tmp/audio/Song.wav\n";
        assert_eq!(
            parse_file(output),
            Some(PathBuf::from("/tmp/audio/Song.wav"))
        );
        assert_eq!(parse_file("nothing"), None);
    }
}
