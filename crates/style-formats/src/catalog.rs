//! A numbered catalog of style files under a folder.
//!
//! The catalog only lists files by extension and assigns stable numbers by
//! sorted path; it never parses a style. Importing a chosen number is the
//! caller's job, so a big library lists instantly.

use std::path::{Path, PathBuf};

/// Extensions treated as styles.
pub const EXTENSIONS: [&str; 8] = ["sty", "prs", "sst", "pst", "bcs", "pcs", "scp", "fps"];

/// One catalogued style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    /// Style number, starting at one, in sorted order.
    pub number: usize,
    /// Display name: the file stem.
    pub name: String,
    /// Full path.
    pub path: PathBuf,
}

/// Lists every style under `root`, sorted by path, numbered from one.
///
/// # Errors
///
/// Returns a message when `root` cannot be read.
pub fn catalog(root: &Path) -> Result<Vec<CatalogEntry>, String> {
    let mut paths = Vec::new();
    walk(root, &mut paths)?;
    paths.sort();
    Ok(paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| CatalogEntry {
            number: index + 1,
            name: path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("style")
                .to_string(),
            path,
        })
        .collect())
}

/// One entry by number.
#[must_use]
pub fn by_number(entries: &[CatalogEntry], number: usize) -> Option<&CatalogEntry> {
    entries.iter().find(|entry| entry.number == number)
}

/// Resolves a style by 1-based number, exact name, then substring.
#[must_use]
pub fn resolve<'a>(
    entries: &'a [CatalogEntry],
    number: Option<usize>,
    name: &str,
) -> Option<&'a CatalogEntry> {
    if let Some(number) = number
        && let Some(entry) = by_number(entries, number)
    {
        return Some(entry);
    }
    let needle = name.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }
    entries
        .iter()
        .find(|entry| entry.name.to_ascii_lowercase() == needle)
        .or_else(|| {
            entries
                .iter()
                .find(|entry| entry.name.to_ascii_lowercase().contains(&needle))
        })
}

fn walk(directory: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", directory.display()))?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if is_style(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn is_style(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("nested")).expect("dir");
        path
    }

    #[test]
    fn styles_are_numbered_ignoring_other_files() {
        let root = temp_dir("lazy-arranger-catalog-test");
        fs::write(root.join("b.sty"), b"x").expect("write");
        fs::write(root.join("a.prs"), b"x").expect("write");
        fs::write(root.join("notes.txt"), b"x").expect("write");
        fs::write(root.join("nested").join("c.STY"), b"x").expect("write");
        let entries = catalog(&root).expect("catalog");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].number, 1);
        assert_eq!(entries[0].name, "a");
        assert_eq!(by_number(&entries, 2).expect("two").name, "b");
        assert_eq!(by_number(&entries, 3).expect("three").name, "c");
        assert!(by_number(&entries, 4).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn styles_resolve_by_number_then_name() {
        let entries = vec![
            CatalogEntry {
                number: 1,
                name: String::from("Ballad"),
                path: PathBuf::from("/a.sty"),
            },
            CatalogEntry {
                number: 2,
                name: String::from("Funk Shuffle"),
                path: PathBuf::from("/b.sty"),
            },
        ];
        assert_eq!(resolve(&entries, Some(2), "").expect("number").number, 2);
        assert_eq!(
            resolve(&entries, None, "funk shuffle")
                .expect("exact")
                .number,
            2
        );
        assert_eq!(resolve(&entries, None, "BALLAD").expect("case").number, 1);
        assert_eq!(
            resolve(&entries, None, "shuffle")
                .expect("substring")
                .number,
            2
        );
        assert!(resolve(&entries, Some(9), "").is_none());
        assert!(resolve(&entries, None, "nothing").is_none());
        assert!(resolve(&entries, None, "").is_none());
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        assert!(catalog(Path::new("/tmp/lazy-arranger-no-such-folder")).is_err());
    }
}
