//! Optional instrument editors.
//!
//! Some windows only make sense with one hardware instrument: the Roland D-05
//! sound editor, the Zoom L6max mixers, the PSR-S970 remote and so on. Each
//! user picks the instruments they own, and the editors for the others stay
//! out of the menus. The choice lives in its own small file beside the
//! settings (`instruments.txt`, one `key = on|off` line per instrument), so a
//! hand edit or a newer build does not disturb the main settings file.
//!
//! A fresh install starts with no instrument editors and asks once. An
//! existing install (a settings file is already there) keeps every editor, so
//! nothing disappears after an update.

use std::path::{Path, PathBuf};

/// The instrument-choice file name, inside [`crate::settings::Settings::data_dir`].
pub const FILE: &str = "instruments.txt";

/// One instrument that has its own editor windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Instrument {
    /// Roland D-05: the sound editor and its device profile.
    RolandD05,
    /// Zoom LiveTrak L6max: the two mixers and the take import.
    ZoomL6max,
    /// Yamaha PSR-S970: the remote window and its device profile.
    YamahaPsrS970,
    /// Yamaha XG synths: the XG part mixer.
    YamahaXg,
    /// Novation XioSynth: its device profile and sound editor.
    NovationXioSynth,
}

impl Instrument {
    /// Every instrument, in the order the chooser shows them.
    pub const ALL: [Self; 5] = [
        Self::RolandD05,
        Self::ZoomL6max,
        Self::YamahaPsrS970,
        Self::YamahaXg,
        Self::NovationXioSynth,
    ];

    /// Key used in the instrument file.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::RolandD05 => "d05",
            Self::ZoomL6max => "l6max",
            Self::YamahaPsrS970 => "psr-s970",
            Self::YamahaXg => "xg",
            Self::NovationXioSynth => "xiosynth",
        }
    }

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RolandD05 => "Roland D-05",
            Self::ZoomL6max => "Zoom LiveTrak L6max",
            Self::YamahaPsrS970 => "Yamaha PSR-S970",
            Self::YamahaXg => "Yamaha XG synth",
            Self::NovationXioSynth => "Novation XioSynth",
        }
    }

    /// What turning it on adds, for the chooser.
    #[must_use]
    pub const fn adds(self) -> &'static str {
        match self {
            Self::RolandD05 => "D-05 sound editor and device profile",
            Self::ZoomL6max => "L6max mixer, L6max device mixer and take import",
            Self::YamahaPsrS970 => "PSR-S970 remote and device profile",
            Self::YamahaXg => "XG part mixer",
            Self::NovationXioSynth => "XioSynth device profile and sound editor",
        }
    }

    /// Ids of the bundled device profiles that belong to this instrument.
    #[must_use]
    pub const fn profile_ids(self) -> &'static [&'static str] {
        match self {
            Self::RolandD05 => &["d05"],
            Self::YamahaPsrS970 => &["psr-s970"],
            Self::NovationXioSynth => &["xiosynth", "novation-xiosynth"],
            Self::ZoomL6max | Self::YamahaXg => &[],
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::RolandD05 => 0,
            Self::ZoomL6max => 1,
            Self::YamahaPsrS970 => 2,
            Self::YamahaXg => 3,
            Self::NovationXioSynth => 4,
        }
    }
}

/// Which instrument editors are installed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Installed {
    on: [bool; Instrument::ALL.len()],
}

impl Installed {
    /// No instrument editors.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            on: [false; Instrument::ALL.len()],
        }
    }

    /// Every instrument editor.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            on: [true; Instrument::ALL.len()],
        }
    }

    /// Whether `instrument`'s editors are installed.
    #[must_use]
    pub const fn has(&self, instrument: Instrument) -> bool {
        self.on[instrument.index()]
    }

    /// Installs or removes `instrument`'s editors.
    pub const fn set(&mut self, instrument: Instrument, on: bool) {
        self.on[instrument.index()] = on;
    }

    /// Whether a device profile should be listed. Profiles that belong to an
    /// instrument that is not installed are hidden; every other profile
    /// (General MIDI, GS, XG, user profiles) stays.
    #[must_use]
    pub fn allows_profile(&self, id: &str) -> bool {
        Instrument::ALL
            .iter()
            .filter(|instrument| instrument.profile_ids().contains(&id))
            .all(|instrument| self.has(*instrument))
    }

    /// Reads `key = on|off` lines. Unknown keys and bad lines are ignored, and
    /// a missing key means "not installed".
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut installed = Self::none();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if let Some(instrument) = Instrument::ALL.iter().find(|item| item.key() == key) {
                installed.set(
                    *instrument,
                    matches!(value.trim(), "on" | "true" | "yes" | "1"),
                );
            }
        }
        installed
    }

    /// The file text for these choices.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::from("# Lazy Arranger instrument editors (on or off)\n");
        for instrument in Instrument::ALL {
            text.push_str(&format!(
                "{} = {}\n",
                instrument.key(),
                if self.has(instrument) { "on" } else { "off" }
            ));
        }
        text
    }

    /// Reads the file at `path`, or `None` when it does not exist yet.
    #[must_use]
    pub fn load(path: &Path) -> Option<Self> {
        std::fs::read_to_string(path)
            .ok()
            .map(|text| Self::parse(&text))
    }

    /// Writes the file at `path`.
    ///
    /// # Errors
    ///
    /// Returns a message when the folder or file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        std::fs::write(path, self.to_text()).map_err(|error| format!("{}: {error}", path.display()))
    }

    /// The saved choices for this user; nothing installed before the first
    /// choice.
    #[must_use]
    pub fn current() -> Self {
        Self::load(&path()).unwrap_or_default()
    }
}

/// Path of the per-user instrument file.
#[must_use]
pub fn path() -> PathBuf {
    crate::settings::Settings::data_dir().join(FILE)
}

/// The choices to start with, and whether to ask the user.
///
/// * The instrument file exists: use it, do not ask.
/// * No instrument file but a settings file (an install from before this
///   choice existed): install everything, save that, do not ask.
/// * Neither (a fresh install): install nothing and ask.
#[must_use]
pub fn startup(data_dir: &Path) -> (Installed, bool) {
    let file = data_dir.join(FILE);
    if let Some(installed) = Installed::load(&file) {
        return (installed, false);
    }
    if data_dir.join(crate::settings::FILE).exists() {
        let installed = Installed::all();
        let _ = installed.save(&file);
        return (installed, false);
    }
    (Installed::none(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lazy-arranger-instruments-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    #[test]
    fn choices_round_trip() {
        let mut installed = Installed::none();
        installed.set(Instrument::RolandD05, true);
        installed.set(Instrument::YamahaXg, true);
        let parsed = Installed::parse(&installed.to_text());
        assert_eq!(parsed, installed);
        assert!(parsed.has(Instrument::RolandD05));
        assert!(!parsed.has(Instrument::ZoomL6max));
    }

    #[test]
    fn unknown_and_bad_lines_are_ignored() {
        let parsed = Installed::parse("# hi\nd05 = on\nmystery = on\nnonsense\nl6max=off\n");
        assert!(parsed.has(Instrument::RolandD05));
        assert!(!parsed.has(Instrument::ZoomL6max));
        assert!(!parsed.has(Instrument::YamahaPsrS970));
    }

    #[test]
    fn keys_are_unique() {
        for (index, instrument) in Instrument::ALL.iter().enumerate() {
            assert_eq!(instrument.index(), index);
            assert!(
                Instrument::ALL[index + 1..]
                    .iter()
                    .all(|other| other.key() != instrument.key())
            );
        }
    }

    #[test]
    fn only_owned_instrument_profiles_are_hidden() {
        let mut installed = Installed::none();
        assert!(!installed.allows_profile("d05"));
        assert!(!installed.allows_profile("psr-s970"));
        assert!(!installed.allows_profile("xiosynth"));
        assert!(installed.allows_profile("generic-gs"));
        assert!(installed.allows_profile("generic-xg"));
        assert!(installed.allows_profile("korg-pa"));
        assert!(installed.allows_profile("my-own-synth"));
        installed.set(Instrument::RolandD05, true);
        assert!(installed.allows_profile("d05"));
        assert!(Installed::all().allows_profile("xiosynth"));
    }

    #[test]
    fn a_fresh_install_asks_with_nothing_installed() {
        let dir = temp("fresh");
        let (installed, ask) = startup(&dir);
        assert!(ask);
        assert_eq!(installed, Installed::none());
        assert!(!dir.join(FILE).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_existing_install_keeps_every_editor() {
        let dir = temp("existing");
        std::fs::write(dir.join(crate::settings::FILE), "language = en\n").expect("settings");
        let (installed, ask) = startup(&dir);
        assert!(!ask);
        assert_eq!(installed, Installed::all());
        assert_eq!(Installed::load(&dir.join(FILE)), Some(Installed::all()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_saved_choice_wins() {
        let dir = temp("saved");
        std::fs::write(dir.join(crate::settings::FILE), "language = en\n").expect("settings");
        let mut choice = Installed::none();
        choice.set(Instrument::ZoomL6max, true);
        choice.save(&dir.join(FILE)).expect("save");
        let (installed, ask) = startup(&dir);
        assert!(!ask);
        assert_eq!(installed, choice);
        let _ = std::fs::remove_dir_all(dir);
    }
}
