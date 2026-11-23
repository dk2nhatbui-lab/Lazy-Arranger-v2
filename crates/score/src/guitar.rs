//! Standard open and barre chord frames for the score.
//!
//! The table is deliberately small and names only common shapes. A slash chord
//! or a quality outside the table gets no frame.

use music_core::{Chord, ChordQuality};

/// A six-string chord shape, low E first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuitarShape {
    /// Written chord name.
    pub name: String,
    /// Fret per string; `None` is muted and `0` is open.
    pub frets: [Option<u8>; 6],
    /// Fret the frame starts at; 1 is the nut.
    pub base_fret: u8,
}

fn parse(root: &str, text: &str, base_fret: u8) -> Option<GuitarShape> {
    let mut frets = [None; 6];
    let mut chars = text.chars();
    for slot in &mut frets {
        let character = chars.next()?;
        *slot = match character {
            'x' | 'X' => None,
            '0' => Some(0),
            digit => Some(digit.to_digit(10)? as u8),
        };
    }
    if chars.next().is_some() {
        return None;
    }
    Some(GuitarShape {
        name: root.to_string(),
        frets,
        base_fret,
    })
}

/// The frame for a chord, when the table has one.
#[must_use]
pub fn shape(chord: &Chord) -> Option<GuitarShape> {
    if chord.bass.is_some() {
        return None;
    }
    let root = chord.root.as_u8();
    let (name, quality) = match (root, chord.quality) {
        (0, ChordQuality::Major) => ("C", ChordQuality::Major),
        (2, ChordQuality::Major) => ("D", ChordQuality::Major),
        (4, ChordQuality::Major) => ("E", ChordQuality::Major),
        (5, ChordQuality::Major) => ("F", ChordQuality::Major),
        (7, ChordQuality::Major) => ("G", ChordQuality::Major),
        (9, ChordQuality::Major) => ("A", ChordQuality::Major),
        (9, ChordQuality::Minor) => ("Am", ChordQuality::Minor),
        (4, ChordQuality::Minor) => ("Em", ChordQuality::Minor),
        (2, ChordQuality::Minor) => ("Dm", ChordQuality::Minor),
        (11, ChordQuality::Minor) => ("Bm", ChordQuality::Minor),
        (9, ChordQuality::Dominant7) => ("A7", ChordQuality::Dominant7),
        (11, ChordQuality::Dominant7) => ("B7", ChordQuality::Dominant7),
        (0, ChordQuality::Dominant7) => ("C7", ChordQuality::Dominant7),
        (2, ChordQuality::Dominant7) => ("D7", ChordQuality::Dominant7),
        (4, ChordQuality::Dominant7) => ("E7", ChordQuality::Dominant7),
        (7, ChordQuality::Dominant7) => ("G7", ChordQuality::Dominant7),
        (9, ChordQuality::Minor7) => ("Am7", ChordQuality::Minor7),
        (4, ChordQuality::Minor7) => ("Em7", ChordQuality::Minor7),
        (2, ChordQuality::Minor7) => ("Dm7", ChordQuality::Minor7),
        (0, ChordQuality::Major7) => ("Cmaj7", ChordQuality::Major7),
        (5, ChordQuality::Major7) => ("Fmaj7", ChordQuality::Major7),
        (7, ChordQuality::Major7) => ("Gmaj7", ChordQuality::Major7),
        (9, ChordQuality::Major7) => ("Amaj7", ChordQuality::Major7),
        _ => return None,
    };
    let (frets, base) = match (name, quality) {
        ("C", ChordQuality::Major) => ("x32010", 1),
        ("D", ChordQuality::Major) => ("xx0232", 1),
        ("E", ChordQuality::Major) => ("022100", 1),
        ("F", ChordQuality::Major) => ("133211", 1),
        ("G", ChordQuality::Major) => ("320003", 1),
        ("A", ChordQuality::Major) => ("x02220", 1),
        ("Am", ChordQuality::Minor) => ("x02210", 1),
        ("Em", ChordQuality::Minor) => ("022000", 1),
        ("Dm", ChordQuality::Minor) => ("xx0231", 1),
        ("Bm", ChordQuality::Minor) => ("x24432", 2),
        ("A7", ChordQuality::Dominant7) => ("x02020", 1),
        ("B7", ChordQuality::Dominant7) => ("x21202", 1),
        ("C7", ChordQuality::Dominant7) => ("x32310", 1),
        ("D7", ChordQuality::Dominant7) => ("xx0212", 1),
        ("E7", ChordQuality::Dominant7) => ("020100", 1),
        ("G7", ChordQuality::Dominant7) => ("320001", 1),
        ("Am7", ChordQuality::Minor7) => ("x02010", 1),
        ("Em7", ChordQuality::Minor7) => ("020000", 1),
        ("Dm7", ChordQuality::Minor7) => ("xx0211", 1),
        ("Cmaj7", ChordQuality::Major7) => ("x32000", 1),
        ("Fmaj7", ChordQuality::Major7) => ("xx3210", 1),
        ("Gmaj7", ChordQuality::Major7) => ("320002", 1),
        ("Amaj7", ChordQuality::Major7) => ("x02120", 1),
        _ => return None,
    };
    parse(name, frets, base)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(name: &str) -> Chord {
        Chord::parse(name).expect("valid")
    }

    #[test]
    fn common_shapes_are_listed() {
        let c = shape(&chord("C")).expect("C");
        assert_eq!(c.frets, [None, Some(3), Some(2), Some(0), Some(1), Some(0)]);
        let f = shape(&chord("F")).expect("F");
        assert_eq!(f.base_fret, 1);
        assert_eq!(f.frets[0], Some(1));
        assert!(shape(&chord("C#dim")).is_none());
        assert!(
            shape(&chord("C/E")).is_none(),
            "slash chords have no open shape"
        );
    }
}
