//! Master bus: gain, a soft limiter, and a simple meter.

use crate::RenderedAudio;

/// Master settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Master {
    /// Gain in decibels.
    pub gain_db: f32,
    /// Whether the limiter is on.
    pub limiter: bool,
}

impl Default for Master {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            limiter: true,
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
        let mut value = *sample * gain;
        if master.limiter {
            value = value.tanh();
        }
        *sample = value.clamp(-1.0, 1.0);
    }
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
            },
        );
        assert_eq!(raw.samples, vec![1.0, -1.0]);
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
