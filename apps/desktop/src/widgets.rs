//! Widgets that reproduce the v1 JUCE drawing.
//!
//! Values and geometry come from `src/app/ui/HeaderBar.cpp` in the C++ app:
//! `LcdDisplay::paint` and `ViewTabs::paint`. The text formatting matches
//! `core/src/Time.cpp::formatBarBeat` and the header's own strings.

use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};
use music_core::{PPQ, Tick, TimeSignature};

use crate::theme;

/// LCD panel size, from `HeaderBar::resized`: 350 x 52.
pub const LCD_SIZE: Vec2 = Vec2::new(330.0, 48.0);
/// Cell widths, from `LcdDisplay::cell`.
const CELLS: [f32; 3] = [142.0, 116.0, 92.0];
/// View tab strip size, from `HeaderBar::resized`.
pub const TABS_SIZE: Vec2 = Vec2::new(280.0, 38.0);

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

/// Size of [`mixer_knob`]; the mixer uses it to keep empty knob slots the same height.
pub(crate) const MIXER_KNOB_SIZE: Vec2 = Vec2::new(44.0, 46.0);
/// Width of the fader column. It matches [`MIXER_KNOB_SIZE`] so the fader
/// cap sits on the same vertical axis as the knobs above it.
const FADER_COLUMN: f32 = MIXER_KNOB_SIZE.x;
/// Gap between the fader column and its level meter, and the meter width.
const FADER_METER_GAP: f32 = 4.0;
const FADER_METER_WIDTH: f32 = 6.0;
/// The fader cap, drawn like a brushed metal hardware cap.
const FADER_CAP: Vec2 = Vec2::new(28.0, 16.0);

/// A hardware-style channel fader with its level meter beside it.
///
/// The meter shares the fader slot's top and bottom, so the two line up.
/// `meter` is the current level, 0..=1. Returns true when the value changed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn mixer_fader(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    height: f32,
    color: egui::Color32,
    label: &str,
    default_value: f32,
    meter: f32,
) -> bool {
    let size = egui::vec2(FADER_COLUMN + FADER_METER_GAP + FADER_METER_WIDTH, height);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let response =
        response.on_hover_text(format!("{label}: drag (Shift fine), double-click to reset"));

    // The cap centre travels between these two lines, so the cap never leaves the widget.
    let center_x = rect.left() + FADER_COLUMN / 2.0;
    let travel_top = rect.top() + FADER_CAP.y / 2.0 + 1.0;
    let travel_bottom = rect.bottom() - FADER_CAP.y / 2.0 - 1.0;
    let travel = (travel_bottom - travel_top).max(1.0);
    let span = (max - min).max(0.0001);

    let mut changed = false;
    if response.double_clicked() {
        *value = default_value.clamp(min, max);
        changed = true;
    } else if response.dragged() {
        let delta = ui.input(|input| input.pointer.delta().y);
        if delta != 0.0 {
            let scale = if ui.input(|input| input.modifiers.shift) {
                0.25
            } else {
                1.0
            };
            // One pixel of drag moves the cap one pixel.
            *value = (*value - delta / travel * span * scale).clamp(min, max);
            changed = true;
        }
    }

    let painter = ui.painter_at(rect.expand(3.0));
    let ratio = ((*value - min) / span).clamp(0.0, 1.0);
    let cap_y = travel_bottom - ratio * travel;
    let slot_top = travel_top - 3.0;
    let slot_bottom = travel_bottom + 3.0;

    // Scale ticks on both sides of the slot; the default (unity) mark is longer.
    let tick = Color32::from_gray(if theme::light() { 120 } else { 110 }).gamma_multiply(0.7);
    for step in 0..=10 {
        let y = travel_bottom - travel * step as f32 / 10.0;
        let length = if step % 5 == 0 { 6.0 } else { 4.0 };
        for side in [-1.0_f32, 1.0] {
            let inner = center_x + side * 6.0;
            painter.line_segment(
                [Pos2::new(inner, y), Pos2::new(inner + side * length, y)],
                Stroke::new(1.0_f32, tick),
            );
        }
    }
    let unity = ((default_value - min) / span).clamp(0.0, 1.0);
    let unity_y = travel_bottom - unity * travel;
    for side in [-1.0_f32, 1.0] {
        let inner = center_x + side * 6.0;
        painter.line_segment(
            [
                Pos2::new(inner, unity_y),
                Pos2::new(inner + side * 9.0, unity_y),
            ],
            Stroke::new(1.0_f32, theme::text_muted()),
        );
    }

    // Recessed slot with the channel colour showing the level below the cap.
    let slot = Rect::from_min_max(
        Pos2::new(center_x - 2.5, slot_top),
        Pos2::new(center_x + 2.5, slot_bottom),
    );
    recessed(&painter, slot);
    let fill = Rect::from_min_max(
        Pos2::new(center_x - 1.0, cap_y),
        Pos2::new(center_x + 1.0, slot_bottom - 1.5),
    );
    painter.rect_filled(fill, 1.0, color.gamma_multiply(0.85));

    // Level meter, recessed, sharing the slot's top and bottom.
    let meter_rect = Rect::from_min_max(
        Pos2::new(rect.left() + FADER_COLUMN + FADER_METER_GAP, slot_top),
        Pos2::new(rect.right(), slot_bottom),
    );
    recessed(&painter, meter_rect);
    let level = meter.clamp(0.0, 1.0);
    if level > 0.0 {
        let inner = meter_rect.shrink(1.0);
        let lit = Rect::from_min_max(
            Pos2::new(inner.left(), inner.bottom() - inner.height() * level),
            inner.max,
        );
        painter.rect_filled(lit, 1.0, color);
    }

    fader_cap(
        &painter,
        Rect::from_center_size(Pos2::new(center_x, cap_y), FADER_CAP),
        color,
        response.hovered() || response.dragged(),
    );
    changed
}

/// A dark groove with an inner shadow along the top and a lit lower edge.
fn recessed(painter: &egui::Painter, rect: Rect) {
    let base = if theme::light() {
        Color32::from_rgb(0x4a, 0x46, 0x40)
    } else {
        Color32::from_rgb(0x05, 0x06, 0x07)
    };
    painter.rect_filled(rect, 2.0, base);
    painter.line_segment(
        [
            Pos2::new(rect.left() + 1.0, rect.top() + 0.5),
            Pos2::new(rect.right() - 1.0, rect.top() + 0.5),
        ],
        Stroke::new(1.0_f32, Color32::BLACK.gamma_multiply(0.6)),
    );
    painter.rect_stroke(
        rect.expand(0.5),
        2.5,
        Stroke::new(1.0_f32, Color32::WHITE.gamma_multiply(0.06)),
        StrokeKind::Outside,
    );
}

/// Brushed metal fader cap: drop shadow, a vertical metal gradient, bevelled
/// edges, grip ridges and a coloured index line through the middle.
fn fader_cap(painter: &egui::Painter, cap: Rect, color: Color32, hot: bool) {
    // Soft drop shadow, offset downwards as if lit from above.
    for (grow, alpha) in [(3.0, 0.10), (2.0, 0.16), (1.0, 0.24)] {
        painter.rect_filled(
            cap.translate(Vec2::new(0.0, 2.5)).expand(grow),
            3.0 + grow,
            Color32::BLACK.gamma_multiply(alpha),
        );
    }

    let lift = if hot { 0.08 } else { 0.0 };
    let stops = [
        (0.00, Color32::from_rgb(0xf2, 0xf3, 0xf5)),
        (0.18, Color32::from_rgb(0xcf, 0xd2, 0xd7)),
        (0.46, Color32::from_rgb(0xa8, 0xad, 0xb4)),
        (0.54, Color32::from_rgb(0x8c, 0x92, 0x9a)),
        (0.82, Color32::from_rgb(0xa3, 0xa8, 0xaf)),
        (1.00, Color32::from_rgb(0x6c, 0x72, 0x7a)),
    ]
    .map(|(at, colour)| (at, lighten(colour, lift)));
    painter.rect_filled(cap, 2.5, stops[0].1);
    vertical_gradient(painter, cap.shrink2(Vec2::new(0.0, 1.0)), &stops);

    // Fine horizontal brushing.
    let mut y = cap.top() + 2.0;
    while y < cap.bottom() - 1.0 {
        painter.line_segment(
            [
                Pos2::new(cap.left() + 1.0, y),
                Pos2::new(cap.right() - 1.0, y),
            ],
            Stroke::new(0.5_f32, Color32::WHITE.gamma_multiply(0.10)),
        );
        y += 1.5;
    }

    // Grip ridges: a dark groove with a highlight just below it.
    for offset in [-4.5_f32, 4.5] {
        let y = cap.center().y + offset;
        painter.line_segment(
            [
                Pos2::new(cap.left() + 2.0, y),
                Pos2::new(cap.right() - 2.0, y),
            ],
            Stroke::new(
                1.0_f32,
                Color32::from_rgb(0x4a, 0x4f, 0x56).gamma_multiply(0.8),
            ),
        );
        painter.line_segment(
            [
                Pos2::new(cap.left() + 2.0, y + 1.0),
                Pos2::new(cap.right() - 2.0, y + 1.0),
            ],
            Stroke::new(1.0_f32, Color32::WHITE.gamma_multiply(0.45)),
        );
    }

    // Index line in the channel colour.
    painter.line_segment(
        [
            Pos2::new(cap.left() + 1.5, cap.center().y),
            Pos2::new(cap.right() - 1.5, cap.center().y),
        ],
        Stroke::new(2.0_f32, color),
    );

    // Bevel: bright top edge, dark bottom edge, then a crisp outline.
    painter.line_segment(
        [
            Pos2::new(cap.left() + 2.0, cap.top() + 0.5),
            Pos2::new(cap.right() - 2.0, cap.top() + 0.5),
        ],
        Stroke::new(1.0_f32, Color32::WHITE.gamma_multiply(0.9)),
    );
    painter.line_segment(
        [
            Pos2::new(cap.left() + 2.0, cap.bottom() - 0.5),
            Pos2::new(cap.right() - 2.0, cap.bottom() - 0.5),
        ],
        Stroke::new(1.0_f32, Color32::from_rgb(0x3c, 0x40, 0x46)),
    );
    painter.rect_stroke(
        cap,
        2.5,
        Stroke::new(1.0_f32, Color32::from_rgb(0x2a, 0x2e, 0x33)),
        StrokeKind::Outside,
    );
}

/// Fills `rect` with a top-to-bottom gradient through `stops` (position 0..=1, colour).
fn vertical_gradient(painter: &egui::Painter, rect: Rect, stops: &[(f32, Color32)]) {
    let mut mesh = egui::Mesh::default();
    for (index, (at, colour)) in stops.iter().enumerate() {
        let y = rect.top() + rect.height() * at.clamp(0.0, 1.0);
        mesh.colored_vertex(Pos2::new(rect.left(), y), *colour);
        mesh.colored_vertex(Pos2::new(rect.right(), y), *colour);
        if index > 0 {
            let base = u32::try_from(index * 2).unwrap_or(0);
            mesh.add_triangle(base - 2, base - 1, base);
            mesh.add_triangle(base - 1, base, base + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

pub(crate) fn painter_strip(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 3.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 1.0, color);
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

/// Draws a machined metal knob: drop shadow, a dark skirt and a cap whose
/// shine turns with the angle, like light on a brushed aluminium cone.
fn metal_knob(painter: &egui::Painter, center: Pos2, radius: f32, enabled: bool) {
    for (grow, alpha) in [(3.0, 0.12), (1.5, 0.22)] {
        painter.circle_filled(
            center + Vec2::new(0.0, 2.5),
            radius + grow,
            Color32::BLACK.gamma_multiply(alpha),
        );
    }
    painter.circle_filled(center, radius, Color32::from_rgb(0x1e, 0x21, 0x25));
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.0_f32, Color32::from_rgb(0x0c, 0x0d, 0x0f)),
    );

    let cap = radius - 3.0;
    let (light, dark) = if enabled {
        (
            Color32::from_rgb(0xee, 0xf0, 0xf3),
            Color32::from_rgb(0x6a, 0x70, 0x78),
        )
    } else {
        (
            Color32::from_rgb(0x9a, 0x9e, 0xa4),
            Color32::from_rgb(0x55, 0x59, 0x5f),
        )
    };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, lerp_colour(dark, light, 0.55));
    let segments = 48_u32;
    for index in 0..segments {
        let angle = std::f32::consts::TAU * index as f32 / segments as f32;
        // Two highlights opposite each other (conical brushing), brighter at the top left.
        let cone = 0.5 + 0.5 * (2.0 * (angle + std::f32::consts::FRAC_PI_4)).cos();
        let top = 0.5 - 0.5 * (angle + std::f32::consts::FRAC_PI_4).sin();
        let shade = (0.65 * cone + 0.35 * top).clamp(0.0, 1.0);
        mesh.colored_vertex(
            center + Vec2::angled(angle) * cap,
            lerp_colour(dark, light, shade),
        );
        let next = (index + 1) % segments;
        mesh.add_triangle(0, index + 1, next + 1);
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.circle_stroke(
        center,
        cap,
        Stroke::new(1.0_f32, Color32::WHITE.gamma_multiply(0.35)),
    );
}

/// Mixes two colours: `t` = 0 gives `from`, 1 gives `to`.
fn lerp_colour(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix =
        |a: u8, b: u8| -> u8 { (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8 };
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

/// Background colours of the mixer: (panel top, panel bottom, strip top, strip bottom).
fn mixer_metal() -> [Color32; 4] {
    if theme::light() {
        [
            Color32::from_rgb(0xcf, 0xd2, 0xd6),
            Color32::from_rgb(0xb8, 0xbc, 0xc2),
            Color32::from_rgb(0xf1, 0xf2, 0xf4),
            Color32::from_rgb(0xd3, 0xd6, 0xdb),
        ]
    } else {
        [
            Color32::from_rgb(0x1f, 0x22, 0x26),
            Color32::from_rgb(0x12, 0x14, 0x17),
            Color32::from_rgb(0x3a, 0x3f, 0x46),
            Color32::from_rgb(0x23, 0x27, 0x2c),
        ]
    }
}

/// Fine horizontal lines with a fixed pseudo-random pattern, so the
/// brushing does not shimmer between frames.
fn brushing(painter: &egui::Painter, rect: Rect, strength: f32) {
    let mut y = rect.top() + 1.0;
    let mut seed = 0x9e37_79b9_u32;
    while y < rect.bottom() - 1.0 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let alpha = (seed % 100) as f32 / 100.0 * strength;
        let colour = if seed.is_multiple_of(2) {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        painter.line_segment(
            [
                Pos2::new(rect.left() + 1.0, y),
                Pos2::new(rect.right() - 1.0, y),
            ],
            Stroke::new(1.0_f32, colour.gamma_multiply(alpha)),
        );
        y += 2.0;
    }
}

/// Paints the brushed metal behind the mixer panel. Call it first in the
/// panel so the strips draw on top.
pub(crate) fn mixer_panel_background(ui: &egui::Ui) {
    let [top, bottom, _, _] = mixer_metal();
    let rect = ui.clip_rect();
    let painter = ui.painter();
    vertical_gradient(painter, rect, &[(0.0, top), (1.0, bottom)]);
    brushing(painter, rect, 0.035);
    // A lit top edge where the panel meets the timeline.
    painter.line_segment(
        [
            Pos2::new(rect.left(), rect.top() + 0.5),
            Pos2::new(rect.right(), rect.top() + 0.5),
        ],
        Stroke::new(1.0_f32, Color32::WHITE.gamma_multiply(0.10)),
    );
}

/// Inner margin of a [`mixer_strip`].
const STRIP_MARGIN: f32 = 6.0;
const STRIP_RADIUS: f32 = 6.0;

/// A raised metal channel strip with a drop shadow. `size` is the room for
/// the contents; the strip adds its own margin around it.
pub(crate) fn mixer_strip<R>(
    ui: &mut egui::Ui,
    size: Vec2,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    // Reserve the background layers now so they sit under the contents.
    let shadow_slot = ui.painter().add(egui::Shape::Noop);
    let body_slot = ui.painter().add(egui::Shape::Noop);
    let outer = size + Vec2::splat(STRIP_MARGIN * 2.0);
    let inner = ui.allocate_ui(outer, |ui| {
        egui::Frame::new()
            .inner_margin(STRIP_MARGIN)
            .show(ui, |ui| {
                ui.set_width(size.x);
                add_contents(ui)
            })
            .inner
    });
    let rect = inner.response.rect;
    let [_, _, top, bottom] = mixer_metal();
    let shadow = egui::Shadow {
        offset: [0, 4],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(if theme::light() { 70 } else { 150 }),
    };
    ui.painter()
        .set(shadow_slot, shadow.as_shape(rect, STRIP_RADIUS));

    let painter = ui.painter();
    let mut body = Vec::new();
    // Rounded ends in the end colours, a gradient between them.
    let cap_height = STRIP_RADIUS * 2.0;
    body.push(egui::Shape::rect_filled(
        Rect::from_min_size(rect.min, Vec2::new(rect.width(), cap_height)),
        STRIP_RADIUS,
        top,
    ));
    body.push(egui::Shape::rect_filled(
        Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - cap_height), rect.max),
        STRIP_RADIUS,
        bottom,
    ));
    let middle = rect.shrink2(Vec2::new(0.0, STRIP_RADIUS));
    let mut mesh = egui::Mesh::default();
    for (index, colour) in [top, bottom].into_iter().enumerate() {
        let y = if index == 0 {
            middle.top()
        } else {
            middle.bottom()
        };
        mesh.colored_vertex(Pos2::new(middle.left(), y), colour);
        mesh.colored_vertex(Pos2::new(middle.right(), y), colour);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    body.push(egui::Shape::mesh(mesh));
    painter.set(body_slot, egui::Shape::Vec(body));

    // Outline and a lit top edge for the bevel.
    painter.rect_stroke(
        rect,
        STRIP_RADIUS,
        Stroke::new(1.0_f32, Color32::BLACK.gamma_multiply(0.55)),
        StrokeKind::Outside,
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + STRIP_RADIUS, rect.top() + 0.5),
            Pos2::new(rect.right() - STRIP_RADIUS, rect.top() + 0.5),
        ],
        Stroke::new(1.0_f32, Color32::WHITE.gamma_multiply(0.22)),
    );
    inner
}

pub(crate) fn mixer_knob(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    label: &str,
    enabled: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(MIXER_KNOB_SIZE, egui::Sense::click_and_drag());
    if enabled && response.dragged() {
        let delta = ui.input(|input| input.pointer.delta().y);
        *value = (*value - delta / 90.0 * (max - min)).clamp(min, max);
    }
    let changed = enabled && response.dragged() && ui.input(|input| input.pointer.delta().y != 0.0);
    let center = egui::pos2(rect.center().x, rect.top() + 15.0);
    let radius = 13.0;
    metal_knob(ui.painter(), center, radius, enabled);
    let ratio = ((*value - min) / (max - min).max(0.0001)).clamp(0.0, 1.0);
    let angle = (-std::f32::consts::FRAC_PI_2 - 2.356) + ratio * 4.712;
    let end = egui::pos2(
        center.x + angle.cos() * (radius - 3.0),
        center.y + angle.sin() * (radius - 3.0),
    );
    ui.painter().line_segment(
        [center, end],
        egui::Stroke::new(
            2.0_f32,
            if enabled {
                theme::accent()
            } else {
                theme::text_dim()
            },
        ),
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 6.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(9.0),
        theme::text_muted(),
    );
    let response = if enabled {
        response.on_hover_text("Drag up or down")
    } else {
        response.on_hover_text("Chorus send is not routed")
    };
    let _ = response;
    changed
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
