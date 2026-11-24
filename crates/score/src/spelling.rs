//! Spelling and accidentals for the score.
//!
//! A key is detected from the chords when present, otherwise from the notes.
//! Each pitch is then written with a letter and an alteration that follows the
//! key signature, showing an accidental only where the note differs from the
//! signature or from an earlier accidental in the same measure.

use music_core::{Chord, ChordQuality, Pitch, PitchClass, Tick};
use project::song::Song;

/// The detected key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    /// Tonic pitch class.
    pub tonic: PitchClass,
    /// True for minor.
    pub minor: bool,
}

/// Semitones of the natural letters C D E F G A B.
const NATURAL: [i8; 7] = [0, 2, 4, 5, 7, 9, 11];
/// Major scale degrees in semitones.
const MAJOR: [i8; 7] = [0, 2, 4, 5, 7, 9, 11];
/// Natural minor scale degrees.
const MINOR: [i8; 7] = [0, 2, 3, 5, 7, 8, 10];

impl Key {
    /// Alterations the key signature applies to each letter.
    #[must_use]
    pub fn signature(self) -> [i8; 7] {
        let scale = if self.minor { MINOR } else { MAJOR };
        // Letters ascend with the scale degrees, so the signature follows the
        // letters of the key: F major puts a flat on B, not on E.
        let (tonic_letter, tonic_alter) = tonic_spelling(self);
        let tonic_sounding = (NATURAL[tonic_letter] + tonic_alter).rem_euclid(12);
        let mut signature = [0_i8; 7];
        for (degree, step) in scale.iter().enumerate() {
            let letter = (tonic_letter + degree) % 7;
            let natural = NATURAL[letter];
            let sounding = (tonic_sounding + step).rem_euclid(12);
            let mut diff = (sounding - natural).rem_euclid(12);
            if diff > 6 {
                diff -= 12;
            }
            signature[letter] = diff.clamp(-1, 1);
        }
        signature
    }
}

/// Conventional letter and alteration for a key tonic.
fn tonic_spelling(key: Key) -> (usize, i8) {
    let pc = key.tonic.as_u8();
    if key.minor {
        match pc {
            0 => (5, 0),   // A
            1 => (0, 1),   // C#
            2 => (1, 0),   // D
            3 => (2, -1),  // Eb
            4 => (2, 0),   // E
            5 => (3, 0),   // F
            6 => (3, 1),   // F#
            7 => (4, 0),   // G
            8 => (4, 1),   // G#
            9 => (5, 0),   // A
            10 => (6, -1), // Bb
            _ => (6, 0),   // B
        }
    } else {
        match pc {
            0 => (0, 0),   // C
            1 => (1, -1),  // Db
            2 => (1, 0),   // D
            3 => (2, -1),  // Eb
            4 => (2, 0),   // E
            5 => (3, 0),   // F
            6 => (3, 1),   // F#
            7 => (4, 0),   // G
            8 => (5, -1),  // Ab
            9 => (5, 0),   // A
            10 => (6, -1), // Bb
            _ => (6, 0),   // B
        }
    }
}

/// One note as a musician reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spelled {
    /// Letter index: 0 = C, 1 = D, ... 6 = B.
    pub step: usize,
    /// Alteration in semitones.
    pub alter: i8,
    /// Octave number.
    pub octave: i32,
    /// Accidental to print, when the note differs from the current spelling.
    pub accidental: Option<i8>,
}

/// Detects the key from the chords, then from the notes.
#[must_use]
pub fn detect_key(song: &Song) -> Key {
    if let Some(chord) = song.chords.last().map(|event| event.chord) {
        let minor = matches!(
            chord.quality,
            ChordQuality::Minor
                | ChordQuality::Minor7
                | ChordQuality::MinorMajor7
                | ChordQuality::Diminished
                | ChordQuality::Diminished7
                | ChordQuality::HalfDiminished7
        );
        return Key {
            tonic: chord.root,
            minor,
        };
    }
    let mut histogram = [0_usize; 12];
    for track in &song.tracks {
        for note in &track.notes {
            histogram[usize::from(note.pitch.as_u8() % 12)] += 1;
        }
    }
    let mut best = Key {
        tonic: PitchClass::new(0),
        minor: false,
    };
    let mut best_score = 0_i64;
    for tonic in 0..12_u8 {
        for minor in [false, true] {
            let scale = if minor { MINOR } else { MAJOR };
            let mut score = 0_i64;
            for (index, weight) in histogram.iter().enumerate() {
                let class = i8::try_from(index).unwrap_or(0);
                if scale.iter().any(|degree| {
                    (i8::try_from(tonic).unwrap_or(0) + degree).rem_euclid(12) == class
                }) {
                    score += i64::try_from(*weight).unwrap_or(0);
                }
            }
            // Prefer a key whose tonic is present; this breaks the tie with
            // its relative major/minor, whose scale uses the same notes.
            score += 2 * i64::try_from(histogram[usize::from(tonic)]).unwrap_or(0);
            if score > best_score {
                best_score = score;
                best = Key {
                    tonic: PitchClass::new(i32::from(tonic)),
                    minor,
                };
            }
        }
    }
    best
}

/// Spells a pitch in a key, updating the accidental state for this measure.
pub fn spell(pitch: Pitch, key: Key, state: &mut [i8; 7]) -> Spelled {
    let pc = i8::try_from(pitch.as_u8() % 12).unwrap_or(0);
    let octave = i32::from(pitch.as_u8() / 12) - 1;
    let signature = key.signature();
    let scale = if key.minor { MINOR } else { MAJOR };
    let (tonic_letter, tonic_alter) = tonic_spelling(key);
    let tonic_sounding = (NATURAL[tonic_letter] + tonic_alter).rem_euclid(12);

    // Diatonic notes use the key's own letters.
    let mut spelled = None;
    for (degree, step) in scale.iter().enumerate() {
        let sounding = (tonic_sounding + step).rem_euclid(12);
        if sounding == pc {
            let letter = (tonic_letter + degree) % 7;
            spelled = Some((letter, signature[letter]));
            break;
        }
    }
    // Chromatic notes take the direction the key signature suggests.
    let sharp_key = signature.iter().filter(|alter| **alter > 0).count()
        >= signature.iter().filter(|alter| **alter < 0).count();
    if spelled.is_none() {
        let mut candidates = Vec::new();
        for (letter, natural) in NATURAL.iter().enumerate() {
            let diff = (pc - natural).rem_euclid(12);
            let alter = if diff > 6 { diff - 12 } else { diff };
            // A natural note can still need a sign when the signature alters
            // its letter.
            if alter.abs() == 1 || (alter == 0 && signature[letter] != 0) {
                candidates.push((letter, alter));
            }
        }
        candidates.sort_by_key(|(letter, alter)| {
            let direction = if sharp_key {
                i8::from(*alter < 0)
            } else {
                i8::from(*alter > 0)
            };
            (direction, alter.abs(), *letter)
        });
        spelled = candidates.first().copied();
    }
    let (step, alter) = spelled.unwrap_or((0, 0));
    let mut sounding_octave = octave;
    if !(0..12).contains(&(NATURAL[step] + alter)) {
        sounding_octave -= 1;
    }
    let accidental = if state[step] != alter {
        state[step] = alter;
        Some(alter)
    } else {
        None
    };
    Spelled {
        step,
        alter,
        octave: sounding_octave,
        accidental,
    }
}

/// The key's chords, when present, for callers that show a key.
#[must_use]
pub fn first_chord_after(song: &Song, tick: Tick) -> Option<Chord> {
    song.chords
        .iter()
        .find(|event| event.tick >= tick)
        .map(|event| event.chord)
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Velocity};
    use project::NoteId;
    use project::song::{ChordEvent, Note, ProjectFile, Track};

    fn note(name: &str, start: Tick) -> Note {
        Note {
            id: NoteId::new(1).expect("valid"),
            start,
            duration: PPQ,
            pitch: Pitch::from_name(name).expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        }
    }

    fn song_with(names: &[&str]) -> Song {
        let mut song = Song::new("Spelling", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        for (index, name) in names.iter().enumerate() {
            let mut entry = note(name, (index as Tick) * PPQ);
            entry.id = NoteId::new(index as u32 + 1).expect("valid");
            track.notes.push(entry);
        }
        song.tracks.push(track);
        song
    }

    fn c_major() -> Key {
        Key {
            tonic: PitchClass::new(0),
            minor: false,
        }
    }

    #[test]
    fn a_c_major_scale_needs_no_accidentals() {
        let mut state = c_major().signature();
        let mut accidentals = Vec::new();
        for name in ["C4", "D4", "E4", "F4", "G4", "A4", "B4"] {
            let spelled = spell(
                Pitch::from_name(name).expect("valid"),
                c_major(),
                &mut state,
            );
            accidentals.push(spelled.accidental);
        }
        assert!(accidentals.iter().all(Option::is_none));
    }

    #[test]
    fn chromatic_notes_get_accidentals_once_per_measure() {
        let mut state = c_major().signature();
        let sharp = spell(
            Pitch::from_name("C#4").expect("valid"),
            c_major(),
            &mut state,
        );
        assert_eq!((sharp.step, sharp.alter, sharp.accidental), (0, 1, Some(1)));
        let again = spell(
            Pitch::from_name("C#4").expect("valid"),
            c_major(),
            &mut state,
        );
        assert_eq!(again.accidental, None, "same spelling in the measure");
    }

    #[test]
    fn the_signature_suppresses_accidentals_in_other_keys() {
        let d_major = Key {
            tonic: PitchClass::new(2),
            minor: false,
        };
        let mut state = d_major.signature();
        let f_sharp = spell(Pitch::from_name("F#4").expect("valid"), d_major, &mut state);
        assert_eq!(
            (f_sharp.step, f_sharp.alter, f_sharp.accidental),
            (3, 1, None)
        );
        let f_natural = spell(Pitch::from_name("F4").expect("valid"), d_major, &mut state);
        assert_eq!(f_natural.accidental, Some(0), "natural sign");
        let f_sharp_again = spell(Pitch::from_name("F#4").expect("valid"), d_major, &mut state);
        assert_eq!(f_sharp_again.accidental, Some(1));
    }

    #[test]
    fn a_flat_key_spells_b_flat() {
        let f_major = Key {
            tonic: PitchClass::new(5),
            minor: false,
        };
        let mut state = f_major.signature();
        assert_eq!(f_major.signature()[6], -1, "B flat in the signature");
        let b_flat = spell(Pitch::from_name("A#4").expect("valid"), f_major, &mut state);
        assert_eq!((b_flat.step, b_flat.alter), (6, -1));
        assert_eq!(b_flat.accidental, None);
    }

    #[test]
    fn c_sharp_minor_uses_its_own_letters() {
        let key = Key {
            tonic: PitchClass::new(1),
            minor: true,
        };
        assert_eq!(key.signature(), [1, 1, 0, 1, 1, 0, 0]);
        let mut state = key.signature();
        let a = spell(Pitch::from_name("A4").expect("valid"), key, &mut state);
        assert_eq!((a.step, a.alter, a.accidental), (5, 0, None));
    }

    #[test]
    fn the_key_is_detected_from_chords_then_notes() {
        let mut song = Song::new("Keys", 120.0);
        song.chords.push(ChordEvent {
            tick: 0,
            chord: Chord::parse("Am").expect("valid"),
        });
        assert_eq!(
            detect_key(&song),
            Key {
                tonic: PitchClass::new(9),
                minor: true
            }
        );
        let notes = song_with(&["C4", "E4", "G4", "C5"]);
        let key = detect_key(&notes);
        assert_eq!(key.tonic.as_u8(), 0);
        assert!(!key.minor);
        let e_flat = song_with(&["D#4", "G4", "A#4", "D#5"]);
        assert_eq!(
            detect_key(&e_flat),
            Key {
                tonic: PitchClass::new(3),
                minor: false
            }
        );
        let _ = ProjectFile::new(notes);
    }
}
