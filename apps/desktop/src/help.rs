//! Short in-app help, translated for every shipped language.

use crate::strings::Language;

/// One help section.
#[derive(Debug)]
pub struct Section {
    /// Section heading.
    pub title: &'static str,
    /// Advice lines.
    pub lines: &'static [&'static str],
}

const EN: &[Section] = &[
    Section {
        title: "Getting started",
        lines: &[
            "Pick a song in Windows > Songs & tutorial.",
            "Press Play, Space or Cmd/Ctrl+P.",
            "Click the ruler to seek while playing.",
        ],
    },
    Section {
        title: "Arrange",
        lines: &[
            "Drag a note to move it; drag its right edge to resize it.",
            "Shift-click clips to select several; arrows move, Shift+arrows resize.",
            "Snap 1/16 toggles snapping.",
        ],
    },
    Section {
        title: "Tools",
        lines: &[
            "Windows opens the Mixer, Score, Lessons, Theory and more.",
            "Cmd/Ctrl+Z undoes one edit; Cmd/Ctrl+Shift+Z redoes it.",
        ],
    },
];

const DA: &[Section] = &[
    Section {
        title: "Kom godt i gang",
        lines: &[
            "Vælg en sang i Windows > Sange og vejledning.",
            "Tryk på Afspil, mellemrum eller Cmd/Ctrl+P.",
            "Klik på linealen for at springe, mens der afspilles.",
        ],
    },
    Section {
        title: "Arranger",
        lines: &[
            "Træk en node for at flytte den; træk i højre kant for at ændre længden.",
            "Shift-klik på klip for at vælge flere; piletaster flytter, Shift+piletaster ændrer længde.",
            "Snap 1/16 slår snapping til og fra.",
        ],
    },
    Section {
        title: "Værktøjer",
        lines: &[
            "Windows åbner Mixer, Partitur, Lektioner, Teori og mere.",
            "Cmd/Ctrl+Z fortryder én ændring; Cmd/Ctrl+Shift+Z gentager den.",
        ],
    },
];

const FR: &[Section] = &[
    Section {
        title: "Pour commencer",
        lines: &[
            "Choisissez un morceau dans Windows > Morceaux et tutoriel.",
            "Appuyez sur Lecture, Espace ou Cmd/Ctrl+P.",
            "Cliquez sur la règle pour vous déplacer pendant la lecture.",
        ],
    },
    Section {
        title: "Arrangement",
        lines: &[
            "Faites glisser une note pour la déplacer ; faites glisser son bord droit pour la redimensionner.",
            "Maj-clic sur des clips pour en sélectionner plusieurs ; les flèches déplacent, Maj+flèches redimensionnent.",
            "Snap 1/16 active ou désactive l'aimantation.",
        ],
    },
    Section {
        title: "Outils",
        lines: &[
            "Windows ouvre le Mixeur, la Partition, les Leçons, la Théorie et plus.",
            "Cmd/Ctrl+Z annule une modification ; Cmd/Ctrl+Shift+Z la rétablit.",
        ],
    },
];

const VI: &[Section] = &[
    Section {
        title: "Bắt đầu",
        lines: &[
            "Chọn một bài trong Windows > Bài hát và hướng dẫn.",
            "Nhấn Phát, phím cách hoặc Cmd/Ctrl+P.",
            "Nhấp vào thước để tua trong khi phát.",
        ],
    },
    Section {
        title: "Sắp xếp",
        lines: &[
            "Kéo nốt để di chuyển; kéo cạnh phải để đổi độ dài.",
            "Shift-nhấp clip để chọn nhiều; phím mũi tên di chuyển, Shift+mũi tên đổi độ dài.",
            "Snap 1/16 bật hoặc tắt bám dính.",
        ],
    },
    Section {
        title: "Công cụ",
        lines: &[
            "Windows mở Mixer, Bản nhạc, Bài học, Lý thuyết và hơn nữa.",
            "Cmd/Ctrl+Z hoàn tác một chỉnh sửa; Cmd/Ctrl+Shift+Z làm lại.",
        ],
    },
];

const DE: &[Section] = &[
    Section {
        title: "Erste Schritte",
        lines: &[
            "Wählen Sie einen Song unter Windows > Songs & Tutorial.",
            "Drücken Sie Wiedergabe, Leertaste oder Cmd/Ctrl+P.",
            "Klicken Sie auf das Lineal, um während der Wiedergabe zu springen.",
        ],
    },
    Section {
        title: "Arrangieren",
        lines: &[
            "Ziehen Sie eine Note zum Verschieben; ziehen Sie die rechte Kante zum Ändern der Länge.",
            "Shift-Klick auf Clips wählt mehrere; Pfeile verschieben, Shift+Pfeile ändern die Länge.",
            "Snap 1/16 schaltet das Einrasten um.",
        ],
    },
    Section {
        title: "Werkzeuge",
        lines: &[
            "Windows öffnet Mixer, Partitur, Lektionen, Theorie und mehr.",
            "Cmd/Ctrl+Z macht eine Änderung rückgängig; Cmd/Ctrl+Shift+Z stellt sie wieder her.",
        ],
    },
];

const ES: &[Section] = &[
    Section {
        title: "Primeros pasos",
        lines: &[
            "Elige una canción en Windows > Canciones y tutorial.",
            "Pulsa Reproducir, Espacio o Cmd/Ctrl+P.",
            "Haz clic en la regla para saltar mientras suena.",
        ],
    },
    Section {
        title: "Arreglo",
        lines: &[
            "Arrastra una nota para moverla; arrastra su borde derecho para cambiar la duración.",
            "Mayús-clic en clips selecciona varios; las flechas mueven, Mayús+flechas cambian la duración.",
            "Snap 1/16 activa o desactiva el ajuste.",
        ],
    },
    Section {
        title: "Herramientas",
        lines: &[
            "Windows abre Mezclador, Partitura, Lecciones, Teoría y más.",
            "Cmd/Ctrl+Z deshace un cambio; Cmd/Ctrl+Shift+Z lo rehace.",
        ],
    },
];

const IT: &[Section] = &[
    Section {
        title: "Per iniziare",
        lines: &[
            "Scegli un brano in Windows > Brani e tutorial.",
            "Premi Riproduci, Spazio o Cmd/Ctrl+P.",
            "Clicca sul righello per spostarti durante la riproduzione.",
        ],
    },
    Section {
        title: "Arrangiamento",
        lines: &[
            "Trascina una nota per spostarla; trascina il bordo destro per cambiarne la durata.",
            "Maiusc-clic sui clip per selezionarne più di uno; le frecce spostano, Maiusc+frecce ridimensionano.",
            "Snap 1/16 attiva o disattiva l'aggancio.",
        ],
    },
    Section {
        title: "Strumenti",
        lines: &[
            "Windows apre Mixer, Partitura, Lezioni, Teoria e altro.",
            "Cmd/Ctrl+Z annulla una modifica; Cmd/Ctrl+Shift+Z la ripristina.",
        ],
    },
];

const PT: &[Section] = &[
    Section {
        title: "Primeiros passos",
        lines: &[
            "Escolha uma música em Windows > Músicas e tutorial.",
            "Pressione Reproduzir, Espaço ou Cmd/Ctrl+P.",
            "Clique na régua para saltar durante a reprodução.",
        ],
    },
    Section {
        title: "Arranjo",
        lines: &[
            "Arraste uma nota para movê-la; arraste a borda direita para redimensionar.",
            "Shift-clique nos clipes para selecionar vários; as setas movem, Shift+setas redimensionam.",
            "Snap 1/16 ativa ou desativa o encaixe.",
        ],
    },
    Section {
        title: "Ferramentas",
        lines: &[
            "Windows abre Mixer, Partitura, Lições, Teoria e mais.",
            "Cmd/Ctrl+Z desfaz uma alteração; Cmd/Ctrl+Shift+Z refaz.",
        ],
    },
];

/// Help for one language.
#[must_use]
pub fn sections(language: Language) -> &'static [Section] {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_three_non_empty_sections() {
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
            let sections = sections(language);
            assert_eq!(sections.len(), 3);
            for section in sections {
                assert!(!section.title.is_empty());
                assert!(!section.lines.is_empty());
                assert!(section.lines.iter().all(|line| !line.is_empty()));
            }
        }
        assert_eq!(sections(Language::Fr)[0].title, "Pour commencer");
        assert_eq!(sections(Language::Es)[0].title, "Primeros pasos");
        assert_eq!(sections(Language::It)[0].title, "Per iniziare");
        assert_eq!(sections(Language::Pt)[0].title, "Primeiros passos");
    }
}
