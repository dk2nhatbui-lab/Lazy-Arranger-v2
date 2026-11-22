use serde::{Deserialize, Serialize};

use crate::MusicError;
use crate::pitch::{PitchClass, parse_pitch_class};

/// How a chord symbol is spelled when it is rendered as text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spelling {
    /// Use sharp names (`C#`, `F#`).
    #[default]
    Sharp,
    /// Use flat names (`Db`, `Gb`).
    Flat,
}

/// One chord quality: the interval set that sits above the root.
///
/// The set is the common arranger-keyboard vocabulary. It is closed on
/// purpose: parsing, display and voicing all read one private table
/// (`Qualities::TABLE`), so adding a quality means adding one row and one
/// variant and they cannot drift apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChordQuality {
    /// Major triad: 0 4 7.
    Major,
    /// Minor triad: 0 3 7.
    Minor,
    /// Dominant seventh: 0 4 7 10.
    Dominant7,
    /// Major seventh: 0 4 7 11.
    Major7,
    /// Minor seventh: 0 3 7 10.
    Minor7,
    /// Minor triad with a major seventh: 0 3 7 11.
    MinorMajor7,
    /// Diminished triad: 0 3 6.
    Diminished,
    /// Diminished seventh: 0 3 6 9.
    Diminished7,
    /// Half-diminished seventh: 0 3 6 10.
    HalfDiminished7,
    /// Augmented triad: 0 4 8.
    Augmented,
    /// Suspended second: 0 2 7.
    Suspended2,
    /// Suspended fourth: 0 5 7.
    Suspended4,
    /// Major sixth: 0 4 7 9.
    Major6,
    /// Minor sixth: 0 3 7 9.
    Minor6,
    /// Dominant seventh with a suspended fourth: 0 5 7 10.
    Dominant7Sus4,
}

/// Parsing and display data for every [`ChordQuality`].
struct QualityDef {
    quality: ChordQuality,
    suffix: &'static str,
    aliases: &'static [&'static str],
    intervals: &'static [u8],
}

/// The single source of truth binding quality, symbol text and intervals.
struct Qualities;

impl Qualities {
    const TABLE: &'static [QualityDef] = &[
        QualityDef {
            quality: ChordQuality::HalfDiminished7,
            suffix: "m7b5",
            aliases: &["\u{00f8}7", "\u{00f8}", "min7b5"],
            intervals: &[0, 3, 6, 10],
        },
        QualityDef {
            quality: ChordQuality::Dominant7Sus4,
            suffix: "7sus4",
            aliases: &["7sus"],
            intervals: &[0, 5, 7, 10],
        },
        QualityDef {
            quality: ChordQuality::MinorMajor7,
            suffix: "mM7",
            aliases: &["minmaj7", "mMaj7"],
            intervals: &[0, 3, 7, 11],
        },
        QualityDef {
            quality: ChordQuality::Diminished7,
            suffix: "dim7",
            aliases: &["\u{00b0}7", "o7"],
            intervals: &[0, 3, 6, 9],
        },
        QualityDef {
            quality: ChordQuality::Diminished,
            suffix: "dim",
            aliases: &["\u{00b0}", "o"],
            intervals: &[0, 3, 6],
        },
        QualityDef {
            quality: ChordQuality::Augmented,
            suffix: "aug",
            aliases: &["+"],
            intervals: &[0, 4, 8],
        },
        QualityDef {
            quality: ChordQuality::Major7,
            suffix: "maj7",
            aliases: &["M7", "\u{0394}7", "ma7"],
            intervals: &[0, 4, 7, 11],
        },
        QualityDef {
            quality: ChordQuality::Minor7,
            suffix: "m7",
            aliases: &["min7", "-7"],
            intervals: &[0, 3, 7, 10],
        },
        QualityDef {
            quality: ChordQuality::Suspended2,
            suffix: "sus2",
            aliases: &[],
            intervals: &[0, 2, 7],
        },
        QualityDef {
            quality: ChordQuality::Suspended4,
            suffix: "sus4",
            aliases: &["sus"],
            intervals: &[0, 5, 7],
        },
        QualityDef {
            quality: ChordQuality::Major6,
            suffix: "6",
            aliases: &["maj6"],
            intervals: &[0, 4, 7, 9],
        },
        QualityDef {
            quality: ChordQuality::Minor6,
            suffix: "m6",
            aliases: &["min6"],
            intervals: &[0, 3, 7, 9],
        },
        QualityDef {
            quality: ChordQuality::Dominant7,
            suffix: "7",
            aliases: &["dom7"],
            intervals: &[0, 4, 7, 10],
        },
        QualityDef {
            quality: ChordQuality::Minor,
            suffix: "m",
            aliases: &["min", "-"],
            intervals: &[0, 3, 7],
        },
        QualityDef {
            quality: ChordQuality::Major,
            suffix: "",
            aliases: &["maj"],
            intervals: &[0, 4, 7],
        },
    ];

    fn def(quality: ChordQuality) -> &'static QualityDef {
        Self::TABLE
            .iter()
            .find(|def| def.quality == quality)
            .expect("every ChordQuality has a table row")
    }
}

impl ChordQuality {
    /// Returns the semitone offsets above the root, starting at 0.
    #[must_use]
    pub fn intervals(self) -> &'static [u8] {
        Qualities::def(self).intervals
    }

    /// Returns the canonical symbol suffix, empty for a major triad.
    #[must_use]
    pub fn suffix(self) -> &'static str {
        Qualities::def(self).suffix
    }

    /// Parses a symbol suffix such as `m7` or `maj7`; empty means major.
    #[must_use]
    pub fn from_suffix(suffix: &str) -> Option<Self> {
        Qualities::TABLE
            .iter()
            .find(|def| def.suffix == suffix || def.aliases.contains(&suffix))
            .map(|def| def.quality)
    }
}

impl std::fmt::Display for ChordQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.suffix())
    }
}

impl Serialize for ChordQuality {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.serde_name())
    }
}

impl<'de> Deserialize<'de> for ChordQuality {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::from_serde_name(&raw)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown chord quality {raw:?}")))
    }
}

impl ChordQuality {
    const fn serde_name(self) -> &'static str {
        match self {
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Dominant7 => "dominant-7",
            Self::Major7 => "major-7",
            Self::Minor7 => "minor-7",
            Self::MinorMajor7 => "minor-major-7",
            Self::Diminished => "diminished",
            Self::Diminished7 => "diminished-7",
            Self::HalfDiminished7 => "half-diminished-7",
            Self::Augmented => "augmented",
            Self::Suspended2 => "suspended-2",
            Self::Suspended4 => "suspended-4",
            Self::Major6 => "major-6",
            Self::Minor6 => "minor-6",
            Self::Dominant7Sus4 => "dominant-7-sus-4",
        }
    }

    fn from_serde_name(name: &str) -> Option<Self> {
        [
            Self::Major,
            Self::Minor,
            Self::Dominant7,
            Self::Major7,
            Self::Minor7,
            Self::MinorMajor7,
            Self::Diminished,
            Self::Diminished7,
            Self::HalfDiminished7,
            Self::Augmented,
            Self::Suspended2,
            Self::Suspended4,
            Self::Major6,
            Self::Minor6,
            Self::Dominant7Sus4,
        ]
        .into_iter()
        .find(|quality| quality.serde_name() == name)
    }
}

/// A chord symbol: root, quality and an optional slash bass.
///
/// `bass` is the pitch class under the chord, not the lowest chord tone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chord {
    /// The root pitch class.
    pub root: PitchClass,
    /// The quality above the root.
    pub quality: ChordQuality,
    /// The slash bass, when the symbol names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bass: Option<PitchClass>,
}

impl Chord {
    /// Creates a chord without a slash bass.
    #[must_use]
    pub const fn new(root: PitchClass, quality: ChordQuality) -> Self {
        Self {
            root,
            quality,
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

    /// Parses a chord symbol such as `C`, `Am7`, `F#dim7` or `G7/B`.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::Chord`] when the root, quality or slash bass is
    /// not understood.
    pub fn parse(symbol: &str) -> Result<Self, MusicError> {
        let text = symbol.trim();
        if text.is_empty() {
            return Err(MusicError::Chord(String::from("empty chord symbol")));
        }
        let (head, bass_part) = match text.split_once('/') {
            Some((head, bass)) => (head, Some(bass)),
            None => (text, None),
        };
        let (root_part, quality_part) = split_root_and_suffix(head)?;
        let root = parse_pitch_class(root_part)
            .map_err(|err| MusicError::Chord(format!("{text:?}: {err}")))?;
        let quality = ChordQuality::from_suffix(quality_part).ok_or_else(|| {
            MusicError::Chord(format!(
                "unknown chord quality {quality_part:?} in {text:?}"
            ))
        })?;
        let bass = match bass_part {
            Some(part) => Some(
                parse_pitch_class(part)
                    .map_err(|err| MusicError::Chord(format!("{text:?}: {err}")))?,
            ),
            None => None,
        };
        Ok(Self {
            root,
            quality,
            bass,
        })
    }

    /// Returns the chord tones in the octave whose root sits at `octave`.
    ///
    /// Tones above MIDI 127 are dropped, so the result is safe to schedule.
    #[must_use]
    pub fn tones(&self, octave: i8) -> Vec<crate::Pitch> {
        let base = (i32::from(octave) + 1) * 12 + i32::from(self.root.as_u8());
        self.quality
            .intervals()
            .iter()
            .filter_map(|offset| {
                let midi = base + i32::from(*offset);
                u8::try_from(midi)
                    .ok()
                    .and_then(|midi| crate::Pitch::new(midi).ok())
            })
            .collect()
    }

    /// Returns the slash bass pitch below `octave`, or the root when there is
    /// no slash bass.
    #[must_use]
    pub fn bass_tone(&self, octave: i8) -> Option<crate::Pitch> {
        let pc = self.bass.unwrap_or(self.root);
        let base = (i32::from(octave) + 1) * 12 + i32::from(pc.as_u8());
        u8::try_from(base)
            .ok()
            .and_then(|midi| crate::Pitch::new(midi).ok())
    }

    /// Renders the symbol with the requested spelling.
    #[must_use]
    pub fn format(&self, spelling: Spelling) -> String {
        let name = |pc: PitchClass| match spelling {
            Spelling::Sharp => pc.name_sharp(),
            Spelling::Flat => pc.name_flat(),
        };
        let mut out = String::new();
        out.push_str(name(self.root));
        out.push_str(self.quality.suffix());
        if let Some(bass) = self.bass {
            out.push('/');
            out.push_str(name(bass));
        }
        out
    }
}

impl std::fmt::Display for Chord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format(Spelling::Sharp))
    }
}

/// Splits `Am7` into `("A", "m7")` by finding the longest quality suffix.
fn split_root_and_suffix(head: &str) -> Result<(&str, &str), MusicError> {
    let mut best: Option<(usize, &'static str)> = None;
    for def in Qualities::TABLE {
        for candidate in std::iter::once(def.suffix).chain(def.aliases.iter().copied()) {
            if head.len() >= candidate.len() && head.ends_with(candidate) {
                let quality_len = candidate.len();
                if best.is_none_or(|(len, _)| quality_len > len) {
                    best = Some((quality_len, candidate));
                }
            }
        }
    }
    match best {
        Some((len, _)) => {
            let (root, suffix) = head.split_at(head.len() - len);
            if root.is_empty() {
                return Err(MusicError::Chord(format!(
                    "{head:?} has a quality but no root"
                )));
            }
            Ok((root, suffix))
        }
        None => Err(MusicError::Chord(format!(
            "unknown chord quality in {head:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_triads_and_sevenths() {
        let cases = [
            ("C", ChordQuality::Major, None),
            ("Am", ChordQuality::Minor, None),
            ("G7", ChordQuality::Dominant7, None),
            ("Cmaj7", ChordQuality::Major7, None),
            ("Dm7", ChordQuality::Minor7, None),
            ("Bdim", ChordQuality::Diminished, None),
            ("Bdim7", ChordQuality::Diminished7, None),
            ("Bm7b5", ChordQuality::HalfDiminished7, None),
            ("Caug", ChordQuality::Augmented, None),
            ("Csus4", ChordQuality::Suspended4, None),
            ("Csus2", ChordQuality::Suspended2, None),
            ("C6", ChordQuality::Major6, None),
            ("Cm6", ChordQuality::Minor6, None),
            ("C7sus4", ChordQuality::Dominant7Sus4, None),
            ("CmM7", ChordQuality::MinorMajor7, None),
        ];
        for (symbol, quality, bass) in cases {
            let chord = Chord::parse(symbol).expect("symbol should parse");
            assert_eq!(chord.quality, quality, "{symbol}");
            assert_eq!(chord.bass, bass, "{symbol}");
        }
    }

    #[test]
    fn parses_roots_accidentals_and_slash_bass() {
        let chord = Chord::parse("F#m7/C#").expect("valid");
        assert_eq!(chord.root.as_u8(), 6);
        assert_eq!(chord.quality, ChordQuality::Minor7);
        assert_eq!(chord.bass.map(PitchClass::as_u8), Some(1));

        let flat = Chord::parse("Bb7").expect("valid");
        assert_eq!(flat.root.as_u8(), 10);

        let unicode = Chord::parse("B\u{266d}maj7").expect("valid");
        assert_eq!(unicode.root.as_u8(), 10);
    }

    #[test]
    fn rejects_malformed_symbols() {
        for symbol in ["", "H", "Cfoo", "m7", "C/", "/G", "C/xyz", "C#/H", "Cm7b5/"] {
            assert!(Chord::parse(symbol).is_err(), "{symbol:?} should fail");
        }
        assert_eq!(
            Chord::parse("Cmaj")
                .expect("maj is an accepted alias")
                .quality,
            ChordQuality::Major
        );
    }

    #[test]
    fn renders_with_either_spelling() {
        let chord = Chord::parse("Bb7").expect("valid");
        assert_eq!(chord.format(Spelling::Sharp), "A#7");
        assert_eq!(chord.format(Spelling::Flat), "Bb7");

        let slash = Chord::parse("C/G").expect("valid");
        assert_eq!(slash.format(Spelling::Sharp), "C/G");
        assert_eq!(slash.to_string(), "C/G");
    }

    #[test]
    fn quality_intervals_are_canonical() {
        assert_eq!(ChordQuality::Major.intervals(), &[0, 4, 7]);
        assert_eq!(ChordQuality::Minor.intervals(), &[0, 3, 7]);
        assert_eq!(ChordQuality::Dominant7.intervals(), &[0, 4, 7, 10]);
        assert_eq!(ChordQuality::HalfDiminished7.intervals(), &[0, 3, 6, 10]);
        assert_eq!(ChordQuality::Dominant7Sus4.intervals(), &[0, 5, 7, 10]);
        for def in Qualities::TABLE {
            assert_eq!(def.intervals[0], 0, "{:?}", def.quality);
            assert_eq!(
                ChordQuality::from_suffix(def.suffix),
                Some(def.quality),
                "{:?}",
                def.quality
            );
        }
    }

    #[test]
    fn tones_and_bass_follow_the_symbol() {
        let c = Chord::parse("C").expect("valid");
        let tones: Vec<u8> = c.tones(4).iter().map(|p| p.as_u8()).collect();
        assert_eq!(tones, vec![60, 64, 67]);

        let am7 = Chord::parse("Am7").expect("valid");
        let tones: Vec<u8> = am7.tones(3).iter().map(|p| p.as_u8()).collect();
        assert_eq!(tones, vec![57, 60, 64, 67]);

        let slash = Chord::parse("C/G").expect("valid");
        assert_eq!(slash.bass_tone(3).map(|p| p.as_u8()), Some(55));
        assert_eq!(c.bass_tone(3).map(|p| p.as_u8()), Some(48));
    }

    #[test]
    fn tones_drop_out_of_range_pitches() {
        let chord = Chord::parse("B").expect("valid");
        let in_range: Vec<u8> = chord.tones(8).iter().map(|p| p.as_u8()).collect();
        assert_eq!(in_range, vec![119, 123, 126]);
        assert!(
            chord.tones(9).is_empty(),
            "B above MIDI 127 has no playable tone"
        );
    }

    #[test]
    fn chord_serde_round_trips() {
        let chord = Chord::parse("F#m7/C#").expect("valid");
        let json = serde_json::to_string(&chord).expect("serialize");
        assert_eq!(json, r#"{"root":6,"quality":"minor-7","bass":1}"#);
        let back: Chord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, chord);
    }

    #[test]
    fn chord_serde_rejects_unknown_quality() {
        assert!(serde_json::from_str::<Chord>(r#"{"root":0,"quality":"lydian"}"#).is_err());
    }
}
