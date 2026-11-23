//! Groove extraction and application.
//!
//! A groove is the average timing offset and velocity level per grid slot in a
//! bar. Extracting reads a track; applying shifts and accents another track by
//! adjustable amounts. Everything is integer ticks and rounded velocities, so a
//! groove round-trips exactly at full amount.

use music_core::{Tick, Velocity};
use project::song::Note;

/// Timing and accent strength, each 0..=1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Amounts {
    /// Share of the timing offset to apply.
    pub timing: f32,
    /// Share of the level change to apply.
    pub accents: f32,
}

impl Default for Amounts {
    fn default() -> Self {
        Self {
            timing: 1.0,
            accents: 1.0,
        }
    }
}

/// Average feel per grid slot in one bar.
#[derive(Clone, Debug, PartialEq)]
pub struct Groove {
    /// Slot length in ticks.
    pub grid: Tick,
    /// Signed timing offset per slot.
    pub offsets: Vec<i32>,
    /// Velocity level per slot, 1.0 meaning "as written".
    pub levels: Vec<f32>,
}

/// Extracts the feel of `notes` over one bar.
///
/// # Errors
///
/// Returns `None` when the bar or grid is not positive.
#[must_use]
pub fn extract(notes: &[Note], bar: Tick, grid: Tick) -> Option<Groove> {
    if bar <= 0 || grid <= 0 {
        return None;
    }
    let slots = usize::try_from(bar / grid).ok()?.max(1);
    let mut offset_sums = vec![0_i64; slots];
    let mut offset_counts = vec![0_i64; slots];
    let mut level_sums = vec![0_f64; slots];
    let mut level_counts = vec![0_i64; slots];
    let mean = notes
        .iter()
        .map(|note| f64::from(note.velocity.as_u8()))
        .sum::<f64>()
        / notes.len().max(1) as f64;
    for note in notes {
        let in_bar = note.start.rem_euclid(bar);
        let slot = usize::try_from(in_bar / grid).unwrap_or(0) % slots;
        let bar_start = note.start - in_bar;
        let slot_start = bar_start + in_bar - note.start.rem_euclid(grid);
        offset_sums[slot] += note.start - slot_start;
        offset_counts[slot] += 1;
        if mean > 0.0 {
            level_sums[slot] += f64::from(note.velocity.as_u8()) / mean;
            level_counts[slot] += 1;
        }
    }
    Some(Groove {
        grid,
        offsets: (0..slots)
            .map(|slot| {
                if offset_counts[slot] == 0 {
                    0
                } else {
                    i32::try_from(offset_sums[slot] / offset_counts[slot]).unwrap_or(0)
                }
            })
            .collect(),
        levels: (0..slots)
            .map(|slot| {
                if level_counts[slot] == 0 {
                    1.0
                } else {
                    (level_sums[slot] / level_counts[slot] as f64) as f32
                }
            })
            .collect(),
    })
}

/// Applies a groove to notes, returning the changed copy.
///
/// Notes outside the groove's grid keep their timing; only the slots the groove
/// describes are shifted.
#[must_use]
pub fn apply(notes: &[Note], groove: &Groove, amounts: Amounts) -> Vec<Note> {
    if groove.grid <= 0 || groove.offsets.is_empty() {
        return notes.to_vec();
    }
    let slots = groove.offsets.len() as Tick;
    let bar = groove.grid * slots;
    notes
        .iter()
        .map(|note| {
            let slot = usize::try_from(note.start.rem_euclid(bar) / groove.grid)
                .unwrap_or(0)
                .min(groove.offsets.len() - 1);
            let offset = f32::from(i16::try_from(groove.offsets[slot]).unwrap_or(0))
                * amounts.timing.clamp(0.0, 1.0);
            let level = 1.0 + (groove.levels[slot] - 1.0) * amounts.accents.clamp(0.0, 1.0);
            let velocity = (f32::from(note.velocity.as_u8()) * level)
                .round()
                .clamp(1.0, 127.0) as u8;
            Note {
                start: note.start + offset.round() as Tick,
                velocity: Velocity::new(velocity).unwrap_or(note.velocity),
                ..*note
            }
        })
        .collect()
}

/// How closely two grooves match, 1.0 meaning identical.
#[must_use]
pub fn feel_match(a: &Groove, b: &Groove) -> f64 {
    if a.offsets.len() != b.offsets.len() || a.grid != b.grid || a.offsets.is_empty() {
        return 0.0;
    }
    let spread = f64::from(i32::try_from(a.grid).unwrap_or(1).max(1));
    let timing = a
        .offsets
        .iter()
        .zip(&b.offsets)
        .map(|(left, right)| f64::from((left - right).abs()) / spread)
        .sum::<f64>()
        / a.offsets.len() as f64;
    let accent = a
        .levels
        .iter()
        .zip(&b.levels)
        .map(|(left, right)| f64::from((left - right).abs()))
        .sum::<f64>()
        / a.levels.len() as f64;
    (1.0 - (timing + accent) / 2.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, Pitch};
    use project::NoteId;

    fn note(id: u32, start: Tick, velocity: u8) -> Note {
        Note {
            id: NoteId::new(id).expect("valid"),
            start,
            duration: 120,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(velocity).expect("valid"),
            channel: Channel::first(),
        }
    }

    #[test]
    fn notes_after_the_first_bar_keep_their_in_bar_offset() {
        let bar = 4 * music_core::PPQ;
        let grid = music_core::PPQ / 4;
        let notes = vec![
            note(1, 0, 100),
            note(2, bar, 100),
            note(3, bar * 2 + 10, 100),
        ];
        let groove = extract(&notes, bar, grid).expect("groove");
        assert_eq!(groove.offsets[0], 3);
        assert!(
            groove
                .offsets
                .iter()
                .all(|offset| offset.abs() < grid as i32),
            "{:?}",
            groove.offsets
        );
    }

    #[test]
    fn extraction_averages_offsets_and_levels_per_slot() {
        let notes = vec![
            note(1, 0, 100),
            note(2, 240, 80),
            note(3, 480 + 20, 120),
            note(4, 480 + 40, 120),
        ];
        let groove = extract(&notes, 1920, 240).expect("groove");
        assert_eq!(groove.offsets.len(), 8);
        assert_eq!(groove.offsets[0], 0);
        assert_eq!(groove.offsets[2], 30, "average of 20 and 40");
        assert!(groove.levels[2] > 1.0, "louder slot");
        assert!(groove.levels[1] < 1.0, "quieter slot");
    }

    #[test]
    fn applying_at_full_amount_shifts_timing_and_accents() {
        let groove = Groove {
            grid: 240,
            offsets: vec![0, 20, 0, 0, 0, 0, 0, 0],
            levels: vec![1.0, 0.8, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        };
        let notes = vec![note(1, 0, 100), note(2, 240, 100)];
        let applied = apply(&notes, &groove, Amounts::default());
        assert_eq!(applied[0].start, 0);
        assert_eq!(applied[1].start, 260);
        assert_eq!(applied[1].velocity.as_u8(), 80);
    }

    #[test]
    fn half_amounts_halve_the_change() {
        let groove = Groove {
            grid: 240,
            offsets: vec![0, 40, 0, 0, 0, 0, 0, 0],
            levels: vec![1.0, 1.2, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        };
        let notes = vec![note(1, 240, 100)];
        let applied = apply(
            &notes,
            &groove,
            Amounts {
                timing: 0.5,
                accents: 0.5,
            },
        );
        assert_eq!(applied[0].start, 260);
        assert_eq!(applied[0].velocity.as_u8(), 110);
    }

    #[test]
    fn feel_match_is_one_for_identical_grooves() {
        let groove = Groove {
            grid: 240,
            offsets: vec![0, 20, 0, 0, 0, 0, 0, 0],
            levels: vec![1.0, 1.1, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        };
        assert!((feel_match(&groove, &groove) - 1.0).abs() < 1e-9);
        let flat = Groove {
            offsets: vec![0; 8],
            levels: vec![1.0; 8],
            ..groove.clone()
        };
        assert!(feel_match(&groove, &flat) < 1.0);
        assert_eq!(
            feel_match(
                &groove,
                &Groove {
                    grid: 480,
                    offsets: vec![0; 4],
                    levels: vec![1.0; 4]
                }
            ),
            0.0
        );
    }

    #[test]
    fn bad_grids_return_nothing_or_leave_notes_alone() {
        assert!(extract(&[note(1, 0, 100)], 1920, 0).is_none());
        let notes = vec![note(1, 0, 100)];
        let empty = Groove {
            grid: 0,
            offsets: Vec::new(),
            levels: Vec::new(),
        };
        assert_eq!(apply(&notes, &empty, Amounts::default()), notes);
    }
}
