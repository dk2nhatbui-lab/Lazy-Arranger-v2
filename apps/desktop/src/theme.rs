//! The v1 "Studio Console" theme, as data.
//!
//! Values come from `docs/design/v1-visual-language.md`, which was extracted
//! from the C++/JUCE application. Widgets must read these tokens instead of
//! hard-coding colours.

use egui::{Color32, Context, FontFamily, FontId, TextStyle};

/// Window background.
pub const BACKGROUND: Color32 = Color32::from_rgb(0x0b, 0x0d, 0x0f);
/// Panels and list boxes.
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x15, 0x18);
/// Raised surfaces: buttons, editors, menus.
pub const RAISED: Color32 = Color32::from_rgb(0x1a, 0x1e, 0x22);
/// Separators and scrollbar thumbs.
pub const LINE: Color32 = Color32::from_rgb(0x23, 0x28, 0x2d);
/// Emphasised outlines.
pub const LINE_STRONG: Color32 = Color32::from_rgb(0x35, 0x3c, 0x44);
/// Primary text.
pub const TEXT: Color32 = Color32::from_rgb(0xee, 0xe9, 0xe1);
/// Secondary text.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0xa6, 0x9e, 0x93);
/// Tertiary text.
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x6e, 0x69, 0x61);
/// Selection, focus and caret.
pub const ACCENT: Color32 = Color32::from_rgb(0xfc, 0x72, 0x56);
/// Text on the accent.
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0x1b, 0x12, 0x08);
/// Transport playing.
pub const PLAYING: Color32 = Color32::from_rgb(0x7e, 0xcb, 0x72);
/// Recording.
pub const RECORDING: Color32 = Color32::from_rgb(0xdc, 0x7a, 0x6b);
/// Timeline background.
pub const TIMELINE: Color32 = Color32::from_rgb(0x0a, 0x0c, 0x0e);
/// Track lane.
pub const LANE: Color32 = Color32::from_rgb(0x10, 0x13, 0x16);
/// Alternating track lane.
pub const LANE_ALT: Color32 = Color32::from_rgb(0x13, 0x17, 0x1a);
/// Selected track lane.
pub const LANE_SELECTED: Color32 = Color32::from_rgb(0x1b, 0x20, 0x25);
/// Bar grid line.
pub const GRID_BAR: Color32 = Color32::from_rgb(0x2b, 0x31, 0x38);
/// Beat grid line.
pub const GRID_BEAT: Color32 = Color32::from_rgb(0x1d, 0x22, 0x27);
/// Playhead.
pub const PLAYHEAD: Color32 = Color32::from_rgb(0xe7, 0xbd, 0x86);
/// LCD face.
pub const LCD: Color32 = Color32::from_rgb(0x07, 0x09, 0x0a);
/// LCD text.
pub const LCD_TEXT: Color32 = Color32::from_rgb(0xf1, 0xec, 0xe4);

/// The seven track colours, in palette order.
pub const TRACK_COLOURS: [Color32; 7] = [
    Color32::from_rgb(0xfc, 0x72, 0x56),
    Color32::from_rgb(0xd9, 0xc1, 0x95),
    Color32::from_rgb(0x6a, 0xa7, 0x9a),
    Color32::from_rgb(0x76, 0x93, 0xb5),
    Color32::from_rgb(0x9a, 0x8b, 0xbb),
    Color32::from_rgb(0x8b, 0xa9, 0x7b),
    Color32::from_rgb(0xfb, 0xa6, 0xa3),
];

/// Base spacing unit.
pub const GRID: f32 = 4.0;
/// Panel rounding.
pub const RADIUS: f32 = 9.0;
/// Small control rounding.
pub const RADIUS_SMALL: f32 = 7.0;
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

/// Colour for a track index, cycling through the palette.
#[must_use]
pub fn track_colour(index: usize) -> Color32 {
    TRACK_COLOURS[index % TRACK_COLOURS.len()]
}

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
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = TIMELINE;
    visuals.faint_bg_color = LANE_ALT;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, LINE);
    visuals.widgets.inactive.weak_bg_fill = RAISED;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, LINE);
    visuals.widgets.hovered.weak_bg_fill = LINE;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, LINE_STRONG);
    visuals.widgets.active.weak_bg_fill = ACCENT.gamma_multiply(0.25);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
    visuals.window_stroke = egui::Stroke::new(1.0_f32, LINE_STRONG);
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_matches_the_v1_theme() {
        assert_eq!(BACKGROUND.to_array(), [0x0b, 0x0d, 0x0f, 0xff]);
        assert_eq!(ACCENT.to_array(), [0xfc, 0x72, 0x56, 0xff]);
        assert_eq!(PLAYHEAD.to_array(), [0xe7, 0xbd, 0x86, 0xff]);
        assert_eq!(TRACK_COLOURS.len(), 7);
    }

    #[test]
    fn track_colours_cycle() {
        assert_eq!(track_colour(0), TRACK_COLOURS[0]);
        assert_eq!(track_colour(7), TRACK_COLOURS[0]);
        assert_eq!(track_colour(8), TRACK_COLOURS[1]);
    }

    #[test]
    fn the_clear_colour_is_not_white() {
        // ADR-011: a white default must never flash before the first frame.
        assert!(BACKGROUND.r() < 32 && BACKGROUND.g() < 32 && BACKGROUND.b() < 32);
    }
}
