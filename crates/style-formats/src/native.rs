//! Writes a native `arranger::Style` as a Yamaha style container.
//!
//! The style's sections become marked regions in a MIDI song; each part becomes
//! one MIDI track on its destination channel. The same rules are written to the
//! CASM chunk, so an imported file keeps the section list and chord rules.

use arranger::style::Style;
use music_core::{Channel, Pitch, TimeSignature, Velocity};
use project::ids::{Marker, NoteId, TrackId};
use project::song::{Note, Song, Track};

use crate::casm::{CasmRules, CasmStats, SectionRules, SffVersion};
use crate::write::write_style;
use crate::{ImportStats, ImportedSection, ImportedStyle, StyleError};

/// Writes a native style as an SFF1/SFF2 container.
///
/// # Errors
///
/// Returns a message when pitch, velocity, channel or MIDI encoding fails.
pub fn write_native_style(style: &Style, version: SffVersion) -> Result<Vec<u8>, StyleError> {
    let mut song = Song::new(&style.name, 120.0);
    song.time_signature = TimeSignature::new(4, 4).unwrap_or_default();
    let mut sections = Vec::new();
    let mut groups = Vec::new();
    let mut offset: music_core::Tick = 0;
    let mut track_id = 1_u32;
    for section in &style.sections {
        song.markers.push(Marker {
            tick: offset,
            name: section.name.clone(),
        });
        let mut rules = Vec::new();
        let mut notes = 0_usize;
        for part in &section.parts {
            let channel = Channel::from_one_based(part.rule.dest_channel)
                .map_err(|_| StyleError::Midi(String::from("invalid destination channel")))?;
            let id = TrackId::new(track_id).map_err(|error| StyleError::Midi(error.to_string()))?;
            track_id += 1;
            let mut track = Track::new(id, part.name.clone());
            for (index, note) in part.notes.iter().enumerate() {
                track.notes.push(Note {
                    id: NoteId::new(index as u32 + 1)
                        .map_err(|error| StyleError::Midi(error.to_string()))?,
                    start: offset.saturating_add(note.tick),
                    duration: note.duration,
                    pitch: Pitch::new(note.pitch)
                        .map_err(|error| StyleError::Midi(error.to_string()))?,
                    velocity: Velocity::new(note.velocity)
                        .map_err(|error| StyleError::Midi(error.to_string()))?,
                    channel,
                });
                notes += 1;
            }
            song.tracks.push(track);
            rules.push(part.rule.clone());
        }
        sections.push(ImportedSection {
            name: section.name.clone(),
            length_ticks: section.length_ticks,
            notes,
            parts: section.parts.len(),
        });
        groups.push(SectionRules {
            sections: vec![section.name.clone()],
            rules,
        });
        offset = offset.saturating_add(section.length_ticks);
    }
    let rules = CasmRules {
        groups,
        version,
        has_casm: true,
        stats: CasmStats::default(),
    };
    let imported = ImportedStyle {
        native: style.clone(),
        source_ppq: 960,
        tempo_bpm: 120.0,
        time_signature: TimeSignature::new(4, 4).unwrap_or_default(),
        song,
        sections,
        stats: ImportStats::default(),
    };
    write_style(&imported, &rules, version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_style;
    use music_core::PitchClass;
    use project::song::Song;

    #[test]
    fn a_native_style_round_trips_through_sff1() {
        let mut song = Song::new("Clips", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Bass");
        track.notes.push(Note {
            id: NoteId::new(1).expect("note"),
            start: 0,
            duration: music_core::PPQ,
            pitch: Pitch::from_name("C2").expect("pitch"),
            velocity: Velocity::new(100).expect("velocity"),
            channel: Channel::from_one_based(11).expect("channel"),
        });
        song.tracks.push(track);
        let style = arranger::style::from_song(
            &song,
            "Clips",
            arranger::chord_type::ArrangerChord::new(
                PitchClass::new(0),
                arranger::chord_type::ChordType::Major,
            ),
            music_core::PPQ * 4,
        );
        let bytes = write_native_style(&style, SffVersion::Sff1).expect("write");
        let imported = import_style(&bytes).expect("import");
        assert!(imported.section_names().contains(&"Main A"));
        assert_eq!(imported.native.sections[0].parts.len(), 1);
        assert_eq!(imported.native.sections[0].parts[0].notes.len(), 1);
    }
}
