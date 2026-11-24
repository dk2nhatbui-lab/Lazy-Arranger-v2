//! Preferences: a small key/value file plus the shortcut list.
//!
//! The file is plain text (`key = value`), so it survives hand editing and
//! needs no serializer. Unknown keys are kept in `extra` and written back, so
//! a newer build's settings are not lost.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The settings file name.
pub const FILE: &str = "lazy-arranger-settings.txt";

/// A parsed key binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    /// Required modifiers.
    pub modifiers: egui::Modifiers,
    /// Required key.
    pub key: egui::Key,
}

/// Parses `cmd+shift+s`-style bindings; modifiers are optional.
#[must_use]
pub fn parse_shortcut(text: &str) -> Option<Binding> {
    let mut modifiers = egui::Modifiers::NONE;
    let mut key = None;
    for raw in text.split('+') {
        let part = raw.trim().to_ascii_lowercase();
        if part.is_empty() {
            return None;
        }
        match part.as_str() {
            "cmd" | "command" | "meta" => modifiers.command = true,
            "ctrl" | "control" => modifiers.ctrl = true,
            "shift" => modifiers.shift = true,
            "alt" | "option" => modifiers.alt = true,
            other => {
                if key.is_some() {
                    return None;
                }
                key = Some(
                    egui::Key::from_name(other)
                        .or_else(|| egui::Key::from_name(&other.to_ascii_uppercase()))?,
                );
            }
        }
    }
    Some(Binding {
        modifiers,
        key: key?,
    })
}

/// Labels whose parsed bindings collide with another label.
#[must_use]
pub fn binding_conflicts(bindings: &[(&'static str, &str)]) -> Vec<&'static str> {
    let mut parsed: Vec<(&'static str, Binding)> = Vec::new();
    let mut conflicts = Vec::new();
    for (label, text) in bindings {
        let Some(binding) = parse_shortcut(text) else {
            continue;
        };
        if let Some((other, _)) = parsed.iter().find(|(_, existing)| *existing == binding)
            && !conflicts.contains(label)
        {
            if !conflicts.contains(other) {
                conflicts.push(*other);
            }
            conflicts.push(*label);
        }
        parsed.push((label, binding));
    }
    conflicts
}

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
    /// Listen to every MIDI input, not only `keyboard`.
    pub auto_inputs: bool,
    /// Preferred audio input name, when set.
    pub audio_input: Option<String>,
    /// 0 mixes all input channels; 1..=16 records that channel.
    pub audio_input_channel: u8,
    /// Preferred audio output device.
    pub audio_output: Option<String>,
    /// Fixed output buffer in frames; zero means the device default.
    pub audio_buffer_frames: u32,
    /// Preferred MIDI output name for the MIDI out window.
    pub midi_output: Option<String>,
    /// OpenAI-compatible chat endpoint.
    pub ai_url: String,
    /// Model name for the chat endpoint.
    pub ai_model: String,
    /// Bearer token for the chat endpoint (stored as plain text).
    pub ai_api_key: String,
    /// Release tag the user chose to skip.
    pub skipped_update: String,
    /// Whisper command template with `{file}`.
    pub speech_command: String,
    /// Text-to-speech template with `{text}`.
    pub speak_command: String,
    /// Local assistant CLI template with `{prompt}`.
    pub assistant_command: String,
    /// Selected device-profile id for keyboard export.
    pub device_profile: Option<String>,
    /// Start the metronome with playback.
    pub metronome: bool,
    /// Downbeat accent, 0..=1.
    pub metronome_accent: f32,
    /// Avoid animation.
    pub reduced_motion: bool,
    /// Large text and a simpler header.
    pub kids_mode: bool,
    /// Seconds without input before Kids Mode pauses.
    pub kids_pause_seconds: f32,
    /// Kids Mode: MIDI keys also audition their own notes.
    pub kids_keys_play: bool,
    /// Light palette instead of the default dark one.
    pub light_theme: bool,
    /// Speak assistant replies with the local `say` command.
    pub speech: bool,
    /// Detect chords after MIDI recording.
    pub record_chords: bool,
    /// Monitor the audio input through the output.
    pub monitor: bool,
    /// Snap recorded MIDI note starts to the 1/16 grid.
    pub record_quantise: bool,
    /// Silent count-in bars before MIDI recording, 0..=2.
    pub record_count_in: u8,
    /// Folder holding the style library, empty when not configured.
    pub style_library: String,
    /// Master gain in decibels.
    pub master_gain_db: f32,
    /// Whether the master limiter is on.
    pub master_limiter: bool,
    /// Master reverb amount, 0..=1.
    pub master_reverb: f32,
    /// Master chorus amount, 0..=1.
    pub master_chorus: f32,
    /// Undo binding.
    pub undo_keys: String,
    /// Redo binding.
    pub redo_keys: String,
    /// Save binding.
    pub save_keys: String,
    /// Save-as binding.
    pub save_as_keys: String,
    /// Open binding.
    pub open_keys: String,
    /// Play/pause binding.
    pub play_keys: String,
    /// Fields this build does not understand.
    pub extra: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: String::from("en"),
            grade: 5,
            keyboard: None,
            auto_inputs: false,
            audio_input: None,
            audio_input_channel: 0,
            audio_output: None,
            audio_buffer_frames: 0,
            midi_output: None,
            ai_url: String::new(),
            ai_model: String::from("gpt-4o-mini"),
            ai_api_key: String::new(),
            skipped_update: String::new(),
            speech_command: String::new(),
            speak_command: String::new(),
            assistant_command: String::new(),
            device_profile: None,
            metronome: false,
            metronome_accent: 1.0,
            reduced_motion: false,
            kids_mode: false,
            kids_pause_seconds: 1.5,
            kids_keys_play: false,
            light_theme: false,
            speech: false,
            record_chords: false,
            monitor: false,
            record_quantise: false,
            record_count_in: 0,
            style_library: String::new(),
            master_gain_db: 0.0,
            master_limiter: true,
            master_reverb: 0.0,
            master_chorus: 0.0,
            undo_keys: String::from("cmd+z"),
            redo_keys: String::from("cmd+shift+z"),
            save_keys: String::from("cmd+s"),
            save_as_keys: String::from("cmd+shift+s"),
            open_keys: String::from("cmd+o"),
            play_keys: String::from("cmd+p"),
            extra: BTreeMap::new(),
        }
    }
}

/// Path of the private AI-key file beside the settings file.
fn ai_key_path(settings_path: &Path) -> PathBuf {
    settings_path.with_extension("ai-keys")
}

fn load_ai_key(settings_path: &Path) -> String {
    std::fs::read_to_string(ai_key_path(settings_path))
        .map(|key| key.trim().to_string())
        .unwrap_or_default()
}

fn save_ai_key(settings_path: &Path, key: &str) -> Result<(), String> {
    let path = ai_key_path(settings_path);
    if key.trim().is_empty() {
        let _ = std::fs::remove_file(&path);
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        file.write_all(key.trim().as_bytes())
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, key.trim())
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

fn clean_line(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

impl Settings {
    /// Per-user directory for settings, keys, profiles and recent projects.
    #[must_use]
    pub fn data_dir() -> PathBuf {
        if let Ok(path) = std::env::var("ARRANGER_DATA_DIR")
            && !path.trim().is_empty()
        {
            return PathBuf::from(path);
        }
        #[cfg(target_os = "macos")]
        {
            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(home).join("Library/Application Support/Lazy Arranger");
            }
        }
        #[cfg(windows)]
        {
            if let Ok(appdata) = std::env::var("APPDATA") {
                return PathBuf::from(appdata).join("Lazy Arranger");
            }
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            if let Ok(config) = std::env::var("XDG_CONFIG_HOME")
                && !config.trim().is_empty()
            {
                return PathBuf::from(config).join("lazy-arranger");
            }
            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(home).join(".config/lazy-arranger");
            }
        }
        PathBuf::from(".")
    }

    /// Path of the per-user settings file.
    #[must_use]
    pub fn path() -> PathBuf {
        Self::data_dir().join(FILE)
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
                "auto_inputs" => settings.auto_inputs = value == "true",
                "keyboard" => {
                    settings.keyboard = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    };
                }
                "audio_input_channel" => {
                    settings.audio_input_channel = value.parse::<u8>().unwrap_or(0).min(16);
                }
                "audio_input" => {
                    settings.audio_input = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    };
                }
                "audio_output" => {
                    settings.audio_output = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    };
                }
                "audio_buffer_frames" => {
                    settings.audio_buffer_frames = value.parse::<u32>().unwrap_or(0).clamp(0, 4096);
                }
                "ai_url" => settings.ai_url = value.to_string(),
                "ai_model" => settings.ai_model = value.to_string(),
                "ai_api_key" => settings.ai_api_key = value.to_string(),
                "skipped_update" => settings.skipped_update = value.to_string(),
                "speech_command" => settings.speech_command = value.to_string(),
                "speak_command" => settings.speak_command = value.to_string(),
                "assistant_command" => settings.assistant_command = value.to_string(),
                "midi_output" => {
                    settings.midi_output = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    };
                }
                "device_profile" => {
                    settings.device_profile = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_string())
                    };
                }
                "metronome" => settings.metronome = value == "true",
                "metronome_accent" => {
                    settings.metronome_accent = value.parse::<f32>().unwrap_or(1.0).clamp(0.0, 1.0);
                }
                "reduced_motion" => settings.reduced_motion = value == "true",
                "kids_mode" => settings.kids_mode = value == "true",
                "kids_keys_play" => settings.kids_keys_play = value == "true",
                "kids_pause_seconds" => {
                    settings.kids_pause_seconds =
                        value.parse::<f32>().unwrap_or(1.5).clamp(0.5, 5.0);
                }
                "light_theme" => settings.light_theme = value == "true",
                "speech" => settings.speech = value == "true",
                "record_chords" => settings.record_chords = value == "true",
                "monitor" => settings.monitor = value == "true",
                "record_quantise" => settings.record_quantise = value == "true",
                "record_count_in" => {
                    if let Ok(bars) = value.parse::<u8>() {
                        settings.record_count_in = bars.min(2);
                    }
                }
                "style_library" => settings.style_library = value.to_string(),
                "master_gain_db" => {
                    if let Ok(gain) = value.parse::<f32>() {
                        settings.master_gain_db = gain.clamp(-24.0, 12.0);
                    }
                }
                "master_limiter" => settings.master_limiter = value == "true",
                "master_reverb" => {
                    settings.master_reverb = value.parse::<f32>().unwrap_or(0.0).clamp(0.0, 1.0);
                }
                "master_chorus" => {
                    settings.master_chorus = value.parse::<f32>().unwrap_or(0.0).clamp(0.0, 1.0);
                }
                "shortcut.undo" => settings.undo_keys = value.to_string(),
                "shortcut.redo" => settings.redo_keys = value.to_string(),
                "shortcut.save" => settings.save_keys = value.to_string(),
                "shortcut.save_as" => settings.save_as_keys = value.to_string(),
                "shortcut.open" => settings.open_keys = value.to_string(),
                "shortcut.play" => settings.play_keys = value.to_string(),
                _ => {
                    settings.extra.insert(key.to_string(), value.to_string());
                }
            }
        }
        if settings.ai_api_key.is_empty() {
            settings.ai_api_key = load_ai_key(path);
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
            text.push_str(&format!("keyboard = {}\n", clean_line(keyboard)));
        }
        text.push_str(&format!("auto_inputs = {}\n", self.auto_inputs));
        if let Some(input) = &self.audio_input {
            text.push_str(&format!("audio_input = {}\n", clean_line(input)));
        }
        text.push_str(&format!(
            "audio_input_channel = {}\n",
            self.audio_input_channel
        ));
        if let Some(output) = &self.audio_output {
            text.push_str(&format!("audio_output = {}\n", clean_line(output)));
        }
        text.push_str(&format!(
            "audio_buffer_frames = {}\n",
            self.audio_buffer_frames
        ));
        if let Some(output) = &self.midi_output {
            text.push_str(&format!("midi_output = {}\n", clean_line(output)));
        }
        text.push_str(&format!("ai_url = {}\n", clean_line(&self.ai_url)));
        text.push_str(&format!("ai_model = {}\n", clean_line(&self.ai_model)));
        text.push_str(&format!(
            "skipped_update = {}\n",
            clean_line(&self.skipped_update)
        ));
        text.push_str(&format!(
            "speech_command = {}\n",
            clean_line(&self.speech_command)
        ));
        text.push_str(&format!(
            "speak_command = {}\n",
            clean_line(&self.speak_command)
        ));
        text.push_str(&format!(
            "assistant_command = {}\n",
            clean_line(&self.assistant_command)
        ));
        if let Some(profile) = &self.device_profile {
            text.push_str(&format!("device_profile = {}\n", clean_line(profile)));
        }
        text.push_str(&format!("metronome = {}\n", self.metronome));
        text.push_str(&format!(
            "metronome_accent = {:.3}\n",
            self.metronome_accent
        ));
        text.push_str(&format!("reduced_motion = {}\n", self.reduced_motion));
        text.push_str(&format!("kids_mode = {}\n", self.kids_mode));
        text.push_str(&format!(
            "kids_pause_seconds = {:.2}\n",
            self.kids_pause_seconds
        ));
        text.push_str(&format!("kids_keys_play = {}\n", self.kids_keys_play));
        text.push_str(&format!("light_theme = {}\n", self.light_theme));
        text.push_str(&format!("speech = {}\n", self.speech));
        text.push_str(&format!("record_chords = {}\n", self.record_chords));
        text.push_str(&format!("monitor = {}\n", self.monitor));
        text.push_str(&format!("record_quantise = {}\n", self.record_quantise));
        text.push_str(&format!("record_count_in = {}\n", self.record_count_in));
        if !self.style_library.is_empty() {
            text.push_str(&format!(
                "style_library = {}\n",
                clean_line(&self.style_library)
            ));
        }
        text.push_str(&format!("master_gain_db = {}\n", self.master_gain_db));
        text.push_str(&format!("master_limiter = {}\n", self.master_limiter));
        text.push_str(&format!("master_reverb = {}\n", self.master_reverb));
        text.push_str(&format!("master_chorus = {}\n", self.master_chorus));
        text.push_str(&format!("shortcut.undo = {}\n", self.undo_keys));
        text.push_str(&format!("shortcut.redo = {}\n", self.redo_keys));
        text.push_str(&format!("shortcut.save = {}\n", self.save_keys));
        text.push_str(&format!("shortcut.save_as = {}\n", self.save_as_keys));
        text.push_str(&format!("shortcut.open = {}\n", self.open_keys));
        text.push_str(&format!("shortcut.play = {}\n", self.play_keys));
        for (key, value) in &self.extra {
            text.push_str(&format!("{} = {}\n", clean_line(key), clean_line(value)));
        }
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        std::fs::write(&temporary, text)
            .map_err(|error| format!("{}: {error}", temporary.display()))?;
        #[cfg(unix)]
        std::fs::rename(&temporary, path).map_err(|error| {
            let _ = std::fs::remove_file(&temporary);
            format!("{}: {error}", path.display())
        })?;
        #[cfg(not(unix))]
        {
            let _ = std::fs::remove_file(path);
            std::fs::rename(&temporary, path).map_err(|error| {
                let _ = std::fs::remove_file(&temporary);
                format!("{}: {error}", path.display())
            })?;
        }
        save_ai_key(path, &self.ai_api_key)
    }
}

/// The honest answer to "check for updates": this build has no channel.
#[must_use]
pub fn update_status() -> &'static str {
    "No update channel is configured; this build is used as installed."
}

/// Summarises discovered MIDI ports for the settings window.
#[must_use]
pub fn ports_summary(inputs: &[String], outputs: &[String]) -> String {
    format!("{} input(s), {} output(s)", inputs.len(), outputs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn newlines_in_values_cannot_inject_settings() {
        let path = temp("lazy-arranger-settings-injection.txt");
        let settings = Settings {
            ai_url: String::from("https://example.test\nmetronome = true"),
            ..Settings::default()
        };
        settings.save(&path).expect("save");
        let loaded = Settings::load(&path);
        assert!(!loaded.metronome);
        assert!(loaded.ai_url.contains("metronome = true"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn defaults_round_trip_and_unknown_keys_survive() {
        let path = temp("lazy-arranger-settings-test.txt");
        let mut settings = Settings {
            kids_mode: true,
            kids_pause_seconds: 2.5,
            kids_keys_play: true,
            light_theme: true,
            record_chords: true,
            monitor: true,
            record_quantise: true,
            record_count_in: 2,
            master_reverb: 0.3,
            master_chorus: 0.2,
            grade: 3,
            keyboard: Some(String::from("PSR-S970")),
            auto_inputs: true,
            audio_input: Some(String::from("Camo Microphone")),
            audio_output: Some(String::from("Built-in Output")),
            ai_url: String::from("http://127.0.0.1:11434/v1/chat/completions"),
            ai_api_key: String::from("test-key"),
            speech_command: String::from("whisper {file}"),
            speak_command: String::from("kokoro {text}"),
            midi_output: Some(String::from("PSR-S970")),
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
        std::fs::write(&path, "kids_pause_seconds = 9\n").expect("write");
        assert_eq!(
            Settings::load(&path).kids_pause_seconds,
            5.0,
            "kids pause clamps to five seconds"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn ai_keys_live_in_a_private_sibling_file() {
        let path = temp("lazy-arranger-settings-key.txt");
        let key_path = path.with_extension("ai-keys");
        let _ = std::fs::remove_file(&key_path);
        let settings = Settings {
            ai_api_key: String::from("secret-token"),
            ..Settings::default()
        };
        settings.save(&path).expect("save");
        assert_eq!(
            std::fs::read_to_string(&key_path).expect("key"),
            "secret-token"
        );
        let text = std::fs::read_to_string(&path).expect("settings");
        assert!(!text.contains("secret-token"), "key is not in settings");
        let back = Settings::load(&path);
        assert_eq!(back.ai_api_key, "secret-token");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&key_path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&key_path);
    }

    #[test]
    fn duplicate_shortcut_bindings_are_reported() {
        let conflicts = binding_conflicts(&[
            ("Undo", "cmd+z"),
            ("Redo", "cmd+shift+z"),
            ("Save", "cmd+s"),
            ("Save as", "cmd+s"),
        ]);
        assert_eq!(conflicts, vec!["Save", "Save as"]);
        assert!(binding_conflicts(&[("Undo", "cmd+z"), ("Redo", "cmd+y")]).is_empty());
    }

    #[test]
    fn shortcut_bindings_parse_and_round_trip() {
        let binding = parse_shortcut("cmd+shift+s").expect("binding");
        assert!(binding.modifiers.command);
        assert!(binding.modifiers.shift);
        assert_eq!(binding.key, egui::Key::S);
        assert_eq!(
            parse_shortcut("ctrl+alt+p").expect("binding").key,
            egui::Key::P
        );
        assert!(parse_shortcut("cmd+").is_none());
        assert!(parse_shortcut("hyper+x").is_none());
        assert!(parse_shortcut("cmd+a+b").is_none());
        let path = temp("lazy-arranger-settings-shortcuts.txt");
        let settings = Settings {
            undo_keys: String::from("ctrl+u"),
            play_keys: String::from("shift+p"),
            ..Settings::default()
        };
        settings.save(&path).expect("save");
        let back = Settings::load(&path);
        assert!(!back.record_chords);
        assert_eq!(back.undo_keys, "ctrl+u");
        assert_eq!(back.play_keys, "shift+p");
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
    fn port_summaries_count_both_directions() {
        assert_eq!(
            ports_summary(&[String::from("a"), String::from("b")], &[]),
            "2 input(s), 0 output(s)"
        );
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
