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

/// A bank loaded from a profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bank {
    /// Display name.
    pub name: String,
    /// MIDI program number.
    pub program: u8,
}

/// A drum kit loaded from a profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrumKit {
    /// Display name.
    pub name: String,
    /// MIDI program number.
    pub program: u8,
}

/// A profile loaded from the YAML subset this build understands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadedProfile {
    /// Short identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Voices in file order.
    pub voices: Vec<(String, u8)>,
    /// Banks, parsed but not used by export yet.
    pub banks: Vec<Bank>,
    /// Drum kits, parsed but not used by export yet.
    pub drum_kits: Vec<DrumKit>,
}

/// Parses the profile subset: scalars, and `- name`/`program` lists under
/// `voices`, `banks` and `drum_kits`.
///
/// # Errors
///
/// Returns a message for a missing id or name, an unknown key, a bad program or
/// a list item without a name.
pub fn parse_yaml(text: &str) -> Result<LoadedProfile, String> {
    #[derive(PartialEq)]
    enum Section {
        None,
        Voices,
        Banks,
        DrumKits,
    }
    let mut profile = LoadedProfile::default();
    let mut section = Section::None;
    let mut pending: Option<(String, Option<u8>)> = None;
    let flush = |profile: &mut LoadedProfile,
                 section: &Section,
                 pending: &mut Option<(String, Option<u8>)>|
     -> Result<(), String> {
        if let Some((name, program)) = pending.take() {
            let program = program.ok_or_else(|| format!("{name}: missing program"))?;
            match section {
                Section::Voices => profile.voices.push((name, program)),
                Section::Banks => profile.banks.push(Bank { name, program }),
                Section::DrumKits => profile.drum_kits.push(DrumKit { name, program }),
                Section::None => return Err(String::from("list item outside a section")),
            }
        }
        Ok(())
    };
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let error = |message: &str| format!("line {}: {message}", number + 1);
        if let Some(rest) = trimmed.strip_prefix("- ") {
            flush(&mut profile, &section, &mut pending)?;
            let Some(name) = rest.trim().strip_prefix("name:") else {
                return Err(error("a list item must start with name:"));
            };
            pending = Some((name.trim().to_string(), None));
            continue;
        }
        let (key, value) = trimmed
            .split_once(':')
            .ok_or_else(|| error("expected key: value"))?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "id" => profile.id = value.to_string(),
            "name" => profile.name = value.to_string(),
            "voices" | "banks" | "drum_kits" => {
                flush(&mut profile, &section, &mut pending)?;
                section = match key {
                    "voices" => Section::Voices,
                    "banks" => Section::Banks,
                    _ => Section::DrumKits,
                };
            }
            "program" => {
                let program: u8 = value
                    .parse()
                    .map_err(|_| error("program must be 0..=127"))?;
                if program > 127 {
                    return Err(error("program must be 0..=127"));
                }
                match &mut pending {
                    Some((_, slot)) => *slot = Some(program),
                    None => return Err(error("program without a list item")),
                }
            }
            other => return Err(error(&format!("unknown key {other:?}"))),
        }
    }
    flush(&mut profile, &section, &mut pending)?;
    if profile.id.is_empty() {
        return Err(String::from("profile needs an id"));
    }
    if profile.name.is_empty() {
        return Err(String::from("profile needs a name"));
    }
    if profile.voices.is_empty() {
        return Err(String::from("profile needs at least one voice"));
    }
    Ok(profile)
}

/// Loads a profile file.
///
/// # Errors
///
/// Returns a message when the file cannot be read or parsed.
pub fn load_file(path: &std::path::Path) -> Result<LoadedProfile, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    parse_yaml(&text)
}

/// The nearest program in a loaded profile, using the same keyword scoring as
/// the built-in profile and falling back to [`program_for_track`].
#[must_use]
pub fn nearest_in_profile(profile: &LoadedProfile, track_name: &str) -> u8 {
    let words: Vec<String> = track_name
        .to_ascii_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect();
    let mut best = None;
    let mut best_score = 0;
    for (name, program) in &profile.voices {
        let voice = name.to_ascii_lowercase();
        let score = words
            .iter()
            .filter(|word| voice.contains(word.as_str()))
            .count();
        if score > best_score {
            best_score = score;
            best = Some(*program);
        }
    }
    best.unwrap_or_else(|| program_for_track(track_name))
}

#[cfg(test)]
mod yaml_tests {
    use super::*;

    const SAMPLE: &str = r#"
# a small profile
id: psr-s970
name: PSR-S970
voices:
  - name: Grand Piano
    program: 0
  - name: Finger Bass
    program: 33
banks:
  - name: Preset
    program: 0
drum_kits:
  - name: Standard Kit
    program: 0
"#;

    #[test]
    fn a_profile_loads_with_voices_banks_and_kits() {
        let profile = parse_yaml(SAMPLE).expect("parse");
        assert_eq!(profile.id, "psr-s970");
        assert_eq!(profile.voices.len(), 2);
        assert_eq!(profile.banks.len(), 1);
        assert_eq!(profile.drum_kits[0].name, "Standard Kit");
        assert_eq!(nearest_in_profile(&profile, "Finger Bass"), 33);
        assert_eq!(nearest_in_profile(&profile, "piano"), 0);
        assert_eq!(
            nearest_in_profile(&profile, "Trumpet"),
            program_for_track("Trumpet")
        );
    }

    #[test]
    fn malformed_profiles_are_rejected() {
        assert!(parse_yaml("id: x\nname: X\n").is_err(), "no voices");
        assert!(parse_yaml("id: x\nname: X\nvoices:\n  - name: A\n").is_err());
        assert!(parse_yaml("id: x\nname: X\nvoices:\n  - name: A\n    program: 200\n").is_err());
        assert!(parse_yaml("id: x\nname: X\nvoices:\n  - program: 1\n").is_err());
        assert!(
            parse_yaml("id: x\nname: X\nvoices:\n  - name: A\n    program: 1\ncolor: red\n")
                .is_err()
        );
    }

    #[test]
    fn a_missing_file_is_an_error() {
        assert!(load_file(std::path::Path::new("/tmp/no-profile-lazy.yaml")).is_err());
    }
}
