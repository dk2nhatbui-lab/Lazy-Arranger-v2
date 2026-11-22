//! The 35 Yamaha chord types and the data that drives chord conversion.
//!
//! The vocabulary, the interval table and the scale table follow the C++
//! oracle (`src/core/src/StyleChord.cpp:20-39`, `StyleChord.h:13-24`). They
//! describe musical facts, not source expression, and are re-stated here in
//! Rust types with tests that pin the values.
//!
//! Chord conversion needs, for each type: the third, fifth and seventh
//! interval above the root (or none), and a seven-note scale that tells a
//! melody note which degree of the source chord it is.

use music_core::{Chord, ChordQuality, PitchClass};

/// A scale as seven semitone offsets from its root.
pub type Scale = [i32; 7];

/// The major scale.
pub const MAJOR: Scale = [0, 2, 4, 5, 7, 9, 11];
/// The lydian mode.
pub const LYDIAN: Scale = [0, 2, 4, 6, 7, 9, 11];
/// The mixolydian mode.
pub const MIXOLYDIAN: Scale = [0, 2, 4, 5, 7, 9, 10];
/// The dorian mode.
pub const DORIAN: Scale = [0, 2, 3, 5, 7, 9, 10];
/// The melodic minor scale.
pub const MELODIC_MINOR: Scale = [0, 2, 3, 5, 7, 9, 11];
/// The locrian mode.
pub const LOCRIAN: Scale = [0, 1, 3, 5, 6, 8, 10];
/// The whole-half diminished scale.
pub const DIMINISHED: Scale = [0, 2, 3, 5, 6, 8, 9];
/// The augmented scale used for augmented triads.
pub const AUGMENTED: Scale = [0, 2, 4, 5, 8, 9, 11];
/// The augmented scale used for augmented sevenths.
pub const AUGMENTED_SEVENTH: Scale = [0, 2, 4, 5, 8, 9, 10];

/// One row of the chord-type table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeInfo {
    /// Suffix used when the chord is rendered as text.
    pub suffix: &'static str,
    /// Interval of the third, or 0 when the type has none.
    pub third: i32,
    /// Interval of the fifth.
    pub fifth: i32,
    /// Interval of the seventh, or -1 when the type has none.
    pub seventh: i32,
    /// Scale that melody conversion reads for this type.
    pub scale: Scale,
}

/// The Yamaha chord vocabulary, in the file's chord-type order (0..=34).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChordType {
    /// Major triad.
    Major,
    /// Major sixth.
    Major6,
    /// Major seventh.
    Major7,
    /// Major seventh with a sharp eleventh.
    Major7Sharp11,
    /// Major triad with an added ninth.
    MajorAdd9,
    /// Major seventh with a ninth.
    Major7Nine,
    /// Major sixth with a ninth.
    Major6Nine,
    /// Augmented triad.
    Augmented,
    /// Minor triad.
    Minor,
    /// Minor sixth.
    Minor6,
    /// Minor seventh.
    Minor7,
    /// Half-diminished seventh.
    Minor7Flat5,
    /// Minor triad with an added ninth.
    MinorAdd9,
    /// Minor seventh with a ninth.
    Minor7Nine,
    /// Minor seventh with an eleventh.
    Minor7Eleven,
    /// Minor triad with a major seventh.
    MinorMajor7,
    /// Minor-major seventh with a ninth.
    MinorMajor7Nine,
    /// Diminished triad.
    Diminished,
    /// Diminished seventh.
    Diminished7,
    /// Dominant seventh.
    Seventh,
    /// Dominant seventh with a suspended fourth.
    SeventhSus4,
    /// Dominant seventh with a flat fifth.
    SeventhFlat5,
    /// Dominant seventh with a ninth.
    SeventhNine,
    /// Dominant seventh with a sharp eleventh.
    SeventhSharp11,
    /// Dominant seventh with a thirteenth.
    SeventhThirteen,
    /// Dominant seventh with a flat ninth.
    SeventhFlat9,
    /// Dominant seventh with a flat thirteenth.
    SeventhFlat13,
    /// Dominant seventh with a sharp ninth.
    SeventhSharp9,
    /// Major seventh with an augmented fifth.
    Major7Augmented,
    /// Dominant seventh with an augmented fifth.
    SeventhAugmented,
    /// Root and octave.
    Octave,
    /// Root and fifth.
    Fifth,
    /// Suspended fourth.
    Sus4,
    /// Suspended second.
    Sus2,
    /// No chord: every non-rhythm part stops.
    Cancel,
}

/// Every type in file order, used by index-based conversions and tests.
pub const ALL_TYPES: [ChordType; 35] = [
    ChordType::Major,
    ChordType::Major6,
    ChordType::Major7,
    ChordType::Major7Sharp11,
    ChordType::MajorAdd9,
    ChordType::Major7Nine,
    ChordType::Major6Nine,
    ChordType::Augmented,
    ChordType::Minor,
    ChordType::Minor6,
    ChordType::Minor7,
    ChordType::Minor7Flat5,
    ChordType::MinorAdd9,
    ChordType::Minor7Nine,
    ChordType::Minor7Eleven,
    ChordType::MinorMajor7,
    ChordType::MinorMajor7Nine,
    ChordType::Diminished,
    ChordType::Diminished7,
    ChordType::Seventh,
    ChordType::SeventhSus4,
    ChordType::SeventhFlat5,
    ChordType::SeventhNine,
    ChordType::SeventhSharp11,
    ChordType::SeventhThirteen,
    ChordType::SeventhFlat9,
    ChordType::SeventhFlat13,
    ChordType::SeventhSharp9,
    ChordType::Major7Augmented,
    ChordType::SeventhAugmented,
    ChordType::Octave,
    ChordType::Fifth,
    ChordType::Sus4,
    ChordType::Sus2,
    ChordType::Cancel,
];

/// Note names the oracle uses when it renders a chord: sharps for C# and F#,
/// flats elsewhere. Kept separate from `music_core` spelling so chord names in
/// the arranger match the C++ application.
pub const YAMAHA_NAMES: [&str; 12] = [
    "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

const TABLE: [TypeInfo; 35] = [
    TypeInfo {
        suffix: "",
        third: 4,
        fifth: 7,
        seventh: -1,
        scale: MAJOR,
    },
    TypeInfo {
        suffix: "6",
        third: 4,
        fifth: 7,
        seventh: 9,
        scale: MAJOR,
    },
    TypeInfo {
        suffix: "M7",
        third: 4,
        fifth: 7,
        seventh: 11,
        scale: MAJOR,
    },
    TypeInfo {
        suffix: "M7#11",
        third: 4,
        fifth: 7,
        seventh: 11,
        scale: LYDIAN,
    },
    TypeInfo {
        suffix: "add9",
        third: 4,
        fifth: 7,
        seventh: -1,
        scale: MAJOR,
    },
    TypeInfo {
        suffix: "M7(9)",
        third: 4,
        fifth: 7,
        seventh: 11,
        scale: MAJOR,
    },
    TypeInfo {
        suffix: "6(9)",
        third: 4,
        fifth: 7,
        seventh: 9,
        scale: MAJOR,
    },
    TypeInfo {
        suffix: "aug",
        third: 4,
        fifth: 8,
        seventh: -1,
        scale: AUGMENTED,
    },
    TypeInfo {
        suffix: "m",
        third: 3,
        fifth: 7,
        seventh: -1,
        scale: DORIAN,
    },
    TypeInfo {
        suffix: "m6",
        third: 3,
        fifth: 7,
        seventh: 9,
        scale: DORIAN,
    },
    TypeInfo {
        suffix: "m7",
        third: 3,
        fifth: 7,
        seventh: 10,
        scale: DORIAN,
    },
    TypeInfo {
        suffix: "m7b5",
        third: 3,
        fifth: 6,
        seventh: 10,
        scale: LOCRIAN,
    },
    TypeInfo {
        suffix: "m(9)",
        third: 3,
        fifth: 7,
        seventh: -1,
        scale: DORIAN,
    },
    TypeInfo {
        suffix: "m7(9)",
        third: 3,
        fifth: 7,
        seventh: 10,
        scale: DORIAN,
    },
    TypeInfo {
        suffix: "m7(11)",
        third: 3,
        fifth: 7,
        seventh: 10,
        scale: DORIAN,
    },
    TypeInfo {
        suffix: "mM7",
        third: 3,
        fifth: 7,
        seventh: 11,
        scale: MELODIC_MINOR,
    },
    TypeInfo {
        suffix: "mM7(9)",
        third: 3,
        fifth: 7,
        seventh: 11,
        scale: MELODIC_MINOR,
    },
    TypeInfo {
        suffix: "dim",
        third: 3,
        fifth: 6,
        seventh: -1,
        scale: DIMINISHED,
    },
    TypeInfo {
        suffix: "dim7",
        third: 3,
        fifth: 6,
        seventh: 9,
        scale: DIMINISHED,
    },
    TypeInfo {
        suffix: "7",
        third: 4,
        fifth: 7,
        seventh: 10,
        scale: MIXOLYDIAN,
    },
    TypeInfo {
        suffix: "7sus4",
        third: 5,
        fifth: 7,
        seventh: 10,
        scale: MIXOLYDIAN,
    },
    TypeInfo {
        suffix: "7b5",
        third: 4,
        fifth: 6,
        seventh: 10,
        scale: [0, 2, 4, 5, 6, 9, 10],
    },
    TypeInfo {
        suffix: "7(9)",
        third: 4,
        fifth: 7,
        seventh: 10,
        scale: MIXOLYDIAN,
    },
    TypeInfo {
        suffix: "7#11",
        third: 4,
        fifth: 7,
        seventh: 10,
        scale: [0, 2, 4, 6, 7, 9, 10],
    },
    TypeInfo {
        suffix: "7(13)",
        third: 4,
        fifth: 7,
        seventh: 10,
        scale: MIXOLYDIAN,
    },
    TypeInfo {
        suffix: "7(b9)",
        third: 4,
        fifth: 7,
        seventh: 10,
        scale: [0, 1, 4, 5, 7, 8, 10],
    },
    TypeInfo {
        suffix: "7(b13)",
        third: 4,
        fifth: 7,
        seventh: 10,
        scale: [0, 2, 4, 5, 7, 8, 10],
    },
    TypeInfo {
        suffix: "7(#9)",
        third: 4,
        fifth: 7,
        seventh: 10,
        scale: [0, 3, 4, 5, 7, 9, 10],
    },
    TypeInfo {
        suffix: "M7aug",
        third: 4,
        fifth: 8,
        seventh: 11,
        scale: AUGMENTED,
    },
    TypeInfo {
        suffix: "7aug",
        third: 4,
        fifth: 8,
        seventh: 10,
        scale: AUGMENTED_SEVENTH,
    },
    TypeInfo {
        suffix: "1+8",
        third: 0,
        fifth: 0,
        seventh: -1,
        scale: MAJOR,
    },
    TypeInfo {
        suffix: "1+5",
        third: 7,
        fifth: 7,
        seventh: -1,
        scale: MAJOR,
    },
    TypeInfo {
        suffix: "sus4",
        third: 5,
        fifth: 7,
        seventh: -1,
        scale: MIXOLYDIAN,
    },
    TypeInfo {
        suffix: "sus2",
        third: 2,
        fifth: 7,
        seventh: -1,
        scale: MAJOR,
    },
    TypeInfo {
        suffix: "",
        third: 4,
        fifth: 7,
        seventh: -1,
        scale: MAJOR,
    },
];

impl ChordType {
    /// Position of this type in the file's chord-type numbering.
    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    /// Converts a file chord-type number.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ArrangerError::ChordType`] for a number above 34.
    pub fn from_index(index: usize) -> Result<Self, crate::ArrangerError> {
        ALL_TYPES
            .get(index)
            .copied()
            .ok_or(crate::ArrangerError::ChordType(index))
    }

    /// Returns the interval, scale and suffix data for this type.
    #[must_use]
    pub fn info(self) -> &'static TypeInfo {
        &TABLE[self.as_index()]
    }

    /// Returns the suffix used in chord names.
    #[must_use]
    pub fn suffix(self) -> &'static str {
        self.info().suffix
    }

    /// Returns the chord tones as semitone offsets, including added notes.
    ///
    /// The first element is always 0. Extended notes keep their compound
    /// interval (a ninth is 14, not 2), because callers place them above the
    /// root rather than inside one octave.
    #[must_use]
    pub fn tones(self) -> Vec<i32> {
        let info = self.info();
        let mut tones = vec![0];
        for value in [info.third, info.fifth, info.seventh] {
            if value > 0 && !tones.contains(&value) {
                tones.push(value);
            }
        }
        let extra = match self {
            Self::MajorAdd9
            | Self::Major7Nine
            | Self::Major6Nine
            | Self::MinorAdd9
            | Self::Minor7Nine
            | Self::MinorMajor7Nine
            | Self::SeventhNine => Some(14),
            Self::Major7Sharp11 | Self::SeventhSharp11 => Some(18),
            Self::Minor7Eleven => Some(17),
            Self::SeventhThirteen => Some(21),
            Self::SeventhFlat9 => Some(13),
            Self::SeventhFlat13 => Some(20),
            Self::SeventhSharp9 => Some(15),
            Self::Octave => Some(12),
            _ => None,
        };
        if let Some(extra) = extra {
            tones.push(extra);
        }
        tones.sort_unstable();
        tones
    }

    /// Renders `root` and this type as a chord symbol, for example `Cm7b5`.
    #[must_use]
    pub fn name(self, root: PitchClass) -> String {
        if self == Self::Cancel {
            return String::from("-");
        }
        format!(
            "{}{}",
            YAMAHA_NAMES[usize::from(root.as_u8())],
            self.suffix()
        )
    }
}

impl std::fmt::Display for ChordType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.suffix())
    }
}

/// A chord as the style rules see it: root, Yamaha type and optional slash bass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArrangerChord {
    /// Root pitch class.
    pub root: PitchClass,
    /// Yamaha chord type.
    pub chord_type: ChordType,
    /// Slash bass, when the symbol names one.
    pub bass: Option<PitchClass>,
}

impl ArrangerChord {
    /// Creates a chord without a slash bass.
    #[must_use]
    pub const fn new(root: PitchClass, chord_type: ChordType) -> Self {
        Self {
            root,
            chord_type,
            bass: None,
        }
    }

    /// Adds a slash bass.
    #[must_use]
    pub const fn with_bass(self, bass: PitchClass) -> Self {
        Self {
            bass: Some(bass),
            ..self
        }
    }

    /// A chord that stops the accompaniment.
    #[must_use]
    pub const fn cancel() -> Self {
        Self::new(PitchClass::new(0), ChordType::Cancel)
    }

    /// Renders the chord as a symbol, adding the slash bass when present.
    #[must_use]
    pub fn name(self) -> String {
        let mut name = self.chord_type.name(self.root);
        if let Some(bass) = self.bass
            && self.chord_type != ChordType::Cancel
        {
            name.push('/');
            name.push_str(YAMAHA_NAMES[usize::from(bass.as_u8())]);
        }
        name
    }
}

/// Maps a platform chord quality onto the Yamaha vocabulary.
///
/// The 15 qualities this workspace parses cover the arranger types that
/// matter first. A quality with no arranger type is reported, not guessed.
impl TryFrom<&Chord> for ArrangerChord {
    type Error = crate::ArrangerError;

    fn try_from(chord: &Chord) -> Result<Self, Self::Error> {
        let chord_type = match chord.quality {
            ChordQuality::Major => ChordType::Major,
            ChordQuality::Minor => ChordType::Minor,
            ChordQuality::Dominant7 => ChordType::Seventh,
            ChordQuality::Major7 => ChordType::Major7,
            ChordQuality::Minor7 => ChordType::Minor7,
            ChordQuality::MinorMajor7 => ChordType::MinorMajor7,
            ChordQuality::Diminished => ChordType::Diminished,
            ChordQuality::Diminished7 => ChordType::Diminished7,
            ChordQuality::HalfDiminished7 => ChordType::Minor7Flat5,
            ChordQuality::Augmented => ChordType::Augmented,
            ChordQuality::Suspended2 => ChordType::Sus2,
            ChordQuality::Suspended4 => ChordType::Sus4,
            ChordQuality::Major6 => ChordType::Major6,
            ChordQuality::Minor6 => ChordType::Minor6,
            ChordQuality::Dominant7Sus4 => ChordType::SeventhSus4,
        };
        Ok(Self {
            root: chord.root,
            chord_type,
            bass: chord.bass,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_has_table_data_and_a_stable_index() {
        for (index, chord_type) in ALL_TYPES.into_iter().enumerate() {
            assert_eq!(chord_type.as_index(), index, "{chord_type:?}");
            assert_eq!(
                ChordType::from_index(index).expect("valid index"),
                chord_type
            );
            let info = chord_type.info();
            assert!(info.third >= 0 && info.third <= 11, "{chord_type:?}");
            assert!(info.fifth >= 0 && info.fifth <= 11, "{chord_type:?}");
            assert!(info.seventh <= 11, "{chord_type:?}");
            assert_eq!(info.scale[0], 0, "{chord_type:?}");
            assert!(info.scale.windows(2).all(|pair| pair[0] < pair[1]));
        }
        assert!(ChordType::from_index(35).is_err());
    }

    #[test]
    fn tones_match_the_oracle_examples() {
        assert_eq!(ChordType::Minor7.tones(), vec![0, 3, 7, 10]);
        assert_eq!(ChordType::SeventhNine.tones(), vec![0, 4, 7, 10, 14]);
        assert_eq!(ChordType::Diminished7.tones(), vec![0, 3, 6, 9]);
        assert_eq!(ChordType::Octave.tones(), vec![0, 12]);
        assert_eq!(ChordType::Fifth.tones(), vec![0, 7]);
        assert_eq!(ChordType::Sus4.tones(), vec![0, 5, 7]);
        // A type whose third and fifth are the same adds the interval once.
        assert_eq!(ChordType::Fifth.tones(), vec![0, 7]);
        for chord_type in ALL_TYPES {
            let tones = chord_type.tones();
            assert_eq!(tones[0], 0, "{chord_type:?}");
            assert!(tones.windows(2).all(|pair| pair[0] < pair[1]));
        }
    }

    #[test]
    fn names_match_the_oracle() {
        assert_eq!(ChordType::Major.name(PitchClass::new(0)), "C");
        assert_eq!(ChordType::Minor7.name(PitchClass::new(6)), "F#m7");
        assert_eq!(
            ChordType::SeventhSus4.name(PitchClass::new(10)),
            "Bb7sus4",
            "chord names use the oracle's mixed spelling"
        );
        assert_eq!(ChordType::Major.name(PitchClass::new(3)), "Eb");
        assert_eq!(ChordType::Major.name(PitchClass::new(6)), "F#");
        assert_eq!(ChordType::Cancel.name(PitchClass::new(0)), "-");
        let mut chord = ArrangerChord::new(PitchClass::new(0), ChordType::Major);
        assert_eq!(chord.name(), "C");
        chord = chord.with_bass(PitchClass::new(4));
        assert_eq!(chord.name(), "C/E");
        assert_eq!(ArrangerChord::cancel().name(), "-");
    }

    #[test]
    fn platform_qualities_map_onto_the_yamaha_vocabulary() {
        let cases = [
            ("C", ChordType::Major),
            ("Am", ChordType::Minor),
            ("G7", ChordType::Seventh),
            ("Cmaj7", ChordType::Major7),
            ("Dm7", ChordType::Minor7),
            ("CmM7", ChordType::MinorMajor7),
            ("Bdim", ChordType::Diminished),
            ("Bdim7", ChordType::Diminished7),
            ("Bm7b5", ChordType::Minor7Flat5),
            ("Caug", ChordType::Augmented),
            ("Csus2", ChordType::Sus2),
            ("Csus4", ChordType::Sus4),
            ("C6", ChordType::Major6),
            ("Cm6", ChordType::Minor6),
            ("C7sus4", ChordType::SeventhSus4),
        ];
        for (symbol, expected) in cases {
            let chord = Chord::parse(symbol).expect("valid symbol");
            let mapped = ArrangerChord::try_from(&chord).expect("mappable");
            assert_eq!(mapped.chord_type, expected, "{symbol}");
        }
        let slash = Chord::parse("C/E").expect("valid");
        let mapped = ArrangerChord::try_from(&slash).expect("mappable");
        assert_eq!(mapped.name(), "C/E");
    }
}
