//! Assistant tools for the user's device profiles.
//!
//! The AI can list the profile files, read one as a pattern, and save a new
//! one, so a user whose synth has no bundled profile can ask for one built
//! from its MIDI chart: sounds, banks, drum kits and the CC controls the
//! Device editor shows. Saving checks the YAML first, so a bad file is never
//! written. Saving is not a `get_` tool, so the assistant proposes it and the
//! user presses Apply.

use serde_json::Value;
use std::path::Path;

/// Ids of the profiles that ship with the app.
pub const BUNDLED: [&str; 6] = [
    "d05",
    "generic-gs",
    "generic-xg",
    "korg-pa",
    "psr-s970",
    "xiosynth",
];

/// What a device-file tool did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Text for the assistant log and the next model round.
    pub text: String,
    /// True when a profile file was written.
    pub saved: bool,
}

/// The extra assistant instructions that name these tools.
pub const HELP: &str = "Device profiles describe a hardware synth: its sounds \
(voices with program and optional bank msb/lsb), banks, drum_kits and controls \
(name, cc, channel, min, max) that the Device editor shows as sliders. Tools: \
`tool get_device_files {}` lists them, `tool get_device_file {\"id\":\"korg-pa\"}` \
returns one as YAML (read one before writing, and use it as the pattern), and \
`tool save_device_file {\"yaml\":\"...\"}` saves a new or changed profile. \
Use these when the user wants to add their own instrument or map its controls.";

/// Runs `command` when it is one of the device-file tools; `None` otherwise.
#[must_use]
pub fn handle(command: &str, folder: &Path) -> Option<Outcome> {
    let rest = command.trim().strip_prefix("tool ")?.trim_start();
    let (name, arguments) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(name, arguments)| (name, arguments.trim()));
    if !matches!(
        name,
        "get_device_files" | "get_device_file" | "save_device_file"
    ) {
        return None;
    }
    let arguments: Value = if arguments.is_empty() {
        Value::Object(serde_json::Map::new())
    } else {
        match serde_json::from_str(arguments) {
            Ok(value) => value,
            Err(error) => return Some(failed(format!("{name}: bad JSON: {error}"))),
        }
    };
    Some(match name {
        "get_device_files" => list(folder),
        "get_device_file" => read(folder, arguments.get("id").and_then(Value::as_str)),
        _ => save(folder, arguments.get("yaml").and_then(Value::as_str)),
    })
}

fn failed(text: String) -> Outcome {
    Outcome { text, saved: false }
}

fn list(folder: &Path) -> Outcome {
    match midi_devices::profiles::load_directory(folder) {
        Ok((profiles, problems)) => {
            let mut text = String::from("Device profiles (id: name):");
            for profile in &profiles {
                let origin = if BUNDLED.contains(&profile.id.as_str()) {
                    "built-in"
                } else {
                    "user"
                };
                text.push_str(&format!(
                    "\n{}: {} ({origin}, {} voices, {} controls)",
                    profile.id,
                    profile.name,
                    profile.voices.len(),
                    profile.controls.len()
                ));
            }
            for (path, error) in problems {
                text.push_str(&format!("\nbroken file {}: {error}", path.display()));
            }
            failed(text)
        }
        Err(error) => failed(format!("get_device_files: {error}")),
    }
}

fn file_for(folder: &Path, id: &str) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(folder).ok()?;
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
                && midi_devices::profiles::load_file(path).is_ok_and(|profile| profile.id == id)
        })
}

fn read(folder: &Path, id: Option<&str>) -> Outcome {
    let Some(id) = id.filter(|id| !id.trim().is_empty()) else {
        return failed(String::from("get_device_file: give an \"id\""));
    };
    match file_for(folder, id).map(std::fs::read_to_string) {
        Some(Ok(text)) => failed(text),
        Some(Err(error)) => failed(format!("get_device_file: {error}")),
        None => failed(format!("get_device_file: no profile with id {id}")),
    }
}

/// True for ids that are safe as a file name.
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn save(folder: &Path, yaml: Option<&str>) -> Outcome {
    let Some(yaml) = yaml.filter(|yaml| !yaml.trim().is_empty()) else {
        return failed(String::from(
            "save_device_file: give the profile as \"yaml\"",
        ));
    };
    let profile = match midi_devices::profiles::parse_yaml(yaml) {
        Ok(profile) => profile,
        Err(error) => return failed(format!("save_device_file: not saved, {error}")),
    };
    if !safe_id(&profile.id) {
        return failed(format!(
            "save_device_file: not saved, the id {:?} may only use letters, digits, - and _",
            profile.id
        ));
    }
    if let Err(error) = std::fs::create_dir_all(folder) {
        return failed(format!("save_device_file: {error}"));
    }
    let path = file_for(folder, &profile.id)
        .unwrap_or_else(|| folder.join(format!("{}.yaml", profile.id)));
    let mut body = yaml.trim_end().to_string();
    body.push('\n');
    match std::fs::write(&path, body) {
        Ok(()) => Outcome {
            text: format!(
                "saved {} ({}): {} voices, {} controls. It is in the Device editor now.",
                profile.name,
                profile.id,
                profile.voices.len(),
                profile.controls.len()
            ),
            saved: true,
        },
        Err(error) => failed(format!("save_device_file: {}: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTH: &str = "id: my-synth\nname: My Synth\nvoices:\n  - name: Piano\n    program: 0\ncontrols:\n  - name: Cutoff\n    cc: 74\n    channel: 1\n";

    fn folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("lazy-arranger-device-files-{name}"));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("dir");
        folder
    }

    fn save_command(yaml: &str) -> String {
        format!(
            "tool save_device_file {}",
            serde_json::json!({ "yaml": yaml })
        )
    }

    #[test]
    fn other_tools_are_left_alone() {
        let folder = folder("other");
        assert_eq!(handle("tool add_track {\"name\":\"Bass\"}", &folder), None);
        assert_eq!(handle("help", &folder), None);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_new_profile_is_saved_listed_and_read_back() {
        let folder = folder("save");
        let saved = handle(&save_command(SYNTH), &folder).expect("tool");
        assert!(saved.saved, "{}", saved.text);
        assert!(folder.join("my-synth.yaml").exists());
        let listed = handle("tool get_device_files {}", &folder).expect("tool");
        assert!(
            listed
                .text
                .contains("my-synth: My Synth (user, 1 voices, 1 controls)")
        );
        let read = handle("tool get_device_file {\"id\":\"my-synth\"}", &folder).expect("tool");
        assert_eq!(read.text, SYNTH);
        assert!(!read.saved);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn saving_again_replaces_the_same_file() {
        let folder = folder("replace");
        std::fs::write(folder.join("custom-name.yaml"), SYNTH).expect("seed");
        let changed = SYNTH.replace("My Synth", "My Synth II");
        assert!(
            handle(&save_command(&changed), &folder)
                .expect("tool")
                .saved
        );
        assert!(!folder.join("my-synth.yaml").exists());
        let text = std::fs::read_to_string(folder.join("custom-name.yaml")).expect("read");
        assert!(text.contains("My Synth II"));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn bad_profiles_and_ids_are_not_written() {
        let folder = folder("bad");
        let broken = handle(&save_command("name: no id\n"), &folder).expect("tool");
        assert!(!broken.saved);
        assert!(broken.text.contains("not saved"));
        let escape = handle(
            &save_command(&SYNTH.replace("id: my-synth", "id: ../escape")),
            &folder,
        )
        .expect("tool");
        assert!(!escape.saved);
        assert!(
            !handle("tool save_device_file {}", &folder)
                .expect("tool")
                .saved
        );
        assert!(
            !handle("tool save_device_file {oops", &folder)
                .expect("tool")
                .saved
        );
        assert_eq!(std::fs::read_dir(&folder).expect("dir").count(), 0);
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn reading_needs_a_known_id() {
        let folder = folder("read");
        assert!(
            handle("tool get_device_file {}", &folder)
                .expect("tool")
                .text
                .contains("give an")
        );
        assert!(
            handle("tool get_device_file {\"id\":\"nope\"}", &folder)
                .expect("tool")
                .text
                .contains("no profile")
        );
        let _ = std::fs::remove_dir_all(folder);
    }
}
