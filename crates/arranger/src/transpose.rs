//! Chord-to-note conversion, transcribed from the C++ oracle.
//!
//! The ordered steps and their edge cases are documented in
//! `docs/audit/notes/cpp-style-arranger.md` §3. This module implements that
//! specification: eight steps, strict split-point comparisons, chord-tone
//! mapping before scale mapping, nearest-note placement for fixed roots,
//! octave folding inside the limits, and a final clamp to the MIDI range.

use crate::chord_type::{ArrangerChord, ChordType, TypeInfo};
use crate::rules::{ChordRule, Ntr, Ntt, TransposeRange};

/// Wraps a value into `0..12`.
#[must_use]
pub fn mod12(value: i32) -> i32 {
    value.rem_euclid(12)
}

/// Maps a source interval onto its interval in the target chord.
///
/// Returns `None` when the source interval is not the root, third, fifth or
/// seventh of the source chord type.
#[must_use]
pub fn chord_tone_for(rel: i32, from: &TypeInfo, to: &TypeInfo) -> Option<i32> {
    if rel == 0 {
        return Some(0);
    }
    if rel == from.third {
        return Some(to.third);
    }
    if rel == from.fifth {
        return Some(to.fifth);
    }
    if from.seventh >= 0 && rel == from.seventh {
        return Some(if to.seventh >= 0 { to.seventh } else { 12 });
    }
    None
}

/// Maps a source interval onto the same scale degree of the target chord type,
/// keeping a chromatic note chromatic.
#[must_use]
pub fn scale_tone_for(rel: i32, from: &TypeInfo, to: &TypeInfo) -> i32 {
    let mut degree = 0;
    for (index, step) in from.scale.iter().enumerate() {
        if *step <= rel {
            degree = index;
        }
    }
    let offset = rel - from.scale[degree];
    to.scale[degree] + offset
}

/// Converts one style note for the current chord.
///
/// Returns `None` when the note must not sound.
#[must_use]
pub fn transpose(pitch: u8, rule: &ChordRule, chord: &ArrangerChord) -> Option<u8> {
    // Step 0: choose the record that covers this pitch.
    let range = rule.range_for(pitch);
    let fixed_root = range.ntr != Ntr::RootTransposition;
    let pitch_i = i32::from(pitch);

    // Step 1: bypass runs before the no-chord test, so drums survive it.
    if fixed_root && range.ntt == Ntt::Bypass {
        return Some(pitch);
    }

    // Step 2: no chord.
    if chord.chord_type == ChordType::Cancel {
        return None;
    }

    // Step 3: target root, optionally following the slash bass.
    let target_root = i32::from(if range.bass {
        chord.bass.unwrap_or(chord.root).as_u8()
    } else {
        chord.root.as_u8()
    });

    // Step 4: guitar fingering mode ignores NTT.
    if range.ntr == Ntr::Guitar {
        let mut shift = mod12(target_root - i32::from(rule.source_root));
        if shift > 6 {
            shift -= 12;
        }
        let moved = pitch_i + shift;
        let mut in_chord = [false; 12];
        for tone in chord.chord_type.tones() {
            in_chord[usize::try_from(mod12(i32::from(chord.root.as_u8()) + tone))
                .expect("mod12 is 0..12")] = true;
        }
        let mut result = moved;
        for distance in 0..=6 {
            if in_chord[usize::try_from(mod12(moved - distance)).expect("mod12 is 0..12")] {
                result = moved - distance;
                break;
            }
            if in_chord[usize::try_from(mod12(moved + distance)).expect("mod12 is 0..12")] {
                result = moved + distance;
                break;
            }
        }
        return Some(fold_and_clamp(result, range));
    }

    // Step 5: source geometry.
    let from = rule.source_chord_type.info();
    let to = chord.chord_type.info();
    let rel = mod12(pitch_i - i32::from(rule.source_root));
    let base = pitch_i - rel;

    // Step 6: interval mapping.
    let interval = if range.ntt == Ntt::Chord {
        chord_tone_for(rel, from, to).unwrap_or_else(|| scale_tone_for(rel, from, to))
    } else if range.ntt == Ntt::Bypass {
        rel
    } else {
        // Melody and every scale flavour: chord notes stay chord notes, but an
        // interval of 12 (a seventh over a triad) is left to the scale path.
        match chord_tone_for(rel, from, to) {
            Some(value) if value < 12 => value,
            _ => scale_tone_for(rel, from, to),
        }
    };

    // Step 7: place the root.
    let result = if fixed_root {
        let wanted = mod12(target_root + interval);
        let mut up = pitch_i;
        while mod12(up) != wanted {
            up += 1;
        }
        let mut down = pitch_i;
        while mod12(down) != wanted {
            down -= 1;
        }
        if (up - pitch_i) < (pitch_i - down) {
            up
        } else {
            down
        }
    } else {
        let high_key = i32::from(range.high_key.min(11));
        let shift = mod12(high_key - i32::from(rule.source_root)) - mod12(high_key - target_root);
        base + interval + shift
    };

    // Step 8: fold by octaves inside the limits, then clamp.
    Some(fold_and_clamp(result, range))
}

fn fold_and_clamp(result: i32, range: &TransposeRange) -> u8 {
    let low = i32::from(range.low_limit);
    let high = i32::from(range.high_limit);
    let mut value = result;
    if low <= high && high - low >= 11 {
        while value < low {
            value += 12;
        }
        while value > high {
            value -= 12;
        }
    }
    u8::try_from(value.clamp(0, 127)).expect("clamp keeps the range")
}

/// Tells whether a part may sound for this chord.
///
/// A false result clears the whole part in the oracle
/// (`StyleConvert.cpp:509-510`); this function only reports the decision.
#[must_use]
pub fn plays(rule: &ChordRule, chord: &ArrangerChord) -> bool {
    if chord.chord_type == ChordType::Cancel {
        return (rule.chord_mute >> ChordType::Cancel.as_index()) & 1 != 0;
    }
    let root_bit = mod12(i32::from(chord.root.as_u8()));
    if (rule.note_mute >> root_bit) & 1 == 0 {
        return false;
    }
    (rule.chord_mute >> chord.chord_type.as_index()) & 1 != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Ntr, Ntt};
    use music_core::PitchClass;

    fn pitch_class(value: i32) -> PitchClass {
        PitchClass::new(value)
    }

    fn chord(root: i32, chord_type: ChordType) -> ArrangerChord {
        ArrangerChord::new(pitch_class(root), chord_type)
    }

    /// The bass rule from `StyleFileTests.cpp:173-200`.
    fn bass_rule() -> ChordRule {
        ChordRule {
            source_root: 0,
            source_chord_type: ChordType::Major7,
            transpose: TransposeRange {
                ntt: Ntt::Melody,
                high_key: 6,
                low_limit: 28,
                high_limit: 60,
                ..TransposeRange::default()
            },
            ..ChordRule::default()
        }
    }

    /// The chord rule from `StyleFileTests.cpp:205-216`.
    fn chord_rule() -> ChordRule {
        ChordRule {
            source_root: 0,
            source_chord_type: ChordType::Major7,
            transpose: TransposeRange {
                ntr: Ntr::RootFixed,
                ntt: Ntt::Chord,
                ..TransposeRange::default()
            },
            ..ChordRule::default()
        }
    }

    #[test]
    fn pinned_bass_cases_from_the_oracle() {
        let rule = bass_rule();
        assert_eq!(transpose(36, &rule, &chord(0, ChordType::Major)), Some(36));
        assert_eq!(transpose(36, &rule, &chord(5, ChordType::Major)), Some(41));
        assert_eq!(transpose(36, &rule, &chord(7, ChordType::Major)), Some(31));
        assert_eq!(transpose(40, &rule, &chord(9, ChordType::Minor)), Some(36));
        assert_eq!(
            transpose(43, &rule, &chord(2, ChordType::Minor7Flat5)),
            Some(44)
        );
        assert_eq!(transpose(38, &rule, &chord(0, ChordType::Minor)), Some(38));
    }

    #[test]
    fn bass_flag_follows_the_slash_bass() {
        let mut rule = bass_rule();
        rule.transpose.bass = true;
        let c_over_e = chord(0, ChordType::Major).with_bass(pitch_class(4));
        assert_eq!(transpose(36, &rule, &c_over_e), Some(40));

        rule.transpose.bass = false;
        assert_eq!(transpose(36, &rule, &c_over_e), Some(36));
    }

    #[test]
    fn limits_fold_by_octaves() {
        let mut rule = bass_rule();
        rule.transpose.low_limit = 36;
        rule.transpose.high_limit = 60;
        assert_eq!(transpose(36, &rule, &chord(7, ChordType::Major)), Some(43));
    }

    #[test]
    fn pinned_fixed_root_cases_from_the_oracle() {
        let rule = chord_rule();
        assert_eq!(transpose(52, &rule, &chord(9, ChordType::Minor)), Some(48));
        assert_eq!(transpose(55, &rule, &chord(9, ChordType::Minor)), Some(52));
        assert_eq!(transpose(59, &rule, &chord(0, ChordType::Major)), Some(60));
        assert_eq!(
            transpose(59, &rule, &chord(0, ChordType::Seventh)),
            Some(58)
        );
        assert_eq!(transpose(52, &rule, &chord(0, ChordType::Sus4)), Some(53));
    }

    #[test]
    fn fixed_root_ties_go_down() {
        // C (60) into F# major: the root is the only chord tone the source
        // note maps to, so the wanted class is 6. F# is six semitones up (66)
        // and six down (54); the oracle resolves the tie downwards.
        let rule = chord_rule();
        assert_eq!(transpose(60, &rule, &chord(6, ChordType::Major)), Some(54));
    }

    #[test]
    fn drums_bypass_everything_including_no_chord() {
        let drums = ChordRule {
            transpose: TransposeRange {
                ntr: Ntr::RootFixed,
                ntt: Ntt::Bypass,
                ..TransposeRange::default()
            },
            ..ChordRule::default()
        };
        assert_eq!(transpose(38, &drums, &chord(7, ChordType::Minor)), Some(38));
        assert_eq!(
            transpose(38, &drums, &ArrangerChord::cancel()),
            Some(38),
            "bypass runs before the no-chord test"
        );

        let chord_part = chord_rule();
        assert_eq!(
            transpose(52, &chord_part, &ArrangerChord::cancel()),
            None,
            "other parts stop"
        );
    }

    #[test]
    fn split_ranges_choose_their_own_rule() {
        let mut split = chord_rule();
        split.middle_low = 48;
        split.low_range = TransposeRange {
            low_limit: 0,
            high_limit: 127,
            ..bass_rule().transpose
        };
        assert_eq!(transpose(36, &split, &chord(5, ChordType::Major)), Some(41));
        assert_eq!(transpose(52, &split, &chord(9, ChordType::Minor)), Some(48));
    }

    #[test]
    fn mute_masks_gate_parts() {
        let mut muted = chord_rule();
        muted.note_mute = 0x0fff & !(1 << 7);
        muted.chord_mute &= !(1_u64 << ChordType::Diminished.as_index());
        // Mutating a clone is clearer here than a long struct literal.
        assert!(plays(&muted, &chord(0, ChordType::Major)));
        assert!(!plays(&muted, &chord(7, ChordType::Major)));
        assert!(!plays(&muted, &chord(0, ChordType::Diminished)));
    }

    #[test]
    fn cancel_gating_follows_the_chord_mute_bit() {
        // The oracle default mask has all 40 bits set, including the cancel
        // bit, so the part gate says "plays". The notes still die in
        // `transpose` unless the rule bypasses (drums).
        let rule = ChordRule::default();
        assert!(plays(&rule, &ArrangerChord::cancel()));

        let mut rhythm_only = rule;
        rhythm_only.chord_mute &= !(1_u64 << ChordType::Cancel.as_index());
        assert!(!plays(&rhythm_only, &ArrangerChord::cancel()));
        assert!(
            plays(&rhythm_only, &chord(0, ChordType::Major)),
            "the other bits stay set"
        );
    }

    #[test]
    fn scale_flavours_behave_like_melody() {
        let melody = bass_rule();
        for ntt in [
            Ntt::MelodicMinor,
            Ntt::HarmonicMinor,
            Ntt::NaturalMinor,
            Ntt::Dorian,
            Ntt::Dorian5th,
            Ntt::HarmonicMinor5th,
            Ntt::MelodicMinor5th,
            Ntt::NaturalMinor5th,
        ] {
            let mut flavoured = melody.clone();
            flavoured.transpose.ntt = ntt;
            for pitch in [31_u8, 36, 38, 40, 43, 47, 52] {
                for target in [
                    chord(0, ChordType::Major),
                    chord(5, ChordType::Major),
                    chord(7, ChordType::Minor),
                    chord(9, ChordType::Minor7),
                ] {
                    assert_eq!(
                        transpose(pitch, &flavoured, &target),
                        transpose(pitch, &melody, &target),
                        "{ntt:?} differs from melody"
                    );
                }
            }
        }
    }

    #[test]
    fn a_triad_target_puts_the_source_seventh_an_octave_up() {
        let chord_path = chord_rule();
        // B (59) is the seventh of CMaj7 (interval 11); C major is a triad.
        assert_eq!(
            transpose(59, &chord_path, &chord(0, ChordType::Major)),
            Some(60)
        );

        let melody = bass_rule();
        // The melody path refuses the 12 and follows the scale instead.
        let result = transpose(59, &melody, &chord(0, ChordType::Major));
        assert!(result.is_some());
        assert_ne!(result, Some(72), "the 12 is not placed above the root");
    }

    #[test]
    fn results_are_clamped_to_the_midi_range() {
        let rule = ChordRule {
            transpose: TransposeRange {
                low_limit: 0,
                high_limit: 127,
                high_key: 11,
                ..bass_rule().transpose
            },
            ..bass_rule()
        };
        // A very low note with a root shift must stay playable.
        for pitch in [0_u8, 1, 60, 127] {
            if let Some(result) = transpose(pitch, &rule, &chord(11, ChordType::Major)) {
                assert!(result <= 127);
            }
        }
    }

    #[test]
    fn guitar_mode_moves_by_the_shortest_way_then_snaps() {
        let guitar = ChordRule {
            source_root: 0,
            source_chord_type: ChordType::Major,
            transpose: TransposeRange {
                ntr: Ntr::Guitar,
                ..TransposeRange::default()
            },
            ..ChordRule::default()
        };
        // C major fingering to A minor: the root moves down 3, so C becomes A
        // (57), which is already an A-minor tone.
        assert_eq!(
            transpose(60, &guitar, &chord(9, ChordType::Minor)),
            Some(57)
        );
        // E (64) moved down 3 is C# (61); the nearest A-minor tone is C (60),
        // one semitone below.
        assert_eq!(
            transpose(64, &guitar, &chord(9, ChordType::Minor)),
            Some(60)
        );
    }

    #[test]
    fn source_seventh_larger_than_target_uses_root_above() {
        let from = ChordType::Major7.info();
        let to = ChordType::Major.info();
        assert_eq!(chord_tone_for(11, from, to), Some(12));
        assert_eq!(chord_tone_for(4, from, to), Some(4));
        assert_eq!(chord_tone_for(5, from, to), None);
    }

    #[test]
    fn chromatic_notes_stay_chromatic() {
        let rule = ChordRule {
            source_root: 0,
            source_chord_type: ChordType::Major7,
            transpose: TransposeRange {
                ntt: Ntt::Melody,
                ..TransposeRange::default()
            },
            ..ChordRule::default()
        };
        // Eb4 (63) over a C major source stays Eb in C major...
        assert_eq!(transpose(63, &rule, &chord(0, ChordType::Major)), Some(63));
        // ...and becomes Ab4 in F major, keeping the minor third.
        assert_eq!(transpose(63, &rule, &chord(5, ChordType::Major)), Some(68));
    }
}
