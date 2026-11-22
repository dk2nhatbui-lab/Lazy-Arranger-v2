//! Per-channel chord rules: the Yamaha "Ctab" data that tells a style channel
//! how its notes follow the chord.
//!
//! Field names and defaults follow `src/core/include/arranger/core/StyleRules.h`
//! in the C++ oracle. The three note ranges (`low_range`, `transpose`,
//! `high_range`) let an SFF2 style give notes below or above a split point
//! their own behaviour.

use crate::chord_type::ChordType;

/// How the root of the part moves with the chord.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ntr {
    /// The part transposes with the chord root (default).
    #[default]
    RootTransposition,
    /// The part keeps its position and takes nearest chord tones.
    RootFixed,
    /// Guitar fingering mode: shift by the shortest way, then snap to a chord tone.
    Guitar,
}

/// How notes that are not chord tones are mapped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ntt {
    /// Leave the note alone; the root may still move.
    Bypass,
    /// Chord tones stay chord tones, everything else follows the scale.
    Melody,
    /// Map by chord tone first, then by scale (default).
    #[default]
    Chord,
    /// The melodic-minor flavour. Behaviourally identical to [`Ntt::Melody`].
    MelodicMinor,
    /// The melodic-minor fifth flavour. Behaviourally identical to [`Ntt::Melody`].
    MelodicMinor5th,
    /// The harmonic-minor flavour. Behaviourally identical to [`Ntt::Melody`].
    HarmonicMinor,
    /// The harmonic-minor fifth flavour. Behaviourally identical to [`Ntt::Melody`].
    HarmonicMinor5th,
    /// The natural-minor flavour. Behaviourally identical to [`Ntt::Melody`].
    NaturalMinor,
    /// The natural-minor fifth flavour. Behaviourally identical to [`Ntt::Melody`].
    NaturalMinor5th,
    /// The dorian flavour. Behaviourally identical to [`Ntt::Melody`].
    Dorian,
    /// The dorian fifth flavour. Behaviourally identical to [`Ntt::Melody`].
    Dorian5th,
}

impl Ntt {
    /// True for the flavours that the oracle treats exactly like melody.
    ///
    /// The oracle never reads the flavour when it builds the scale table
    /// (`StyleChord.cpp:366-371`), so the Rust code states that explicitly
    /// instead of copying eight silent aliases.
    #[must_use]
    pub const fn behaves_like_melody(self) -> bool {
        matches!(
            self,
            Self::Melody
                | Self::MelodicMinor
                | Self::MelodicMinor5th
                | Self::HarmonicMinor
                | Self::HarmonicMinor5th
                | Self::NaturalMinor
                | Self::NaturalMinor5th
                | Self::Dorian
                | Self::Dorian5th
        )
    }
}

/// What happens to a sounding note when a new section starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rtr {
    /// Stop the note.
    Stop,
    /// Shift the pitch (default).
    #[default]
    PitchShift,
    /// Shift the pitch to the new root.
    PitchShiftToRoot,
    /// Retrigger the note.
    Retrigger,
    /// Retrigger the note at the new root.
    RetriggerToRoot,
    /// Let the note generate a new one.
    NoteGenerator,
}

/// One note range of a channel rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransposeRange {
    /// How the root moves.
    pub ntr: Ntr,
    /// How non-chord notes map.
    pub ntt: Ntt,
    /// Follow the slash bass of an on-bass chord.
    pub bass: bool,
    /// Highest root before the part drops an octave, `0..=11`.
    pub high_key: u8,
    /// Lowest allowed note.
    pub low_limit: u8,
    /// Highest allowed note.
    pub high_limit: u8,
    /// What happens to held notes on a section change.
    pub rtr: Rtr,
}

impl Default for TransposeRange {
    fn default() -> Self {
        Self {
            ntr: Ntr::RootTransposition,
            ntt: Ntt::Chord,
            bass: false,
            high_key: 6,
            low_limit: 0,
            high_limit: 127,
            rtr: Rtr::PitchShift,
        }
    }
}

/// The rule for one source channel of a style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChordRule {
    /// Source channel in the style, `1..=16`.
    pub source_channel: u8,
    /// Up to eight characters of display name.
    pub name: String,
    /// Channel the part plays on, `1..=16`.
    pub dest_channel: u8,
    /// The style editor may change the rule.
    pub editable: bool,
    /// Bit `n` set: the part plays when the chord root is `n` (0 = C).
    pub note_mute: u16,
    /// Bit `n` set: chord type `n` plays.
    pub chord_mute: u64,
    /// Root the notes were written in, `0..=11`.
    pub source_root: u8,
    /// Chord type the notes were written in.
    pub source_chord_type: ChordType,
    /// Rule for notes inside the middle range.
    pub transpose: TransposeRange,
    /// Lower split point, below which `low_range` applies (SFF2).
    pub middle_low: u8,
    /// Upper split point, above which `high_range` applies (SFF2).
    pub middle_high: u8,
    /// Rule for notes below `middle_low`.
    pub low_range: TransposeRange,
    /// Rule for notes above `middle_high`.
    pub high_range: TransposeRange,
    /// Bytes after the known fields, kept verbatim so a reader never drops
    /// vendor data. Empty for rules this workspace builds itself.
    pub special: Vec<u8>,
}

impl Default for ChordRule {
    fn default() -> Self {
        Self {
            source_channel: 9,
            name: String::new(),
            dest_channel: 9,
            editable: true,
            note_mute: 0x0fff,
            chord_mute: 0xff_ffff_ffff,
            source_root: 0,
            source_chord_type: ChordType::Major7,
            transpose: TransposeRange::default(),
            middle_low: 0,
            middle_high: 127,
            low_range: TransposeRange::default(),
            high_range: TransposeRange::default(),
            special: Vec::new(),
        }
    }
}

impl ChordRule {
    /// Creates a rule with the oracle's defaults, named and split by channel.
    #[must_use]
    pub fn new(source_channel: u8, dest_channel: u8) -> Self {
        Self {
            source_channel,
            dest_channel,
            ..Self::default()
        }
    }

    /// Returns the range that applies to `pitch`.
    ///
    /// The comparisons are strict, so a note exactly on a split point uses the
    /// middle range, matching `StyleChord.cpp:320`.
    #[must_use]
    pub fn range_for(&self, pitch: u8) -> &TransposeRange {
        if pitch < self.middle_low {
            &self.low_range
        } else if pitch > self.middle_high {
            &self.high_range
        } else {
            &self.transpose
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_oracle() {
        let rule = ChordRule::default();
        assert_eq!(rule.source_channel, 9);
        assert_eq!(rule.dest_channel, 9);
        assert_eq!(rule.note_mute, 0x0fff);
        assert_eq!(rule.chord_mute, 0xff_ffff_ffff);
        assert_eq!(rule.source_root, 0);
        assert_eq!(rule.source_chord_type, ChordType::Major7);
        assert_eq!(rule.transpose.ntr, Ntr::RootTransposition);
        assert_eq!(rule.transpose.ntt, Ntt::Chord);
        assert_eq!(rule.transpose.high_key, 6);
        assert_eq!(rule.transpose.low_limit, 0);
        assert_eq!(rule.transpose.high_limit, 127);
        assert_eq!(rule.transpose.rtr, Rtr::PitchShift);
        assert_eq!(rule.middle_low, 0);
        assert_eq!(rule.middle_high, 127);
    }

    #[test]
    fn range_selection_uses_strict_comparisons() {
        let rule = ChordRule {
            middle_low: 48,
            middle_high: 60,
            ..ChordRule::default()
        };
        assert_eq!(rule.range_for(47).low_limit, 0);
        assert_eq!(rule.range_for(48).high_limit, 127);
        assert_eq!(rule.range_for(60).high_limit, 127);
        // Above the middle range the high range applies; mark it to tell them apart.
        let mut marked = rule;
        marked.high_range.high_limit = 100;
        assert_eq!(marked.range_for(61).high_limit, 100);
    }

    #[test]
    fn scale_flavours_are_melody() {
        assert!(Ntt::HarmonicMinor.behaves_like_melody());
        assert!(Ntt::Dorian5th.behaves_like_melody());
        assert!(!Ntt::Chord.behaves_like_melody());
        assert!(!Ntt::Bypass.behaves_like_melody());
    }
}
