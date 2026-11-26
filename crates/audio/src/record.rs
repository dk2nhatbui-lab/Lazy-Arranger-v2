//! Recording from the default audio input.
//!
//! Opens the default device through cpal, captures f32/i16/u16 samples until
//! the requested duration ends, mixes channels to mono and returns a stereo
//! buffer. Microphone permission and device availability are outside this
//! module's control.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::RenderedAudio;
use crate::spsc::SampleQueue;

/// Averages interleaved channels into mono frames.
#[must_use]
pub fn mix_channels(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels == 0 {
        return Vec::new();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Names of the available audio input devices.
#[must_use]
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devices| devices.filter_map(|device| device.name().ok()).collect())
        .unwrap_or_default()
}

/// Records the default input for `seconds`, returning a stereo buffer.
///
/// # Errors
///
/// Returns a message when there is no input device, the format is unsupported,
/// the stream cannot start, or the device reports an error.
pub fn record_seconds(seconds: f64) -> Result<RenderedAudio, String> {
    record_seconds_from(None, seconds)
}

/// Records a named input (or the default when `device_name` is `None`).
///
/// # Errors
///
/// Returns a message when the named device is absent, the format is unsupported,
/// the stream cannot start, or the device reports an error.
pub fn record_seconds_from(
    device_name: Option<&str>,
    seconds: f64,
) -> Result<RenderedAudio, String> {
    record_seconds_from_channel(device_name, None, seconds)
}

/// Picks one interleaved channel, or mixes all channels when `channel` is None.
#[must_use]
pub fn channel_samples(interleaved: &[f32], channels: usize, channel: Option<usize>) -> Vec<f32> {
    if channels == 0 {
        return Vec::new();
    }
    match channel {
        Some(channel) => interleaved
            .chunks(channels)
            .map(|frame| frame.get(channel).copied().unwrap_or(0.0))
            .collect(),
        None => mix_channels(interleaved, channels),
    }
}

/// Records a named input and one channel of it (or the default/mix).
///
/// # Errors
///
/// Returns a message when the named device is absent, the format is unsupported,
/// the stream cannot start, or the device reports an error.
pub fn record_seconds_from_channel(
    device_name: Option<&str>,
    channel: Option<usize>,
    seconds: f64,
) -> Result<RenderedAudio, String> {
    if !seconds.is_finite() {
        return Err(String::from("recording length must be finite"));
    }
    // A day at most keeps Duration and the sample buffer in range.
    let seconds = seconds.clamp(0.0, 86_400.0);
    let host = cpal::default_host();
    let device = match device_name {
        Some(name) => host
            .input_devices()
            .map_err(|error| format!("could not list inputs: {error}"))?
            .find(|device| device.name().ok().as_deref() == Some(name))
            .ok_or_else(|| format!("audio input {name:?} not found"))?,
        None => host
            .default_input_device()
            .ok_or_else(|| String::from("no default audio input device"))?,
    };
    let supported = device
        .default_input_config()
        .map_err(|error| format!("default input config: {error}"))?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = usize::from(config.channels);
    let sample_rate = config.sample_rate.0;
    if channels == 0 || sample_rate == 0 {
        return Err(String::from("input reports no channels or rate"));
    }
    // The callback only pushes into a preallocated lock-free queue; this
    // thread drains it while the recording runs.
    let queue = Arc::new(SampleQueue::new(
        (sample_rate as usize)
            .saturating_mul(channels)
            .saturating_mul(2),
    ));
    let error_slot: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, &queue, &error_slot)?,
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, &queue, &error_slot)?,
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, &queue, &error_slot)?,
        other => return Err(format!("unsupported input sample format {other:?}")),
    };
    stream
        .play()
        .map_err(|error| format!("could not start input: {error}"))?;
    let mut interleaved = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
        while let Some(sample) = queue.pop() {
            interleaved.push(sample);
        }
    }
    drop(stream);
    while let Some(sample) = queue.pop() {
        interleaved.push(sample);
    }
    if let Some(message) = error_slot.lock().ok().and_then(|slot| slot.clone()) {
        return Err(message);
    }
    interleaved.truncate(interleaved.len() / channels * channels);
    let mono = channel_samples(&interleaved, channels, channel);
    if mono.is_empty() {
        return Err(String::from("input captured no samples"));
    }
    let mut samples = Vec::with_capacity(mono.len() * 2);
    for value in mono {
        samples.push(value);
        samples.push(value);
    }
    Ok(RenderedAudio {
        samples,
        sample_rate,
    })
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: &Arc<SampleQueue>,
    error_slot: &Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let queue = Arc::clone(queue);
    let errors = Arc::clone(error_slot);
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                // Drop a whole buffer rather than split a frame.
                if queue.free() < data.len() {
                    return;
                }
                for sample in data {
                    let _ = queue.push(sample.to_sample::<f32>());
                }
            },
            move |error| {
                if let Ok(mut slot) = errors.lock() {
                    *slot = Some(format!("input stream error: {error}"));
                }
            },
            None,
        )
        .map_err(|error| format!("could not open input stream: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_channel_is_selected_or_all_are_mixed() {
        let interleaved = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(channel_samples(&interleaved, 2, Some(0)), vec![1.0, 3.0]);
        assert_eq!(channel_samples(&interleaved, 2, Some(1)), vec![2.0, 4.0]);
        assert_eq!(channel_samples(&interleaved, 2, None), vec![1.5, 3.5]);
        assert!(channel_samples(&interleaved, 0, Some(0)).is_empty());
    }

    #[test]
    fn interleaved_channels_average_to_mono() {
        assert_eq!(mix_channels(&[1.0, 3.0, -1.0, 1.0], 2), vec![2.0, 0.0]);
        assert!(mix_channels(&[], 2).is_empty());
        assert!(mix_channels(&[1.0, 2.0], 0).is_empty());
    }
}
