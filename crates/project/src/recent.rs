//! The recent-projects list.
//!
//! A plain text file with one path per line, newest first. Missing files are
//! dropped when the list is read or written, so a moved project never shows as
//! openable. The list is capped, and recording a path moves it to the front.

use std::fs;
use std::path::{Path, PathBuf};

/// Default list length, matching the C++ app.
pub const LIMIT: usize = 10;

/// Reads the list, dropping paths that no longer exist.
#[must_use]
pub fn read(path: &Path) -> Vec<PathBuf> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .filter(|entry| entry.is_file())
        .take(LIMIT)
        .collect()
}

/// Adds a project to the front of the list and writes it back.
///
/// # Errors
///
/// Returns a message when the list cannot be written.
pub fn record(path: &Path, project: &Path) -> Result<Vec<PathBuf>, String> {
    let mut entries = read(path);
    let project = project.to_path_buf();
    entries.retain(|entry| entry != &project);
    if project.is_file() {
        entries.insert(0, project);
    }
    entries.truncate(LIMIT);
    write(path, &entries)?;
    Ok(entries)
}

/// Clears the list.
///
/// # Errors
///
/// Returns a message when the file cannot be written.
pub fn clear(path: &Path) -> Result<(), String> {
    write(path, &[])
}

fn write(path: &Path, entries: &[PathBuf]) -> Result<(), String> {
    let mut text = String::new();
    for entry in entries {
        text.push_str(&entry.display().to_string());
        text.push('\n');
    }
    fs::write(path, text).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    fn project(name: &str) -> PathBuf {
        let path = temp(name);
        fs::write(&path, "{}").expect("project file");
        path
    }

    #[test]
    fn recording_moves_a_project_to_the_front_once() {
        let list = temp("recent-list-a.txt");
        let first = project("recent-project-a.json");
        let second = project("recent-project-b.json");
        record(&list, &first).expect("first");
        record(&list, &second).expect("second");
        record(&list, &first).expect("again");
        let entries = read(&list);
        assert_eq!(entries, vec![first.clone(), second.clone()]);
        clear(&list).expect("clear");
        assert!(read(&list).is_empty());
        let _ = fs::remove_file(first);
        let _ = fs::remove_file(second);
        let _ = fs::remove_file(list);
    }

    #[test]
    fn missing_files_are_dropped() {
        let list = temp("recent-list-b.txt");
        let present = project("recent-project-c.json");
        fs::write(
            &list,
            format!("{}\n{}\n", temp("gone.json").display(), present.display()),
        )
        .expect("list");
        assert_eq!(read(&list), vec![present.clone()]);
        let _ = fs::remove_file(present);
        let _ = fs::remove_file(list);
    }

    #[test]
    fn the_list_is_capped() {
        let list = temp("recent-list-c.txt");
        let mut paths = Vec::new();
        for index in 0..(LIMIT + 3) {
            let path = project(&format!("recent-project-many-{index}.json"));
            record(&list, &path).expect("record");
            paths.push(path);
        }
        assert_eq!(read(&list).len(), LIMIT);
        for path in paths {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_file(list);
    }
}
