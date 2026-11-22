//! Style import: turn a Yamaha style file into a playable native style.
//!
//! Two sources are combined:
//!
//! * section boundaries and notes come from the style's MIDI track, where each
//!   section starts with a marker meta event (type `0x06`) whose name matches a
//!   `Sdec` entry in the CASM part;
//! * chord rules come from [`crate::casm::read_rules`].
//!
//! Ticks are rescaled to the project's 960 PPQ by the MIDI reader. Channels a
//! section has no rule for get the oracle's default rule
//! (`arranger::ChordRule::default_for_channel`).
//!
//! Not imported yet: program changes and other setup events, voices, and the
//! `SInt` setup region before the first section marker. Notes outside any
//! section are counted, not silently dropped.

use std::collections::BTreeMap;

use arranger::{ArrangerChord, ChordRule, ChordType, Style, StylePart, StyleSection};
use music_core::{Tick, TimeSignature};
use project::song::Song;

use crate::StyleError;
use crate::casm::{CasmStats, read_rules};

/// Counters describing what the import had to decide.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImportStats {
    /// Marker meta events in the MIDI track, including setup markers.
    pub markers_total: usize,
    /// Markers whose name is a CASM section.
    pub markers_used: usize,
    /// Markers ignored because no CASM rule set names them.
    pub markers_skipped: usize,
    /// CASM sections that carried no notes.
    pub sections_without_notes: usize,
    /// Parts that used the default rule because the section had no rule for
    /// their channel.
    pub channels_without_rule: usize,
    /// Notes before the first section marker; usually setup noise.
    pub notes_before_first_section: usize,
    /// Note-ons the MIDI reader dropped because the file never released them.
    pub unpaired_note_ons: usize,
    /// CASM rule parsing counters.
    pub casm: CasmStats,
}

/// One imported section, for reporting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedSection {
    /// Section name.
    pub name: String,
    /// Length in project ticks.
    pub length_ticks: Tick,
    /// Notes carried across.
    pub notes: usize,
    /// Parts built.
    pub parts: usize,
}

/// A style ready for `arranger::Style::compile`, plus what the import found.
#[derive(Clone, Debug)]
pub struct ImportedStyle {
    /// The playable style.
    pub native: Style,
    /// MIDI resolution of the source file.
    pub source_ppq: u16,
    /// Tempo found in the source file.
    pub tempo_bpm: f64,
    /// Meter found in the source file.
    pub time_signature: TimeSignature,
    /// The decoded MIDI song the style came from (960 PPQ, markers included).
    pub song: Song,
    /// Sections in source order.
    pub sections: Vec<ImportedSection>,
    /// Import decisions and counts.
    pub stats: ImportStats,
}

impl ImportedStyle {
    /// Section names in source order.
    #[must_use]
    pub fn section_names(&self) -> Vec<&str> {
        self.native
            .sections
            .iter()
            .map(|section| section.name.as_str())
            .collect()
    }
}

/// Imports a Yamaha style from its container bytes.
///
/// # Errors
///
/// Returns [`StyleError::Midi`] when the MIDI track cannot be read, and
/// [`StyleError::NoSections`] when no marker matches a CASM section.
pub fn import_style(bytes: &[u8]) -> Result<ImportedStyle, StyleError> {
    let rules = read_rules(bytes);
    let (song, report) = smf::decode(bytes).map_err(|error| StyleError::Midi(error.to_string()))?;

    let section_names = rules.section_names();
    let mut stats = ImportStats {
        markers_total: song.markers.len(),
        casm: rules.stats,
        unpaired_note_ons: report.unpaired_note_ons,
        ..ImportStats::default()
    };

    let mut selected = Vec::new();
    for marker in &song.markers {
        if section_names.iter().any(|name| name == &marker.name) {
            if selected
                .iter()
                .any(|(name, _): &(String, Tick)| name == &marker.name)
            {
                continue;
            }
            selected.push((marker.name.clone(), marker.tick));
        } else {
            stats.markers_skipped += 1;
        }
    }
    stats.markers_used = selected.len();
    if selected.is_empty() {
        return Err(StyleError::NoSections);
    }

    let song_end = song.end_tick().max(selected[selected.len() - 1].1 + 1);
    let notes: Vec<_> = song
        .tracks
        .iter()
        .flat_map(|track| track.notes.iter())
        .collect();
    let first_start = selected[0].1;

    let mut native_sections = Vec::new();
    let mut imported_sections = Vec::new();
    let mut source_chord: Option<ArrangerChord> = None;

    let bar = song.time_signature.bar_ticks().max(1);
    let last = selected.len().saturating_sub(1);
    for (index, (section_name, start)) in selected.iter().enumerate() {
        let end = selected.get(index + 1).map_or(song_end, |(_, next)| *next);
        let mut length = end - start;
        if index == last {
            // No marker follows the last section, so its length comes from its
            // notes. Round it up to a bar, because styles loop by the bar.
            let note_end = notes
                .iter()
                .filter(|note| note.start >= *start && note.start < end)
                .map(|note| note.start + note.duration.min(end - note.start))
                .max()
                .unwrap_or(*start);
            let played = (note_end - start).max(1);
            length = played.saturating_add(bar - 1) / bar * bar;
        }
        if length <= 0 {
            continue;
        }
        let mut by_channel: BTreeMap<u8, Vec<arranger::StyleNote>> = BTreeMap::new();
        for note in &notes {
            if note.start < *start || note.start >= end {
                continue;
            }
            let duration = note.duration.min(end - note.start).max(1);
            by_channel
                .entry(note.channel.one_based())
                .or_default()
                .push(arranger::StyleNote {
                    tick: note.start - start,
                    duration,
                    pitch: note.pitch.as_u8(),
                    velocity: note.velocity.as_u8(),
                });
        }

        let section_rules: Vec<ChordRule> =
            rules.rules_for(section_name).into_iter().cloned().collect();
        if source_chord.is_none()
            && let Some(rule) = section_rules.first()
        {
            source_chord = Some(ArrangerChord::new(
                music_core::PitchClass::new(i32::from(rule.source_root)),
                rule.source_chord_type,
            ));
        }

        let mut parts = Vec::new();
        let mut note_count = 0usize;
        for (channel, style_notes) in by_channel {
            let rule = section_rules
                .iter()
                .find(|rule| rule.source_channel == channel)
                .cloned()
                .unwrap_or_else(|| {
                    stats.channels_without_rule += 1;
                    ChordRule::default_for_channel(channel)
                });
            let part_name = if rule.name.is_empty() {
                ChordRule::default_for_channel(rule.dest_channel).name
            } else {
                rule.name.clone()
            };
            note_count += style_notes.len();
            parts.push(StylePart {
                name: part_name,
                rule,
                notes: style_notes,
            });
        }
        if parts.is_empty() {
            stats.sections_without_notes += 1;
            continue;
        }
        parts.sort_by_key(|part| part.rule.dest_channel);
        imported_sections.push(ImportedSection {
            name: section_name.clone(),
            length_ticks: length,
            notes: note_count,
            parts: parts.len(),
        });
        native_sections.push(StyleSection {
            name: section_name.clone(),
            length_ticks: length,
            parts,
        });
    }

    stats.notes_before_first_section = notes.iter().filter(|note| note.start < first_start).count();

    if native_sections.is_empty() {
        return Err(StyleError::NoSections);
    }

    let source_chord = source_chord
        .unwrap_or_else(|| ArrangerChord::new(music_core::PitchClass::new(0), ChordType::Major7));
    Ok(ImportedStyle {
        native: Style {
            name: String::new(),
            source_chord,
            sections: native_sections,
        },
        source_ppq: u16::try_from(report.source_ppq).unwrap_or(0),
        tempo_bpm: song.tempo_map.bpm_at(0),
        time_signature: song.time_signature,
        song: song.clone(),
        sections: imported_sections,
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casm::read_rules;

    fn push_varint(out: &mut Vec<u8>, mut value: u32) {
        let mut stack = [0u8; 4];
        let mut count = 0;
        loop {
            stack[count] = (value & 0x7F) as u8;
            value >>= 7;
            count += 1;
            if value == 0 {
                break;
            }
        }
        for index in (0..count).rev() {
            let byte = if index == 0 {
                stack[index]
            } else {
                stack[index] | 0x80
            };
            out.push(byte);
        }
    }

    fn track_event(out: &mut Vec<u8>, delta: u32, bytes: &[u8]) {
        push_varint(out, delta);
        out.extend_from_slice(bytes);
    }

    fn marker(out: &mut Vec<u8>, delta: u32, name: &str) {
        let mut bytes = vec![0xFF, 0x06];
        push_varint(&mut bytes, u32::try_from(name.len()).expect("small"));
        bytes.extend_from_slice(name.as_bytes());
        track_event(out, delta, &bytes);
    }

    fn sff2_rule(source_channel: u8, dest_channel: u8, name: &str, ntr: u8, ntt: u8) -> Vec<u8> {
        let mut rule = vec![0u8; 40];
        rule[0] = source_channel - 1;
        let padded = format!("{name:<8}");
        rule[1..9].copy_from_slice(&padded.as_bytes()[..8]);
        rule[9] = dest_channel - 1;
        rule[10] = 0;
        rule[11..13].copy_from_slice(&0x0fffu16.to_be_bytes());
        rule[13..18].copy_from_slice(&[0x00, 0xff, 0xff, 0xff, 0xff]);
        rule[18] = 0;
        rule[19] = 2; // major 7
        rule[20] = 0;
        rule[21] = 127;
        for base in [22, 28, 34] {
            rule[base] = ntr;
            rule[base + 1] = ntt;
            rule[base + 2] = 6;
            rule[base + 3] = 0;
            rule[base + 4] = 127;
            rule[base + 5] = 1;
        }
        rule.push(0); // vendor tail
        rule
    }

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&u32::try_from(body.len()).expect("small").to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    /// A one-bar style at 1920 PPQ: Main A (bass + drums), Fill In AA (drums).
    fn style_bytes() -> Vec<u8> {
        let mut track = Vec::new();
        marker(&mut track, 0, "SInt");
        track_event(&mut track, 0, &[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // 120 bpm
        track_event(&mut track, 0, &[0xFF, 0x58, 0x04, 4, 2, 24, 8]);
        marker(&mut track, 0, "Main A");
        // Bass C2 (channel 11) for one quarter.
        track_event(&mut track, 0, &[0x9A, 48, 100]);
        // Kick on channel 10 after one eighth, held for another eighth.
        track_event(&mut track, 960, &[0x99, 36, 110]);
        track_event(&mut track, 480, &[0x99, 36, 0]);
        track_event(&mut track, 480, &[0x8A, 48, 64]);
        marker(&mut track, 5760, "Fill In AA");
        track_event(&mut track, 0, &[0x99, 38, 100]);
        track_event(&mut track, 960, &[0x99, 38, 0]);
        track_event(&mut track, 0, &[0xFF, 0x2F, 0x00]);

        let names = b"Main A,Fill In AA";
        let mut cseg = Vec::new();
        cseg.extend_from_slice(&chunk(b"Sdec", names));
        cseg.extend_from_slice(&chunk(b"Ctb2", &sff2_rule(10, 10, "Drums", 1, 0)));
        cseg.extend_from_slice(&chunk(b"Ctb2", &sff2_rule(11, 11, "Bass", 0, 1)));
        let mut casm = Vec::new();
        casm.extend_from_slice(&chunk(b"CSEG", &cseg));

        let mut out = Vec::new();
        out.extend_from_slice(&chunk(b"MThd", &[0, 0, 0, 1, 0x07, 0x80]));
        out.extend_from_slice(&chunk(b"MTrk", &track));
        out.extend_from_slice(&chunk(b"CASM", &casm));
        out
    }

    #[test]
    fn imports_sections_notes_and_rules() {
        let bytes = style_bytes();
        let rules = read_rules(&bytes);
        assert_eq!(rules.rule_count(), 2, "two CASM rules parse");

        let imported = import_style(&bytes).expect("imports");
        assert_eq!(imported.source_ppq, 1920);
        assert!((imported.tempo_bpm - 120.0).abs() < 0.001);
        assert_eq!(imported.time_signature, TimeSignature::default());
        assert_eq!(imported.section_names(), vec!["Main A", "Fill In AA"]);
        assert_eq!(imported.stats.markers_total, 3);
        assert_eq!(imported.stats.markers_skipped, 1, "SInt is not a section");
        assert_eq!(imported.stats.channels_without_rule, 0);

        let main = imported.native.section("Main A").expect("section");
        assert_eq!(main.length_ticks, 3840, "one bar at 960 PPQ");
        assert_eq!(main.parts.len(), 2);
        let bass = main
            .parts
            .iter()
            .find(|part| part.rule.dest_channel == 11)
            .expect("bass part");
        assert_eq!(bass.notes.len(), 1);
        assert_eq!(bass.notes[0].tick, 0);
        assert_eq!(bass.notes[0].duration, 960, "1920 source ticks halve");
        assert_eq!(bass.notes[0].pitch, 48);
        let drums = main
            .parts
            .iter()
            .find(|part| part.rule.dest_channel == 10)
            .expect("drum part");
        assert_eq!(drums.notes.len(), 1);
        assert_eq!(drums.notes[0].tick, 480);
        assert_eq!(drums.notes[0].duration, 240);

        let fill = imported.native.section("Fill In AA").expect("section");
        assert_eq!(fill.length_ticks, 3840, "last section rounds up to a bar");
        assert_eq!(fill.parts.len(), 1);
        assert_eq!(fill.parts[0].notes.len(), 1);
        assert_eq!(fill.parts[0].notes[0].tick, 0, "rebased to the section");
    }

    #[test]
    fn compiles_an_imported_section_at_the_source_chord() {
        let imported = import_style(&style_bytes()).expect("imports");
        let compiled = imported.native.compile("Main A", 0, &[]).expect("compiles");
        assert_eq!(compiled.len(), 2, "bass and drum both play");
        let bass = compiled
            .iter()
            .find(|note| note.channel.one_based() == 11)
            .expect("bass");
        assert_eq!(bass.pitch, 48, "C major 7 source chord keeps C");
    }

    #[test]
    fn a_style_without_matching_markers_is_rejected() {
        let mut track = Vec::new();
        marker(&mut track, 0, "Not A Section");
        track_event(&mut track, 0, &[0xFF, 0x2F, 0x00]);
        let mut out = Vec::new();
        out.extend_from_slice(&chunk(b"MThd", &[0, 0, 0, 1, 0x07, 0x80]));
        out.extend_from_slice(&chunk(b"MTrk", &track));
        let error = import_style(&out).expect_err("no sections");
        assert_eq!(error, StyleError::NoSections);
    }
}
