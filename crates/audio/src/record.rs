//! Recording from the default audio input.
//!
//! Opens the default device through cpal, captures f32/i16/u16 samples until
//! the requested duration ends, mixes channels to mono and returns a stereo
//! buffer. Microphone permission and device availability are outside this
//! module's control.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::RenderedAudio;

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

/// Records the default input for `seconds`, returning a stereo buffer.
///
/// # Errors
///
/// Returns a message when there is no input device, the format is unsupported,
/// the stream cannot start, or the device reports an error.
pub fn record_seconds(seconds: f64) -> Result<RenderedAudio, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| String::from("no default audio input device"))?;
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
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let error_slot: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    let stream = match format {
        cpal::SampleFormat::F32 => build::<f32>(&device, &config, &buffer, &error_slot)?,
        cpal::SampleFormat::I16 => build::<i16>(&device, &config, &buffer, &error_slot)?,
        cpal::SampleFormat::U16 => build::<u16>(&device, &config, &buffer, &error_slot)?,
        other => return Err(format!("unsupported input sample format {other:?}")),
    };
    stream
        .play()
        .map_err(|error| format!("could not start input: {error}"))?;
    std::thread::sleep(std::time::Duration::from_secs_f64(seconds.max(0.0)));
    drop(stream);
    if let Some(message) = error_slot.lock().ok().and_then(|slot| slot.clone()) {
        return Err(message);
    }
    let interleaved = buffer
        .lock()
        .map_err(|_| String::from("input buffer lock poisoned"))?
        .clone();
    let mono = mix_channels(&interleaved, channels);
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
    buffer: &Arc<Mutex<Vec<f32>>>,
    error_slot: &Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let buffer = Arc::clone(buffer);
    let errors = Arc::clone(error_slot);
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if let Ok(mut samples) = buffer.lock() {
                    samples.extend(data.iter().map(|sample| sample.to_sample::<f32>()));
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
    fn interleaved_channels_average_to_mono() {
        assert_eq!(mix_channels(&[1.0, 3.0, -1.0, 1.0], 2), vec![2.0, 0.0]);
        assert!(mix_channels(&[], 2).is_empty());
        assert!(mix_channels(&[1.0, 2.0], 0).is_empty());
    }
}
