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

pub(crate) fn metronome_button(ui: &mut egui::Ui, rect: Rect, active: bool) -> bool {
    let response = ui.interact(rect, ui.id().with("metronome-button"), egui::Sense::click());
    let fill = if active {
        theme::accent().gamma_multiply(0.25)
    } else if response.hovered() {
        theme::line()
    } else {
        theme::raised()
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(
            1.0_f32,
            if active {
                theme::accent()
            } else {
                theme::line()
            },
        ),
        egui::StrokeKind::Inside,
    );
    draw_metronome_icon(
        ui.painter(),
        rect.shrink(6.0),
        if active {
            theme::accent()
        } else {
            theme::text()
        },
    );
    response.on_hover_text("Metronome").clicked()
}

pub(crate) fn draw_transport_icon(
    painter: &egui::Painter,
    rect: Rect,
    glyph: &str,
    color: egui::Color32,
) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    match glyph {
        "\u{25A0}" => {
            painter.rect_filled(
                Rect::from_center_size(c, egui::vec2(s * 0.42, s * 0.42)),
                2.0,
                color,
            );
        }
        "\u{25B6}" => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - s * 0.22, c.y - s * 0.30),
                    egui::pos2(c.x - s * 0.22, c.y + s * 0.30),
                    egui::pos2(c.x + s * 0.30, c.y),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        "\u{2016}" => {
            for offset in [-1.0_f32, 1.0] {
                painter.rect_filled(
                    Rect::from_center_size(
                        egui::pos2(c.x + offset * s * 0.13, c.y),
                        egui::vec2(s * 0.12, s * 0.44),
                    ),
                    1.0,
                    color,
                );
            }
        }
        "\u{25CF}" => {
            painter.circle_filled(c, s * 0.28, color);
        }
        "\u{21B6}" => {
            let stroke = egui::Stroke::new((s * 0.09).max(1.2), color);
            let tip = egui::pos2(c.x - s * 0.30, c.y + s * 0.28);
            let base = egui::pos2(tip.x + s * 0.14, tip.y - s * 0.14);
            let barb = s * 0.085;
            painter.line_segment([tip, egui::pos2(base.x + barb, base.y + barb)], stroke);
            painter.line_segment([tip, egui::pos2(base.x - barb, base.y - barb)], stroke);
            let control = egui::pos2(c.x + s * 0.22, c.y - s * 0.16);
            let end = egui::pos2(c.x + s * 0.12, c.y - s * 0.36);
            let points: Vec<egui::Pos2> = (0..=16)
                .map(|index| {
                    let t = index as f32 / 16.0;
                    let one = 1.0 - t;
                    egui::pos2(
                        one * one * base.x + 2.0 * one * t * control.x + t * t * end.x,
                        one * one * base.y + 2.0 * one * t * control.y + t * t * end.y,
                    )
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
        }
        _ => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                glyph,
                egui::FontId::proportional(s * 0.32),
                color,
            );
        }
    }
}

pub(crate) fn transport_button(ui: &mut egui::Ui, rect: Rect, glyph: &str, active: bool) -> bool {
    let response = ui.interact(rect, ui.id().with(glyph), egui::Sense::click());
    let fill = if active {
        theme::accent().gamma_multiply(0.25)
    } else if response.hovered() {
        theme::line()
    } else {
        theme::raised()
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(
            1.0_f32,
            if active {
                theme::accent()
            } else {
                theme::line()
            },
        ),
        egui::StrokeKind::Inside,
    );
    let icon_colour = match glyph {
        "\u{25CF}" => theme::recording(),
        "\u{25B6}" => theme::playing(),
        "\u{2016}" => theme::accent(),
        _ => {
            if active {
                theme::accent()
            } else {
                theme::text()
            }
        }
    };
    draw_transport_icon(ui.painter(), rect, glyph, icon_colour);
    let hint = match glyph {
        "\u{25A0}" => "Stop",
        "\u{25B6}" => "Play",
        "\u{2016}" => "Pause",
        "\u{25CF}" => "Record",
        "\u{21B6}" => "Undo",
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
