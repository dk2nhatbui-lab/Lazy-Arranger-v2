//! Keyboard song export for the Yamaha PSR-S970.
//!
//! The keyboard expects a setup bar before the music: tempo, meter, the voice
//! (program change), volume and pan for every part, and lyrics and chord names
//! as text events. Notes start one bar later and drum parts are forced onto
//! channel 10. The output is a standard MIDI file the keyboard's Song player
//! can read; it is not a Yamaha style file.

use music_core::{PPQ, Tick};
use project::song::Song;

/// One part's keyboard setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    /// MIDI channel, 0..=15.
    pub channel: u8,
    /// Program number, 0..=127.
    pub program: u8,
    /// Initial volume, 0..=127.
    pub volume: u8,
    /// Initial pan, 0..=127.
    pub pan: u8,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            channel: 0,
            program: 0,
            volume: 100,
            pan: 64,
        }
    }
}

/// What the export contains and what it had to change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Music tracks exported.
    pub tracks: usize,
    /// Notes written.
    pub notes: usize,
    /// Ticks added to move the music after the setup bar.
    pub shift: Tick,
    /// Chord names written as markers.
    pub chords: usize,
    /// Lyric lines written.
    pub lyrics: usize,
    /// Notes dropped because their channel or pitch was invalid.
    pub dropped: usize,
}

/// Exports a song for the keyboard.
#[must_use]
pub fn export(song: &Song, voices: &[Voice], lyrics: &[(Tick, String)]) -> (Vec<u8>, Report) {
    let bar = song.time_signature.bar_ticks().max(1);
    let mut report = Report {
        tracks: song.tracks.len(),
        shift: bar,
        ..Report::default()
    };
    let mut tracks: Vec<Vec<u8>> = Vec::new();

    // Setup track.
    let mut setup: Vec<(Tick, Vec<u8>)> = Vec::new();
    setup.push((0, meta_text(0x03, "Setup")));
    let bpm = song.tempo_map.bpm_at(0);
    let microseconds = (60_000_000.0 / bpm.max(1.0))
        .round()
        .clamp(1.0, 16_777_215.0) as u32;
    setup.push((
        0,
        vec![
            0xff,
            0x51,
            0x03,
            (microseconds >> 16) as u8,
            (microseconds >> 8) as u8,
            microseconds as u8,
        ],
    ));
    setup.push((
        0,
        vec![
            0xff,
            0x58,
            0x04,
            song.time_signature.numerator,
            song.time_signature.denominator.trailing_zeros() as u8,
            24,
            8,
        ],
    ));
    for (index, voice) in voices.iter().enumerate() {
        let channel = voice.channel & 0x0f;
        setup.push((0, vec![0xc0 | channel, voice.program & 0x7f]));
        setup.push((0, vec![0xb0 | channel, 7, voice.volume & 0x7f]));
        setup.push((0, vec![0xb0 | channel, 10, voice.pan & 0x7f]));
        let _ = index;
    }
    for (tick, text) in lyrics {
        setup.push((*tick + bar, meta_text(0x05, text)));
        report.lyrics += 1;
    }
    for event in &song.chords {
        let text = format!("Chord: {}", event.chord.format(music_core::Spelling::Sharp));
        setup.push((event.tick + bar, meta_text(0x06, &text)));
        report.chords += 1;
    }
    setup.sort_by_key(|(tick, _)| *tick);
    tracks.push(encode_track(&setup, 0));

    // Music tracks.
    for (index, track) in song.tracks.iter().enumerate() {
        let voice = voices.get(index).copied().unwrap_or_default();
        let drums = track
            .notes
            .iter()
            .any(|note| note.channel.zero_based() == 9)
            || voice.channel == 9;
        let channel = if drums { 9 } else { voice.channel & 0x0f };
        let mut events: Vec<(Tick, Vec<u8>)> = Vec::new();
        events.push((0, meta_text(0x03, &track.name)));
        for note in &track.notes {
            let pitch = note.pitch.as_u8();
            if pitch > 127 {
                report.dropped += 1;
                continue;
            }
            let start = note.start + bar;
            let stop = start + note.duration.max(1);
            events.push((
                start,
                vec![0x90 | channel, pitch, note.velocity.as_u8().min(127)],
            ));
            events.push((stop, vec![0x80 | channel, pitch, 64]));
            report.notes += 1;
        }
        events.sort_by_key(|(tick, _)| *tick);
        tracks.push(encode_track(&events, channel));
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"MThd");
    bytes.extend_from_slice(&6u32.to_be_bytes());
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    bytes.extend_from_slice(&(PPQ as u16).to_be_bytes());
    for track in &tracks {
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&(track.len() as u32).to_be_bytes());
        bytes.extend_from_slice(track);
    }
    (bytes, report)
}

fn meta_text(kind: u8, text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut event = vec![0xff, kind];
    event.extend_from_slice(&varint(bytes.len() as u32));
    event.extend_from_slice(bytes);
    event
}

fn encode_track(events: &[(Tick, Vec<u8>)], _channel: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut last = 0;
    for (tick, event) in events {
        out.extend_from_slice(&varint((tick - last).max(0) as u32));
        out.extend_from_slice(event);
        last = *tick;
    }
    out.extend_from_slice(&[0x00, 0xff, 0x2f, 0x00]);
    out
}

fn varint(mut value: u32) -> Vec<u8> {
    let mut buffer = vec![(value & 0x7f) as u8];
    value >>= 7;
    while value > 0 {
        buffer.insert(0, ((value & 0x7f) as u8) | 0x80);
        value >>= 7;
    }
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, Chord, Pitch, Velocity};
    use project::song::{ChordEvent, Note, Track};
    use project::{NoteId, TrackId};

    fn song() -> Song {
        let mut song = Song::new("Keyboard", 120.0);
        song.chords.push(ChordEvent {
            tick: 0,
            chord: Chord::parse("C").expect("valid"),
        });
        let mut piano = Track::new(TrackId::new(1).expect("valid"), "Piano");
        piano.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        let mut drums = Track::new(TrackId::new(2).expect("valid"), "Drums");
        drums.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 120,
            pitch: Pitch::new(38).expect("valid"),
            velocity: Velocity::new(110).expect("valid"),
            channel: Channel::new(9).expect("valid"),
        });
        song.tracks.push(piano);
        song.tracks.push(drums);
        song
    }

    fn contains(bytes: &[u8], needle: &[u8]) -> bool {
        bytes.windows(needle.len()).any(|window| window == needle)
    }

    #[test]
    fn the_file_is_a_format_one_midi_file_with_a_setup_track() {
        let voices = [
            Voice {
                channel: 0,
                program: 0,
                volume: 100,
                pan: 64,
            },
            Voice {
                channel: 9,
                program: 0,
                volume: 110,
                pan: 64,
            },
        ];
        let (bytes, report) = export(&song(), &voices, &[(0, String::from("la"))]);
        assert_eq!(&bytes[0..4], b"MThd");
        assert_eq!(u16::from_be_bytes([bytes[8], bytes[9]]), 1);
        assert_eq!(
            u16::from_be_bytes([bytes[10], bytes[11]]),
            3,
            "setup + two parts"
        );
        assert_eq!(report.tracks, 2);
        assert_eq!(report.notes, 2);
        assert_eq!(report.chords, 1);
        assert_eq!(report.lyrics, 1);
        assert_eq!(report.shift, 3840);
        assert!(contains(&bytes, &[0xc0, 0]));
        assert!(contains(&bytes, &[0xb9, 7, 110]));
        assert!(
            contains(&bytes, &[0x99, 38, 110]),
            "drums forced to channel 10"
        );
        assert!(contains(&bytes, &[0x90, 60, 100]));
        assert!(contains(&bytes, b"Chord: C"));
        assert!(contains(&bytes, &[0xff, 0x58, 0x04, 4, 2]));
    }

    #[test]
    fn the_music_starts_after_the_setup_bar() {
        let (bytes, _) = export(&song(), &[Voice::default()], &[]);
        let decoded = crate::decode(&bytes).expect("midi");
        let piano = decoded
            .0
            .tracks
            .iter()
            .find(|track| track.name == "Piano")
            .expect("piano");
        assert_eq!(piano.notes[0].start, 3840);
        let marker = decoded
            .0
            .markers
            .iter()
            .find(|marker| marker.name.starts_with("Chord:"))
            .expect("chord marker");
        assert_eq!(marker.tick, 3840);
    }

    #[test]
    fn velocity_and_channels_survive_the_writer() {
        let voices = [Voice {
            channel: 2,
            program: 40,
            volume: 90,
            pan: 30,
        }];
        let (bytes, _) = export(&song(), &voices, &[]);
        assert!(contains(&bytes, &[0xc2, 40]));
        assert!(contains(&bytes, &[0xb2, 10, 30]));
    }
}
