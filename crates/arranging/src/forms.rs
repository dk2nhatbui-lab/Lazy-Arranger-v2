//! Song Forms: find sections, build a form, write it to the timeline.
//!
//! Sections come from the song's markers: each runs from its marker to the next
//! one, and the last runs to the end of the song (minimum one bar). A form is a
//! list of section names with repeats; applying it copies the notes, chords and
//! markers of each chosen section to a fresh timeline in form order.

use app_commands::{Command, EditSession};
use music_core::Tick;
use project::song::{ChordEvent, Note, Song};
use project::{Marker, TrackId};

use crate::ArrangingError;

/// One section of the song.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Marker name.
    pub name: String,
    /// First tick.
    pub start: Tick,
    /// Length in ticks.
    pub length: Tick,
}

/// One entry in a form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormPart {
    /// Section to play.
    pub section: String,
    /// How many times in a row.
    pub repeats: usize,
    /// Drop the melody (highest track) for this part.
    pub no_melody: bool,
}

/// A song form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    /// Display name.
    pub name: String,
    /// Parts in order.
    pub parts: Vec<FormPart>,
}

impl Form {
    fn part(&mut self, section: &str, repeats: usize) {
        self.parts.push(FormPart {
            section: section.to_string(),
            repeats,
            no_melody: false,
        });
    }
}

/// Reads sections from the song's markers.
#[must_use]
pub fn sections(song: &Song) -> Vec<Section> {
    let bar = song.time_signature.bar_ticks().max(1);
    let end = song.end_tick().max(bar);
    song.markers
        .iter()
        .enumerate()
        .map(|(index, marker)| {
            let next = song.markers.get(index + 1).map_or(end, |next| next.tick);
            Section {
                name: marker.name.clone(),
                start: marker.tick,
                length: (next - marker.tick).max(bar),
            }
        })
        .collect()
}

/// The short form: intro (if any), each main section once, ending (if any).
#[must_use]
pub fn short(sections: &[Section]) -> Form {
    let mut form = Form {
        name: String::from("Short"),
        parts: Vec::new(),
    };
    for section in sections {
        let lower = section.name.to_lowercase();
        if lower.contains("intro") || lower.contains("main") || lower.contains("ending") {
            form.part(&section.name, 1);
        }
    }
    if form.parts.is_empty() {
        form.parts = sections
            .iter()
            .map(|section| FormPart {
                section: section.name.clone(),
                repeats: 1,
                no_melody: false,
            })
            .collect();
    }
    form
}

/// A gig version: the first main section returns at the end.
#[must_use]
pub fn gig(sections: &[Section]) -> Form {
    let mut form = short(sections);
    form.name = String::from("Gig");
    let first_main = form
        .parts
        .iter()
        .find(|part| part.section.to_lowercase().contains("main"))
        .map(|part| part.section.clone());
    if let Some(name) = first_main {
        form.part(&name, 1);
    }
    form
}

/// A long intro: every intro section twice.
#[must_use]
pub fn long_intro(sections: &[Section]) -> Form {
    let mut form = short(sections);
    form.name = String::from("Long intro");
    for part in &mut form.parts {
        if part.section.to_lowercase().contains("intro") {
            part.repeats = 2;
        }
    }
    form
}

/// An instrumental version: the melody is dropped from main sections.
#[must_use]
pub fn instrumental(sections: &[Section]) -> Form {
    let mut form = short(sections);
    form.name = String::from("Instrumental");
    for part in &mut form.parts {
        if part.section.to_lowercase().contains("main") {
            part.no_melody = true;
        }
    }
    form
}

/// Where each form part lands on the new timeline.
#[must_use]
pub fn plan(sections: &[Section], form: &Form) -> Vec<(Section, Tick, bool)> {
    let mut placed = Vec::new();
    let mut target = 0;
    for part in &form.parts {
        let Some(section) = sections.iter().find(|section| section.name == part.section) else {
            continue;
        };
        for _ in 0..part.repeats.max(1) {
            placed.push((section.clone(), target, part.no_melody));
            target += section.length;
        }
    }
    placed
}

/// Writes a form to the timeline.
///
/// Copies notes per track, chords and markers; one command per track plus one
/// for chords and one for markers, each undoable.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when an edit is rejected.
pub fn apply(
    session: &mut EditSession,
    sections: &[Section],
    form: &Form,
) -> Result<usize, ArrangingError> {
    let placed = plan(sections, form);
    let melody = session
        .song()
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
    let mut copied = 0;
    let track_ids: Vec<(usize, TrackId)> = session
        .song()
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| (index, track.id))
        .collect();
    for (index, track_id) in track_ids {
        let Some(track) = session
            .song()
            .tracks
            .iter()
            .find(|track| track.id == track_id)
        else {
            continue;
        };
        let mut notes: Vec<Note> = Vec::new();
        for (section, target, no_melody) in &placed {
            if *no_melody && Some(index) == melody {
                continue;
            }
            for note in &track.notes {
                if note.start < section.start || note.start >= section.start + section.length {
                    continue;
                }
                let mut copy = note.clone();
                copy.start = target + (note.start - section.start);
                notes.push(copy);
                copied += 1;
            }
        }
        for (index, note) in notes.iter_mut().enumerate() {
            if let Ok(id) = project::NoteId::new(index as u32 + 1) {
                note.id = id;
            }
        }
        session.apply(Command::ReplaceTrackNotes {
            track: track_id,
            notes,
        })?;
    }
    let mut chords: Vec<ChordEvent> = Vec::new();
    let source_chords = session.song().chords.clone();
    for (section, target, _) in &placed {
        for event in &source_chords {
            if event.tick >= section.start && event.tick < section.start + section.length {
                chords.push(ChordEvent {
                    tick: target + (event.tick - section.start),
                    chord: event.chord,
                });
            }
        }
    }
    session.apply(Command::SetChords { chords })?;
    let markers: Vec<Marker> = placed
        .iter()
        .map(|(section, target, _)| Marker {
            tick: *target,
            name: section.name.clone(),
        })
        .collect();
    session.apply(Command::SetMarkers { markers })?;
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, Chord, PPQ, Pitch, Velocity};
    use project::NoteId;
    use project::song::Track;

    fn bar() -> Tick {
        3840
    }

    fn song() -> Song {
        let mut song = Song::new("Forms", 120.0);
        song.markers.push(Marker {
            tick: 0,
            name: String::from("Intro"),
        });
        song.markers.push(Marker {
            tick: bar(),
            name: String::from("Main A"),
        });
        song.markers.push(Marker {
            tick: 2 * bar(),
            name: String::from("Ending"),
        });
        song.chords.push(ChordEvent {
            tick: 0,
            chord: Chord::parse("C").expect("valid"),
        });
        song.chords.push(ChordEvent {
            tick: bar(),
            chord: Chord::parse("Am").expect("valid"),
        });
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for index in 0..3_i64 {
            track.notes.push(Note {
                id: NoteId::new(index as u32 + 1).expect("valid"),
                start: index * bar(),
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        song
    }

    #[test]
    fn sections_run_from_marker_to_marker() {
        let sections = sections(&song());
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].name, "Intro");
        assert_eq!(sections[0].length, bar());
        assert_eq!(sections[2].name, "Ending");
    }

    #[test]
    fn forms_choose_sections_and_plan_positions() {
        let sections = sections(&song());
        let short = short(&sections);
        assert_eq!(short.parts.len(), 3);
        let gig = gig(&sections);
        assert_eq!(gig.parts.len(), 4);
        assert_eq!(gig.parts[3].section, "Main A");
        let placed = plan(&sections, &gig);
        assert_eq!(placed.len(), 4);
        assert_eq!(placed[3].1, 3 * bar());
        assert!(
            instrumental(&sections)
                .parts
                .iter()
                .any(|part| part.section == "Main A" && part.no_melody)
        );
    }

    #[test]
    fn applying_a_form_rewrites_notes_chords_markers_and_undoes() {
        let sections = sections(&song());
        let form = Form {
            name: String::from("Test"),
            parts: vec![
                FormPart {
                    section: String::from("Main A"),
                    repeats: 2,
                    no_melody: false,
                },
                FormPart {
                    section: String::from("Ending"),
                    repeats: 1,
                    no_melody: false,
                },
            ],
        };
        let mut session = EditSession::new(project::song::ProjectFile::new(song())).expect("valid");
        let before = session.song().tracks[0].notes.len();
        let copied = apply(&mut session, &sections, &form).expect("apply");
        assert_eq!(copied, 3, "Main A twice and Ending once each hold one note");
        assert_eq!(session.song().tracks[0].notes.len(), 3);
        assert_eq!(session.song().markers.len(), 3);
        assert_eq!(session.song().markers[1].tick, bar());
        while session.undo().is_ok() {}
        assert_eq!(session.song().tracks[0].notes.len(), before);
        assert_eq!(session.song().markers.len(), 3);
    }
}

/// What happens at a join between two form parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    /// No added notes.
    Straight,
    /// Four snare hits over the last beat before the join.
    DrumFill,
    /// A chord stab one beat before the join.
    LeadIn,
    /// Drum fill when the song has drums, otherwise a lead-in.
    Automatic,
    /// Holds the last chord through the bar before the join.
    Hold,
    /// Silences the bar before the join.
    Break,
}

/// Notes a transition adds, before identifiers are assigned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionPlan {
    /// Track and note pairs to add.
    pub notes: Vec<(TrackId, Note)>,
    /// What was chosen, for a status line.
    pub description: String,
}

/// The bar a break silences, ending at `join_tick`.
#[must_use]
pub fn break_range(join_tick: Tick, bar: Tick) -> (Tick, Tick) {
    ((join_tick - bar.max(1)).max(0), join_tick)
}

/// Removes every note starting in `from..to` on all tracks, as one edit per
/// track, and returns how many were removed.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when a track edit is rejected.
pub fn apply_break(
    session: &mut EditSession,
    from: Tick,
    to: Tick,
) -> Result<usize, ArrangingError> {
    let tracks: Vec<TrackId> = session.song().tracks.iter().map(|track| track.id).collect();
    let mut removed = 0;
    for track in tracks {
        let notes: Vec<Note> = session
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| {
                entry
                    .notes
                    .iter()
                    .filter(|note| note.start < from || note.start >= to)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let before = session
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map_or(0, |entry| entry.notes.len());
        if notes.len() == before {
            continue;
        }
        session.apply(Command::ReplaceTrackNotes { track, notes })?;
        removed += before
            - session
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .map_or(before, |entry| entry.notes.len());
    }
    Ok(removed)
}

/// Plans a transition ending exactly at `join_tick`.
#[must_use]
pub fn transition(song: &Song, join_tick: Tick, kind: Transition) -> TransitionPlan {
    let drums = song.tracks.iter().find(|track| {
        track
            .notes
            .iter()
            .any(|note| note.channel.zero_based() == 9)
    });
    let chosen = match kind {
        Transition::Automatic => {
            if drums.is_some() {
                Transition::DrumFill
            } else {
                Transition::LeadIn
            }
        }
        other => other,
    };
    let mut notes = Vec::new();
    match chosen {
        Transition::Break => {}
        Transition::Hold => {
            let bar = song.time_signature.bar_ticks().max(1);
            let chord = song
                .chords
                .iter()
                .filter(|event| event.tick <= join_tick)
                .max_by_key(|event| event.tick)
                .map(|event| event.chord);
            let track = song.tracks.iter().find(|track| {
                track
                    .notes
                    .iter()
                    .any(|note| note.channel.zero_based() != 9)
            });
            if let (Some(chord), Some(track)) = (chord, track) {
                for (index, pitch) in chord.tones(4).iter().enumerate() {
                    notes.push((
                        track.id,
                        Note {
                            id: project::NoteId::new(index as u32 + 1).expect("valid"),
                            start: (join_tick - bar).max(0),
                            duration: bar,
                            pitch: *pitch,
                            velocity: music_core::Velocity::new(90).expect("valid"),
                            channel: music_core::Channel::first(),
                        },
                    ));
                }
            }
        }
        Transition::DrumFill => {
            if let Some(track) = drums {
                for (index, velocity) in [70_u8, 85, 100, 115].into_iter().enumerate() {
                    let start = join_tick - (4 - index as Tick) * (music_core::PPQ / 4);
                    notes.push((
                        track.id,
                        Note {
                            id: project::NoteId::new(1).expect("valid"),
                            start,
                            duration: music_core::PPQ / 4,
                            pitch: music_core::Pitch::new(38).expect("valid"),
                            velocity: music_core::Velocity::new(velocity).expect("valid"),
                            channel: track
                                .notes
                                .first()
                                .map_or(music_core::Channel::first(), |note| note.channel),
                        },
                    ));
                }
            }
        }
        Transition::LeadIn => {
            let chord = song
                .chords
                .iter()
                .filter(|event| event.tick <= join_tick)
                .max_by_key(|event| event.tick)
                .map(|event| event.chord);
            if let (Some(chord), Some(track)) = (
                chord,
                song.tracks.iter().find(|track| {
                    track
                        .notes
                        .iter()
                        .any(|note| note.channel.zero_based() != 9)
                }),
            ) {
                for pitch in chord.tones(3) {
                    notes.push((
                        track.id,
                        Note {
                            id: project::NoteId::new(1).expect("valid"),
                            start: (join_tick - music_core::PPQ).max(0),
                            duration: music_core::PPQ / 2,
                            pitch,
                            velocity: music_core::Velocity::new(90).expect("valid"),
                            channel: track
                                .notes
                                .first()
                                .map_or(music_core::Channel::first(), |note| note.channel),
                        },
                    ));
                }
            }
        }
        Transition::Straight => {}
        Transition::Automatic => {}
    }
    TransitionPlan {
        description: match chosen {
            Transition::DrumFill => String::from("drum fill"),
            Transition::LeadIn => String::from("lead-in chord"),
            Transition::Straight => String::from("straight cut"),
            Transition::Automatic => String::from("automatic"),
            Transition::Hold => String::from("hold"),
            Transition::Break => String::from("break"),
        },
        notes,
    }
}

/// Adds a transition's notes to the timeline.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when an edit is rejected.
pub fn apply_transition(
    session: &mut EditSession,
    plan: &TransitionPlan,
) -> Result<usize, ArrangingError> {
    use std::collections::BTreeMap;
    let mut by_track: BTreeMap<TrackId, Vec<Note>> = BTreeMap::new();
    for (track, note) in &plan.notes {
        by_track.entry(*track).or_default().push(note.clone());
    }
    for (track_id, new_notes) in by_track {
        let Some(track) = session
            .song()
            .tracks
            .iter()
            .find(|track| track.id == track_id)
        else {
            continue;
        };
        let mut notes = track.notes.clone();
        notes.extend(new_notes);
        notes.sort_by_key(|note| note.start);
        for (index, note) in notes.iter_mut().enumerate() {
            if let Ok(id) = project::NoteId::new(index as u32 + 1) {
                note.id = id;
            }
        }
        session.apply(Command::ReplaceTrackNotes {
            track: track_id,
            notes,
        })?;
    }
    Ok(plan.notes.len())
}

#[cfg(test)]
mod transition_tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::NoteId;
    use project::song::Track;

    fn drum_track() -> Track {
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Drums");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::new(36).expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::new(9).expect("valid"),
        });
        track
    }

    fn pitched_track() -> Track {
        let mut track = Track::new(TrackId::new(2).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        track
    }

    fn song_with(tracks: Vec<Track>, chord: bool) -> Song {
        let mut song = Song::new("Transitions", 120.0);
        song.tracks = tracks;
        if chord {
            song.chords.push(ChordEvent {
                tick: 0,
                chord: music_core::Chord::parse("C").expect("valid"),
            });
        }
        song
    }

    #[test]
    fn a_drum_fill_lands_on_the_last_beat() {
        let song = song_with(vec![drum_track()], false);
        let plan = transition(&song, 7680, Transition::DrumFill);
        assert_eq!(plan.notes.len(), 4);
        assert_eq!(plan.notes[0].1.start, 7680 - 4 * (PPQ / 4));
        assert_eq!(plan.notes[3].1.start + plan.notes[3].1.duration, 7680);
        assert!(plan.notes[3].1.velocity.as_u8() > plan.notes[0].1.velocity.as_u8());
    }

    #[test]
    fn a_lead_in_uses_the_chord_in_force() {
        let song = song_with(vec![pitched_track()], true);
        let plan = transition(&song, 3840, Transition::LeadIn);
        assert_eq!(plan.notes.len(), 3, "a C major triad");
        assert_eq!(plan.notes[0].1.start, 3840 - PPQ);
    }

    #[test]
    fn automatic_prefers_drums_and_falls_back_to_lead_in() {
        let with_drums = song_with(vec![drum_track(), pitched_track()], true);
        assert_eq!(
            transition(&with_drums, 3840, Transition::Automatic).description,
            "drum fill"
        );
        let without = song_with(vec![pitched_track()], true);
        assert_eq!(
            transition(&without, 3840, Transition::Automatic).description,
            "lead-in chord"
        );
        assert_eq!(
            transition(&with_drums, 3840, Transition::Straight)
                .notes
                .len(),
            0
        );
    }

    #[test]
    fn applying_a_transition_adds_notes_and_undoes() {
        let song = song_with(vec![drum_track()], false);
        let plan = transition(&song, 7680, Transition::DrumFill);
        let mut session = EditSession::new(project::song::ProjectFile::new(song)).expect("valid");
        let before = session.song().tracks[0].notes.len();
        let added = apply_transition(&mut session, &plan).expect("apply");
        assert_eq!(added, 4);
        assert_eq!(session.song().tracks[0].notes.len(), before + 4);
        session.undo().expect("undo");
        assert_eq!(session.song().tracks[0].notes.len(), before);
    }
}

#[cfg(test)]
mod transition_extra_tests {
    use super::*;
    use app_commands::EditSession;
    use music_core::{Channel, Chord, PPQ, Pitch, Velocity};
    use project::song::{ChordEvent, Note, ProjectFile, Song, Track};
    use project::{NoteId, TrackId};

    fn song() -> Song {
        let mut song = Song::new("Forms", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        song.chords.push(ChordEvent {
            tick: 0,
            chord: Chord::parse("C").expect("valid"),
        });
        song
    }

    #[test]
    fn hold_sustains_the_last_chord_for_a_bar() {
        let song = song();
        let plan = transition(&song, 3840, Transition::Hold);
        assert_eq!(plan.description, "hold");
        assert_eq!(plan.notes.len(), 3);
        assert!(
            plan.notes
                .iter()
                .all(|(_, note)| note.start == 0 && note.duration == 3840)
        );
    }

    #[test]
    fn break_range_and_apply_remove_the_bar() {
        let mut session = EditSession::new(ProjectFile::new(song())).expect("valid");
        let bar = session.song().time_signature.bar_ticks();
        let (from, to) = break_range(bar, bar);
        assert_eq!((from, to), (0, bar));
        let removed = apply_break(&mut session, from, to).expect("break");
        assert_eq!(removed, 1);
        assert!(session.song().tracks[0].notes.is_empty());
        session.undo().expect("undo");
        assert_eq!(session.song().tracks[0].notes.len(), 1);
    }
}

/// What a join loop changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoopOutcome {
    /// Section that was repeated.
    pub section: String,
    /// Length copied, in ticks.
    pub length: Tick,
    /// Extra copies inserted.
    pub copies: usize,
    /// Notes added.
    pub notes: usize,
    /// Chords added.
    pub chords: usize,
    /// Lyrics added.
    pub lyrics: usize,
    /// Marks added.
    pub marks: usize,
    /// Clips copied.
    pub clips: usize,
    /// Audio references moved.
    pub audio: usize,
}

/// Repeats a section in place, shifting later material.
///
/// `total` is the number of copies that should exist, so `2` inserts one extra
/// copy. Notes, chords, lyrics and marks inside the section are copied; later
/// notes, chords, lyrics, marks and markers move by the inserted length. Clips,
/// automation and audio references are not shifted.
///
/// # Errors
///
/// Returns a rejection for an unknown section or a total below two.
pub fn join_loop(
    session: &mut EditSession,
    section_name: &str,
    total: usize,
) -> Result<LoopOutcome, ArrangingError> {
    if total < 2 {
        return Err(ArrangingError::Command(
            app_commands::CommandError::Validation(String::from(
                "a join loop needs at least two total copies",
            )),
        ));
    }
    let song = session.song().clone();
    let sections = sections(&song);
    let Some(section) = sections.iter().find(|section| section.name == section_name) else {
        return Err(ArrangingError::Command(
            app_commands::CommandError::Validation(format!("no section named {section_name:?}")),
        ));
    };
    let start = section.start;
    let end = start + section.length;
    let shift = section
        .length
        .saturating_mul(Tick::try_from(total - 1).unwrap_or(1));
    let copies = total - 1;
    let mut outcome = LoopOutcome {
        section: section.name.clone(),
        length: section.length,
        copies,
        ..LoopOutcome::default()
    };

    for track in &song.tracks {
        let mut notes = Vec::new();
        for note in &track.notes {
            if note.start >= end {
                let mut moved = note.clone();
                moved.start += shift;
                notes.push(moved);
            } else {
                notes.push(note.clone());
            }
            if note.start >= start && note.start < end {
                for copy in 0..copies {
                    let mut added = note.clone();
                    added.start =
                        note.start + end + section.length * Tick::try_from(copy).unwrap_or(0)
                            - start;
                    notes.push(added);
                    outcome.notes += 1;
                }
            }
        }
        notes.sort_by_key(|note| note.start);
        let mut next = notes.iter().map(|note| note.id.get()).max().unwrap_or(0) + 1;
        for note in &mut notes {
            if let Ok(id) = project::NoteId::new(next) {
                note.id = id;
                next += 1;
            }
        }
        if notes != track.notes {
            session.apply(Command::ReplaceTrackNotes {
                track: track.id,
                notes,
            })?;
        }

        if !track.clips.is_empty() {
            let mut clips = Vec::new();
            let mut next_clip = track
                .clips
                .iter()
                .map(|clip| clip.id.get())
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            for clip in &track.clips {
                let mut kept = clip.clone();
                if clip.start >= end {
                    kept.start += shift;
                }
                clips.push(kept);
                if clip.start >= start && clip.end() <= end {
                    for copy in 0..copies {
                        let mut added = clip.clone();
                        added.start =
                            clip.start + end + section.length * Tick::try_from(copy).unwrap_or(0)
                                - start;
                        if let Ok(id) = project::ClipId::new(next_clip) {
                            added.id = id;
                            next_clip = next_clip.saturating_add(1);
                            clips.push(added);
                            outcome.clips += 1;
                        }
                    }
                }
            }
            clips.sort_by_key(|clip| clip.start);
            if clips != track.clips {
                session.apply(Command::SetClips {
                    track: track.id,
                    clips,
                })?;
            }
        }

        if !track.automation.is_empty() {
            let mut lanes = track.automation.clone();
            for lane in &mut lanes {
                for point in &mut lane.points {
                    if point.tick >= end {
                        point.tick += shift;
                    }
                }
                lane.points.sort_by_key(|point| point.tick);
            }
            if lanes != track.automation {
                session.apply(Command::SetAutomation {
                    track: track.id,
                    lanes,
                })?;
            }
        }

        if let Some(reference) = &track.audio
            && reference.start >= end
        {
            let mut moved = reference.clone();
            moved.start += shift;
            session.apply(Command::SetAudioRef {
                track: track.id,
                reference: Some(moved),
            })?;
            outcome.audio = outcome.audio.saturating_add(1);
        }
    }

    let mut chords: Vec<ChordEvent> = Vec::new();
    for event in &song.chords {
        let mut kept = event.clone();
        if event.tick >= end {
            kept.tick += shift;
        }
        chords.push(kept);
        if event.tick >= start && event.tick < end {
            for copy in 0..copies {
                let mut added = event.clone();
                added.tick =
                    event.tick + end + section.length * Tick::try_from(copy).unwrap_or(0) - start;
                chords.push(added);
                outcome.chords += 1;
            }
        }
    }
    chords.sort_by_key(|event| event.tick);
    if chords != song.chords {
        session.apply(Command::SetChords { chords })?;
    }

    let mut lyrics = Vec::new();
    for event in &song.lyrics {
        let mut kept = event.clone();
        if event.tick >= end {
            kept.tick += shift;
        }
        lyrics.push(kept);
        if event.tick >= start && event.tick < end {
            for copy in 0..copies {
                let mut added = event.clone();
                added.tick =
                    event.tick + end + section.length * Tick::try_from(copy).unwrap_or(0) - start;
                lyrics.push(added);
                outcome.lyrics += 1;
            }
        }
    }
    lyrics.sort_by_key(|event| event.tick);
    if lyrics != song.lyrics {
        session.apply(Command::SetLyrics { lyrics })?;
    }

    let mut marks = Vec::new();
    for mark in &song.marks {
        let mut kept = mark.clone();
        if mark.tick >= end {
            kept.tick += shift;
        }
        marks.push(kept);
        if mark.tick >= start && mark.tick < end {
            for copy in 0..copies {
                let mut added = mark.clone();
                added.tick =
                    mark.tick + end + section.length * Tick::try_from(copy).unwrap_or(0) - start;
                marks.push(added);
                outcome.marks += 1;
            }
        }
    }
    marks.sort_by_key(|mark| mark.tick);
    if marks != song.marks {
        session.apply(Command::SetMarks { marks })?;
    }

    let mut markers = Vec::new();
    for marker in &song.markers {
        let mut kept = marker.clone();
        if marker.tick >= end {
            kept.tick += shift;
        }
        markers.push(kept);
    }
    markers.sort_by_key(|marker| marker.tick);
    if markers != song.markers {
        session.apply(Command::SetMarkers { markers })?;
    }
    Ok(outcome)
}

#[cfg(test)]
mod join_loop_tests {
    use super::*;
    use music_core::{Channel, Chord, PPQ, Pitch, Velocity};
    use project::song::{LyricEvent, Mark, MarkKind, ProjectFile, Song, Track};
    use project::{NoteId, TrackId};

    fn song() -> Song {
        let mut song = Song::new("Loop", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, start) in [(1_u32, 0_i64), (2, 3840)] {
            track.notes.push(Note {
                id: NoteId::new(index).expect("valid"),
                start,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        song.chords.push(ChordEvent {
            tick: 0,
            chord: Chord::parse("C").expect("valid"),
        });
        song.chords.push(ChordEvent {
            tick: 3840,
            chord: Chord::parse("G").expect("valid"),
        });
        song.tracks[0].clips.push(project::song::Clip {
            id: project::ClipId::new(1).expect("valid"),
            name: String::from("A"),
            start: 0,
            length: 3840,
        });
        song.tracks[0].clips.push(project::song::Clip {
            id: project::ClipId::new(2).expect("valid"),
            name: String::from("B"),
            start: 3840,
            length: 3840,
        });
        song.tracks[0].automation.push(project::song::Automation {
            target: project::song::AutomationTarget::Volume,
            points: vec![project::song::AutomationPoint {
                tick: 3840,
                value: 80.0,
            }],
        });
        song.tracks[0].audio = Some(project::song::AudioRef {
            path: String::from("/tmp/take.wav"),
            start: 3840,
            length: 960,
            offset: 0,

            fade_in: 0,
            fade_out: 0,

            stretch: false,
        });
        song.lyrics.push(LyricEvent {
            tick: 0,
            text: String::from("la"),
        });
        song.marks.push(Mark {
            tick: 0,
            kind: MarkKind::Dynamic(project::song::Dynamic::F),
        });
        song.markers.push(Marker {
            tick: 0,
            name: String::from("A"),
        });
        song.markers.push(Marker {
            tick: 3840,
            name: String::from("B"),
        });
        song
    }

    #[test]
    fn joining_loops_the_section_and_shifts_later_material() {
        let mut session = EditSession::new(ProjectFile::new(song())).expect("valid");
        let outcome = join_loop(&mut session, "A", 2).expect("join");
        assert_eq!(outcome.copies, 1);
        assert_eq!(outcome.notes, 1);
        assert_eq!(outcome.chords, 1);
        let song = session.song();
        assert_eq!(song.tracks[0].notes.len(), 3);
        let starts: Vec<Tick> = song.tracks[0].notes.iter().map(|note| note.start).collect();
        assert!(starts.contains(&3840) && starts.contains(&7680));
        assert_eq!(song.chords.len(), 3);
        assert_eq!(song.markers[1].tick, 7680, "the B marker moved");
        assert_eq!(song.lyrics.len(), 2);
        assert_eq!(song.marks.len(), 2);
        assert_eq!(song.tracks[0].clips.len(), 3, "A copied, B moved");
        let ids: std::collections::BTreeSet<u32> = song.tracks[0]
            .clips
            .iter()
            .map(|clip| clip.id.get())
            .collect();
        assert_eq!(ids.len(), 3, "clip ids stay unique");
        assert_eq!(song.tracks[0].automation[0].points[0].tick, 7680);
        assert_eq!(song.tracks[0].audio.as_ref().expect("audio").start, 7680);
        session.undo().expect("undo marks");
        session.undo().expect("undo lyrics");
        session.undo().expect("undo chords");
        session.undo().expect("undo notes");
        session.undo().expect("undo markers");
        assert_eq!(session.song().markers[1].tick, 3840);
    }

    #[test]
    fn join_loop_requires_a_known_section_and_two_copies() {
        let mut session = EditSession::new(ProjectFile::new(song())).expect("valid");
        assert!(join_loop(&mut session, "A", 1).is_err());
        assert!(join_loop(&mut session, "Z", 2).is_err());
    }
}
