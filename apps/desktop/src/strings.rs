//! User-facing strings in the shipped languages.
//!
//! Two languages ship: English and Danish. Every key must have a non-empty
//! entry in both tables; a missing language falls back to English.

/// A language the app can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// English.
    En,
    /// Danish.
    Da,
}

impl Language {
    /// Parses a language tag, falling back to English.
    #[must_use]
    pub fn parse(tag: &str) -> Self {
        match tag.trim().to_ascii_lowercase().as_str() {
            "da" | "danish" => Self::Da,
            _ => Self::En,
        }
    }

    /// The tag written to settings.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Da => "da",
        }
    }
}

/// A translatable string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Perform tab.
    Perform,
    /// Arrange tab.
    Arrange,
    /// Mix tab.
    Mix,
    /// Loops tab.
    Loops,
    /// Score tab.
    Score,
    /// Save.
    Save,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Play.
    Play,
    /// Stop.
    Stop,
    /// Settings.
    Settings,
    /// Shortcuts.
    Shortcuts,
    /// Recent.
    Recent,
    /// Tracks.
    Tracks,
    /// Kids Mode banner.
    KidsMode,
    /// Open the demo.
    OpenDemo,
    /// No project loaded.
    NoProject,
    /// Saved.
    Saved,
    /// Playback stopped.
    Stopped,
}

const EN: &[(&str, &str)] = &[
    ("Perform", "Perform"),
    ("Arrange", "Arrange"),
    ("Mix", "Mix"),
    ("Loops", "Loops"),
    ("Score", "Score"),
    ("Save", "Save"),
    ("Undo", "Undo"),
    ("Redo", "Redo"),
    ("Play", "Play"),
    ("Stop", "Stop"),
    ("Settings", "Settings"),
    ("Shortcuts", "Shortcuts"),
    ("Recent", "Recent"),
    ("Tracks", "Tracks"),
    ("Kids Mode", "KIDS MODE"),
    ("Open demo", "Open demo"),
    ("No project", "No project loaded"),
    ("Saved", "saved"),
    ("Stopped", "stopped"),
];

const DA: &[(&str, &str)] = &[
    ("Perform", "Spil"),
    ("Arrange", "Arranger"),
    ("Mix", "Mix"),
    ("Loops", "Loops"),
    ("Score", "Partitur"),
    ("Save", "Gem"),
    ("Undo", "Fortryd"),
    ("Redo", "Gentag"),
    ("Play", "Afspil"),
    ("Stop", "Stop"),
    ("Settings", "Indstillinger"),
    ("Shortcuts", "Genveje"),
    ("Recent", "Seneste"),
    ("Tracks", "Spor"),
    ("Kids Mode", "BØRNETILSTAND"),
    ("Open demo", "Åbn demo"),
    ("No project", "Intet projekt indlæst"),
    ("Saved", "gemt"),
    ("Stopped", "stoppet"),
];

/// Returns the English key text, used to look up translations.
const fn label(key: Key) -> &'static str {
    match key {
        Key::Perform => "Perform",
        Key::Arrange => "Arrange",
        Key::Mix => "Mix",
        Key::Loops => "Loops",
        Key::Score => "Score",
        Key::Save => "Save",
        Key::Undo => "Undo",
        Key::Redo => "Redo",
        Key::Play => "Play",
        Key::Stop => "Stop",
        Key::Settings => "Settings",
        Key::Shortcuts => "Shortcuts",
        Key::Recent => "Recent",
        Key::Tracks => "Tracks",
        Key::KidsMode => "Kids Mode",
        Key::OpenDemo => "Open demo",
        Key::NoProject => "No project",
        Key::Saved => "Saved",
        Key::Stopped => "Stopped",
    }
}

fn table(language: Language) -> &'static [(&'static str, &'static str)] {
    match language {
        Language::En => EN,
        Language::Da => DA,
    }
}

/// The text for a key in a language.
#[must_use]
pub fn text(language: Language, key: Key) -> &'static str {
    let wanted = label(key);
    table(language)
        .iter()
        .find(|(name, _)| *name == wanted)
        .map(|(_, value)| *value)
        .or_else(|| {
            table(Language::En)
                .iter()
                .find(|(name, _)| *name == wanted)
                .map(|(_, value)| *value)
        })
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Key; 19] = [
        Key::Perform,
        Key::Arrange,
        Key::Mix,
        Key::Loops,
        Key::Score,
        Key::Save,
        Key::Undo,
        Key::Redo,
        Key::Play,
        Key::Stop,
        Key::Settings,
        Key::Shortcuts,
        Key::Recent,
        Key::Tracks,
        Key::KidsMode,
        Key::OpenDemo,
        Key::NoProject,
        Key::Saved,
        Key::Stopped,
    ];

    #[test]
    fn every_key_has_text_in_both_languages() {
        for key in ALL {
            assert!(!text(Language::En, key).is_empty(), "{key:?} in English");
            assert!(!text(Language::Da, key).is_empty(), "{key:?} in Danish");
        }
        assert_eq!(text(Language::Da, Key::Save), "Gem");
        assert_eq!(text(Language::Da, Key::Score), "Partitur");
        assert_ne!(
            text(Language::Da, Key::Arrange),
            text(Language::En, Key::Arrange)
        );
    }

    #[test]
    fn unknown_languages_fall_back_to_english() {
        assert_eq!(Language::parse("fr"), Language::En);
        assert_eq!(Language::parse("da"), Language::Da);
        assert_eq!(Language::parse("DA").tag(), "da");
    }
}
