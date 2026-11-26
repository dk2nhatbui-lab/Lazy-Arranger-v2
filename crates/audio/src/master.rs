//! Master bus: gain, a soft limiter, and a simple meter.

use crate::RenderedAudio;

/// Master settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Master {
    /// Gain in decibels.
    pub gain_db: f32,
    /// Whether the limiter is on.
    pub limiter: bool,
    /// Master reverb wet amount, 0..=1.
    pub reverb: f32,
    /// Master chorus wet amount, 0..=1.
    pub chorus: f32,
}

impl Default for Master {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            limiter: true,
            reverb: 0.0,
            chorus: 0.0,
        }
    }
}

/// Peak and RMS of a buffer, in linear 0..1+ units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Meter {
    /// Largest absolute sample.
    pub peak: f32,
    /// Root mean square over all channels.
    pub rms: f32,
}

/// Applies gain and, when enabled, a soft limiter that keeps samples within
/// -1..=1.
pub fn apply(rendered: &mut RenderedAudio, master: Master) {
    let gain = 10.0_f32.powf(master.gain_db / 20.0);
    for sample in &mut rendered.samples {
        *sample *= gain;
    }
    if master.chorus > 0.0 {
        chorus(rendered, master.chorus.clamp(0.0, 1.0));
    }
    if master.reverb > 0.0 {
        reverb(rendered, master.reverb.clamp(0.0, 1.0));
    }
    for sample in &mut rendered.samples {
        let value = if master.limiter {
            sample.tanh()
        } else {
            *sample
        };
        *sample = crate::spsc::sanitize(value);
    }
}

fn chorus(rendered: &mut RenderedAudio, amount: f32) {
    let rate = rendered.sample_rate.max(1) as f32;
    let max_delay = (rate * 0.05) as usize + 2;
    if max_delay < 4 {
        return;
    }
    let mut lines = [vec![0.0_f32; max_delay], vec![0.0_f32; max_delay]];
    let mut index = 0;
    for (frame_index, frame) in rendered
        .samples
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .enumerate()
    {
        let time = frame_index as f32 / rate;
        for (channel, line) in lines.iter_mut().enumerate() {
            let phase = time * std::f32::consts::TAU * 0.5 + channel as f32 * 0.8;
            let delay = rate * 0.018 + phase.sin() * rate * 0.004;
            let read = (index as f32 - delay).rem_euclid(max_delay as f32);
            let first = (read.floor() as usize).min(max_delay - 1);
            let second = (first + 1) % max_delay;
            let mix = read - read.floor();
            let delayed = line[first] * (1.0 - mix) + line[second] * mix;
            line[index] = frame[channel];
            frame[channel] += delayed * amount * 0.5;
        }
        index = (index + 1) % max_delay;
    }
}

fn reverb(rendered: &mut RenderedAudio, amount: f32) {
    let rate = rendered.sample_rate.max(1) as f32;
    let comb_seconds = [0.0297_f32, 0.0371, 0.0411, 0.0437];
    let allpass_seconds = [0.0050_f32, 0.0017];
    let mut channels: [ReverbChannel; 2] =
        std::array::from_fn(|_| ReverbChannel::new(rate, &comb_seconds, &allpass_seconds));
    for frame in rendered.samples.as_chunks_mut::<2>().0.iter_mut() {
        for (channel, sample) in frame.iter_mut().enumerate() {
            let dry = *sample;
            let wet = channels[channel].process(dry);
            *sample = dry + wet * amount * 0.6;
        }
    }
}

struct CombLine {
    buffer: Vec<f32>,
    index: usize,
    damped: f32,
}

struct AllpassLine {
    buffer: Vec<f32>,
    index: usize,
}

struct ReverbChannel {
    combs: [CombLine; 4],
    allpasses: [AllpassLine; 2],
}

impl ReverbChannel {
    fn new(rate: f32, comb_seconds: &[f32; 4], allpass_seconds: &[f32; 2]) -> Self {
        Self {
            combs: std::array::from_fn(|index| CombLine {
                buffer: vec![0.0; (rate * comb_seconds[index]) as usize + 1],
                index: 0,
                damped: 0.0,
            }),
            allpasses: std::array::from_fn(|index| AllpassLine {
                buffer: vec![0.0; (rate * allpass_seconds[index]) as usize + 1],
                index: 0,
            }),
        }
    }

    fn process(&mut self, dry: f32) -> f32 {
        let mut wet = 0.0;
        for comb in &mut self.combs {
            let output = comb.buffer[comb.index];
            comb.damped = output * 0.7 + comb.damped * 0.3;
            comb.buffer[comb.index] = dry + comb.damped * 0.78;
            comb.index = (comb.index + 1) % comb.buffer.len();
            wet += output;
        }
        wet /= 4.0;
        for allpass in &mut self.allpasses {
            let buffered = allpass.buffer[allpass.index];
            let output = -wet + buffered;
            allpass.buffer[allpass.index] = wet + buffered * 0.5;
            allpass.index = (allpass.index + 1) % allpass.buffer.len();
            wet = output;
        }
        wet
    }
}

/// Measures a stereo window at `start` sample, for live meters.
#[must_use]
pub fn meter_at(samples: &[f32], start: usize, frames: usize) -> Meter {
    let start = start.min(samples.len());
    let length = frames
        .saturating_mul(2)
        .min(samples.len().saturating_sub(start));
    if length == 0 {
        return Meter::default();
    }
    let window = &samples[start..start + length];
    let peak = window
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let rms = (window
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum::<f64>()
        / length as f64)
        .sqrt() as f32;
    Meter { peak, rms }
}

/// Measures a buffer.
#[must_use]
pub fn meter(rendered: &RenderedAudio) -> Meter {
    if rendered.samples.is_empty() {
        return Meter::default();
    }
    let peak = rendered
        .samples
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let sum: f64 = rendered
        .samples
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum();
    let rms = (sum / rendered.samples.len() as f64).sqrt() as f32;
    Meter { peak, rms }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(samples: Vec<f32>) -> RenderedAudio {
        RenderedAudio {
            samples,
            sample_rate: 48_000,
        }
    }

    #[test]
    fn gain_scales_and_the_limiter_caps() {
        let mut buffer = audio(vec![0.5, -0.5]);
        apply(
            &mut buffer,
            Master {
                gain_db: -6.0,
                limiter: false,
                ..Master::default()
            },
        );
        assert!((buffer.samples[0] - 0.25).abs() < 0.01);
        let mut hot = audio(vec![3.0, -3.0]);
        apply(&mut hot, Master::default());
        assert!(hot.samples.iter().all(|sample| sample.abs() <= 1.0));
        let mut raw = audio(vec![3.0, -3.0]);
        apply(
            &mut raw,
            Master {
                gain_db: 0.0,
                limiter: false,
                ..Master::default()
            },
        );
        assert_eq!(raw.samples, vec![1.0, -1.0]);
    }

    #[test]
    fn chorus_does_not_index_past_the_delay_line() {
        let mut buffer = audio(vec![0.0; 48_000 * 2]);
        apply(
            &mut buffer,
            Master {
                reverb: 0.0,
                chorus: 1.0,
                limiter: false,
                ..Master::default()
            },
        );
        assert!(buffer.samples.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn reverb_keeps_the_channels_separate() {
        let mut impulse = audio(vec![0.0; 8192]);
        impulse.samples[0] = 1.0;
        apply(
            &mut impulse,
            Master {
                reverb: 0.8,
                chorus: 0.0,
                limiter: false,
                ..Master::default()
            },
        );
        let right_tail: f32 = impulse.samples[1..]
            .iter()
            .step_by(2)
            .map(|sample| sample.abs())
            .sum();
        let left_tail: f32 = impulse.samples[0..]
            .iter()
            .step_by(2)
            .map(|sample| sample.abs())
            .sum();
        assert!(left_tail > 0.01, "left tail {left_tail}");
        assert!(right_tail < 1.0e-6, "right tail {right_tail}");
    }

    #[test]
    fn reverb_and_chorus_add_tail_energy() {
        let mut impulse = audio(vec![0.0; 4096]);
        impulse.samples[0] = 1.0;
        let mut wet = impulse.clone();
        apply(
            &mut wet,
            Master {
                reverb: 0.8,
                chorus: 0.0,
                limiter: false,
                ..Master::default()
            },
        );
        let tail: f32 = wet.samples[1000..].iter().map(|sample| sample.abs()).sum();
        assert!(tail > 0.001, "reverb tail {tail}");
        let mut dry = impulse.clone();
        apply(
            &mut dry,
            Master {
                limiter: false,
                ..Master::default()
            },
        );
        assert_eq!(dry.samples, impulse.samples);
        let mut modulated = impulse;
        apply(
            &mut modulated,
            Master {
                reverb: 0.0,
                chorus: 0.8,
                limiter: false,
                ..Master::default()
            },
        );
        assert!(
            modulated
                .samples
                .iter()
                .zip(dry.samples.iter())
                .any(|(left, right)| (left - right).abs() > 1.0e-6)
        );
    }

    #[test]
    fn a_window_meter_reports_only_its_slice() {
        let samples = [0.5, -0.5, 1.0, -1.0, 0.0, 0.0];
        let meter = meter_at(&samples, 2, 1);
        assert!((meter.peak - 1.0).abs() < 1.0e-6);
        assert_eq!(meter_at(&samples, 6, 1), Meter::default());
    }

    #[test]
    fn meters_report_peak_and_rms() {
        let buffer = audio(vec![0.5, -0.5, 0.0, 0.0]);
        let levels = meter(&buffer);
        assert!((levels.peak - 0.5).abs() < 1.0e-6);
        assert!((levels.rms - 0.353_553_4).abs() < 1.0e-4);
        assert_eq!(meter(&audio(Vec::new())), Meter::default());
    }
}
