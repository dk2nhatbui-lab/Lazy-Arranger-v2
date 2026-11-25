//! Widgets that reproduce the v1 JUCE drawing.
//!
//! Values and geometry come from `src/app/ui/HeaderBar.cpp` in the C++ app:
//! `LcdDisplay::paint` and `ViewTabs::paint`. The text formatting matches
//! `core/src/Time.cpp::formatBarBeat` and the header's own strings.

use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};
use music_core::{PPQ, Tick, TimeSignature};

use crate::theme;

mod mixer;
pub(crate) use mixer::{
    MIXER_KNOB_SIZE, mixer_fader, mixer_knob, mixer_panel_background, mixer_strip, name_tab,
    strip_button, strip_caption,
};

/// LCD panel size, from `HeaderBar::resized`: 350 x 52.
pub const LCD_SIZE: Vec2 = Vec2::new(350.0, 52.0);
/// Cell widths, from `LcdDisplay::cell`.
const CELLS: [f32; 3] = [142.0, 116.0, 92.0];
/// Width of the LCD meter cell, which opens the meter menu.
pub const LCD_METER_WIDTH: f32 = CELLS[2];

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
            theme::semibold(9.5),
            theme::lcd_dim(),
            1.4,
        );
        let value_rect = Rect::from_min_max(Pos2::new(cell.left(), cell.top() + 11.0), cell.max);
        let font = theme::semibold(20.0);
        let galley = painter.layout_no_wrap(values[index].clone(), font.clone(), theme::lcd_ink());
        let y = value_rect.center().y - galley.size().y * 0.5;
        painter.galley(Pos2::new(value_rect.left(), y), galley, theme::lcd_ink());
        left += width;
    }
    response
}

/// View tabs drawn like the v1 strip; returns the newly selected index.
pub fn view_tabs(ui: &mut Ui, current: usize, names: &[&str]) -> Option<usize> {
    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click());
    let rect = response.rect;
    let strip = if theme::light() {
        theme::lcd().gamma_multiply(0.06)
    } else {
        theme::lcd()
    };
    painter.rect_filled(rect, theme::RADIUS, strip);
    painter.rect_stroke(
        rect.shrink(0.5),
        theme::RADIUS,
        Stroke::new(1.0_f32, theme::line()),
        StrokeKind::Inside,
    );

    let inner = rect.shrink(5.0);
    let mut widths = Vec::with_capacity(names.len());
    let mut used = 0.0_f32;
    for name in names {
        let width = painter
            .layout_no_wrap(String::from(*name), theme::semibold(10.5), theme::text())
            .size()
            .x
            + 10.0;
        widths.push(width);
        used += width;
    }
    let extra = inner.width() - used;
    for width in &mut widths {
        *width = (*width + extra / names.len() as f32).max(20.0);
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
        let selected = index == current;
        if selected {
            painter.rect_filled(
                tab,
                theme::RADIUS_SMALL,
                theme::accent().gamma_multiply(0.14),
            );
            painter.rect_stroke(
                tab.shrink(0.5),
                theme::RADIUS_SMALL,
                Stroke::new(1.0_f32, theme::accent().gamma_multiply(0.4)),
                StrokeKind::Inside,
            );
            let width = 26.0_f32.min(tab.width() * 0.4);
            let underline = Rect::from_center_size(
                Pos2::new(tab.center().x, tab.bottom() - 7.0),
                Vec2::new(width, 2.0),
            );
            painter.rect_filled(underline, 0.0, theme::accent());
        }
        let (font, colour) = if selected {
            (theme::semibold(10.5), theme::text())
        } else {
            (FontId::proportional(10.5), theme::text_muted())
        };
        fitted_text(
            &painter,
            tab.shrink2(Vec2::new(2.0, 0.0)),
            name,
            font,
            colour,
        );
        left += width;
    }
    clicked
}

/// Draws `text` centred in `rect` like JUCE `drawFittedText` with a minimum
/// scale of 0.7: shrinks it to fit, then cuts it with an ellipsis.
pub fn fitted_text(painter: &egui::Painter, rect: Rect, text: &str, font: FontId, colour: Color32) {
    let galley = painter.layout_no_wrap(text.to_owned(), font.clone(), colour);
    let width = galley.size().x;
    let galley = if width <= rect.width() {
        galley
    } else if width * 0.7 <= rect.width() {
        let scaled = FontId::new(font.size * rect.width() / width, font.family.clone());
        painter.layout_no_wrap(text.to_owned(), scaled, colour)
    } else {
        let small = FontId::new(font.size * 0.7, font.family.clone());
        let mut cut: Vec<char> = text.chars().collect();
        loop {
            cut.pop();
            let candidate: String = cut.iter().collect::<String>() + "\u{2026}";
            let galley = painter.layout_no_wrap(candidate, small.clone(), colour);
            if galley.size().x <= rect.width() || cut.is_empty() {
                break galley;
            }
        }
    };
    let pos = rect.center() - galley.size() * 0.5;
    painter.with_clip_rect(rect).galley(pos, galley, colour);
}

/// An inspector section heading (`sectionLabel` in Panels.cpp): 11 pt
/// semibold capitals in `textDim`, slightly spaced, on the bottom of a row
/// `height` points tall.
pub fn section_label(ui: &mut Ui, text: &str, height: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let galley_height = 14.0;
    tracked_text(
        ui.painter(),
        Pos2::new(rect.left(), rect.bottom() - galley_height),
        &text.to_uppercase(),
        theme::semibold(11.0),
        theme::text_dim(),
        1.1,
    );
}

/// A v1 toggle switch (`AppLookAndFeel::drawToggleButton`): the label on the
/// left, a 34 x 20 switch on the right of `rect`. Returns true when clicked.
pub fn toggle_switch(ui: &mut Ui, rect: Rect, id: &str, label: &str, on: bool) -> bool {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    let switch = Rect::from_center_size(
        Pos2::new(rect.right() - 17.0, rect.center().y),
        Vec2::new(34.0, 20.0),
    );
    let track = if on {
        theme::accent()
    } else if response.hovered() {
        theme::line_strong()
    } else {
        theme::line_strong().gamma_multiply(0.8)
    };
    ui.painter().rect_filled(switch, theme::RADIUS, track);
    let knob_x = if on {
        switch.right() - 10.0
    } else {
        switch.left() + 10.0
    };
    ui.painter().circle_filled(
        Pos2::new(knob_x, switch.center().y),
        7.0,
        if on {
            theme::accent_text()
        } else {
            theme::text()
        },
    );
    if !label.is_empty() {
        ui.painter().text(
            Pos2::new(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            theme::text(),
        );
    }
    response.clicked()
}

/// A v1 horizontal slider without a text box (`drawLinearSlider`): a 4 pt
/// `lineStrong` track, filled in accent up to the 14 pt round thumb.
/// Returns true while the value changes.
pub fn flat_slider(ui: &mut Ui, rect: Rect, id: &str, value: &mut f32) -> bool {
    let response = ui.interact(rect, ui.id().with(id), Sense::click_and_drag());
    let left = rect.left() + 7.0;
    let right = rect.right() - 7.0;
    let mut changed = false;
    if (response.dragged() || response.clicked())
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let new = ((pointer.x - left) / (right - left).max(1.0)).clamp(0.0, 1.0);
        if (new - *value).abs() > f32::EPSILON {
            *value = new;
            changed = true;
        }
    }
    let track = Rect::from_min_max(
        Pos2::new(left, rect.center().y - 2.0),
        Pos2::new(right, rect.center().y + 2.0),
    );
    ui.painter()
        .rect_filled(track, theme::RADIUS, theme::line_strong());
    let x = left + (right - left) * value.clamp(0.0, 1.0);
    ui.painter().rect_filled(
        Rect::from_min_max(track.min, Pos2::new(x, track.bottom())),
        theme::RADIUS,
        theme::accent(),
    );
    ui.painter()
        .circle_filled(Pos2::new(x, track.center().y), 7.0, theme::text());
    changed
}

/// A v1 text button (`AppLookAndFeel::drawButtonBackground`): raised fill,
/// `lineStrong` outline, 13 pt semibold label; toggled buttons take an accent
/// tint.
pub fn text_button(ui: &mut Ui, rect: Rect, id: &str, label: &str, toggled: bool) -> Response {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    let mut fill = if toggled {
        crate::transport::blend(theme::raised(), theme::accent(), 0.18)
    } else {
        theme::raised()
    };
    if response.is_pointer_button_down_on() {
        fill = crate::transport::blend(fill, Color32::BLACK, 0.15);
    } else if response.hovered() {
        fill = crate::transport::blend(fill, Color32::WHITE, 0.06);
    }
    let r = rect.shrink(0.5);
    ui.painter().rect_filled(r, theme::RADIUS_SMALL, fill);
    ui.painter().rect_stroke(
        r,
        theme::RADIUS_SMALL,
        Stroke::new(
            1.0_f32,
            if toggled {
                theme::accent().gamma_multiply(0.6)
            } else {
                theme::line_strong()
            },
        ),
        StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        theme::semibold(13.0),
        theme::text(),
    );
    response
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

/// A round transport button drawn like v1's `RoundButton`.
pub(crate) fn draw_track_icon(
    ui: &egui::Ui,
    rect: Rect,
    name: &str,
    program: Option<u8>,
    audio: bool,
    color: egui::Color32,
) {
    let lower = name.to_ascii_lowercase();
    let has = |word: &str| lower.contains(word);
    let icon = if audio {
        "waveform"
    } else if has("drum") || has("kit") || has("perc") {
        "drums"
    } else if has("guitar") {
        "guitar"
    } else if has("bass") {
        "bass"
    } else if has("brass") || has("trumpet") || has("trombone") || has("sax") || has("horn") {
        "brass"
    } else if has("piano") || has("keys") {
        "piano"
    } else if has("lead") || has("synth") || has("pad") || has("arp") {
        "synth"
    } else {
        match program.unwrap_or(0) {
            24..=31 => "guitar",
            32..=39 => "bass",
            56..=63 => "brass",
            80..=87 => "synth",
            0..=7 => "piano",
            _ => "instrument",
        }
    };
    let painter = ui.painter();
    let c = rect.center();
    let ink = theme::text_muted();
    let stroke = egui::Stroke::new(1.4_f32, ink);
    let line = |a: egui::Pos2, b: egui::Pos2| painter.line_segment([a, b], stroke);
    match icon {
        "drums" => {
            painter.circle_stroke(egui::pos2(c.x - 3.0, c.y + 3.0), 4.5, stroke);
            painter.circle_stroke(egui::pos2(c.x - 3.0, c.y + 3.0), 1.8, stroke);
            let snare =
                Rect::from_center_size(egui::pos2(c.x + 5.0, c.y + 2.0), egui::vec2(5.0, 3.5));
            painter.rect_stroke(snare, 1.0, stroke, egui::StrokeKind::Inside);
            line(
                egui::pos2(snare.left(), snare.bottom()),
                egui::pos2(snare.left(), c.y + 8.0),
            );
            line(
                egui::pos2(snare.right(), snare.bottom()),
                egui::pos2(snare.right(), c.y + 8.0),
            );
            line(
                egui::pos2(c.x - 7.0, c.y - 8.0),
                egui::pos2(c.x + 2.0, c.y - 8.0),
            );
            line(egui::pos2(c.x - 5.0, c.y - 8.0), egui::pos2(c.x, c.y + 1.0));
        }
        "guitar" => {
            painter.circle_stroke(egui::pos2(c.x - 4.0, c.y + 4.0), 4.5, stroke);
            painter.circle_stroke(egui::pos2(c.x + 0.5, c.y - 0.5), 3.5, stroke);
            line(
                egui::pos2(c.x + 1.0, c.y - 1.0),
                egui::pos2(c.x + 9.0, c.y - 9.0),
            );
            painter.circle_filled(egui::pos2(c.x - 4.0, c.y + 4.0), 1.5, ink);
        }
        "bass" => {
            painter.circle_stroke(egui::pos2(c.x - 4.0, c.y + 4.0), 5.0, stroke);
            line(
                egui::pos2(c.x - 1.0, c.y + 1.0),
                egui::pos2(c.x + 8.0, c.y - 8.0),
            );
            painter.rect_filled(
                Rect::from_center_size(egui::pos2(c.x + 8.5, c.y - 8.5), egui::vec2(3.0, 4.0)),
                1.0,
                ink,
            );
        }
        "brass" => {
            let points = vec![
                egui::pos2(c.x - 9.0, c.y),
                egui::pos2(c.x - 2.0, c.y + 1.0),
                egui::pos2(c.x + 2.0, c.y + 5.0),
                egui::pos2(c.x + 8.0, c.y + 6.0),
                egui::pos2(c.x + 8.0, c.y - 5.0),
                egui::pos2(c.x + 2.0, c.y - 3.0),
                egui::pos2(c.x - 2.0, c.y + 1.0),
            ];
            painter.add(egui::Shape::line(points, stroke));
            line(
                egui::pos2(c.x - 1.0, c.y - 3.0),
                egui::pos2(c.x - 1.0, c.y + 3.0),
            );
        }
        "piano" => {
            let keys = Rect::from_center_size(egui::pos2(c.x, c.y - 1.0), egui::vec2(15.0, 7.0));
            painter.rect_stroke(keys, 1.0, stroke, egui::StrokeKind::Inside);
            for index in 1..5 {
                let x = keys.left() + keys.width() * index as f32 / 5.0;
                line(egui::pos2(x, keys.top()), egui::pos2(x, keys.bottom()));
            }
            line(
                egui::pos2(keys.left() + 2.0, keys.bottom()),
                egui::pos2(keys.left() + 1.0, c.y + 9.0),
            );
            line(
                egui::pos2(keys.right() - 2.0, keys.bottom()),
                egui::pos2(keys.right() - 1.0, c.y + 9.0),
            );
        }
        "synth" => {
            let panel = Rect::from_center_size(c, egui::vec2(15.0, 10.0));
            painter.rect_stroke(panel, 1.0, stroke, egui::StrokeKind::Inside);
            for index in 0..3 {
                painter.circle_stroke(
                    egui::pos2(panel.left() + 3.0 + index as f32 * 4.0, panel.top() + 3.0),
                    1.4,
                    stroke,
                );
            }
            line(
                egui::pos2(panel.left() + 2.0, panel.bottom() - 2.0),
                egui::pos2(panel.right() - 2.0, panel.bottom() - 2.0),
            );
        }
        "waveform" => {
            let heights = [4.0_f32, 8.0, 11.0, 6.0, 9.0];
            for (index, height) in heights.iter().enumerate() {
                let x = c.x - 8.0 + index as f32 * 4.0;
                painter.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(x, c.y - height / 2.0),
                        egui::pos2(x + 2.5, c.y + height / 2.0),
                    ),
                    1.0,
                    color,
                );
            }
        }
        _ => {
            let keys = Rect::from_center_size(c, egui::vec2(14.0, 10.0));
            painter.rect_stroke(keys, 1.0, stroke, egui::StrokeKind::Inside);
            for index in 1..4 {
                let x = keys.left() + keys.width() * index as f32 / 4.0;
                line(egui::pos2(x, keys.top()), egui::pos2(x, keys.bottom()));
            }
            for index in 0..3 {
                let x = keys.left() + 2.0 + index as f32 * 3.5;
                painter.rect_filled(
                    Rect::from_min_size(egui::pos2(x, keys.top()), egui::vec2(2.0, 5.0)),
                    0.5,
                    ink,
                );
            }
        }
    }
}

pub(crate) fn pan_label(pan: f32) -> String {
    let value = pan.round() as i32;
    if value == 64 {
        String::from("C")
    } else if value < 64 {
        format!("L{}", 64 - value)
    } else {
        format!("R{}", value - 64)
    }
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
