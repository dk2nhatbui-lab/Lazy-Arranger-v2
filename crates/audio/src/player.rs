//! Realtime playback of a pre-rendered song.
//!
//! The stream callback only copies from an immutable finished buffer and updates
//! atomic transport fields, so it does no synthesis, locking, or allocation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::RenderedAudio;
use crate::master::{Meter, meter_at};

/// Playback position and buffer, shared lock-free with the audio callback.
#[derive(Debug)]
pub struct Playback {
    samples: Vec<f32>,
    sample_rate: u32,
    position: AtomicUsize,
    playing: AtomicBool,
    looping: AtomicBool,
    loop_start: AtomicUsize,
    loop_end: AtomicUsize,
    loop_wraps: AtomicU64,
}

impl Playback {
    /// Wraps rendered samples.
    #[must_use]
    pub fn new(audio: RenderedAudio) -> Self {
        let len = audio.samples.len();
        Self {
            samples: audio.samples,
            sample_rate: audio.sample_rate,
            position: AtomicUsize::new(0),
            playing: AtomicBool::new(false),
            looping: AtomicBool::new(false),
            loop_start: AtomicUsize::new(0),
            loop_end: AtomicUsize::new(len),
            loop_wraps: AtomicU64::new(0),
        }
    }

    /// Sample rate of the underlying buffer in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Starts or resumes playback from the current position.
    pub fn play(&self) {
        let (start, end) = self.active_bounds();
        let pos = self.position.load(Ordering::Acquire);
        if self.looping.load(Ordering::Acquire) {
            if pos < start || pos >= end {
                self.position.store(start, Ordering::Release);
            }
        } else if pos >= self.samples.len() {
            self.position.store(0, Ordering::Release);
        }
        self.playing.store(true, Ordering::Release);
    }

    /// Pauses without losing the position.
    pub fn pause(&self) {
        self.playing.store(false, Ordering::Release);
    }

    /// Stops and rewinds.
    pub fn stop(&self) {
        self.playing.store(false, Ordering::Release);
        self.position.store(0, Ordering::Release);
    }

    /// Moves to a frame without changing the play state.
    pub fn seek_frame(&self, frame: usize) {
        self.position.store(
            frame.saturating_mul(2).min(self.samples.len()),
            Ordering::Release,
        );
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
        self.loop_start.store(start, Ordering::Relaxed);
        self.loop_end.store(end, Ordering::Relaxed);
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
        self.position.load(Ordering::Acquire) / 2
    }

    /// Current position in seconds using the playback sample rate.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.frame() as f64 / f64::from(self.sample_rate.max(1))
    }

    /// Peak/RMS over the next `frames` stereo frames, for a meter.
    #[must_use]
    pub fn meter(&self, frames: usize) -> Meter {
        meter_at(&self.samples, self.position.load(Ordering::Acquire), frames)
    }

    /// True while samples are being produced.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Acquire)
    }

    fn active_bounds(&self) -> (usize, usize) {
        let len = self.samples.len();
        if !self.looping.load(Ordering::Acquire) {
            return (0, len);
        }
        let raw_start = self.loop_start.load(Ordering::Relaxed).min(len);
        let raw_end = self.loop_end.load(Ordering::Relaxed);
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
        if !self.playing.load(Ordering::Acquire) {
            output.fill(0.0);
            return;
        }
        let looping = self.looping.load(Ordering::Acquire);
        let (start, end) = self.active_bounds();
        let start_pos = self.position.load(Ordering::Acquire);
        let mut pos = start_pos;
        let mut written = 0;
        let mut wrapped = 0u64;
        let mut finished = false;

        if end <= start {
            output.fill(0.0);
            if self
                .position
                .compare_exchange(start_pos, 0, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                self.playing.store(false, Ordering::Release);
            }
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
            let available = end - pos;
            let needed = output.len() - written;
            let chunk = available.min(needed);
            output[written..written + chunk].copy_from_slice(&self.samples[pos..pos + chunk]);
            pos += chunk;
            written += chunk;
        }

        if looping && pos >= end {
            pos = start;
            wrapped += 1;
        } else if !looping && pos >= self.samples.len() {
            finished = true;
        }

        if finished {
            if self
                .position
                .compare_exchange(start_pos, 0, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                self.playing.store(false, Ordering::Release);
            }
        } else {
            let _ =
                self.position
                    .compare_exchange(start_pos, pos, Ordering::AcqRel, Ordering::Relaxed);
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

/// Resamples interleaved stereo audio with linear interpolation.
#[must_use]
fn resample_stereo(audio: RenderedAudio, target_rate: u32) -> RenderedAudio {
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
        let device = find_output_device(name)?;
        let config = stereo_config(&device, audio.sample_rate)?;
        let sample_format = config.sample_format();
        let mut stream_config: cpal::StreamConfig = config.into();
        if let Some(frames) = buffer_frames {
            stream_config.buffer_size = cpal::BufferSize::Fixed(frames.max(32));
        }
        let sample_rate = stream_config.sample_rate.0;
        let audio = resample_stereo(audio, sample_rate);
        let playback = Arc::new(Playback::new(audio));
        let shared = Arc::clone(&playback);
        let error_callback = |error| eprintln!("audio stream error: {error}");
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
                            *slot = (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
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
    fn mono_buffers_are_filled_with_silence() {
        let state = playback_with(2);
        state.play();
        let mut output = [0.5_f32; 3];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 1.0, 2.0]);
    }
}
