//! Drum pads: load a loop, slice it, and audition the slices on GM pads.

use std::path::Path;

use audio::drums::{self, Kit, Pad};

use crate::theme;

/// The 16 pad names, from GM notes 36 to 51.
pub const GM_NAMES: [&str; 16] = [
    "Kick",
    "Rim",
    "Snare",
    "Clap",
    "Snare2",
    "Tom low",
    "Hat closed",
    "Tom low2",
    "Tom mid",
    "Hat pedal",
    "Tom high",
    "Tom high2",
    "Crash",
    "Ride",
    "China",
    "Ride bell",
];

/// A loaded loop, ready to audition.
#[derive(Debug)]
pub struct Pads {
    /// Slices mapped to pads.
    pub kit: Kit,
    /// Decoded mono samples.
    pub samples: Vec<f32>,
    /// Sample rate.
    pub sample_rate: u32,
}

impl Pads {
    /// Decodes a WAV, slices it and maps the slices to pads.
    ///
    /// # Errors
    ///
    /// Returns a message for an unreadable file or a loop with no attacks.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let decoded = audio::wav::decode_mono(&bytes)?;
        let cuts = drums::slices(&decoded.samples, decoded.sample_rate);
        let kit = drums::kit(&cuts);
        if kit.pads.is_empty() {
            return Err(String::from("no slices found"));
        }
        Ok(Self {
            kit,
            samples: decoded.samples,
            sample_rate: decoded.sample_rate,
        })
    }

    /// Pad length in milliseconds.
    #[must_use]
    pub fn millis(&self, pad: Pad) -> u64 {
        ((pad.slice.len() as f64 / f64::from(self.sample_rate.max(1))) * 1000.0).round() as u64
    }

    /// Audition buffer for one pad.
    #[must_use]
    pub fn audio(&self, pad: Pad) -> audio::RenderedAudio {
        drums::audition(&self.samples, pad, self.sample_rate)
    }
}

/// The display name for a GM drum note.
#[must_use]
pub fn label(note: u8) -> &'static str {
    usize::from(note.saturating_sub(36))
        .checked_sub(0)
        .filter(|index| *index < GM_NAMES.len())
        .map_or("Pad", |index| GM_NAMES[index])
}

/// Draws the 4x4 pad grid; returns the clicked pad.
pub fn show(ui: &mut egui::Ui, pads: &Pads) -> Option<Pad> {
    let mut hit = None;
    for row in 0..4 {
        ui.horizontal(|ui| {
            for column in 0..4 {
                let index = row * 4 + column;
                let Some(pad) = pads.kit.pads.get(index) else {
                    continue;
                };
                let text = format!("{} {}\n{} ms", label(pad.note), pad.note, pads.millis(*pad));
                if ui
                    .add_sized(
                        [86.0, 44.0],
                        egui::Button::new(egui::RichText::new(text).size(10.0)),
                    )
                    .clicked()
                {
                    hit = Some(*pad);
                }
            }
        });
    }
    if let Some(pad) = hit {
        ui.label(
            egui::RichText::new(format!("Auditioning {}", label(pad.note)))
                .size(10.0)
                .color(theme::ACCENT),
        );
    }
    hit
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio::RenderedAudio;

    fn click_loop() -> Vec<u8> {
        let rate = 48_000_usize;
        // Stereo interleaved: both channels carry the same frame value.
        let mut samples = vec![0.0_f32; rate * 2 * 2];
        for start in [rate / 2, rate] {
            for frame in 0..rate / 100 {
                let value = if frame % 2 == 0 { 0.5 } else { -0.5 };
                let index = (start + frame) * 2;
                if index + 1 < samples.len() {
                    samples[index] = value;
                    samples[index + 1] = value;
                }
            }
        }
        RenderedAudio {
            samples,
            sample_rate: 48_000,
        }
        .to_wav()
    }

    #[test]
    fn a_loaded_loop_maps_slices_to_pads() {
        let path = std::env::temp_dir().join("lazy-arranger-pads-test.wav");
        std::fs::write(&path, click_loop()).expect("write");
        let pads = Pads::load(&path).expect("load");
        assert!(pads.kit.pads.len() >= 2);
        assert_eq!(pads.kit.pads[0].note, 36);
        assert!(pads.millis(pads.kit.pads[0]) > 0);
        assert!(!pads.audio(pads.kit.pads[0]).samples.is_empty());
        assert_eq!(label(36), "Kick");
        assert_eq!(label(38), "Snare");
        assert_eq!(label(99), "Pad");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn an_unreadable_file_is_an_error() {
        assert!(Pads::load(Path::new("/tmp/does-not-exist-pads.wav")).is_err());
    }
}
