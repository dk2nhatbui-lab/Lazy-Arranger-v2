//! Deterministic graded theory exercises.

use crate::PitchClass;

/// The kind of exercise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LessonKind {
    /// Name a chord's quality.
    ChordQuality,
    /// Name a chord's roman numeral in C major.
    RomanNumeral,
    /// Name an interval.
    Interval,
}

impl LessonKind {
    /// Every kind, in menu order.
    pub const ALL: [Self; 3] = [Self::ChordQuality, Self::RomanNumeral, Self::Interval];

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ChordQuality => "Chord quality",
            Self::RomanNumeral => "Roman numeral",
            Self::Interval => "Interval",
        }
    }
}

/// One multiple-choice question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    /// Question text.
    pub prompt: String,
    /// Answer choices.
    pub choices: Vec<String>,
    /// Index of the correct choice.
    pub answer: usize,
}

fn next(state: &mut u64, bound: u64) -> usize {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    usize::try_from((*state >> 33) % bound.max(1)).unwrap_or(0)
}

const ROOTS: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Builds one question deterministically from `seed`.
#[must_use]
pub fn question(kind: LessonKind, seed: u64) -> Question {
    let mut state = seed
        .wrapping_mul(2_862_933_555_777_941_757)
        .wrapping_add(3_037_000_493);
    match kind {
        LessonKind::ChordQuality => {
            let root = next(&mut state, 12);
            let quality = next(&mut state, 4);
            let suffix = match quality {
                0 => "",
                1 => "m",
                2 => "7",
                _ => "dim",
            };
            let choices = vec![
                String::from("major"),
                String::from("minor"),
                String::from("dominant 7th"),
                String::from("diminished"),
            ];
            Question {
                prompt: format!("What quality is {}{}?", ROOTS[root], suffix),
                choices,
                answer: quality,
            }
        }
        LessonKind::RomanNumeral => {
            let chords = ["C", "Dm", "Em", "F", "G", "Am", "Bdim"];
            let labels = ["I", "ii", "iii", "IV", "V", "vi", "vii°"];
            let index = next(&mut state, 7);
            Question {
                prompt: format!("In C major, what is {}?", chords[index]),
                choices: labels.iter().map(|label| (*label).to_string()).collect(),
                answer: index,
            }
        }
        LessonKind::Interval => {
            let root = next(&mut state, 12);
            let intervals = [3_u8, 4, 5, 7, 12];
            let labels = [
                "minor third",
                "major third",
                "perfect fourth",
                "perfect fifth",
                "octave",
            ];
            let index = next(&mut state, 5);
            let target = PitchClass::new(i32::from(
                (u8::try_from(root).unwrap_or(0) + intervals[index]) % 12,
            ));
            Question {
                prompt: format!(
                    "{} up to {} is which interval?",
                    ROOTS[root],
                    target.name_sharp()
                ),
                choices: labels.iter().map(|label| (*label).to_string()).collect(),
                answer: index,
            }
        }
    }
}

impl Question {
    /// True when `choice` is the answer.
    #[must_use]
    pub fn is_correct(&self, choice: usize) -> bool {
        choice == self.answer
    }

    /// The answer rendered as text.
    #[must_use]
    pub fn answer_text(&self) -> &str {
        self.choices.get(self.answer).map_or("", String::as_str)
    }

    /// A short explanation.
    #[must_use]
    pub fn explanation(&self) -> String {
        format!("Answer: {}", self.answer_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_produces_a_well_formed_question() {
        for kind in LessonKind::ALL {
            for seed in 0..50 {
                let question = question(kind, seed);
                assert!(!question.prompt.is_empty());
                assert!(question.choices.len() >= 4, "{kind:?} seed {seed}");
                assert!(question.answer < question.choices.len());
                assert_eq!(question, question.clone());
            }
        }
    }

    #[test]
    fn questions_are_deterministic_and_answers_match() {
        let first = question(LessonKind::RomanNumeral, 7);
        let second = question(LessonKind::RomanNumeral, 7);
        assert_eq!(first, second);
        assert!(first.is_correct(first.answer));
        assert!(!first.is_correct((first.answer + 1) % first.choices.len()));
        assert!(first.explanation().starts_with("Answer: "));
    }
}
