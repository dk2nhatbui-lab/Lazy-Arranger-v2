//! Chord voicings: inversions of a chord in close position.
//!
//! A voicing rotates the chord tones and lifts the wrapped tones an octave, so
//! every inversion is ascending and playable. Open spread is not implemented.

use crate::{Chord, Pitch};

/// Which chord tone is in the bass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inversion {
    /// The root.
    Root,
    /// The third or the lowest upper tone.
    First,
    /// The fifth.
    Second,
    /// The seventh, for seventh chords.
    Third,
}

impl Inversion {
    /// The number shown to users.
    #[must_use]
    pub fn number(self) -> usize {
        match self {
            Self::Root => 0,
            Self::First => 1,
            Self::Second => 2,
            Self::Third => 3,
        }
    }

    /// The label shown to users.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Root => "root position",
            Self::First => "1st inversion",
            Self::Second => "2nd inversion",
            Self::Third => "3rd inversion",
        }
    }
}

/// One ascending voicing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Voicing {
    /// The chord.
    pub chord: Chord,
    /// Bass tone.
    pub inversion: Inversion,
    /// Ascending pitches.
    pub pitches: Vec<Pitch>,
}

impl Voicing {
    /// A label such as `C/E (1st inversion)`.
    #[must_use]
    pub fn label(&self) -> String {
        let chord = self.chord.format(crate::Spelling::Sharp);
        match self.inversion {
            Inversion::Root => format!("{chord} ({})", self.inversion.label()),
            _ => {
                let bass = self.pitches.first().map_or(String::from("?"), |pitch| {
                    pitch.pitch_class().name_sharp().to_string()
                });
                format!("{chord}/{bass} ({})", self.inversion.label())
            }
        }
    }

    /// The pitches as MIDI numbers.
    #[must_use]
    pub fn midi(&self) -> Vec<u8> {
        self.pitches.iter().map(|pitch| pitch.as_u8()).collect()
    }
}

/// Every close-position inversion of a chord at the given octave.
#[must_use]
pub fn inversions(chord: Chord, octave: i8) -> Vec<Voicing> {
    let base: Vec<Pitch> = chord.tones(octave);
    let count = base.len();
    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        let mut pitches = Vec::with_capacity(count);
        for step in 0..count {
            let source = base[(index + step) % count];
            let lift = if index + step >= count { 12 } else { 0 };
            let midi = i32::from(source.as_u8()) + lift;
            if let Ok(value) = u8::try_from(midi)
                && let Ok(pitch) = Pitch::new(value)
            {
                pitches.push(pitch);
            }
        }
        let inversion = match index {
            0 => Inversion::Root,
            1 => Inversion::First,
            2 => Inversion::Second,
            _ => Inversion::Third,
        };
        out.push(Voicing {
            chord,
            inversion,
            pitches,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(name: &str) -> Chord {
        Chord::parse(name).expect("valid")
    }

    #[test]
    fn triads_have_three_ascending_inversions() {
        let voicings = inversions(chord("C"), 4);
        assert_eq!(voicings.len(), 3);
        assert_eq!(voicings[0].midi(), vec![60, 64, 67]);
        assert_eq!(voicings[1].midi(), vec![64, 67, 72]);
        assert_eq!(voicings[2].midi(), vec![67, 72, 76]);
        assert_eq!(voicings[1].label(), "C/E (1st inversion)");
        for voicing in &voicings {
            assert!(
                voicing
                    .pitches
                    .windows(2)
                    .all(|pair| pair[0].as_u8() < pair[1].as_u8())
            );
        }
    }

    #[test]
    fn sevenths_have_four_inversions() {
        let voicings = inversions(chord("G7"), 3);
        assert_eq!(voicings.len(), 4);
        assert_eq!(voicings[0].midi(), vec![55, 59, 62, 65]);
        assert_eq!(voicings[3].midi(), vec![65, 67, 71, 74]);
        assert_eq!(voicings[3].inversion.label(), "3rd inversion");
    }

    #[test]
    fn minor_and_slash_chords_voice_normally() {
        let a_minor = inversions(chord("Am"), 4);
        assert_eq!(a_minor[0].midi(), vec![69, 72, 76]);
        assert_eq!(a_minor[1].midi(), vec![72, 76, 81]);
        assert_eq!(a_minor[1].label(), "Am/C (1st inversion)");
    }
}
