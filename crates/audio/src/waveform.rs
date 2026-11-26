//! Waveform peaks for drawing audio references.

/// Minimum and maximum sample in one column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Peak {
    /// Lowest sample.
    pub min: f32,
    /// Highest sample.
    pub max: f32,
}

/// Start and end indices for a zoomed window over `len` columns.
///
/// `position` slides the window from 0 (start) to 1 (end); `zoom` is 1 for the
/// whole waveform and larger to show less.
#[must_use]
pub fn visible(len: usize, zoom: f32, position: f32) -> (usize, usize) {
    if len == 0 {
        return (0, 0);
    }
    let count = ((len as f32 / zoom.max(1.0)).ceil() as usize).clamp(1, len);
    let max_start = len - count;
    let start = (max_start as f32 * position.clamp(0.0, 1.0)).round() as usize;
    (start, start + count)
}

/// Reduces samples to `bins` min/max columns; the last bin takes any remainder.
#[must_use]
pub fn peaks(samples: &[f32], bins: usize) -> Vec<Peak> {
    if samples.is_empty() || bins == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(bins);
    let per_bin = samples.len().div_ceil(bins.max(1));
    for index in 0..bins {
        let start = index * per_bin;
        if start >= samples.len() {
            break;
        }
        let end = (start + per_bin).min(samples.len());
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for sample in &samples[start..end] {
            min = min.min(*sample);
            max = max.max(*sample);
        }
        out.push(Peak { min, max });
    }
    out
}

/// Overview columns per second of audio: fine enough to show single drum
/// hits when the timeline is zoomed in.
pub const COLUMNS_PER_SECOND: f64 = 200.0;

/// Fewest and most overview columns for one file.
const COLUMN_RANGE: (usize, usize) = (256, 1 << 16);

/// Min/max columns of an audio file, per channel and mixed to mono.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overview {
    /// Left and right columns; one entry when both channels are the same.
    pub channels: Vec<Vec<Peak>>,
    /// Both channels mixed, for meters and the clip editor.
    pub mono: Vec<Peak>,
}

impl Overview {
    /// Builds an overview from interleaved stereo at `sample_rate`.
    #[must_use]
    pub fn from_stereo(samples: &[f32], sample_rate: u32) -> Self {
        let frames = samples.len() / 2;
        if frames == 0 {
            return Self::default();
        }
        let seconds = frames as f64 / f64::from(sample_rate.max(1));
        let bins = ((seconds * COLUMNS_PER_SECOND).ceil() as usize)
            .clamp(COLUMN_RANGE.0, COLUMN_RANGE.1)
            .min(frames);
        let per_bin = frames.div_ceil(bins);
        let empty = Peak {
            min: f32::MAX,
            max: f32::MIN,
        };
        let mut left = Vec::with_capacity(bins);
        let mut right = Vec::with_capacity(bins);
        let mut mono = Vec::with_capacity(bins);
        let mut same = true;
        for chunk in samples[..frames * 2].chunks(per_bin * 2) {
            let (mut l, mut r, mut m) = (empty, empty, empty);
            for frame in chunk.chunks_exact(2) {
                let (a, b) = (frame[0], frame[1]);
                same &= (a - b).abs() < 1e-4;
                let mid = (a + b) * 0.5;
                l = Peak {
                    min: l.min.min(a),
                    max: l.max.max(a),
                };
                r = Peak {
                    min: r.min.min(b),
                    max: r.max.max(b),
                };
                m = Peak {
                    min: m.min.min(mid),
                    max: m.max.max(mid),
                };
            }
            left.push(l);
            right.push(r);
            mono.push(m);
        }
        let channels = if same { vec![left] } else { vec![left, right] };
        Self { channels, mono }
    }

    /// Whether there is nothing to draw.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.mono.is_empty()
    }
}

/// The lowest and highest sample over columns `from..to` (fractional
/// indices), so a screen column covering many overview columns keeps every
/// transient. `None` when the range falls outside `peaks`.
#[must_use]
pub fn span(peaks: &[Peak], from: f32, to: f32) -> Option<Peak> {
    if peaks.is_empty() || to < 0.0 {
        return None;
    }
    let first = (from.max(0.0).floor() as usize).min(peaks.len());
    let last = (to.ceil() as usize).max(first + 1).min(peaks.len());
    if first >= last {
        return None;
    }
    let mut out = peaks[first];
    for peak in &peaks[first + 1..last] {
        out.min = out.min.min(peak.min);
        out.max = out.max.max(peak.max);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_cover_every_column() {
        let samples: Vec<f32> = (0..100).map(|index| (index as f32 - 50.0) / 50.0).collect();
        let columns = peaks(&samples, 10);
        assert_eq!(columns.len(), 10);
        assert!(columns[0].min < columns[0].max);
        assert!(columns[9].max > 0.9);
        assert!(columns[0].min >= -1.0);
        assert!(peaks(&[], 4).is_empty());
        assert!(peaks(&samples, 0).is_empty());
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;

    #[test]
    fn zoom_and_position_select_a_window() {
        assert_eq!(visible(100, 1.0, 0.0), (0, 100));
        assert_eq!(visible(100, 2.0, 0.0), (0, 50));
        assert_eq!(visible(100, 2.0, 1.0), (50, 100));
        assert_eq!(visible(100, 4.0, 0.5), (38, 63));
        assert_eq!(visible(0, 4.0, 0.5), (0, 0));
    }
}

#[cfg(test)]
mod overview_tests {
    use super::*;

    #[test]
    fn stereo_keeps_both_channels_and_mono_folds_them() {
        // One second at 1 kHz: left loud, right quiet.
        let samples: Vec<f32> = (0..1000)
            .flat_map(|index| {
                let wave = if index % 2 == 0 { 1.0 } else { -1.0 };
                [wave * 0.8, wave * 0.2]
            })
            .collect();
        let overview = Overview::from_stereo(&samples, 1000);
        assert_eq!(overview.channels.len(), 2);
        assert!(overview.mono.len() >= 200);
        assert!(overview.channels[0][0].max > 0.7);
        assert!(overview.channels[1][0].max < 0.3);
        assert!((overview.mono[0].max - 0.5).abs() < 1e-3);
    }

    #[test]
    fn identical_channels_draw_as_one() {
        let samples = vec![0.5_f32; 2000];
        assert_eq!(Overview::from_stereo(&samples, 1000).channels.len(), 1);
        assert!(Overview::from_stereo(&[], 44_100).is_empty());
    }

    #[test]
    fn span_takes_the_extremes_of_every_column() {
        let peaks = [
            Peak {
                min: -0.1,
                max: 0.1,
            },
            Peak {
                min: -0.9,
                max: 0.2,
            },
            Peak {
                min: -0.2,
                max: 0.7,
            },
        ];
        let all = span(&peaks, 0.0, 3.0).expect("columns");
        assert_eq!((all.min, all.max), (-0.9, 0.7));
        assert_eq!(span(&peaks, 0.2, 0.4).expect("one").max, 0.1);
        assert!(span(&peaks, 5.0, 6.0).is_none());
    }
}
