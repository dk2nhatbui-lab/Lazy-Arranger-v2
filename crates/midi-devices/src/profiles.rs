//! Device profiles and nearest-voice matching.
//!
//! A profile lists the voices a device offers. Matching is a keyword score over
//! the track name, falling back to a General MIDI program guess, so a track
//! called "Finger Bass" picks a bass voice even when the profile has no exact
//! name. This is deliberately small: the C++ app loads full YAML profiles with
//! banks, drum kits and mixer maps, which are not implemented.

/// One voice a device offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceSlot {
    /// Display name of the voice.
    pub name: &'static str,
    /// General MIDI program number.
    pub program: u8,
}

/// A device profile with its voice list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceProfile {
    /// Short identifier.
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    /// Voices, in profile order.
    pub voices: &'static [VoiceSlot],
}

/// A subset of the Yamaha PSR-S970 voice list, using GM program numbers.
pub const PSR_S970: DeviceProfile = DeviceProfile {
    id: "psr-s970",
    name: "Yamaha PSR-S970",
    voices: &[
        VoiceSlot {
            name: "Grand Piano",
            program: 0,
        },
        VoiceSlot {
            name: "Bright Piano",
            program: 1,
        },
        VoiceSlot {
            name: "Electric Piano",
            program: 4,
        },
        VoiceSlot {
            name: "Drawbar Organ",
            program: 16,
        },
        VoiceSlot {
            name: "Nylon Guitar",
            program: 24,
        },
        VoiceSlot {
            name: "Steel Guitar",
            program: 25,
        },
        VoiceSlot {
            name: "Acoustic Bass",
            program: 32,
        },
        VoiceSlot {
            name: "Finger Bass",
            program: 33,
        },
        VoiceSlot {
            name: "Strings",
            program: 48,
        },
        VoiceSlot {
            name: "Brass",
            program: 61,
        },
        VoiceSlot {
            name: "Sax",
            program: 65,
        },
        VoiceSlot {
            name: "Flute",
            program: 73,
        },
        VoiceSlot {
            name: "Synth Pad",
            program: 88,
        },
    ],
};

/// A General MIDI program guess from a track name.
#[must_use]
pub fn program_for_track(name: &str) -> u8 {
    let lower = name.to_lowercase();
    if lower
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| matches!(word, "ep" | "rhodes" | "dx"))
    {
        return 4;
    }
    const TABLE: &[(&[&str], u8)] = &[
        (&["drum", "percussion", "kit"], 0),
        (&["piano", "keys", "rhodes"], 0),
        (&["e.piano", "electric piano"], 4),
        (&["organ"], 16),
        (&["nylon", "classical guitar"], 24),
        (&["guitar"], 25),
        (&["acoustic bass", "upright"], 32),
        (&["bass"], 33),
        (&["strings", "violin", "cello"], 48),
        (&["brass", "trumpet", "horn"], 61),
        (&["sax"], 65),
        (&["flute", "whistle"], 73),
        (&["pad", "synth"], 88),
    ];
    for (keywords, program) in TABLE {
        if keywords.iter().any(|keyword| lower.contains(keyword)) {
            return *program;
        }
    }
    0
}

/// The profile's program closest to a track name.
///
/// Scores each voice name by shared words with the track name, then falls back
/// to [`program_for_track`] when nothing matches.
#[must_use]
pub fn nearest_voice(profile: &DeviceProfile, track_name: &str) -> u8 {
    let track_words: Vec<String> = track_name
        .to_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(String::from)
        .collect();
    let mut best = None;
    for voice in profile.voices {
        let voice_words: Vec<String> = voice
            .name
            .to_lowercase()
            .split(|character: char| !character.is_ascii_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(String::from)
            .collect();
        let score = track_words
            .iter()
            .filter(|word| voice_words.contains(word))
            .count();
        if score > 0 {
            best = Some(match best {
                Some((best_score, program)) if best_score >= score => (best_score, program),
                _ => (score, voice.program),
            });
        }
    }
    best.map_or_else(|| program_for_track(track_name), |(_, program)| program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_names_map_to_general_midi_programs() {
        assert_eq!(program_for_track("Acoustic Piano"), 0);
        assert_eq!(program_for_track("Finger Bass"), 33);
        assert_eq!(program_for_track("Warm Strings"), 48);
        assert_eq!(program_for_track("Drum Kit"), 0);
        assert_eq!(program_for_track("Mystery Part"), 0);
        assert_eq!(program_for_track("Ep Main A"), 4);
    }

    #[test]
    fn nearest_voice_prefers_the_profile_name() {
        assert_eq!(nearest_voice(&PSR_S970, "Grand Piano"), 0);
        assert_eq!(nearest_voice(&PSR_S970, "Nylon Guitar"), 24);
        assert_eq!(nearest_voice(&PSR_S970, "Sax"), 65);
        assert_eq!(nearest_voice(&PSR_S970, "Finger Bass"), 33);
    }

    #[test]
    fn unknown_names_fall_back_to_the_heuristic() {
        assert_eq!(nearest_voice(&PSR_S970, "Lead Line"), 0);
        assert_eq!(nearest_voice(&PSR_S970, "Cello Section"), 48);
    }
}
