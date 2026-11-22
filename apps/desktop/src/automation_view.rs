//! The automation lane: a volume curve for the selected track.
//!
//! Click empty space to add a point, click a point to remove it. The edit is
//! returned to the caller, which applies it as one command.

use egui::{Pos2, Sense, Stroke, Ui, Vec2};
use music_core::Tick;
use project::song::{Automation, AutomationPoint};

use crate::theme;

/// Height of the lane.
pub const LANE_HEIGHT: f32 = 58.0;

/// Tick and value mapping for the lane.
#[derive(Clone, Copy, Debug)]
pub struct Mapping {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
    /// One past the last tick shown.
    pub end: Tick,
}

impl Mapping {
    /// Screen position of a point.
    #[must_use]
    pub fn position(&self, point: &AutomationPoint) -> Pos2 {
        let x = self.x + (point.tick.max(0) as f32 / self.end.max(1) as f32) * self.width;
        let y = self.y + self.height - (point.value.clamp(0.0, 127.0) / 127.0) * self.height;
        Pos2::new(x, y)
    }

    /// Tick at a position.
    #[must_use]
    pub fn tick_at(&self, x: f32) -> Tick {
        (((x - self.x) / self.width.max(1.0)) * self.end as f32).round() as Tick
    }

    /// Value at a position.
    #[must_use]
    pub fn value_at(&self, y: f32) -> f32 {
        ((self.y + self.height - y) / self.height.max(1.0) * 127.0).clamp(0.0, 127.0)
    }
}

/// Draws the lane and returns the edited automation when the user changes it.
pub fn show(ui: &mut Ui, lane: &Automation, end: Tick) -> Option<Automation> {
    let (response, painter) =
        ui.allocate_painter(Vec2::new(ui.available_width(), LANE_HEIGHT), Sense::click());
    let rect = response.rect;
    let mapping = Mapping {
        x: rect.left() + 6.0,
        y: rect.top() + 6.0,
        width: rect.width() - 12.0,
        height: rect.height() - 12.0,
        end: end.max(1),
    };
    painter.rect_filled(rect, 0.0, theme::LANE);
    painter.line_segment(
        [
            Pos2::new(mapping.x, mapping.y + mapping.height),
            Pos2::new(mapping.x + mapping.width, mapping.y + mapping.height),
        ],
        Stroke::new(1.0_f32, theme::LINE),
    );
    let mut points = lane.points.clone();
    points.sort_by_key(|point| point.tick);
    let mut previous = None;
    for point in &points {
        let position = mapping.position(point);
        if let Some(last) = previous {
            painter.line_segment([last, position], Stroke::new(1.4_f32, theme::ACCENT));
        }
        painter.circle_filled(position, 3.5, theme::ACCENT);
        previous = Some(position);
    }
    painter.text(
        Pos2::new(mapping.x + 4.0, rect.top() + 2.0),
        egui::Align2::LEFT_TOP,
        "volume automation",
        egui::FontId::proportional(9.0),
        theme::TEXT_DIM,
    );
    let pointer = response.interact_pointer_pos()?;
    if !response.clicked() {
        return None;
    }
    if let Some(index) = points.iter().position(|point| {
        let position = mapping.position(point);
        (position.x - pointer.x).abs() < 6.0 && (position.y - pointer.y).abs() < 6.0
    }) {
        points.remove(index);
    } else {
        points.push(AutomationPoint {
            tick: mapping.tick_at(pointer.x).max(0),
            value: mapping.value_at(pointer.y),
        });
        points.sort_by_key(|point| point.tick);
    }
    Some(Automation {
        points,
        ..lane.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::song::AutomationTarget;

    fn mapping() -> Mapping {
        Mapping {
            x: 10.0,
            y: 10.0,
            width: 200.0,
            height: 40.0,
            end: 3840,
        }
    }

    #[test]
    fn positions_and_readbacks_invert() {
        let mapping = mapping();
        let point = AutomationPoint {
            tick: 1920,
            value: 64.0,
        };
        let position = mapping.position(&point);
        assert!((mapping.tick_at(position.x) - point.tick).abs() <= 8);
        assert!((mapping.value_at(position.y) - point.value).abs() < 0.5);
    }

    #[test]
    fn the_lane_keeps_its_target() {
        let lane = Automation {
            target: AutomationTarget::Volume,
            points: vec![AutomationPoint {
                tick: 0,
                value: 100.0,
            }],
        };
        assert_eq!(lane.target, AutomationTarget::Volume);
    }
}
