//! Importing a style's decoded tracks into the current project.

use std::path::Path;

use app_commands::{Command, EditSession};
use project::song::Track;

/// What an import added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Imported {
    /// Tracks inserted.
    pub tracks: usize,
    /// Notes across them.
    pub notes: usize,
}

/// Copies tracks into the session with fresh ids and a name prefix.
///
/// # Errors
///
/// Returns a message when a track cannot be inserted.
pub fn copy_tracks(
    session: &mut EditSession,
    source: &[Track],
    prefix: &str,
) -> Result<Imported, String> {
    let mut imported = Imported {
        tracks: 0,
        notes: 0,
    };
    for track in source {
        let id = session.next_track_id().map_err(|error| error.to_string())?;
        let mut copy = track.clone();
        copy.id = id;
        copy.name = format!("{prefix} {}", track.name);
        imported.notes += copy.notes.len();
        session
            .apply(Command::InsertTrack { track: copy })
            .map_err(|error| error.to_string())?;
        imported.tracks += 1;
    }
    Ok(imported)
}

/// Imports a style file's decoded song tracks.
///
/// # Errors
///
/// Returns a message when the file cannot be read or parsed.
pub fn import_file(
    session: &mut EditSession,
    path: &Path,
    prefix: &str,
) -> Result<Imported, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let imported =
        style_formats::import::import_style(&bytes).map_err(|error| error.to_string())?;
    copy_tracks(session, &imported.song.tracks, prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, ProjectFile, Song};

    fn session() -> EditSession {
        let mut song = Song::new("Import", 120.0);
        song.tracks
            .push(Track::new(TrackId::new(1).expect("valid"), "Piano"));
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    fn source() -> Vec<Track> {
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Bass");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C2").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        vec![track]
    }

    #[test]
    fn copying_tracks_renames_and_keeps_original() {
        let mut session = session();
        let imported = copy_tracks(&mut session, &source(), "Style").expect("copy");
        assert_eq!(imported.tracks, 1);
        assert_eq!(imported.notes, 1);
        assert_eq!(session.song().tracks.len(), 2);
        assert_eq!(session.song().tracks[1].name, "Style Bass");
        assert_ne!(session.song().tracks[1].id, TrackId::new(1).expect("valid"));
        session.undo().expect("undo");
        assert_eq!(session.song().tracks.len(), 1);
    }

    #[test]
    fn a_missing_style_file_is_an_error() {
        let mut session = session();
        assert!(
            import_file(
                &mut session,
                Path::new("/tmp/no-such-style-lazy.sty"),
                "Style"
            )
            .is_err()
        );
    }
}
