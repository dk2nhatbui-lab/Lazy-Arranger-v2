//! Small theory helpers: scales, roman labels, cadences and voice leading.

use crate::{Chord, ChordQuality, MusicError, PitchClass};

/// A scale type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Ionian.
    Major,
    /// Aeolian.
    NaturalMinor,
    /// Dorian.
    Dorian,
    /// Phrygian.
    Phrygian,
    /// Lydian.
    Lydian,
    /// Mixolydian.
    Mixolydian,
    /// Locrian.
    Locrian,
    /// Harmonic minor.
    HarmonicMinor,
    /// Melodic minor ascending.
    MelodicMinor,
}

impl Mode {
    /// Semitones above the tonic.
    #[must_use]
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Self::Major => &[0, 2, 4, 5, 7, 9, 11],
            Self::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
            Self::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Self::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Self::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Self::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Self::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Self::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Self::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
        }
    }

    /// Parses a mode name.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::Note`] for an unknown name.
    pub fn parse(name: &str) -> Result<Self, MusicError> {
        Ok(match name.trim().to_ascii_lowercase().as_str() {
            "major" | "ionian" => Self::Major,
            "minor" | "aeolian" | "natural_minor" | "natural minor" => Self::NaturalMinor,
            "dorian" => Self::Dorian,
            "phrygian" => Self::Phrygian,
            "lydian" => Self::Lydian,
            "mixolydian" => Self::Mixolydian,
            "locrian" => Self::Locrian,
            "harmonic_minor" | "harmonic minor" => Self::HarmonicMinor,
            "melodic_minor" | "melodic minor" => Self::MelodicMinor,
            other => {
                return Err(MusicError::Pitch(format!("unknown mode {other:?}")));
            }
        })
    }

    /// The mode's name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Major => "major",
            Self::NaturalMinor => "natural minor",
            Self::Dorian => "dorian",
            Self::Phrygian => "phrygian",
            Self::Lydian => "lydian",
            Self::Mixolydian => "mixolydian",
            Self::Locrian => "locrian",
            Self::HarmonicMinor => "harmonic minor",
            Self::MelodicMinor => "melodic minor",
        }
    }
}

/// The seven pitch classes of a scale.
#[must_use]
pub fn scale(tonic: PitchClass, mode: Mode) -> Vec<PitchClass> {
    mode.intervals()
        .iter()
        .map(|step| tonic.transpose(i32::from(*step)))
        .collect()
}

/// The ending of a progression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cadence {
    /// Dominant to tonic.
    Authentic,
    /// Subdominant to tonic.
    Plagal,
    /// Anything to the dominant.
    Half,
    /// Dominant to the submediant.
    Deceptive,
    /// No cadence in the last pair.
    None,
}

/// Classifies the last two chords against a tonic.
#[must_use]
pub fn cadence(chords: &[Chord], tonic: PitchClass) -> Cadence {
    let Some(last) = chords.last() else {
        return Cadence::None;
    };
    let Some(previous) = chords.iter().rev().nth(1) else {
        return Cadence::None;
    };
    let dominant = tonic.transpose(7);
    let subdominant = tonic.transpose(5);
    if last.root == tonic {
        if previous.root == dominant {
            Cadence::Authentic
        } else if previous.root == subdominant {
            Cadence::Plagal
        } else {
            Cadence::None
        }
    } else if previous.root == dominant {
        if last.root == tonic.transpose(9) {
            Cadence::Deceptive
        } else if last.root == dominant {
            Cadence::None
        } else {
            Cadence::Half
        }
    } else if last.root == dominant {
        Cadence::Half
    } else {
        Cadence::None
    }
}

/// A roman numeral label for a chord against a major tonic.
#[must_use]
pub fn roman_label(chord: Chord, tonic: PitchClass) -> String {
    let interval = (i32::from(chord.root.as_u8()) - i32::from(tonic.as_u8())).rem_euclid(12);
    let base = match interval {
        0 => ("I", ""),
        2 => ("II", ""),
        4 => ("III", ""),
        5 => ("IV", ""),
        7 => ("V", ""),
        9 => ("VI", ""),
        11 => ("VII", ""),
        other => {
            let nearest = match other {
                1 | 3 | 6 | 8 | 10 => other,
                _ => 0,
            };
            let (letter, prefix) = match nearest {
                1 => ("I", "b"),
                3 => ("III", "b"),
                6 => ("V", "#"),
                8 => ("VI", "b"),
                10 => ("VII", "b"),
                _ => ("I", ""),
            };
            (letter, prefix)
        }
    };
    let quality = match chord.quality {
        ChordQuality::Minor | ChordQuality::Minor7 | ChordQuality::MinorMajor7 => "m",
        ChordQuality::Diminished | ChordQuality::Diminished7 => "dim",
        ChordQuality::HalfDiminished7 => "m7b5",
        ChordQuality::Augmented => "aug",
        ChordQuality::Dominant7 => "7",
        ChordQuality::Major7 => "maj7",
        ChordQuality::Suspended2 | ChordQuality::Suspended4 => "sus",
        _ => "",
    };
    format!("{}{}{}", base.1, base.0, quality)
}

/// Voice-leading cost between two chords: the smallest total semitone motion
/// across inversions of the second chord.
#[must_use]
pub fn voice_leading_cost(from: Chord, to: Chord) -> u32 {
    let source: Vec<i32> = from
        .tones(4)
        .into_iter()
        .map(|pitch| i32::from(pitch.as_u8()))
        .collect();
    let target: Vec<i32> = to
        .tones(4)
        .into_iter()
        .map(|pitch| i32::from(pitch.as_u8()))
        .collect();
    if source.is_empty() || target.is_empty() {
        return 0;
    }
    let mut best = u32::MAX;
    for rotation in 0..target.len() {
        let mut motion = 0_u32;
        for (index, pitch) in source.iter().enumerate() {
            let other = target[(index + rotation) % target.len()];
            let mut distance = i32::MAX;
            for octave in -2_i32..=2 {
                distance = distance.min((other + 12 * octave - pitch).abs());
            }
            motion += u32::try_from(distance).unwrap_or(0);
        }
        best = best.min(motion);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(symbol: &str) -> Chord {
        Chord::parse(symbol).expect("valid chord")
    }

    #[test]
    fn scales_have_seven_degrees() {
        let c = PitchClass::new(0);
        let major = scale(c, Mode::Major);
        let names: Vec<u8> = major.iter().map(|pitch| pitch.as_u8()).collect();
        assert_eq!(names, vec![0, 2, 4, 5, 7, 9, 11]);
        let minor = scale(PitchClass::new(9), Mode::NaturalMinor);
        let names: Vec<u8> = minor.iter().map(|pitch| pitch.as_u8()).collect();
        assert_eq!(names, vec![9, 11, 0, 2, 4, 5, 7]);
        assert_eq!(
            Mode::parse("harmonic minor").expect("mode").intervals()[6],
            11
        );
        assert!(Mode::parse("blues").is_err());
    }

    #[test]
    fn cadences_are_classified() {
        let c = PitchClass::new(0);
        assert_eq!(cadence(&[chord("G"), chord("C")], c), Cadence::Authentic);
        assert_eq!(cadence(&[chord("F"), chord("C")], c), Cadence::Plagal);
        assert_eq!(cadence(&[chord("C"), chord("G")], c), Cadence::Half);
        assert_eq!(cadence(&[chord("G"), chord("Am")], c), Cadence::Deceptive);
        assert_eq!(cadence(&[chord("Dm"), chord("Em")], c), Cadence::None);
        assert_eq!(cadence(&[], c), Cadence::None);
    }

    #[test]
    fn roman_labels_follow_the_tonic() {
        let c = PitchClass::new(0);
        assert_eq!(roman_label(chord("C"), c), "I");
        assert_eq!(roman_label(chord("Dm"), c), "IIm");
        assert_eq!(roman_label(chord("G7"), c), "V7");
        assert_eq!(roman_label(chord("Bb"), c), "bVII");
    }

    #[test]
    fn a_close_key_costs_less_than_a_distant_one() {
        let c = chord("C");
        let f = chord("F");
        let f_sharp = chord("F#");
        assert!(voice_leading_cost(c, f) < voice_leading_cost(c, f_sharp));
    }
}
