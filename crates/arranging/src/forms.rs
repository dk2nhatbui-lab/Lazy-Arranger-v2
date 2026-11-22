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
