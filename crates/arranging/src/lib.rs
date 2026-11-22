//! Rule-based arranging help: Arrangement Doctor and Fit to My Synths.
//!
//! Analyses read a song and return [`Proposal`] values. A proposal is data:
//! applying it rewrites notes through `app-commands`, so it is one undo step
//! per affected track. The Arrangement Doctor never edits the melody (the
//! highest-sounding track); Fit to My Synths only moves, softens or removes
//! notes that break a synth's stated limits.

pub mod beat;
pub mod chords;
pub mod demo;
pub mod forms;
pub mod grid;
pub mod groove;

use std::collections::BTreeMap;

use app_commands::{Command, CommandError, EditSession};
use music_core::{Pitch, Tick, Velocity};
use project::TrackId;
use project::song::{Note, Song};
use thiserror::Error;

/// What a proposal changes on one note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    /// New velocity.
    Velocity(u8),
    /// New pitch.
    Pitch(u8),
    /// Remove the note.
    Remove,
}

/// One change to one note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteEdit {
    /// Track holding the note.
    pub track: TrackId,
    /// Note to change.
    pub note: project::NoteId,
    /// The change.
    pub kind: EditKind,
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
    pub edits: Vec<NoteEdit>,
}

impl Proposal {
    /// True when there is nothing to change.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

/// Limits a synth reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SynthSpec {
    /// Notes that may sound at once, per track.
    pub polyphony: usize,
    /// Lowest playable note.
    pub low: u8,
    /// Highest playable note.
    pub high: u8,
    /// Whether the device has a drum kit on channel 10.
    pub has_drum_kit: bool,
}

impl Default for SynthSpec {
    fn default() -> Self {
        Self {
            polyphony: 16,
            low: 21,
            high: 108,
            has_drum_kit: true,
        }
    }
}

/// Errors from applying a proposal.
#[derive(Debug, Error)]
pub enum ArrangingError {
    /// A command was rejected.
    #[error("edit failed: {0}")]
    Command(#[from] CommandError),
}

/// The Arrangement Doctor: crowded bars and literal repeats.
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
            let edits: Vec<NoteEdit> = bar_notes
                .iter()
                .filter(|(track_index, _, note)| {
                    Some(*track_index) != melody && note.velocity.as_u8() > 1
                })
                .map(|(_, track, note)| NoteEdit {
                    track: *track,
                    note: note.id,
                    kind: EditKind::Velocity(
                        (u16::from(note.velocity.as_u8()) * 4 / 5).max(1) as u8
                    ),
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
            let lifts: Vec<NoteEdit> = song
                .tracks
                .iter()
                .flat_map(|track| {
                    track.notes.iter().filter_map(|note| {
                        if note.start < start || note.start >= stop || note.velocity.as_u8() >= 127
                        {
                            return None;
                        }
                        Some(NoteEdit {
                            track: track.id,
                            note: note.id,
                            kind: EditKind::Velocity(note.velocity.as_u8().saturating_add(6)),
                        })
                    })
                })
                .collect();
            proposals.push(Proposal {
                title: format!("Repeat of bar {}", bar_index + 1),
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

/// Fit to My Synths: range and polyphony against a synth's limits.
#[must_use]
pub fn fit_to_synth(song: &Song, spec: SynthSpec) -> Vec<Proposal> {
    let bar = song.time_signature.bar_ticks().max(1);
    let mut proposals = Vec::new();
    for track in &song.tracks {
        let is_drums = track
            .notes
            .iter()
            .any(|note| note.channel.zero_based() == 9);
        if is_drums && !spec.has_drum_kit {
            proposals.push(Proposal {
                title: format!("No drum kit: {}", track.name),
                detail: String::from("The synth has no drum kit; this track will not sound right."),
                bar: 0,
                edits: Vec::new(),
            });
            continue;
        }
        if is_drums {
            continue;
        }
        let mut range_edits = Vec::new();
        for note in &track.notes {
            let pitch = note.pitch.as_u8();
            if let Some(fitted) = fit_pitch(pitch, spec.low, spec.high) {
                range_edits.push(NoteEdit {
                    track: track.id,
                    note: note.id,
                    kind: EditKind::Pitch(fitted),
                });
            }
        }
        if !range_edits.is_empty() {
            proposals.push(Proposal {
                title: format!("Range: {}", track.name),
                detail: format!(
                    "{} note(s) outside {}..{}; nearest octave moves them into range.",
                    range_edits.len(),
                    spec.low,
                    spec.high
                ),
                bar: 0,
                edits: range_edits,
            });
        }
        for bar_index in 0..=(song.end_tick() / bar).max(0) {
            let start = bar_index * bar;
            let stop = start + bar;
            let mut notes: Vec<&Note> = track
                .notes
                .iter()
                .filter(|note| note.start < stop && note.start + note.duration.max(1) > start)
                .collect();
            notes.sort_by_key(|note| note.start);
            let mut active: Vec<&Note> = Vec::new();
            let mut excess: Vec<&Note> = Vec::new();
            for note in notes {
                active.retain(|held| held.start + held.duration.max(1) > note.start);
                active.push(note);
                if active.len() > spec.polyphony
                    && let Some(victim) = active
                        .iter()
                        .min_by_key(|held| {
                            (held.velocity.as_u8(), std::cmp::Reverse(held.pitch.as_u8()))
                        })
                        .copied()
                    && !excess.iter().any(|seen| seen.id == victim.id)
                {
                    excess.push(victim);
                    active.retain(|held| held.id != victim.id);
                }
            }
            if !excess.is_empty() {
                proposals.push(Proposal {
                    title: format!("Polyphony: {} bar {}", track.name, bar_index + 1),
                    detail: format!(
                        "{} note(s) exceed {} voices; the quietest inner notes go.",
                        excess.len(),
                        spec.polyphony
                    ),
                    bar: usize::try_from(bar_index).unwrap_or(0),
                    edits: excess
                        .iter()
                        .map(|note| NoteEdit {
                            track: track.id,
                            note: note.id,
                            kind: EditKind::Remove,
                        })
                        .collect(),
                });
            }
        }
    }
    proposals
}

/// Moves a pitch into range by octaves, or to the nearest bound.
fn fit_pitch(pitch: u8, low: u8, high: u8) -> Option<u8> {
    if pitch >= low && pitch <= high {
        return None;
    }
    if high <= low {
        return None;
    }
    let mut candidate = pitch;
    while candidate < low {
        candidate = candidate.saturating_add(12);
    }
    while candidate > high {
        candidate = candidate.saturating_sub(12);
    }
    if candidate < low || candidate > high {
        candidate = if pitch < low { low } else { high };
    }
    (candidate != pitch).then_some(candidate)
}

/// Applies one proposal, one command per track.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when an edit is rejected.
pub fn apply(session: &mut EditSession, proposal: &Proposal) -> Result<usize, ArrangingError> {
    let mut by_track: BTreeMap<TrackId, Vec<NoteEdit>> = BTreeMap::new();
    for edit in &proposal.edits {
        by_track.entry(edit.track).or_default().push(*edit);
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
            let Some(index) = notes.iter().position(|note| note.id == edit.note) else {
                continue;
            };
            match edit.kind {
                EditKind::Velocity(velocity) => {
                    if let Ok(value) = Velocity::new(velocity) {
                        notes[index].velocity = value;
                        changed += 1;
                    }
                }
                EditKind::Pitch(pitch) => {
                    if let Ok(value) = Pitch::new(pitch) {
                        notes[index].pitch = value;
                        changed += 1;
                    }
                }
                EditKind::Remove => {
                    notes.remove(index);
                    changed += 1;
                }
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
    use music_core::{Channel, PPQ};
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
        assert!(
            crowded
                .edits
                .iter()
                .all(|edit| matches!(edit.kind, EditKind::Velocity(v) if v < 100))
        );
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
        assert!(
            repeat
                .edits
                .iter()
                .all(|edit| matches!(edit.kind, EditKind::Velocity(96)))
        );
    }

    #[test]
    fn a_unique_sparse_song_has_no_proposals() {
        let notes = vec![note(1, 0, "C4", 90), note(2, 960, "E4", 90)];
        assert!(analyze(&song_with(notes)).is_empty());
    }

    #[test]
    fn applying_a_proposal_is_one_undo_step_per_track() {
        let notes: Vec<Note> = (0..14).map(|index| note(index + 1, 0, "C3", 100)).collect();
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

    #[test]
    fn out_of_range_notes_move_by_octaves() {
        let notes = vec![note(1, 0, "C1", 90), note(2, 960, "C8", 90)];
        let proposals = fit_to_synth(
            &song_with(notes),
            SynthSpec {
                low: 36,
                high: 96,
                ..SynthSpec::default()
            },
        );
        let range = proposals
            .iter()
            .find(|proposal| proposal.title.starts_with("Range"))
            .expect("range proposal");
        assert_eq!(range.edits.len(), 2);
        assert!(
            range.edits.iter().all(
                |edit| matches!(edit.kind, EditKind::Pitch(pitch) if (36..=96).contains(&pitch))
            )
        );
    }

    #[test]
    fn polyphony_removes_the_quietest_inner_notes() {
        let notes = vec![
            note(1, 0, "C4", 120),
            note(2, 0, "E4", 60),
            note(3, 0, "G4", 100),
        ];
        let proposals = fit_to_synth(
            &song_with(notes),
            SynthSpec {
                polyphony: 2,
                ..SynthSpec::default()
            },
        );
        let polyphony = proposals
            .iter()
            .find(|proposal| proposal.title.starts_with("Polyphony"))
            .expect("polyphony proposal");
        assert_eq!(polyphony.edits.len(), 1);
        assert!(matches!(polyphony.edits[0].kind, EditKind::Remove));
        assert_eq!(
            polyphony.edits[0].note,
            project::NoteId::new(2).expect("valid")
        );
    }

    #[test]
    fn applying_fit_moves_and_removes_notes() {
        let notes = vec![note(1, 0, "C1", 90), note(2, 0, "C4", 90)];
        let song = song_with(notes);
        let proposals = fit_to_synth(
            &song,
            SynthSpec {
                low: 36,
                high: 96,
                polyphony: 1,
                ..SynthSpec::default()
            },
        );
        let mut session = EditSession::new(project::song::ProjectFile::new(song)).expect("valid");
        for proposal in &proposals {
            apply(&mut session, proposal).expect("apply");
        }
        for track in &session.song().tracks {
            assert!(track.notes.len() <= 1);
            assert!(
                track
                    .notes
                    .iter()
                    .all(|note| (36..=96).contains(&note.pitch.as_u8()))
            );
        }
    }
}
