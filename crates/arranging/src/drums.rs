//! The Drums view's step grid, as in the C++ `BeatView`: one row per drum
//! sound, one square per step, over a pattern of notes measured from the
//! pattern start.

use music_core::{Channel, Pitch, Tick, Velocity};
use project::NoteId;
use project::song::Note;

/// Soft, normal and accent velocities.
pub const SOFT: u8 = 60;
/// Normal step velocity.
pub const NORMAL: u8 = 100;
/// Accent step velocity.
pub const ACCENT: u8 = 127;

/// Rows every drum grid shows: crash, ride, hats, tom, clap, snare, kick.
pub const DEFAULT_ROWS: [u8; 8] = [49, 51, 46, 42, 45, 39, 38, 36];

/// The General MIDI name of a drum note, when it has one.
#[must_use]
pub fn gm_drum_name(pitch: u8) -> Option<&'static str> {
    const NAMES: [&str; 47] = [
        "Acoustic Bass Drum",
        "Kick",
        "Side Stick",
        "Snare",
        "Clap",
        "Electric Snare",
        "Low Floor Tom",
        "Closed Hat",
        "High Floor Tom",
        "Pedal Hat",
        "Low Tom",
        "Open Hat",
        "Low-Mid Tom",
        "High-Mid Tom",
        "Crash",
        "High Tom",
        "Ride",
        "Chinese Cymbal",
        "Ride Bell",
        "Tambourine",
        "Splash",
        "Cowbell",
        "Crash 2",
        "Vibraslap",
        "Ride 2",
        "High Bongo",
        "Low Bongo",
        "Mute High Conga",
        "Open High Conga",
        "Low Conga",
        "High Timbale",
        "Low Timbale",
        "High Agogo",
        "Low Agogo",
        "Cabasa",
        "Maracas",
        "Short Whistle",
        "Long Whistle",
        "Short Guiro",
        "Long Guiro",
        "Claves",
        "High Wood Block",
        "Low Wood Block",
        "Mute Cuica",
        "Open Cuica",
        "Mute Triangle",
        "Open Triangle",
    ];
    pitch
        .checked_sub(35)
        .and_then(|index| NAMES.get(usize::from(index)))
        .copied()
}

/// The grid rows, top to bottom: the default sounds plus any other pitch the
/// pattern uses, highest first, each with its name.
#[must_use]
pub fn rows(notes: &[Note]) -> Vec<(u8, String)> {
    let mut pitches: Vec<u8> = DEFAULT_ROWS.to_vec();
    pitches.extend(notes.iter().map(|note| note.pitch.as_u8()));
    pitches.sort_unstable_by(|a, b| b.cmp(a));
    pitches.dedup();
    pitches
        .into_iter()
        .map(|pitch| {
            let name = gm_drum_name(pitch).map_or_else(
                || Pitch::new(pitch).map_or_else(|_| pitch.to_string(), |p| p.to_string()),
                str::to_string,
            );
            (pitch, name)
        })
        .collect()
}

/// The velocity of the hit on `pitch` in `[from, from + step)`, or 0.
#[must_use]
pub fn velocity_at(notes: &[Note], pitch: u8, from: Tick, step: Tick) -> u8 {
    notes
        .iter()
        .find(|note| note.pitch.as_u8() == pitch && note.start >= from && note.start < from + step)
        .map_or(0, |note| note.velocity.as_u8())
}

/// Sets one step: removes the hits on `pitch` in the step and, for a
/// velocity above 0, adds one half a step long. Returns false when nothing
/// changed.
pub fn set_step(
    notes: &mut Vec<Note>,
    pitch: u8,
    from: Tick,
    step: Tick,
    velocity: u8,
    channel: Channel,
) -> bool {
    if velocity_at(notes, pitch, from, step) == velocity {
        return false;
    }
    notes.retain(|note| {
        !(note.pitch.as_u8() == pitch && note.start >= from && note.start < from + step)
    });
    if velocity > 0
        && let (Ok(pitch), Ok(velocity), Ok(id)) = (
            Pitch::new(pitch),
            Velocity::new(velocity.min(127)),
            NoteId::new(1),
        )
    {
        notes.push(Note {
            id,
            start: from,
            duration: (step / 2).max(1),
            pitch,
            velocity,
            channel,
        });
    }
    renumber(notes);
    true
}

/// The next velocity a right-click gives: soft, normal, accent, soft.
#[must_use]
pub fn cycle(velocity: u8) -> u8 {
    if velocity == 0 {
        SOFT
    } else if velocity < NORMAL {
        NORMAL
    } else if velocity < ACCENT {
        ACCENT
    } else {
        SOFT
    }
}

/// Repeats a pattern (`pattern` measured from 0, `length` long) over
/// `[from, to)` in `notes`, replacing what was there.
pub fn place(notes: &mut Vec<Note>, pattern: &[Note], length: Tick, from: Tick, to: Tick) {
    notes.retain(|note| note.start < from || note.start >= to);
    let length = length.max(1);
    let mut base = from;
    while base < to {
        for note in pattern.iter().filter(|note| note.start < length) {
            let start = base + note.start;
            if start >= to {
                continue;
            }
            let mut copy = note.clone();
            copy.start = start;
            copy.duration = copy.duration.min(to - start).max(1);
            notes.push(copy);
        }
        base += length;
    }
    renumber(notes);
}

/// Sorts notes and gives them ids from 1.
pub fn renumber(notes: &mut [Note]) {
    notes.sort_by_key(|note| (note.start, note.pitch.as_u8()));
    for (index, note) in notes.iter_mut().enumerate() {
        if let Ok(id) = NoteId::new(index as u32 + 1) {
            note.id = id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::PPQ;

    fn drum_channel() -> Channel {
        Channel::new(9).expect("valid")
    }

    #[test]
    fn rows_list_the_defaults_and_used_pitches_highest_first() {
        let mut notes = Vec::new();
        set_step(&mut notes, 56, 0, PPQ / 4, NORMAL, drum_channel());
        let rows = rows(&notes);
        assert_eq!(rows.first().map(|row| row.0), Some(56));
        assert_eq!(rows.first().map(|row| row.1.as_str()), Some("Cowbell"));
        assert_eq!(rows.last().map(|row| row.1.as_str()), Some("Kick"));
        assert_eq!(rows.len(), 9);
    }

    #[test]
    fn steps_toggle_and_keep_one_hit() {
        let step = PPQ / 4;
        let mut notes = Vec::new();
        assert!(set_step(&mut notes, 36, step, step, NORMAL, drum_channel()));
        assert_eq!(velocity_at(&notes, 36, step, step), NORMAL);
        assert!(!set_step(
            &mut notes,
            36,
            step,
            step,
            NORMAL,
            drum_channel()
        ));
        assert!(set_step(&mut notes, 36, step, step, ACCENT, drum_channel()));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].duration, step / 2);
        assert!(set_step(&mut notes, 36, step, step, 0, drum_channel()));
        assert!(notes.is_empty());
        assert_eq!(cycle(0), SOFT);
        assert_eq!(cycle(ACCENT), SOFT);
    }

    #[test]
    fn a_pattern_repeats_over_a_range_replacing_it() {
        let mut pattern = Vec::new();
        set_step(&mut pattern, 36, 0, PPQ / 4, NORMAL, drum_channel());
        let mut notes = Vec::new();
        set_step(&mut notes, 38, 4000, PPQ / 4, NORMAL, drum_channel());
        set_step(&mut notes, 38, 20000, PPQ / 4, NORMAL, drum_channel());
        place(&mut notes, &pattern, 3840, 3840, 3840 * 3);
        let starts: Vec<Tick> = notes.iter().map(|note| note.start).collect();
        assert_eq!(starts, vec![3840, 7680, 20000]);
    }
}
