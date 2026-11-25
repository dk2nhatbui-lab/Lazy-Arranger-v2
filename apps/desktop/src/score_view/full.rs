//! The all-tracks score on screen: `score::full` lays it out and draws it
//! through an egui canvas, the same layout the PDF export uses.

use egui::{Color32, Pos2, Rect, Shape, Stroke, Ui};
use music_core::Tick;
use score::full::{Anchor, Canvas, FullScore, Ink, Point};
use score::{Engraved, Settings};

use super::ScoreState;
use crate::theme;

const PAPER: Color32 = Color32::from_rgb(0xF4, 0xF1, 0xEA);

/// Paints page points onto the screen.
struct EguiCanvas<'a> {
    painter: &'a egui::Painter,
    origin: Pos2,
    scale: f32,
    treble: Option<egui::TextureId>,
    bass: Option<egui::TextureId>,
}

impl EguiCanvas<'_> {
    fn at(&self, (x, y): Point) -> Pos2 {
        Pos2::new(
            self.origin.x + x * self.scale,
            self.origin.y + y * self.scale,
        )
    }

    fn colour(ink: Ink) -> Color32 {
        match ink {
            Ink::Black => Color32::from_rgb(0x14, 0x14, 0x14),
            Ink::Grey => Color32::from_rgb(0x50, 0x50, 0x50),
            Ink::Paper => PAPER,
            Ink::Playhead => Color32::from_rgb(0xD0, 0x30, 0x30),
        }
    }

    fn stroke(&self, width: f32, ink: Ink) -> Stroke {
        Stroke::new((width * self.scale).max(0.5), Self::colour(ink))
    }

    fn font(&self, size: f32) -> egui::FontId {
        egui::FontId::proportional((size * self.scale).max(1.0))
    }
}

impl Canvas for EguiCanvas<'_> {
    fn line(&self, from: Point, to: Point, width: f32, ink: Ink) {
        let width = if ink == Ink::Playhead {
            1.0 / self.scale
        } else {
            width
        };
        self.painter
            .line_segment([self.at(from), self.at(to)], self.stroke(width, ink));
    }

    fn polyline(&self, points: &[Point], width: f32) {
        self.painter.add(Shape::line(
            points.iter().map(|point| self.at(*point)).collect(),
            self.stroke(width, Ink::Black),
        ));
    }

    fn polygon(&self, points: &[Point], fill: Ink, outline: Option<f32>) {
        self.painter.add(Shape::convex_polygon(
            points.iter().map(|point| self.at(*point)).collect(),
            Self::colour(fill),
            outline.map_or(Stroke::NONE, |width| self.stroke(width, Ink::Black)),
        ));
    }

    fn circle(&self, centre: Point, radius: f32) {
        self.painter.circle_filled(
            self.at(centre),
            radius * self.scale,
            Self::colour(Ink::Black),
        );
    }

    fn fill_rect(&self, min: Point, max: Point) {
        self.painter.rect_filled(
            Rect::from_min_max(self.at(min), self.at(max)),
            0.0,
            Self::colour(Ink::Black),
        );
    }

    fn stroke_rect(&self, min: Point, max: Point, width: f32) {
        self.painter.rect_stroke(
            Rect::from_min_max(self.at(min), self.at(max)),
            0.0,
            self.stroke(width, Ink::Black),
            egui::StrokeKind::Inside,
        );
    }

    fn text(&self, at: Point, anchor: Anchor, text: &str, size: f32, ink: Ink) {
        let galley =
            self.painter
                .layout_no_wrap(text.to_string(), self.font(size), Self::colour(ink));
        let size = galley.size();
        let pos = self.at(at) - egui::vec2(size.x * anchor.x, size.y * anchor.y);
        self.painter.galley(pos, galley, Self::colour(ink));
    }

    fn text_width(&self, text: &str, size: f32) -> f32 {
        self.painter
            .layout_no_wrap(text.to_string(), self.font(size), Color32::BLACK)
            .size()
            .x
            / self.scale
    }

    fn clef(&self, treble: bool, min: Point, max: Point) {
        let texture = if treble { self.treble } else { self.bass };
        if let Some(texture) = texture {
            self.painter.image(
                texture,
                Rect::from_min_max(self.at(min), self.at(max)),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
}

/// Draws the all-tracks score page and returns the page count and a seek
/// tick from a click.
#[allow(clippy::too_many_arguments)]
pub fn show_full(
    ui: &mut Ui,
    tracks: &[(String, Engraved)],
    page: usize,
    zoom: f32,
    title: &str,
    state: &mut ScoreState,
    position: Tick,
    frames: bool,
    bpm: f64,
    markers: &[project::Marker],
) -> (usize, Option<Tick>) {
    let settings = Settings::default();
    let full = FullScore::new(tracks, frames, markers, settings);
    let page_count = full.page_count();
    let page_index = page.min(page_count.saturating_sub(1));
    super::ensure_clefs(ui.ctx(), state);
    let (response, painter) =
        ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let available = response.rect.size();
    let fit = ((available.x - 24.0).max(120.0) / settings.page_width)
        .min((available.y - 24.0).max(120.0) / settings.page_height)
        .clamp(0.15, 1.0);
    let scale = fit * zoom;
    let page_rect = super::page_rect(response.rect.center(), settings, scale);
    painter.rect_filled(page_rect, 0.0, PAPER);
    painter.rect_stroke(
        page_rect,
        0.0,
        Stroke::new(1.0_f32, theme::line_strong()),
        egui::StrokeKind::Inside,
    );
    let canvas = EguiCanvas {
        painter: &painter,
        origin: page_rect.min,
        scale,
        treble: state.glyph_treble.as_ref().map(egui::TextureHandle::id),
        bass: state.glyph_bass.as_ref().map(egui::TextureHandle::id),
    };
    full.draw_page(
        &canvas,
        page_index,
        title,
        bpm,
        Some(position),
        frames,
        markers,
    );
    let seek = response
        .interact_pointer_pos()
        .filter(|_| response.clicked() || response.dragged())
        .filter(|point| page_rect.contains(*point))
        .and_then(|point| {
            full.tick_at(
                page_index,
                (point.x - page_rect.left()) / scale,
                (point.y - page_rect.top()) / scale,
            )
        });
    (page_count, seek)
}

/// The full-score page holding a tick, when a system contains it.
#[must_use]
pub fn full_score_page_for_tick(
    tracks: &[(String, Engraved)],
    tick: Tick,
    frames: bool,
    markers: &[project::Marker],
) -> Option<usize> {
    FullScore::new(tracks, frames, markers, Settings::default()).page_for_tick(tick)
}
