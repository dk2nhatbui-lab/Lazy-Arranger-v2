//! Clef glyphs shared by the canvas and PDF renderers.

use std::sync::OnceLock;

/// A decoded RGBA clef image.
#[derive(Debug)]
pub struct ClefImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight-alpha RGBA bytes.
    pub rgba: Vec<u8>,
}

/// Treble-clef PNG bytes (public domain, see `assets/ATTRIBUTION.md`).
pub const TREBLE_PNG: &[u8] = include_bytes!("../assets/treble_clef.png");
/// Bass-clef PNG bytes (public domain, see `assets/ATTRIBUTION.md`).
pub const BASS_PNG: &[u8] = include_bytes!("../assets/bass_clef.png");

static TREBLE: OnceLock<ClefImage> = OnceLock::new();
static BASS: OnceLock<ClefImage> = OnceLock::new();

/// The decoded treble clef.
#[must_use]
pub fn treble() -> &'static ClefImage {
    TREBLE.get_or_init(|| decode(TREBLE_PNG))
}

/// The decoded bass clef.
#[must_use]
pub fn bass() -> &'static ClefImage {
    BASS.get_or_init(|| decode(BASS_PNG))
}

fn decode(bytes: &[u8]) -> ClefImage {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .expect("bundled clef PNG")
        .to_rgba8();
    ClefImage {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clef_images_decode_with_alpha() {
        for image in [treble(), bass()] {
            assert!(image.width > 0 && image.height > 0);
            assert_eq!(
                image.rgba.len(),
                image.width as usize * image.height as usize * 4
            );
            assert!(
                image
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] > 0)
            );
            assert!(
                image
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] == 0)
            );
        }
    }
}
