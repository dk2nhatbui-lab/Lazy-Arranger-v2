//! Beat view: a drum pattern as a step grid.
//!
//! Four rows (kick, snare, closed hat, open hat) and sixteen steps per bar.
//! A pattern reads from a drum track and writes back as notes through the usual
//! commands, so one apply is one undo step.

use app_commands::{Command, EditSession};
use music_core::{Channel, PPQ, Pitch, Tick, Velocity};
use project::song::Note;
use project::{NoteId, TrackId};

use crate::ArrangingError;

/// The four editable rows: pitch and label.
pub const ROWS: [(u8, &str); 4] = [(36, "Kick"), (38, "Snare"), (42, "Hat"), (46, "Open")];
/// Steps in one bar at 1/16.
pub const STEPS: usize = 16;
/// Velocity levels the step button cycles through.
pub const LEVELS: [u8; 4] = [0, 60, 100, 120];

/// A bar of drum steps, velocities 0..=127 where 0 means off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeatPattern {
    /// Row velocities, `ROWS.len()` rows of `STEPS` steps.
    pub grid: Vec<Vec<u8>>,
}

impl Default for BeatPattern {
    fn default() -> Self {
        Self {
            grid: vec![vec![0; STEPS]; ROWS.len()],
        }
    }
}

impl BeatPattern {
    /// Reads the first occurrence of each row pitch in one bar.
    #[must_use]
    pub fn from_track(track: &project::song::Track, start: Tick, bar: Tick) -> Self {
        let mut pattern = Self::default();
        if bar <= 0 {
            return pattern;
        }
        let step = bar / STEPS as Tick;
        for (row, (pitch, _)) in ROWS.iter().enumerate() {
            for note in &track.notes {
                if note.pitch.as_u8() != *pitch || note.start < start || note.start >= start + bar {
                    continue;
                }
                let slot = usize::try_from((note.start - start) / step)
                    .unwrap_or(0)
                    .min(STEPS - 1);
                pattern.grid[row][slot] = note.velocity.as_u8().max(1);
            }
        }
        pattern
    }

    /// Cycles a step through off, soft, normal and loud.
    pub fn cycle(&mut self, row: usize, slot: usize) {
        if row >= self.grid.len() || slot >= STEPS {
            return;
        }
        let current = self.grid[row][slot];
        let next = match current {
            0 => LEVELS[1],
            value if value <= LEVELS[1] => LEVELS[2],
            value if value <= LEVELS[2] => LEVELS[3],
            _ => 0,
        };
        self.grid[row][slot] = next;
    }

    /// Clears one row.
    pub fn clear_row(&mut self, row: usize) {
        if let Some(line) = self.grid.get_mut(row) {
            line.fill(0);
        }
    }

    /// Number of active steps.
    #[must_use]
    pub fn hits(&self) -> usize {
        self.grid
            .iter()
            .map(|row| row.iter().filter(|velocity| **velocity > 0).count())
            .sum()
    }

    /// Notes for one bar, with identifiers starting at 1.
    #[must_use]
    pub fn to_notes(&self, start: Tick, bar: Tick, channel: Channel) -> Vec<Note> {
        let mut notes = Vec::new();
        let step = bar / STEPS as Tick;
        let mut next = 0_u32;
        for (row, (pitch, _)) in ROWS.iter().enumerate() {
            for (slot, velocity) in self.grid[row].iter().enumerate() {
                let velocity = *velocity;
                if velocity == 0 {
                    continue;
                }
                next += 1;
                let Ok(id) = NoteId::new(next) else {
                    continue;
                };
                notes.push(Note {
                    id,
                    start: start + (slot as Tick) * step,
                    duration: step.max(1),
                    pitch: Pitch::new(*pitch).expect("drum pitches are valid"),
                    velocity: Velocity::new(velocity)
                        .unwrap_or_else(|_| Velocity::new(100).expect("valid")),
                    channel,
                });
            }
        }
        notes
    }
}

/// Replaces the drum notes in one bar with a pattern.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn apply(
    session: &mut EditSession,
    track: TrackId,
    start: Tick,
    bar: Tick,
    pattern: &BeatPattern,
) -> Result<usize, ArrangingError> {
    let Some(entry) = session.song().tracks.iter().find(|entry| entry.id == track) else {
        return Ok(0);
    };
    let channel = entry
        .notes
        .iter()
        .find(|note| note.channel.zero_based() == 9)
        .map_or_else(|| Channel::new(9).expect("valid"), |note| note.channel);
    let mut notes: Vec<Note> = entry
        .notes
        .iter()
        .filter(|note| note.start < start || note.start >= start + bar)
        .cloned()
        .collect();
    let added = pattern.to_notes(start, bar, channel);
    notes.extend(added.iter().cloned());
    notes.sort_by_key(|note| note.start);
    for (index, note) in notes.iter_mut().enumerate() {
        if let Ok(id) = NoteId::new(index as u32 + 1) {
            note.id = id;
        }
    }
    session.apply(Command::ReplaceTrackNotes { track, notes })?;
    Ok(added.len())
}

/// One sixteenth in ticks.
#[must_use]
pub fn step_ticks() -> Tick {
    PPQ / 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::song::{ProjectFile, Song, Track};

    fn drum_track() -> Track {
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Drums");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ / 4,
            pitch: Pitch::new(36).expect("valid"),
            velocity: Velocity::new(120).expect("valid"),
            channel: Channel::new(9).expect("valid"),
        });
        track.notes.push(Note {
            id: NoteId::new(2).expect("valid"),
            start: PPQ / 4,
            duration: PPQ / 4,
            pitch: Pitch::new(38).expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::new(9).expect("valid"),
        });
        track
    }

    #[test]
    fn a_bar_reads_into_the_grid() {
        let pattern = BeatPattern::from_track(&drum_track(), 0, 3840);
        assert_eq!(pattern.grid[0][0], 120);
        assert_eq!(pattern.grid[1][1], 100);
        assert_eq!(pattern.hits(), 2);
    }

    #[test]
    fn steps_cycle_and_clear() {
        let mut pattern = BeatPattern::default();
        pattern.cycle(0, 0);
        assert_eq!(pattern.grid[0][0], LEVELS[1]);
        pattern.cycle(0, 0);
        pattern.cycle(0, 0);
        assert_eq!(pattern.grid[0][0], LEVELS[3]);
        pattern.cycle(0, 0);
        assert_eq!(pattern.grid[0][0], 0);
        pattern.cycle(1, 4);
        pattern.clear_row(1);
        assert_eq!(pattern.hits(), 0);
    }

    #[test]
    fn notes_land_on_sixteenths() {
        let mut pattern = BeatPattern::default();
        pattern.cycle(0, 0);
        pattern.cycle(1, 4);
        let notes = pattern.to_notes(3840, 3840, Channel::new(9).expect("valid"));
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].start, 3840);
        assert_eq!(notes[1].start, 3840 + PPQ);
        assert_eq!(notes[0].pitch.as_u8(), 36);
        assert_eq!(notes[1].pitch.as_u8(), 38);
    }

    #[test]
    fn applying_replaces_the_bar_and_undoes() {
        let mut song = Song::new("Beat", 120.0);
        song.tracks.push(drum_track());
        let mut session = EditSession::new(ProjectFile::new(song)).expect("valid");
        let mut pattern = BeatPattern::default();
        pattern.cycle(0, 0);
        pattern.cycle(2, 8);
        let added = apply(
            &mut session,
            TrackId::new(1).expect("valid"),
            0,
            3840,
            &pattern,
        )
        .expect("apply");
        assert_eq!(added, 2);
        assert_eq!(session.song().tracks[0].notes.len(), 2);
        session.undo().expect("undo");
        assert_eq!(session.song().tracks[0].notes.len(), 2);
        assert_eq!(session.song().tracks[0].notes[0].pitch.as_u8(), 36);
    }
}
