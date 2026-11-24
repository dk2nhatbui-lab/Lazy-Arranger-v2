//! Sample-level edits for WAV files.

use crate::RenderedAudio;

/// Multiplies every sample by a decibel gain and returns the factor used.
pub fn gain(audio: &mut RenderedAudio, decibels: f32) -> f32 {
    let factor = 10.0_f32.powf(decibels / 20.0);
    for sample in &mut audio.samples {
        *sample *= factor;
    }
    factor
}

/// Reverses frames in place, keeping each stereo pair together.
pub fn reverse(audio: &mut RenderedAudio) {
    let frames = audio.samples.len() / 2;
    for frame in 0..frames / 2 {
        let other = frames - 1 - frame;
        for channel in 0..2 {
            audio.samples.swap(frame * 2 + channel, other * 2 + channel);
        }
    }
}

/// Scales the buffer so its peak equals `target` (clamped to 0..=1).
///
/// Returns the factor used; silence returns 1.0 and stays silent.
pub fn normalize_peak(audio: &mut RenderedAudio, target: f32) -> f32 {
    let target = target.clamp(0.0, 1.0);
    let peak = audio
        .samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0_f32, f32::max);
    if peak <= f32::EPSILON {
        return 1.0;
    }
    let factor = target / peak;
    for sample in &mut audio.samples {
        *sample *= factor;
    }
    factor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(samples: Vec<f32>) -> RenderedAudio {
        RenderedAudio {
            samples,
            sample_rate: 48_000,
        }
    }

    #[test]
    fn gain_scales_by_decibels() {
        let mut audio = buffer(vec![0.5, -0.5]);
        let factor = gain(&mut audio, 6.0);
        assert!((factor - 1.995_262).abs() < 0.001);
        assert!((audio.samples[0] - 0.997_631).abs() < 0.001);
    }

    #[test]
    fn reverse_keeps_stereo_pairs_together() {
        let mut audio = buffer(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
        reverse(&mut audio);
        assert_eq!(audio.samples, vec![0.5, 0.6, 0.3, 0.4, 0.1, 0.2]);
    }

    #[test]
    fn normalize_moves_the_peak_to_target() {
        let mut audio = buffer(vec![0.25, -0.5, 0.1]);
        let factor = normalize_peak(&mut audio, 0.9);
        assert!((factor - 1.8).abs() < 0.0001);
        assert!((audio.samples[1] + 0.9).abs() < 0.0001);
        let mut silence = buffer(vec![0.0, 0.0]);
        assert_eq!(normalize_peak(&mut silence, 0.9), 1.0);
    }
}
