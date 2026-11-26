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
    left: Gain,
    right: Gain,
}

/// The master stage after the lanes: gain, then the limiter.
#[derive(Debug)]
struct MasterStage {
    gain: Gain,
    limiter: bool,
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
        let plain =
            render.lanes.is_empty() && !render.limiter && (render.master_gain - 1.0).abs() < 1e-6;
        let fixed = clean(render.fixed.samples, len);
        let mut synths = render.synths;
        synths.resize_with(render.lanes.len(), || None);
        let lanes = render
            .lanes
            .into_iter()
            .zip(synths)
            .zip(render.tracks)
            .zip(render.gains)
            .map(|(((samples, synth), track), (left, right))| Lane {
                track,
                source: match synth {
                    Some(synth) => Source::Synth(Box::new(synth)),
                    None => Source::Buffer(clean(samples, len)),
                },
                left: Gain::new(left),
                right: Gain::new(right),
            })
            .collect();
        Self {
            fixed,
            lanes,
            scratch: vec![0.0; QUANTUM * 2],
            master: (!plain).then(|| MasterStage {
                gain: Gain::new(render.master_gain),
                limiter: render.limiter,
            }),
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

    /// Takes over the gains of the graph it replaces, so a swap neither
    /// jumps nor loses a change sent while the new graph was being built.
    fn adopt(&mut self, old: &Self) {
        for lane in &mut self.lanes {
            if let Some(previous) = old.lanes.iter().find(|entry| entry.track == lane.track) {
                lane.left = previous.left;
                lane.right = previous.right;
            }
        }
        if let (Some(master), Some(previous)) = (&mut self.master, &old.master) {
            master.gain = previous.gain;
        }
    }

    /// Writes the mix of samples `pos..pos + output.len()` into `output`,
    /// ramping every gain to its target across it.
    fn render(&mut self, output: &mut [f32], pos: usize) {
        match self.fixed.get(pos..pos + output.len()) {
            Some(source) => output.copy_from_slice(source),
            None => output.fill(0.0),
        }
        let Some(master) = &mut self.master else {
            return;
        };
        let frames = output.len() / 2;
        for lane in &mut self.lanes {
            let left = lane.left.ramp(frames);
            let right = lane.right.ramp(frames);
            let source = match &mut lane.source {
                Source::Buffer(samples) => samples.get(pos..pos + output.len()),
                Source::Synth(synth) => {
                    // A render call never exceeds one quantum.
                    let Some(scratch) = self.scratch.get_mut(..output.len()) else {
                        continue;
                    };
                    synth.render(scratch, pos);
                    Some(&*scratch)
                }
            };
            let Some(source) = source else {
                continue;
            };
            for (frame, (target, pair)) in output
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
        let gain = master.gain.ramp(frames);
        for (frame, pair) in output.as_chunks_mut::<2>().0.iter_mut().enumerate() {
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
    (
        Engine {
            graph: Some(Box::new(graph)),
            graphs: graph_consumer,
            garbage: garbage_producer,
            params: param_consumer,
            synths: synth_consumer,
            synth_garbage: synth_garbage_producer,
            transport: Arc::clone(&transport),
        },
        EngineControl {
            graphs: graph_producer,
            garbage: garbage_consumer,
            params: param_producer,
            synths: synth_producer,
            synth_garbage: synth_garbage_consumer,
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

    /// Frees graphs and synth lanes the audio thread has handed back.
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
            if let Some(old) = &self.graph {
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
