//! Reharmonization proposals for one chord.
//!
//! Each proposal is a short chord sequence to place over the bar the original
//! chord occupies: one chord, or two for a ii-V. These are the rule-based
//! options the chord lane's "Reharmonize" action offers.

use crate::chord::{Chord, ChordQuality};

/// A kind of reharmonization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Dominant a tritone away, for a dominant chord.
    TritoneSubstitute,
    /// Borrow the parallel minor's iv, or the parallel major's IV.
    Borrowed,
    /// Replace with the ii-V that leads to this chord.
    TwoFive,
    /// Swap major and relative minor.
    Relative,
}

impl Kind {
    /// Short label for a button.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::TritoneSubstitute => "Tritone",
            Self::Borrowed => "Borrowed",
            Self::TwoFive => "ii-V",
            Self::Relative => "Relative",
        }
    }
}

/// The chords a reharmonization puts in place of `chord`.
#[must_use]
pub fn apply(chord: Chord, kind: Kind) -> Vec<Chord> {
    let root = chord.root;
    match kind {
        Kind::TritoneSubstitute => vec![Chord::new(root.transpose(6), ChordQuality::Dominant7)],
        Kind::Borrowed => {
            if is_minor(chord.quality) {
                vec![Chord::new(root.transpose(5), ChordQuality::Major)]
            } else {
                vec![Chord::new(root.transpose(5), ChordQuality::Minor)]
            }
        }
        Kind::TwoFive => vec![
            Chord::new(
                root.transpose(2),
                if is_minor(chord.quality) {
                    ChordQuality::HalfDiminished7
                } else {
                    ChordQuality::Minor7
                },
            ),
            Chord::new(root.transpose(7), ChordQuality::Dominant7),
        ],
        Kind::Relative => {
            if is_minor(chord.quality) {
                vec![Chord::new(root.transpose(3), ChordQuality::Major)]
            } else {
                vec![Chord::new(root.transpose(9), ChordQuality::Minor)]
            }
        }
    }
}

/// Every proposal for a chord, in button order.
#[must_use]
pub fn candidates(chord: Chord) -> Vec<(Kind, Vec<Chord>)> {
    [
        Kind::TritoneSubstitute,
        Kind::Borrowed,
        Kind::TwoFive,
        Kind::Relative,
    ]
    .into_iter()
    .map(|kind| (kind, apply(chord, kind)))
    .collect()
}

fn is_minor(quality: ChordQuality) -> bool {
    matches!(
        quality,
        ChordQuality::Minor
            | ChordQuality::Minor7
            | ChordQuality::MinorMajor7
            | ChordQuality::Diminished
            | ChordQuality::Diminished7
            | ChordQuality::HalfDiminished7
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::Spelling;

    fn text(chords: &[Chord]) -> Vec<String> {
        chords
            .iter()
            .map(|chord| chord.format(Spelling::Sharp))
            .collect()
    }

    fn chord(symbol: &str) -> Chord {
        Chord::parse(symbol).expect("valid chord")
    }

    #[test]
    fn a_tritone_substitute_sits_six_semitones_away() {
        assert_eq!(
            text(&apply(chord("G7"), Kind::TritoneSubstitute)),
            vec!["C#7"]
        );
        assert_eq!(
            text(&apply(chord("C"), Kind::TritoneSubstitute)),
            vec!["F#7"]
        );
    }

    #[test]
    fn borrowed_chords_flip_the_third() {
        assert_eq!(text(&apply(chord("C"), Kind::Borrowed)), vec!["Fm"]);
        assert_eq!(text(&apply(chord("Cm"), Kind::Borrowed)), vec!["F"]);
    }

    #[test]
    fn two_five_leads_to_the_chord() {
        assert_eq!(text(&apply(chord("C"), Kind::TwoFive)), vec!["Dm7", "G7"]);
        assert_eq!(
            text(&apply(chord("Am"), Kind::TwoFive)),
            vec!["Bm7b5", "E7"]
        );
    }

    #[test]
    fn relative_swaps_mode() {
        assert_eq!(text(&apply(chord("C"), Kind::Relative)), vec!["Am"]);
        assert_eq!(
            text(&apply(chord("Cm"), Kind::Relative)),
            vec!["D#".to_string()]
        );
    }

    #[test]
    fn candidates_are_the_four_rules_in_order() {
        let kinds: Vec<Kind> = candidates(chord("C"))
            .into_iter()
            .map(|(kind, _)| kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                Kind::TritoneSubstitute,
                Kind::Borrowed,
                Kind::TwoFive,
                Kind::Relative
            ]
        );
        assert_eq!(Kind::TwoFive.label(), "ii-V");
    }
}
