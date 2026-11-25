//! Writing a Yamaha style container: the MIDI song followed by `CASM`.
//!
//! Yamaha places `CASM` after the MIDI data in the same byte stream; the MIDI
//! reader stops at the end of the last track, and the CASM reader ignores the
//! MIDI bytes. Writing both parts together is therefore the whole container.

use project::song::Song;

use crate::casm::{CasmRules, SffVersion, write_casm_sff1, write_casm_sff2};
use crate::{ImportedStyle, StyleError};

/// Per-channel voice and mixer initialization written into a style's `SInt` setup bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetupVoice {
    /// Zero-based MIDI channel (`0..=15`).
    pub channel: u8,
    /// Bank select MSB (`CC 0`, `0..=127`).
    pub bank_msb: u8,
    /// Bank select LSB (`CC 32`, `0..=127`).
    pub bank_lsb: u8,
    /// Program number (`0..=127`).
    pub program: u8,
    /// Channel volume (`CC 7`, `0..=127`).
    pub volume: u8,
    /// Channel pan (`CC 10`, `0..=127`, `64` = center).
    pub pan: u8,
}

/// Extracts per-channel setup voices from a song's tracks for the `SInt` setup bar.
#[must_use]
pub fn setup_voices_from_song(song: &Song) -> Vec<SetupVoice> {
    let mut voices = Vec::new();
    let mut seen_channels = [false; 16];
    for track in &song.tracks {
        let channel = track
            .notes
            .first()
            .map_or(0, |note| note.channel.zero_based())
            .min(15);
        if seen_channels[usize::from(channel)] {
            continue;
        }
        let Some(program) = track.program else {
            if track.bank.is_none() && track.volume == 100 && track.pan == 64 {
                continue;
            }
            seen_channels[usize::from(channel)] = true;
            voices.push(SetupVoice {
                channel,
                bank_msb: track.bank.map_or(0, |bank| bank.msb.min(127)),
                bank_lsb: track.bank.map_or(0, |bank| bank.lsb.min(127)),
                program: 0,
                volume: track.volume.min(127),
                pan: track.pan.min(127),
            });
            continue;
        };
        seen_channels[usize::from(channel)] = true;
        voices.push(SetupVoice {
            channel,
            bank_msb: track.bank.map_or(0, |bank| bank.msb.min(127)),
            bank_lsb: track.bank.map_or(0, |bank| bank.lsb.min(127)),
            program: program.min(127),
            volume: track.volume.min(127),
            pan: track.pan.min(127),
        });
    }
    voices
}

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

/// Builds a Yamaha-compliant Format 0 style file with an explicit `SFF1`/`SFF2`
/// and `SInt` one-bar setup region containing per-channel Bank Select, Program
/// Change, Volume, and Pan setup events before the first musical section.
///
/// When `voices` is empty, setup voices are derived from `imported.song` via
/// [`setup_voices_from_song`].
///
/// # Errors
///
/// Returns [`StyleError::Midi`] when the MIDI track cannot be encoded.
pub fn write_style_with_setup(
    imported: &ImportedStyle,
    rules: &CasmRules,
    version: SffVersion,
    voices: &[SetupVoice],
) -> Result<Vec<u8>, StyleError> {
    let song = &imported.song;
    let derived;
    let voices = if voices.is_empty() {
        derived = setup_voices_from_song(song);
        derived.as_slice()
    } else {
        voices
    };

    let bar = song.time_signature.bar_ticks().max(1);
    let has_sint = song
        .markers
        .iter()
        .any(|marker| marker.name.eq_ignore_ascii_case("SInt"));
    let section_offset = if has_sint { 0 } else { bar };

    // Collect timed events: (tick, order, owned event)
    // Order: 0 = meta header, 1 = setup markers, 2 = setup CCs/programs,
    // 3 = section markers, 4 = note-offs, 5 = note-ons.
    enum OwnedEvent {
        Tempo(u32),
        TimeSig(u8, u8),
        Marker(Vec<u8>),
        Controller { ch: u8, cc: u8, val: u8 },
        Program { ch: u8, program: u8 },
        NoteOn { ch: u8, key: u8, vel: u8 },
        NoteOff { ch: u8, key: u8 },
    }

    let mut timed: Vec<(i64, u8, OwnedEvent)> = Vec::new();
    let den_pow = match song.time_signature.denominator {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        16 => 4,
        32 => 5,
        _ => 2,
    };
    timed.push((
        0,
        0,
        OwnedEvent::TimeSig(song.time_signature.numerator, den_pow),
    ));
    let bpm = song.tempo_map.bpm_at(0).clamp(5.0, 500.0);
    let micros = (60_000_000.0 / bpm).round() as u32;
    timed.push((0, 0, OwnedEvent::Tempo(micros)));

    let sff_tag = match version {
        SffVersion::Sff2 => b"SFF2".to_vec(),
        _ => b"SFF1".to_vec(),
    };
    if !song
        .markers
        .iter()
        .any(|marker| marker.name == "SFF1" || marker.name == "SFF2")
    {
        timed.push((0, 1, OwnedEvent::Marker(sff_tag)));
    }
    if !has_sint {
        timed.push((0, 1, OwnedEvent::Marker(b"SInt".to_vec())));
    }

    for voice in voices {
        let ch = voice.channel.min(15);
        timed.push((
            0,
            2,
            OwnedEvent::Controller {
                ch,
                cc: 0,
                val: voice.bank_msb.min(127),
            },
        ));
        timed.push((
            0,
            2,
            OwnedEvent::Controller {
                ch,
                cc: 32,
                val: voice.bank_lsb.min(127),
            },
        ));
        timed.push((
            0,
            2,
            OwnedEvent::Program {
                ch,
                program: voice.program.min(127),
            },
        ));
        timed.push((
            0,
            2,
            OwnedEvent::Controller {
                ch,
                cc: 7,
                val: voice.volume.min(127),
            },
        ));
        timed.push((
            0,
            2,
            OwnedEvent::Controller {
                ch,
                cc: 10,
                val: voice.pan.min(127),
            },
        ));
    }

    for marker in &song.markers {
        let at = if marker.name.eq_ignore_ascii_case("SInt")
            || marker.name == "SFF1"
            || marker.name == "SFF2"
        {
            marker.tick.max(0)
        } else {
            marker.tick.max(0) + section_offset
        };
        timed.push((at, 3, OwnedEvent::Marker(marker.name.as_bytes().to_vec())));
    }

    for track in &song.tracks {
        for note in &track.notes {
            let start = note.start.max(0) + section_offset;
            let end = note.end().max(note.start.max(0) + 1) + section_offset;
            let ch = note.channel.zero_based().min(15);
            timed.push((
                start,
                5,
                OwnedEvent::NoteOn {
                    ch,
                    key: note.pitch.as_u8().min(127),
                    vel: note.velocity.as_u8().clamp(1, 127),
                },
            ));
            timed.push((
                end,
                4,
                OwnedEvent::NoteOff {
                    ch,
                    key: note.pitch.as_u8().min(127),
                },
            ));
        }
    }

    timed.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

    let mut track = Vec::with_capacity(timed.len() * 6 + 4);
    let mut last_tick = 0_i64;
    for (tick, _, event) in &timed {
        let delta = u32::try_from((*tick - last_tick).max(0)).unwrap_or(0);
        last_tick = *tick;
        push_vlq(&mut track, delta);
        match event {
            OwnedEvent::Tempo(us) => {
                let clamped = (*us).min(0xFF_FFFF);
                track.extend_from_slice(&[
                    0xFF,
                    0x51,
                    0x03,
                    ((clamped >> 16) & 0xFF) as u8,
                    ((clamped >> 8) & 0xFF) as u8,
                    (clamped & 0xFF) as u8,
                ]);
            }
            OwnedEvent::TimeSig(num, pow) => {
                track.extend_from_slice(&[0xFF, 0x58, 0x04, *num, *pow, 24, 8]);
            }
            OwnedEvent::Marker(text) => {
                track.extend_from_slice(&[0xFF, 0x06]);
                push_vlq(&mut track, u32::try_from(text.len()).unwrap_or(0));
                track.extend_from_slice(text);
            }
            OwnedEvent::Controller { ch, cc, val } => {
                track.extend_from_slice(&[0xB0 | (*ch & 0x0F), *cc & 0x7F, *val & 0x7F]);
            }
            OwnedEvent::Program { ch, program } => {
                track.extend_from_slice(&[0xC0 | (*ch & 0x0F), *program & 0x7F]);
            }
            OwnedEvent::NoteOn { ch, key, vel } => {
                track.extend_from_slice(&[0x90 | (*ch & 0x0F), *key & 0x7F, *vel & 0x7F]);
            }
            OwnedEvent::NoteOff { ch, key } => {
                track.extend_from_slice(&[0x80 | (*ch & 0x0F), *key & 0x7F, 0]);
            }
        }
    }
    track.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

    let track_len = u32::try_from(track.len())
        .map_err(|_| StyleError::Midi(String::from("MIDI track exceeds 4 GiB")))?;
    let ppq = u16::try_from(music_core::PPQ).unwrap_or(480);
    let mut bytes = Vec::with_capacity(14 + 8 + track.len() + 256);
    bytes.extend_from_slice(b"MThd");
    bytes.extend_from_slice(&6_u32.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes()); // Format 0 (SingleTrack)
    bytes.extend_from_slice(&1_u16.to_be_bytes()); // 1 track
    bytes.extend_from_slice(&ppq.to_be_bytes());
    bytes.extend_from_slice(b"MTrk");
    bytes.extend_from_slice(&track_len.to_be_bytes());
    bytes.extend_from_slice(&track);
    let casm = match version {
        SffVersion::Sff2 => write_casm_sff2(&rules.groups),
        _ => write_casm_sff1(&rules.groups),
    };
    bytes.extend_from_slice(&casm);
    Ok(bytes)
}

fn push_vlq(out: &mut Vec<u8>, mut value: u32) {
    let mut buf = [0_u8; 5];
    let mut len = 1;
    buf[0] = (value & 0x7F) as u8;
    value >>= 7;
    while value > 0 {
        buf[len] = ((value & 0x7F) as u8) | 0x80;
        value >>= 7;
        len += 1;
    }
    for byte in buf[..len].iter().rev() {
        out.push(*byte);
    }
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
        track.program = Some(40);
        track.bank = Some(project::song::BankSelect { msb: 63, lsb: 1 });
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
        assert_eq!(imported.song.tracks[0].program, Some(40));
        assert_eq!(
            imported.song.tracks[0].bank,
            Some(project::song::BankSelect { msb: 63, lsb: 1 })
        );
        let (song, _) = smf::decode(&written).expect("midi part still decodes");
        assert_eq!(song.markers.len(), imported.song.markers.len());
        assert_eq!(song.tracks.len(), imported.song.tracks.len());
        assert_eq!(song.tracks[0].program, Some(40));
        assert_eq!(
            song.tracks[0].bank,
            Some(project::song::BankSelect { msb: 63, lsb: 1 })
        );
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

    #[test]
    fn write_style_with_setup_emits_sint_voices_and_reimports_cleanly() {
        let source = source();
        let imported = import_style(&source).expect("import");
        let rules = read_rules(&source);
        let bytes =
            write_style_with_setup(&imported, &rules, SffVersion::Sff2, &[]).expect("write");

        // Verify Format 0 MIDI header (`MThd`, len 6, format 0, 1 track).
        assert_eq!(&bytes[0..12], b"MThd\x00\x00\x00\x06\x00\x00\x00\x01");
        assert!(
            bytes.windows(7).any(|w| w == b"\xFF\x06\x04SFF2"),
            "SFF2 marker missing"
        );
        assert!(
            bytes.windows(7).any(|w| w == b"\xFF\x06\x04SInt"),
            "SInt setup marker missing"
        );
        assert!(
            bytes.windows(3).any(|w| w == [0xB0, 0, 63]),
            "Bank MSB missing"
        );
        assert!(
            bytes.windows(3).any(|w| w == [0xB0, 32, 1]),
            "Bank LSB missing"
        );
        assert!(
            bytes.windows(2).any(|w| w == [0xC0, 40]),
            "Program change missing"
        );
        assert!(
            bytes.windows(3).any(|w| w == [0xB0, 7, 100]),
            "Volume CC 7 missing"
        );
        assert!(
            bytes.windows(3).any(|w| w == [0xB0, 10, 64]),
            "Pan CC 10 missing"
        );

        // Decode through smf and re-import through import_style.
        let (decoded, _) = smf::decode(&bytes).expect("valid smf");
        assert!(decoded.markers.iter().any(|m| m.name == "SFF2"));
        assert!(decoded.markers.iter().any(|m| m.name == "SInt"));
        assert_eq!(decoded.tracks[0].program, Some(40));
        assert_eq!(
            decoded.tracks[0].bank,
            Some(project::song::BankSelect { msb: 63, lsb: 1 })
        );

        let reimported = import_style(&bytes).expect("reimport");
        assert_eq!(reimported.section_names(), vec!["Main A"]);
        assert_eq!(reimported.sections[0].notes, 1);
        assert_eq!(read_rules(&bytes).version, SffVersion::Sff2);
    }
}
