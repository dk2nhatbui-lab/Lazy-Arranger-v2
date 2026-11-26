use serde::{Deserialize, Serialize};

use crate::MusicError;

const SHARP_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
const FLAT_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
];

/// A pitch class: an octave-independent chroma, `0` = C, `11` = B.
///
/// The value carries no spelling. Use [`PitchClass::name_sharp`],
/// [`PitchClass::name_flat`] or [`crate::Chord::format`] to render a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct PitchClass(u8);

impl PitchClass {
    /// Creates a pitch class from any semitone number, wrapping into `0..12`.
    #[must_use]
    pub const fn new(semitone: i32) -> Self {
        Self(semitone.rem_euclid(12) as u8)
    }

    /// Returns the raw semitone `0..12`.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// Returns the sharp spelling, for example `C#`.
    #[must_use]
    pub const fn name_sharp(self) -> &'static str {
        SHARP_NAMES[self.0 as usize]
    }

    /// Returns the flat spelling, for example `Db`.
    #[must_use]
    pub const fn name_flat(self) -> &'static str {
        FLAT_NAMES[self.0 as usize]
    }

    /// Transposes by semitones, wrapping at the octave.
    #[must_use]
    pub const fn transpose(self, semitones: i32) -> Self {
        Self::new(self.0 as i32 + semitones)
    }
}

impl TryFrom<u8> for PitchClass {
    type Error = MusicError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value < 12 {
            Ok(Self(value))
        } else {
            Err(MusicError::Pitch(format!(
                "{value} is not a pitch class; expected 0..12"
            )))
        }
    }
}

impl From<PitchClass> for u8 {
    fn from(value: PitchClass) -> Self {
        value.0
    }
}

impl std::fmt::Display for PitchClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name_sharp())
    }
}

/// Parses a bare pitch-class name such as `C`, `F#`, `Bb` or `Bb`.
pub(crate) fn parse_pitch_class(name: &str) -> Result<PitchClass, MusicError> {
    let trimmed = name.trim();
    let mut chars = trimmed.chars();
    let letter = chars
        .next()
        .ok_or_else(|| MusicError::Pitch(String::from("empty pitch name")))?;
    let base: i32 = match letter.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => {
            return Err(MusicError::Pitch(format!(
                "{trimmed:?} does not start with a note letter A-G"
            )));
        }
    };
    let mut semitone = base;
    let mut rest = String::new();
    loop {
        match chars.next() {
            Some('#' | '\u{266f}') => semitone += 1,
            Some('b' | '\u{266d}') => semitone -= 1,
            Some(other) => {
                rest.push(other);
                rest.extend(chars);
                break;
            }
            None => break,
        }
    }
    if !rest.is_empty() {
        return Err(MusicError::Pitch(format!(
            "{trimmed:?} has trailing characters {rest:?}"
        )));
    }
    Ok(PitchClass::new(semitone))
}

/// A MIDI pitch `0..=127`, where `60` is middle C (`C4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Pitch(u8);

impl Pitch {
    /// Creates a pitch from a MIDI note number.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::Pitch`] when `midi` is above 127.
    pub fn new(midi: u8) -> Result<Self, MusicError> {
        if midi <= 127 {
            Ok(Self(midi))
        } else {
            Err(MusicError::Pitch(format!("{midi} is not a MIDI note")))
        }
    }

    /// Parses scientific pitch notation such as `C4`, `F#3` or `Bb2`.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::Pitch`] for a malformed name or a pitch outside
    /// the MIDI range.
    pub fn from_name(name: &str) -> Result<Self, MusicError> {
        let trimmed = name.trim();
        let mut chars = trimmed.chars();
        let letter = chars
            .next()
            .ok_or_else(|| MusicError::Pitch(String::from("empty note name")))?;
        let base: i32 = match letter.to_ascii_uppercase() {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => {
                return Err(MusicError::Pitch(format!(
                    "{trimmed:?} does not start with a note letter A-G"
                )));
            }
        };
        let mut semitone = base;
        let mut rest = String::new();
        loop {
            match chars.next() {
                Some('#' | '\u{266f}') => semitone += 1,
                Some('b' | '\u{266d}') => semitone -= 1,
                Some(other) => {
                    rest.push(other);
                    rest.extend(chars);
                    break;
                }
                None => {
                    return Err(MusicError::Pitch(format!(
                        "{trimmed:?} has no octave number"
                    )));
                }
            }
        }
        let octave: i32 = rest
            .parse()
            .map_err(|_| MusicError::Pitch(format!("{trimmed:?} has no octave number")))?;
        let midi = octave
            .checked_add(1)
            .and_then(|octave| octave.checked_mul(12))
            .and_then(|base| base.checked_add(semitone))
            .unwrap_or(-1);
        if !(0..=127).contains(&midi) {
            return Err(MusicError::Pitch(format!(
                "{trimmed:?} is outside the MIDI range 0..=127"
            )));
        }
        Ok(Self(midi as u8))
    }

    /// Returns the raw MIDI note number.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// Returns the pitch class.
    #[must_use]
    pub const fn pitch_class(self) -> PitchClass {
        PitchClass::new(self.0 as i32)
    }

    /// Returns the octave in scientific pitch notation; `C4` is octave 4.
    #[must_use]
    pub const fn octave(self) -> i8 {
        (self.0 / 12) as i8 - 1
    }

    /// Transposes by semitones, saturating at the MIDI range.
    #[must_use]
    pub const fn transpose(self, semitones: i8) -> Self {
        let raw = self.0 as i32 + semitones as i32;
        Self(if raw < 0 {
            0
        } else if raw > 127 {
            127
        } else {
            raw as u8
        })
    }
}

impl TryFrom<u8> for Pitch {
    type Error = MusicError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Pitch> for u8 {
    fn from(value: Pitch) -> Self {
        value.0
    }
}

impl std::fmt::Display for Pitch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.pitch_class().name_sharp(), self.octave())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_class_wraps_in_both_directions() {
        assert_eq!(PitchClass::new(0).as_u8(), 0);
        assert_eq!(PitchClass::new(12).as_u8(), 0);
        assert_eq!(PitchClass::new(-1).as_u8(), 11);
        assert_eq!(PitchClass::new(13).name_sharp(), "C#");
        assert_eq!(PitchClass::new(10).name_flat(), "Bb");
    }

    #[test]
    fn pitch_class_rejects_out_of_range_serde_values() {
        let err = serde_json::from_str::<PitchClass>("12").expect_err("12 is not a pitch class");
        assert!(err.to_string().contains("not a pitch class"), "{err}");
    }

    #[test]
    fn pitch_parses_names_and_round_trips() {
        for (name, midi) in [
            ("C4", 60),
            ("c4", 60),
            ("A4", 69),
            ("C#4", 61),
            ("Db4", 61),
            ("Cb4", 59),
            ("B#4", 72),
            ("C-1", 0),
            ("G9", 127),
            ("F\u{266f}3", 54),
        ] {
            let pitch = Pitch::from_name(name).expect("name should parse");
            assert_eq!(pitch.as_u8(), midi, "{name}");
            assert_eq!(
                pitch.to_string(),
                Pitch::new(midi).expect("valid").to_string()
            );
        }
    }

    #[test]
    fn pitch_rejects_bad_names() {
        for name in ["", "H4", "C", "Cx4", "128", "C#", "C4x"] {
            assert!(Pitch::from_name(name).is_err(), "{name:?} should fail");
        }
        assert!(
            Pitch::from_name("C4 ").is_ok(),
            "outer whitespace is ignored"
        );
        assert_eq!(
            Pitch::from_name("C##4")
                .expect("double sharp is legal")
                .as_u8(),
            62,
            "C##4 is enharmonic with D4"
        );
        assert!(Pitch::new(128).is_err());
    }

    #[test]
    fn pitch_transpose_saturates() {
        let c4 = Pitch::from_name("C4").expect("valid");
        assert_eq!(c4.transpose(12).as_u8(), 72);
        assert_eq!(c4.transpose(-100).as_u8(), 0);
        assert_eq!(c4.transpose(100).as_u8(), 127);
    }

    #[test]
    fn pitch_octaves_match_scientific_notation() {
        assert_eq!(Pitch::from_name("C4").expect("valid").octave(), 4);
        assert_eq!(Pitch::from_name("B3").expect("valid").octave(), 3);
        assert_eq!(Pitch::from_name("C-1").expect("valid").octave(), -1);
    }
}
