//! Waveform peaks for drawing audio references.

/// Minimum and maximum sample in one column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Peak {
    /// Lowest sample.
    pub min: f32,
    /// Highest sample.
    pub max: f32,
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
