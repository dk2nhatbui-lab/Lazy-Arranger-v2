//! Deterministic sample-clock scheduling.
//!
//! The audio sample count is the master clock. A pattern is compiled on a
//! control thread into an immutable [`PreparedPattern`] that holds each event's
//! absolute sample position, and the real-time thread only walks that sorted
//! list. Compiling is the only place that does tempo-map arithmetic, so block
//! size cannot change when an event sounds.
//!
//! This mirrors the C++ oracle (`ProcessContext::sampleForTick` and
//! `Engine::offsetFor`): an event's sample is the ceiling of its exact time and
//! ordering inside one sample is note-off, then controllers, then note-on.

use music_core::{MidiMessage, SamplePosition, TempoMap, Tick, TimeSignature};
use project::{Song, TrackId};
use thiserror::Error;

/// One prepared event: absolute position on the playback timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledEvent {
    /// Absolute sample position from song start.
    pub sample: SamplePosition,
    /// The tick this event came from.
    pub tick: Tick,
    /// Track that owns the event, used as an ordering tie-break.
    pub track: TrackId,
    /// The MIDI message to send.
    pub message: MidiMessage,
}

impl ScheduledEvent {
    /// Offset inside a block that starts at `block_start`.
    ///
    /// Clamped so a late event plays at the start of the block instead of
    /// being dropped, which is what the C++ engine does.
    #[must_use]
    pub fn offset_in(self, block_start: SamplePosition, frames: usize) -> u32 {
        let offset = self.sample - block_start;
        let clamped = offset.clamp(0, (frames.saturating_sub(1)) as SamplePosition);
        u32::try_from(clamped).unwrap_or(0)
    }
}

/// Errors raised while preparing a pattern.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SequenceError {
    /// The sample rate must be positive.
    #[error("sample rate {0} is invalid")]
    SampleRate(u32),
    /// A note had a negative start or a non-positive length.
    #[error("track {track} note {note}: start {start} and duration {duration} cannot be scheduled")]
    InvalidNote {
        /// The track that owns the note.
        track: TrackId,
        /// The note identifier.
        note: project::NoteId,
        /// The note start.
        start: Tick,
        /// The note length.
        duration: Tick,
    },
}

/// An immutable, sample-addressed event list published to the scheduler.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedPattern {
    events: Vec<ScheduledEvent>,
    tempo_map: TempoMap,
    time_signature: TimeSignature,
    sample_rate: u32,
    end_tick: Tick,
    end_sample: SamplePosition,
}

impl PreparedPattern {
    /// Compiles a song into absolute sample positions at `sample_rate`.
    ///
    /// # Errors
    ///
    /// Returns [`SequenceError::SampleRate`] for a zero rate and
    /// [`SequenceError::InvalidNote`] for a note that cannot be scheduled.
    pub fn prepare(song: &Song, sample_rate: u32) -> Result<Self, SequenceError> {
        if sample_rate == 0 {
            return Err(SequenceError::SampleRate(sample_rate));
        }
        let mut events = Vec::with_capacity(song.tracks.iter().map(|t| t.notes.len() * 2).sum());
        let mut end_tick: Tick = 0;
        for track in song.tracks.iter().filter(|track| song.is_audible(track)) {
            for note in &track.notes {
                if note.start < 0 || note.duration <= 0 {
                    return Err(SequenceError::InvalidNote {
                        track: track.id,
                        note: note.id,
                        start: note.start,
                        duration: note.duration,
                    });
                }
                let on_tick = note.start;
                let off_tick = note.start.saturating_add(note.duration);
                events.push(ScheduledEvent {
                    sample: song.tempo_map.samples_at(on_tick, sample_rate),
                    tick: on_tick,
                    track: track.id,
                    message: MidiMessage::note_on(note.channel, note.pitch, note.velocity),
                });
                events.push(ScheduledEvent {
                    sample: song.tempo_map.samples_at(off_tick, sample_rate),
                    tick: off_tick,
                    track: track.id,
                    message: MidiMessage::note_off(note.channel, note.pitch, zero_velocity()),
                });
                end_tick = end_tick.max(off_tick);
            }
        }
        if let Some(marker) = song.markers.iter().map(|marker| marker.tick).max() {
            end_tick = end_tick.max(marker);
        }
        if let Some(chord) = song.chords.last() {
            end_tick = end_tick.max(chord.tick);
        }
        // Stable sort: equal keys keep track order, then note order.
        events.sort_by_key(|event| (event.sample, event.message.sort_rank(), event.track.get()));
        let end_sample = song.tempo_map.samples_at(end_tick, sample_rate);
        Ok(Self {
            events,
            tempo_map: song.tempo_map.clone(),
            time_signature: song.time_signature,
            sample_rate,
            end_tick,
            end_sample,
        })
    }

    /// Prepared events in playback order.
    #[must_use]
    pub fn events(&self) -> &[ScheduledEvent] {
        &self.events
    }

    /// The tempo map the events were compiled against.
    #[must_use]
    pub fn tempo_map(&self) -> &TempoMap {
        &self.tempo_map
    }

    /// The time signature of the compiled song.
    #[must_use]
    pub fn time_signature(&self) -> TimeSignature {
        self.time_signature
    }

    /// The sample rate the events were compiled for.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Last event tick, including the tail of the last note.
    #[must_use]
    pub fn end_tick(&self) -> Tick {
        self.end_tick
    }

    /// Sample position where the last note stops sounding.
    #[must_use]
    pub fn end_sample(&self) -> SamplePosition {
        self.end_sample
    }
}

fn zero_velocity() -> music_core::Velocity {
    music_core::Velocity::new(0).expect("zero is a valid velocity")
}

/// A fixed-capacity event buffer that never allocates after construction.
///
/// The real-time thread pushes into it; overflow is counted, not hidden. The
/// capacity is chosen by the caller to cover the largest block it will process.
#[derive(Debug)]
pub struct EventBuffer {
    events: Vec<ScheduledEvent>,
    overflowed: usize,
}

impl EventBuffer {
    /// Creates a buffer with room for `capacity` events.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
            overflowed: 0,
        }
    }

    /// Empties the buffer and resets the overflow counter. Keeps capacity.
    pub fn clear(&mut self) {
        self.events.clear();
        self.overflowed = 0;
    }

    /// Adds an event; returns false and counts the loss when the buffer is full.
    pub fn push(&mut self, event: ScheduledEvent) -> bool {
        if self.events.len() == self.events.capacity() {
            self.overflowed += 1;
            false
        } else {
            self.events.push(event);
            true
        }
    }

    /// Events for this block, in playback order.
    #[must_use]
    pub fn events(&self) -> &[ScheduledEvent] {
        &self.events
    }

    /// Number of events dropped by this block because the buffer was full.
    #[must_use]
    pub fn overflowed(&self) -> usize {
        self.overflowed
    }

    /// Capacity chosen at construction.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.events.capacity()
    }
}

/// What one block produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockReport {
    /// Events written to the buffer.
    pub emitted: usize,
    /// Events lost because the buffer was full.
    pub overflowed: usize,
    /// First sample of the block.
    pub block_start: SamplePosition,
    /// Song position in ticks at the end of the block.
    pub tick: Tick,
    /// True once the end of the prepared pattern has been passed.
    pub finished: bool,
}

/// A note the scheduler started and has not ended yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveNote {
    track: TrackId,
    channel: music_core::Channel,
    key: music_core::Pitch,
    /// Overlapping note-ons of the same key on the same track.
    count: u16,
    /// Set when the next block must end this note itself.
    release: bool,
}

/// Most notes tracked at once; storage is allocated up front.
const MAX_ACTIVE_NOTES: usize = 512;

/// The real-time scheduler: one prepared snapshot, one sample cursor.
#[derive(Clone, Debug)]
pub struct Scheduler {
    prepared: PreparedPattern,
    position: SamplePosition,
    cursor: usize,
    playing: bool,
    /// Sounding notes, per track, channel and key.
    active: Vec<ActiveNote>,
    /// Set by stop, seek and publish: the next block first releases the
    /// marked notes, so none is left hanging.
    release_pending: bool,
}

impl Scheduler {
    /// Creates a scheduler parked at song start.
    #[must_use]
    pub fn new(prepared: PreparedPattern) -> Self {
        Self {
            prepared,
            position: 0,
            cursor: 0,
            playing: false,
            active: Vec::with_capacity(MAX_ACTIVE_NOTES),
            release_pending: false,
        }
    }

    /// Replaces the prepared snapshot at a block boundary.
    ///
    /// The new snapshot must be compiled for the same sample rate. The cursor
    /// moves to the first event at or after the current position, so a tempo
    /// edit during playback never replays or drops an event.
    ///
    /// A sounding note whose note-off is gone from the new snapshot (the note
    /// was edited, deleted or muted) is released by the next block.
    pub fn publish(&mut self, prepared: PreparedPattern) {
        self.prepared = prepared;
        self.seek_to_position();
        let upcoming = &self.prepared.events[self.cursor..];
        for note in &mut self.active {
            let ends_later = upcoming.iter().any(|event| {
                event.track == note.track
                    && matches!(
                        event.message.normalized(),
                        MidiMessage::NoteOff { channel, key, .. }
                            if channel == note.channel && key == note.key
                    )
            });
            note.release |= !ends_later;
        }
        self.release_pending = self.active.iter().any(|note| note.release);
    }

    /// The active snapshot.
    #[must_use]
    pub fn prepared(&self) -> &PreparedPattern {
        &self.prepared
    }

    /// Starts playback from the current position.
    pub fn play(&mut self) {
        self.playing = true;
    }

    /// Stops playback and keeps the position. The next block releases any
    /// note that is still sounding.
    pub fn stop(&mut self) {
        self.playing = false;
        self.release_all();
    }

    /// True while a note-on has been emitted without its note-off.
    #[must_use]
    pub fn has_active_notes(&self) -> bool {
        !self.active.is_empty()
    }

    fn release_all(&mut self) {
        for note in &mut self.active {
            note.release = true;
        }
        self.release_pending = self.has_active_notes();
    }

    /// True while the scheduler emits events.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Current song position in samples.
    #[must_use]
    pub fn position(&self) -> SamplePosition {
        self.position
    }

    /// Current song position in ticks.
    #[must_use]
    pub fn tick(&self) -> Tick {
        self.prepared
            .tempo_map
            .tick_at_samples(self.position, self.prepared.sample_rate)
    }

    /// Moves to an absolute sample position.
    pub fn seek(&mut self, sample: SamplePosition) {
        self.position = sample.max(0);
        self.release_all();
        self.seek_to_position();
    }

    /// Moves to an absolute tick position, rounded up to the next sample.
    pub fn seek_tick(&mut self, tick: Tick) {
        let sample = self
            .prepared
            .tempo_map
            .samples_at(tick.max(0), self.prepared.sample_rate);
        self.seek(sample);
    }

    /// True when the playhead is past the last event.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.position > self.prepared.end_sample
    }

    /// Produces the events for one block of `frames` samples.
    ///
    /// The block is `[position, position + frames)`. Each prepared event is
    /// emitted exactly once, at its absolute sample position; the caller turns
    /// that into a device offset with [`ScheduledEvent::offset_in`].
    pub fn process_block(&mut self, frames: usize, out: &mut EventBuffer) -> BlockReport {
        out.clear();
        let block_start = self.position;
        let block_end = block_start
            .saturating_add(SamplePosition::try_from(frames).unwrap_or(SamplePosition::MAX));
        let mut emitted = 0;
        if self.release_pending {
            emitted += self.release_active(block_start, out);
        }
        if self.playing {
            while let Some(event) = self.prepared.events.get(self.cursor) {
                if event.sample >= block_end {
                    break;
                }
                if event.sample >= block_start && out.push(*event) {
                    emitted += 1;
                    let event = *event;
                    self.track_note(&event);
                }
                self.cursor += 1;
            }
        }
        self.position = block_end;
        BlockReport {
            emitted,
            overflowed: out.overflowed(),
            block_start,
            tick: self
                .prepared
                .tempo_map
                .tick_at_samples(self.position, self.prepared.sample_rate),
            finished: self.finished(),
        }
    }

    fn track_note(&mut self, event: &ScheduledEvent) {
        let (channel, key, on) = match event.message.normalized() {
            MidiMessage::NoteOn { channel, key, .. } => (channel, key, true),
            MidiMessage::NoteOff { channel, key, .. } => (channel, key, false),
            _ => return,
        };
        let found = self.active.iter().position(|note| {
            note.track == event.track && note.channel == channel && note.key == key
        });
        match (found, on) {
            (Some(index), true) => {
                let note = &mut self.active[index];
                note.count = note.count.saturating_add(1);
            }
            (None, true) if self.active.len() < MAX_ACTIVE_NOTES => {
                self.active.push(ActiveNote {
                    track: event.track,
                    channel,
                    key,
                    count: 1,
                    release: false,
                });
            }
            (Some(index), false) => {
                let note = &mut self.active[index];
                note.count = note.count.saturating_sub(1);
                if note.count == 0 {
                    self.active.swap_remove(index);
                }
            }
            _ => {}
        }
    }

    /// Emits a note-off at `sample` for every note marked for release, on
    /// the track that started it. Notes that do not fit in `out` stay
    /// tracked and are released by the next block.
    fn release_active(&mut self, sample: SamplePosition, out: &mut EventBuffer) -> usize {
        let tick = self
            .prepared
            .tempo_map
            .tick_at_samples(sample, self.prepared.sample_rate);
        let mut emitted = 0;
        let mut index = 0;
        while let Some(note) = self.active.get_mut(index) {
            if !note.release {
                index += 1;
                continue;
            }
            // One note-off per overlapping note-on, for synths that count.
            while note.count > 0 {
                let event = ScheduledEvent {
                    sample,
                    tick,
                    track: note.track,
                    message: MidiMessage::note_off(note.channel, note.key, zero_velocity()),
                };
                if !out.push(event) {
                    return emitted;
                }
                note.count -= 1;
                emitted += 1;
            }
            self.active.swap_remove(index);
        }
        self.release_pending = false;
        emitted
    }

    fn seek_to_position(&mut self) {
        self.cursor = self
            .prepared
            .events
            .partition_point(|event| event.sample < self.position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, Chord, Pitch, Velocity};
    use project::{ChordEvent, Note, NoteId, Track};

    fn velocity(value: u8) -> Velocity {
        Velocity::new(value).expect("valid velocity")
    }

    /// A two-bar pattern at 120 bpm: quarter notes on beat 1 and 3 of each bar.
    fn four_bar_song() -> Song {
        let mut song = Song::new("Timing fixture", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        let mut id = 0;
        for bar in 0..4 {
            for pitch in ["C4", "E4"] {
                id += 1;
                track.notes.push(Note {
                    id: NoteId::new(id).expect("valid"),
                    start: bar * 3840,
                    duration: 960,
                    pitch: Pitch::from_name(pitch).expect("valid pitch"),
                    velocity: velocity(100),
                    channel: Channel::first(),
                });
            }
        }
        song.tracks.push(track);
        song
    }

    fn drain(prepared: &PreparedPattern, block: usize) -> Vec<ScheduledEvent> {
        let mut scheduler = Scheduler::new(prepared.clone());
        scheduler.play();
        let mut buffer = EventBuffer::with_capacity(64);
        let mut out = Vec::new();
        let mut guard = 0;
        loop {
            let report = scheduler.process_block(block, &mut buffer);
            out.extend_from_slice(buffer.events());
            guard += 1;
            if report.finished && report.emitted == 0 {
                break;
            }
            assert!(guard < 1_000_000, "scheduler did not finish");
        }
        out
    }

    #[test]
    fn events_land_on_exact_samples() {
        let song = four_bar_song();
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        // 120 bpm: one quarter = 0.5 s = 24000 samples; one bar = 96000.
        assert_eq!(prepared.events().len(), 16);
        assert_eq!(prepared.events()[0].sample, 0);
        assert_eq!(
            prepared.events()[2].sample,
            24_000,
            "first note-offs at beat 2"
        );
        assert_eq!(
            prepared.events()[4].sample,
            96_000,
            "second bar starts at bar 2"
        );
        assert_eq!(
            prepared.end_tick(),
            3 * 3840 + 960,
            "last note ends in bar 4"
        );
        assert_eq!(prepared.end_sample(), 312_000, "6.5 s at 48 kHz");
        assert_eq!(prepared.tempo_map().bpm_at(0), 120.0);
    }

    #[test]
    fn block_size_does_not_change_event_positions() {
        let song = four_bar_song();
        let prepared = PreparedPattern::prepare(&song, 44_100).expect("prepare");
        let reference = drain(&prepared, 512);
        for block in [1, 7, 32, 64, 128, 256, 480, 1024, 4096] {
            let events = drain(&prepared, block);
            assert_eq!(events, reference, "block size {block} changed the output");
        }
    }

    #[test]
    fn every_prepared_event_is_emitted_once() {
        let song = four_bar_song();
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        let emitted = drain(&prepared, 128);
        assert_eq!(emitted.len(), prepared.events().len());
        assert_eq!(emitted, prepared.events());

        let ons = emitted
            .iter()
            .filter(|event| event.message.is_note_on())
            .count();
        let offs = emitted.len() - ons;
        assert_eq!(ons, 8);
        assert_eq!(offs, 8);
    }

    #[test]
    fn note_offs_sort_before_note_ons_in_one_sample() {
        let mut song = Song::new("Legato", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        // Note 1 ends at tick 960; note 2 starts at tick 960 on the same pitch.
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 960,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: velocity(100),
            channel: Channel::first(),
        });
        track.notes.push(Note {
            id: NoteId::new(2).expect("valid"),
            start: 960,
            duration: 960,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: velocity(90),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        let events = prepared.events();
        assert_eq!(events.len(), 4);
        assert_eq!(events[1].sample, events[2].sample);
        assert!(!events[1].message.is_note_on());
        assert!(events[2].message.is_note_on());
    }

    #[test]
    fn simultaneous_events_order_by_track() {
        let mut song = Song::new("Tracks", 120.0);
        for (index, name) in ["Second", "First"].iter().enumerate() {
            let mut track = Track::new(
                TrackId::new(u32::try_from(index + 1).expect("small")).expect("valid"),
                *name,
            );
            track.notes.push(Note {
                id: NoteId::new(1).expect("valid"),
                start: 0,
                duration: 960,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: velocity(100),
                channel: Channel::first(),
            });
            song.tracks.push(track);
        }
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        let note_ons: Vec<TrackId> = prepared
            .events()
            .iter()
            .filter(|event| event.message.is_note_on())
            .map(|event| event.track)
            .collect();
        assert_eq!(note_ons[0].get(), 1);
        assert_eq!(note_ons[1].get(), 2);
    }

    #[test]
    fn seek_repositions_without_replaying() {
        let song = four_bar_song();
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        let mut scheduler = Scheduler::new(prepared);
        scheduler.play();
        let mut buffer = EventBuffer::with_capacity(16);
        scheduler.seek(100_000);
        let report = scheduler.process_block(128, &mut buffer);
        assert_eq!(report.emitted, 0, "no events between the notes of bar 2");
        scheduler.seek(0);
        let report = scheduler.process_block(128, &mut buffer);
        assert_eq!(
            report.emitted, 2,
            "both notes at tick 0 replay after a seek"
        );
    }

    #[test]
    fn stopped_scheduler_advances_time_without_events() {
        let song = four_bar_song();
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        let mut scheduler = Scheduler::new(prepared);
        let mut buffer = EventBuffer::with_capacity(16);
        let report = scheduler.process_block(480, &mut buffer);
        assert_eq!(report.emitted, 0);
        assert_eq!(scheduler.position(), 480);
        assert_eq!(buffer.events().len(), 0);
    }

    #[test]
    fn overflow_is_counted_not_panicking() {
        let song = four_bar_song();
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        let mut scheduler = Scheduler::new(prepared);
        scheduler.play();
        let mut buffer = EventBuffer::with_capacity(1);
        let report = scheduler.process_block(48_000, &mut buffer);
        assert_eq!(report.emitted, 1);
        assert_eq!(report.overflowed, 3, "four events in the block, one bucket");
        assert_eq!(buffer.events().len(), 1);
    }

    #[test]
    fn block_offsets_clamp_inside_the_block() {
        let song = four_bar_song();
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        let event = prepared.events()[0];
        assert_eq!(event.offset_in(0, 128), 0);
        assert_eq!(
            event.offset_in(50, 128),
            0,
            "an event that is already past the block start clamps to offset 0"
        );
        let last = ScheduledEvent {
            sample: 200,
            ..event
        };
        assert_eq!(last.offset_in(0, 128), 127, "offsets never reach frames");
    }

    #[test]
    fn tempo_changes_are_compiled_into_sample_positions() {
        let mut song = Song::new("Rubato", 120.0);
        song.tempo_map = song
            .tempo_map
            .clone()
            .with_change(3840, 60.0)
            .expect("valid change");
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 3840,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: velocity(100),
            channel: Channel::first(),
        });
        track.notes.push(Note {
            id: NoteId::new(2).expect("valid"),
            start: 3840,
            duration: 3840,
            pitch: Pitch::from_name("E4").expect("valid"),
            velocity: velocity(100),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        // Bar 1: 2 s = 96000 samples. Bar 2 at 60 bpm: 4 s = 192000.
        let starts: Vec<SamplePosition> = prepared
            .events()
            .iter()
            .filter(|event| event.message.is_note_on())
            .map(|event| event.sample)
            .collect();
        assert_eq!(starts, vec![0, 96_000]);
        assert_eq!(prepared.events()[1].sample, 96_000, "first note off");
        assert_eq!(prepared.end_sample(), 96_000 + 192_000);
    }

    #[test]
    fn recompiled_snapshot_keeps_the_position() {
        let song = four_bar_song();
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        let mut scheduler = Scheduler::new(prepared.clone());
        scheduler.play();
        let mut buffer = EventBuffer::with_capacity(16);
        scheduler.process_block(96_000 + 10, &mut buffer);
        let mut retimed = song;
        retimed.tempo_map = TempoMap::constant(90.0);
        let replacement = PreparedPattern::prepare(&retimed, 48_000).expect("prepare");
        scheduler.publish(replacement);
        assert!(scheduler.position() >= 96_000);
        // Publishing must not reset the playhead to zero.
        let report = scheduler.process_block(0, &mut buffer);
        assert_eq!(report.block_start, 96_010);
    }

    #[test]
    fn invalid_notes_and_rates_are_rejected() {
        assert_eq!(
            PreparedPattern::prepare(&Song::default(), 0),
            Err(SequenceError::SampleRate(0))
        );
        let mut song = Song::new("Bad", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 0,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: velocity(100),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        assert!(matches!(
            PreparedPattern::prepare(&song, 48_000),
            Err(SequenceError::InvalidNote { .. })
        ));
    }

    #[test]
    fn chords_extend_the_pattern_end() {
        let mut song = four_bar_song();
        song.chords.push(ChordEvent {
            tick: 20_000,
            chord: Chord::parse("C").expect("valid"),
        });
        let prepared = PreparedPattern::prepare(&song, 48_000).expect("prepare");
        assert_eq!(prepared.end_tick(), 20_000);
        assert_eq!(prepared.end_sample(), 500_000);
    }

    #[test]
    fn buffer_holds_capacity_without_growing() {
        let mut buffer = EventBuffer::with_capacity(2);
        let event = ScheduledEvent {
            sample: 0,
            tick: 0,
            track: TrackId::new(1).expect("valid"),
            message: MidiMessage::note_on(
                Channel::first(),
                Pitch::from_name("C4").expect("valid"),
                velocity(1),
            ),
        };
        assert!(buffer.push(event));
        assert!(buffer.push(event));
        assert!(!buffer.push(event));
        assert_eq!(buffer.overflowed(), 1);
        assert_eq!(buffer.capacity(), 2);
        buffer.clear();
        assert_eq!(buffer.overflowed(), 0);
    }

    #[test]
    fn stopping_mid_note_releases_it() {
        let mut song = Song::new("Held", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: music_core::PPQ * 8,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        let mut scheduler =
            Scheduler::new(PreparedPattern::prepare(&song, 48_000).expect("prepare"));
        let mut out = EventBuffer::with_capacity(16);
        scheduler.play();
        scheduler.process_block(256, &mut out);
        assert!(scheduler.has_active_notes());
        scheduler.stop();
        scheduler.process_block(256, &mut out);
        assert!(matches!(
            out.events()[0].message,
            MidiMessage::NoteOff { .. }
        ));
        assert!(!scheduler.has_active_notes());
    }

    #[test]
    fn deleting_a_sounding_note_releases_it_on_its_track() {
        let note = Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: music_core::PPQ * 8,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        };
        let mut song = Song::new("Held", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        song.tracks.push(track.clone());
        track.id = TrackId::new(2).expect("valid");
        track.notes.push(note);
        song.tracks.push(track);
        let mut scheduler =
            Scheduler::new(PreparedPattern::prepare(&song, 48_000).expect("prepare"));
        let mut out = EventBuffer::with_capacity(16);
        scheduler.play();
        scheduler.process_block(256, &mut out);
        assert!(scheduler.has_active_notes());
        // Publishing the same song keeps the note sounding.
        scheduler.publish(PreparedPattern::prepare(&song, 48_000).expect("prepare"));
        scheduler.process_block(256, &mut out);
        assert!(out.events().is_empty());
        assert!(scheduler.has_active_notes());
        song.tracks[1].notes.clear();
        scheduler.publish(PreparedPattern::prepare(&song, 48_000).expect("prepare"));
        scheduler.process_block(256, &mut out);
        let event = out.events()[0];
        assert!(matches!(event.message, MidiMessage::NoteOff { .. }));
        assert_eq!(event.track, TrackId::new(2).expect("valid"));
        assert!(!scheduler.has_active_notes());
    }
}
