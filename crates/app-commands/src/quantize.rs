//! Quantising note starts.

use music_core::Tick;

use crate::{Command, EditSession};

/// Snaps every note start in every track to `grid`, returning how many tracks
/// changed. One edit per changed track.
///
/// # Errors
///
/// Returns a message when a track edit is rejected.
pub fn quantize(session: &mut EditSession, grid: Tick) -> Result<usize, String> {
    let grid = grid.max(1);
    let tracks: Vec<(project::TrackId, Vec<project::song::Note>)> = session
        .song()
        .tracks
        .iter()
        .map(|track| {
            let mut notes = track.notes.clone();
            for note in &mut notes {
                note.start = (note.start + grid / 2) / grid * grid;
            }
            notes.sort_by_key(|note| note.start);
            (track.id, notes)
        })
        .collect();
    let mut changed = 0;
    for (track, notes) in tracks {
        let current = session
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| entry.notes.clone())
            .unwrap_or_default();
        if notes != current {
            session
                .apply(Command::ReplaceTrackNotes { track, notes })
                .map_err(|error| error.to_string())?;
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, ProjectFile, Song, Track};

    fn session() -> EditSession {
        let mut song = Song::new("Quantise", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 130,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn starts_snap_to_the_grid_and_undo() {
        let mut session = session();
        assert_eq!(quantize(&mut session, PPQ / 4).expect("quantise"), 1);
        assert_eq!(session.song().tracks[0].notes[0].start, PPQ / 4);
        assert_eq!(
            quantize(&mut session, PPQ / 4).expect("again"),
            0,
            "idempotent"
        );
        session.undo().expect("undo");
        assert_eq!(session.song().tracks[0].notes[0].start, 130);
    }
}
