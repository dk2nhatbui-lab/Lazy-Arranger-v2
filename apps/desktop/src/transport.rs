//! Transport helpers: loop regions, wrap arithmetic and the transport
//! buttons.

use egui::Rect;
use music_core::Tick;

use crate::theme;

/// The bar containing `position`, as a start and end tick.
#[must_use]
pub fn bar_region(position: Tick, bar: Tick) -> (Tick, Tick) {
    let bar = bar.max(1);
    let start = position.div_euclid(bar) * bar;
    (start, start + bar)
}

/// Where playback should be after `position`: the loop start when it has
/// reached the end, otherwise the position itself.
#[must_use]
pub fn wrap(position: Tick, region: (Tick, Tick)) -> Tick {
    let (start, end) = region;
    if end <= start || position < end {
        position
    } else {
        start + (position - start) % (end - start)
    }
}

pub(crate) fn draw_metronome_icon(painter: &egui::Painter, rect: Rect, color: egui::Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let stroke = egui::Stroke::new((s * 0.07).max(1.0), color);
    let body = vec![
        egui::pos2(c.x - s * 0.08, c.y - s * 0.38),
        egui::pos2(c.x + s * 0.08, c.y - s * 0.38),
        egui::pos2(c.x + s * 0.30, c.y + s * 0.36),
        egui::pos2(c.x - s * 0.30, c.y + s * 0.36),
    ];
    painter.add(egui::Shape::closed_line(body, stroke));
    painter.line_segment(
        [
            egui::pos2(c.x, c.y + s * 0.20),
            egui::pos2(c.x + s * 0.22, c.y - s * 0.30),
        ],
        stroke,
    );
}

/// The v1 header button: a raised square with a `lineStrong` outline
/// (`IconButton::paintButton`). Returns the response so callers can add
/// their own tooltip.
fn icon_button_frame(
    ui: &mut egui::Ui,
    rect: Rect,
    id: &str,
    active: bool,
    on_colour: egui::Color32,
) -> egui::Response {
    let response = ui.interact(rect, ui.id().with(id), egui::Sense::click());
    let raised = theme::raised();
    let mut fill = if active {
        blend(raised, on_colour, 0.22)
    } else {
        raised
    };
    if response.is_pointer_button_down_on() {
        fill = blend(fill, egui::Color32::BLACK, 0.2);
    } else if response.hovered() {
        fill = blend(fill, egui::Color32::WHITE, 0.08);
    }
    let r = rect.shrink(0.5);
    ui.painter().rect_filled(r, theme::RADIUS, fill);
    ui.painter().rect_stroke(
        r,
        theme::RADIUS,
        egui::Stroke::new(
            1.0_f32,
            if active {
                on_colour.gamma_multiply(0.65)
            } else {
                theme::line_strong()
            },
        ),
        egui::StrokeKind::Inside,
    );
    response
}

/// Mixes `amount` of `over` into `base`, ignoring alpha.
pub(crate) fn blend(base: egui::Color32, over: egui::Color32, amount: f32) -> egui::Color32 {
    let mix = |a: u8, b: u8| -> u8 {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8
    };
    egui::Color32::from_rgb(
        mix(base.r(), over.r()),
        mix(base.g(), over.g()),
        mix(base.b(), over.b()),
    )
}

pub(crate) fn metronome_button(ui: &mut egui::Ui, rect: Rect, active: bool) -> bool {
    let response = icon_button_frame(ui, rect, "metronome-button", active, theme::accent());
    let colour = if active {
        theme::accent()
    } else {
        theme::text_muted()
    };
    draw_metronome_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), egui::vec2(18.0, 18.0)),
        colour,
    );
    response.on_hover_text("Metronome on/off").clicked()
}

/// Draws a transport glyph inside `rect`, shaped like `icons::draw` in v1.
pub(crate) fn draw_transport_icon(
    painter: &egui::Painter,
    rect: Rect,
    glyph: &str,
    color: egui::Color32,
) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    match glyph {
        STOP => {
            painter.rect_filled(
                Rect::from_center_size(c, egui::vec2(s * 0.56, s * 0.56)),
                s * 0.08,
                color,
            );
        }
        PLAY => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - s * 0.28, c.y - s * 0.34),
                    egui::pos2(c.x + s * 0.36, c.y),
                    egui::pos2(c.x - s * 0.28, c.y + s * 0.34),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        RECORD => {
            painter.circle_filled(c, s * 0.30, color);
        }
        LOOP => {
            let w = s * 0.62;
            let h = s * 0.42;
            let stroke = egui::Stroke::new((s * 0.09).max(1.2), color);
            let arc_centre = egui::pos2(c.x + w / 2.0 - h / 2.0, c.y);
            let mut points = vec![
                egui::pos2(c.x - w / 2.0, c.y + h / 2.0),
                egui::pos2(arc_centre.x, c.y + h / 2.0),
            ];
            for index in 1..=12 {
                let angle = std::f32::consts::PI * index as f32 / 12.0;
                points.push(egui::pos2(
                    arc_centre.x + h / 2.0 * angle.sin(),
                    arc_centre.y + h / 2.0 * angle.cos(),
                ));
            }
            points.push(egui::pos2(c.x - w / 2.0 + 2.0, c.y - h / 2.0));
            painter.add(egui::Shape::line(points, stroke));
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - w / 2.0 - s * 0.02, c.y - h / 2.0),
                    egui::pos2(c.x - w / 2.0 + s * 0.16, c.y - h / 2.0 - s * 0.12),
                    egui::pos2(c.x - w / 2.0 + s * 0.16, c.y - h / 2.0 + s * 0.12),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        _ => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                glyph,
                egui::FontId::proportional(s * 0.6),
                color,
            );
        }
    }
}

/// The v1 AI mark: a big four-point star with a small one at its top right.
pub(crate) fn draw_sparkle(painter: &egui::Painter, rect: Rect, color: egui::Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let star = |centre: egui::Pos2, r: f32| {
        let k = r * 0.22;
        let corners = [
            (
                egui::pos2(centre.x, centre.y - r),
                egui::pos2(centre.x + k, centre.y - k),
            ),
            (
                egui::pos2(centre.x + r, centre.y),
                egui::pos2(centre.x + k, centre.y + k),
            ),
            (
                egui::pos2(centre.x, centre.y + r),
                egui::pos2(centre.x - k, centre.y + k),
            ),
            (
                egui::pos2(centre.x - r, centre.y),
                egui::pos2(centre.x - k, centre.y - k),
            ),
        ];
        let mut points = Vec::with_capacity(4 * 8);
        for (index, (start, control)) in corners.iter().enumerate() {
            let end = corners[(index + 1) % 4].0;
            for step in 0..8 {
                let t = step as f32 / 8.0;
                let one = 1.0 - t;
                points.push(egui::pos2(
                    one * one * start.x + 2.0 * one * t * control.x + t * t * end.x,
                    one * one * start.y + 2.0 * one * t * control.y + t * t * end.y,
                ));
            }
        }
        // The star is concave but star-shaped around its centre: fill it as
        // one triangle fan mesh, so no seams show between the triangles.
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(centre, color);
        for point in &points {
            mesh.colored_vertex(*point, color);
        }
        let count = points.len() as u32;
        for index in 0..count {
            mesh.add_triangle(0, 1 + index, 1 + (index + 1) % count);
        }
        painter.add(mesh);
    };
    star(egui::pos2(c.x - s * 0.08, c.y + s * 0.08), s * 0.4);
    star(egui::pos2(c.x + s * 0.3, c.y - s * 0.3), s * 0.17);
}

/// Stop glyph.
pub(crate) const STOP: &str = "\u{25A0}";
/// Play glyph.
pub(crate) const PLAY: &str = "\u{25B6}";
/// Record glyph.
pub(crate) const RECORD: &str = "\u{25CF}";
/// Loop glyph.
pub(crate) const LOOP: &str = "\u{27F2}";

/// A v1 header transport button: stop, play, record or loop. Play keeps its
/// triangle while playing and lights up instead, as in v1.
pub(crate) fn transport_button(ui: &mut egui::Ui, rect: Rect, glyph: &str, active: bool) -> bool {
    let on_colour = match glyph {
        PLAY => theme::playing(),
        RECORD => theme::recording(),
        _ => theme::accent(),
    };
    let response = icon_button_frame(ui, rect, glyph, active, on_colour);
    let icon_colour = match glyph {
        RECORD => theme::recording(),
        PLAY => theme::playing(),
        LOOP if !active => theme::text_muted(),
        _ if active => on_colour,
        _ => theme::text(),
    };
    let icon_size = if glyph == PLAY { 24.0 } else { 18.0 };
    draw_transport_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), egui::vec2(icon_size, icon_size)),
        glyph,
        icon_colour,
    );
    let hint = match glyph {
        STOP => "Stop. Press twice to go back to the start",
        PLAY => "Play / stop",
        RECORD => "Record on the armed (or selected) track",
        LOOP => "Loop between the locators. Drag in the ruler to set them.",
        _ => "Transport",
    };
    response.on_hover_text(hint).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_region_starts_on_the_bar() {
        assert_eq!(bar_region(5000, 3840), (3840, 7680));
        assert_eq!(bar_region(0, 3840), (0, 3840));
        assert_eq!(bar_region(-1, 3840), (-3840, 0));
    }

    #[test]
    fn wrapping_returns_the_loop_start_or_the_position() {
        let region = (100, 400);
        assert_eq!(wrap(50, region), 50);
        assert_eq!(wrap(400, region), 100);
        assert_eq!(wrap(550, region), 250);
        assert_eq!(wrap(100, (100, 100)), 100, "empty region does not loop");
    }
}
