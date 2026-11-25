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

/// A clef drawn from the Bravura music font (bundled with Cecilia, SIL OFL),
/// with its box in staff spaces around the line the clef sits on.
#[derive(Debug)]
pub struct ClefGlyph {
    /// The rendered glyph, black on transparent.
    pub image: ClefImage,
    /// Left edge from the glyph origin, in staff spaces.
    pub left: f32,
    /// Top edge from the clef's line (G line for treble, F line for bass),
    /// in staff spaces, y down.
    pub top: f32,
    /// Width in staff spaces.
    pub width: f32,
    /// Height in staff spaces.
    pub height: f32,
}

static TREBLE_GLYPH: OnceLock<ClefGlyph> = OnceLock::new();
static BASS_GLYPH: OnceLock<ClefGlyph> = OnceLock::new();

/// The Bravura treble (G) clef; its origin sits on the G line.
#[must_use]
pub fn treble_glyph() -> &'static ClefGlyph {
    TREBLE_GLYPH.get_or_init(|| render_glyph('\u{E050}'))
}

/// The Bravura bass (F) clef; its origin sits on the F line.
#[must_use]
pub fn bass_glyph() -> &'static ClefGlyph {
    BASS_GLYPH.get_or_init(|| render_glyph('\u{E062}'))
}

/// Pixels per staff space in rendered glyphs.
const GLYPH_SPACE: f64 = 100.0;

fn render_glyph(ch: char) -> ClefGlyph {
    use cecilia_engrave::display::FontKind;
    use kurbo::PathEl;

    let fonts = cecilia_engrave::fonts::fonts();
    // One em of the music font is four staff spaces.
    let size = 4.0 * GLYPH_SPACE;
    let bbox = fonts.bbox(FontKind::Music, ch, size);
    let pad = 2.0;
    let width = (bbox.width() + 2.0 * pad).ceil().max(1.0) as u32;
    let height = (bbox.height() + 2.0 * pad).ceil().max(1.0) as u32;
    let mut rgba = vec![0_u8; width as usize * height as usize * 4];
    if let (Some(path), Some(mut pixmap)) = (
        fonts.glyph_path(FontKind::Music, ch, pad - bbox.x0, pad - bbox.y0, size),
        tiny_skia::Pixmap::new(width, height),
    ) {
        let mut builder = tiny_skia::PathBuilder::new();
        for element in path.elements() {
            match *element {
                PathEl::MoveTo(p) => builder.move_to(p.x as f32, p.y as f32),
                PathEl::LineTo(p) => builder.line_to(p.x as f32, p.y as f32),
                PathEl::QuadTo(a, p) => {
                    builder.quad_to(a.x as f32, a.y as f32, p.x as f32, p.y as f32);
                }
                PathEl::CurveTo(a, b, p) => builder.cubic_to(
                    a.x as f32, a.y as f32, b.x as f32, b.y as f32, p.x as f32, p.y as f32,
                ),
                PathEl::ClosePath => builder.close(),
            }
        }
        if let Some(path) = builder.finish() {
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(0, 0, 0, 255);
            paint.anti_alias = true;
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
        for (out, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().zip(pixmap.pixels()) {
            *out = [0x14, 0x14, 0x14, pixel.alpha()];
        }
    }
    let space = GLYPH_SPACE as f32;
    ClefGlyph {
        image: ClefImage {
            width,
            height,
            rgba,
        },
        left: (bbox.x0 - pad) as f32 / space,
        top: (bbox.y0 - pad) as f32 / space,
        width: width as f32 / space,
        height: height as f32 / space,
    }
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
    fn bravura_clefs_render_uncut_around_their_line() {
        let treble = treble_glyph();
        // A G clef reaches about 4.4 spaces above its line and 2.6 below.
        assert!((treble.top + 4.4).abs() < 0.2, "top {}", treble.top);
        assert!((treble.top + treble.height - 2.6).abs() < 0.2);
        let bass = bass_glyph();
        assert!(bass.top < -0.9 && bass.top + bass.height > 2.4);
        for glyph in [treble, bass] {
            let image = &glyph.image;
            let width = image.width as usize;
            let alpha = |x: usize, y: usize| image.rgba[(y * width + x) * 4 + 3];
            // Nothing touches the edges: the glyph is not clipped.
            for x in 0..width {
                assert_eq!(alpha(x, 0), 0);
                assert_eq!(alpha(x, image.height as usize - 1), 0);
            }
            assert!(
                image
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] == 255)
            );
        }
    }

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
