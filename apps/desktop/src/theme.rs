//! The v1 "Studio Console" theme, as data.
//!
//! Values come from `docs/design/v1-visual-language.md`, which was extracted
//! from the C++/JUCE application. Widgets must read these tokens instead of
//! hard-coding colours.

use egui::{Color32, Context, FontFamily, FontId, TextStyle};

/// True when the light palette is active.
#[must_use]
pub fn light() -> bool {
    LIGHT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Switches between the dark (default) and light palettes.
pub fn set_light(on: bool) {
    LIGHT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Reapplies the current palette's visuals after a switch.
pub fn refresh(context: &Context) {
    let mut style = (*context.style()).clone();
    style.visuals = visuals();
    context.set_style(style);
}

static LIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

macro_rules! palette {
    ($(($name:ident, $doc:literal, $dark:expr, $light:expr)),* $(,)?) => {
        $(
            #[doc = $doc]
            #[must_use]
            pub fn $name() -> Color32 {
                if light() { $light } else { $dark }
            }
        )*
    };
}

palette!(
    (
        background,
        "Window background.",
        Color32::from_rgb(0x0b, 0x0d, 0x0f),
        Color32::from_rgb(0xf2, 0xed, 0xe3)
    ),
    (
        panel,
        "Panels and list boxes.",
        Color32::from_rgb(0x12, 0x15, 0x18),
        Color32::from_rgb(0xfa, 0xf6, 0xed)
    ),
    (
        raised,
        "Raised surfaces: buttons, editors, menus.",
        Color32::from_rgb(0x1a, 0x1e, 0x22),
        Color32::from_rgb(0xe7, 0xdf, 0xd2)
    ),
    (
        line,
        "Separators and scrollbar thumbs.",
        Color32::from_rgb(0x23, 0x28, 0x2d),
        Color32::from_rgb(0xd8, 0xcf, 0xc0)
    ),
    (
        line_strong,
        "Emphasised outlines.",
        Color32::from_rgb(0x35, 0x3c, 0x44),
        Color32::from_rgb(0xb7, 0xad, 0x9d)
    ),
    (
        text,
        "Primary text.",
        Color32::from_rgb(0xee, 0xe9, 0xe1),
        Color32::from_rgb(0x1b, 0x17, 0x13)
    ),
    (
        text_muted,
        "Secondary text.",
        Color32::from_rgb(0xa6, 0x9e, 0x93),
        Color32::from_rgb(0x5f, 0x57, 0x4c)
    ),
    (
        text_dim,
        "Tertiary text.",
        Color32::from_rgb(0x6e, 0x69, 0x61),
        Color32::from_rgb(0x85, 0x7b, 0x6d)
    ),
    (
        accent,
        "Selection, focus and caret.",
        Color32::from_rgb(0xfc, 0x72, 0x56),
        Color32::from_rgb(0xc9, 0x4f, 0x34)
    ),
    (
        accent_text,
        "Text on the accent.",
        Color32::from_rgb(0x1b, 0x12, 0x08),
        Color32::from_rgb(0xff, 0xf8, 0xf2)
    ),
    (
        playing,
        "Transport playing.",
        Color32::from_rgb(0x7e, 0xcb, 0x72),
        Color32::from_rgb(0x3f, 0x7d, 0x3a)
    ),
    (
        recording,
        "Recording.",
        Color32::from_rgb(0xdc, 0x7a, 0x6b),
        Color32::from_rgb(0xb2, 0x3b, 0x2b)
    ),
    (
        timeline,
        "Timeline background.",
        Color32::from_rgb(0x0a, 0x0c, 0x0e),
        Color32::from_rgb(0xea, 0xe3, 0xd7)
    ),
    (
        lane,
        "Track lane.",
        Color32::from_rgb(0x10, 0x13, 0x16),
        Color32::from_rgb(0xf5, 0xef, 0xe5)
    ),
    (
        lane_alt,
        "Alternating track lane.",
        Color32::from_rgb(0x13, 0x17, 0x1a),
        Color32::from_rgb(0xee, 0xe7, 0xdb)
    ),
    (
        lane_selected,
        "Selected track lane.",
        Color32::from_rgb(0x1b, 0x20, 0x25),
        Color32::from_rgb(0xe1, 0xd7, 0xc7)
    ),
    (
        grid_bar,
        "Bar grid line.",
        Color32::from_rgb(0x2b, 0x31, 0x38),
        Color32::from_rgb(0xc4, 0xb9, 0xa8)
    ),
    (
        grid_beat,
        "Beat grid line.",
        Color32::from_rgb(0x1d, 0x22, 0x27),
        Color32::from_rgb(0xde, 0xd4, 0xc4)
    ),
    (
        playhead,
        "Playhead.",
        Color32::from_rgb(0xe7, 0xbd, 0x86),
        Color32::from_rgb(0xa8, 0x5a, 0x2f)
    ),
    (
        lcd,
        "LCD face (the black surround and tab strip).",
        Color32::from_rgb(0x07, 0x09, 0x0a),
        Color32::from_rgb(0x07, 0x09, 0x0a)
    ),
    (
        lcd_back,
        "LCD backlit plate, from `lcdBack`.",
        Color32::from_rgb(0xb3, 0xbb, 0xb0),
        Color32::from_rgb(0xb3, 0xbb, 0xb0)
    ),
    (
        lcd_ink,
        "LCD ink for values, from `lcdInk`.",
        Color32::from_rgb(0x1d, 0x24, 0x1d),
        Color32::from_rgb(0x1d, 0x24, 0x1d)
    ),
    (
        lcd_dim,
        "LCD dim ink for labels and dividers, from `lcdDim`.",
        Color32::from_rgb(0x4d, 0x57, 0x4d),
        Color32::from_rgb(0x4d, 0x57, 0x4d)
    ),
    (
        lcd_bezel,
        "LCD bezel, from `lcdBezel`.",
        Color32::from_rgb(0x2a, 0x2e, 0x2b),
        Color32::from_rgb(0x2a, 0x2e, 0x2b)
    ),
    (
        lcd_text,
        "LCD text.",
        Color32::from_rgb(0xf1, 0xec, 0xe4),
        Color32::from_rgb(0xf1, 0xec, 0xe4)
    ),
);

const DARK_TRACKS: [Color32; 7] = [
    Color32::from_rgb(0xfc, 0x72, 0x56),
    Color32::from_rgb(0xd9, 0xc1, 0x95),
    Color32::from_rgb(0x6a, 0xa7, 0x9a),
    Color32::from_rgb(0x76, 0x93, 0xb5),
    Color32::from_rgb(0x9a, 0x8b, 0xbb),
    Color32::from_rgb(0x8b, 0xa9, 0x7b),
    Color32::from_rgb(0xfb, 0xa6, 0xa3),
];

const LIGHT_TRACKS: [Color32; 7] = [
    Color32::from_rgb(0xc2, 0x4f, 0x35),
    Color32::from_rgb(0x8a, 0x6f, 0x3f),
    Color32::from_rgb(0x3f, 0x7a, 0x70),
    Color32::from_rgb(0x4a, 0x6f, 0x96),
    Color32::from_rgb(0x6d, 0x5a, 0x91),
    Color32::from_rgb(0x5f, 0x7f, 0x4f),
    Color32::from_rgb(0xc4, 0x6b, 0x68),
];

/// Colour for a track index, cycling through the current palette.
#[must_use]
pub fn track_colour(index: usize) -> Color32 {
    let colours = if light() { &LIGHT_TRACKS } else { &DARK_TRACKS };
    colours[index % colours.len()]
}

/// Base spacing unit.
pub const GRID: f32 = 4.0;
/// Panel corner radius. The owner asked for square corners, so this is 0
/// (v1 used 9 and 7).
pub const RADIUS: f32 = 0.0;
/// Small control corner radius; square, like [`RADIUS`].
pub const RADIUS_SMALL: f32 = 0.0;
/// Top bar height.
pub const HEADER_HEIGHT: f32 = 60.0;
/// Status bar height.
pub const STATUS_HEIGHT: f32 = 28.0;
/// Browser width.
pub const SIDEBAR_WIDTH: f32 = 248.0;
/// Inspector width, reserved for later.
pub const INSPECTOR_WIDTH: f32 = 296.0;
/// One track lane height.
pub const TRACK_HEIGHT: f32 = 64.0;
/// Base font size.
pub const FONT_SIZE: f32 = 13.0;

/// Installs fonts, visuals and spacing. Call once per context.
pub fn install(context: &Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        String::from("inter"),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        String::from("inter-semibold"),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/Inter-SemiBold.ttf"
        ))),
    );
    if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
        family.insert(0, String::from("inter"));
    }
    // A FontFamily::Name needs its own entry; registering font data alone is
    // not enough, and using an unbound name panics inside the paint callback.
    fonts.families.insert(
        FontFamily::Name(String::from("inter-semibold").into()),
        vec![String::from("inter-semibold")],
    );
    if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
        family.push(String::from("inter"));
    }
    context.set_fonts(fonts);

    let mut style = (*context.style()).clone();
    style.visuals = visuals();
    style.spacing.item_spacing = egui::vec2(GRID, GRID);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size.y = 22.0;
    style.text_styles = [
        (TextStyle::Body, FontId::proportional(FONT_SIZE)),
        (TextStyle::Button, FontId::proportional(FONT_SIZE)),
        (TextStyle::Small, FontId::proportional(11.0)),
        (
            TextStyle::Heading,
            FontId::new(15.0, FontFamily::Name("inter-semibold".into())),
        ),
        (TextStyle::Monospace, FontId::monospace(12.0)),
    ]
    .into();
    context.set_style(style);
}

/// The dark visuals derived from the palette.
#[must_use]
pub fn visuals() -> egui::Visuals {
    let mut visuals = if light() {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    visuals.panel_fill = panel();
    visuals.window_fill = panel();
    visuals.extreme_bg_color = timeline();
    visuals.faint_bg_color = lane_alt();
    visuals.override_text_color = Some(text());
    visuals.selection.bg_fill = accent().gamma_multiply(0.35);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent());
    visuals.hyperlink_color = accent();
    visuals.widgets.noninteractive.bg_fill = panel();
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, line());
    visuals.widgets.inactive.weak_bg_fill = raised();
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, line());
    visuals.widgets.hovered.weak_bg_fill = line();
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, line_strong());
    visuals.widgets.active.weak_bg_fill = accent().gamma_multiply(0.25);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, accent());
    visuals.window_stroke = egui::Stroke::new(1.0_f32, line_strong());
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_matches_the_v1_theme_and_switches() {
        set_light(false);
        assert_eq!(background().to_array(), [0x0b, 0x0d, 0x0f, 0xff]);
        assert_eq!(accent().to_array(), [0xfc, 0x72, 0x56, 0xff]);
        assert_eq!(playhead().to_array(), [0xe7, 0xbd, 0x86, 0xff]);
        set_light(true);
        assert_eq!(background().to_array(), [0xf2, 0xed, 0xe3, 0xff]);
        assert!(light());
        set_light(false);
        assert!(!light());
    }

    #[test]
    fn track_colours_cycle() {
        set_light(false);
        assert_eq!(track_colour(0).to_array(), [0xfc, 0x72, 0x56, 0xff]);
        assert_eq!(track_colour(7), track_colour(0));
        assert_eq!(track_colour(8).to_array(), DARK_TRACKS[1].to_array());
        set_light(true);
        assert_eq!(track_colour(7), track_colour(0));
        assert_ne!(track_colour(0).to_array(), [0xfc, 0x72, 0x56, 0xff]);
        set_light(false);
    }

    #[test]
    fn every_text_style_has_a_bound_font_family() {
        let context = Context::default();
        install(&context);
        context.begin_pass(egui::RawInput::default());
        let styles = context.style().text_styles.clone();
        for (style, font_id) in styles {
            let galley = context.fonts(|fonts| {
                fonts.layout_no_wrap(String::from("Lazy Arranger"), font_id.clone(), text())
            });
            assert!(
                !galley.rows.is_empty(),
                "text style {style:?} with {font_id:?} produced no glyphs"
            );
        }
        let _ = context.end_pass();
    }

    #[test]
    fn the_clear_colour_is_not_white() {
        // ADR-011: a white default must never flash before the first frame.
        set_light(false);
        let dark = background();
        assert!(dark.r() < 32 && dark.g() < 32 && dark.b() < 32);
    }
}
