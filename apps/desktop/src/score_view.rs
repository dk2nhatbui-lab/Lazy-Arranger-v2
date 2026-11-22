//! The score canvas: the first renderer over `score::Engraved` (ADR-010).
//!
//! Draws page, staff lines, barlines and noteheads from the layout only; it
//! makes no musical decisions, so a PDF renderer can reuse the same layout.

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use score::{Engraved, Settings};

use crate::theme;

/// Screen rectangle of a page at a zoom factor.
#[must_use]
pub fn page_rect(centre: Pos2, settings: Settings, zoom: f32) -> Rect {
    Rect::from_center_size(
        centre,
        Vec2::new(settings.page_width * zoom, settings.page_height * zoom),
    )
}

/// Y of one staff line of a system, page coordinates scaled by zoom.
#[must_use]
pub fn staff_line_y(system_y: f32, line: usize, settings: Settings, zoom: f32) -> f32 {
    system_y * zoom + (line as f32) * settings.staff_space * zoom
}

/// Current page index clamped to the document.
#[must_use]
pub fn clamp_page(engraved: &Engraved, page: usize) -> usize {
    page.min(engraved.pages.len().saturating_sub(1))
}

/// Draws one page of the engraved document.
pub fn show(ui: &mut Ui, engraved: &Engraved, page: usize, zoom: f32) {
    let settings = Settings::default();
    let page_index = clamp_page(engraved, page);
    let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::hover());
    let centre = response.rect.center();
    let page_rect = page_rect(centre, settings, zoom);
    painter.rect_filled(page_rect, 0.0, Color32::from_rgb(0xF4, 0xF1, 0xEA));
    painter.rect_stroke(
        page_rect,
        0.0,
        Stroke::new(1.0_f32, theme::LINE_STRONG),
        egui::StrokeKind::Inside,
    );

    let origin = page_rect.min;
    if let Some(page) = engraved.pages.get(page_index) {
        for system in &page.systems {
            for line in 0..5 {
                let y = origin.y + staff_line_y(system.y, line, settings, zoom);
                let left = origin.x + settings.margin * zoom;
                let right = page_rect.max.x - settings.margin * zoom;
                painter.line_segment(
                    [Pos2::new(left, y), Pos2::new(right, y)],
                    Stroke::new(0.8_f32, Color32::from_rgb(0x3A, 0x3A, 0x3A)),
                );
            }
            let top = origin.y + staff_line_y(system.y, 0, settings, zoom);
            let bottom = origin.y + staff_line_y(system.y, 4, settings, zoom);
            for measure in &system.measures {
                let x = origin.x + measure.x * zoom;
                painter.line_segment(
                    [Pos2::new(x, top), Pos2::new(x, bottom)],
                    Stroke::new(1.2_f32, Color32::from_rgb(0x24, 0x24, 0x24)),
                );
                painter.text(
                    Pos2::new(x + 2.0, top - 12.0 * zoom),
                    egui::Align2::LEFT_BOTTOM,
                    measure.number.to_string(),
                    egui::FontId::proportional(9.0 * zoom),
                    Color32::from_rgb(0x55, 0x55, 0x55),
                );
            }
            // Closing barline of the last measure.
            if let Some(last) = system.measures.last() {
                let x = origin.x + last.x_end * zoom;
                painter.line_segment(
                    [Pos2::new(x, top), Pos2::new(x, bottom)],
                    Stroke::new(1.2_f32, Color32::from_rgb(0x24, 0x24, 0x24)),
                );
            }
        }
        for note in engraved.notes.iter().filter(|note| note.page == page_index) {
            let x = origin.x + note.x * zoom;
            let y = origin.y + note.y * zoom;
            let radius = 0.58 * settings.staff_space * zoom;
            if let Some(alter) = note.spelled.accidental {
                let glyph = match alter {
                    1 => "\u{266F}",
                    -1 => "\u{266D}",
                    _ => "\u{266E}",
                };
                painter.text(
                    Pos2::new(x - radius * 2.4, y),
                    egui::Align2::CENTER_CENTER,
                    glyph,
                    egui::FontId::proportional(radius * 2.2),
                    Color32::from_rgb(0x14, 0x14, 0x14),
                );
            }
            painter.circle_filled(Pos2::new(x, y), radius, Color32::from_rgb(0x14, 0x14, 0x14));
            painter.line_segment(
                [
                    Pos2::new(x + radius, y),
                    Pos2::new(x + radius, y - 3.4 * settings.staff_space * zoom),
                ],
                Stroke::new(1.1_f32, Color32::from_rgb(0x14, 0x14, 0x14)),
            );
        }
        painter.text(
            Pos2::new(centre.x, page_rect.max.y - 12.0),
            egui::Align2::CENTER_BOTTOM,
            format!("Page {} / {}", page_index + 1, engraved.pages.len()),
            egui::FontId::proportional(10.0),
            Color32::from_rgb(0x55, 0x55, 0x55),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_scale_with_zoom() {
        let settings = Settings::default();
        let rect = page_rect(Pos2::ZERO, settings, 2.0);
        assert!((rect.width() - settings.page_width * 2.0).abs() < 0.01);
        let line = staff_line_y(100.0, 4, settings, 2.0);
        assert!((line - (200.0 + 4.0 * settings.staff_space * 2.0)).abs() < 0.01);
    }

    #[test]
    fn page_indices_are_clamped() {
        let engraved = score::engrave(
            &project::song::Song::new("Empty", 120.0),
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        assert_eq!(clamp_page(&engraved, 5), 0);
    }
}
