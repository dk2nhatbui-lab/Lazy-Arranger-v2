//! Realtime playback of a pre-rendered song.
//!
//! The stream callback only copies from a finished buffer, so it does no
//! synthesis or allocation. Position and transport live behind a mutex; the
//! critical section is a memcpy-sized copy.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::RenderedAudio;
use crate::master::{Meter, meter_at};

/// Playback position and buffer, shared with the audio callback.
#[derive(Debug)]
pub struct Playback {
    samples: Arc<Vec<f32>>,
    sample_rate: u32,
    position: usize,
    playing: bool,
}

impl Playback {
    /// Wraps rendered samples.
    #[must_use]
    pub fn new(audio: RenderedAudio) -> Self {
        Self {
            samples: Arc::new(audio.samples),
            sample_rate: audio.sample_rate,
            position: 0,
            playing: false,
        }
    }

    /// Starts or resumes playback from the current position.
    pub fn play(&mut self) {
        if self.position >= self.samples.len() {
            self.position = 0;
        }
        self.playing = true;
    }

    /// Pauses without losing the position.
    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Stops and rewinds.
    pub fn stop(&mut self) {
        self.playing = false;
        self.position = 0;
    }

    /// Moves to a frame without changing the play state.
    pub fn seek_frame(&mut self, frame: usize) {
        self.position = frame.saturating_mul(2).min(self.samples.len());
    }

    /// Moves to a time in seconds without changing the play state.
    pub fn seek_seconds(&mut self, seconds: f64) {
        let frame = (seconds.max(0.0) * f64::from(self.sample_rate)).round() as usize;
        self.seek_frame(frame);
    }

    /// Current position in frames.
    #[must_use]
    pub fn frame(&self) -> usize {
        self.position / 2
    }

    /// Peak/RMS over the next `frames` stereo frames, for a meter.
    #[must_use]
    pub fn meter(&self, frames: usize) -> Meter {
        meter_at(&self.samples, self.position, frames)
    }

    /// True while samples are being produced.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Copies the next frames into an interleaved stereo output buffer.
    pub fn fill(&mut self, output: &mut [f32]) {
        for sample in output {
            if self.playing && self.position < self.samples.len() {
                *sample = self.samples[self.position];
                self.position += 1;
            } else {
                *sample = 0.0;
            }
        }
        if self.position >= self.samples.len() {
            self.playing = false;
            self.position = 0;
        }
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

/// A live output stream.
pub struct Player {
    stream: cpal::Stream,
    playback: Arc<Mutex<Playback>>,
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
        let host = cpal::default_host();
        let device = match name {
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
                })?
            }
            None => host
                .default_output_device()
                .ok_or_else(|| String::from("no default output device"))?,
        };
        let config = device
            .default_output_config()
            .map_err(|error| format!("no output configuration: {error}"))?;
        let playback = Arc::new(Mutex::new(Playback::new(audio)));
        let shared = Arc::clone(&playback);
        let error_callback = |error| eprintln!("audio stream error: {error}");
        let sample_format = config.sample_format();
        let mut stream_config: cpal::StreamConfig = config.into();
        if let Some(frames) = buffer_frames {
            stream_config.buffer_size = cpal::BufferSize::Fixed(frames.max(32));
        }
        let sample_rate = stream_config.sample_rate.0;
        let actual_buffer = match stream_config.buffer_size {
            cpal::BufferSize::Fixed(frames) => Some(frames),
            cpal::BufferSize::Default => None,
        };
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &stream_config,
                move |output: &mut [f32], _| {
                    if let Ok(mut state) = shared.lock() {
                        state.fill(output);
                    } else {
                        output.fill(0.0);
                    }
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &stream_config,
                move |output: &mut [i16], _| {
                    let mut scratch = vec![0.0_f32; output.len()];
                    if let Ok(mut state) = shared.lock() {
                        state.fill(&mut scratch);
                    }
                    for (slot, value) in output.iter_mut().zip(scratch) {
                        *slot = (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
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
        if let Ok(mut state) = self.playback.lock() {
            state.play();
        }
    }

    /// Moves the live playback position, keeping the play state.
    ///
    /// # Errors
    ///
    /// Returns a message when the playback lock is poisoned.
    pub fn seek_seconds(&self, seconds: f64) -> Result<(), String> {
        let mut playback = self
            .playback
            .lock()
            .map_err(|_| String::from("playback lock poisoned"))?;
        playback.seek_seconds(seconds);
        Ok(())
    }

    /// Pauses playback.
    pub fn pause(&self) {
        if let Ok(mut state) = self.playback.lock() {
            state.pause();
        }
    }

    /// Stops and rewinds.
    pub fn stop(&self) {
        if let Ok(mut state) = self.playback.lock() {
            state.stop();
        }
    }

    /// Current frame position.
    #[must_use]
    pub fn frame(&self) -> usize {
        self.playback.lock().map(|state| state.frame()).unwrap_or(0)
    }

    /// True while playing.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playback
            .lock()
            .map(|state| state.is_playing())
            .unwrap_or(false)
    }

    /// Master peak/RMS over a short window at the current position.
    #[must_use]
    pub fn meter(&self) -> Meter {
        self.playback
            .lock()
            .map(|state| state.meter(state.sample_rate as usize / 20))
            .unwrap_or_default()
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
        let samples = (0..frames * 2).map(|index| index as f32).collect();
        Playback::new(RenderedAudio {
            samples,
            sample_rate: 48_000,
        })
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
        let mut playback = Playback::new(RenderedAudio {
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
        let mut state = playback_with(4);
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
        let mut state = playback_with(96_000);
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
        let mut state = playback_with(2);
        state.play();
        let mut output = [0.0_f32; 4];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 1.0, 2.0, 3.0]);
        assert!(!state.is_playing());
        assert_eq!(state.frame(), 0);
    }

    #[test]
    fn pause_keeps_the_position_and_stop_rewinds() {
        let mut state = playback_with(8);
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
        let mut state = playback_with(2);
        state.play();
        let mut output = [0.5_f32; 3];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 1.0, 2.0]);
    }
}
