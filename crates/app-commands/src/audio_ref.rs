//! Trimming audio references.
//!
//! Trimming moves the timeline start and the in-point together, or changes the
//! length, always through `SetAudioRef` so undo is one step.

use music_core::Tick;
use project::TrackId;

use crate::{Command, CommandError, EditSession};

/// Trims the front of an audio reference by `delta` ticks.
///
/// # Errors
///
/// Returns a rejection when the track has no audio, the delta is not positive
/// or it would consume the whole reference.
pub fn trim_start(
    session: &mut EditSession,
    track: TrackId,
    delta: Tick,
) -> Result<(), CommandError> {
    let Some(mut reference) = reference(session, track)? else {
        return Err(CommandError::Validation(format!(
            "track {track} has no audio reference"
        )));
    };
    if delta <= 0 || delta >= reference.length {
        return Err(CommandError::Validation(format!(
            "trim {delta} is outside the {}-tick reference",
            reference.length
        )));
    }
    reference.start = reference.start.saturating_add(delta);
    reference.offset = reference.offset.saturating_add(delta);
    reference.length -= delta;
    session
        .apply(Command::SetAudioRef {
            track,
            reference: Some(reference),
        })
        .map(|_| ())
}

/// Sets a reference's timeline length.
///
/// # Errors
///
/// Returns a rejection for a non-positive length or a missing reference.
pub fn set_length(
    session: &mut EditSession,
    track: TrackId,
    length: Tick,
) -> Result<(), CommandError> {
    let Some(mut reference) = reference(session, track)? else {
        return Err(CommandError::Validation(format!(
            "track {track} has no audio reference"
        )));
    };
    if length <= 0 {
        return Err(CommandError::Validation(String::from(
            "audio length must be positive",
        )));
    }
    reference.length = length;
    session
        .apply(Command::SetAudioRef {
            track,
            reference: Some(reference),
        })
        .map(|_| ())
}

fn reference(
    session: &EditSession,
    track: TrackId,
) -> Result<Option<project::song::AudioRef>, CommandError> {
    let found = session
        .song()
        .tracks
        .iter()
        .find(|entry| entry.id == track)
        .ok_or(CommandError::UnknownTrack(track))?;
    Ok(found.audio.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::TrackId;
    use project::song::{AudioRef, ProjectFile, Song, Track};

    fn session() -> EditSession {
        let mut song = Song::new("Audio", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Take");
        track.audio = Some(AudioRef {
            path: String::from("/tmp/take.wav"),
            start: 0,
            length: 3840,
            offset: 0,
        });
        song.tracks.push(track);
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn trimming_the_front_moves_start_and_in_point() {
        let mut session = session();
        let track = TrackId::new(1).expect("valid");
        trim_start(&mut session, track, 960).expect("trim");
        let reference = session.song().tracks[0].audio.clone().expect("audio");
        assert_eq!(
            (reference.start, reference.offset, reference.length),
            (960, 960, 2880)
        );
        session.undo().expect("undo");
        let reference = session.song().tracks[0].audio.clone().expect("audio");
        assert_eq!(
            (reference.start, reference.offset, reference.length),
            (0, 0, 3840)
        );
        assert!(trim_start(&mut session, track, 0).is_err());
        assert!(trim_start(&mut session, track, 3840).is_err());
    }

    #[test]
    fn a_new_length_must_be_positive() {
        let mut session = session();
        let track = TrackId::new(1).expect("valid");
        set_length(&mut session, track, 1920).expect("length");
        assert_eq!(
            session.song().tracks[0]
                .audio
                .as_ref()
                .expect("audio")
                .length,
            1920
        );
        assert!(set_length(&mut session, track, 0).is_err());
    }
}
