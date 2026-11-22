//! Autosave and recovery, independent of egui so it can be tested.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Suffix appended to the project path.
pub const AUTOSAVE_SUFFIX: &str = ".autosave";

/// Autosave path for a project.
#[must_use]
pub fn autosave_path(project: &Path) -> PathBuf {
    let mut path = project.as_os_str().to_owned();
    path.push(AUTOSAVE_SUFFIX);
    PathBuf::from(path)
}

/// Writes JSON to `path` through a temporary file and a rename.
///
/// # Errors
///
/// Returns a message when the write or rename fails.
pub fn save_atomic(path: &Path, json: &str) -> Result<(), String> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, json).map_err(|error| format!("{}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| format!("{}: {error}", path.display()))
}

/// True when an autosave exists and is newer than its project.
#[must_use]
pub fn recovery_available(project: &Path) -> bool {
    let autosave = autosave_path(project);
    let Ok(autosave_time) = fs::metadata(&autosave).and_then(|meta| meta.modified()) else {
        return false;
    };
    match fs::metadata(project).and_then(|meta| meta.modified()) {
        Ok(project_time) => autosave_time > project_time,
        Err(_) => true,
    }
}

/// Rate limiter for autosave writes.
#[derive(Debug)]
pub struct Autosave {
    interval: Duration,
    last: Instant,
}

impl Autosave {
    /// A limiter that allows one write per `interval`.
    #[must_use]
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: Instant::now(),
        }
    }

    /// Writes the autosave when enough time has passed and the project is dirty.
    ///
    /// # Errors
    ///
    /// Returns a message when the file cannot be written.
    pub fn maybe_save(
        &mut self,
        project: Option<&Path>,
        dirty: bool,
        json: &str,
    ) -> Result<Option<PathBuf>, String> {
        let Some(project) = project else {
            return Ok(None);
        };
        if !dirty || self.last.elapsed() < self.interval {
            return Ok(None);
        }
        let path = autosave_path(project);
        save_atomic(&path, json)?;
        self.last = Instant::now();
        Ok(Some(path))
    }
}

/// Modification time, for tests and diagnostics.
#[must_use]
pub fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("lazy-arranger-{name}.json"))
    }

    #[test]
    fn autosave_path_is_the_project_path_plus_suffix() {
        assert_eq!(
            autosave_path(Path::new("/tmp/song.json")),
            PathBuf::from("/tmp/song.json.autosave")
        );
    }

    #[test]
    fn atomic_save_replaces_the_file_and_leaves_no_temp() {
        let path = temp("atomic");
        save_atomic(&path, "{\"a\":1}").expect("write");
        assert_eq!(fs::read_to_string(&path).expect("read"), "{\"a\":1}");
        save_atomic(&path, "{\"a\":2}").expect("rewrite");
        assert_eq!(fs::read_to_string(&path).expect("read"), "{\"a\":2}");
        assert!(!path.with_extension("tmp").exists());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn recovery_is_only_offered_when_the_autosave_is_newer() {
        let project = temp("recovery");
        save_atomic(&project, "{}").expect("project");
        assert!(!recovery_available(&project), "no autosave yet");
        let autosave = autosave_path(&project);
        sleep(Duration::from_millis(10));
        save_atomic(&autosave, "{}").expect("autosave");
        assert!(recovery_available(&project));
        sleep(Duration::from_millis(10));
        save_atomic(&project, "{}").expect("newer project");
        assert!(!recovery_available(&project));
        let _ = fs::remove_file(&project);
        let _ = fs::remove_file(&autosave);
    }

    #[test]
    fn the_limiter_respects_interval_and_dirty_flag() {
        let project = temp("limiter");
        save_atomic(&project, "{}").expect("project");
        let mut limiter = Autosave::new(Duration::from_millis(50));
        assert!(
            limiter
                .maybe_save(Some(&project), false, "{}")
                .expect("clean")
                .is_none()
        );
        assert!(
            limiter
                .maybe_save(None, true, "{}")
                .expect("no path")
                .is_none()
        );
        sleep(Duration::from_millis(60));
        assert!(
            limiter
                .maybe_save(Some(&project), true, "{\"x\":1}")
                .expect("write")
                .is_some()
        );
        assert!(
            limiter
                .maybe_save(Some(&project), true, "{\"x\":2}")
                .expect("too soon")
                .is_none()
        );
        assert_eq!(
            fs::read_to_string(autosave_path(&project)).expect("autosave"),
            "{\"x\":1}"
        );
        let _ = fs::remove_file(&project);
        let _ = fs::remove_file(autosave_path(&project));
    }
}
