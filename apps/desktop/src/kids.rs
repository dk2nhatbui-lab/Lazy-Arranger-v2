//! Kids Mode lesson: step through the song's chords with one big button.

use music_core::Chord;

/// A chord step-through.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lesson {
    /// Chords in song order.
    pub chords: Vec<Chord>,
    /// Current position; equals the length when finished.
    pub index: usize,
}

impl Lesson {
    /// Builds a lesson from a song's chord events.
    #[must_use]
    pub fn new(chords: Vec<Chord>) -> Self {
        Self { chords, index: 0 }
    }

    /// True when there is nothing to step through.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chords.is_empty()
    }

    /// The chord to play now.
    #[must_use]
    pub fn current(&self) -> Option<Chord> {
        self.chords.get(self.index).copied()
    }

    /// Advances, returning the next chord; finishes after the last one.
    pub fn advance(&mut self) -> Option<Chord> {
        if self.index < self.chords.len() {
            self.index += 1;
        }
        self.current()
    }

    /// Rebuilds the lesson when the chord list changed, keeping the position
    /// when the new list is long enough.
    pub fn sync(&mut self, chords: Vec<Chord>) -> bool {
        if self.chords == chords {
            return false;
        }
        self.chords = chords;
        self.index = self.index.min(self.chords.len());
        true
    }

    /// Starts over.
    pub fn reset(&mut self) {
        self.index = 0;
    }

    /// Position as shown in the UI: one-based current, and the total.
    #[must_use]
    pub fn progress(&self) -> (usize, usize) {
        if self.chords.is_empty() {
            return (0, 0);
        }
        ((self.index + 1).min(self.chords.len()), self.chords.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(name: &str) -> Chord {
        Chord::parse(name).expect("valid")
    }

    #[test]
    fn stepping_walks_the_chords_and_finishes() {
        let mut lesson = Lesson::new(vec![chord("C"), chord("F"), chord("G")]);
        assert_eq!(lesson.current(), Some(chord("C")));
        assert_eq!(lesson.progress(), (1, 3));
        assert_eq!(lesson.advance(), Some(chord("F")));
        assert_eq!(lesson.advance(), Some(chord("G")));
        assert_eq!(lesson.advance(), None, "finished");
        assert_eq!(lesson.progress(), (3, 3));
        lesson.reset();
        assert_eq!(lesson.current(), Some(chord("C")));
    }

    #[test]
    fn syncing_keeps_the_position_when_possible() {
        let mut lesson = Lesson::new(vec![chord("C"), chord("F"), chord("G")]);
        lesson.advance();
        assert!(!lesson.sync(vec![chord("C"), chord("F"), chord("G")]));
        assert_eq!(lesson.current(), Some(chord("F")));
        assert!(lesson.sync(vec![chord("C"), chord("F"), chord("G"), chord("Am")]));
        assert_eq!(lesson.current(), Some(chord("F")), "position kept");
        assert!(lesson.sync(vec![chord("C")]));
        assert_eq!(lesson.current(), None, "position clamped");
    }

    #[test]
    fn an_empty_lesson_has_nothing_to_step() {
        let mut lesson = Lesson::new(Vec::new());
        assert!(lesson.is_empty());
        assert_eq!(lesson.current(), None);
        assert_eq!(lesson.advance(), None);
        assert_eq!(lesson.progress(), (0, 0));
    }
}
