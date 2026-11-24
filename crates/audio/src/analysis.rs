//! Monophonic audio-to-MIDI: pitch detection and note segmentation.
//!
//! This is the in-process half of the C++ `AudioToMidi` path. The model-based
//! halves (stem separation, polyphonic transcription, instrument tagging,
//! lyrics) stay outside the core in the C++ design and are recorded as missing.

use music_core::{Pitch, Tick};

/// One detected note, in seconds and MIDI pitch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectedNote {
    /// Start in seconds.
    pub start: f64,
    /// Duration in seconds.
    pub duration: f64,
    /// MIDI pitch number.
    pub pitch: u8,
    /// Peak amplitude of the segment, 0..=1.
    pub level: f32,
}

/// Estimates the fundamental of one frame, or `None` when it is too quiet or
/// has no clear period.
#[must_use]
pub fn detect_pitch(samples: &[f32], sample_rate: u32) -> Option<f64> {
    if samples.len() < 512 || sample_rate == 0 {
        return None;
    }
    let energy: f32 = samples.iter().map(|sample| sample * sample).sum();
    if energy / (samples.len() as f32) < 1.0e-6 {
        return None;
    }
    let rate = f64::from(sample_rate);
    let min_lag = (rate / 1_000.0).max(2.0) as usize;
    let max_lag = (rate / 70.0).min(samples.len() as f64 / 2.0) as usize;
    if max_lag <= min_lag {
        return None;
    }
    let mut correlations = Vec::with_capacity(max_lag + 1);
    correlations.push(0.0_f64);
    for lag in 1..=max_lag {
        if lag < min_lag || lag + 1 > samples.len() {
            correlations.push(0.0);
            continue;
        }
        let mut correlation = 0.0_f64;
        for index in 0..samples.len() - lag {
            correlation += f64::from(samples[index]) * f64::from(samples[index + lag]);
        }
        correlations.push(correlation / (samples.len() - lag) as f64);
    }
    let best = correlations.iter().copied().fold(f64::MIN, f64::max);
    if best <= 0.0 {
        return None;
    }
    // First local maximum close to the best one: avoids octave-down winners at
    // multiples of the period.
    let mut chosen = None;
    for lag in (min_lag + 1)..max_lag {
        let value = correlations[lag];
        if value >= correlations[lag - 1] && value >= correlations[lag + 1] && value >= best * 0.90
        {
            chosen = Some(lag);
            break;
        }
    }
    let lag = chosen?;
    let left = correlations[lag - 1];
    let centre = correlations[lag];
    let right = correlations[lag + 1];
    let denominator = left - 2.0 * centre + right;
    let shift = if denominator.abs() > f64::EPSILON {
        0.5 * (left - right) / denominator
    } else {
        0.0
    };
    let refined = lag as f64 + shift.clamp(-0.5, 0.5);
    let rms = (energy / (samples.len() as f32)).sqrt();
    if rms < 0.005 {
        return None;
    }
    let frequency = rate / refined;
    (70.0..=1_000.0).contains(&frequency).then_some(frequency)
}

/// Converts monophonic audio into notes, one per stable pitch run.
#[must_use]
pub fn to_midi(samples: &[f32], sample_rate: u32) -> Vec<DetectedNote> {
    const FRAME: usize = 2_048;
    const HOP: usize = 512;
    if sample_rate == 0 || samples.len() < FRAME {
        return Vec::new();
    }
    let mut notes = Vec::new();
    let mut current: Option<(u8, usize, usize, f32)> = None;
    let mut frame_start = 0;
    while frame_start + FRAME <= samples.len() {
        let frame = &samples[frame_start..frame_start + FRAME];
        let pitch = detect_pitch(frame, sample_rate).map(nearest_pitch);
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / FRAME as f32).sqrt();
        match (pitch, current.as_mut()) {
            (Some(pitch), Some((held, start, last, level))) if *held == pitch => {
                *last = frame_start;
                *level = level.max(rms);
            }
            (Some(pitch), _) => {
                if let Some((held, start, last, level)) = current.take() {
                    notes.push(note(held, start, last + HOP, sample_rate, level));
                }
                current = Some((pitch, frame_start, frame_start, rms));
            }
            (None, Some(_)) => {
                if let Some((held, start, last, level)) = current.take() {
                    notes.push(note(held, start, last + HOP, sample_rate, level));
                }
            }
            (None, None) => {}
        }
        frame_start += HOP;
    }
    if let Some((held, start, last, level)) = current {
        notes.push(note(held, start, last + HOP, sample_rate, level));
    }
    notes
}

fn note(pitch: u8, start: usize, end: usize, sample_rate: u32, level: f32) -> DetectedNote {
    let rate = f64::from(sample_rate);
    DetectedNote {
        start: start as f64 / rate,
        duration: (end.saturating_sub(start)) as f64 / rate,
        pitch,
        level: level.min(1.0),
    }
}

fn nearest_pitch(hz: f64) -> u8 {
    let midi = (69.0 + 12.0 * (hz / 440.0).log2()).round();
    midi.clamp(0.0, 127.0) as u8
}

/// A rough kick/snare/hi-hat transcription from onsets and band energy.
///
/// This is a time-domain heuristic, not a trained classifier: low-frequency
/// onsets become kick, high-frequency ones hi-hat, the rest snare.
#[must_use]
pub fn to_drums(samples: &[f32], sample_rate: u32) -> Vec<DetectedNote> {
    if sample_rate == 0 || samples.is_empty() {
        return Vec::new();
    }
    let rate = f64::from(sample_rate);
    let onsets = crate::drums::onsets(samples, sample_rate);
    let window = ((rate * 0.04) as usize).max(32);
    let alpha = 1.0 - (-std::f64::consts::TAU * 200.0 / rate).exp();
    let mut notes = Vec::new();
    for (index, onset) in onsets.iter().enumerate() {
        let start = *onset;
        let end = (start + window).min(samples.len());
        if end.saturating_sub(start) < 8 {
            continue;
        }
        let mut low = 0.0_f64;
        let mut low_energy = 0.0_f64;
        let mut high_energy = 0.0_f64;
        let mut previous = samples[start];
        for sample in &samples[start..end] {
            low += alpha * (f64::from(*sample) - low);
            low_energy += low.abs();
            high_energy += f64::from((*sample - previous).abs());
            previous = *sample;
        }
        let frame = end.saturating_sub(start) as f64;
        let low_energy = low_energy / frame;
        let high_energy = high_energy / frame;
        let energy: f32 = samples[start..end]
            .iter()
            .map(|sample| sample.abs())
            .sum::<f32>()
            / frame as f32;
        if energy < 0.005 {
            continue;
        }
        let pitch = if high_energy > low_energy * 2.5 {
            42
        } else if low_energy > high_energy * 1.5 {
            36
        } else {
            38
        };
        let next = onsets.get(index + 1).copied().unwrap_or(samples.len());
        let duration = (next.saturating_sub(start) as f64 / rate).clamp(0.04, 0.25);
        notes.push(DetectedNote {
            start: start as f64 / rate,
            duration,
            pitch,
            level: energy.min(1.0),
        });
    }
    notes
}

/// Converts a detected note into a project note at 120 bpm's tick scale.
#[must_use]
pub fn as_ticks(note: &DetectedNote, bpm: f64) -> (Tick, Tick) {
    let ticks_per_second = music_core::PPQ as f64 * bpm / 60.0;
    (
        (note.start * ticks_per_second).round() as Tick,
        (note.duration * ticks_per_second).round() as Tick,
    )
}

/// The pitch of a detected MIDI number.
#[must_use]
pub fn as_pitch(pitch: u8) -> Option<Pitch> {
    Pitch::new(pitch).ok()
}

/// Estimates tempo from the median gap between detected onsets.
///
/// Returns `None` when fewer than four onsets are found. The result is folded
/// into 60..=180 bpm.
#[must_use]
pub fn estimate_tempo(samples: &[f32], sample_rate: u32) -> Option<f64> {
    if sample_rate == 0 {
        return None;
    }
    let onsets = crate::drums::onsets(samples, sample_rate);
    if onsets.len() < 4 {
        return None;
    }
    let mut gaps: Vec<f64> = onsets
        .windows(2)
        .filter_map(|pair| {
            let gap = pair[1].saturating_sub(pair[0]) as f64 / f64::from(sample_rate);
            (0.15..=2.0).contains(&gap).then_some(gap)
        })
        .collect();
    if gaps.len() < 3 {
        return None;
    }
    gaps.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let median = gaps[gaps.len() / 2];
    if median <= 0.0 {
        return None;
    }
    let mut bpm = 60.0 / median;
    while bpm < 60.0 {
        bpm *= 2.0;
    }
    while bpm > 180.0 {
        bpm /= 2.0;
    }
    (60.0..=180.0).contains(&bpm).then_some(bpm)
}

/// A coarse, model-free guess at what a monophonic clip contains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstrumentGuess {
    /// Dense onsets with short pitched runs.
    Drums,
    /// Mostly low pitches.
    Bass,
    /// Sustained pitches.
    Pad,
    /// Everything else with clear pitch.
    Melodic,
}

impl InstrumentGuess {
    /// Stable lowercase label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Drums => "drums",
            Self::Bass => "bass",
            Self::Pad => "pad",
            Self::Melodic => "melodic",
        }
    }
}

/// A heuristic instrument guess with a 0..=1 confidence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InstrumentTag {
    /// Guessed family.
    pub instrument: InstrumentGuess,
    /// Confidence in the guess.
    pub confidence: f64,
}

/// Guesses an instrument family from register, onset density and note length.
///
/// This is a model-free heuristic, not source separation or a trained tagger.
#[must_use]
pub fn tag_instrument(samples: &[f32], sample_rate: u32) -> Option<InstrumentTag> {
    if sample_rate == 0 {
        return None;
    }
    let notes = to_midi(samples, sample_rate);
    if notes.is_empty() {
        return None;
    }
    let seconds = samples.len() as f64 / f64::from(sample_rate);
    if seconds <= 0.0 {
        return None;
    }
    let onsets = crate::drums::onsets(samples, sample_rate);
    let density = onsets.len() as f64 / seconds;
    let mut pitches: Vec<u8> = notes.iter().map(|note| note.pitch).collect();
    pitches.sort_unstable();
    let median_pitch = pitches[pitches.len() / 2];
    let mean_duration = notes.iter().map(|note| note.duration).sum::<f64>() / notes.len() as f64;
    if density >= 1.5 && mean_duration < 0.25 {
        Some(InstrumentTag {
            instrument: InstrumentGuess::Drums,
            confidence: (density / 8.0).clamp(0.54, 0.9),
        })
    } else if median_pitch < 48 {
        Some(InstrumentTag {
            instrument: InstrumentGuess::Bass,
            confidence: 0.8,
        })
    } else if mean_duration >= 1.5 {
        Some(InstrumentTag {
            instrument: InstrumentGuess::Pad,
            confidence: 0.7,
        })
    } else {
        Some(InstrumentTag {
            instrument: InstrumentGuess::Melodic,
            confidence: 0.6,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frequency: f64, seconds: f64, rate: u32) -> Vec<f32> {
        let count = (f64::from(rate) * seconds) as usize;
        (0..count)
            .map(|index| {
                (std::f64::consts::TAU * frequency * index as f64 / f64::from(rate)).sin() as f32
                    * 0.5
            })
            .collect()
    }

    #[test]
    fn a_sustained_low_tone_is_guessed_as_bass() {
        let samples = tone(80.0, 2.0, 48_000);
        let tag = tag_instrument(&samples, 48_000).expect("tag");
        assert_eq!(tag.instrument, InstrumentGuess::Bass);
    }

    #[test]
    fn clicks_are_guessed_as_drums() {
        let rate = 48_000_u32;
        let mut samples = vec![0.0_f32; rate as usize * 2];
        for click in 0..8 {
            let start = click as usize * rate as usize / 4;
            for (index, sample) in samples.iter_mut().skip(start).take(120).enumerate() {
                let envelope = 1.0 - index as f32 / 120.0;
                *sample = if index % 2 == 0 { envelope } else { -envelope };
            }
        }
        let tag = tag_instrument(&samples, rate).expect("tag");
        assert_eq!(tag.instrument, InstrumentGuess::Drums, "{tag:?}");
    }

    #[test]
    fn regular_clicks_estimate_a_tempo() {
        let rate = 48_000_u32;
        let mut samples = vec![0.0_f32; rate as usize * 4];
        for click in 0..8 {
            let start = click as usize * rate as usize / 2;
            for (index, sample) in samples.iter_mut().skip(start).take(200).enumerate() {
                let envelope = 1.0 - index as f32 / 200.0;
                *sample = if index % 2 == 0 { envelope } else { -envelope };
            }
        }
        let tempo = estimate_tempo(&samples, rate).expect("tempo");
        assert!((tempo - 120.0).abs() < 2.0, "got {tempo}");
    }

    #[test]
    fn a_440_hz_tone_is_found() {
        let samples = tone(440.0, 0.25, 48_000);
        let pitch = detect_pitch(&samples, 48_000).expect("pitch");
        assert!((pitch - 440.0).abs() < 4.0, "got {pitch}");
    }

    #[test]
    fn silence_has_no_pitch() {
        assert_eq!(detect_pitch(&vec![0.0_f32; 4_096], 48_000), None);
    }

    #[test]
    fn drum_transcription_classifies_low_and_high_hits() {
        let rate = 48_000_u32;
        let mut samples = vec![0.0_f32; rate as usize];
        let burst = 0.06 * f64::from(rate);
        let start = (0.1 * f64::from(rate)) as usize;
        for index in 0..burst as usize {
            let time = index as f64 / f64::from(rate);
            samples[start + index] += (std::f64::consts::TAU * 60.0 * time).sin() as f32 * 0.8;
        }
        let start = (0.5 * f64::from(rate)) as usize;
        for index in 0..burst as usize {
            samples[start + index] += if index % 2 == 0 { 0.4 } else { -0.4 };
        }
        let notes = to_drums(&samples, rate);
        assert!(!notes.is_empty());
        assert!(notes.iter().any(|note| note.pitch == 36));
        assert!(notes.iter().any(|note| note.pitch == 42));
    }

    #[test]
    fn audio_to_midi_finds_an_a4_note() {
        let samples = tone(440.0, 0.5, 48_000);
        let notes = to_midi(&samples, 48_000);
        assert!(!notes.is_empty(), "expected a note, got {notes:?}");
        assert_eq!(nearest_pitch(440.0), 69);
        assert!((notes[0].start - 0.0).abs() < 0.1);
        assert!(notes[0].duration > 0.2);
        assert_eq!(nearest_pitch(440.0), 69);
    }

    #[test]
    fn ticks_follow_the_tempo() {
        let note = DetectedNote {
            start: 0.5,
            duration: 0.5,
            pitch: 69,
            level: 0.5,
        };
        let (start, length) = as_ticks(&note, 120.0);
        assert_eq!(start, 960);
        assert_eq!(length, 960);
        assert_eq!(as_pitch(200), None);
    }
}
