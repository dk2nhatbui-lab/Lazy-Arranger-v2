//! Chord recognition from audio.
//!
//! Each segment's chroma is estimated with a Goertzel filter per pitch class
//! over several octaves, weak classes are dropped, and the survivors go to the
//! existing chord recognizer. This finds triads and sevenths in clear material;
//! dense mixes and inversions beyond the bass rule need the model-based
//! pipeline the C++ app uses and this crate does not ship.

use music_core::{Chord, PPQ, PitchClass, Tick};
use std::f64::consts::TAU;

/// A chord found in one segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioChord {
    /// Segment start in seconds.
    pub start: f64,
    /// Segment length in seconds.
    pub duration: f64,
    /// The chord.
    pub chord: Chord,
    /// Share of the combined pitch-class set the chord explains.
    pub score: f64,
}

/// Chroma energy per pitch class over `samples`.
#[must_use]
pub fn chroma(samples: &[f32], sample_rate: u32) -> [f64; 12] {
    let mut bins = [0.0_f64; 12];
    if samples.is_empty() || sample_rate == 0 {
        return bins;
    }
    let rate = f64::from(sample_rate);
    let window = samples.len() as f64;
    for (index, bin) in bins.iter_mut().enumerate() {
        for octave in 2..=6_u8 {
            let midi = f64::from(index as u8 + 12 * (octave + 1));
            let frequency = 440.0 * 2.0_f64.powf((midi - 69.0) / 12.0);
            if frequency >= rate / 2.0 {
                continue;
            }
            let omega = TAU * frequency / rate;
            let coefficient = 2.0 * omega.cos();
            let mut s_prev = 0.0_f64;
            let mut s_prev2 = 0.0_f64;
            for (position, sample) in samples.iter().enumerate() {
                let hann = 0.5 - 0.5 * (TAU * position as f64 / window).cos();
                let value = f64::from(*sample) * hann;
                let s = value + coefficient * s_prev - s_prev2;
                s_prev2 = s_prev;
                s_prev = s;
            }
            let power = s_prev * s_prev + s_prev2 * s_prev2 - coefficient * s_prev * s_prev2;
            *bin += power.max(0.0);
        }
    }
    let peak = bins.iter().copied().fold(0.0_f64, f64::max);
    if peak > 0.0 {
        for bin in &mut bins {
            *bin /= peak;
        }
    }
    bins
}

/// Pitch classes loud enough to belong to a chord.
#[must_use]
pub fn chord_pitch_classes(bins: &[f64; 12], threshold: f64) -> Vec<u8> {
    let peak = bins.iter().copied().fold(0.0_f64, f64::max);
    if peak < 1.0e-9 {
        return Vec::new();
    }
    (0..12)
        .filter(|index| bins[*index] >= threshold * peak)
        .map(|index| index as u8)
        .collect()
}

/// Recognizes a chord per fixed-length segment.
#[must_use]
pub fn recognize(samples: &[f32], sample_rate: u32, segment_seconds: f64) -> Vec<AudioChord> {
    if sample_rate == 0 || segment_seconds <= 0.0 {
        return Vec::new();
    }
    let segment = (segment_seconds * f64::from(sample_rate)) as usize;
    if segment < 256 {
        return Vec::new();
    }
    let mut chords = Vec::new();
    let mut start = 0;
    while start + segment <= samples.len() {
        let bins = chroma(&samples[start..start + segment], sample_rate);
        let classes = chord_pitch_classes(&bins, 0.45);
        if classes.len() >= 2 {
            let pitches: Vec<u8> = classes.iter().map(|class| class + 48).collect();
            if let Some(found) = arranger::recognize(&pitches, None) {
                chords.push(AudioChord {
                    start: start as f64 / f64::from(sample_rate),
                    duration: segment as f64 / f64::from(sample_rate),
                    chord: found.chord,
                    score: found.score,
                });
            }
        }
        start += segment;
    }
    chords
}

/// Converts audio seconds to ticks at a tempo.
#[must_use]
pub fn to_ticks(seconds: f64, bpm: f64) -> Tick {
    (seconds * bpm / 60.0 * PPQ as f64).round() as Tick
}

/// A major or minor triad's pitch classes, for tests and callers.
#[must_use]
pub fn triad(root: PitchClass, minor: bool) -> [PitchClass; 3] {
    [
        root,
        root.transpose(if minor { 3 } else { 4 }),
        root.transpose(7),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::Spelling;

    fn tone(frequency: f64, seconds: f64, rate: u32) -> Vec<f32> {
        let count = (f64::from(rate) * seconds) as usize;
        (0..count)
            .map(|index| (TAU * frequency * index as f64 / f64::from(rate)).sin() as f32 * 0.3)
            .collect()
    }

    fn mixed(notes: &[f64], seconds: f64, rate: u32) -> Vec<f32> {
        let count = (f64::from(rate) * seconds) as usize;
        (0..count)
            .map(|index| {
                notes
                    .iter()
                    .map(|frequency| (TAU * frequency * index as f64 / f64::from(rate)).sin() * 0.2)
                    .sum::<f64>() as f32
            })
            .collect()
    }

    fn frequency(midi: u8) -> f64 {
        440.0 * 2.0_f64.powf((f64::from(midi) - 69.0) / 12.0)
    }

    #[test]
    fn chroma_peaks_on_a_c_major_triad() {
        let samples = mixed(&[frequency(60), frequency(64), frequency(67)], 0.5, 48_000);
        let bins = chroma(&samples, 48_000);
        let classes = chord_pitch_classes(&bins, 0.45);
        assert!(classes.contains(&0) && classes.contains(&4) && classes.contains(&7));
        assert!(!classes.contains(&1));
    }

    #[test]
    fn a_triad_is_recognized_from_audio() {
        let samples = mixed(&[frequency(60), frequency(64), frequency(67)], 1.0, 48_000);
        let chords = recognize(&samples, 48_000, 0.5);
        assert_eq!(chords.len(), 2);
        assert_eq!(chords[0].chord.format(Spelling::Sharp), "C");
        assert!((chords[0].start - 0.0).abs() < 1.0e-9);
        assert!((chords[1].start - 0.5).abs() < 1.0e-9);
    }

    #[test]
    fn an_a_minor_triad_is_recognized() {
        let samples = mixed(&[frequency(57), frequency(60), frequency(64)], 0.5, 48_000);
        let chords = recognize(&samples, 48_000, 0.5);
        assert_eq!(chords.len(), 1);
        assert_eq!(chords[0].chord.format(Spelling::Sharp), "Am");
    }

    #[test]
    fn silence_and_short_buffers_yield_nothing() {
        assert!(recognize(&vec![0.0; 48_000], 48_000, 0.5).is_empty());
        assert!(recognize(&tone(440.0, 0.001, 48_000), 48_000, 0.5).is_empty());
        assert!(recognize(&[], 48_000, 0.5).is_empty());
        assert!(recognize(&[0.0; 1000], 0, 0.5).is_empty());
    }

    #[test]
    fn ticks_and_triads_are_available() {
        assert_eq!(to_ticks(1.0, 120.0), 1920);
        let triad = triad(PitchClass::new(0), false);
        assert_eq!(triad[1].as_u8(), 4);
        assert_eq!(triad[2].as_u8(), 7);
    }
}
