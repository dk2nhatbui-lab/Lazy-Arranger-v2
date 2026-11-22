//! Standard MIDI File reading and writing for the project schema.
//!
//! The writer always emits format 1 (a conductor track plus one track per
//! project track) at the project's fixed 960 PPQ. The reader accepts any
//! metrical resolution and rescales ticks to 960 with integer half-up
//! rounding, which is what the C++ oracle does (`src/core/src/MidiFile.cpp`).
//!
//! Deliberate limits, each surfaced in [`DecodeReport`]:
//!
//! * SysEx and escape events are ignored.
//! * A note-on without a matching note-off is dropped and counted.
//! * One tempo map and one time signature are read; a later time-signature
//!   change is reported, not applied.
//! * Tracks with no notes are not imported, so a conductor track does not
//!   become an empty project track.

use midly::num::{u4, u7, u15, u24, u28};
use midly::{
    Format, Header, MetaMessage, MidiMessage as Midi, Smf, Timing, TrackEvent, TrackEventKind,
};
use music_core::{Channel, PPQ, Pitch, TempoChange, TempoMap, Tick, TimeSignature, Velocity};
use project::ids::{Marker, NoteId, TrackId};
use project::song::{Note, Song, Track};
use thiserror::Error;

pub mod keyboard;
pub use keyboard::{Report as KeyboardReport, Voice as KeyboardVoice, export as export_keyboard};

/// Ticks per quarter note used by every encoded file.
pub const EXPORT_PPQ: u16 = 960;

/// Errors raised while encoding or decoding a MIDI file.
#[derive(Debug, Error)]
pub enum SmfError {
    /// The byte stream was not a valid standard MIDI file.
    #[error("MIDI file could not be parsed: {0}")]
    Parse(String),
    /// The file used SMPTE timecode timing instead of musical time.
    #[error("MIDI file uses SMPTE timecode timing; only metrical files are supported")]
    SmpteTiming,
    /// The writer failed.
    #[error("MIDI file could not be encoded: {0}")]
    Io(#[from] std::io::Error),
    /// A tempo could not be represented as microseconds per quarter note.
    #[error("tempo {bpm} bpm cannot be written as MIDI microseconds per quarter")]
    Tempo {
        /// The offending tempo.
        bpm: f64,
    },
    /// A source value did not fit the target type.
    #[error("MIDI value is out of range: {0}")]
    Range(String),
}

/// What the reader had to approximate or drop.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DecodeReport {
    /// Number of MIDI track chunks read.
    pub source_tracks: usize,
    /// Notes carried into the song.
    pub notes: usize,
    /// Note-ons with no matching note-off.
    pub unpaired_note_ons: usize,
    /// Note-offs for a pitch that was not sounding.
    pub unmatched_note_offs: usize,
    /// SysEx or escape events that were skipped.
    pub skipped_system_events: usize,
    /// Ticks per quarter note in the source file.
    pub source_ppq: u32,
    /// True when the source resolution was not 960 PPQ and ticks were rescaled.
    pub rescaled: bool,
    /// Human-readable notes about lossy conversion.
    pub warnings: Vec<String>,
}

impl DecodeReport {
    /// Renders the report as text for a CLI or log.
    #[must_use]
    pub fn to_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "read {} track chunk(s), {} note(s){}",
            self.source_tracks,
            self.notes,
            if self.rescaled {
                ", ticks rescaled to 960 PPQ"
            } else {
                ""
            }
        );
        if self.unpaired_note_ons > 0 {
            let _ = writeln!(
                out,
                "  dropped {} note-on(s) with no note-off",
                self.unpaired_note_ons
            );
        }
        if self.unmatched_note_offs > 0 {
            let _ = writeln!(
                out,
                "  ignored {} note-off(s) with no note-on",
                self.unmatched_note_offs
            );
        }
        if self.skipped_system_events > 0 {
            let _ = writeln!(
                out,
                "  skipped {} SysEx or escape event(s)",
                self.skipped_system_events
            );
        }
        for warning in &self.warnings {
            let _ = writeln!(out, "  warning: {warning}");
        }
        out
    }
}

/// Encodes a song as a standard MIDI file.
///
/// # Errors
///
/// Returns [`SmfError::Tempo`] for a tempo that cannot be stored as
/// microseconds per quarter note.
pub fn encode(song: &Song) -> Result<Vec<u8>, SmfError> {
    let mut tracks: Vec<Vec<TrackEvent<'_>>> = Vec::with_capacity(song.tracks.len() + 1);

    let mut conductor = TrackBuilder::new("Conductor");
    conductor.push(
        0,
        1,
        TrackEventKind::Meta(MetaMessage::TimeSignature(
            song.time_signature.numerator,
            song.time_signature.denominator.trailing_zeros() as u8,
            24,
            8,
        )),
    );
    for change in song.tempo_map.changes() {
        conductor.push(
            change.tick,
            0,
            TrackEventKind::Meta(MetaMessage::Tempo(tempo_micros(change.bpm)?)),
        );
    }
    for marker in &song.markers {
        conductor.push(
            marker.tick,
            2,
            TrackEventKind::Meta(MetaMessage::Marker(marker.name.as_bytes())),
        );
    }
    tracks.push(conductor.finish());

    for track in &song.tracks {
        let mut builder = TrackBuilder::new(track.name.as_str());
        for note in &track.notes {
            builder.push(
                note.start,
                3,
                TrackEventKind::Midi {
                    channel: channel_to_u4(note.channel),
                    message: Midi::NoteOn {
                        key: key_to_u7(note.pitch),
                        vel: velocity_to_u7(note.velocity),
                    },
                },
            );
            builder.push(
                note.end(),
                0,
                TrackEventKind::Midi {
                    channel: channel_to_u4(note.channel),
                    message: Midi::NoteOff {
                        key: key_to_u7(note.pitch),
                        vel: u7::new(0),
                    },
                },
            );
        }
        tracks.push(builder.finish());
    }

    let header = Header::new(Format::Parallel, Timing::Metrical(u15::new(EXPORT_PPQ)));
    let mut smf = Smf::new(header);
    smf.tracks = tracks;
    let mut bytes = Vec::new();
    smf.write_std(&mut bytes)?;
    Ok(bytes)
}

/// A note collected while reading, before identifiers are assigned.
struct RawNote {
    start: Tick,
    duration: Tick,
    pitch: Pitch,
    velocity: Velocity,
    channel: Channel,
}

/// Decodes a standard MIDI file into a song.
///
/// # Errors
///
/// Returns [`SmfError::Parse`] for malformed bytes and
/// [`SmfError::SmpteTiming`] for SMPTE-timed files.
pub fn decode(bytes: &[u8]) -> Result<(Song, DecodeReport), SmfError> {
    let smf = Smf::parse(bytes).map_err(|err| SmfError::Parse(err.to_string()))?;
    let source_ppq = match smf.header.timing {
        Timing::Metrical(ppq) => u32::from(ppq.as_int()),
        Timing::Timecode(_, _) => return Err(SmfError::SmpteTiming),
    };
    let mut report = DecodeReport {
        source_tracks: smf.tracks.len(),
        source_ppq,
        rescaled: source_ppq != u32::from(EXPORT_PPQ),
        ..DecodeReport::default()
    };

    let mut tempo_changes: Vec<TempoChange> = Vec::new();
    let mut time_signature: Option<TimeSignature> = None;
    let mut markers: Vec<Marker> = Vec::new();
    let mut tracks: Vec<Track> = Vec::new();
    let mut next_track_id: u32 = 1;

    for (index, events) in smf.tracks.iter().enumerate() {
        let mut absolute: u64 = 0;
        let mut name: Option<String> = None;
        let mut sounding: Vec<(u8, u8, Tick, Velocity)> = Vec::new();
        let mut notes: Vec<RawNote> = Vec::new();

        for event in events {
            absolute += u64::from(event.delta.as_int());
            let tick = rescale_tick(absolute, source_ppq)?;
            match event.kind {
                TrackEventKind::Midi { channel, message } => match message {
                    Midi::NoteOn { key, vel } if vel.as_int() > 0 => {
                        let velocity = Velocity::new(vel.as_int())
                            .map_err(|err| SmfError::Range(err.to_string()))?;
                        sounding.push((channel.as_int(), key.as_int(), tick, velocity));
                    }
                    Midi::NoteOn { key, .. } => close_note(
                        &mut sounding,
                        &mut notes,
                        channel.as_int(),
                        key.as_int(),
                        tick,
                        &mut report,
                    ),
                    Midi::NoteOff { key, .. } => close_note(
                        &mut sounding,
                        &mut notes,
                        channel.as_int(),
                        key.as_int(),
                        tick,
                        &mut report,
                    ),
                    _ => {}
                },
                TrackEventKind::Meta(meta) => match meta {
                    MetaMessage::Tempo(micros) => {
                        let bpm = 60_000_000.0 / f64::from(micros.as_int());
                        tempo_changes.push(TempoChange { tick, bpm });
                    }
                    MetaMessage::TimeSignature(numerator, power, _, _) => {
                        let denominator = 1u8.checked_shl(u32::from(power)).unwrap_or(0);
                        match TimeSignature::new(numerator, denominator) {
                            Ok(signature) if time_signature.is_none() => {
                                time_signature = Some(signature);
                            }
                            Ok(signature) if time_signature == Some(signature) => {}
                            Ok(_) => report.warnings.push(format!(
                                "time signature change to {numerator}/{denominator} at tick {tick} ignored; one signature per song"
                            )),
                            Err(err) => report.warnings.push(format!("{err} at tick {tick}")),
                        }
                    }
                    MetaMessage::TrackName(text) => {
                        name = Some(String::from_utf8_lossy(text).into_owned());
                    }
                    MetaMessage::Marker(text) => markers.push(Marker {
                        tick,
                        name: String::from_utf8_lossy(text).into_owned(),
                    }),
                    MetaMessage::EndOfTrack => {}
                    _ => {}
                },
                TrackEventKind::SysEx(_) | TrackEventKind::Escape(_) => {
                    report.skipped_system_events += 1;
                }
            }
        }
        report.unpaired_note_ons += sounding.len();
        if notes.is_empty() {
            continue;
        }
        notes.sort_by(|left, right| {
            (left.start, left.channel, left.pitch).cmp(&(right.start, right.channel, right.pitch))
        });
        let track_id =
            TrackId::new(next_track_id).map_err(|err| SmfError::Range(err.to_string()))?;
        next_track_id += 1;
        let mut track = Track::new(
            track_id,
            name.unwrap_or_else(|| format!("Track {}", index + 1)),
        );
        for (note_index, raw) in notes.into_iter().enumerate() {
            let id = NoteId::new(u32::try_from(note_index + 1).unwrap_or(u32::MAX))
                .map_err(|err| SmfError::Range(err.to_string()))?;
            track.notes.push(Note {
                id,
                start: raw.start,
                duration: raw.duration,
                pitch: raw.pitch,
                velocity: raw.velocity,
                channel: raw.channel,
            });
        }
        report.notes += track.notes.len();
        tracks.push(track);
    }

    if tempo_changes.is_empty() {
        report
            .warnings
            .push(String::from("no tempo change in file; using 120 bpm"));
        tempo_changes.push(TempoChange {
            tick: 0,
            bpm: 120.0,
        });
    }
    tempo_changes.sort_by_key(|change| change.tick);
    tempo_changes.dedup_by_key(|change| change.tick);
    if tempo_changes[0].tick != 0 {
        let first = tempo_changes[0];
        tempo_changes.insert(
            0,
            TempoChange {
                tick: 0,
                bpm: first.bpm,
            },
        );
    }
    let tempo_map = TempoMap::new(tempo_changes).map_err(|err| SmfError::Parse(err.to_string()))?;
    markers.sort_by_key(|marker| marker.tick);

    let song = Song {
        scene_names: Vec::new(),
        marks: Vec::new(),
        lyrics: Vec::new(),
        name: String::new(),
        tempo_map,
        time_signature: time_signature.unwrap_or_default(),
        tracks,
        chords: Vec::new(),
        arranger: Vec::new(),
        markers,
        extra: Default::default(),
    };
    Ok((song, report))
}

fn close_note(
    sounding: &mut Vec<(u8, u8, Tick, Velocity)>,
    notes: &mut Vec<RawNote>,
    channel: u8,
    key: u8,
    tick: Tick,
    report: &mut DecodeReport,
) {
    let Some(position) = sounding
        .iter()
        .rposition(|(ch, pitch, _, _)| *ch == channel && *pitch == key)
    else {
        report.unmatched_note_offs += 1;
        return;
    };
    let (_, _, start, velocity) = sounding.remove(position);
    let Some(duration) = tick.checked_sub(start).filter(|duration| *duration > 0) else {
        report.unmatched_note_offs += 1;
        return;
    };
    let (Ok(channel), Ok(pitch)) = (Channel::new(channel), Pitch::new(key)) else {
        report.unmatched_note_offs += 1;
        return;
    };
    notes.push(RawNote {
        start,
        duration,
        pitch,
        velocity,
        channel,
    });
}

/// Rescales a source tick to 960 PPQ with integer half-up rounding.
fn rescale_tick(tick: u64, source_ppq: u32) -> Result<Tick, SmfError> {
    if source_ppq == u32::try_from(PPQ).unwrap_or(u32::MAX) {
        return Tick::try_from(tick).map_err(|_| SmfError::Range(format!("tick {tick}")));
    }
    let numerator = u128::from(tick) * u128::from(EXPORT_PPQ);
    let half = u128::from(source_ppq) / 2;
    let scaled = (numerator + half) / u128::from(source_ppq);
    Tick::try_from(scaled).map_err(|_| SmfError::Range(format!("tick {tick} after rescaling")))
}

fn tempo_micros(bpm: f64) -> Result<u24, SmfError> {
    if !bpm.is_finite() || bpm <= 0.0 {
        return Err(SmfError::Tempo { bpm });
    }
    let micros = (60_000_000.0 / bpm).round();
    if !(1.0..=f64::from(u24::max_value().as_int())).contains(&micros) {
        return Err(SmfError::Tempo { bpm });
    }
    u24::try_from(micros as u32).ok_or(SmfError::Tempo { bpm })
}

fn channel_to_u4(channel: Channel) -> u4 {
    u4::new(channel.zero_based())
}

fn key_to_u7(pitch: Pitch) -> u7 {
    u7::new(pitch.as_u8())
}

fn velocity_to_u7(velocity: Velocity) -> u7 {
    u7::new(velocity.as_u8())
}

/// Collects absolute-tick events and emits them as delta-tick MIDI events.
struct TrackBuilder<'a> {
    name: &'a str,
    events: Vec<(Tick, u8, TrackEventKind<'a>)>,
}

impl<'a> TrackBuilder<'a> {
    fn new(name: &'a str) -> Self {
        Self {
            name,
            events: Vec::new(),
        }
    }

    fn push(&mut self, tick: Tick, order: u8, kind: TrackEventKind<'a>) {
        self.events.push((tick.max(0), order, kind));
    }

    fn finish(mut self) -> Vec<TrackEvent<'a>> {
        self.events.sort_by_key(|(tick, order, _)| (*tick, *order));
        let mut out = Vec::with_capacity(self.events.len() + 2);
        out.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(self.name.as_bytes())),
        });
        let mut previous: Tick = 0;
        let max_delta = Tick::from(u28::max_value().as_int());
        for (tick, _, kind) in self.events {
            let delta = (tick - previous).min(max_delta);
            previous = tick;
            let delta = u32::try_from(delta).unwrap_or(u32::MAX);
            out.push(TrackEvent {
                delta: u28::from_int_lossy(delta),
                kind,
            });
        }
        out.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        out
    }
}

/// A note as a caller-facing tuple, used by tests and diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteTuple {
    /// Start tick.
    pub start: Tick,
    /// Length in ticks.
    pub duration: Tick,
    /// MIDI note number.
    pub pitch: u8,
    /// Channel, zero-based.
    pub channel: u8,
}

impl NoteTuple {
    /// Extracts the tuple from a note.
    #[must_use]
    pub fn from_note(note: &Note) -> Self {
        Self {
            start: note.start,
            duration: note.duration,
            pitch: note.pitch.as_u8(),
            channel: note.channel.zero_based(),
        }
    }
}

/// Note tuples per track, in track order.
#[must_use]
pub fn notes_of(song: &Song) -> Vec<Vec<NoteTuple>> {
    song.tracks
        .iter()
        .map(|track| track.notes.iter().map(NoteTuple::from_note).collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_song() -> Song {
        let mut song = Song::new("Round trip", 120.0);
        song.time_signature = TimeSignature::new(6, 8).expect("valid");
        song.markers.push(Marker {
            tick: 0,
            name: String::from("Intro"),
        });
        song.markers.push(Marker {
            tick: 1920,
            name: String::from("Verse"),
        });
        let mut piano = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, (start, pitch, channel)) in [
            (0i64, "C4", 1u8),
            (960, "E4", 1),
            (1920, "G4", 1),
            (2880, "C5", 1),
        ]
        .into_iter()
        .enumerate()
        {
            piano.notes.push(Note {
                id: NoteId::new(u32::try_from(index + 1).expect("small")).expect("valid"),
                start,
                duration: 480,
                pitch: Pitch::from_name(pitch).expect("valid"),
                velocity: Velocity::new(96).expect("valid"),
                channel: Channel::from_one_based(channel).expect("valid"),
            });
        }
        song.tracks.push(piano);
        let mut drums = Track::new(TrackId::new(2).expect("valid"), "Drums");
        drums.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 120,
            pitch: Pitch::from_name("C2").expect("valid"),
            velocity: Velocity::new(120).expect("valid"),
            channel: Channel::drums(),
        });
        song.tracks.push(drums);
        song
    }

    #[test]
    fn round_trip_preserves_notes_tempo_and_markers() {
        let song = example_song();
        let bytes = encode(&song).expect("encode");
        let (back, report) = decode(&bytes).expect("decode");
        assert_eq!(report.notes, 5);
        assert_eq!(report.source_tracks, 3);
        assert!(!report.rescaled);
        assert_eq!(notes_of(&back), notes_of(&song));
        assert_eq!(back.markers, song.markers);
        assert_eq!(back.time_signature, song.time_signature);
        assert!((back.tempo_map.bpm_at(0) - 120.0).abs() < 0.001);
        assert_eq!(
            back.tracks
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Piano", "Drums"]
        );
    }

    #[test]
    fn two_tempos_survive_the_round_trip() {
        let mut song = example_song();
        song.tempo_map = song
            .tempo_map
            .clone()
            .with_change(1920, 75.0)
            .expect("valid change");
        let bytes = encode(&song).expect("encode");
        let (back, _) = decode(&bytes).expect("decode");
        assert_eq!(back.tempo_map.changes().len(), 2);
        assert!((back.tempo_map.bpm_at(0) - 120.0).abs() < 0.001);
        assert!((back.tempo_map.bpm_at(1920) - 75.0).abs() < 0.001);
    }

    #[test]
    fn simultaneous_note_off_comes_before_note_on_in_the_file() {
        let mut song = Song::new("Legato", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 960,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        track.notes.push(Note {
            id: NoteId::new(2).expect("valid"),
            start: 960,
            duration: 960,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(90).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        let bytes = encode(&song).expect("encode");
        let smf = Smf::parse(&bytes).expect("parse");
        let kinds: Vec<u8> = smf.tracks[1]
            .iter()
            .filter_map(|event| match event.kind {
                TrackEventKind::Midi { message, .. } => Some(match message {
                    Midi::NoteOn { .. } => 0x90,
                    Midi::NoteOff { .. } => 0x80,
                    _ => 0x00,
                }),
                _ => None,
            })
            .collect();
        assert_eq!(kinds, vec![0x90, 0x80, 0x90, 0x80]);
        // The note-off at tick 960 must be encoded before the note-on.
        let (back, _) = decode(&bytes).expect("decode");
        assert_eq!(notes_of(&back)[0].len(), 2);
    }

    #[test]
    fn decodes_foreign_ppq_by_rescaling() {
        // Build a 480 PPQ file with one quarter note at tick 0.
        let header = Header::new(Format::SingleTrack, Timing::Metrical(u15::new(480)));
        let mut smf = Smf::new(header);
        smf.tracks.push(vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(500_000))),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: Midi::NoteOn {
                        key: u7::new(60),
                        vel: u7::new(100),
                    },
                },
            },
            TrackEvent {
                delta: u28::new(480),
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: Midi::NoteOff {
                        key: u7::new(60),
                        vel: u7::new(0),
                    },
                },
            },
        ]);
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).expect("write");
        let (song, report) = decode(&bytes).expect("decode");
        assert!(report.rescaled);
        let note = &song.tracks[0].notes[0];
        assert_eq!(note.start, 0);
        assert_eq!(
            note.duration, 960,
            "480 PPQ quarter becomes 960 PPQ quarter"
        );
    }

    #[test]
    fn unmatched_note_events_are_counted() {
        let header = Header::new(Format::SingleTrack, Timing::Metrical(u15::new(960)));
        let mut smf = Smf::new(header);
        smf.tracks.push(vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: Midi::NoteOn {
                        key: u7::new(60),
                        vel: u7::new(100),
                    },
                },
            },
            TrackEvent {
                delta: u28::new(10),
                kind: TrackEventKind::Midi {
                    channel: u4::new(1),
                    message: Midi::NoteOff {
                        key: u7::new(64),
                        vel: u7::new(0),
                    },
                },
            },
        ]);
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).expect("write");
        let (song, report) = decode(&bytes).expect("decode");
        assert_eq!(report.unpaired_note_ons, 1);
        assert_eq!(report.unmatched_note_offs, 1);
        assert_eq!(song.tracks.len(), 0, "no notes means no track");
    }

    #[test]
    fn tempo_conversion_rejects_impossible_values() {
        for bpm in [0.0, -1.0, f64::NAN, 1.0e9] {
            assert!(
                matches!(tempo_micros(bpm), Err(SmfError::Tempo { .. })),
                "{bpm}"
            );
        }
        assert_eq!(tempo_micros(120.0).expect("valid").as_int(), 500_000);
    }
}
