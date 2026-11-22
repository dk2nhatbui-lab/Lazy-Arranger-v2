//! Writing a Yamaha style container: the MIDI song followed by `CASM`.
//!
//! Yamaha places `CASM` after the MIDI data in the same byte stream; the MIDI
//! reader stops at the end of the last track, and the CASM reader ignores the
//! MIDI bytes. Writing both parts together is therefore the whole container.

use project::song::Song;

use crate::casm::{CasmRules, SffVersion, write_casm_sff1, write_casm_sff2};
use crate::{ImportedStyle, StyleError};

/// Builds a style file: the imported MIDI song, then the CASM chunk.
///
/// # Errors
///
/// Returns [`StyleError::Midi`] when the song cannot be encoded.
pub fn write_style(
    imported: &ImportedStyle,
    rules: &CasmRules,
    version: SffVersion,
) -> Result<Vec<u8>, StyleError> {
    let song: &Song = &imported.song;
    let mut bytes = smf::encode(song).map_err(|error| StyleError::Midi(error.to_string()))?;
    let casm = match version {
        SffVersion::Sff2 => write_casm_sff2(&rules.groups),
        _ => write_casm_sff1(&rules.groups),
    };
    bytes.extend_from_slice(&casm);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casm::{SectionRules, SffVersion, read_rules, write_casm_sff1};
    use crate::import_style;
    use arranger::rules::{ChordRule, Ntr, Ntt};
    use music_core::PPQ;
    use project::ids::{Marker, NoteId, TrackId};
    use project::song::{Note, Track};

    fn rule() -> ChordRule {
        let mut rule = ChordRule::new(11, 11);
        rule.name = String::from("Bass");
        rule.transpose.ntr = Ntr::RootFixed;
        rule.transpose.ntt = Ntt::Melody;
        rule.transpose.bass = true;
        rule.transpose.high_key = 6;
        rule.transpose.high_limit = 127;
        rule
    }

    /// A container built from a synthetic song: MIDI with markers, then CASM.
    fn source() -> Vec<u8> {
        let mut song = Song::new("Fixture", 120.0);
        song.markers.push(Marker {
            tick: 0,
            name: String::from("Main A"),
        });
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Bass");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: music_core::Pitch::from_name("C2").expect("valid"),
            velocity: music_core::Velocity::new(100).expect("valid"),
            channel: music_core::Channel::first(),
        });
        song.tracks.push(track);
        let mut bytes = smf::encode(&song).expect("midi");
        bytes.extend_from_slice(&write_casm_sff1(&[SectionRules {
            sections: vec![String::from("Main A")],
            rules: vec![rule()],
        }]));
        bytes
    }

    #[test]
    fn a_written_style_reads_back_as_midi_and_casm() {
        let source = source();
        let imported = import_style(&source).expect("import");
        let rules = read_rules(&source);
        let written = write_style(&imported, &rules, SffVersion::Sff2).expect("write");
        assert!(
            crate::scan_chunks(&written)
                .expect("chunks")
                .iter()
                .any(|chunk| &chunk.id == b"CASM")
        );
        let casm = read_rules(&written);
        assert!(casm.has_casm);
        assert_eq!(casm.version, SffVersion::Sff2);
        assert!(casm.groups.iter().any(|group| group.sections == ["Main A"]));
        let (song, _) = smf::decode(&written).expect("midi part still decodes");
        assert_eq!(song.markers.len(), imported.song.markers.len());
        assert_eq!(song.tracks.len(), imported.song.tracks.len());
    }

    #[test]
    fn sff1_writes_ctab_and_sff2_writes_ctb2() {
        let source = source();
        let imported = import_style(&source).expect("import");
        let rules = read_rules(&source);
        let sff1 = write_style(&imported, &rules, SffVersion::Sff1).expect("write");
        assert_eq!(read_rules(&sff1).version, SffVersion::Sff1);
        let sff2 = write_style(&imported, &rules, SffVersion::Sff2).expect("write");
        assert_eq!(read_rules(&sff2).version, SffVersion::Sff2);
    }
}
