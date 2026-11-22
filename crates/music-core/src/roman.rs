//! Roman-numeral chords and next-chord suggestions.
//!
//! The chord lane accepts `vi IV I V` in the song key, and the suggestion
//! helper proposes functional neighbours for a chord.

use crate::MusicError;
use crate::chord::{Chord, ChordQuality};
use crate::pitch::PitchClass;

/// Semitones of the major scale.
const MAJOR_SCALE: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Converts one roman-numeral token to a chord in `key`.
///
/// Accepts upper and lower case degrees, a `b`/`#` accidental, `°` or `o` for
/// diminished, and a `7` seventh. Upper case is major, lower case minor.
///
/// # Errors
///
/// Returns [`MusicError::Chord`] when the token is not a roman numeral.
pub fn roman_to_chord(token: &str, key: PitchClass) -> Result<Chord, MusicError> {
    let text = token.trim();
    let (accidental, body) = match text.strip_prefix('b') {
        Some(rest) => (-1, rest),
        None => match text.strip_prefix('#') {
            Some(rest) => (1, rest),
            None => (0, text),
        },
    };
    let uppercase = body.chars().next().is_some_and(char::is_uppercase);
    let upper = body.to_ascii_uppercase();
    let (degree, rest) = [
        ("VII", 7),
        ("III", 3),
        ("VI", 6),
        ("IV", 4),
        ("II", 2),
        ("V", 5),
        ("I", 1),
    ]
    .into_iter()
    .find_map(|(prefix, degree)| {
        upper
            .strip_prefix(prefix)
            .map(|rest| (degree, rest.to_string()))
    })
    .ok_or_else(|| MusicError::Chord(format!("not a roman numeral: {token}")))?;
    let cleaned = rest
        .replace("maj7", "")
        .replace("MAJ7", "")
        .replace('7', "");
    let cleaned = cleaned.replace('°', "").replace(['o', 'O'], "");
    if !cleaned.is_empty() {
        return Err(MusicError::Chord(format!(
            "unsupported roman-numeral suffix: {token}"
        )));
    }
    let diminished = rest.contains('°') || rest.contains(['o', 'O']);
    let seventh = rest.contains('7');
    let major_seventh = rest.contains("maj7") || rest.contains("MAJ7");
    let quality = match (uppercase, diminished, seventh, major_seventh) {
        (_, true, true, _) => ChordQuality::Diminished7,
        (_, true, false, _) => ChordQuality::Diminished,
        (true, false, true, true) => ChordQuality::Major7,
        (true, false, true, false) => ChordQuality::Dominant7,
        (true, false, false, _) => ChordQuality::Major,
        (false, false, true, _) => ChordQuality::Minor7,
        (false, false, false, _) => ChordQuality::Minor,
    };
    let offset = MAJOR_SCALE[degree - 1] + accidental;
    let root = PitchClass::new(i32::from(key.as_u8()) + offset);
    Ok(Chord::new(root, quality))
}

/// True when a token looks like a roman numeral rather than a chord symbol.
#[must_use]
pub fn is_roman(token: &str) -> bool {
    let rest = token
        .strip_prefix('b')
        .or_else(|| token.strip_prefix('#'))
        .unwrap_or(token);
    let first = rest.chars().next();
    matches!(first, Some('i' | 'I' | 'v' | 'V'))
        && roman_to_chord(token, PitchClass::new(0)).is_ok()
}

/// Functional next chords for a chord, most common first.
///
/// Treats the chord as a key centre: a major or dominant chord suggests its
/// IV, V, vi and ii; a minor chord suggests its III, iv, v and VI.
#[must_use]
pub fn suggest_next(chord: Chord) -> Vec<Chord> {
    let minor = matches!(
        chord.quality,
        ChordQuality::Minor
            | ChordQuality::Minor7
            | ChordQuality::MinorMajor7
            | ChordQuality::Diminished
            | ChordQuality::Diminished7
            | ChordQuality::HalfDiminished7
    );
    let steps: &[(i32, ChordQuality)] = if minor {
        &[
            (3, ChordQuality::Major),
            (5, ChordQuality::Minor),
            (7, ChordQuality::Minor),
            (8, ChordQuality::Major),
        ]
    } else {
        &[
            (5, ChordQuality::Major),
            (7, ChordQuality::Major),
            (9, ChordQuality::Minor),
            (2, ChordQuality::Minor),
        ]
    };
    steps
        .iter()
        .map(|(offset, quality)| Chord::new(chord.root.transpose(*offset), *quality))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::Spelling;

    fn c() -> PitchClass {
        PitchClass::new(0)
    }

    fn text(chord: Chord) -> String {
        chord.format(Spelling::Sharp)
    }

    #[test]
    fn diatonic_romans_in_c_major() {
        assert_eq!(text(roman_to_chord("I", c()).expect("I")), "C");
        assert_eq!(text(roman_to_chord("ii", c()).expect("ii")), "Dm");
        assert_eq!(text(roman_to_chord("iii", c()).expect("iii")), "Em");
        assert_eq!(text(roman_to_chord("IV", c()).expect("IV")), "F");
        assert_eq!(text(roman_to_chord("V", c()).expect("V")), "G");
        assert_eq!(text(roman_to_chord("vi", c()).expect("vi")), "Am");
        assert!(text(roman_to_chord("viio", c()).expect("viio")).starts_with('B'));
    }

    #[test]
    fn sevenths_and_accidentals() {
        assert_eq!(
            roman_to_chord("V7", c()).expect("V7").quality,
            ChordQuality::Dominant7
        );
        assert_eq!(
            roman_to_chord("Imaj7", c()).expect("Imaj7").quality,
            ChordQuality::Major7
        );
        assert_eq!(
            roman_to_chord("ii7", c()).expect("ii7").quality,
            ChordQuality::Minor7
        );
        assert_eq!(text(roman_to_chord("bVII", c()).expect("bVII")), "A#");
        assert!(
            roman_to_chord("V/V", c()).is_err(),
            "secondary dominants are not parsed yet"
        );
    }

    #[test]
    fn the_key_transposes_the_degrees() {
        let g = PitchClass::new(7);
        assert_eq!(text(roman_to_chord("I", g).expect("I")), "G");
        assert_eq!(text(roman_to_chord("vi", g).expect("vi")), "Em");
        assert_eq!(text(roman_to_chord("IV", g).expect("IV")), "C");
    }

    #[test]
    fn chord_symbols_are_not_romans() {
        assert!(!is_roman("Am"));
        assert!(!is_roman("F"));
        assert!(is_roman("ii"));
        assert!(is_roman("bVII"));
        assert!(roman_to_chord("H", c()).is_err());
    }

    #[test]
    fn suggestions_follow_function() {
        let suggestions = suggest_next(Chord::new(c(), ChordQuality::Major));
        let names: Vec<String> = suggestions.iter().map(|chord| text(*chord)).collect();
        assert_eq!(names, vec!["F", "G", "Am", "Dm"]);
        let minor = suggest_next(Chord::new(c(), ChordQuality::Minor));
        let names: Vec<String> = minor.iter().map(|chord| text(*chord)).collect();
        assert_eq!(names, vec!["D#", "Fm", "Gm", "G#"]);
    }
}
