//! Drum slicing: find onsets in a loop, cut it into slices and map slices to
//! General MIDI drum pads.
//!
//! Onsets come from a short-time energy envelope compared with the recent
//! average, which finds clear attacks in a loop without a model.

use crate::RenderedAudio;

/// A slice of a buffer, in samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slice {
    /// First sample.
    pub start: usize,
    /// One past the last sample.
    pub end: usize,
}

impl Slice {
    /// Length in samples.
    #[must_use]
    pub fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// True when the slice holds no samples.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
}

/// General MIDI drum notes used as pads, from kick to crash.
pub const GM_PADS: [u8; 16] = [
    36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51,
];

/// One pad mapped to a slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pad {
    /// MIDI note that triggers it.
    pub note: u8,
    /// Audio slice.
    pub slice: Slice,
}

/// A kit: pads in note order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kit {
    /// Assigned pads.
    pub pads: Vec<Pad>,
}

/// Finds attack positions in samples.
#[must_use]
pub fn onsets(samples: &[f32], sample_rate: u32) -> Vec<usize> {
    const FRAME: usize = 256;
    const HOP: usize = 128;
    let minimum_gap = (f64::from(sample_rate) * 0.05) as usize;
    if samples.len() < FRAME || sample_rate == 0 {
        return Vec::new();
    }
    let mut energy = Vec::new();
    let mut position = 0;
    while position + FRAME <= samples.len() {
        let sum: f32 = samples[position..position + FRAME]
            .iter()
            .map(|sample| sample * sample)
            .sum();
        energy.push(sum / FRAME as f32);
        position += HOP;
    }
    let mut found = Vec::new();
    let mut last = 0_usize;
    for index in 8..energy.len() {
        let recent: f32 = energy[index - 8..index].iter().sum::<f32>() / 8.0;
        let level = energy[index];
        let rising = level > recent * 4.0 && level > 0.000_5;
        let far_enough = index * HOP >= last + minimum_gap;
        if rising && far_enough {
            found.push(index * HOP);
            last = index * HOP;
        }
    }
    found
}

/// Cuts a buffer at its onsets; the first slice starts at the buffer start.
#[must_use]
pub fn slices(samples: &[f32], sample_rate: u32) -> Vec<Slice> {
    let mut points = onsets(samples, sample_rate);
    if points.first().is_none_or(|first| *first > 0) {
        points.insert(0, 0);
    }
    points.dedup();
    let mut out = Vec::with_capacity(points.len());
    for (index, start) in points.iter().enumerate() {
        let end = points.get(index + 1).copied().unwrap_or(samples.len());
        if end > *start {
            out.push(Slice { start: *start, end });
        }
    }
    out
}

/// Maps slices to the first GM pads, in order.
#[must_use]
pub fn kit(slices: &[Slice]) -> Kit {
    Kit {
        pads: slices
            .iter()
            .zip(GM_PADS)
            .map(|(slice, note)| Pad {
                note,
                slice: *slice,
            })
            .collect(),
    }
}

/// The audio of one pad.
#[must_use]
pub fn pad_audio(samples: &[f32], pad: Pad) -> &[f32] {
    let end = pad.slice.end.min(samples.len());
    let start = pad.slice.start.min(end);
    &samples[start..end]
}

/// Renders one pad as a stereo buffer, for audition.
#[must_use]
pub fn audition(samples: &[f32], pad: Pad, sample_rate: u32) -> RenderedAudio {
    let mono = pad_audio(samples, pad);
    let mut out = Vec::with_capacity(mono.len() * 2);
    for sample in mono {
        out.push(*sample);
        out.push(*sample);
    }
    RenderedAudio {
        samples: out,
        sample_rate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clicks(rate: u32, seconds: f64, positions: &[f64]) -> Vec<f32> {
        let mut samples = vec![0.0_f32; (f64::from(rate) * seconds) as usize];
        for position in positions {
            let start = (position * f64::from(rate)) as usize;
            for index in 0..(rate / 200) as usize {
                if let Some(sample) = samples.get_mut(start + index) {
                    *sample = if index % 2 == 0 { 0.6 } else { -0.6 };
                }
            }
        }
        samples
    }

    #[test]
    fn clicks_become_onsets_and_slices() {
        let rate = 48_000;
        let samples = clicks(rate, 1.2, &[0.5, 1.0]);
        let found = onsets(&samples, rate);
        assert_eq!(found.len(), 2, "two attacks found: {found:?}");
        let at = |target: f64| (target * f64::from(rate)) as i64;
        assert!((found[0] as i64 - at(0.5)).abs() < 500);
        assert!((found[1] as i64 - at(1.0)).abs() < 500);
        let cuts = slices(&samples, rate);
        assert_eq!(cuts.len(), 3, "lead-in plus two hits");
        assert_eq!(cuts[0].start, 0);
        assert!(cuts.iter().all(|slice| slice.len() > 0));
    }

    #[test]
    fn pads_take_the_first_gm_notes() {
        let cuts = vec![Slice { start: 0, end: 10 }, Slice { start: 10, end: 20 }];
        let kit = kit(&cuts);
        assert_eq!(kit.pads.len(), 2);
        assert_eq!(kit.pads[0].note, 36);
        assert_eq!(kit.pads[1].note, 37);
        let samples = vec![0.1_f32; 30];
        assert_eq!(pad_audio(&samples, kit.pads[1]).len(), 10);
        let rendered = audition(&samples, kit.pads[1], 48_000);
        assert_eq!(rendered.samples.len(), 20);
    }

    #[test]
    fn silence_has_no_onsets_and_no_slices() {
        let samples = vec![0.0_f32; 48_000];
        assert!(onsets(&samples, 48_000).is_empty());
        let cuts = slices(&samples, 48_000);
        assert_eq!(cuts.len(), 1);
        assert_eq!((cuts[0].start, cuts[0].end), (0, samples.len()));
        assert!(onsets(&[], 48_000).is_empty());
    }
}
