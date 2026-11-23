//! Import plan for Zoom L6max recordings.
//!
//! The L6max records 32-bit float WAV takes. This module scans a folder of
//! takes, reads their headers and reports what an import would add; it does not
//! parse the device's proprietary project files.

use std::path::{Path, PathBuf};

use crate::wav;

/// One readable take.
#[derive(Clone, Debug, PartialEq)]
pub struct Take {
    /// File path.
    pub path: PathBuf,
    /// Display name, from the file stem.
    pub name: String,
    /// Duration in seconds.
    pub seconds: f64,
    /// Channel count.
    pub channels: u16,
    /// Sample rate.
    pub sample_rate: u32,
    /// Bits per sample.
    pub bits: u16,
    /// WAV format tag; 3 is IEEE float.
    pub format_tag: u16,
}

impl Take {
    /// True for the L6max's 32-bit float recordings.
    #[must_use]
    pub fn is_float32(&self) -> bool {
        self.format_tag == 3 && self.bits == 32
    }
}

/// A scan result.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Takes in path order.
    pub takes: Vec<Take>,
    /// Files that could not be read, with the reason.
    pub problems: Vec<(PathBuf, String)>,
}

impl Plan {
    /// Total recorded seconds.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.takes.iter().map(|take| take.seconds).sum()
    }

    /// How many takes are 32-bit float.
    #[must_use]
    pub fn float32(&self) -> usize {
        self.takes.iter().filter(|take| take.is_float32()).count()
    }
}

/// Scans a folder for `.wav` takes, sorted by path.
///
/// # Errors
///
/// Returns a message when the folder cannot be read.
pub fn scan(folder: &Path) -> Result<Plan, String> {
    let mut paths = Vec::new();
    let entries =
        std::fs::read_dir(folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", folder.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
        {
            paths.push(path);
        }
    }
    paths.sort();
    let mut plan = Plan::default();
    for path in paths {
        match std::fs::read(&path) {
            Ok(bytes) => match wav::info(&bytes) {
                Ok(info) => plan.takes.push(Take {
                    name: path
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("take")
                        .to_string(),
                    path,
                    seconds: info.seconds,
                    channels: info.channels,
                    sample_rate: info.sample_rate,
                    bits: info.bits,
                    format_tag: info.format_tag,
                }),
                Err(error) => plan.problems.push((path, error)),
            },
            Err(error) => plan.problems.push((path, format!("read: {error}"))),
        }
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenderedAudio;

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("dir");
        path
    }

    fn float32_wav(seconds: usize) -> Vec<u8> {
        let data_len = seconds * 48_000 * 2 * 4;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&3u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&48_000u32.to_le_bytes());
        out.extend_from_slice(&(48_000u32 * 2 * 4).to_le_bytes());
        out.extend_from_slice(&8u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data_len as u32).to_le_bytes());
        out.resize(out.len() + data_len, 0);
        out
    }

    #[test]
    fn scans_takes_and_reports_float32() {
        let folder = temp("lazy-arranger-l6max-test");
        std::fs::write(folder.join("b-take.wav"), float32_wav(1)).expect("write");
        let pcm = RenderedAudio {
            samples: vec![0.0; 96_000],
            sample_rate: 48_000,
        };
        std::fs::write(folder.join("a-take.wav"), pcm.to_wav()).expect("write");
        std::fs::write(folder.join("notes.txt"), b"ignore").expect("write");
        std::fs::write(folder.join("broken.wav"), b"nope").expect("write");
        let plan = scan(&folder).expect("scan");
        assert_eq!(plan.takes.len(), 2);
        assert_eq!(plan.takes[0].name, "a-take");
        assert_eq!(plan.takes[1].name, "b-take");
        assert!(plan.takes[1].is_float32());
        assert_eq!(plan.float32(), 1);
        assert!((plan.seconds() - 2.0).abs() < 0.01);
        assert_eq!(plan.problems.len(), 1);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        assert!(scan(Path::new("/tmp/lazy-arranger-no-l6max")).is_err());
    }
}
