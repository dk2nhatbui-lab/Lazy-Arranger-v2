//! Pure note capture: pairs note-on and note-off messages by arrival time.
//!
//! Pairing is per channel and pitch. When a pitch is struck twice before it is
//! released, the oldest sounding note is closed first (FIFO), which matches the
//! C++ recorder and standard MIDI-file pairing.

use std::collections::VecDeque;

use music_core::{Channel, MidiMessage, Pitch, TempoMap, Tick, Velocity};
use project::ids::{NoteId, TrackId};
use project::song::{Note, Track};

/// One recorded note, in capture time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturedNote {
    /// Microseconds from the start of capture.
    pub start_us: u64,
    /// Held time in microseconds; always greater than zero.
    pub duration_us: u64,
    /// MIDI note number.
    pub pitch: Pitch,
    /// Note-on velocity.
    pub velocity: Velocity,
    /// Channel.
    pub channel: Channel,
}

/// Counters describing what capture discarded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureStats {
    /// Note-offs that matched nothing.
    pub unmatched_note_offs: usize,
    /// Note-ons still held when capture stopped; they were dropped.
    pub held_at_stop: usize,
    /// Note-ons and note-offs that arrived at the same microsecond and would
    /// have produced a zero-length note.
    pub zero_length: usize,
    /// Messages that were not notes (controllers, bend, program).
    pub non_note_messages: usize,
}

/// A sounding note waiting for its note-off.
#[derive(Clone, Copy, Debug)]
struct Pending {
    channel: Channel,
    pitch: Pitch,
    velocity: Velocity,
    start_us: u64,
}

/// Collector for one recording pass.
#[derive(Debug, Default)]
pub struct NoteCapture {
    pending: VecDeque<Pending>,
    notes: Vec<CapturedNote>,
    stats: CaptureStats,
    last_time_us: u64,
}

impl NoteCapture {
    /// Creates an empty capture.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one message with its arrival time in microseconds.
    ///
    /// Times must not go backwards; a message that arrives "before" the last
    /// one is treated as arriving at the last time, which keeps durations
    /// non-negative without silently reordering notes.
    pub fn ingest(&mut self, time_us: u64, message: MidiMessage) {
        let time_us = time_us.max(self.last_time_us);
        self.last_time_us = time_us;
        match message.normalized() {
            MidiMessage::NoteOn {
                channel,
                key,
                velocity,
            } => self.pending.push_back(Pending {
                channel,
                pitch: key,
                velocity,
                start_us: time_us,
            }),
            MidiMessage::NoteOff { channel, key, .. } => {
                let position = self
                    .pending
                    .iter()
                    .position(|pending| pending.channel == channel && pending.pitch == key);
                let Some(position) = position else {
                    self.stats.unmatched_note_offs += 1;
                    return;
                };
                let Some(pending) = self.pending.remove(position) else {
                    self.stats.unmatched_note_offs += 1;
                    return;
                };
                let Some(duration_us) = time_us.checked_sub(pending.start_us) else {
                    self.stats.zero_length += 1;
                    return;
                };
                if duration_us == 0 {
                    self.stats.zero_length += 1;
                    return;
                }
                self.notes.push(CapturedNote {
                    start_us: pending.start_us,
                    duration_us,
                    pitch: pending.pitch,
                    velocity: pending.velocity,
                    channel: pending.channel,
                });
            }
            _ => self.stats.non_note_messages += 1,
        }
    }

    /// Finishes capture and returns the notes sorted by start time.
    ///
    /// Notes still held are dropped and counted, because their length is
    /// unknown.
    #[must_use]
    pub fn finish(mut self) -> (Vec<CapturedNote>, CaptureStats) {
        self.stats.held_at_stop = self.pending.len();
        self.pending.clear();
        self.notes
            .sort_by_key(|note| (note.start_us, note.channel, note.pitch));
        (self.notes, self.stats)
    }

    /// Notes captured so far, unsorted.
    #[must_use]
    pub fn notes(&self) -> &[CapturedNote] {
        &self.notes
    }

    /// Statistics so far.
    #[must_use]
    pub fn stats(&self) -> CaptureStats {
        self.stats
    }
}

impl CapturedNote {
    /// Converts to a project note, placing it on the tick timeline with
    /// [`TempoMap::tick_at_seconds`].
    ///
    /// The resulting note is at least one tick long. Microsecond timestamps
    /// cannot resolve every tick at high tempos, so this is capture-accurate,
    /// not sample-accurate; quantise it before promising grid alignment.
    #[must_use]
    pub fn to_note(self, id: NoteId, tempo_map: &TempoMap) -> Note {
        let start = tempo_map.tick_at_seconds(self.start_us as f64 / 1_000_000.0);
        let end =
            tempo_map.tick_at_seconds((self.start_us + self.duration_us) as f64 / 1_000_000.0);
        Note {
            id,
            start,
            duration: (end - start).max(1),
            pitch: self.pitch,
            velocity: self.velocity,
            channel: self.channel,
        }
    }
}

/// Builds a project track from captured notes.
///
/// # Errors
///
/// Returns an error when the track identifier is zero.
pub fn capture_to_track(
    name: impl Into<String>,
    track_id: TrackId,
    notes: &[CapturedNote],
    tempo_map: &TempoMap,
) -> Result<Track, project::ProjectError> {
    let mut track = Track::new(track_id, name);
    for (index, captured) in notes.iter().enumerate() {
        let id = NoteId::new(u32::try_from(index + 1).unwrap_or(u32::MAX))
            .map_err(|err| project::ProjectError::Legacy(err.to_string()))?;
        track.notes.push(captured.to_note(id, tempo_map));
    }
    Ok(track)
}

/// Tick position of a captured note start, for display.
#[must_use]
pub fn capture_tick(start_us: u64, tempo_map: &TempoMap) -> Tick {
    tempo_map.tick_at_seconds(start_us as f64 / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn velocity(value: u8) -> Velocity {
        Velocity::new(value).expect("valid velocity")
    }

    fn pitch(name: &str) -> Pitch {
        Pitch::from_name(name).expect("valid pitch")
    }

    fn note_on(channel: Channel, name: &str, vel: u8) -> MidiMessage {
        MidiMessage::note_on(channel, pitch(name), velocity(vel))
    }

    fn note_off(channel: Channel, name: &str) -> MidiMessage {
        MidiMessage::note_off(channel, pitch(name), velocity(0))
    }

    #[test]
    fn pairs_note_on_and_note_off() {
        let mut capture = NoteCapture::new();
        capture.ingest(1_000, note_on(Channel::first(), "C4", 100));
        capture.ingest(501_000, note_off(Channel::first(), "C4"));
        let (notes, stats) = capture.finish();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].start_us, 1_000);
        assert_eq!(notes[0].duration_us, 500_000);
        assert_eq!(notes[0].pitch.as_u8(), 60);
        assert_eq!(notes[0].velocity.as_u8(), 100);
        assert_eq!(stats, CaptureStats::default());
    }

    #[test]
    fn zero_velocity_note_on_closes_the_note() {
        let mut capture = NoteCapture::new();
        capture.ingest(0, note_on(Channel::first(), "C4", 100));
        capture.ingest(10_000, note_on(Channel::first(), "C4", 0));
        let (notes, stats) = capture.finish();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].duration_us, 10_000);
        assert_eq!(stats.unmatched_note_offs, 0);
    }

    #[test]
    fn repeated_pitch_closes_the_oldest_note_first() {
        let mut capture = NoteCapture::new();
        capture.ingest(0, note_on(Channel::first(), "C4", 90));
        capture.ingest(100_000, note_on(Channel::first(), "C4", 60));
        capture.ingest(200_000, note_off(Channel::first(), "C4"));
        capture.ingest(300_000, note_off(Channel::first(), "C4"));
        let (notes, _) = capture.finish();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].start_us, 0);
        assert_eq!(notes[0].duration_us, 200_000);
        assert_eq!(notes[0].velocity.as_u8(), 90);
        assert_eq!(notes[1].start_us, 100_000);
        assert_eq!(notes[1].duration_us, 200_000);
    }

    #[test]
    fn channels_are_independent() {
        let mut capture = NoteCapture::new();
        let first = Channel::first();
        let drums = Channel::drums();
        capture.ingest(0, note_on(first, "C4", 100));
        capture.ingest(0, note_on(drums, "C4", 100));
        capture.ingest(1_000, note_off(drums, "C4"));
        capture.ingest(2_000, note_off(first, "C4"));
        let (notes, _) = capture.finish();
        assert_eq!(notes.len(), 2);
        // Both start at 0; the sort tie-break is channel, so channel 1 is first.
        assert_eq!(notes[0].channel, first);
        assert_eq!(notes[0].duration_us, 2_000);
        assert_eq!(notes[1].channel, drums);
        assert_eq!(notes[1].duration_us, 1_000);
    }

    #[test]
    fn unmatched_events_are_counted() {
        let mut capture = NoteCapture::new();
        capture.ingest(0, note_off(Channel::first(), "C4"));
        capture.ingest(
            1_000,
            MidiMessage::ControlChange {
                channel: Channel::first(),
                controller: music_core::ControlNumber::channel_volume(),
                value: 100,
            },
        );
        capture.ingest(2_000, note_on(Channel::first(), "E4", 100));
        let (notes, stats) = capture.finish();
        assert!(notes.is_empty());
        assert_eq!(stats.unmatched_note_offs, 1);
        assert_eq!(stats.non_note_messages, 1);
        assert_eq!(stats.held_at_stop, 1);
    }

    #[test]
    fn zero_length_notes_are_dropped_and_counted() {
        let mut capture = NoteCapture::new();
        capture.ingest(5_000, note_on(Channel::first(), "C4", 100));
        capture.ingest(5_000, note_off(Channel::first(), "C4"));
        let (notes, stats) = capture.finish();
        assert!(notes.is_empty());
        assert_eq!(stats.zero_length, 1);
    }

    #[test]
    fn backwards_timestamps_do_not_produce_negative_durations() {
        let mut capture = NoteCapture::new();
        capture.ingest(10_000, note_on(Channel::first(), "C4", 100));
        capture.ingest(1_000, note_off(Channel::first(), "C4"));
        let (notes, stats) = capture.finish();
        assert!(notes.is_empty(), "clamped to zero length, then dropped");
        assert_eq!(stats.zero_length, 1);
    }

    #[test]
    fn conversion_to_ticks_follows_the_tempo_map() {
        let mut capture = NoteCapture::new();
        capture.ingest(500_000, note_on(Channel::first(), "C4", 100));
        capture.ingest(750_000, note_off(Channel::first(), "C4"));
        let (notes, _) = capture.finish();
        let tempo_map = TempoMap::constant(120.0);
        let track = capture_to_track(
            "Recorded",
            TrackId::new(1).expect("valid"),
            &notes,
            &tempo_map,
        )
        .expect("track");
        assert_eq!(track.notes[0].start, 960, "0.5 s at 120 bpm is one quarter");
        assert_eq!(track.notes[0].duration, 480, "0.25 s is one eighth");
        assert_eq!(capture_tick(500_000, &tempo_map), 960);
    }

    #[test]
    fn a_very_short_note_is_still_one_tick() {
        let mut capture = NoteCapture::new();
        capture.ingest(0, note_on(Channel::first(), "C4", 100));
        capture.ingest(1, note_off(Channel::first(), "C4"));
        let (notes, _) = capture.finish();
        let tempo_map = TempoMap::constant(120.0);
        let track = capture_to_track("Ticks", TrackId::new(1).expect("valid"), &notes, &tempo_map)
            .expect("track");
        assert_eq!(track.notes[0].duration, 1);
    }
}
