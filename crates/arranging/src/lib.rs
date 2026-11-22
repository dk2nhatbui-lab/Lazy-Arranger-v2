//! Rule-based arranging help.
//!
//! The Arrangement Doctor reads the song and proposes targeted changes: thin a
//! crowded bar, or vary a bar that repeats the one before it. It never touches
//! the melody: the highest-sounding track is excluded from every proposal.
//! Proposals are data; applying one replaces track notes through
//! `app-commands`, so each proposal is one undo step per affected track.

use std::collections::BTreeMap;

use app_commands::{Command, CommandError, EditSession};
use music_core::{Tick, Velocity};
use project::TrackId;
use project::song::{Note, Song};
use thiserror::Error;

/// A velocity change for one note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VelocityEdit {
    /// Track holding the note.
    pub track: TrackId,
    /// Note to change.
    pub note: project::NoteId,
    /// New velocity.
    pub velocity: u8,
}

/// One rule finding with the changes it proposes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    /// Short title for a card.
    pub title: String,
    /// Why the rule fired.
    pub detail: String,
    /// Bar where it applies, from zero.
    pub bar: usize,
    /// The changes.
    pub edits: Vec<VelocityEdit>,
}

impl Proposal {
    /// True when there is nothing to change.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

/// Errors from applying a proposal.
#[derive(Debug, Error)]
pub enum ArrangingError {
    /// A command was rejected.
    #[error("edit failed: {0}")]
    Command(#[from] CommandError),
}

/// Analyzes a song and returns proposals, in bar order.
#[must_use]
pub fn analyze(song: &Song) -> Vec<Proposal> {
    let bar = song.time_signature.bar_ticks().max(1);
    let end = song.end_tick().max(bar);
    let bars = usize::try_from((end + bar - 1).div_euclid(bar).max(1)).unwrap_or(1);
    let melody = song
        .tracks
        .iter()
        .enumerate()
        .max_by_key(|(_, track)| {
            track
                .notes
                .iter()
                .map(|note| note.pitch.as_u8())
                .max()
                .unwrap_or(0)
        })
        .map(|(index, _)| index);

    let mut proposals = Vec::new();
    let mut previous_signature: Option<Vec<(Tick, Tick, u8)>> = None;
    for bar_index in 0..bars {
        let start = (bar_index as Tick) * bar;
        let stop = start + bar;
        let mut signature = Vec::new();
        let mut bar_notes: Vec<(usize, TrackId, Note)> = Vec::new();
        for (track_index, track) in song.tracks.iter().enumerate() {
            for note in &track.notes {
                if note.start < start || note.start >= stop {
                    continue;
                }
                signature.push((note.start - start, note.duration, note.pitch.as_u8()));
                bar_notes.push((track_index, track.id, note.clone()));
            }
        }
        let sounding = bar_notes.len();
        if sounding > 11 {
            let edits: Vec<VelocityEdit> = bar_notes
                .iter()
                .filter(|(track_index, _, note)| {
                    Some(*track_index) != melody && note.velocity.as_u8() > 1
                })
                .map(|(_, track, note)| VelocityEdit {
                    track: *track,
                    note: note.id,
                    velocity: (u16::from(note.velocity.as_u8()) * 4 / 5).max(1) as u8,
                })
                .collect();
            if !edits.is_empty() {
                proposals.push(Proposal {
                    title: format!("Crowded bar {}", bar_index + 1),
                    detail: format!(
                        "{sounding} notes start in this bar; the accompaniment can sit back."
                    ),
                    bar: bar_index,
                    edits,
                });
            }
        }
        signature.sort_unstable();
        if let Some(previous) = &previous_signature
            && !signature.is_empty()
            && *previous == signature
        {
            let mut lifts = Vec::new();
            for track in &song.tracks {
                for note in &track.notes {
                    if note.start >= start && note.start < stop && note.velocity.as_u8() < 127 {
                        lifts.push(VelocityEdit {
                            track: track.id,
                            note: note.id,
                            velocity: note.velocity.as_u8().saturating_add(6),
                        });
                    }
                }
            }
            proposals.push(Proposal {
                title: format!("Repeat of bar {}", bar_index),
                detail: String::from(
                    "This bar repeats the one before it; a small lift marks the repeat.",
                ),
                bar: bar_index,
                edits: lifts,
            });
        }
        previous_signature = Some(signature);
    }
    proposals
}

/// Applies one proposal, one command per track.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when an edit is rejected.
pub fn apply(session: &mut EditSession, proposal: &Proposal) -> Result<usize, ArrangingError> {
    let mut by_track: BTreeMap<TrackId, Vec<&VelocityEdit>> = BTreeMap::new();
    for edit in &proposal.edits {
        by_track.entry(edit.track).or_default().push(edit);
    }
    let mut changed = 0;
    for (track_id, edits) in by_track {
        let Some(track) = session
            .song()
            .tracks
            .iter()
            .find(|track| track.id == track_id)
        else {
            continue;
        };
        let mut notes: Vec<Note> = track.notes.clone();
        for edit in &edits {
            if let Some(note) = notes.iter_mut().find(|note| note.id == edit.note) {
                note.velocity = Velocity::new(edit.velocity).unwrap_or(note.velocity);
                changed += 1;
            }
        }
        session.apply(Command::ReplaceTrackNotes {
            track: track_id,
            notes,
        })?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch};
    use project::song::Track;

    fn note(id: u32, start: Tick, pitch: &str, velocity: u8) -> Note {
        Note {
            id: project::NoteId::new(id).expect("valid"),
            start,
            duration: PPQ,
            pitch: Pitch::from_name(pitch).expect("valid"),
            velocity: Velocity::new(velocity).expect("valid"),
            channel: Channel::first(),
        }
    }

    fn song_with_band(band: Vec<Note>) -> Song {
        let mut song = Song::new("Doctor", 120.0);
        let mut lead = Track::new(TrackId::new(1).expect("valid"), "Lead");
        lead.notes.push(note(100, 0, "C6", 110));
        let mut track = Track::new(TrackId::new(2).expect("valid"), "Band");
        track.notes = band;
        song.tracks.push(lead);
        song.tracks.push(track);
        song
    }

    fn song_with(notes: Vec<Note>) -> Song {
        let mut song = Song::new("Doctor", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Band");
        track.notes = notes;
        song.tracks.push(track);
        song
    }

    #[test]
    fn a_crowded_bar_proposes_a_quieter_accompaniment() {
        let notes: Vec<Note> = (0..14)
            .map(|index| {
                note(
                    index + 1,
                    0,
                    ["C3", "E3", "G3", "B3", "D4"][index as usize % 5],
                    100,
                )
            })
            .collect();
        let proposals = analyze(&song_with_band(notes));
        let crowded = proposals
            .iter()
            .find(|proposal| proposal.title.contains("Crowded"))
            .expect("crowded proposal");
        assert_eq!(crowded.bar, 0);
        assert!(!crowded.is_empty());
        assert!(crowded.edits.iter().all(|edit| edit.velocity < 100));
    }

    #[test]
    fn a_repeated_bar_proposes_a_lift() {
        let mut notes = Vec::new();
        for bar in 0..2_u32 {
            notes.push(note(bar * 2 + 1, i64::from(bar) * 3840, "C4", 90));
            notes.push(note(bar * 2 + 2, i64::from(bar) * 3840 + 960, "E4", 90));
        }
        let proposals = analyze(&song_with(notes));
        let repeat = proposals
            .iter()
            .find(|proposal| proposal.title.contains("Repeat"))
            .expect("repeat proposal");
        assert_eq!(repeat.bar, 1);
        assert!(repeat.edits.iter().all(|edit| edit.velocity == 96));
    }

    #[test]
    fn a_unique_sparse_song_has_no_proposals() {
        let notes = vec![note(1, 0, "C4", 90), note(2, 960, "E4", 90)];
        assert!(analyze(&song_with(notes)).is_empty());
    }

    #[test]
    fn applying_a_proposal_is_one_undo_step_per_track() {
        let notes: Vec<Note> = (0..12).map(|index| note(index + 1, 0, "C3", 100)).collect();
        let song = song_with_band(notes);
        let proposals = analyze(&song);
        let proposal = proposals.first().expect("proposal");
        let mut session = EditSession::new(project::song::ProjectFile::new(song)).expect("valid");
        let changed = apply(&mut session, proposal).expect("apply");
        assert!(changed > 0);
        assert!(
            session.song().tracks[1]
                .notes
                .iter()
                .all(|note| note.velocity.as_u8() < 100)
        );
        session.undo().expect("undo");
        assert!(
            session.song().tracks[1]
                .notes
                .iter()
                .all(|note| note.velocity.as_u8() == 100)
        );
    }
}
