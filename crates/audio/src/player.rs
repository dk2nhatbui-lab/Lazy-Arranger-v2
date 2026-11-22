//! Realtime playback of a pre-rendered song.
//!
//! The stream callback only copies from a finished buffer, so it does no
//! synthesis or allocation. Position and transport live behind a mutex; the
//! critical section is a memcpy-sized copy.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::RenderedAudio;

/// Playback position and buffer, shared with the audio callback.
#[derive(Debug)]
pub struct Playback {
    samples: Arc<Vec<f32>>,
    position: usize,
    playing: bool,
}

impl Playback {
    /// Wraps rendered samples.
    #[must_use]
    pub fn new(audio: RenderedAudio) -> Self {
        Self {
            samples: Arc::new(audio.samples),
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

    /// Current position in frames.
    #[must_use]
    pub fn frame(&self) -> usize {
        self.position / 2
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

/// A live output stream.
pub struct Player {
    stream: cpal::Stream,
    playback: Arc<Mutex<Playback>>,
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
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| String::from("no default output device"))?;
        let config = device
            .default_output_config()
            .map_err(|error| format!("no output configuration: {error}"))?;
        let playback = Arc::new(Mutex::new(Playback::new(audio)));
        let shared = Arc::clone(&playback);
        let error_callback = |error| eprintln!("audio stream error: {error}");
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config.into(),
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
                &config.into(),
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
        Ok(Self { stream, playback })
    }

    /// Starts playback from the current position.
    pub fn play(&self) {
        if let Ok(mut state) = self.playback.lock() {
            state.play();
        }
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
