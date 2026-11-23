//! Bouncing the mix into a new audio track.

use std::path::{Path, PathBuf};

use music_core::Tick;
use project::song::{AudioRef, Track};

use crate::{Command, CommandError, EditSession};

/// What a bounce produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bounce {
    /// New track.
    pub track: project::TrackId,
    /// Written WAV.
    pub path: PathBuf,
    /// Reference length in ticks.
    pub length: Tick,
}

/// Renders the song, writes it as a WAV and inserts it as an audio track.
///
/// The new track plays the file, so bouncing again without muting it doubles
/// the mix.
///
/// # Errors
///
/// Returns a rejection when the file cannot be written or the track inserted.
pub fn bounce_to_track(
    session: &mut EditSession,
    path: &Path,
    name: &str,
    sample_rate: u32,
) -> Result<Bounce, CommandError> {
    let mut rendered = audio::render(session.song(), sample_rate);
    let _ = audio::mix::mix_audio_references(&mut rendered, session.song(), 0.9);
    std::fs::write(path, rendered.to_wav())
        .map_err(|error| CommandError::Validation(format!("{}: {error}", path.display())))?;
    let bpm = session.song().tempo_map.bpm_at(0);
    let length = audio::chords::to_ticks(rendered.duration_seconds(), bpm);
    let track_id = session.next_track_id()?;
    let mut track = Track::new(track_id, name.to_string());
    track.audio = Some(AudioRef {
        path: path.display().to_string(),
        start: 0,
        length,
        offset: 0,
    });
    session.apply(Command::InsertTrack { track })?;
    Ok(Bounce {
        track: track_id,
        path: path.to_path_buf(),
        length,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, ProjectFile, Song};

    fn session() -> EditSession {
        let mut song = Song::new("Bounce", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn bouncing_writes_a_wav_and_adds_an_audio_track() {
        let mut session = session();
        let path = std::env::temp_dir().join("lazy-arranger-bounce-test.wav");
        let bounce = bounce_to_track(&mut session, &path, "Bounce", 48_000).expect("bounce");
        assert!(path.is_file());
        assert!(bounce.length > 0);
        let bytes = std::fs::read(&path).expect("read");
        let decoded = audio::wav::decode_mono(&bytes).expect("decode");
        assert!(decoded.samples.iter().any(|sample| sample.abs() > 0.001));
        let track = session
            .song()
            .tracks
            .iter()
            .find(|track| track.id == bounce.track)
            .expect("bounce track");
        assert_eq!(
            track.audio.as_ref().expect("reference").length,
            bounce.length
        );
        session.undo().expect("undo insert");
        assert_eq!(session.song().tracks.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_bad_destination_is_rejected() {
        let mut session = session();
        let path = std::path::Path::new("/no-such-folder-lazy/bounce.wav");
        assert!(bounce_to_track(&mut session, path, "Bounce", 48_000).is_err());
    }
}
