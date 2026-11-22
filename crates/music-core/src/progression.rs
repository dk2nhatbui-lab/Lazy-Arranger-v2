//! Chord progressions written as text, as the chord lane accepts them.
//!
//! The grammar is the one the C++ chord lane documents: chord names separated
//! by spaces or `|` barlines, `%` repeats the previous bar, `/` holds the
//! previous chord for a bar. Without barlines every chord owns a bar; between
//! two barlines the chords divide the bar evenly. Roman numerals resolve in the
//! song key.

use crate::MusicError;
use crate::chord::Chord;
use crate::pitch::PitchClass;
use crate::time::{Tick, TimeSignature};

/// One chord placed on the timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedChord {
    /// Tick where the chord starts.
    pub tick: Tick,
    /// The chord.
    pub chord: Chord,
}

/// Parses a progression of chord symbols into placed chords.
///
/// # Errors
///
/// Returns [`MusicError::Chord`] for an unreadable chord symbol.
pub fn parse(
    text: &str,
    start: Tick,
    meter: TimeSignature,
) -> Result<Vec<PlacedChord>, MusicError> {
    parse_with(text, start, meter, Chord::parse)
}

/// Parses a progression that may use roman numerals in `key`.
///
/// # Errors
///
/// Returns [`MusicError::Chord`] for an unreadable token.
pub fn parse_in_key(
    text: &str,
    start: Tick,
    meter: TimeSignature,
    key: PitchClass,
) -> Result<Vec<PlacedChord>, MusicError> {
    parse_with(text, start, meter, |token| {
        if crate::roman::is_roman(token) {
            crate::roman::roman_to_chord(token, key)
        } else {
            Chord::parse(token)
        }
    })
}

fn parse_with(
    text: &str,
    start: Tick,
    meter: TimeSignature,
    resolve: impl Fn(&str) -> Result<Chord, MusicError>,
) -> Result<Vec<PlacedChord>, MusicError> {
    let bar = meter.bar_ticks().max(1);
    let mut out = Vec::new();
    let grouped = text
        .split_whitespace()
        .any(|token| matches!(token, "|" | "%" | "/"));
    if !grouped {
        for token in text.split_whitespace() {
            out.push(PlacedChord {
                tick: start + (out.len() as Tick) * bar,
                chord: resolve(token)?,
            });
        }
        return Ok(out);
    }

    let mut bars: Vec<Vec<Chord>> = Vec::new();
    let mut current: Vec<Chord> = Vec::new();
    let tokens = text.split_whitespace();
    for token in tokens {
        match token {
            "|" => {
                if !current.is_empty() {
                    bars.push(std::mem::take(&mut current));
                }
            }
            "%" => {
                if !current.is_empty() {
                    bars.push(std::mem::take(&mut current));
                }
                let previous = bars.last().cloned().unwrap_or_default();
                bars.push(previous);
            }
            "/" => {
                if !current.is_empty() {
                    bars.push(std::mem::take(&mut current));
                }
                bars.push(Vec::new());
            }
            _ => current.push(resolve(token)?),
        }
    }
    if !current.is_empty() {
        bars.push(current);
    }

    for (bar_index, chords) in bars.iter().enumerate() {
        if chords.is_empty() {
            continue;
        }
        let bar_start = start + (bar_index as Tick) * bar;
        let step = bar / chords.len() as Tick;
        for (index, chord) in chords.iter().enumerate() {
            out.push(PlacedChord {
                tick: bar_start + (index as Tick) * step,
                chord: *chord,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::Spelling;

    fn four_four() -> TimeSignature {
        TimeSignature::default()
    }

    fn symbols(placed: &[PlacedChord]) -> Vec<String> {
        placed
            .iter()
            .map(|entry| entry.chord.format(Spelling::Sharp))
            .collect()
    }

    #[test]
    fn one_chord_per_bar_without_barlines() {
        let placed = parse("Am F C G", 0, four_four()).expect("parse");
        assert_eq!(placed.len(), 4);
        assert_eq!(placed[0].tick, 0);
        assert_eq!(placed[1].tick, 3840);
        assert_eq!(placed[3].tick, 11520);
        assert_eq!(symbols(&placed)[0], "Am");
    }

    #[test]
    fn barlines_and_holds_are_respected() {
        let placed = parse("C | F | / | G", 0, four_four()).expect("parse");
        assert_eq!(placed.len(), 3, "the held bar adds no chord");
        assert_eq!(placed[1].tick, 3840);
        assert_eq!(placed[2].tick, 11520);
    }

    #[test]
    fn percent_repeats_the_previous_bar() {
        let placed = parse("Am F | % | C", 0, four_four()).expect("parse");
        assert_eq!(symbols(&placed), vec!["Am", "F", "Am", "F", "C"]);
        assert_eq!(placed[2].tick, 3840);
        assert_eq!(placed[3].tick, 5760);
        assert_eq!(placed[4].tick, 7680);
    }

    #[test]
    fn several_chords_between_barlines_divide_the_bar() {
        let placed = parse("C Am | F G", 960, four_four()).expect("parse");
        assert_eq!(placed.len(), 4);
        assert_eq!(placed[0].tick, 960);
        assert_eq!(placed[1].tick, 2880);
        assert_eq!(placed[2].tick, 4800);
        assert_eq!(placed[3].tick, 6720);
    }

    #[test]
    fn a_three_four_bar_uses_the_meter() {
        let meter = TimeSignature::new(3, 4).expect("valid");
        let placed = parse("C | G", 0, meter).expect("parse");
        assert_eq!(placed[1].tick, 2880);
    }

    #[test]
    fn roman_numerals_resolve_in_the_key() {
        let placed = parse_in_key("vi IV I V", 0, four_four(), PitchClass::new(0)).expect("parse");
        assert_eq!(symbols(&placed), vec!["Am", "F", "C", "G"]);
        let in_g = parse_in_key("vi IV I V", 0, four_four(), PitchClass::new(7)).expect("parse");
        assert_eq!(symbols(&in_g), vec!["Em", "C", "G", "D"]);
    }

    #[test]
    fn bad_chords_are_errors() {
        assert!(parse("C notachord", 0, four_four()).is_err());
        assert!(parse_in_key("V/V", 0, four_four(), PitchClass::new(0)).is_err());
    }
}
