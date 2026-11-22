//! Chord progressions written as text, as the chord lane accepts them.
//!
//! The grammar is the one the C++ chord lane documents: chord names separated
//! by spaces or `|` barlines, `%` repeats the previous bar, `/` holds the
//! previous chord for a bar. Several chords in one bar divide it evenly.

use crate::MusicError;
use crate::chord::Chord;
use crate::time::{Tick, TimeSignature};

/// One chord placed on the timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedChord {
    /// Tick where the chord starts.
    pub tick: Tick,
    /// The chord.
    pub chord: Chord,
}

/// Parses a progression into placed chords.
///
/// `start` is the tick of the first bar and `meter` decides bar length. `%`
/// repeats the chords of the previous bar; `/` holds, adding no chord.
///
/// # Errors
///
/// Returns [`MusicError::Chord`] for an unreadable chord symbol.
pub fn parse(
    text: &str,
    start: Tick,
    meter: TimeSignature,
) -> Result<Vec<PlacedChord>, MusicError> {
    let bar = meter.bar_ticks().max(1);
    let mut out = Vec::new();
    // Without barlines or repeat marks every chord owns a bar; with them,
    // chords between two barlines share the bar evenly.
    let grouped = text
        .split_whitespace()
        .any(|token| matches!(token, "|" | "%" | "/"));
    // Bars as lists of chords; holds are empty bars.
    let mut bars: Vec<Vec<Chord>> = Vec::new();
    let mut current: Vec<Chord> = Vec::new();
    if !grouped {
        for token in text.split_whitespace() {
            out.push(PlacedChord {
                tick: start + (out.len() as Tick) * bar,
                chord: Chord::parse(token)?,
            });
        }
        return Ok(out);
    }
    let mut tokens = text.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
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
            _ => current.push(Chord::parse(token)?),
        }
        if tokens.peek().is_none() {
            break;
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
    use crate::pitch::PitchClass;

    fn four_four() -> TimeSignature {
        TimeSignature::default()
    }

    fn symbols(placed: &[PlacedChord]) -> Vec<String> {
        placed
            .iter()
            .map(|entry| entry.chord.format(crate::chord::Spelling::Sharp))
            .collect()
    }

    #[test]
    fn one_chord_per_bar_spreads_over_bars() {
        let placed = parse("Am F C G", 0, four_four()).expect("parse");
        assert_eq!(placed.len(), 4);
        assert_eq!(placed[0].tick, 0);
        assert_eq!(placed[1].tick, 3840);
        assert_eq!(placed[3].tick, 11520);
        assert_eq!(symbols(&placed)[0], "Am");
        assert_eq!(placed[0].chord.root, PitchClass::new(9));
    }

    #[test]
    fn barlines_and_holds_are_respected() {
        let placed = parse("C | F | / | G", 0, four_four()).expect("parse");
        assert_eq!(placed.len(), 3, "the held bar adds no chord");
        assert_eq!(placed[0].tick, 0);
        assert_eq!(placed[1].tick, 3840);
        assert_eq!(placed[2].tick, 11520);
    }

    #[test]
    fn percent_repeats_the_previous_bar() {
        let placed = parse("Am F | % | C", 0, four_four()).expect("parse");
        let names = symbols(&placed);
        assert_eq!(names, vec!["Am", "F", "Am", "F", "C"]);
        assert_eq!(placed[2].tick, 3840);
        assert_eq!(placed[3].tick, 3840 + 1920);
        assert_eq!(placed[4].tick, 7680);
    }

    #[test]
    fn several_chords_between_barlines_divide_the_bar() {
        let placed = parse("C Am | F G", 960, four_four()).expect("parse");
        assert_eq!(placed.len(), 4);
        assert_eq!(placed[0].tick, 960);
        assert_eq!(placed[1].tick, 960 + 1920);
        assert_eq!(placed[2].tick, 960 + 3840);
        assert_eq!(placed[3].tick, 960 + 3840 + 1920);
    }

    #[test]
    fn a_three_four_bar_uses_the_meter() {
        let meter = TimeSignature::new(3, 4).expect("valid");
        let placed = parse("C | G", 0, meter).expect("parse");
        assert_eq!(placed[1].tick, 2880);
    }

    #[test]
    fn bad_chords_are_errors() {
        assert!(parse("C notachord", 0, four_four()).is_err());
    }
}
