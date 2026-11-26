//! Real-time input monitoring through the default audio devices.
//!
//! The input stream mixes every frame to mono and pushes it into a bounded
//! queue; the output stream pops one sample per frame and writes it to every
//! output channel. Input and output must share a sample rate. Feedback is the
//! caller's risk; the setting that enables this is off by default.

use std::sync::Arc;

use crate::spsc::{SampleQueue, sanitize};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// A running input-to-output monitor. Dropping it stops both streams.
pub struct Monitor {
    _input: cpal::Stream,
    _output: cpal::Stream,
}

impl std::fmt::Debug for Monitor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Monitor")
    }
}

/// Mixes one interleaved frame to mono.
#[must_use]
pub fn mono_frame(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        0.0
    } else {
        frame.iter().sum::<f32>() / frame.len() as f32
    }
}

/// Starts monitoring the default input on the default output.
///
/// # Errors
///
/// Returns a message when a device is missing, the formats differ, the rates
/// differ, samples are unsupported, or a stream cannot start.
pub fn start(gain: f32) -> Result<Monitor, String> {
    start_on(None, gain)
}

/// Starts monitoring a named input (or the default) on the default output.
///
/// # Errors
///
/// Returns a message when the named device is absent, a device is missing, the
/// formats or rates differ, samples are unsupported, or a stream cannot start.
pub fn start_on(device_name: Option<&str>, gain: f32) -> Result<Monitor, String> {
    start_on_channel(device_name, None, gain)
}

/// Starts monitoring one input channel, or a mono mix when `channel` is None.
///
/// # Errors
///
/// Returns a message when the devices, formats or sample rates do not match.
pub fn start_on_channel(
    device_name: Option<&str>,
    channel: Option<usize>,
    gain: f32,
) -> Result<Monitor, String> {
    let host = cpal::default_host();
    let input = match device_name {
        Some(name) => host
            .input_devices()
            .map_err(|error| format!("could not list inputs: {error}"))?
            .find(|device| device.name().ok().as_deref() == Some(name))
            .ok_or_else(|| format!("audio input {name:?} not found"))?,
        None => host
            .default_input_device()
            .ok_or_else(|| String::from("no default audio input device"))?,
    };
    let output = host
        .default_output_device()
        .ok_or_else(|| String::from("no default audio output device"))?;
    let input_supported = input
        .default_input_config()
        .map_err(|error| format!("default input config: {error}"))?;
    let output_supported = output
        .default_output_config()
        .map_err(|error| format!("default output config: {error}"))?;
    let input_format = input_supported.sample_format();
    let output_format = output_supported.sample_format();
    let input_config: cpal::StreamConfig = input_supported.into();
    let output_config: cpal::StreamConfig = output_supported.into();
    if input_config.sample_rate != output_config.sample_rate {
        return Err(format!(
            "input runs at {} Hz but output at {} Hz",
            input_config.sample_rate.0, output_config.sample_rate.0
        ));
    }
    // Two seconds of headroom, allocated before any callback runs.
    let capacity = (input_config.sample_rate.0 as usize)
        .saturating_mul(2)
        .max(1);
    let queue = Arc::new(SampleQueue::new(capacity));
    let input_stream = build_input(
        &input,
        &input_config,
        input_format,
        channel,
        Arc::clone(&queue),
    )?;
    let output_stream = build_output(&output, &output_config, output_format, queue, gain)?;
    input_stream
        .play()
        .map_err(|error| format!("could not start input: {error}"))?;
    output_stream
        .play()
        .map_err(|error| format!("could not start output: {error}"))?;
    Ok(Monitor {
        _input: input_stream,
        _output: output_stream,
    })
}

fn build_input(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    channel: Option<usize>,
    queue: Arc<SampleQueue>,
) -> Result<cpal::Stream, String> {
    match format {
        cpal::SampleFormat::F32 => input_stream::<f32>(device, config, channel, queue),
        cpal::SampleFormat::I16 => input_stream::<i16>(device, config, channel, queue),
        cpal::SampleFormat::U16 => input_stream::<u16>(device, config, channel, queue),
        other => Err(format!("unsupported input sample format {other:?}")),
    }
}

fn build_output(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    format: cpal::SampleFormat,
    queue: Arc<SampleQueue>,
    gain: f32,
) -> Result<cpal::Stream, String> {
    match format {
        cpal::SampleFormat::F32 => output_stream::<f32>(device, config, queue, gain),
        cpal::SampleFormat::I16 => output_stream::<i16>(device, config, queue, gain),
        cpal::SampleFormat::U16 => output_stream::<u16>(device, config, queue, gain),
        other => Err(format!("unsupported output sample format {other:?}")),
    }
}

fn input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channel: Option<usize>,
    queue: Arc<SampleQueue>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = usize::from(config.channels).max(1);
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                // Lock-free and allocation-free: a full queue drops samples.
                for frame in data.chunks_exact(channels) {
                    let mono = if let Some(channel) = channel {
                        frame
                            .get(channel)
                            .map_or(0.0, |sample| sample.to_sample::<f32>())
                    } else {
                        frame
                            .iter()
                            .map(|sample| sample.to_sample::<f32>())
                            .sum::<f32>()
                            / frame.len() as f32
                    };
                    let _ = queue.push(mono);
                }
            },
            move |_| {},
            None,
        )
        .map_err(|error| format!("could not open input stream: {error}"))
}

fn output_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    queue: Arc<SampleQueue>,
    gain: f32,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let channels = usize::from(config.channels).max(1);
    // Keep at most ~50 ms waiting beyond this callback's need. A late start or
    // clock drift would otherwise fill the queue and lock the delay at two
    // seconds; dropping the oldest samples brings it back down.
    let backlog = (config.sample_rate.0 as usize / 20).max(1);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                let needed = data.len() / channels;
                let limit = needed.saturating_add(backlog);
                let excess = queue.len().saturating_sub(limit);
                for _ in 0..excess {
                    let _ = queue.pop();
                }
                for frame in data.chunks_exact_mut(channels) {
                    let sample = sanitize(queue.pop().unwrap_or(0.0) * gain);
                    let value = T::from_sample(sample);
                    for slot in frame {
                        *slot = value;
                    }
                }
            },
            move |_| {},
            None,
        )
        .map_err(|error| format!("could not open output stream: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_mix_to_mono() {
        assert_eq!(mono_frame(&[1.0, 3.0]), 2.0);
        assert_eq!(mono_frame(&[-1.0]), -1.0);
        assert_eq!(mono_frame(&[]), 0.0);
    }
}
