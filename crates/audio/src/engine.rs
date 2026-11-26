//! The real-time engine core: a prepared graph that the audio callback runs.
//!
//! The control thread builds a [`Graph`] (it may allocate, decode and render
//! as much as it likes) and sends it to the audio thread over a lock-free
//! ring. The audio callback installs it at the next buffer and sends the
//! graph it replaced back on a second ring, so memory is only ever freed on
//! the control thread. Parameter changes (track gains, the master gain) travel
//! on a third ring and are applied at the next 32-frame quantum, ramped over
//! that quantum so a change never clicks.
//!
//! The callback never allocates, locks or frees. The transport (position,
//! play state, loop) is shared through atomics in [`Transport`].
//!
//! Design notes: this follows the graph-swap pattern studied in Cadenza
//! (`docs/audit/notes/refs-cadenza-vibez.md`, §1.6 item 3) and Vibez's rule
//! that every message to the audio thread carries prepared data. No code was
//! taken from either project.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use project::TrackId;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::inserts::{ChainId, Inserts};
use crate::live::LiveRender;
use crate::master::{self, Meter};
use crate::synth::SynthLane;

/// Frames per processing quantum. Parameters change only on its edges.
pub const QUANTUM: usize = 32;
/// Graphs that can wait for the audio thread at once.
const GRAPH_SLOTS: usize = 4;
/// Parameter changes that can wait for the audio thread at once.
const PARAM_SLOTS: usize = 4096;
/// Synth lanes that can wait for the audio thread at once.
const SYNTH_SLOTS: usize = 64;

/// A sample position and the play flag, packed with an edit counter in one
/// atomic.
///
/// Every control change (seek, play, pause, stop) bumps the counter, so a
/// callback that read the state before it cannot overwrite it afterwards,
/// even when the change lands on the same value (the ABA case). Keeping the
/// play flag in the same word means a callback that finishes the song clears
/// it and rewinds in one step, never racing a `play()` in between.
#[derive(Debug)]
struct Cursor(AtomicU64);

const POSITION_BITS: u32 = 44;
const POSITION_MASK: u64 = (1 << POSITION_BITS) - 1;
const PLAYING_BIT: u64 = 1 << POSITION_BITS;
const EPOCH_SHIFT: u32 = POSITION_BITS + 1;

impl Cursor {
    fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    fn position(raw: u64) -> usize {
        (raw & POSITION_MASK) as usize
    }

    fn playing(raw: u64) -> bool {
        raw & PLAYING_BIT != 0
    }

    fn pack(epoch: u64, position: usize, playing: bool) -> u64 {
        let position = (position as u64).min(POSITION_MASK);
        (epoch << EPOCH_SHIFT) | if playing { PLAYING_BIT } else { 0 } | position
    }

    fn load_raw(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }

    fn load(&self) -> usize {
        Self::position(self.load_raw())
    }

    /// Applies a control change to `(position, playing)`, starting a new epoch.
    fn update(&self, change: impl Fn(usize, bool) -> (usize, bool)) {
        let mut raw = self.0.load(Ordering::Acquire);
        loop {
            let epoch = (raw >> EPOCH_SHIFT).wrapping_add(1);
            let (position, playing) = change(Self::position(raw), Self::playing(raw));
            let next = Self::pack(epoch, position, playing);
            match self
                .0
                .compare_exchange_weak(raw, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(current) => raw = current,
            }
        }
    }

    /// Moves the position from the control side, keeping the play state.
    fn store(&self, position: usize) {
        self.update(|_, playing| (position, playing));
    }

    /// Advances from `expected` (a raw value read earlier) unless a control
    /// change happened since. Returns true when it was applied.
    fn advance(&self, expected: u64, position: usize, playing: bool) -> bool {
        let next = Self::pack(expected >> EPOCH_SHIFT, position, playing);
        self.0
            .compare_exchange(expected, next, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }
}

/// Loop bounds in samples, packed as two `u32` frame counts so the callback
/// reads both in one load and never sees a half-updated range.
fn pack_loop(start: usize, end: usize) -> u64 {
    let frames = |samples: usize| u64::from(u32::try_from(samples / 2).unwrap_or(u32::MAX));
    (frames(start) << 32) | frames(end)
}

fn unpack_loop(raw: u64) -> (usize, usize) {
    (
        ((raw >> 32) as usize).saturating_mul(2),
        ((raw & 0xFFFF_FFFF) as usize).saturating_mul(2),
    )
}

/// Transport state shared lock-free between the control side and the audio
/// callback.
#[derive(Debug)]
pub struct Transport {
    position: Cursor,
    looping: AtomicBool,
    loop_bounds: AtomicU64,
    loop_wraps: AtomicU64,
    /// Samples (not frames) in the installed graph.
    len: AtomicUsize,
    sample_rate: u32,
    /// Largest absolute output sample since the meter was last read.
    peak: AtomicU32,
    /// RMS of the last processed buffer.
    rms: AtomicU32,
}

impl Transport {
    fn new(len: usize, sample_rate: u32) -> Self {
        Self {
            position: Cursor::new(),
            looping: AtomicBool::new(false),
            loop_bounds: AtomicU64::new(pack_loop(0, len)),
            loop_wraps: AtomicU64::new(0),
            len: AtomicUsize::new(len),
            sample_rate,
            peak: AtomicU32::new(0),
            rms: AtomicU32::new(0),
        }
    }

    fn len(&self) -> usize {
        self.len.load(Ordering::Acquire)
    }

    /// Sample rate of the output in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Starts or resumes playback from the current position.
    pub fn play(&self) {
        let looping = self.looping.load(Ordering::Acquire);
        let len = self.len();
        let (start, end) = self.active_bounds(looping, len);
        self.position.update(|pos, _| {
            let pos = if looping {
                if pos < start || pos >= end {
                    start
                } else {
                    pos
                }
            } else if pos >= len {
                0
            } else {
                pos
            };
            (pos, true)
        });
    }

    /// Pauses without losing the position.
    pub fn pause(&self) {
        self.position.update(|pos, _| (pos, false));
    }

    /// Stops and rewinds.
    pub fn stop(&self) {
        self.position.update(|_, _| (0, false));
    }

    /// Moves to a frame without changing the play state.
    pub fn seek_frame(&self, frame: usize) {
        self.position.store(frame.saturating_mul(2).min(self.len()));
    }

    /// Moves to a time in seconds without changing the play state.
    pub fn seek_seconds(&self, seconds: f64) {
        let frame = (seconds.max(0.0) * f64::from(self.sample_rate)).round() as usize;
        self.seek_frame(frame);
    }

    /// Enables or disables seamless in-callback looping over a time range.
    ///
    /// When `end_seconds <= start_seconds`, loops to the end of the buffer.
    pub fn set_loop(&self, enabled: bool, start_seconds: f64, end_seconds: f64) {
        let len = self.len() & !1;
        let rate = f64::from(self.sample_rate);
        let start = ((start_seconds.max(0.0) * rate).round() as usize)
            .saturating_mul(2)
            .min(len);
        let end = if end_seconds > start_seconds {
            ((end_seconds.max(0.0) * rate).round() as usize)
                .saturating_mul(2)
                .min(len)
        } else {
            len
        };
        let (start, end) = if end > start { (start, end) } else { (0, len) };
        self.loop_bounds
            .store(pack_loop(start, end), Ordering::Release);
        self.looping.store(enabled, Ordering::Release);
    }

    /// Number of times playback wrapped from loop end back to loop start.
    #[must_use]
    pub fn loop_wraps(&self) -> u64 {
        self.loop_wraps.load(Ordering::Relaxed)
    }

    /// Current position in frames.
    #[must_use]
    pub fn frame(&self) -> usize {
        self.position.load() / 2
    }

    /// Current position in seconds.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.frame() as f64 / f64::from(self.sample_rate.max(1))
    }

    /// True while samples are being produced.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        Cursor::playing(self.position.load_raw())
    }

    /// Peak since the last read and RMS of the last buffer, of what the
    /// callback actually played.
    #[must_use]
    pub fn meter(&self) -> Meter {
        Meter {
            peak: f32::from_bits(self.peak.swap(0, Ordering::AcqRel)),
            rms: f32::from_bits(self.rms.load(Ordering::Acquire)),
        }
    }

    fn active_bounds(&self, looping: bool, len: usize) -> (usize, usize) {
        if !looping {
            return (0, len);
        }
        let (raw_start, raw_end) = unpack_loop(self.loop_bounds.load(Ordering::Acquire));
        let raw_start = raw_start.min(len);
        let end = if raw_end > raw_start && raw_end <= len {
            raw_end
        } else {
            len
        };
        let start = if raw_start < end { raw_start } else { 0 };
        (start, end)
    }

    /// Publishes the level of one output buffer.
    fn publish_meter(&self, output: &[f32]) {
        let mut peak = 0.0_f32;
        let mut sum = 0.0_f32;
        for sample in output {
            peak = peak.max(sample.abs());
            sum += sample * sample;
        }
        let rms = (sum / output.len().max(1) as f32).sqrt();
        // Non-negative floats order like their bits.
        self.peak.fetch_max(peak.to_bits(), Ordering::AcqRel);
        self.rms.store(rms.to_bits(), Ordering::Release);
    }
}

/// A parameter change for the audio thread.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Param {
    /// Left and right gain of a track's live lane.
    TrackGain {
        /// The track, a stable address across graph swaps.
        track: TrackId,
        /// Left gain.
        left: f32,
        /// Right gain.
        right: f32,
    },
    /// Linear master gain.
    MasterGain(f32),
    /// Master chorus and reverb wet amounts, 0..=1.
    MasterEffects {
        /// Chorus amount.
        chorus: f32,
        /// Reverb amount.
        reverb: f32,
    },
    /// Whether the master soft limiter is on.
    Limiter(bool),
    /// Left and right gain of a track's route into a live bus.
    BusFeed {
        /// The track.
        track: TrackId,
        /// The bus id.
        bus: u8,
        /// Left gain.
        left: f32,
        /// Right gain.
        right: f32,
    },
    /// Left and right gain of a live bus: its fader, pan and mute.
    BusGain {
        /// The bus id.
        bus: u8,
        /// Left gain.
        left: f32,
        /// Right gain.
        right: f32,
    },
}

/// A gain the audio thread ramps from its current value to a target.
#[derive(Clone, Copy, Debug)]
struct Gain {
    current: f32,
    target: f32,
}

impl Gain {
    fn new(value: f32) -> Self {
        let value = finite(value);
        Self {
            current: value,
            target: value,
        }
    }

    fn set(&mut self, value: f32) {
        self.target = finite(value);
    }

    /// The ramp across the next `frames`; afterwards the gain is at target.
    fn ramp(&mut self, frames: usize) -> Ramp {
        let ramp = Ramp {
            start: self.current,
            step: (self.target - self.current) / frames.max(1) as f32,
        };
        self.current = self.target;
        ramp
    }
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// A linear gain ramp over one quantum.
#[derive(Clone, Copy, Debug)]
struct Ramp {
    start: f32,
    step: f32,
}

impl Ramp {
    fn at(self, frame: usize) -> f32 {
        self.start + self.step * frame as f32
    }
}

/// Adds `source` into `target` with a gain ramp per channel.
fn mix_into(target: &mut [f32], source: &[f32], left: Ramp, right: Ramp) {
    for (frame, (target, pair)) in target
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .zip(source.as_chunks::<2>().0)
        .enumerate()
    {
        target[0] += pair[0] * left.at(frame);
        target[1] += pair[1] * right.at(frame);
    }
}

/// Where a live lane's signal comes from.
#[derive(Debug)]
enum Source {
    /// A rendered buffer at the graph's length.
    Buffer(Vec<f32>),
    /// Notes played in the callback.
    Synth(Box<SynthLane>),
}

/// One live track and its gains.
#[derive(Debug)]
struct Lane {
    track: TrackId,
    source: Source,
    /// Insert plug-ins run on the source before the gains.
    inserts: Option<Box<Inserts>>,
    /// Gains of the direct route to the master.
    left: Gain,
    right: Gain,
    /// Routes into live buses.
    feeds: Vec<Feed>,
}

/// A lane's route into a live bus.
#[derive(Debug)]
struct Feed {
    /// Index of the bus in the graph.
    bus: usize,
    id: u8,
    left: Gain,
    right: Gain,
}

/// A fixed delay, in interleaved samples, for latency compensation.
#[derive(Debug, Default)]
struct Delay {
    line: Vec<f32>,
    at: usize,
}

impl Delay {
    fn new(frames: usize) -> Self {
        Self {
            line: vec![0.0; frames * 2],
            at: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.line.is_empty()
    }

    fn process(&mut self, samples: &mut [f32]) {
        let len = self.line.len();
        if len == 0 {
            return;
        }
        for sample in samples {
            std::mem::swap(sample, &mut self.line[self.at]);
            self.at += 1;
            if self.at == len {
                self.at = 0;
            }
        }
    }
}

/// A bus whose plug-ins and gains run in the graph.
#[derive(Debug)]
struct BusNode {
    id: u8,
    /// The sum of its feeds for one quantum.
    sum: Vec<f32>,
    inserts: Box<Inserts>,
    delay: Delay,
    left: Gain,
    right: Gain,
}

/// The master stage after the lanes: chorus and reverb, gain, then the
/// limiter.
#[derive(Debug)]
struct MasterStage {
    gain: Gain,
    limiter: bool,
    chorus: Gain,
    reverb: Gain,
    effects: master::Effects,
}

/// Everything the audio callback needs to produce the song, prepared on the
/// control thread.
#[derive(Debug)]
pub struct Graph {
    fixed: Vec<f32>,
    lanes: Vec<Lane>,
    /// Room for one quantum of a synth lane.
    scratch: Vec<f32>,
    /// None for a finished mix that is copied out as it is.
    master: Option<MasterStage>,
    /// Live buses.
    buses: Vec<BusNode>,
    /// One quantum of the direct route, when it is delayed.
    direct: Vec<f32>,
    /// The bus latency compensation on the direct route.
    direct_delay: Delay,
    /// The master plug-ins, before the master stage.
    master_inserts: Option<Box<Inserts>>,
    sample_rate: u32,
}

/// Whole stereo frames only, so left and right never swap, and clean
/// samples, so the callback can copy them straight to the device.
fn clean(mut samples: Vec<f32>, len: usize) -> Vec<f32> {
    samples.resize(len, 0.0);
    for sample in &mut samples {
        if !sample.is_finite() {
            *sample = 0.0;
        }
    }
    samples
}

impl Graph {
    /// Prepares a render for playback. A render with no live lanes and no
    /// master stage plays back exactly as rendered.
    #[must_use]
    pub fn from_render(render: LiveRender) -> Self {
        let sample_rate = render.fixed.sample_rate;
        let len = render.fixed.samples.len() & !1;
        let plain = render.finished;
        let fixed = clean(render.fixed.samples, len);
        let mut synths = render.synths;
        synths.resize_with(render.lanes.len(), || None);
        let mut inserts = render.inserts;
        inserts.resize_with(render.lanes.len(), || None);
        let mut feeds = render.feeds;
        feeds.resize_with(render.lanes.len(), Vec::new);
        let buses: Vec<BusNode> = render
            .buses
            .into_iter()
            .map(|bus| BusNode {
                id: bus.id,
                sum: vec![0.0; QUANTUM * 2],
                inserts: Box::new(bus.inserts),
                delay: Delay::new(bus.delay),
                left: Gain::new(bus.gains.0),
                right: Gain::new(bus.gains.1),
            })
            .collect();
        let lanes = render
            .lanes
            .into_iter()
            .zip(synths)
            .zip(inserts)
            .zip(feeds)
            .zip(render.tracks)
            .zip(render.gains)
            .map(
                |(((((samples, synth), inserts), feeds), track), (left, right))| Lane {
                    track,
                    source: match synth {
                        Some(synth) => Source::Synth(Box::new(synth)),
                        None => Source::Buffer(clean(samples, len)),
                    },
                    inserts: inserts.map(Box::new),
                    left: Gain::new(left),
                    right: Gain::new(right),
                    feeds: feeds
                        .into_iter()
                        .filter_map(|(id, left, right)| {
                            Some(Feed {
                                bus: buses.iter().position(|bus| bus.id == id)?,
                                id,
                                left: Gain::new(left),
                                right: Gain::new(right),
                            })
                        })
                        .collect(),
                },
            )
            .collect();
        Self {
            fixed,
            lanes,
            scratch: vec![0.0; QUANTUM * 2],
            master: (!plain).then(|| MasterStage {
                gain: Gain::new(render.master_gain),
                limiter: render.limiter,
                chorus: Gain::new(render.chorus),
                reverb: Gain::new(render.reverb),
                effects: master::Effects::new(sample_rate),
            }),
            buses,
            direct: vec![0.0; QUANTUM * 2],
            direct_delay: Delay::new(render.bus_delay),
            master_inserts: render.master_inserts.map(Box::new),
            sample_rate,
        }
    }

    /// Length in interleaved samples.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fixed.len()
    }

    /// True when the graph holds no audio.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fixed.is_empty()
    }

    /// Sample rate in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn apply(&mut self, param: Param) {
        match param {
            Param::TrackGain { track, left, right } => {
                if let Some(lane) = self.lanes.iter_mut().find(|lane| lane.track == track) {
                    lane.left.set(left);
                    lane.right.set(right);
                }
            }
            Param::MasterGain(gain) => {
                if let Some(master) = &mut self.master {
                    master.gain.set(gain);
                }
            }
            Param::MasterEffects { chorus, reverb } => {
                if let Some(master) = &mut self.master {
                    master.chorus.set(chorus.clamp(0.0, 1.0));
                    master.reverb.set(reverb.clamp(0.0, 1.0));
                }
            }
            Param::Limiter(on) => {
                if let Some(master) = &mut self.master {
                    master.limiter = on;
                }
            }
            Param::BusFeed {
                track,
                bus,
                left,
                right,
            } => {
                for lane in self.lanes.iter_mut().filter(|lane| lane.track == track) {
                    for feed in lane.feeds.iter_mut().filter(|feed| feed.id == bus) {
                        feed.left.set(left);
                        feed.right.set(right);
                    }
                }
            }
            Param::BusGain { bus, left, right } => {
                if let Some(node) = self.buses.iter_mut().find(|node| node.id == bus) {
                    node.left.set(left);
                    node.right.set(right);
                }
            }
        }
    }

    /// Puts a new synth lane in place of a track's current one and returns
    /// the one it replaced, or the new one when the track has no synth lane.
    fn swap_synth(&mut self, track: TrackId, synth: Box<SynthLane>) -> Box<SynthLane> {
        for lane in &mut self.lanes {
            if lane.track == track
                && let Source::Synth(current) = &mut lane.source
            {
                return std::mem::replace(current, synth);
            }
        }
        synth
    }

    /// Puts a new chain in place of the current one and returns the one it
    /// replaced, or the new one when that chain does not run live.
    fn swap_inserts(&mut self, chain: ChainId, inserts: Box<Inserts>) -> Box<Inserts> {
        let current = match chain {
            ChainId::Track(track) => self
                .lanes
                .iter_mut()
                .find(|lane| lane.track == track)
                .and_then(|lane| lane.inserts.as_mut()),
            ChainId::Bus(id) => self
                .buses
                .iter_mut()
                .find(|bus| bus.id == id)
                .map(|bus| &mut bus.inserts),
            ChainId::Master => self.master_inserts.as_mut(),
        };
        match current {
            Some(current) => std::mem::replace(current, inserts),
            None => inserts,
        }
    }

    /// Plays the whole graph from the start, as playback would, into a new
    /// buffer.
    pub(crate) fn render_all(mut self) -> Vec<f32> {
        let mut output = vec![0.0; self.len()];
        for (index, chunk) in output.chunks_mut(QUANTUM * 2).enumerate() {
            self.render(chunk, index * QUANTUM * 2);
        }
        output
    }

    /// Takes over the gains of the graph it replaces, so a swap neither
    /// jumps nor loses a change sent while the new graph was being built.
    /// The reverb and chorus tails carry over too: the old graph leaves with
    /// the new graph's empty delay lines.
    fn adopt(&mut self, old: &mut Self) {
        for lane in &mut self.lanes {
            if let Some(previous) = old.lanes.iter().find(|entry| entry.track == lane.track) {
                lane.left = previous.left;
                lane.right = previous.right;
                for feed in &mut lane.feeds {
                    if let Some(known) = previous.feeds.iter().find(|entry| entry.id == feed.id) {
                        feed.left = known.left;
                        feed.right = known.right;
                    }
                }
            }
        }
        for bus in &mut self.buses {
            if let Some(previous) = old.buses.iter().find(|entry| entry.id == bus.id) {
                bus.left = previous.left;
                bus.right = previous.right;
            }
        }
        if let (Some(master), Some(previous)) = (&mut self.master, &mut old.master) {
            master.gain = previous.gain;
            master.limiter = previous.limiter;
            master.chorus = previous.chorus;
            master.reverb = previous.reverb;
            if self.sample_rate == old.sample_rate {
                std::mem::swap(&mut master.effects, &mut previous.effects);
            }
        }
    }

    /// Writes the mix of samples `pos..pos + output.len()` into `output`,
    /// ramping every gain to its target across it.
    fn render(&mut self, output: &mut [f32], pos: usize) {
        let Self {
            fixed,
            lanes,
            scratch,
            master,
            buses,
            direct,
            direct_delay,
            master_inserts,
            ..
        } = self;
        match fixed.get(pos..pos + output.len()) {
            Some(source) => output.copy_from_slice(source),
            None => output.fill(0.0),
        }
        let Some(master) = master else {
            return;
        };
        let frames = output.len() / 2;
        // A render call never exceeds one quantum.
        let (Some(scratch), Some(direct)) = (
            scratch.get_mut(..output.len()),
            direct.get_mut(..output.len()),
        ) else {
            return;
        };
        let delayed = !direct_delay.is_empty();
        if delayed {
            direct.fill(0.0);
        }
        for bus in buses.iter_mut() {
            if let Some(sum) = bus.sum.get_mut(..output.len()) {
                sum.fill(0.0);
            }
        }
        for lane in lanes.iter_mut() {
            let left = lane.left.ramp(frames);
            let right = lane.right.ramp(frames);
            let source = match (&mut lane.source, &mut lane.inserts) {
                (Source::Buffer(samples), None) => samples.get(pos..pos + output.len()),
                (source, inserts) => {
                    match source {
                        Source::Buffer(samples) => match samples.get(pos..pos + scratch.len()) {
                            Some(samples) => scratch.copy_from_slice(samples),
                            None => scratch.fill(0.0),
                        },
                        Source::Synth(synth) => synth.render(scratch, pos),
                    }
                    if let Some(inserts) = inserts {
                        inserts.process(scratch);
                        for sample in scratch.iter_mut() {
                            *sample = finite(*sample);
                        }
                    }
                    Some(&*scratch)
                }
            };
            let Some(source) = source else {
                continue;
            };
            let target: &mut [f32] = if delayed { direct } else { output };
            mix_into(target, source, left, right);
            for feed in &mut lane.feeds {
                let left = feed.left.ramp(frames);
                let right = feed.right.ramp(frames);
                if let Some(sum) = buses
                    .get_mut(feed.bus)
                    .and_then(|bus| bus.sum.get_mut(..source.len()))
                {
                    mix_into(sum, source, left, right);
                }
            }
        }
        if delayed {
            direct_delay.process(direct);
            for (target, sample) in output.iter_mut().zip(direct.iter()) {
                *target += sample;
            }
        }
        for bus in buses.iter_mut() {
            let left = bus.left.ramp(frames);
            let right = bus.right.ramp(frames);
            let Some(sum) = bus.sum.get_mut(..output.len()) else {
                continue;
            };
            bus.inserts.process(sum);
            for sample in sum.iter_mut() {
                *sample = finite(*sample);
            }
            bus.delay.process(sum);
            mix_into(output, sum, left, right);
        }
        if let Some(inserts) = master_inserts
            && !inserts.is_empty()
        {
            inserts.process(output);
            for sample in output.iter_mut() {
                *sample = finite(*sample);
            }
        }
        let gain = master.gain.ramp(frames);
        let chorus = master.chorus.ramp(frames);
        let reverb = master.reverb.ramp(frames);
        let first = pos / 2;
        for (frame, pair) in output.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            // The effects always run, so a send turned up mid-song already
            // has the recent past in its delay lines.
            master
                .effects
                .process(pair, first + frame, chorus.at(frame), reverb.at(frame));
            let at = gain.at(frame);
            pair[0] = master::finish(pair[0] * at, master.limiter);
            pair[1] = master::finish(pair[1] * at, master.limiter);
        }
    }
}

/// The audio-thread side of the engine. Owned by the device callback.
#[derive(Debug)]
pub struct Engine {
    graph: Option<Box<Graph>>,
    graphs: Consumer<Box<Graph>>,
    garbage: Producer<Box<Graph>>,
    params: Consumer<Param>,
    synths: Consumer<(TrackId, Box<SynthLane>)>,
    synth_garbage: Producer<Box<SynthLane>>,
    inserts: Consumer<(ChainId, Box<Inserts>)>,
    insert_garbage: Producer<Box<Inserts>>,
    transport: Arc<Transport>,
}

/// The control-thread side of the engine.
#[derive(Debug)]
pub struct EngineControl {
    graphs: Producer<Box<Graph>>,
    garbage: Consumer<Box<Graph>>,
    params: Producer<Param>,
    synths: Producer<(TrackId, Box<SynthLane>)>,
    synth_garbage: Consumer<Box<SynthLane>>,
    inserts: Producer<(ChainId, Box<Inserts>)>,
    insert_garbage: Consumer<Box<Inserts>>,
    transport: Arc<Transport>,
}

/// Creates an engine already holding `graph`, its control side and the
/// shared transport.
#[must_use]
pub fn engine(graph: Graph) -> (Engine, EngineControl, Arc<Transport>) {
    let transport = Arc::new(Transport::new(graph.len(), graph.sample_rate));
    // One more garbage slot than graph slots, so the audio thread always has
    // room to hand back the graph it replaces.
    let (graph_producer, graph_consumer) = RingBuffer::new(GRAPH_SLOTS);
    let (garbage_producer, garbage_consumer) = RingBuffer::new(GRAPH_SLOTS + 1);
    let (param_producer, param_consumer) = RingBuffer::new(PARAM_SLOTS);
    let (synth_producer, synth_consumer) = RingBuffer::new(SYNTH_SLOTS);
    let (synth_garbage_producer, synth_garbage_consumer) = RingBuffer::new(SYNTH_SLOTS + 1);
    let (insert_producer, insert_consumer) = RingBuffer::new(SYNTH_SLOTS);
    let (insert_garbage_producer, insert_garbage_consumer) = RingBuffer::new(SYNTH_SLOTS + 1);
    (
        Engine {
            graph: Some(Box::new(graph)),
            graphs: graph_consumer,
            garbage: garbage_producer,
            params: param_consumer,
            synths: synth_consumer,
            synth_garbage: synth_garbage_producer,
            inserts: insert_consumer,
            insert_garbage: insert_garbage_producer,
            transport: Arc::clone(&transport),
        },
        EngineControl {
            graphs: graph_producer,
            garbage: garbage_consumer,
            params: param_producer,
            synths: synth_producer,
            synth_garbage: synth_garbage_consumer,
            inserts: insert_producer,
            insert_garbage: insert_garbage_consumer,
            transport: Arc::clone(&transport),
        },
        transport,
    )
}

impl EngineControl {
    /// Sends a new graph to the audio thread, which installs it at the next
    /// buffer and keeps the play position.
    ///
    /// # Errors
    ///
    /// Returns the graph when too many are still waiting to be installed.
    pub fn send_graph(&mut self, graph: Graph) -> Result<(), Box<Graph>> {
        self.collect_garbage();
        self.graphs
            .push(Box::new(graph))
            .map_err(|rtrb::PushError::Full(graph)| graph)
    }

    /// Sends a parameter change. Returns false when the ring is full; the
    /// caller sends it again later.
    pub fn send(&mut self, param: Param) -> bool {
        self.params.push(param).is_ok()
    }

    /// Replaces one track's synth lane at the next buffer, for a note edit
    /// while playing.
    ///
    /// # Errors
    ///
    /// Returns the lane when too many are still waiting.
    pub fn send_synth(&mut self, track: TrackId, synth: SynthLane) -> Result<(), Box<SynthLane>> {
        self.collect_garbage();
        self.synths
            .push((track, Box::new(synth)))
            .map_err(|rtrb::PushError::Full((_, synth))| synth)
    }

    /// Replaces one plug-in chain at the next buffer, for a plug-in edit
    /// while playing.
    ///
    /// # Errors
    ///
    /// Returns the chain when too many are still waiting.
    pub fn send_inserts(&mut self, chain: ChainId, inserts: Inserts) -> Result<(), Box<Inserts>> {
        self.collect_garbage();
        self.inserts
            .push((chain, Box::new(inserts)))
            .map_err(|rtrb::PushError::Full((_, inserts))| inserts)
    }

    /// Frees graphs, synth lanes and insert chains the audio thread has
    /// handed back.
    pub fn collect_garbage(&mut self) -> usize {
        let mut freed = 0;
        while let Ok(graph) = self.garbage.pop() {
            drop(graph);
            freed += 1;
        }
        while let Ok(synth) = self.synth_garbage.pop() {
            drop(synth);
            freed += 1;
        }
        while let Ok(inserts) = self.insert_garbage.pop() {
            drop(inserts);
            freed += 1;
        }
        freed
    }

    /// The shared transport.
    #[must_use]
    pub fn transport(&self) -> &Arc<Transport> {
        &self.transport
    }
}

impl Engine {
    /// Installs waiting graphs and applies waiting parameter changes.
    fn receive(&mut self) {
        // Only take a graph when the old one has somewhere to go, so nothing
        // is ever freed here.
        while self.garbage.slots() > 0 {
            let Ok(mut next) = self.graphs.pop() else {
                break;
            };
            if let Some(old) = self.graph.as_deref_mut() {
                next.adopt(old);
            }
            self.transport.len.store(next.len(), Ordering::Release);
            if let Some(old) = self.graph.replace(next) {
                // Cannot fail: a slot was checked above.
                let _ = self.garbage.push(old);
            }
        }
        while self.synth_garbage.slots() > 0 {
            let Ok((track, synth)) = self.synths.pop() else {
                break;
            };
            let old = match &mut self.graph {
                Some(graph) => graph.swap_synth(track, synth),
                None => synth,
            };
            // Cannot fail: a slot was checked above.
            let _ = self.synth_garbage.push(old);
        }
        while self.insert_garbage.slots() > 0 {
            let Ok((chain, inserts)) = self.inserts.pop() else {
                break;
            };
            let old = match &mut self.graph {
                Some(graph) => graph.swap_inserts(chain, inserts),
                None => inserts,
            };
            // Cannot fail: a slot was checked above.
            let _ = self.insert_garbage.push(old);
        }
        while let Ok(param) = self.params.pop() {
            if let Some(graph) = &mut self.graph {
                graph.apply(param);
            }
        }
    }

    /// Fills an interleaved stereo output buffer: the real-time callback.
    ///
    /// Lock-free, allocation-free and free-free.
    pub fn fill(&mut self, output: &mut [f32]) {
        self.receive();
        let transport = Arc::clone(&self.transport);
        self.fill_with(&transport, output);
        transport.publish_meter(output);
    }

    fn fill_with(&mut self, transport: &Transport, output: &mut [f32]) {
        let start_raw = transport.position.load_raw();
        let Some(graph) = self.graph.as_deref_mut() else {
            output.fill(0.0);
            return;
        };
        if !Cursor::playing(start_raw) {
            output.fill(0.0);
            return;
        }
        // Whole frames only; a stray odd slot gets silence, so the next
        // callback still starts on a left sample.
        let frames_len = output.len() & !1;
        if let Some(extra) = output.get_mut(frames_len..) {
            extra.fill(0.0);
        }
        let output = &mut output[..frames_len];
        let len = graph.len();
        // Read the loop state once, so one callback never mixes two ranges.
        let looping = transport.looping.load(Ordering::Acquire);
        let (start, end) = transport.active_bounds(looping, len);
        let mut pos = Cursor::position(start_raw).min(len) & !1;
        let mut written = 0;
        let mut wrapped = 0u64;
        let mut finished = false;

        if end <= start {
            output.fill(0.0);
            let _ = transport.position.advance(start_raw, 0, false);
            return;
        }

        while written < output.len() {
            if pos >= end {
                if looping {
                    pos = start;
                    wrapped += 1;
                } else {
                    output[written..].fill(0.0);
                    finished = true;
                    break;
                }
            }
            // Stop at loop ends and at quantum edges.
            let chunk = (end - pos).min(output.len() - written).min(QUANTUM * 2);
            graph.render(&mut output[written..written + chunk], pos);
            pos += chunk;
            written += chunk;
        }

        if looping && pos >= end {
            pos = start;
            wrapped += 1;
        } else if !looping && pos >= len {
            finished = true;
        }

        // A control change since `start_raw` wins; this callback's result is
        // then dropped.
        if finished {
            let _ = transport.position.advance(start_raw, 0, false);
        } else {
            let _ = transport.position.advance(start_raw, pos, true);
        }

        if wrapped > 0 {
            transport.loop_wraps.fetch_add(wrapped, Ordering::Relaxed);
        }
    }
}
