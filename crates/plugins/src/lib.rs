//! Discovery of installed VST3 and Audio Unit bundles.
//!
//! This reads the standard plug-in folders and reports each bundle's name, path
//! and format. It does not load, validate or host a plug-in.

use std::path::PathBuf;

/// Plug-in format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

/// One bundle found on disk.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FoundPlugin {
    /// Display name, from the bundle stem.
    pub name: String,
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

/// Scans roots for bundles of the expected extension, sorted by path.
///
/// Missing roots are skipped. A directory counts as a bundle only when it has
/// the expected extension; this does not prove that the plug-in loads.
#[must_use]
pub fn scan_roots(roots: &[ScanRoot]) -> Vec<FoundPlugin> {
    let mut found = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root.path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let extension = path.extension().and_then(|value| value.to_str());
            if extension
                .is_some_and(|value| value.eq_ignore_ascii_case(bundle_extension(root.format)))
            {
                let name = path.file_stem().map_or_else(
                    || String::from("unknown"),
                    |stem| stem.to_string_lossy().into_owned(),
                );
                found.push(FoundPlugin {
                    name,
                    path,
                    format: root.format,
                });
            }
        }
    }
    found.sort_by(|left, right| left.path.cmp(&right.path));
    found
}

/// Convenience over [`default_roots`].
#[must_use]
pub fn scan_installed() -> Vec<FoundPlugin> {
    scan_roots(&default_roots())
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
