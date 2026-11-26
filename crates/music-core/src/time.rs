use serde::{Deserialize, Serialize};

use crate::MusicError;

/// The edit-timeline unit. One quarter note is [`PPQ`] ticks.
pub type Tick = i64;

/// A position on the playback timeline, counted in audio samples.
pub type SamplePosition = i64;

/// Ticks per quarter note. Fixed for every project file in schema version 1.
pub const PPQ: Tick = 960;

/// A time signature with a power-of-two denominator.
///
/// Version 1 of the project schema allows one signature per song. The type is
/// separate so a signature map can be added without changing callers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSignature {
    /// Beats per bar.
    pub numerator: u8,
    /// Note value that counts as one beat; must be a power of two.
    pub denominator: u8,
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

impl TimeSignature {
    /// Validates and creates a time signature.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::TimeSignature`] when the numerator is zero, the
    /// numerator exceeds 32, or the denominator is not a power of two in
    /// `1..=32`.
    pub fn new(numerator: u8, denominator: u8) -> Result<Self, MusicError> {
        let invalid = |numerator, denominator| MusicError::TimeSignature {
            numerator,
            denominator,
        };
        if numerator == 0 || numerator > 32 {
            return Err(invalid(numerator, denominator));
        }
        if denominator == 0 || denominator > 32 || !denominator.is_power_of_two() {
            return Err(invalid(numerator, denominator));
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    /// Ticks in one beat.
    #[must_use]
    pub const fn beat_ticks(self) -> Tick {
        // The fields are public, so guard an unvalidated zero instead of
        // dividing by it.
        if self.denominator == 0 {
            return PPQ;
        }
        let ticks = PPQ * 4 / self.denominator as Tick;
        if ticks > 0 { ticks } else { 1 }
    }

    /// Ticks in one bar.
    #[must_use]
    pub const fn bar_ticks(self) -> Tick {
        let numerator = if self.numerator == 0 {
            1
        } else {
            self.numerator as Tick
        };
        self.beat_ticks() * numerator
    }

    /// Ticks in `bars` bars.
    #[must_use]
    pub const fn bars_to_ticks(self, bars: u32) -> Tick {
        self.bar_ticks().saturating_mul(bars as Tick)
    }

    /// Zero-based bar index that contains `tick`.
    #[must_use]
    pub const fn bar_index_at(self, tick: Tick) -> i64 {
        tick.div_euclid(self.bar_ticks())
    }

    /// Zero-based beat index that contains `tick`, counted from bar 1.
    #[must_use]
    pub const fn beat_index_at(self, tick: Tick) -> i64 {
        tick.div_euclid(self.beat_ticks())
    }

    /// One-based beat number inside the bar, `1..=numerator`.
    #[must_use]
    pub const fn beat_in_bar(self, tick: Tick) -> u8 {
        let numerator = if self.numerator == 0 {
            1
        } else {
            self.numerator as i64
        };
        (self.beat_index_at(tick).rem_euclid(numerator) + 1) as u8
    }

    /// First beat boundary at or after `tick`.
    #[must_use]
    pub const fn beat_boundary_at_or_after(self, tick: Tick) -> Tick {
        let beat = self.beat_ticks();
        // Near the end of the range there is no later boundary; never
        // round down below `tick`.
        match tick.checked_add(beat - 1) {
            Some(end) => end.div_euclid(beat) * beat,
            None => Tick::MAX,
        }
    }

    /// First bar boundary at or after `tick`.
    #[must_use]
    pub const fn bar_boundary_at_or_after(self, tick: Tick) -> Tick {
        let bar = self.bar_ticks();
        // Near the end of the range there is no later boundary; never
        // round down below `tick`.
        match tick.checked_add(bar - 1) {
            Some(end) => end.div_euclid(bar) * bar,
            None => Tick::MAX,
        }
    }

    /// First beat boundary strictly after `tick`.
    #[must_use]
    pub const fn beat_boundary_after(self, tick: Tick) -> Tick {
        let beat = self.beat_ticks();
        tick.div_euclid(beat).saturating_add(1).saturating_mul(beat)
    }

    /// First bar boundary strictly after `tick`.
    #[must_use]
    pub const fn bar_boundary_after(self, tick: Tick) -> Tick {
        self.bar_index_at(tick)
            .saturating_add(1)
            .saturating_mul(self.bar_ticks())
    }
}

/// Converts ticks to quarter-note units.
#[must_use]
pub fn ticks_to_quarters(tick: Tick) -> f64 {
    tick as f64 / PPQ as f64
}

/// Converts quarter-note units to ticks, rounding to the nearest tick.
#[must_use]
pub fn quarters_to_ticks(quarters: f64) -> Tick {
    (quarters * PPQ as f64).round() as Tick
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_geometry() {
        let four_four = TimeSignature::default();
        assert_eq!(four_four.beat_ticks(), 960);
        assert_eq!(four_four.bar_ticks(), 3840);
        assert_eq!(four_four.bars_to_ticks(4), 15360);

        let six_eight = TimeSignature::new(6, 8).expect("valid");
        assert_eq!(six_eight.beat_ticks(), 480);
        assert_eq!(six_eight.bar_ticks(), 2880);

        assert_eq!(four_four.bar_index_at(3839), 0);
        assert_eq!(four_four.bar_index_at(3840), 1);
        assert_eq!(four_four.beat_in_bar(0), 1);
        assert_eq!(four_four.beat_in_bar(959), 1);
        assert_eq!(four_four.beat_in_bar(960), 2);
        assert_eq!(four_four.beat_in_bar(3840), 1);
    }

    #[test]
    fn boundary_helpers_cover_exact_and_interior_ticks() {
        let sig = TimeSignature::default();
        assert_eq!(sig.beat_boundary_at_or_after(0), 0);
        assert_eq!(sig.beat_boundary_at_or_after(1), 960);
        assert_eq!(sig.beat_boundary_at_or_after(960), 960);
        assert_eq!(sig.beat_boundary_after(960), 1920);
        assert_eq!(sig.bar_boundary_at_or_after(3841), 7680);
        assert_eq!(sig.bar_boundary_at_or_after(3840), 3840);
        assert_eq!(sig.bar_boundary_after(3840), 7680);
        assert_eq!(sig.bar_boundary_at_or_after(0), 0);
    }

    #[test]
    fn boundary_helpers_handle_negative_ticks() {
        let sig = TimeSignature::default();
        assert_eq!(sig.beat_boundary_at_or_after(-1), 0);
        assert_eq!(sig.beat_boundary_at_or_after(-960), -960);
        assert_eq!(sig.bar_boundary_at_or_after(-1), 0);
        assert_eq!(sig.bar_index_at(-1), -1);
    }

    #[test]
    fn signature_validation_rejects_bad_values() {
        for (numerator, denominator) in [(0, 4), (4, 0), (4, 3), (4, 64), (33, 4), (4, 5)] {
            assert!(
                TimeSignature::new(numerator, denominator).is_err(),
                "{numerator}/{denominator}"
            );
        }
        for (numerator, denominator) in [(1, 1), (4, 4), (6, 8), (7, 16), (12, 8), (32, 32)] {
            assert!(
                TimeSignature::new(numerator, denominator).is_ok(),
                "{numerator}/{denominator}"
            );
        }
    }

    #[test]
    fn quarter_conversion_round_trips() {
        for tick in [0, 1, 480, 960, 15360, 999_999] {
            assert_eq!(quarters_to_ticks(ticks_to_quarters(tick)), tick);
        }
    }
}
