//! Widgets that reproduce the v1 JUCE drawing.
//!
//! Values and geometry come from `src/app/ui/HeaderBar.cpp` in the C++ app:
//! `LcdDisplay::paint` and `ViewTabs::paint`. The text formatting matches
//! `core/src/Time.cpp::formatBarBeat` and the header's own strings.

use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};
use music_core::{PPQ, Tick, TimeSignature};

use crate::theme;

/// LCD panel size, from `HeaderBar::resized`: 350 x 52.
pub const LCD_SIZE: Vec2 = Vec2::new(340.0, 48.0);
/// Cell widths, from `LcdDisplay::cell`.
const CELLS: [f32; 3] = [142.0, 116.0, 92.0];
/// View tab strip size, from `HeaderBar::resized`.
pub const TABS_SIZE: Vec2 = Vec2::new(320.0, 38.0);

/// Bar, beat and sixteenth, one-based, as the LCD shows them.
#[must_use]
pub fn bar_beat_text(position: Tick, meter: TimeSignature) -> String {
    let bar = meter.bar_ticks().max(1);
    let beat = meter.beat_ticks().max(1);
    let sixteenth = PPQ / 4;
    let bar_index = position.div_euclid(bar);
    let in_bar = position.rem_euclid(bar);
    let beat_index = in_bar.div_euclid(beat);
    let in_beat = in_bar.rem_euclid(beat);
    let sixteenth_index = in_beat.div_euclid(sixteenth);
    format!(
        "{} . {} . {}",
        bar_index + 1,
        beat_index + 1,
        sixteenth_index + 1
    )
}

/// Tempo with two decimals, as `juce::String(tempo, 2)` prints it.
#[must_use]
pub fn tempo_text(bpm: f64) -> String {
    format!("{bpm:.2}")
}

/// Meter with spaces around the slash, as the LCD draws it.
#[must_use]
pub fn meter_text(meter: TimeSignature) -> String {
    format!("{} / {}", meter.numerator, meter.denominator)
}

/// The glass LCD with its three cells.
pub fn lcd(ui: &mut Ui, position: Tick, bpm: f64, meter: TimeSignature) -> Response {
    let (response, painter) = ui.allocate_painter(LCD_SIZE, Sense::hover());
    let outer = response.rect.shrink(0.5);
    let screen = outer.shrink(4.0);
    let radius = 0.0_f32;

    // Bezel and its outline.
    painter.rect_filled(outer, radius, theme::lcd_bezel());
    painter.rect_stroke(
        outer,
        radius,
        Stroke::new(1.0_f32, theme::line_strong().gamma_multiply(0.8)),
        StrokeKind::Inside,
    );

    // Glass: three bands approximate the top-brighter/bottom-darker gradient
    // the JUCE code builds with ColourGradient.
    let bands = [
        (0.00, 0.40, lighten(theme::lcd_back(), 0.05)),
        (0.40, 0.75, theme::lcd_back()),
        (0.75, 1.00, darken(theme::lcd_back(), 0.14)),
    ];
    for (from, to, colour) in bands {
        let band = Rect::from_min_max(
            Pos2::new(screen.left(), screen.top() + screen.height() * from),
            Pos2::new(screen.right(), screen.top() + screen.height() * to),
        );
        painter.rect_filled(band, 0.0, colour);
    }

    // Scanlines and the top highlight, as in LcdDisplay::paint.
    let mut y = screen.top() + 1.0;
    while y < screen.bottom() {
        painter.line_segment(
            [Pos2::new(screen.left(), y), Pos2::new(screen.right(), y)],
            Stroke::new(1.0_f32, Color32::BLACK.gamma_multiply(0.07)),
        );
        y += 3.0;
    }
    let highlight = Rect::from_min_size(
        screen.min,
        Vec2::new(screen.width(), screen.height() * 0.40),
    );
    painter.rect_filled(highlight, 0.0, Color32::WHITE.gamma_multiply(0.11));

    // Cell separators.
    let mut x = outer.left();
    for (index, width) in CELLS.iter().enumerate() {
        x += width;
        if index < CELLS.len() - 1 {
            painter.line_segment(
                [
                    Pos2::new(x, outer.top() + 12.0),
                    Pos2::new(x, outer.bottom() - 10.0),
                ],
                Stroke::new(1.0_f32, theme::lcd_dim().gamma_multiply(0.35)),
            );
        }
    }

    let values = [
        bar_beat_text(position, meter),
        tempo_text(bpm),
        meter_text(meter),
    ];
    let labels = ["BAR \u{b7} BEAT", "TEMPO", "METER"];
    let mut left = outer.left();
    for (index, width) in CELLS.iter().enumerate() {
        let cell = Rect::from_min_size(
            Pos2::new(left, outer.top()),
            Vec2::new(*width, outer.height()),
        )
        .shrink2(Vec2::new(14.0, 6.0));
        let label_rect = Rect::from_min_size(cell.min, Vec2::new(cell.width(), 11.0));
        tracked_text(
            &painter,
            label_rect.left_top(),
            labels[index],
            FontId::proportional(9.5),
            theme::lcd_dim(),
            1.4,
        );
        let value_rect = Rect::from_min_max(Pos2::new(cell.left(), cell.top() + 11.0), cell.max);
        let font = FontId::proportional(20.0);
        let galley = painter.layout_no_wrap(values[index].clone(), font.clone(), theme::lcd_ink());
        let y = value_rect.center().y - galley.size().y * 0.5;
        painter.galley(Pos2::new(value_rect.left(), y), galley, theme::lcd_ink());
        left += width;
    }
    response
}

/// View tabs drawn like the v1 strip; returns the newly selected index.
pub fn view_tabs(ui: &mut Ui, current: usize, names: &[&str]) -> Option<usize> {
    let (response, painter) = ui.allocate_painter(TABS_SIZE, Sense::click());
    let rect = response.rect;
    painter.rect_filled(rect, 0.0, theme::panel());
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, theme::line()),
        StrokeKind::Inside,
    );

    let inner = rect.shrink2(Vec2::new(5.0, 5.0));
    let mut widths = Vec::with_capacity(names.len());
    let mut used = 0.0_f32;
    for name in names {
        let width = painter
            .layout_no_wrap(
                String::from(*name),
                FontId::proportional(10.5),
                theme::text(),
            )
            .size()
            .x
            + 10.0;
        widths.push(width);
        used += width;
    }
    let extra = inner.width() - used;
    if extra > 0.0 {
        for width in &mut widths {
            *width += extra / names.len() as f32;
        }
    }

    let mut clicked = None;
    let mut left = inner.left();
    for (index, name) in names.iter().enumerate() {
        let width = widths[index];
        let tab = Rect::from_min_size(
            Pos2::new(left, inner.top()),
            Vec2::new(width, inner.height()),
        );
        if response.clicked()
            && let Some(pointer) = response.interact_pointer_pos()
            && tab.contains(pointer)
        {
            clicked = Some(index);
        }
        if index == current {
            painter.rect_filled(
                tab,
                theme::RADIUS_SMALL,
                theme::accent().gamma_multiply(0.10),
            );
            painter.line_segment(
                [tab.left_bottom(), tab.right_bottom()],
                Stroke::new(2.0_f32, theme::accent()),
            );
            let width = 26.0_f32.min(tab.width() * 0.4);
            let underline = Rect::from_center_size(
                Pos2::new(tab.center().x, tab.bottom() - 7.0),
                Vec2::new(width, 2.0),
            );
            painter.rect_filled(underline, 0.0, theme::accent());
        }
        let colour = if index == current {
            theme::text()
        } else {
            theme::text_muted()
        };
        painter.text(
            tab.center(),
            Align2::CENTER_CENTER,
            *name,
            FontId::proportional(10.5),
            colour,
        );
        left += width;
    }
    clicked
}

/// Draws text with extra tracking between characters.
fn tracked_text(
    painter: &egui::Painter,
    pos: Pos2,
    text: &str,
    font: FontId,
    colour: Color32,
    tracking: f32,
) {
    let mut x = pos.x;
    for character in text.chars() {
        let galley = painter.layout_no_wrap(character.to_string(), font.clone(), colour);
        let width = galley.size().x;
        painter.galley(Pos2::new(x, pos.y), galley, colour);
        x += width + tracking;
    }
}

/// Lightens towards white by `amount` (0..=1), matching JUCE's `brighter`.
fn lighten(colour: Color32, amount: f32) -> Color32 {
    let mix = |channel: u8| -> u8 {
        (f32::from(channel) + (255.0 - f32::from(channel)) * amount.clamp(0.0, 1.0)).round() as u8
    };
    Color32::from_rgb(mix(colour.r()), mix(colour.g()), mix(colour.b()))
}

/// Darkens towards black by `amount` (0..=1), matching JUCE's `darker`.
fn darken(colour: Color32, amount: f32) -> Color32 {
    let mix =
        |channel: u8| -> u8 { (f32::from(channel) * (1.0 - amount.clamp(0.0, 1.0))).round() as u8 };
    Color32::from_rgb(mix(colour.r()), mix(colour.g()), mix(colour.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_beat_matches_the_oracle_format() {
        let four_four = TimeSignature::default();
        assert_eq!(bar_beat_text(0, four_four), "1 . 1 . 1");
        assert_eq!(bar_beat_text(PPQ, four_four), "1 . 2 . 1");
        assert_eq!(bar_beat_text(PPQ / 4, four_four), "1 . 1 . 2");
        assert_eq!(bar_beat_text(3840, four_four), "2 . 1 . 1");
        assert_eq!(bar_beat_text(3840 + 2 * PPQ, four_four), "2 . 3 . 1");
        let three_four = TimeSignature::new(3, 4).expect("valid");
        assert_eq!(bar_beat_text(3 * PPQ, three_four), "2 . 1 . 1");
    }

    #[test]
    fn tempo_and_meter_text_match_the_oracle() {
        assert_eq!(tempo_text(120.0), "120.00");
        assert_eq!(tempo_text(112.5), "112.50");
        assert_eq!(meter_text(TimeSignature::default()), "4 / 4");
        assert_eq!(
            meter_text(TimeSignature::new(6, 8).expect("valid")),
            "6 / 8"
        );
    }

    #[test]
    fn gradient_helpers_move_towards_white_and_black() {
        let base = Color32::from_rgb(100, 100, 100);
        let lighter = lighten(base, 0.05);
        let darker = darken(base, 0.14);
        assert!(lighter.r() > base.r());
        assert!(darker.r() < base.r());
        assert_eq!(lighter.to_array()[3], 255);
    }
}
