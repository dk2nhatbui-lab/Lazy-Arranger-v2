//! Discovery of installed VST3 and Audio Unit bundles.
//!
//! This reads the standard plug-in folders and reports each bundle's name, path
//! and format. It does not load, validate or host a plug-in.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Plug-in format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Format {
    /// VST3.
    Vst3,
    /// Audio Unit component.
    AudioUnit,
}

impl Format {
    /// Short label for output.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Vst3 => "VST3",
            Self::AudioUnit => "AU",
        }
    }
}

/// What a plug-in can do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Role {
    /// Generates sound from notes.
    Instrument,
    /// Processes audio.
    Effect,
    /// Not declared by the bundle metadata.
    #[default]
    Unknown,
}

/// One plug-in found on disk, with the metadata its bundle declares.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FoundPlugin {
    /// Display name, from the bundle or its metadata.
    pub name: String,
    /// Vendor, when the bundle declares one.
    pub vendor: String,
    /// Bundle version, when declared.
    pub version: String,
    /// Stable-ish identifier: VST3 class id or AU manufacturer:subtype.
    pub identifier: String,
    /// Instrument or effect.
    pub role: Role,
    /// Bundle path.
    pub path: PathBuf,
    /// Plug-in format.
    pub format: Format,
}

/// A directory to scan for one format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanRoot {
    /// Format expected in the directory.
    pub format: Format,
    /// Folder to scan.
    pub path: PathBuf,
}

/// Standard plug-in folders for the current platform.
#[must_use]
pub fn default_roots() -> Vec<ScanRoot> {
    let mut roots = Vec::new();
    if cfg!(target_os = "macos") {
        for base in ["/Library/Audio/Plug-Ins", "/System/Library/Audio/Plug-Ins"] {
            roots.push(ScanRoot {
                format: Format::AudioUnit,
                path: PathBuf::from(base).join("Components"),
            });
        }
        roots.push(ScanRoot {
            format: Format::Vst3,
            path: PathBuf::from("/Library/Audio/Plug-Ins/VST3"),
        });
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(ScanRoot {
                format: Format::AudioUnit,
                path: PathBuf::from(&home).join("Library/Audio/Plug-Ins/Components"),
            });
            roots.push(ScanRoot {
                format: Format::Vst3,
                path: PathBuf::from(&home).join("Library/Audio/Plug-Ins/VST3"),
            });
        }
    } else if cfg!(target_os = "windows") {
        for base in [
            r"C:\Program Files\Common Files\VST3",
            r"C:\Program Files (x86)\Common Files\VST3",
        ] {
            roots.push(ScanRoot {
                format: Format::Vst3,
                path: PathBuf::from(base),
            });
        }
    } else {
        roots.push(ScanRoot {
            format: Format::Vst3,
            path: PathBuf::from("/usr/lib/vst3"),
        });
        roots.push(ScanRoot {
            format: Format::Vst3,
            path: PathBuf::from("/usr/local/lib/vst3"),
        });
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(ScanRoot {
                format: Format::Vst3,
                path: PathBuf::from(home).join(".vst3"),
            });
        }
    }
    roots
}

fn bundle_extension(format: Format) -> &'static str {
    match format {
        Format::Vst3 => "vst3",
        Format::AudioUnit => "component",
    }
}

/// Result of a scan, including bundles that crashed or timed out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanReport {
    /// Successfully scanned plug-ins.
    pub plugins: Vec<FoundPlugin>,
    /// VST3 bundles the isolated scanner could not read.
    pub failed: Vec<PathBuf>,
}

/// Default blocklist file.
pub const BLOCKLIST_FILE: &str = "lazy-arranger-plugin-blocklist.json";

/// Reads a blocklist; missing or malformed files are empty.
#[must_use]
pub fn load_blocklist(path: &Path) -> Vec<PathBuf> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes a blocklist.
///
/// # Errors
///
/// Returns a message when the file cannot be written.
pub fn save_blocklist(path: &Path, blocked: &[PathBuf]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(blocked).map_err(|error| error.to_string())?;
    std::fs::write(path, json).map_err(|error| format!("{}: {error}", path.display()))
}

/// Scans roots for bundles of the expected extension, sorted by path.
///
/// Missing roots are skipped. A directory counts as a bundle only when it has
/// the expected extension; this does not prove that the plug-in loads.
#[must_use]
pub fn scan_roots(roots: &[ScanRoot]) -> Vec<FoundPlugin> {
    scan_roots_filtered(roots, &[]).plugins
}

/// Scans roots, skipping blocked bundle paths and recording VST3 failures.
#[must_use]
pub fn scan_roots_filtered(roots: &[ScanRoot], blocked: &[PathBuf]) -> ScanReport {
    let mut report = ScanReport::default();
    // Compare canonical paths so a symlink cannot sneak a blocked bundle in.
    let canonical =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let blocked: Vec<PathBuf> = blocked.iter().map(|path| canonical(path)).collect();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root.path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || blocked.contains(&canonical(&path)) {
                continue;
            }
            let extension = path.extension().and_then(|value| value.to_str());
            if extension
                .is_some_and(|value| value.eq_ignore_ascii_case(bundle_extension(root.format)))
            {
                match root.format {
                    Format::AudioUnit => report.plugins.extend(au_plugins(&path)),
                    Format::Vst3 => report
                        .plugins
                        .extend(vst3_plugins(&path, &mut report.failed)),
                }
            }
        }
    }
    report
        .plugins
        .sort_by(|left, right| left.path.cmp(&right.path));
    report
}

fn fallback(path: &Path, format: Format) -> FoundPlugin {
    FoundPlugin {
        name: path.file_stem().map_or_else(
            || String::from("unknown"),
            |stem| stem.to_string_lossy().into_owned(),
        ),
        vendor: String::new(),
        version: String::new(),
        identifier: path.display().to_string(),
        role: Role::Unknown,
        path: path.to_path_buf(),
        format,
    }
}

fn role_from_text(text: &str) -> Role {
    let text = text.to_ascii_lowercase();
    if text.contains("instrument") || text.contains("synth") || text.contains("generator") {
        Role::Instrument
    } else if text.contains("fx") || text.contains("effect") || text.contains("aufx") {
        Role::Effect
    } else {
        Role::Unknown
    }
}

fn au_plugins(path: &Path) -> Vec<FoundPlugin> {
    let info = path.join("Contents").join("Info.plist");
    let Some(Ok(value)) =
        read_capped(&info).map(|bytes| plist::Value::from_reader(std::io::Cursor::new(bytes)))
    else {
        return vec![fallback(path, Format::AudioUnit)];
    };
    let Some(dict) = value.as_dictionary() else {
        return vec![fallback(path, Format::AudioUnit)];
    };
    let bundle_name = dict
        .get("CFBundleDisplayName")
        .or_else(|| dict.get("CFBundleName"))
        .and_then(plist::Value::as_string)
        .map(str::to_string);
    let bundle_id = dict
        .get("CFBundleIdentifier")
        .and_then(plist::Value::as_string)
        .unwrap_or("");
    let version = dict
        .get("CFBundleShortVersionString")
        .and_then(plist::Value::as_string)
        .unwrap_or("");
    let components: Vec<&plist::Dictionary> = dict
        .get("AudioComponents")
        .and_then(plist::Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(plist::Value::as_dictionary)
                .collect()
        })
        .unwrap_or_default();
    if components.is_empty() {
        return vec![FoundPlugin {
            name: bundle_name.unwrap_or_else(|| fallback(path, Format::AudioUnit).name),
            vendor: bundle_id.split('.').nth(1).unwrap_or("").to_string(),
            version: version.to_string(),
            identifier: bundle_id.to_string(),
            role: Role::Unknown,
            path: path.to_path_buf(),
            format: Format::AudioUnit,
        }];
    }
    components
        .into_iter()
        .map(|component| {
            let name = component
                .get("name")
                .and_then(plist::Value::as_string)
                .or(bundle_name.as_deref())
                .unwrap_or("Audio Unit");
            let manufacturer = component
                .get("manufacturer")
                .and_then(plist::Value::as_string)
                .unwrap_or("");
            let subtype = component
                .get("subtype")
                .and_then(plist::Value::as_string)
                .unwrap_or("");
            let kind = component
                .get("type")
                .and_then(plist::Value::as_string)
                .unwrap_or("");
            FoundPlugin {
                name: name.to_string(),
                vendor: manufacturer.to_string(),
                version: version.to_string(),
                identifier: format!("{manufacturer}:{subtype}"),
                role: role_from_text(kind),
                path: path.to_path_buf(),
                format: Format::AudioUnit,
            }
        })
        .collect()
}

#[derive(serde::Deserialize)]
struct ScannedClass {
    cid: String,
    name: String,
    vendor: String,
    version: String,
    subcategories: String,
}

/// The largest metadata file (`moduleinfo.json`, `Info.plist`) the scanner reads.
const MAX_METADATA_BYTES: u64 = 4 * 1024 * 1024;

/// Reads a bundle metadata file, or `None` when it is missing or too large.
fn read_capped(path: &Path) -> Option<Vec<u8>> {
    if std::fs::metadata(path).ok()?.len() > MAX_METADATA_BYTES {
        return None;
    }
    std::fs::read(path).ok()
}

fn scanner_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    // Test binaries live one folder below the helper (`target/*/deps`).
    let name = format!("lazy-arranger-plugin-scan{}", std::env::consts::EXE_SUFFIX);
    std::iter::once(dir)
        .chain(dir.parent())
        .map(|base| base.join(&name))
        .find(|candidate| candidate.is_file() && *candidate != exe)
}

/// The most the scanner helper may print before its output is ignored.
const MAX_SCAN_OUTPUT: u64 = 1024 * 1024;

fn scan_vst3_isolated(path: &Path) -> Result<Vec<ScannedClass>, ()> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let helper = scanner_path().ok_or(())?;
    let mut child = Command::new(helper)
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ())?;
    let stdout = child.stdout.take().ok_or(())?;
    // The reader reports through a channel, so a grandchild that keeps the
    // pipe open can never block the host past the deadline.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        let _ = stdout.take(MAX_SCAN_OUTPUT).read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(());
            }
        }
    };
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .max(Duration::from_millis(100));
    let stdout = receiver.recv_timeout(remaining).map_err(|_| ())?;
    if !status.success() {
        return Err(());
    }
    serde_json::from_slice(&stdout).map_err(|_| ())
}

fn found_from_classes(path: &Path, classes: Vec<ScannedClass>) -> Vec<FoundPlugin> {
    classes
        .into_iter()
        .map(|class| FoundPlugin {
            name: class.name,
            vendor: class.vendor,
            version: class.version,
            identifier: class.cid,
            role: role_from_text(&class.subcategories),
            path: path.to_path_buf(),
            format: Format::Vst3,
        })
        .collect()
}

fn has_vst3_binary(path: &Path) -> bool {
    let contents = path.join("Contents");
    let Ok(entries) = std::fs::read_dir(&contents) else {
        return false;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() || dir.file_name().is_some_and(|name| name == "Resources") {
            continue;
        }
        if std::fs::read_dir(&dir)
            .ok()
            .is_some_and(|files| files.flatten().any(|file| file.path().is_file()))
        {
            return true;
        }
    }
    false
}

fn moduleinfo_plugins(path: &Path) -> Option<Vec<FoundPlugin>> {
    let moduleinfo = path
        .join("Contents")
        .join("Resources")
        .join("moduleinfo.json");
    let bytes = read_capped(&moduleinfo)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let classes = value.get("Classes")?.as_array()?;
    let mut found = Vec::new();
    for class in classes {
        if class
            .get("Category")
            .and_then(serde_json::Value::as_str)
            .is_none_or(|category| category != "Audio Module Class")
        {
            continue;
        }
        let name = class
            .get("Name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("VST3 plug-in");
        let vendor = class
            .get("Vendor")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let version = class
            .get("Version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let subcategories = class
            .get("Sub Categories")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let identifier = class
            .get("CID")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        found.push(FoundPlugin {
            name: name.to_string(),
            vendor: vendor.to_string(),
            version: version.to_string(),
            identifier: identifier.to_string(),
            role: role_from_text(&subcategories),
            path: path.to_path_buf(),
            format: Format::Vst3,
        });
    }
    (!found.is_empty()).then_some(found)
}

fn vst3_plugins(path: &Path, failed: &mut Vec<PathBuf>) -> Vec<FoundPlugin> {
    if let Some(found) = moduleinfo_plugins(path) {
        return found;
    }
    if scanner_path().is_some() && has_vst3_binary(path) {
        match scan_vst3_isolated(path) {
            Ok(classes) if !classes.is_empty() => {
                return found_from_classes(path, classes);
            }
            Ok(_) => {}
            Err(()) => {
                failed.push(path.to_path_buf());
                return Vec::new();
            }
        }
    }
    vec![fallback(path, Format::Vst3)]
}

/// Convenience over [`default_roots`].
#[must_use]
pub fn scan_installed() -> Vec<FoundPlugin> {
    scan_installed_report(&[]).plugins
}

/// Scans installed plug-ins, skipping blocked paths and reporting failures.
#[must_use]
pub fn scan_installed_report(blocked: &[PathBuf]) -> ScanReport {
    scan_roots_filtered(&default_roots(), blocked)
}

/// Default cache file for the plugin manager.
pub const CATALOG_FILE: &str = "lazy-arranger-plugins.json";

/// Writes the scanned catalog as JSON.
///
/// # Errors
///
/// Returns a message when the file cannot be serialized or written.
pub fn save_catalog(path: &Path, plugins: &[FoundPlugin]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(plugins).map_err(|error| error.to_string())?;
    std::fs::write(path, json).map_err(|error| format!("{}: {error}", path.display()))
}

/// Reads a cached catalog, if the file exists and parses.
#[must_use]
pub fn load_catalog(path: &Path) -> Option<Vec<FoundPlugin>> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Adds a folder for a format to a scan list; `format` is the extension to look
/// for.
#[must_use]
pub fn root(format: Format, path: impl Into<PathBuf>) -> ScanRoot {
    ScanRoot {
        format,
        path: path.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_metadata_supplies_names_and_roles() {
        let directory = std::env::temp_dir().join("lazy-arranger-plugin-meta");
        let _ = std::fs::remove_dir_all(&directory);
        let au = directory.join("Test.component").join("Contents");
        std::fs::create_dir_all(&au).expect("au");
        std::fs::write(
            au.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
            <plist version="1.0"><dict>
              <key>CFBundleName</key><string>Test AU</string>
              <key>CFBundleIdentifier</key><string>com.example.test</string>
              <key>CFBundleShortVersionString</key><string>2.1</string>
              <key>AudioComponents</key><array><dict>
                <key>name</key><string>Example: Test AU</string>
                <key>manufacturer</key><string>Exmp</string>
                <key>subtype</key><string>tst1</string>
                <key>type</key><string>aufx</string>
              </dict></array>
            </dict></plist>"#,
        )
        .expect("plist");
        let vst = directory
            .join("Test.vst3")
            .join("Contents")
            .join("Resources");
        std::fs::create_dir_all(&vst).expect("vst");
        std::fs::write(
            vst.join("moduleinfo.json"),
            r#"{"Classes":[{"Category":"Audio Module Class","Name":"Test Synth",
                "Vendor":"Example","Version":"1.2","Sub Categories":["Instrument","Synth"],
                "CID":"0123456789ABCDEF0123456789ABCDEF"}]}"#,
        )
        .expect("moduleinfo");
        let found = scan_roots(&[
            root(Format::AudioUnit, &directory),
            root(Format::Vst3, &directory),
        ]);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "Example: Test AU");
        assert_eq!(found[0].vendor, "Exmp");
        assert_eq!(found[0].role, Role::Effect);
        assert_eq!(found[1].name, "Test Synth");
        assert_eq!(found[1].vendor, "Example");
        assert_eq!(found[1].role, Role::Instrument);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn blocked_paths_are_skipped_and_blocklist_round_trips() {
        let directory = std::env::temp_dir().join("lazy-arranger-plugin-block");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("Keep.component")).expect("keep");
        std::fs::create_dir_all(directory.join("Skip.component")).expect("skip");
        let blocked = vec![directory.join("Skip.component")];
        let file = directory.join("blocklist.json");
        save_blocklist(&file, &blocked).expect("save");
        assert_eq!(load_blocklist(&file), blocked);
        let report = scan_roots_filtered(&[root(Format::AudioUnit, &directory)], &blocked);
        assert_eq!(report.plugins.len(), 1);
        assert_eq!(report.plugins[0].name, "Keep");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn scanner_json_shape_parses() {
        let text = br#"[{"cid":"0123456789ABCDEF0123456789ABCDEF",
            "name":"Synth","vendor":"Example","version":"1.0",
            "subcategories":"Instrument|Synth"}]"#;
        let classes: Vec<ScannedClass> = serde_json::from_slice(text).expect("json");
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, "Synth");
        assert_eq!(role_from_text(&classes[0].subcategories), Role::Instrument);
    }

    #[test]
    fn a_catalog_round_trips_through_json() {
        let path = std::env::temp_dir().join("lazy-arranger-plugins-test.json");
        let plugin = FoundPlugin {
            name: String::from("Test"),
            vendor: String::from("Vendor"),
            version: String::from("1.0"),
            identifier: String::from("Vndr:tst1"),
            role: Role::Effect,
            path: PathBuf::from("/tmp/Test.component"),
            format: Format::AudioUnit,
        };
        save_catalog(&path, std::slice::from_ref(&plugin)).expect("save");
        assert_eq!(load_catalog(&path), Some(vec![plugin]));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn bundles_of_the_expected_format_are_found_sorted() {
        let directory = std::env::temp_dir().join("lazy-arranger-plugin-scan");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("Zeta.vst3")).expect("vst3");
        std::fs::create_dir_all(directory.join("Alpha.component")).expect("au");
        std::fs::write(directory.join("notes.txt"), b"x").expect("file");
        let found = scan_roots(&[
            root(Format::Vst3, &directory),
            root(Format::AudioUnit, &directory),
        ]);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "Alpha");
        assert_eq!(found[0].format, Format::AudioUnit);
        assert_eq!(found[1].name, "Zeta");
        assert_eq!(found[1].format, Format::Vst3);
        let missing = scan_roots(&[root(Format::Vst3, directory.join("absent"))]);
        assert!(missing.is_empty());
        let _ = std::fs::remove_dir_all(&directory);
    }
}
