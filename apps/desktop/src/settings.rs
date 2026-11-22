//! Preferences: a small key/value file plus the shortcut list.
//!
//! The file is plain text (`key = value`), so it survives hand editing and
//! needs no serializer. Unknown keys are kept in `extra` and written back, so
//! a newer build's settings are not lost.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The settings file name.
pub const FILE: &str = "lazy-arranger-settings.txt";

/// One keyboard shortcut for the help window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// Key chord as shown to the user.
    pub keys: &'static str,
    /// What it does.
    pub action: &'static str,
    /// Where it applies.
    pub context: &'static str,
}

/// Every shortcut the desktop app handles.
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        keys: "Ctrl/Cmd+P",
        action: "toggle playback",
        context: "all views",
    },
    Shortcut {
        keys: "Ctrl/Cmd+S",
        action: "save project",
        context: "all views",
    },
    Shortcut {
        keys: "Ctrl/Cmd+Z",
        action: "undo",
        context: "all views",
    },
    Shortcut {
        keys: "Ctrl/Cmd+Shift+Z",
        action: "redo",
        context: "all views",
    },
    Shortcut {
        keys: "D",
        action: "arrangement doctor",
        context: "Arrange",
    },
    Shortcut {
        keys: "F",
        action: "find chords",
        context: "Arrange",
    },
    Shortcut {
        keys: "G",
        action: "groove match",
        context: "Arrange",
    },
    Shortcut {
        keys: "H",
        action: "reharmonise",
        context: "Arrange",
    },
    Shortcut {
        keys: "K",
        action: "chord pads",
        context: "Arrange",
    },
    Shortcut {
        keys: "M",
        action: "mute the selected track",
        context: "Arrange",
    },
    Shortcut {
        keys: "S",
        action: "solo the selected track",
        context: "Arrange",
    },
    Shortcut {
        keys: "T",
        action: "transition",
        context: "Arrange",
    },
    Shortcut {
        keys: "Y",
        action: "song form",
        context: "Arrange",
    },
    Shortcut {
        keys: "B",
        action: "beat grid",
        context: "Arrange",
    },
    Shortcut {
        keys: ",",
        action: "settings",
        context: "all views",
    },
    Shortcut {
        keys: "F1",
        action: "shortcuts",
        context: "all views",
    },
];

/// User settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Language tag; only `en` ships strings today.
    pub language: String,
    /// Chord grade shown by default, 1..=9.
    pub grade: u8,
    /// Preferred MIDI input name, when set.
    pub keyboard: Option<String>,
    /// Start the metronome with playback.
    pub metronome: bool,
    /// Avoid animation.
    pub reduced_motion: bool,
    /// Large text and a simpler header.
    pub kids_mode: bool,
    /// Fields this build does not understand.
    pub extra: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: String::from("en"),
            grade: 5,
            keyboard: None,
            metronome: false,
            reduced_motion: false,
            kids_mode: false,
            extra: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Path of the settings file in the current directory.
    #[must_use]
    pub fn path() -> PathBuf {
        PathBuf::from(FILE)
    }

    /// Reads settings; missing or malformed lines fall back to defaults.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let mut settings = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "language" => settings.language = value.to_string(),
                "grade" => {
                    if let Ok(grade) = value.parse::<u8>() {
                        settings.grade = grade.clamp(1, 9);
                    }
                }
                "keyboard" => {
                    settings.keyboard = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    };
                }
                "metronome" => settings.metronome = value == "true",
                "reduced_motion" => settings.reduced_motion = value == "true",
                "kids_mode" => settings.kids_mode = value == "true",
                _ => {
                    settings.extra.insert(key.to_string(), value.to_string());
                }
            }
        }
        settings
    }

    /// Writes settings.
    ///
    /// # Errors
    ///
    /// Returns a message when the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut text = String::from("# Lazy Arranger settings\n");
        text.push_str(&format!("language = {}\n", self.language));
        text.push_str(&format!("grade = {}\n", self.grade));
        if let Some(keyboard) = &self.keyboard {
            text.push_str(&format!("keyboard = {keyboard}\n"));
        }
        text.push_str(&format!("metronome = {}\n", self.metronome));
        text.push_str(&format!("reduced_motion = {}\n", self.reduced_motion));
        text.push_str(&format!("kids_mode = {}\n", self.kids_mode));
        for (key, value) in &self.extra {
            text.push_str(&format!("{key} = {value}\n"));
        }
        std::fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn defaults_round_trip_and_unknown_keys_survive() {
        let path = temp("lazy-arranger-settings-test.txt");
        let mut settings = Settings {
            kids_mode: true,
            grade: 3,
            keyboard: Some(String::from("PSR-S970")),
            ..Settings::default()
        };
        settings
            .extra
            .insert(String::from("future"), String::from("yes"));
        settings.save(&path).expect("save");
        let back = Settings::load(&path);
        assert_eq!(back, settings);
        std::fs::write(&path, "grade = 99\nlanguage = en\n").expect("write");
        let clamped = Settings::load(&path);
        assert_eq!(clamped.grade, 9, "grades clamp to nine");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_missing_file_gives_defaults() {
        let path = temp("lazy-arranger-settings-missing.txt");
        let settings = Settings::load(&path);
        assert_eq!(settings.language, "en");
        assert_eq!(settings.grade, 5);
        assert!(!settings.kids_mode);
    }

    #[test]
    fn shortcuts_have_unique_keys_and_actions() {
        let mut keys: Vec<&str> = SHORTCUTS.iter().map(|shortcut| shortcut.keys).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), SHORTCUTS.len());
        assert!(SHORTCUTS.iter().any(|shortcut| shortcut.keys == "F1"));
    }
}
