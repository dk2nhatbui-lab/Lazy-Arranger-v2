use serde::{Deserialize, Serialize};

use crate::MusicError;
use crate::time::{PPQ, SamplePosition, Tick, ticks_to_quarters};

/// A constant tempo that starts at a tick position.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TempoChange {
    /// The tick where this tempo starts.
    pub tick: Tick,
    /// Beats per minute from `tick` until the next change.
    pub bpm: f64,
}

/// A piecewise-constant tempo map.
///
/// Each change holds until the next change starts, and the last change holds
/// to the end of the song. Tempo is not ramped: a ramp would need a separate
/// curve type so that both ends and the shape stay explicit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TempoMap {
    changes: Vec<TempoChange>,
}

impl Default for TempoMap {
    fn default() -> Self {
        Self::constant(120.0)
    }
}

impl TempoMap {
    /// Creates a map with one tempo at tick 0.
    #[must_use]
    pub fn constant(bpm: f64) -> Self {
        Self {
            changes: vec![TempoChange { tick: 0, bpm }],
        }
    }

    /// Creates a map from an explicit change list.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::TempoMap`] when the list is empty, does not start
    /// at tick 0, is not strictly ascending, or holds an invalid tempo.
    pub fn new(changes: Vec<TempoChange>) -> Result<Self, MusicError> {
        let map = Self { changes };
        map.validate()?;
        Ok(map)
    }

    /// Checks the map invariants.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::TempoMap`] describing the first broken invariant.
    pub fn validate(&self) -> Result<(), MusicError> {
        let Some(first) = self.changes.first() else {
            return Err(MusicError::TempoMap(String::from("no tempo changes")));
        };
        if first.tick != 0 {
            return Err(MusicError::TempoMap(format!(
                "first tempo change is at tick {}, expected 0",
                first.tick
            )));
        }
        let mut previous = None;
        for change in &self.changes {
            if let Some(previous) = previous
                && change.tick <= previous
            {
                return Err(MusicError::TempoMap(format!(
                    "tempo changes must be strictly ascending: {previous} then {}",
                    change.tick
                )));
            }
            check_bpm(change.bpm)?;
            previous = Some(change.tick);
        }
        Ok(())
    }

    /// Returns the tempo changes in tick order.
    #[must_use]
    pub fn changes(&self) -> &[TempoChange] {
        &self.changes
    }

    /// Returns a copy with one more tempo change.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::TempoMap`] when `tick` already holds a change or
    /// the tempo is invalid.
    pub fn with_change(mut self, tick: Tick, bpm: f64) -> Result<Self, MusicError> {
        check_bpm(bpm)?;
        if tick < 0 {
            return Err(MusicError::TempoMap(format!(
                "tempo change at negative tick {tick}"
            )));
        }
        let position = self
            .changes
            .iter()
            .position(|change| change.tick >= tick)
            .unwrap_or(self.changes.len());
        if self
            .changes
            .get(position)
            .is_some_and(|change| change.tick == tick)
        {
            return Err(MusicError::TempoMap(format!(
                "tick {tick} already holds a tempo change"
            )));
        }
        self.changes.insert(position, TempoChange { tick, bpm });
        self.validate()?;
        Ok(self)
    }

    /// Tempo in effect at `tick`.
    #[must_use]
    pub fn bpm_at(&self, tick: Tick) -> f64 {
        let index = self
            .changes
            .partition_point(|change| change.tick <= tick)
            .saturating_sub(1);
        self.changes.get(index).map_or(120.0, |change| change.bpm)
    }

    /// Seconds from tick 0 to `tick`.
    ///
    /// Computed from the map's absolute positions on every call, so long
    /// sessions do not accumulate floating-point drift.
    #[must_use]
    pub fn seconds_at(&self, tick: Tick) -> f64 {
        if tick <= 0 {
            return 0.0;
        }
        let mut total = 0.0;
        for (index, change) in self.changes.iter().enumerate() {
            if tick <= change.tick {
                break;
            }
            let segment_end = self
                .changes
                .get(index + 1)
                .map_or(tick, |next| next.tick.min(tick));
            total += seconds_for_ticks(segment_end - change.tick, change.bpm);
        }
        total
    }

    /// Seconds between two ticks.
    #[must_use]
    pub fn duration_seconds(&self, from: Tick, to: Tick) -> f64 {
        self.seconds_at(to) - self.seconds_at(from)
    }

    /// Tick at a number of seconds from tick 0, rounded down.
    ///
    /// Rounds down so that converting a timestamp to ticks never lands after
    /// the event it describes.
    #[must_use]
    pub fn tick_at_seconds(&self, seconds: f64) -> Tick {
        if !seconds.is_finite() || seconds <= 0.0 {
            return 0;
        }
        let mut elapsed = 0.0;
        for (index, change) in self.changes.iter().enumerate() {
            let seconds_per_tick = 60.0 / (change.bpm * PPQ as f64);
            let remaining = seconds - elapsed;
            match self.changes.get(index + 1) {
                None => {
                    return change
                        .tick
                        .saturating_add((remaining / seconds_per_tick).floor() as Tick);
                }
                Some(next) => {
                    let segment_ticks = next.tick - change.tick;
                    let segment_seconds = seconds_for_ticks(segment_ticks, change.bpm);
                    if remaining < segment_seconds {
                        return change
                            .tick
                            .saturating_add((remaining / seconds_per_tick).floor() as Tick);
                    }
                    elapsed += segment_seconds;
                }
            }
        }
        // Unreachable while the map starts at tick 0, kept for totality.
        self.changes.last().map_or(0, |change| change.tick)
    }

    /// Sample position of `tick` at `sample_rate`, rounded up.
    ///
    /// Rounding up matches the C++ oracle (`ProcessContext::sampleForTick`,
    /// `src/engine/src/Transport.cpp`), which uses `ceil(samples - 1e-6)` so
    /// that an event never sounds before its tick and floating-point noise
    /// does not push it one sample late. See ADR-002.
    #[must_use]
    pub fn samples_at(&self, tick: Tick, sample_rate: u32) -> SamplePosition {
        seconds_to_samples(self.seconds_at(tick), sample_rate)
    }

    /// Tick at a sample position, rounded down.
    #[must_use]
    pub fn tick_at_samples(&self, sample: SamplePosition, sample_rate: u32) -> Tick {
        if sample_rate == 0 {
            return 0;
        }
        self.tick_at_seconds(sample as f64 / f64::from(sample_rate))
    }
}

fn check_bpm(bpm: f64) -> Result<(), MusicError> {
    if bpm.is_finite() && bpm > 0.0 {
        Ok(())
    } else {
        Err(MusicError::Tempo { bpm })
    }
}

fn seconds_for_ticks(ticks: Tick, bpm: f64) -> f64 {
    ticks_to_quarters(ticks) * 60.0 / bpm
}

/// Smallest sample count whose end is not before `seconds`, with an epsilon
/// that absorbs floating-point noise.
const CEILING_EPSILON: f64 = 1.0e-6;

/// Converts seconds to whole samples, rounding up.
#[must_use]
pub fn seconds_to_samples(seconds: f64, sample_rate: u32) -> SamplePosition {
    (seconds * f64::from(sample_rate) - CEILING_EPSILON).ceil() as SamplePosition
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(left: f64, right: f64, tolerance: f64) -> bool {
        (left - right).abs() <= tolerance
    }

    #[test]
    fn an_empty_deserialized_map_has_a_safe_tempo() {
        let map = TempoMap {
            changes: Vec::new(),
        };
        assert_eq!(map.bpm_at(0), 120.0);
        assert_eq!(map.tick_at_seconds(1.0), 0);
    }

    #[test]
    fn constant_tempo_geometry() {
        let map = TempoMap::constant(120.0);
        assert!(approx(map.seconds_at(960), 0.5, 1e-12));
        assert!(approx(map.seconds_at(3840), 2.0, 1e-12));
        assert_eq!(map.samples_at(3840, 48_000), 96_000);
        assert_eq!(map.samples_at(15_360, 48_000), 384_000);
        assert_eq!(map.samples_at(960, 44_100), 22_050);
        assert_eq!(map.bpm_at(0), 120.0);
        assert_eq!(map.bpm_at(999_999), 120.0);
    }

    #[test]
    fn tempo_changes_are_piecewise_constant() {
        let map = TempoMap::constant(120.0)
            .with_change(3840, 60.0)
            .expect("valid change");
        assert_eq!(map.bpm_at(3839), 120.0);
        assert_eq!(map.bpm_at(3840), 60.0);
        assert!(approx(map.seconds_at(3840), 2.0, 1e-12));
        assert!(approx(map.seconds_at(7680), 6.0, 1e-12));
        assert!(approx(map.duration_seconds(3840, 7680), 4.0, 1e-12));
    }

    #[test]
    fn rounding_never_lands_before_the_tick() {
        // 137 bpm quarter note at 48 kHz: 21021.897... samples, so 21022.
        let map = TempoMap::constant(137.0);
        assert_eq!(map.samples_at(960, 48_000), 21_022);
        // Two ticks at 137 bpm: 43.795... samples, so 44.
        assert_eq!(map.samples_at(2, 48_000), 44);
        // A tick that lands exactly on a sample must stay on that sample, not
        // slip one later because of floating-point noise.
        assert_eq!(TempoMap::constant(120.0).samples_at(1, 48_000), 25);
        assert_eq!(TempoMap::constant(120.0).samples_at(0, 48_000), 0);
        // Every event lands at or after its exact time.
        let map = TempoMap::constant(133.0);
        for tick in 1..200 {
            let exact = map.seconds_at(tick) * 48_000.0;
            let rounded = map.samples_at(tick, 48_000) as f64;
            assert!(rounded >= exact - 1e-6, "tick {tick}: {rounded} < {exact}");
            assert!(
                rounded - exact < 1.0 + 1e-6,
                "tick {tick}: {rounded} >= {exact} + 1"
            );
        }
    }

    #[test]
    fn tick_and_second_conversions_invert_each_other() {
        let map = TempoMap::constant(120.0)
            .with_change(1920, 90.0)
            .expect("valid change");
        for tick in [0, 1, 960, 1920, 1921, 5000, 15_360] {
            let seconds = map.seconds_at(tick);
            let back = map.tick_at_seconds(seconds);
            assert!(back <= tick, "tick {tick} came back as {back}");
            assert!(tick - back <= 1, "tick {tick} came back as {back}");
            let samples = map.samples_at(tick, 48_000);
            let from_samples = map.tick_at_samples(samples, 48_000);
            assert!(
                (tick - from_samples).abs() <= 1,
                "tick {tick} -> {from_samples}"
            );
        }
    }

    #[test]
    fn long_maps_do_not_drift() {
        // Twelve hours at 140 bpm must agree with direct arithmetic.
        let map = TempoMap::constant(140.0);
        let quarters = 12.0 * 60.0 * 140.0;
        let ticks = (quarters * 960.0) as Tick;
        let expected = quarters * 60.0 / 140.0;
        assert!(approx(map.seconds_at(ticks), expected, 1e-6));
        let samples = map.samples_at(ticks, 48_000);
        assert!((samples - (expected * 48_000.0).ceil() as SamplePosition).abs() <= 1);
    }

    #[test]
    fn invalid_tempo_maps_are_rejected() {
        assert!(TempoMap::new(Vec::new()).is_err());
        assert!(
            TempoMap::new(vec![TempoChange {
                tick: 1,
                bpm: 120.0
            }])
            .is_err()
        );
        assert!(
            TempoMap::new(vec![
                TempoChange {
                    tick: 0,
                    bpm: 120.0
                },
                TempoChange {
                    tick: 0,
                    bpm: 120.0
                }
            ])
            .is_err()
        );
        assert!(
            TempoMap::new(vec![
                TempoChange {
                    tick: 0,
                    bpm: 120.0
                },
                TempoChange {
                    tick: 10,
                    bpm: 90.0
                },
                TempoChange { tick: 5, bpm: 80.0 }
            ])
            .is_err()
        );
        assert!(TempoMap::constant(0.0).validate().is_err());
        assert!(TempoMap::constant(f64::NAN).validate().is_err());
        assert!(TempoMap::constant(f64::INFINITY).validate().is_err());
    }

    #[test]
    fn duplicate_change_tick_is_rejected() {
        let map = TempoMap::constant(120.0);
        let err = map.with_change(0, 90.0).expect_err("duplicate tick");
        assert!(err.to_string().contains("already holds"));
    }

    #[test]
    fn negative_and_zero_seconds_map_to_tick_zero() {
        let map = TempoMap::constant(120.0);
        assert_eq!(map.tick_at_seconds(-1.0), 0);
        assert_eq!(map.tick_at_seconds(0.0), 0);
        assert_eq!(map.seconds_at(-5), 0.0);
    }

    #[test]
    fn tempo_map_serde_round_trips() {
        let map = TempoMap::constant(120.0)
            .with_change(1920, 90.0)
            .expect("valid change");
        let json = serde_json::to_string(&map).expect("serialize");
        let back: TempoMap = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, map);
        back.validate().expect("still valid");
    }
}
