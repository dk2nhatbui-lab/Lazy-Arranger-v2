//! Realtime playback of a pre-rendered song.
//!
//! The stream callback only copies from an immutable finished buffer and updates
//! atomic transport fields, so it does no synthesis, locking, or allocation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::RenderedAudio;
use crate::live::LiveRender;
use crate::master::{Meter, meter_at};

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

/// A gain the control side sets and the callback ramps to, as `f32` bits.
#[derive(Debug)]
struct LiveGain {
    target: AtomicU32,
    current: AtomicU32,
}

impl LiveGain {
    fn new(value: f32) -> Self {
        Self {
            target: AtomicU32::new(value.to_bits()),
            current: AtomicU32::new(value.to_bits()),
        }
    }

    fn set(&self, value: f32) {
        let value = if value.is_finite() { value } else { 0.0 };
        self.target.store(value.to_bits(), Ordering::Release);
    }

    fn target(&self) -> f32 {
        f32::from_bits(self.target.load(Ordering::Acquire))
    }

    /// Start and end of this callback's ramp; the end becomes the next start.
    fn ramp(&self) -> (f32, f32) {
        let target = self.target();
        let current = f32::from_bits(self.current.swap(target.to_bits(), Ordering::AcqRel));
        (current, target)
    }
}

/// A live track buffer with its left and right gains.
#[derive(Debug)]
struct Lane {
    samples: Vec<f32>,
    track: project::TrackId,
    left: LiveGain,
    right: LiveGain,
}

/// Gains that one callback ramps across its output.
struct Ramp {
    start: f32,
    step: f32,
}

impl Ramp {
    fn new((start, end): (f32, f32), frames: usize) -> Self {
        Self {
            start,
            step: (end - start) / frames.max(1) as f32,
        }
    }

    fn at(&self, frame: usize) -> f32 {
        self.start + self.step * frame as f32
    }
}

/// Live mixing stages: lanes summed with gains, then the master.
#[derive(Debug)]
struct Live {
    lanes: Vec<Lane>,
    master_gain: LiveGain,
    limiter: bool,
}

/// Playback position and buffer, shared lock-free with the audio callback.
#[derive(Debug)]
pub struct Playback {
    samples: Vec<f32>,
    sample_rate: u32,
    position: Cursor,
    looping: AtomicBool,
    loop_bounds: AtomicU64,
    loop_wraps: AtomicU64,
    live: Option<Live>,
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

impl Playback {
    /// Wraps rendered samples.
    #[must_use]
    pub fn new(audio: RenderedAudio) -> Self {
        let len = audio.samples.len() & !1;
        let samples = clean(audio.samples, len);
        Self {
            samples,
            sample_rate: audio.sample_rate,
            position: Cursor::new(),
            looping: AtomicBool::new(false),
            loop_bounds: AtomicU64::new(pack_loop(0, len)),
            loop_wraps: AtomicU64::new(0),
            live: None,
        }
    }

    /// Wraps a live render; its lane gains and master gain can change while
    /// it plays.
    #[must_use]
    pub fn new_live(render: LiveRender) -> Self {
        let mut playback = Self::new(render.fixed);
        let len = playback.samples.len();
        let lanes = render
            .lanes
            .into_iter()
            .zip(render.tracks)
            .zip(render.gains)
            .map(|((samples, track), (left, right))| Lane {
                samples: clean(samples, len),
                track,
                left: LiveGain::new(left),
                right: LiveGain::new(right),
            })
            .collect();
        playback.live = Some(Live {
            lanes,
            master_gain: LiveGain::new(render.master_gain),
            limiter: render.limiter,
        });
        playback
    }

    /// Sets the gains of live tracks from the song's mixer, and the master
    /// gain. Heard from the next audio buffer, with a short ramp.
    pub fn apply_mix(&self, song: &project::song::Song, master_gain_db: f32) {
        let Some(live) = &self.live else {
            return;
        };
        for lane in &live.lanes {
            let (left, right) = song
                .tracks
                .iter()
                .find(|track| track.id == lane.track)
                .map_or((0.0, 0.0), |track| crate::live::lane_gains(song, track));
            lane.left.set(left);
            lane.right.set(right);
        }
        live.master_gain.set(crate::master::gain(master_gain_db));
    }

    /// Adds the live lanes and runs the master over `output`, which holds
    /// fixed samples copied from `pos`. `offset` is the output's first frame
    /// in this callback, for the gain ramps.
    fn mix_live(
        live: &Live,
        ramps: &[(Ramp, Ramp)],
        master: &Ramp,
        output: &mut [f32],
        pos: usize,
        offset: usize,
    ) {
        for (lane, (left, right)) in live.lanes.iter().zip(ramps) {
            let Some(source) = lane.samples.get(pos..pos + output.len()) else {
                continue;
            };
            for (frame, (target, pair)) in output
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip(source.as_chunks::<2>().0)
                .enumerate()
            {
                let at = offset + frame;
                target[0] += pair[0] * left.at(at);
                target[1] += pair[1] * right.at(at);
            }
        }
        for (frame, pair) in output.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            let gain = master.at(offset + frame);
            pair[0] = crate::master::finish(pair[0] * gain, live.limiter);
            pair[1] = crate::master::finish(pair[1] * gain, live.limiter);
        }
    }

    /// Sample rate of the underlying buffer in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Starts or resumes playback from the current position.
    pub fn play(&self) {
        let looping = self.looping.load(Ordering::Acquire);
        let (start, end) = self.active_bounds(looping);
        let len = self.samples.len();
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
        self.position
            .store(frame.saturating_mul(2).min(self.samples.len()));
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
        let len = self.samples.len() & !1;
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

    /// Current position in seconds using the playback sample rate.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.frame() as f64 / f64::from(self.sample_rate.max(1))
    }

    /// Peak/RMS over the next `frames` stereo frames, for a meter.
    #[must_use]
    pub fn meter(&self, frames: usize) -> Meter {
        let pos = self.position.load();
        let Some(live) = &self.live else {
            return meter_at(&self.samples, pos, frames);
        };
        let end = pos
            .saturating_add(frames.saturating_mul(2))
            .min(self.samples.len());
        let Some(window) = self.samples.get(pos..end) else {
            return Meter::default();
        };
        let mut window = window.to_vec();
        let ramps: Vec<(Ramp, Ramp)> = live
            .lanes
            .iter()
            .map(|lane| {
                let left = lane.left.target();
                let right = lane.right.target();
                (Ramp::new((left, left), 1), Ramp::new((right, right), 1))
            })
            .collect();
        let gain = live.master_gain.target();
        Self::mix_live(
            live,
            &ramps,
            &Ramp::new((gain, gain), 1),
            &mut window,
            pos,
            0,
        );
        meter_at(&window, 0, frames)
    }

    /// True while samples are being produced.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        Cursor::playing(self.position.load_raw())
    }

    fn active_bounds(&self, looping: bool) -> (usize, usize) {
        let len = self.samples.len();
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

    /// Copies the next frames into an interleaved stereo output buffer.
    ///
    /// Lock-free and allocation-free for safe use in a realtime audio callback.
    pub fn fill(&self, output: &mut [f32]) {
        let start_raw = self.position.load_raw();
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
        // Read the loop state once, so one callback never mixes two ranges.
        let looping = self.looping.load(Ordering::Acquire);
        let (start, end) = self.active_bounds(looping);
        let mut pos = Cursor::position(start_raw).min(self.samples.len()) & !1;
        let mut written = 0;
        let mut wrapped = 0u64;
        let mut finished = false;

        if end <= start {
            output.fill(0.0);
            let _ = self.position.advance(start_raw, 0, false);
            return;
        }

        // Ramp the live gains once across the whole output. The ramps are
        // built in a fixed-size array so the callback does not allocate.
        let frames = output.len() / 2;
        let live = self.live.as_ref();
        let mut ramps: [(Ramp, Ramp); crate::live::MAX_LANES] =
            std::array::from_fn(|_| (Ramp::new((0.0, 0.0), 1), Ramp::new((0.0, 0.0), 1)));
        let mut lane_count = 0;
        let master = live.map_or_else(
            || Ramp::new((1.0, 1.0), 1),
            |live| Ramp::new(live.master_gain.ramp(), frames),
        );
        if let Some(live) = live {
            for (slot, lane) in ramps.iter_mut().zip(&live.lanes) {
                *slot = (
                    Ramp::new(lane.left.ramp(), frames),
                    Ramp::new(lane.right.ramp(), frames),
                );
                lane_count += 1;
            }
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
            let available = end - pos;
            let needed = output.len() - written;
            let chunk = available.min(needed);
            let target = &mut output[written..written + chunk];
            target.copy_from_slice(&self.samples[pos..pos + chunk]);
            if let Some(live) = live {
                Self::mix_live(
                    live,
                    &ramps[..lane_count],
                    &master,
                    target,
                    pos,
                    written / 2,
                );
            }
            pos += chunk;
            written += chunk;
        }

        if looping && pos >= end {
            pos = start;
            wrapped += 1;
        } else if !looping && pos >= self.samples.len() {
            finished = true;
        }

        // A control change since `start_raw` wins; this callback's result is
        // then dropped.
        if finished {
            let _ = self.position.advance(start_raw, 0, false);
        } else {
            let _ = self.position.advance(start_raw, pos, true);
        }

        if wrapped > 0 {
            self.loop_wraps.fetch_add(wrapped, Ordering::Relaxed);
        }
    }
}

/// Picks a supported stereo output configuration at the nearest rate.
fn stereo_config(
    device: &cpal::Device,
    source_rate: u32,
) -> Result<cpal::SupportedStreamConfig, String> {
    let configs = device
        .supported_output_configs()
        .map_err(|error| format!("no output configurations: {error}"))?;
    let mut best: Option<cpal::SupportedStreamConfig> = None;
    let mut best_distance = u32::MAX;
    for range in configs {
        if range.channels() != 2
            || !matches!(
                range.sample_format(),
                cpal::SampleFormat::F32 | cpal::SampleFormat::I16
            )
        {
            continue;
        }
        let min = range.min_sample_rate().0;
        let max = range.max_sample_rate().0;
        let rate = source_rate.clamp(min, max);
        let distance = source_rate.abs_diff(rate);
        if distance < best_distance {
            best_distance = distance;
            best = Some(range.with_sample_rate(cpal::SampleRate(rate)));
        }
    }
    best.ok_or_else(|| String::from("the output device has no stereo F32/I16 configuration"))
}

/// Resamples every part of a live render.
fn resample_live(mut render: LiveRender, target_rate: u32) -> LiveRender {
    let source_rate = render.fixed.sample_rate;
    render.fixed = resample_stereo(std::mem::take(&mut render.fixed), target_rate);
    for lane in &mut render.lanes {
        *lane = resample_stereo(
            RenderedAudio {
                samples: std::mem::take(lane),
                sample_rate: source_rate,
            },
            target_rate,
        )
        .samples;
    }
    render
}

/// Resamples interleaved stereo audio with linear interpolation.
#[must_use]
pub(crate) fn resample_stereo(audio: RenderedAudio, target_rate: u32) -> RenderedAudio {
    if audio.sample_rate == target_rate || audio.sample_rate == 0 || target_rate == 0 {
        return audio;
    }
    let source_frames = audio.samples.len() / 2;
    let ratio = f64::from(target_rate) / f64::from(audio.sample_rate);
    let target_frames = ((source_frames as f64) * ratio).ceil() as usize;
    let mut samples = Vec::with_capacity(target_frames.saturating_mul(2));
    for frame in 0..target_frames {
        let source = frame as f64 / ratio;
        let index = source.floor() as usize;
        let fraction = (source - index as f64) as f32;
        let next = (index + 1).min(source_frames.saturating_sub(1));
        for channel in 0..2 {
            let a = audio
                .samples
                .get(index * 2 + channel)
                .copied()
                .unwrap_or(0.0);
            let b = audio.samples.get(next * 2 + channel).copied().unwrap_or(a);
            samples.push(a + (b - a) * fraction);
        }
    }
    RenderedAudio {
        samples,
        sample_rate: target_rate,
    }
}

/// The name of the audio system in use, such as "CoreAudio", "ALSA" or "WASAPI".
#[must_use]
pub fn host_name() -> String {
    cpal::default_host().id().name().to_string()
}

/// Names of available output devices.
#[must_use]
pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices()
        .map(|devices| {
            devices
                .filter_map(|device| device.name().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn find_output_device(name: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    match name {
        Some(name) => {
            let mut available = Vec::new();
            let mut found = None;
            if let Ok(devices) = host.output_devices() {
                for device in devices {
                    let label = device.name().unwrap_or_else(|_| String::from("unknown"));
                    if label == name {
                        found = Some(device);
                        break;
                    }
                    available.push(label);
                }
            }
            found.ok_or_else(|| {
                format!(
                    "no output named {name:?}; available: {}",
                    available.join(", ")
                )
            })
        }
        None => host
            .default_output_device()
            .ok_or_else(|| String::from("no default output device")),
    }
}

/// Preferred stereo sample rate for an output device, or `fallback` if unavailable.
///
/// Callers can render directly at this rate on a background thread so opening
/// [`Player::new_on`] needs no resampling on the UI thread.
#[must_use]
pub fn preferred_sample_rate(name: Option<&str>, fallback: u32) -> u32 {
    find_output_device(name)
        .and_then(|device| stereo_config(&device, fallback))
        .map(|config| config.sample_rate().0)
        .unwrap_or(fallback)
}

/// A live output stream.
pub struct Player {
    stream: cpal::Stream,
    playback: Arc<Playback>,
    sample_rate: u32,
    buffer_frames: Option<u32>,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Player").finish_non_exhaustive()
    }
}

impl Player {
    /// Opens the default output device and starts paused.
    ///
    /// # Errors
    ///
    /// Returns a message when no device or supported configuration exists.
    pub fn new(audio: RenderedAudio) -> Result<Self, String> {
        Self::new_on(audio, None, None)
    }

    /// Opens a named output device with an optional fixed buffer size.
    ///
    /// # Errors
    ///
    /// Returns a message when the device is missing or has no configuration.
    pub fn new_on(
        audio: RenderedAudio,
        name: Option<&str>,
        buffer_frames: Option<u32>,
    ) -> Result<Self, String> {
        Self::new_live_on(LiveRender::from_audio(audio), name, buffer_frames)
    }

    /// Opens a named output device for a live render, whose track gains and
    /// master gain follow [`Player::apply_mix`] while it plays.
    ///
    /// # Errors
    ///
    /// Returns a message when the device is missing or has no configuration.
    pub fn new_live_on(
        render: LiveRender,
        name: Option<&str>,
        buffer_frames: Option<u32>,
    ) -> Result<Self, String> {
        let device = find_output_device(name)?;
        let config = stereo_config(&device, render.sample_rate())?;
        let sample_format = config.sample_format();
        let mut stream_config: cpal::StreamConfig = config.into();
        if let Some(frames) = buffer_frames {
            stream_config.buffer_size = cpal::BufferSize::Fixed(frames.max(32));
        }
        let sample_rate = stream_config.sample_rate.0;
        let plain =
            render.lanes.is_empty() && !render.limiter && (render.master_gain - 1.0).abs() < 1e-6;
        let playback = Arc::new(if plain {
            Playback::new(resample_stereo(render.fixed, sample_rate))
        } else {
            Playback::new_live(resample_live(render, sample_rate))
        });
        let shared = Arc::clone(&playback);
        // Report the first stream error only, so a failing device cannot
        // flood stderr from the audio thread.
        let reported = Arc::new(AtomicBool::new(false));
        let error_callback = move |error| {
            if !reported.swap(true, Ordering::Relaxed) {
                eprintln!("audio stream error: {error}");
            }
        };
        let actual_buffer = match stream_config.buffer_size {
            cpal::BufferSize::Fixed(frames) => Some(frames),
            cpal::BufferSize::Default => None,
        };
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &stream_config,
                move |output: &mut [f32], _| {
                    shared.fill(output);
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &stream_config,
                move |output: &mut [i16], _| {
                    let mut chunk_buf = [0.0_f32; 512];
                    for dst in output.chunks_mut(chunk_buf.len()) {
                        let slice = &mut chunk_buf[..dst.len()];
                        shared.fill(slice);
                        for (slot, value) in dst.iter_mut().zip(slice.iter().copied()) {
                            *slot = (crate::spsc::sanitize(value) * f32::from(i16::MAX)) as i16;
                        }
                    }
                },
                error_callback,
                None,
            ),
            other => return Err(format!("unsupported sample format {other:?}")),
        }
        .map_err(|error| format!("could not open the output stream: {error}"))?;
        stream
            .play()
            .map_err(|error| format!("could not start the stream: {error}"))?;
        Ok(Self {
            stream,
            playback,
            sample_rate,
            buffer_frames: actual_buffer,
        })
    }

    /// Output sample rate.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Actual fixed output buffer size, when known.
    #[must_use]
    pub fn buffer_frames(&self) -> Option<u32> {
        self.buffer_frames
    }

    /// Buffer duration in milliseconds, when known.
    #[must_use]
    pub fn buffer_latency_ms(&self) -> Option<f64> {
        self.buffer_frames
            .map(|frames| f64::from(frames) / f64::from(self.sample_rate.max(1)) * 1000.0)
    }

    /// Starts playback from the current position.
    pub fn play(&self) {
        self.playback.play();
    }

    /// Moves the live playback position, keeping the play state.
    ///
    /// # Errors
    ///
    /// Always succeeds; retained as `Result` for caller compatibility.
    pub fn seek_seconds(&self, seconds: f64) -> Result<(), String> {
        self.playback.seek_seconds(seconds);
        Ok(())
    }

    /// Enables or disables seamless in-callback looping over a time range.
    pub fn set_loop(&self, enabled: bool, start_seconds: f64, end_seconds: f64) {
        self.playback.set_loop(enabled, start_seconds, end_seconds);
    }

    /// Number of times playback wrapped from loop end back to loop start.
    #[must_use]
    pub fn loop_wraps(&self) -> u64 {
        self.playback.loop_wraps()
    }

    /// Pauses playback.
    pub fn pause(&self) {
        self.playback.pause();
    }

    /// Stops and rewinds.
    pub fn stop(&self) {
        self.playback.stop();
    }

    /// Current frame position.
    #[must_use]
    pub fn frame(&self) -> usize {
        self.playback.frame()
    }

    /// Current playback time in seconds using the output device's sample rate.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.playback.seconds()
    }

    /// True while playing.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playback.is_playing()
    }

    /// Sets live track gains from the song's mixer and the master gain.
    pub fn apply_mix(&self, song: &project::song::Song, master_gain_db: f32) {
        self.playback.apply_mix(song, master_gain_db);
    }

    /// Master peak/RMS over a short window at the current position.
    #[must_use]
    pub fn meter(&self) -> Meter {
        self.playback.meter(self.sample_rate as usize / 20)
    }

    /// Drops the stream; the device is released.
    pub fn release(self) {
        drop(self.stream);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playback_with(frames: usize) -> Playback {
        playback_with_rate(frames, 48_000)
    }

    fn playback_with_rate(frames: usize, sample_rate: u32) -> Playback {
        let samples = (0..frames * 2).map(|index| index as f32).collect();
        Playback::new(RenderedAudio {
            samples,
            sample_rate,
        })
    }

    #[test]
    fn resampling_preserves_duration_and_channels() {
        let audio = RenderedAudio {
            samples: (0..16).map(|value| value as f32).collect(),
            sample_rate: 48_000,
        };
        let half = resample_stereo(audio.clone(), 24_000);
        assert_eq!(half.sample_rate, 24_000);
        assert_eq!(half.samples.len(), 8);
        assert_eq!(half.samples[0], 0.0);
        assert_eq!(half.samples[1], 1.0);
        assert_eq!(half.samples[6], 12.0);
        let same = resample_stereo(audio, 48_000);
        assert_eq!(same.samples.len(), 16);
    }

    #[test]
    fn seconds_uses_device_sample_rate() {
        for rate in [44_100_u32, 48_000, 96_000] {
            let state = playback_with_rate(rate as usize * 2, rate);
            state.seek_frame(rate as usize);
            assert!(
                (state.seconds() - 1.0).abs() < 1.0e-9,
                "1s at {rate} Hz should report 1.0s, got {}",
                state.seconds()
            );
            state.seek_seconds(1.5);
            assert_eq!(state.frame(), (rate as usize * 3) / 2);
            assert!(
                (state.seconds() - 1.5).abs() < 1.0e-6,
                "1.5s at {rate} Hz should report 1.5s, got {}",
                state.seconds()
            );
        }
    }

    #[test]
    fn looping_wraps_seamlessly_across_buffer_boundaries() {
        // 44.1 kHz playback looping from 0.25s..0.75s (half-second region = 22,050 frames).
        let state = playback_with_rate(44_100, 44_100);
        state.set_loop(true, 0.25, 0.75);
        state.seek_seconds(0.25);
        state.play();

        let start_frame = 44_100 / 4;
        let end_frame = (44_100 * 3) / 4;
        let loop_frames = end_frame - start_frame;
        assert_eq!(state.frame(), start_frame);

        // Drain 2.5 loop iterations in odd-sized chunks to cross loop boundaries mid-buffer.
        let mut out = vec![0.0_f32; 512];
        let mut expected_frame = start_frame;
        let total_chunks = (loop_frames * 5) / (2 * 256);
        for _ in 0..total_chunks {
            state.fill(&mut out);
            for frame_slice in out.as_chunks::<2>().0 {
                let expected_left = (expected_frame * 2) as f32;
                let expected_right = (expected_frame * 2 + 1) as f32;
                assert_eq!(frame_slice[0], expected_left);
                assert_eq!(frame_slice[1], expected_right);
                expected_frame += 1;
                if expected_frame >= end_frame {
                    expected_frame = start_frame;
                }
            }
        }
        assert!(state.is_playing());
        assert_eq!(state.loop_wraps(), 2);
        assert!(state.seconds() >= 0.25 && state.seconds() < 0.75);
    }

    #[test]
    fn concurrent_meter_and_frame_reads_never_drop_samples() {
        let frames = 44_100;
        let samples = vec![0.5_f32; frames * 2];
        let playback = Arc::new(Playback::new(RenderedAudio {
            samples,
            sample_rate: 44_100,
        }));
        playback.set_loop(true, 0.0, 1.0);
        playback.play();

        let reader = Arc::clone(&playback);
        let done = Arc::new(AtomicBool::new(false));
        let done_reader = Arc::clone(&done);
        let ui_thread = std::thread::spawn(move || {
            while !done_reader.load(Ordering::Relaxed) {
                let meter = reader.meter(2048);
                assert!((meter.peak - 0.5).abs() < 1.0e-6);
                let _ = reader.seconds();
            }
        });

        let mut buffer = [0.0_f32; 256];
        for _ in 0..200 {
            playback.fill(&mut buffer);
            assert!(
                buffer.iter().all(|&sample| (sample - 0.5).abs() < 1.0e-6),
                "lock-free callback must never output silence dropouts"
            );
        }
        done.store(true, Ordering::Relaxed);
        ui_thread.join().expect("ui reader thread");
    }

    #[test]
    fn a_missing_output_device_is_reported() {
        let error = Player::new_on(
            RenderedAudio {
                samples: vec![0.0; 2],
                sample_rate: 48_000,
            },
            Some("lazy-arranger-no-such-device"),
            None,
        )
        .expect_err("missing device");
        assert!(error.contains("no output named"), "{error}");
    }

    #[test]
    fn the_playback_meter_reports_the_current_window() {
        let playback = Playback::new(RenderedAudio {
            samples: vec![0.5, -0.5, 1.0, -1.0],
            sample_rate: 48_000,
        });
        let meter = playback.meter(1);
        assert!((meter.peak - 0.5).abs() < 1.0e-6);
        assert!((meter.rms - 0.5).abs() < 1.0e-6);
        playback.seek_frame(1);
        assert!((playback.meter(1).peak - 1.0).abs() < 1.0e-6);
        playback.seek_frame(2);
        assert_eq!(playback.meter(1), Meter::default());
    }

    #[test]
    fn fill_copies_samples_only_while_playing() {
        let state = playback_with(4);
        let mut output = [1.0_f32; 4];
        state.fill(&mut output);
        assert_eq!(output, [0.0; 4], "paused means silence");

        state.play();
        let mut output = [0.0_f32; 4];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 1.0, 2.0, 3.0]);
        assert_eq!(state.frame(), 2);
        assert!(state.is_playing());
    }

    #[test]
    fn seeking_moves_without_changing_the_play_state() {
        let state = playback_with(96_000);
        state.play();
        state.seek_seconds(0.5);
        assert_eq!(state.frame(), 24_000);
        assert!(state.is_playing());
        state.pause();
        state.seek_frame(10);
        assert_eq!(state.frame(), 10);
        assert!(!state.is_playing());
        state.seek_frame(usize::MAX);
        assert_eq!(state.frame(), 96_000);
    }

    #[test]
    fn reaching_the_end_stops_and_rewinds() {
        let state = playback_with(2);
        state.play();
        let mut output = [0.0_f32; 4];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 1.0, 2.0, 3.0]);
        assert!(!state.is_playing());
        assert_eq!(state.frame(), 0);
    }

    #[test]
    fn pause_keeps_the_position_and_stop_rewinds() {
        let state = playback_with(8);
        state.play();
        let mut output = [0.0_f32; 4];
        state.fill(&mut output);
        state.pause();
        assert_eq!(state.frame(), 2);
        state.play();
        let mut output = [0.0_f32; 4];
        state.fill(&mut output);
        assert_eq!(output, [4.0, 5.0, 6.0, 7.0]);
        state.stop();
        assert_eq!(state.frame(), 0);
        assert!(!state.is_playing());
    }

    #[test]
    fn an_odd_output_slot_gets_silence_and_frames_stay_aligned() {
        let state = playback_with(4);
        state.play();
        let mut output = [0.5_f32; 3];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 1.0, 0.0]);
        let mut output = [0.5_f32; 2];
        state.fill(&mut output);
        assert_eq!(output, [2.0, 3.0]);
    }

    #[test]
    fn non_finite_samples_play_as_silence() {
        let state = Playback::new(RenderedAudio {
            sample_rate: 48_000,
            samples: vec![f32::NAN, f32::INFINITY, 0.25],
        });
        state.play();
        let mut output = [1.0_f32; 2];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 0.0]);
    }

    fn live_song(volume: u8) -> project::song::Song {
        let mut song = project::song::Song::default();
        let mut track = project::song::Track::new(project::TrackId::new(1).expect("track"), "Keys");
        track.volume = volume;
        song.tracks.push(track);
        song
    }

    fn live_playback() -> Playback {
        let song = live_song(100);
        Playback::new_live(LiveRender {
            fixed: RenderedAudio {
                samples: vec![0.0; 2048],
                sample_rate: 48_000,
            },
            lanes: vec![vec![0.5; 2048]],
            tracks: vec![song.tracks[0].id],
            gains: vec![(1.0, 1.0)],
            master_gain: 1.0,
            limiter: false,
        })
    }

    #[test]
    fn a_fader_change_is_heard_in_the_next_buffer() {
        let playback = live_playback();
        playback.play();
        let mut output = [0.0_f32; 64];
        playback.fill(&mut output);
        assert!(output.iter().all(|sample| (sample - 0.5).abs() < 1e-6));

        playback.apply_mix(&live_song(50), 0.0);
        playback.fill(&mut output);
        // One buffer ramps from the old gain to the new one, without a click.
        assert!((output[0] - 0.5).abs() < 1e-6);
        assert!((output[62] - 0.25).abs() < 0.01);
        assert!(output.windows(4).all(|pair| pair[2] <= pair[0] + 1e-6));
        playback.fill(&mut output);
        assert!(output.iter().all(|sample| (sample - 0.25).abs() < 1e-6));
    }

    #[test]
    fn mute_and_master_gain_apply_live() {
        let playback = live_playback();
        playback.play();
        let mut song = live_song(100);
        song.tracks[0].muted = true;
        playback.apply_mix(&song, 0.0);
        let mut output = [0.0_f32; 64];
        playback.fill(&mut output);
        playback.fill(&mut output);
        assert!(output.iter().all(|sample| sample.abs() < 1e-6));

        playback.apply_mix(&live_song(100), -6.0);
        playback.fill(&mut output);
        playback.fill(&mut output);
        let expected = 0.5 * crate::master::gain(-6.0);
        assert!(output.iter().all(|sample| (sample - expected).abs() < 1e-5));
        assert!((playback.meter(16).peak - expected).abs() < 1e-5);
    }
}
