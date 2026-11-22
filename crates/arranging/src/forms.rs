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
}

/// Notes a transition adds, before identifiers are assigned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionPlan {
    /// Track and note pairs to add.
    pub notes: Vec<(TrackId, Note)>,
    /// What was chosen, for a status line.
    pub description: String,
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
