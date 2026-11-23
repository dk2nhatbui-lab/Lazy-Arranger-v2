//! User-facing strings in the shipped languages.
//!
//! Eight languages ship: English, Danish, French, Vietnamese, German, Spanish,
//! Italian and Portuguese.
//! Every key must have a non-empty entry in every table; a missing language
//! falls back to English.

/// A language the app can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// English.
    En,
    /// Danish.
    Da,
    /// French.
    Fr,
    /// Vietnamese.
    Vi,
    /// German.
    De,
    /// Spanish.
    Es,
    /// Italian.
    It,
    /// Portuguese.
    Pt,
}

/// Noun pairs used by count labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nouns {
    /// One note / many notes.
    pub note: (&'static str, &'static str),
    /// One clip / many clips.
    pub clip: (&'static str, &'static str),
    /// One automation lane / many.
    pub lane: (&'static str, &'static str),
}

/// Noun pairs for one language.
#[must_use]
pub fn nouns(language: Language) -> Nouns {
    match language {
        Language::En => Nouns {
            note: ("note", "notes"),
            clip: ("clip", "clips"),
            lane: ("automation lane", "automation lanes"),
        },
        Language::Da => Nouns {
            note: ("node", "noder"),
            clip: ("klip", "klip"),
            lane: ("automatiseringsspor", "automatiseringsspor"),
        },
        Language::Fr => Nouns {
            note: ("note", "notes"),
            clip: ("clip", "clips"),
            lane: ("piste d'automatisation", "pistes d'automatisation"),
        },
        Language::Vi => Nouns {
            note: ("nốt", "nốt"),
            clip: ("clip", "clip"),
            lane: ("làn tự động hóa", "làn tự động hóa"),
        },
        Language::De => Nouns {
            note: ("Note", "Noten"),
            clip: ("Clip", "Clips"),
            lane: ("Automationsspur", "Automationsspuren"),
        },
        Language::Es => Nouns {
            note: ("nota", "notas"),
            clip: ("clip", "clips"),
            lane: ("pista de automatización", "pistas de automatización"),
        },
        Language::It => Nouns {
            note: ("nota", "note"),
            clip: ("clip", "clip"),
            lane: ("traccia di automazione", "tracce di automazione"),
        },
        Language::Pt => Nouns {
            note: ("nota", "notas"),
            clip: ("clipe", "clipes"),
            lane: ("pista de automação", "pistas de automação"),
        },
    }
}

/// Counts a noun with the language's plural rule.
#[must_use]
pub fn count(language: Language, count: usize, one: &str, many: &str) -> String {
    let plural = match language {
        Language::Fr => count > 1,
        Language::Vi => false,
        Language::En | Language::Da | Language::De | Language::Es | Language::It | Language::Pt => {
            count != 1
        }
    };
    format!("{count} {}", if plural { many } else { one })
}

impl Language {
    /// Parses a language tag, falling back to English.
    #[must_use]
    pub fn parse(tag: &str) -> Self {
        match tag.trim().to_ascii_lowercase().as_str() {
            "da" | "danish" => Self::Da,
            "fr" | "french" => Self::Fr,
            "vi" | "vietnamese" => Self::Vi,
            "de" | "german" => Self::De,
            "es" | "spanish" => Self::Es,
            "it" | "italian" => Self::It,
            "pt" | "portuguese" => Self::Pt,
            _ => Self::En,
        }
    }

    /// The tag written to settings.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Da => "da",
            Self::Fr => "fr",
            Self::Vi => "vi",
            Self::De => "de",
            Self::Es => "es",
            Self::It => "it",
            Self::Pt => "pt",
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

const FR: &[(&str, &str)] = &[
    ("Perform", "Jouer"),
    ("Arrange", "Arranger"),
    ("Mix", "Mixage"),
    ("Loops", "Boucles"),
    ("Score", "Partition"),
    ("Save", "Enregistrer"),
    ("Undo", "Annuler"),
    ("Redo", "Rétablir"),
    ("Play", "Lecture"),
    ("Stop", "Arrêt"),
    ("Settings", "Réglages"),
    ("Shortcuts", "Raccourcis"),
    ("Recent", "Récents"),
    ("Tracks", "Pistes"),
    ("Kids Mode", "MODE ENFANT"),
    ("Open demo", "Ouvrir la démo"),
    ("No project", "Aucun projet chargé"),
    ("Saved", "enregistré"),
    ("Stopped", "arrêté"),
];

const VI: &[(&str, &str)] = &[
    ("Perform", "Biểu diễn"),
    ("Arrange", "Sắp xếp"),
    ("Mix", "Trộn"),
    ("Loops", "Vòng lặp"),
    ("Score", "Bản nhạc"),
    ("Save", "Lưu"),
    ("Undo", "Hoàn tác"),
    ("Redo", "Làm lại"),
    ("Play", "Phát"),
    ("Stop", "Dừng"),
    ("Settings", "Cài đặt"),
    ("Shortcuts", "Phím tắt"),
    ("Recent", "Gần đây"),
    ("Tracks", "Kênh"),
    ("Kids Mode", "CHẾ ĐỘ TRẺ EM"),
    ("Open demo", "Mở bản demo"),
    ("No project", "Chưa mở dự án"),
    ("Saved", "đã lưu"),
    ("Stopped", "đã dừng"),
];

const DE: &[(&str, &str)] = &[
    ("Perform", "Spielen"),
    ("Arrange", "Arrangieren"),
    ("Mix", "Mischen"),
    ("Loops", "Loops"),
    ("Score", "Partitur"),
    ("Save", "Speichern"),
    ("Undo", "Rückgängig"),
    ("Redo", "Wiederholen"),
    ("Play", "Wiedergabe"),
    ("Stop", "Stopp"),
    ("Settings", "Einstellungen"),
    ("Shortcuts", "Tastenkürzel"),
    ("Recent", "Zuletzt"),
    ("Tracks", "Spuren"),
    ("Kids Mode", "KINDERMODUS"),
    ("Open demo", "Demo öffnen"),
    ("No project", "Kein Projekt geladen"),
    ("Saved", "gespeichert"),
    ("Stopped", "gestoppt"),
];

const ES: &[(&str, &str)] = &[
    ("Perform", "Tocar"),
    ("Arrange", "Organizar"),
    ("Mix", "Mezcla"),
    ("Loops", "Bucles"),
    ("Score", "Partitura"),
    ("Save", "Guardar"),
    ("Undo", "Deshacer"),
    ("Redo", "Rehacer"),
    ("Play", "Reproducir"),
    ("Stop", "Parar"),
    ("Settings", "Ajustes"),
    ("Shortcuts", "Atajos"),
    ("Recent", "Recientes"),
    ("Tracks", "Pistas"),
    ("Kids Mode", "MODO NIÑOS"),
    ("Open demo", "Abrir demo"),
    ("No project", "Ningún proyecto cargado"),
    ("Saved", "guardado"),
    ("Stopped", "detenido"),
];

const IT: &[(&str, &str)] = &[
    ("Perform", "Esegui"),
    ("Arrange", "Arrangiamento"),
    ("Mix", "Mix"),
    ("Loops", "Loop"),
    ("Score", "Partitura"),
    ("Save", "Salva"),
    ("Undo", "Annulla"),
    ("Redo", "Ripeti"),
    ("Play", "Riproduci"),
    ("Stop", "Ferma"),
    ("Settings", "Impostazioni"),
    ("Shortcuts", "Scorciatoie"),
    ("Recent", "Recenti"),
    ("Tracks", "Tracce"),
    ("Kids Mode", "MODALITÀ BAMBINI"),
    ("Open demo", "Apri demo"),
    ("No project", "Nessun progetto caricato"),
    ("Saved", "salvato"),
    ("Stopped", "fermato"),
];

const PT: &[(&str, &str)] = &[
    ("Perform", "Executar"),
    ("Arrange", "Arranjo"),
    ("Mix", "Mixagem"),
    ("Loops", "Loops"),
    ("Score", "Partitura"),
    ("Save", "Salvar"),
    ("Undo", "Desfazer"),
    ("Redo", "Refazer"),
    ("Play", "Reproduzir"),
    ("Stop", "Parar"),
    ("Settings", "Configurações"),
    ("Shortcuts", "Atalhos"),
    ("Recent", "Recentes"),
    ("Tracks", "Faixas"),
    ("Kids Mode", "MODO CRIANÇA"),
    ("Open demo", "Abrir demo"),
    ("No project", "Nenhum projeto carregado"),
    ("Saved", "salvo"),
    ("Stopped", "parado"),
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
        Language::Fr => FR,
        Language::Vi => VI,
        Language::De => DE,
        Language::Es => ES,
        Language::It => IT,
        Language::Pt => PT,
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
            assert!(!text(Language::Fr, key).is_empty(), "{key:?} in French");
            assert!(!text(Language::Vi, key).is_empty(), "{key:?} in Vietnamese");
            assert!(!text(Language::De, key).is_empty(), "{key:?} in German");
            assert!(!text(Language::Es, key).is_empty(), "{key:?} in Spanish");
            assert!(!text(Language::It, key).is_empty(), "{key:?} in Italian");
            assert!(!text(Language::Pt, key).is_empty(), "{key:?} in Portuguese");
        }
        assert_eq!(text(Language::Da, Key::Save), "Gem");
        assert_eq!(text(Language::Da, Key::Score), "Partitur");
        assert_eq!(text(Language::De, Key::Save), "Speichern");
        assert_eq!(text(Language::Es, Key::Save), "Guardar");
        assert_eq!(text(Language::It, Key::Save), "Salva");
        assert_eq!(text(Language::Pt, Key::Save), "Salvar");
        assert_eq!(Language::parse("de"), Language::De);
        assert_eq!(Language::parse("es"), Language::Es);
        assert_eq!(Language::parse("it"), Language::It);
        assert_eq!(Language::parse("pt"), Language::Pt);
        assert_ne!(
            text(Language::Da, Key::Arrange),
            text(Language::En, Key::Arrange)
        );
    }

    #[test]
    fn plural_rules_follow_the_language() {
        assert_eq!(count(Language::En, 0, "note", "notes"), "0 notes");
        assert_eq!(count(Language::En, 1, "note", "notes"), "1 note");
        assert_eq!(count(Language::Fr, 0, "note", "notes"), "0 note");
        assert_eq!(count(Language::Fr, 2, "note", "notes"), "2 notes");
        assert_eq!(count(Language::Vi, 3, "nốt", "nốt"), "3 nốt");
        assert_eq!(count(Language::De, 1, "Note", "Noten"), "1 Note");
        assert_eq!(count(Language::Es, 2, "nota", "notas"), "2 notas");
        assert_eq!(count(Language::It, 1, "nota", "note"), "1 nota");
        assert_eq!(count(Language::Pt, 2, "nota", "notas"), "2 notas");
        for language in [
            Language::En,
            Language::Da,
            Language::Fr,
            Language::Vi,
            Language::De,
            Language::Es,
            Language::It,
            Language::Pt,
        ] {
            let nouns = nouns(language);
            assert!(!nouns.note.0.is_empty() && !nouns.note.1.is_empty());
            assert!(!nouns.clip.0.is_empty() && !nouns.clip.1.is_empty());
            assert!(!nouns.lane.0.is_empty() && !nouns.lane.1.is_empty());
        }
    }

    #[test]
    fn unknown_languages_fall_back_to_english() {
        assert_eq!(Language::parse("xx"), Language::En);
        assert_eq!(Language::parse("da"), Language::Da);
        assert_eq!(Language::parse("DA").tag(), "da");
    }
}
