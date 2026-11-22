//! Read-only piano roll for the shell.
//!
//! Geometry is a pure value with tests so hit testing and drawing cannot drift
//! apart: both use the same transform (ADR-010's rule for the score applies here
//! too). Editing arrives in a later Stage 6 increment.

use egui::{Pos2, Rect, Ui, Vec2};
use music_core::{Tick, TimeSignature};
use project::TrackId;
use project::song::{Song, Track};

use crate::theme;

/// Horizontal zoom: ticks per screen point.
const MIN_TICKS_PER_POINT: f32 = 2.0;
const MAX_TICKS_PER_POINT: f32 = 240.0;

/// The transform between song ticks and screen positions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RollGeometry {
    /// Screen position of tick 0 at pitch 127 (top).
    pub origin: Pos2,
    /// Horizontal scale.
    pub ticks_per_point: f32,
    /// Height of one pitch row.
    pub row_height: f32,
    /// Highest pitch shown.
    pub top_pitch: u8,
    /// Time signature, for bar and beat lines.
    pub meter: TimeSignature,
}

impl RollGeometry {
    /// X position for a tick.
    #[must_use]
    pub fn x_at(&self, tick: Tick) -> f32 {
        self.origin.x + tick as f32 / self.ticks_per_point
    }

    /// Tick at an X position.
    #[must_use]
    pub fn tick_at(&self, x: f32) -> Tick {
        ((x - self.origin.x) * self.ticks_per_point).round() as Tick
    }

    /// Y position for the top of a pitch row.
    #[must_use]
    pub fn y_at(&self, pitch: u8) -> f32 {
        self.origin.y + f32::from(self.top_pitch.saturating_sub(pitch)) * self.row_height
    }

    /// Row for a position.
    #[must_use]
    pub fn pitch_at(&self, y: f32) -> u8 {
        let rows = ((y - self.origin.y) / self.row_height).floor() as i32;
        let pitch = i32::from(self.top_pitch) - rows;
        u8::try_from(pitch.clamp(0, 127)).unwrap_or(0)
    }

    /// Rectangle for one note.
    #[must_use]
    pub fn note_rect(&self, tick: Tick, duration: Tick, pitch: u8) -> Rect {
        let x = self.x_at(tick);
        let width = (duration.max(1) as f32 / self.ticks_per_point).max(1.5);
        let y = self.y_at(pitch);
        Rect::from_min_size(
            Pos2::new(x, y + 1.0),
            Vec2::new(width, self.row_height - 2.0),
        )
    }
}

/// View state that survives frames.
#[derive(Clone, Copy, Debug)]
pub struct RollState {
    /// Horizontal zoom.
    pub ticks_per_point: f32,
    /// Highest pitch shown.
    pub top_pitch: u8,
    /// Selected track, if any.
    pub selected: Option<TrackId>,
}

impl Default for RollState {
    fn default() -> Self {
        Self {
            ticks_per_point: 12.0,
            top_pitch: 96,
            selected: None,
        }
    }
}

impl RollState {
    /// Clamps zoom and scroll after an input event.
    pub fn clamp(&mut self) {
        self.ticks_per_point = self
            .ticks_per_point
            .clamp(MIN_TICKS_PER_POINT, MAX_TICKS_PER_POINT);
        self.top_pitch = self.top_pitch.clamp(12, 127);
    }
}

/// A primary click in the roll, resolved to musical coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RollClick {
    /// Tick under the pointer.
    pub tick: Tick,
    /// Pitch row under the pointer.
    pub pitch: u8,
    /// Note that was hit, if any.
    pub note: Option<(TrackId, project::NoteId)>,
}

/// Draws the roll and handles zoom and scroll; returns a primary click.
pub fn show(ui: &mut Ui, song: &Song, state: &mut RollState, playhead: Tick) -> Option<RollClick> {
    let available = ui.available_size();
    let (response, painter) = ui.allocate_painter(available, egui::Sense::click_and_drag());
    let rect = response.rect;

    if response.dragged() {
        state.ticks_per_point *= 1.0 + response.drag_delta().x * 0.001;
        let scroll = response.drag_delta().y / theme::GRID;
        state.top_pitch = u8::try_from((i32::from(state.top_pitch) + scroll as i32).clamp(12, 127))
            .unwrap_or(state.top_pitch);
        state.clamp();
    }
    if let Some(hover) = response.hover_pos() {
        let scroll = ui.input(|input| input.raw_scroll_delta.y);
        if scroll.abs() > 0.0 {
            let factor = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
            match hover.x - rect.left() {
                _ if ui.input(|input| input.modifiers.command) => {
                    state.ticks_per_point /= factor;
                    state.clamp();
                }
                _ => {
                    let rows = (scroll / theme::GRID).round() as i32;
                    state.top_pitch =
                        u8::try_from((i32::from(state.top_pitch) - rows).clamp(12, 127))
                            .unwrap_or(state.top_pitch);
                }
            }
        }
    }

    let geometry = RollGeometry {
        origin: Pos2::new(rect.left() + 48.0, rect.top() + 20.0),
        ticks_per_point: state.ticks_per_point,
        row_height: 10.0,
        top_pitch: state.top_pitch,
        meter: song.time_signature,
    };

    painter.rect_filled(rect, 0.0, theme::TIMELINE);
    draw_grid(&painter, rect, &geometry, song);
    for (index, track) in song.tracks.iter().enumerate() {
        let colour = theme::track_colour(index);
        let selected = state.selected == Some(track.id);
        draw_track(&painter, rect, &geometry, track, colour, selected);
    }
    draw_playhead(&painter, rect, &geometry, playhead);
    draw_pitch_guide(&painter, rect, &geometry);

    let pointer = response.interact_pointer_pos()?;
    if !response.clicked() {
        return None;
    }
    let tick = geometry.tick_at(pointer.x).max(0);
    let pitch = geometry.pitch_at(pointer.y);
    let order: Vec<&Track> = state
        .selected
        .and_then(|id| song.tracks.iter().find(|track| track.id == id))
        .into_iter()
        .chain(
            song.tracks
                .iter()
                .filter(|track| Some(track.id) != state.selected),
        )
        .collect();
    for track in order {
        for note in &track.notes {
            let note_rect = geometry.note_rect(note.start, note.duration, note.pitch.as_u8());
            if note_rect.contains(pointer) {
                return Some(RollClick {
                    tick,
                    pitch: note.pitch.as_u8(),
                    note: Some((track.id, note.id)),
                });
            }
        }
    }
    Some(RollClick {
        tick,
        pitch,
        note: None,
    })
}

fn draw_grid(painter: &egui::Painter, rect: Rect, geometry: &RollGeometry, song: &Song) {
    let bar = song.time_signature.bar_ticks().max(1);
    let beat = song.time_signature.beat_ticks().max(1);
    let width = geometry.ticks_per_point;
    let first_bar = geometry.tick_at(rect.left()).div_euclid(bar);
    let last_bar = geometry.tick_at(rect.right()).div_euclid(bar);
    for index in first_bar..=last_bar {
        let tick = index * bar;
        let x = geometry.x_at(tick);
        if x < rect.left() || x > rect.right() {
            continue;
        }
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            egui::Stroke::new(1.0_f32, theme::GRID_BAR),
        );
        if width <= 24.0 {
            painter.text(
                Pos2::new(x + 4.0, rect.top() + 3.0),
                egui::Align2::LEFT_TOP,
                format!("{}", index + 1),
                egui::FontId::proportional(10.0),
                theme::TEXT_DIM,
            );
        }
        for beat_index in 1..i64::from(song.time_signature.numerator) {
            let beat_x = geometry.x_at(tick + beat_index * beat);
            if beat_x > rect.left() && beat_x < rect.right() {
                painter.line_segment(
                    [
                        Pos2::new(beat_x, rect.top()),
                        Pos2::new(beat_x, rect.bottom()),
                    ],
                    egui::Stroke::new(1.0_f32, theme::GRID_BEAT),
                );
            }
        }
    }
}

fn draw_track(
    painter: &egui::Painter,
    rect: Rect,
    geometry: &RollGeometry,
    track: &Track,
    colour: egui::Color32,
    selected: bool,
) {
    let fill = if selected {
        colour.gamma_multiply(0.95)
    } else {
        colour.gamma_multiply(0.65)
    };
    for note in &track.notes {
        let note_rect = geometry.note_rect(note.start, note.duration, note.pitch.as_u8());
        if !rect.intersects(note_rect) {
            continue;
        }
        painter.rect_filled(note_rect, 2.0, fill);
        if selected {
            painter.rect_stroke(
                note_rect,
                2.0,
                egui::Stroke::new(1.0_f32, theme::TEXT),
                egui::StrokeKind::Inside,
            );
        }
    }
}

fn draw_playhead(painter: &egui::Painter, rect: Rect, geometry: &RollGeometry, tick: Tick) {
    let x = geometry.x_at(tick);
    if x >= rect.left() && x <= rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            egui::Stroke::new(1.5_f32, theme::PLAYHEAD),
        );
    }
}

fn draw_pitch_guide(painter: &egui::Painter, rect: Rect, geometry: &RollGeometry) {
    for (pitch, name) in [(60u8, "C4"), (72, "C5"), (48, "C3")] {
        let y = geometry.y_at(pitch);
        if y > rect.top() && y < rect.bottom() {
            painter.text(
                Pos2::new(rect.left() + 6.0, y + geometry.row_height * 0.5),
                egui::Align2::LEFT_CENTER,
                name,
                egui::FontId::proportional(10.0),
                theme::TEXT_DIM,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> RollGeometry {
        RollGeometry {
            origin: Pos2::new(10.0, 20.0),
            ticks_per_point: 10.0,
            row_height: 10.0,
            top_pitch: 96,
            meter: TimeSignature::default(),
        }
    }

    #[test]
    fn ticks_and_points_invert_each_other() {
        let geometry = geometry();
        for tick in [0, 960, 3840, 15360] {
            let x = geometry.x_at(tick);
            assert!((geometry.tick_at(x) - tick).abs() <= 1, "tick {tick}");
        }
    }

    #[test]
    fn notes_grow_with_duration_and_move_with_tick() {
        let geometry = geometry();
        let short = geometry.note_rect(0, 960, 60);
        let long = geometry.note_rect(0, 1920, 60);
        assert!(long.width() > short.width());
        let later = geometry.note_rect(960, 960, 60);
        assert!(later.min.x > short.min.x);
        assert_eq!(short.min.y, later.min.y);
    }

    #[test]
    fn higher_pitches_are_higher_on_screen() {
        let geometry = geometry();
        let low = geometry.note_rect(0, 960, 48);
        let high = geometry.note_rect(0, 960, 72);
        assert!(high.min.y < low.min.y);
        assert_eq!(geometry.pitch_at(high.min.y), 72);
        assert_eq!(geometry.pitch_at(low.min.y), 48);
    }

    #[test]
    fn minimum_note_width_is_visible() {
        let geometry = geometry();
        let tiny = geometry.note_rect(0, 1, 60);
        assert!(tiny.width() >= 1.5);
    }

    #[test]
    fn zoom_stays_inside_limits() {
        let mut too_far_in = RollState {
            ticks_per_point: 0.001,
            ..RollState::default()
        };
        too_far_in.clamp();
        assert_eq!(too_far_in.ticks_per_point, MIN_TICKS_PER_POINT);

        let mut too_far_out = RollState {
            ticks_per_point: 10_000.0,
            ..RollState::default()
        };
        too_far_out.clamp();
        assert_eq!(too_far_out.ticks_per_point, MAX_TICKS_PER_POINT);
    }
}
