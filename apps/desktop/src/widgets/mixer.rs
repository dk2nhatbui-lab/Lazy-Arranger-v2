//! Mixer console drawing: brushed grey channel strips, chrome fader caps with
//! a dB scale, a recessed meter well with a value readout, black knobs, lit
//! S/M/R buttons and a coloured name tab.
//!
//! The look takes its cues from hardware-style console mixers (mid-grey
//! panels, chrome caps, black wells). All of it is drawn here; no artwork is
//! copied.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::lighten;
use crate::theme;

/// Size of [`mixer_knob`]; the mixer uses it to keep empty knob rows the same height.
pub(crate) const MIXER_KNOB_SIZE: Vec2 = Vec2::new(44.0, 46.0);
/// The chrome fader cap: taller than wide, like a long-throw console fader.
const FADER_CAP: Vec2 = Vec2::new(22.0, 32.0);
/// Distance from the strip's left edge to the centre of the fader slot.
const SLOT_X: f32 = 14.0;
/// Width of the meter well on the right of the fader.
const WELL_WIDTH: f32 = 22.0;
/// Height of the numeric readout at the bottom of the meter well.
const READOUT_HEIGHT: f32 = 14.0;
/// dB marks drawn on the fader scale, when they fall inside its range.
const SCALE_MARKS: [f32; 10] = [
    24.0, 12.0, 6.0, 0.0, -6.0, -12.0, -24.0, -48.0, -72.0, -96.0,
];

/// Strip colours: (panel top, panel bottom, strip top, strip bottom).
fn palette() -> [Color32; 4] {
    if theme::light() {
        [
            Color32::from_rgb(0x8c, 0x8f, 0x93),
            Color32::from_rgb(0x7a, 0x7d, 0x81),
            Color32::from_rgb(0xbe, 0xc1, 0xc5),
            Color32::from_rgb(0xa6, 0xa9, 0xad),
        ]
    } else {
        [
            Color32::from_rgb(0x2f, 0x32, 0x36),
            Color32::from_rgb(0x22, 0x24, 0x27),
            Color32::from_rgb(0xa0, 0xa3, 0xa7),
            Color32::from_rgb(0x87, 0x8a, 0x8e),
        ]
    }
}

/// Text on a grey strip.
const STRIP_INK: Color32 = Color32::from_rgb(0x17, 0x19, 0x1c);
/// Secondary text and scale marks on a grey strip.
const STRIP_INK_DIM: Color32 = Color32::from_rgb(0x2f, 0x32, 0x36);
/// Black wells, slots and knobs.
const WELL: Color32 = Color32::from_rgb(0x14, 0x15, 0x17);

/// Mixes two colours: `t` = 0 gives `from`, 1 gives `to`.
fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let channel =
        |a: u8, b: u8| -> u8 { (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8 };
    Color32::from_rgba_unmultiplied(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
        channel(from.a(), to.a()),
    )
}

/// Fills `rect` with a top-to-bottom gradient through `stops` (position 0..=1, colour).
fn vertical_gradient(painter: &egui::Painter, rect: Rect, stops: &[(f32, Color32)]) {
    painter.add(vertical_gradient_shape(rect, stops));
}

fn vertical_gradient_shape(rect: Rect, stops: &[(f32, Color32)]) -> egui::Shape {
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
    egui::Shape::mesh(mesh)
}

/// Fills `rect` with a left-to-right gradient through `stops`.
fn horizontal_gradient(painter: &egui::Painter, rect: Rect, stops: &[(f32, Color32)]) {
    painter.add(horizontal_gradient_shape(rect, stops));
}

fn horizontal_gradient_shape(rect: Rect, stops: &[(f32, Color32)]) -> egui::Shape {
    let mut mesh = egui::Mesh::default();
    for (index, (at, colour)) in stops.iter().enumerate() {
        let x = rect.left() + rect.width() * at.clamp(0.0, 1.0);
        mesh.colored_vertex(Pos2::new(x, rect.top()), *colour);
        mesh.colored_vertex(Pos2::new(x, rect.bottom()), *colour);
        if index > 0 {
            let base = u32::try_from(index * 2).unwrap_or(0);
            mesh.add_triangle(base - 2, base - 1, base);
            mesh.add_triangle(base - 1, base, base + 1);
        }
    }
    egui::Shape::mesh(mesh)
}

/// Fine horizontal lines in a fixed pseudo-random pattern, so the brushing
/// does not shimmer between frames.
fn brushing(rect: Rect, strength: f32, seed: u32) -> Vec<egui::Shape> {
    let mut lines = Vec::new();
    let mut y = rect.top() + 1.0;
    let mut state = seed | 1;
    while y < rect.bottom() - 1.0 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let alpha = (state % 100) as f32 / 100.0 * strength;
        let colour = if state.is_multiple_of(2) {
            Color32::WHITE
        } else {
            Color32::BLACK
        };
        lines.push(egui::Shape::line_segment(
            [
                Pos2::new(rect.left() + 1.0, y),
                Pos2::new(rect.right() - 1.0, y),
            ],
            Stroke::new(1.0_f32, colour.gamma_multiply(alpha)),
        ));
        y += 2.0;
    }
    lines
}

/// A black recessed well: dark fill, shadow along the top, a lit lower rim.
fn well(painter: &egui::Painter, rect: Rect, radius: f32) {
    painter.rect_filled(rect, radius, WELL);
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0_f32, Color32::BLACK.gamma_multiply(0.7)),
        StrokeKind::Inside,
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + radius, rect.bottom() + 0.5),
            Pos2::new(rect.right() - radius, rect.bottom() + 0.5),
        ],
        Stroke::new(1.0_f32, Color32::WHITE.gamma_multiply(0.35)),
    );
}

/// Paints the dark brushed back panel behind the strips. Call it first in the
/// panel so the strips draw on top.
pub(crate) fn mixer_panel_background(ui: &Ui) {
    let [top, bottom, _, _] = palette();
    let rect = ui.clip_rect();
    let painter = ui.painter();
    vertical_gradient(painter, rect, &[(0.0, top), (1.0, bottom)]);
    painter.extend(brushing(rect, 0.035, 0x9e37_79b9));
}

/// Inner margin of a [`mixer_strip`].
const STRIP_MARGIN: f32 = 6.0;
const STRIP_RADIUS: f32 = 3.0;

/// A raised grey brushed-metal channel strip with a drop shadow. `size` is
/// the room for the contents; the strip adds its margin around it. Text in
/// the strip is dark so it reads on the grey.
pub(crate) fn mixer_strip<R>(
    ui: &mut Ui,
    size: Vec2,
    add_contents: impl FnOnce(&mut Ui) -> R,
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
                ui.visuals_mut().override_text_color = Some(STRIP_INK);
                add_contents(ui)
            })
            .inner
    });
    let rect = inner.response.rect;
    let [_, _, top, bottom] = palette();
    let shadow = egui::Shadow {
        offset: [2, 4],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(if theme::light() { 90 } else { 170 }),
    };
    ui.painter()
        .set(shadow_slot, shadow.as_shape(rect, STRIP_RADIUS));

    // The body: rounded ends in the end colours, a gradient between them,
    // horizontal brushing and darker side edges for a slight curve.
    let mut body = Vec::new();
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
    body.push(vertical_gradient_shape(
        middle,
        &[(0.0, top), (1.0, bottom)],
    ));
    body.push(horizontal_gradient_shape(
        rect,
        &[
            (0.0, Color32::BLACK.gamma_multiply(0.16)),
            (0.08, Color32::TRANSPARENT),
            (0.92, Color32::TRANSPARENT),
            (1.0, Color32::BLACK.gamma_multiply(0.2)),
        ],
    ));
    let seed = (rect.left().to_bits() ^ 0x85eb_ca6b) | 1;
    body.extend(brushing(rect, 0.07, seed));
    ui.painter().set(body_slot, egui::Shape::Vec(body));

    let painter = ui.painter();
    painter.rect_stroke(
        rect,
        STRIP_RADIUS,
        Stroke::new(1.0_f32, Color32::BLACK.gamma_multiply(0.6)),
        StrokeKind::Outside,
    );
    painter.line_segment(
        [
            Pos2::new(rect.left() + STRIP_RADIUS, rect.top() + 0.5),
            Pos2::new(rect.right() - STRIP_RADIUS, rect.top() + 0.5),
        ],
        Stroke::new(1.0_f32, Color32::WHITE.gamma_multiply(0.45)),
    );
    inner
}

/// A small caption in the strip's secondary ink.
pub(crate) fn strip_caption(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(9.0).color(STRIP_INK_DIM));
}

/// A bevelled light-grey console button. When `on`, its lamp lights in
/// `lamp`. Returns true when clicked.
pub(crate) fn strip_button(ui: &mut Ui, text: &str, on: bool, lamp: Color32, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(26.0, 18.0), Sense::click());
    let response = response.on_hover_text(hint);
    let painter = ui.painter();
    painter.rect_filled(
        rect.translate(Vec2::new(0.0, 1.5)),
        3.0,
        Color32::BLACK.gamma_multiply(0.35),
    );
    let (face_top, face_bottom) = if on {
        (lighten(lamp, 0.35), lamp)
    } else if response.hovered() {
        (
            Color32::from_rgb(0xf4, 0xf5, 0xf6),
            Color32::from_rgb(0xcf, 0xd1, 0xd4),
        )
    } else {
        (
            Color32::from_rgb(0xe4, 0xe5, 0xe7),
            Color32::from_rgb(0xbb, 0xbe, 0xc2),
        )
    };
    let face = if response.is_pointer_button_down_on() {
        rect.translate(Vec2::new(0.0, 1.0))
    } else {
        rect
    };
    painter.rect_filled(face, 3.0, face_bottom);
    vertical_gradient(
        painter,
        face.shrink2(Vec2::new(1.0, 2.0)),
        &[(0.0, face_top), (1.0, face_bottom)],
    );
    painter.rect_stroke(
        face,
        3.0,
        Stroke::new(1.0_f32, Color32::from_rgb(0x2a, 0x2c, 0x30)),
        StrokeKind::Inside,
    );
    painter.line_segment(
        [
            Pos2::new(face.left() + 3.0, face.top() + 1.5),
            Pos2::new(face.right() - 3.0, face.top() + 1.5),
        ],
        Stroke::new(1.0_f32, Color32::WHITE.gamma_multiply(0.8)),
    );
    painter.text(
        face.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(10.0),
        STRIP_INK,
    );
    response.clicked()
}

/// The coloured name tab at the foot of a strip.
pub(crate) fn name_tab(ui: &mut Ui, name: &str, colour: Color32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, colour);
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0_f32, Color32::BLACK.gamma_multiply(0.45)),
        StrokeKind::Inside,
    );
    let luminance = 0.299 * f32::from(colour.r())
        + 0.587 * f32::from(colour.g())
        + 0.114 * f32::from(colour.b());
    let ink = if luminance > 140.0 {
        STRIP_INK
    } else {
        Color32::WHITE
    };
    let galley = painter.layout(
        name.to_owned(),
        FontId::proportional(10.5),
        ink,
        rect.width() - 6.0,
    );
    let pos = Pos2::new(
        rect.center().x - galley.size().x / 2.0,
        rect.center().y - galley.size().y / 2.0,
    );
    painter.with_clip_rect(rect).galley(pos, galley, ink);
}

/// A console channel fader filling the strip's width: black slot with a dB
/// scale, a chrome cap, and a meter well on the right with the value readout
/// at its foot. The scale and readout show `value - default_value` in dB, so
/// the default sits at 0.
///
/// `meter` is the current level, 0..=1. Returns true when the value changed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn mixer_fader(
    ui: &mut Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    height: f32,
    color: Color32,
    label: &str,
    default_value: f32,
    meter: f32,
) -> bool {
    let width = ui.available_width().max(64.0);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let response =
        response.on_hover_text(format!("{label}: drag (Shift fine), double-click to reset"));

    let center_x = rect.left() + SLOT_X;
    let travel_top = rect.top() + FADER_CAP.y / 2.0;
    let travel_bottom = rect.bottom() - FADER_CAP.y / 2.0;
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

    let painter = ui.painter_at(rect.expand(4.0));
    let y_of = |v: f32| travel_bottom - ((v - min) / span).clamp(0.0, 1.0) * travel;

    // Slot: a thin black groove the length of the travel.
    let slot = Rect::from_min_max(
        Pos2::new(center_x - 2.5, travel_top - 4.0),
        Pos2::new(center_x + 2.5, travel_bottom + 4.0),
    );
    well(&painter, slot, 2.5);
    let fill = Rect::from_min_max(
        Pos2::new(center_x - 1.0, y_of(*value)),
        Pos2::new(center_x + 1.0, slot.bottom() - 1.5),
    );
    painter.rect_filled(fill, 1.0, color.gamma_multiply(0.9));

    // dB scale right of the slot. The 0 dB and the bottom (infinity) marks
    // always show; others are skipped when they would crowd a shown mark.
    let well_left = rect.right() - WELL_WIDTH;
    let tick_left = center_x + 5.0;
    let mut marks: Vec<(f32, String)> = vec![
        (default_value.clamp(min, max), String::from("0")),
        (min, String::from("\u{221e}")),
    ];
    marks.extend(
        SCALE_MARKS
            .iter()
            .filter(|db| **db != 0.0)
            .map(|db| (default_value + db, *db))
            .filter(|(v, _)| *v > min && *v <= max)
            .map(|(v, db)| (v, format!("{}", db.abs() as i32))),
    );
    let mut shown: Vec<f32> = Vec::new();
    for (v, text) in marks {
        let y = y_of(v);
        if shown.iter().any(|other| (y - other).abs() < 12.0) {
            continue;
        }
        shown.push(y);
        let unity = (v - default_value).abs() < 0.01;
        painter.line_segment(
            [
                Pos2::new(tick_left, y),
                Pos2::new(tick_left + if unity { 6.0 } else { 3.5 }, y),
            ],
            Stroke::new(1.0_f32, STRIP_INK_DIM),
        );
        if tick_left + 8.0 < well_left - 8.0 {
            painter.text(
                Pos2::new(tick_left + 8.0, y),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(if unity { 9.0 } else { 8.0 }),
                if unity { STRIP_INK } else { STRIP_INK_DIM },
            );
        }
    }

    // Meter well with an LED-style bar and the value readout at its foot.
    let well_rect = Rect::from_min_max(Pos2::new(well_left, rect.top()), rect.max);
    well(&painter, well_rect, 3.0);
    let bar = Rect::from_min_max(
        Pos2::new(well_rect.left() + 4.0, well_rect.top() + 4.0),
        Pos2::new(
            well_rect.right() - 4.0,
            well_rect.bottom() - READOUT_HEIGHT - 3.0,
        ),
    );
    let stops = [
        (0.0, Color32::from_rgb(0xe8, 0x3b, 0x2e)),
        (0.12, Color32::from_rgb(0xf2, 0xc1, 0x2e)),
        (0.3, Color32::from_rgb(0x7d, 0xd3, 0x4a)),
        (1.0, Color32::from_rgb(0x3c, 0xb0, 0x46)),
    ];
    let dim: Vec<(f32, Color32)> = stops
        .iter()
        .map(|(at, colour)| (*at, mix(WELL, *colour, 0.16)))
        .collect();
    vertical_gradient(&painter, bar, &dim);
    let level = meter.clamp(0.0, 1.0);
    if level > 0.0 {
        let lit = Rect::from_min_max(
            Pos2::new(bar.left(), bar.bottom() - bar.height() * level),
            bar.max,
        );
        vertical_gradient(&painter.with_clip_rect(lit), bar, &stops);
    }
    // LED segment gaps.
    let mut y = bar.top() + 3.0;
    while y < bar.bottom() {
        painter.line_segment(
            [Pos2::new(bar.left(), y), Pos2::new(bar.right(), y)],
            Stroke::new(1.0_f32, WELL.gamma_multiply(0.85)),
        );
        y += 3.0;
    }
    let readout = Rect::from_min_max(
        Pos2::new(well_rect.left(), well_rect.bottom() - READOUT_HEIGHT),
        well_rect.max,
    );
    painter.text(
        readout.center(),
        Align2::CENTER_CENTER,
        format!("{:.1}", *value - default_value),
        FontId::proportional(8.0),
        Color32::from_rgb(0xe6, 0xe8, 0xea),
    );

    chrome_cap(
        &painter,
        Rect::from_center_size(Pos2::new(center_x, y_of(*value)), FADER_CAP),
        response.hovered() || response.dragged(),
    );
    changed
}

/// A chrome fader cap: soft drop shadow, a polished vertical sheen with a
/// dark waist, darker sides for roundness and a black index line.
fn chrome_cap(painter: &egui::Painter, cap: Rect, hot: bool) {
    for (grow, alpha) in [(4.0, 0.08), (2.5, 0.14), (1.0, 0.22)] {
        painter.rect_filled(
            cap.translate(Vec2::new(1.5, 3.0)).expand(grow),
            3.0 + grow,
            Color32::BLACK.gamma_multiply(alpha),
        );
    }
    let lift = if hot { 0.12 } else { 0.0 };
    let stops = [
        (0.00, Color32::from_rgb(0xfb, 0xfb, 0xfc)),
        (0.22, Color32::from_rgb(0xd2, 0xd3, 0xd6)),
        (0.40, Color32::from_rgb(0x8a, 0x8c, 0x91)),
        (0.47, Color32::from_rgb(0xf4, 0xf4, 0xf6)),
        (0.53, Color32::from_rgb(0x9a, 0x9c, 0xa1)),
        (0.66, Color32::from_rgb(0xe2, 0xe3, 0xe5)),
        (0.90, Color32::from_rgb(0xfa, 0xfa, 0xfb)),
        (1.00, Color32::from_rgb(0xa9, 0xab, 0xaf)),
    ]
    .map(|(at, colour)| (at, lighten(colour, lift)));
    painter.rect_filled(cap, 3.0, stops[0].1);
    vertical_gradient(painter, cap.shrink2(Vec2::new(0.0, 1.5)), &stops);
    horizontal_gradient(
        painter,
        cap,
        &[
            (0.0, Color32::BLACK.gamma_multiply(0.35)),
            (0.22, Color32::TRANSPARENT),
            (0.78, Color32::TRANSPARENT),
            (1.0, Color32::BLACK.gamma_multiply(0.4)),
        ],
    );
    // Index line.
    painter.line_segment(
        [
            Pos2::new(cap.left() + 1.0, cap.center().y),
            Pos2::new(cap.right() - 1.0, cap.center().y),
        ],
        Stroke::new(2.0_f32, Color32::from_rgb(0x1d, 0x1e, 0x21)),
    );
    painter.rect_stroke(
        cap,
        3.0,
        Stroke::new(1.0_f32, Color32::from_rgb(0x3a, 0x3c, 0x40)),
        StrokeKind::Outside,
    );
}

/// A black console knob with a soft sheen, a white pointer and range dots.
fn black_knob(painter: &egui::Painter, center: Pos2, radius: f32, ratio: f32, enabled: bool) {
    let start = -std::f32::consts::FRAC_PI_2 - 2.356;
    let sweep = 4.712;
    // Range dots around the knob.
    for step in 0..=4 {
        let angle = start + sweep * step as f32 / 4.0;
        painter.circle_filled(
            center + Vec2::angled(angle) * (radius + 3.5),
            1.0,
            STRIP_INK,
        );
    }
    painter.circle_filled(
        center + Vec2::new(1.0, 2.5),
        radius + 1.0,
        Color32::BLACK.gamma_multiply(0.3),
    );
    // Body: a fan whose rim catches light at the top left.
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, Color32::from_rgb(0x2c, 0x2e, 0x31));
    let segments = 40_u32;
    for index in 0..segments {
        let angle = std::f32::consts::TAU * index as f32 / segments as f32;
        let light = 0.5 - 0.5 * (angle + std::f32::consts::FRAC_PI_4).sin();
        mesh.colored_vertex(
            center + Vec2::angled(angle) * radius,
            mix(
                Color32::from_rgb(0x0b, 0x0c, 0x0d),
                Color32::from_rgb(0x5a, 0x5d, 0x62),
                light * light,
            ),
        );
        let next = (index + 1) % segments;
        mesh.add_triangle(0, index + 1, next + 1);
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.0_f32, Color32::from_rgb(0x08, 0x08, 0x09)),
    );
    let angle = start + ratio * sweep;
    painter.line_segment(
        [
            center + Vec2::angled(angle) * (radius * 0.25),
            center + Vec2::angled(angle) * (radius - 2.0),
        ],
        Stroke::new(
            2.0_f32,
            if enabled {
                Color32::from_rgb(0xf4, 0xf4, 0xf4)
            } else {
                Color32::from_gray(0x70)
            },
        ),
    );
}

/// A rotary knob with its value label underneath. Returns true when dragged.
pub(crate) fn mixer_knob(
    ui: &mut Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    label: &str,
    enabled: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(MIXER_KNOB_SIZE, Sense::click_and_drag());
    if enabled && response.dragged() {
        let delta = ui.input(|input| input.pointer.delta().y);
        *value = (*value - delta / 90.0 * (max - min)).clamp(min, max);
    }
    let changed = enabled && response.dragged() && ui.input(|input| input.pointer.delta().y != 0.0);
    let center = Pos2::new(rect.center().x, rect.top() + 16.0);
    let ratio = ((*value - min) / (max - min).max(0.0001)).clamp(0.0, 1.0);
    black_knob(ui.painter(), center, 11.5, ratio, enabled);
    ui.painter().text(
        Pos2::new(rect.center().x, rect.bottom() - 5.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(9.0),
        STRIP_INK_DIM,
    );
    let _ = if enabled {
        response.on_hover_text("Drag up or down")
    } else {
        response.on_hover_text("Chorus send is not routed")
    };
    changed
}
