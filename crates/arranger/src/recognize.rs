//! Chord recognition from played notes ("Find Chords from the Music").
//!
//! Compares the pitch classes of a bar with the 35 chord types the arranger
//! already knows. An exact chord-tone match wins; otherwise the best overlap
//! above a threshold is offered. The lowest note becomes a slash bass when it
//! is not the root.

use music_core::{Chord, ChordQuality, PitchClass};

use crate::chord_type::{ALL_TYPES, ChordType};

/// A recognized chord and how well it fits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recognized {
    /// The chord, with a slash bass when the lowest note is not the root.
    pub chord: Chord,
    /// Share of the combined pitch-class set that the chord explains, 0..=1.
    pub score: f64,
}

/// The pitch classes of a chord type on a root, low to high.
#[must_use]
pub fn chord_pitch_classes(kind: ChordType, root: PitchClass) -> Vec<u8> {
    let info = kind.info();
    let mut classes = vec![root.as_u8()];
    if info.third > 0 {
        classes.push(root.transpose(info.third).as_u8());
    }
    if info.fifth > 0 {
        classes.push(root.transpose(info.fifth).as_u8());
    }
    if info.seventh >= 0 {
        classes.push(root.transpose(info.seventh).as_u8());
    }
    classes.sort_unstable();
    classes.dedup();
    classes
}

/// The quality for a type, when one exists.
fn quality(kind: ChordType) -> Option<ChordQuality> {
    let info = kind.info();
    Some(match (info.third, info.fifth, info.seventh) {
        (0, 7, -1) => ChordQuality::Major,
        (4, 7, -1) => ChordQuality::Major,
        (3, 7, -1) => ChordQuality::Minor,
        (4, 7, 10) => ChordQuality::Dominant7,
        (4, 7, 11) => ChordQuality::Major7,
        (3, 7, 10) => ChordQuality::Minor7,
        (3, 7, 11) => ChordQuality::MinorMajor7,
        (3, 6, -1) => ChordQuality::Diminished,
        (3, 6, 9) => ChordQuality::Diminished7,
        (3, 6, 10) => ChordQuality::HalfDiminished7,
        (4, 8, -1) => ChordQuality::Augmented,
        _ => return None,
    })
}

/// Recognizes the best chord for a set of pitches.
///
/// `bass` is the lowest pitch class, when the caller knows it.
#[must_use]
pub fn recognize(pitches: &[u8], bass: Option<u8>) -> Option<Recognized> {
    let mut classes: Vec<u8> = pitches.iter().map(|pitch| pitch % 12).collect();
    classes.sort_unstable();
    classes.dedup();
    if classes.len() < 2 {
        return None;
    }
    let mut best: Option<Recognized> = None;
    for kind in ALL_TYPES {
        let Some(quality) = quality(kind) else {
            continue;
        };
        for root in 0..12_u8 {
            let chord_classes = chord_pitch_classes(kind, PitchClass::new(i32::from(root)));
            let matched = chord_classes
                .iter()
                .filter(|class| classes.contains(class))
                .count();
            if matched != chord_classes.len() {
                continue;
            }
            let union = classes.len();
            let score = chord_classes.len() as f64 / union as f64;
            let root_class = PitchClass::new(i32::from(root));
            let mut chord = Chord::new(root_class, quality);
            if let Some(lowest) = bass
                && lowest % 12 != root
                && chord_classes.contains(&(lowest % 12))
            {
                chord.bass = Some(PitchClass::new(i32::from(lowest % 12)));
            }
            let candidate = Recognized { chord, score };
            if best.is_none_or(|current| {
                candidate.score > current.score + 1e-9
                    || ((candidate.score - current.score).abs() < 1e-9
                        && candidate.chord.quality != ChordQuality::Major
                        && current.chord.quality == ChordQuality::Major)
            }) {
                best = Some(candidate);
            }
        }
    }
    best
}

/// Convenience: the chord symbol for a set of pitches, when recognized.
#[must_use]
pub fn symbol(pitches: &[u8], bass: Option<u8>) -> Option<String> {
    recognize(pitches, bass).map(|found| found.chord.format(music_core::Spelling::Sharp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_major_triad_is_recognized() {
        let found = recognize(&[60, 64, 67], Some(60)).expect("chord");
        assert_eq!(found.chord.format(music_core::Spelling::Sharp), "C");
        assert!((found.score - 1.0).abs() < 1e-9);
    }

    #[test]
    fn sevenths_win_over_triads() {
        assert_eq!(
            symbol(&[60, 64, 67, 71], Some(60)).as_deref(),
            Some("Cmaj7")
        );
        assert_eq!(symbol(&[62, 65, 69, 72], Some(62)).as_deref(), Some("Dm7"));
        assert_eq!(symbol(&[55, 59, 62, 65], Some(55)).as_deref(), Some("G7"));
    }

    #[test]
    fn inversions_become_slash_chords() {
        let found = recognize(&[64, 67, 72], Some(64)).expect("chord");
        assert_eq!(found.chord.format(music_core::Spelling::Sharp), "C/E");
    }

    #[test]
    fn minor_and_diminished_are_recognized() {
        assert_eq!(symbol(&[57, 60, 64], Some(57)).as_deref(), Some("Am"));
        assert_eq!(symbol(&[59, 62, 65], Some(59)).as_deref(), Some("Bdim"));
    }

    #[test]
    fn fewer_than_two_notes_and_foreign_sets_give_nothing() {
        assert_eq!(recognize(&[60], Some(60)).map(|found| found.chord), None);
        assert_eq!(
            recognize(&[60, 61], Some(60)).map(|found| found.chord),
            None
        );
    }
}
