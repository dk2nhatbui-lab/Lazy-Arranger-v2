//! Which VST3 bundles may be loaded to make sound.
//!
//! A project file names plug-ins by path, so opening a file must not be
//! enough to run a native binary from anywhere on disk. A bundle is trusted
//! when it sits in a standard VST3 folder, or when the user approved it in
//! this process (the desktop app restores its saved approvals at startup).
//! Scanning is not gated: the user picks the folders to scan.

use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

static APPROVED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// The standard VST3 folders for this platform.
#[must_use]
pub fn default_vst3_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Library/Audio/Plug-Ins/VST3"));
        }
    } else if cfg!(target_os = "windows") {
        roots.push(PathBuf::from(r"C:\Program Files\Common Files\VST3"));
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Common Files\VST3"));
    } else {
        roots.push(PathBuf::from("/usr/lib/vst3"));
        roots.push(PathBuf::from("/usr/local/lib/vst3"));
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join(".vst3"));
        }
    }
    roots
}

/// An absolute path without `.` or `..` parts, as a comparable key
/// (lowercase on Windows, where paths ignore case).
fn key(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => return None,
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    if cfg!(target_os = "windows") {
        Some(PathBuf::from(out.to_string_lossy().to_lowercase()))
    } else {
        Some(out)
    }
}

/// Approves one bundle for this process, after the user said yes to it.
pub fn approve_vst3_bundle(path: &Path) {
    let Some(key) = key(path) else { return };
    if let Ok(mut approved) = APPROVED.lock()
        && !approved.contains(&key)
    {
        approved.push(key);
    }
}

/// True when the bundle sits in a standard VST3 folder or was approved.
#[must_use]
pub fn is_vst3_bundle_trusted(path: &Path) -> bool {
    let Some(bundle) = key(path) else {
        return false;
    };
    let in_root = default_vst3_roots()
        .iter()
        .filter_map(|root| key(root))
        .any(|root| bundle.starts_with(&root) && bundle != root);
    in_root
        || APPROVED
            .lock()
            .is_ok_and(|approved| approved.contains(&bundle))
}

/// Refuses an untrusted bundle with a message that names it.
pub(crate) fn check(path: &Path) -> Result<(), String> {
    if is_vst3_bundle_trusted(path) {
        Ok(())
    } else {
        Err(format!(
            "plug-in {} is outside the VST3 folders and was not approved",
            path.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_standard_folders_and_approved_bundles_are_trusted() {
        let root = default_vst3_roots().remove(0);
        assert!(is_vst3_bundle_trusted(&root.join("Gain.vst3")));
        assert!(!is_vst3_bundle_trusted(&root));
        assert!(!is_vst3_bundle_trusted(&root.join("../evil.vst3")));
        assert!(!is_vst3_bundle_trusted(Path::new("relative/Gain.vst3")));
        let elsewhere = std::env::temp_dir().join("lazy-arranger-trust-test.vst3");
        assert!(!is_vst3_bundle_trusted(&elsewhere));
        approve_vst3_bundle(&elsewhere);
        assert!(is_vst3_bundle_trusted(&elsewhere));
    }
}
