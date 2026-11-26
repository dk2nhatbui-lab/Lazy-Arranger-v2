//! Plug-in instruments played in the audio callback.
//!
//! A track whose first active plug-in is a VST3 instrument opens it once on
//! a control thread, with its saved state, as an [`InstrumentLane`]. The
//! callback sends it the track's notes for each block, sample-accurate, and
//! reads its output, so a note edit swaps in a new note list
//! ([`InstrumentNotes`]) without opening the plug-in again or rendering the
//! track.
//!
//! The plug-in is timed by the [`crate::watchdog`] like a live effect. Audio
//! Unit instruments are still rendered before play.

use std::hash::{Hash, Hasher};

use plugin_host::{NoteEvent, RealtimeInstrument};
use project::song::{AutomationTarget, PluginFormat, PluginSlot, Song, Track};

use crate::watchdog::{self, Watch};

/// Frames per block the instrument is prepared for.
const BLOCK: usize = 512;
/// Note events one callback block can carry without allocating.
const EVENTS: usize = 1024;

fn active(slot: &PluginSlot) -> bool {
    !slot.bypassed && !slot.identifier.is_empty()
}

/// True when a track may play a plug-in instrument live: notes only (no
/// recorded audio), a VST3 plug-in as its first active slot and no
/// plug-in parameter automation. Whether that plug-in really opens as an
/// instrument is only known when the lane is built.
#[must_use]
pub fn candidate(track: &Track) -> bool {
    track.audio.is_none()
        && track.comps.is_empty()
        && track
            .plugins
            .iter()
            .find(|slot| active(slot))
            .is_some_and(|slot| matches!(slot.format, PluginFormat::Vst3))
        && !track
            .automation
            .iter()
            .any(|lane| matches!(lane.target, AutomationTarget::PluginParam { .. }))
}

/// A fingerprint of everything a track's note list is built from: its notes
/// and the tempo. When it changes, the notes are out of date.
#[must_use]
pub fn signature(song: &Song, track: &Track) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}{:?}", track.notes, song.tempo_map).hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Copy, Debug)]
struct Event {
    frame: usize,
    on: bool,
    key: u8,
    velocity: u8,
}

/// A track's notes as instrument events, in frames.
#[derive(Clone, Debug, Default)]
pub struct InstrumentNotes {
    /// Note-ons and note-offs in time order, offs first on a shared frame.
    events: Vec<Event>,
    /// Each note's start and stop frame, key and velocity, to restart the
    /// notes sounding at a seek.
    notes: Vec<(usize, usize, u8, u8)>,
}

impl InstrumentNotes {
    /// Times a track's notes at `sample_rate`, as the offline render does.
    #[must_use]
    pub fn build(song: &Song, track: &Track, sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        let mut events = Vec::new();
        let mut notes = Vec::new();
        for note in &track.notes {
            let start = (song.tempo_map.seconds_at(note.start) * rate).round() as usize;
            let stop = (song
                .tempo_map
                .seconds_at(note.start.saturating_add(note.duration.max(1)))
                * rate)
                .round() as usize;
            let key = note.pitch.as_u8();
            let velocity = note.velocity.as_u8();
            events.push(Event {
                frame: start,
                on: true,
                key,
                velocity,
            });
            events.push(Event {
                frame: stop,
                on: false,
                key,
                velocity: 0,
            });
            notes.push((start, stop, key, velocity));
        }
        events.sort_by_key(|event| (event.frame, event.on));
        Self { events, notes }
    }
}

/// A track's plug-in instrument and its notes, played in the callback.
#[derive(Debug)]
pub struct InstrumentLane {
    instrument: RealtimeInstrument,
    watch: Watch,
    notes: Box<InstrumentNotes>,
    /// Frames the lane is delayed by, to line up with plug-in latency
    /// elsewhere in the song.
    delay: usize,
    /// Next event to send.
    cursor: usize,
    /// The source frame the next call continues from, when contiguous.
    next: Option<usize>,
    /// Note-ons not yet matched by a note-off, per key.
    held: [u16; 128],
    /// Events for the block being processed; capacity reserved.
    scratch: Vec<NoteEvent>,
}

impl InstrumentLane {
    /// Opens the track's first active slot as a VST3 instrument. None when
    /// the track is not a [`candidate`], the plug-in was blocked by the
    /// watchdog, or it does not open as an instrument. Also returns the
    /// slot's index: the slots after it are the track's insert effects.
    #[must_use]
    pub fn build(song: &Song, track: &Track, sample_rate: u32) -> Option<(Self, usize)> {
        if !candidate(track) {
            return None;
        }
        let index = track.plugins.iter().position(active)?;
        let slot = &track.plugins[index];
        if watchdog::is_blocked(slot) {
            return None;
        }
        let instrument = RealtimeInstrument::vst3(
            std::path::Path::new(&slot.path),
            &slot.identifier,
            &slot.state,
            f64::from(sample_rate),
            BLOCK,
        )
        .ok()?;
        Some((
            Self {
                instrument,
                watch: Watch::new(slot, sample_rate),
                notes: Box::new(InstrumentNotes::build(song, track, sample_rate)),
                delay: 0,
                cursor: 0,
                next: None,
                held: [0; 128],
                scratch: Vec::with_capacity(EVENTS),
            },
            index,
        ))
    }

    /// Sets the frames the lane is delayed by.
    pub fn set_delay(&mut self, delay: usize) {
        self.delay = delay;
        self.next = None;
    }

    /// Turns the slow-plug-in rule off for an export.
    pub(crate) fn set_realtime(&mut self, realtime: bool) {
        self.watch.realtime = realtime;
    }

    /// Puts a new note list in place and returns the old one. The notes
    /// sounding now stop, and the new ones sounding at this point start.
    pub(crate) fn replace_notes(&mut self, notes: Box<InstrumentNotes>) -> Box<InstrumentNotes> {
        self.next = None;
        std::mem::replace(&mut self.notes, notes)
    }

    fn push(&mut self, event: NoteEvent) {
        if self.scratch.len() < self.scratch.capacity() {
            self.scratch.push(event);
        }
        let held = &mut self.held[usize::from(event.key & 0x7f)];
        if event.on {
            *held = held.saturating_add(1);
        } else {
            *held = held.saturating_sub(1);
        }
    }

    /// Moves to `source`: stops every held note and restarts the notes
    /// sounding there.
    fn locate(&mut self, source: usize) {
        for key in 0..128_u8 {
            if self.held[usize::from(key)] > 0 {
                self.held[usize::from(key)] = 1;
                self.push(NoteEvent {
                    offset: 0,
                    on: false,
                    key,
                    velocity: 0,
                });
            }
        }
        for index in 0..self.notes.notes.len() {
            let (start, stop, key, velocity) = self.notes.notes[index];
            if start < source && stop > source {
                self.push(NoteEvent {
                    offset: 0,
                    on: true,
                    key,
                    velocity,
                });
            }
        }
        self.cursor = self
            .notes
            .events
            .partition_point(|event| event.frame < source);
    }

    /// Writes the lane's interleaved stereo signal for output samples
    /// `pos..pos + output.len()` into `output`. Never allocates.
    pub fn render(&mut self, output: &mut [f32], pos: usize) {
        output.fill(0.0);
        let first = pos / 2;
        let frames = output.len() / 2;
        // Source frames start `delay` later than output frames.
        let lead = self.delay.saturating_sub(first).min(frames);
        if lead == frames {
            return;
        }
        let source = (first + lead).saturating_sub(self.delay);
        self.scratch.clear();
        if self.next != Some(source) {
            self.locate(source);
        }
        let count = frames - lead;
        let block = self.instrument.max_block().max(1);
        let mut done = 0;
        while done < count {
            let len = (count - done).min(block);
            let start = source + done;
            while self.cursor < self.notes.events.len()
                && self.notes.events[self.cursor].frame < start + len
            {
                let event = self.notes.events[self.cursor];
                self.cursor += 1;
                self.push(NoteEvent {
                    offset: u32::try_from(event.frame.saturating_sub(start)).unwrap_or(0),
                    on: event.on,
                    key: event.key,
                    velocity: event.velocity,
                });
            }
            let chunk = &mut output[(lead + done) * 2..(lead + done + len) * 2];
            if !self.watch.dropped() {
                let Self {
                    instrument,
                    watch,
                    scratch,
                    ..
                } = self;
                watch.run(len, || instrument.process(chunk, scratch));
            }
            self.scratch.clear();
            done += len;
        }
        self.next = Some(source + count);
    }

    /// The whole lane rendered from the start into a new buffer of `len`
    /// samples, for a player at another rate.
    #[must_use]
    pub fn render_all(mut self, len: usize) -> Vec<f32> {
        self.set_realtime(false);
        self.next = None;
        let mut out = vec![0.0; len & !1];
        for (index, chunk) in out.chunks_mut(BLOCK * 2).enumerate() {
            self.render(chunk, index * BLOCK * 2);
        }
        out
    }
}
