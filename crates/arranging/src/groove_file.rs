//! Groove files: a small text format and a folder library.

use std::path::{Path, PathBuf};

use crate::groove::Groove;

/// The file extension.
pub const EXTENSION: &str = "groove";

/// Serialises a groove with its name.
#[must_use]
pub fn to_text(name: &str, groove: &Groove) -> String {
    let offsets: Vec<String> = groove.offsets.iter().map(i32::to_string).collect();
    let levels: Vec<String> = groove
        .levels
        .iter()
        .map(|level| format!("{level:.3}"))
        .collect();
    format!(
        "# Lazy Arranger groove\nname = {name}\ngrid = {}\noffsets = {}\nlevels = {}\n",
        groove.grid,
        offsets.join(","),
        levels.join(",")
    )
}

/// Parses a groove file.
///
/// # Errors
///
/// Returns a message for a missing name, grid, offset or level list, unequal
/// lengths, or an unparsable number.
pub fn parse(text: &str) -> Result<(String, Groove), String> {
    let mut name = String::new();
    let mut grid = 0_i64;
    let mut offsets: Vec<i32> = Vec::new();
    let mut levels: Vec<f32> = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {}: expected key = value", number + 1))?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "name" => name = value.to_string(),
            "grid" => {
                grid = value
                    .parse()
                    .map_err(|_| format!("line {}: bad grid", number + 1))?
            }
            "offsets" => {
                offsets = value
                    .split(',')
                    .map(|item| {
                        item.trim()
                            .parse()
                            .map_err(|_| format!("line {}: bad offset", number + 1))
                    })
                    .collect::<Result<_, _>>()?;
            }
            "levels" => {
                levels = value
                    .split(',')
                    .map(|item| {
                        item.trim()
                            .parse()
                            .map_err(|_| format!("line {}: bad level", number + 1))
                    })
                    .collect::<Result<_, _>>()?;
            }
            other => return Err(format!("line {}: unknown key {other:?}", number + 1)),
        }
    }
    if name.is_empty() {
        return Err(String::from("groove needs a name"));
    }
    // NaN or infinite levels would turn notes into velocity 0 (note-offs).
    if levels
        .iter()
        .any(|level: &f32| !level.is_finite() || *level < 0.0)
    {
        return Err(String::from(
            "groove levels must be finite and not negative",
        ));
    }
    if grid <= 0 {
        return Err(String::from("groove needs a positive grid"));
    }
    if offsets.len() != levels.len() || offsets.is_empty() {
        return Err(String::from(
            "offsets and levels must be equal and non-empty",
        ));
    }
    Ok((
        name,
        Groove {
            grid,
            offsets,
            levels,
        },
    ))
}

/// Saves a groove as `<slug>.groove` in a folder.
///
/// # Errors
///
/// Returns a message when the folder or file cannot be written.
pub fn save(directory: &Path, name: &str, groove: &Groove) -> Result<PathBuf, String> {
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    let slug: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let path = directory.join(format!("{slug}.{EXTENSION}"));
    std::fs::write(&path, to_text(name, groove))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path)
}

/// Lists grooves in a folder: `(name, path)`, sorted by path.
///
/// # Errors
///
/// Returns a message when the folder cannot be read.
pub fn list(directory: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    for entry in entries {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some(EXTENSION) {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if let Ok((name, _)) = parse(&text) {
            out.push((name, path));
        }
    }
    out.sort_by(|left, right| left.1.cmp(&right.1));
    Ok(out)
}

/// Loads one groove file.
///
/// # Errors
///
/// Returns a message when the file cannot be read or parsed.
pub fn load(path: &Path) -> Result<(String, Groove), String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    parse(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groove() -> Groove {
        Groove {
            grid: 240,
            offsets: vec![0, 12, -8, 0],
            levels: vec![1.0, 0.8, 1.1, 1.0],
        }
    }

    #[test]
    fn text_round_trips() {
        let text = to_text("Shuffle", &groove());
        let (name, back) = parse(&text).expect("parse");
        assert_eq!(name, "Shuffle");
        assert_eq!(back.grid, 240);
        assert_eq!(back.offsets, vec![0, 12, -8, 0]);
        assert!((back.levels[1] - 0.8).abs() < 1.0e-3);
    }

    #[test]
    fn malformed_files_are_rejected() {
        assert!(parse("name = X\n").is_err());
        assert!(parse("name = X\ngrid = 240\noffsets = 1,2\nlevels = 1\n").is_err());
        assert!(parse("name = X\ngrid = 240\noffsets = 1\nlevels = 1\ncolor = red\n").is_err());
        assert!(parse("name = X\ngrid = bad\noffsets = 1\nlevels = 1\n").is_err());
    }

    #[test]
    fn saving_and_listing_works() {
        let directory = std::env::temp_dir().join("lazy-arranger-grooves-test");
        let _ = std::fs::remove_dir_all(&directory);
        let path = save(&directory, "Shuffle 8", &groove()).expect("save");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("shuffle-8.groove")
        );
        let entries = list(&directory).expect("list");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "Shuffle 8");
        let (name, loaded) = load(&path).expect("load");
        assert_eq!(name, "Shuffle 8");
        assert_eq!(loaded.offsets, groove().offsets);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
