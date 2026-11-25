//! The automation lane: one target's curve for the selected track.
//!
//! Click empty space to add a point, click a point to remove it. The edit is
//! returned to the caller, which applies it as one command.

use egui::{Pos2, Sense, Stroke, StrokeKind, Ui, Vec2};
use music_core::Tick;
use project::song::{Automation, AutomationPoint, AutomationTarget};

use crate::theme;

/// Human-readable label for an automation target.
#[must_use]
pub fn target_label(target: &AutomationTarget) -> String {
    match target {
        AutomationTarget::Volume => String::from("volume automation"),
        AutomationTarget::Pan => String::from("pan automation"),
        AutomationTarget::Send => String::from("send automation"),
        AutomationTarget::Brightness => String::from("brightness automation"),
        AutomationTarget::PluginParam { slot, parameter } => {
            format!("plugin {slot}:{parameter} automation")
        }
    }
}

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
    let (response, painter) = ui.allocate_painter(
        Vec2::new(ui.available_width(), LANE_HEIGHT),
        Sense::click_and_drag(),
    );
    let rect = response.rect;
    let mapping = Mapping {
        x: rect.left() + 6.0,
        y: rect.top() + 6.0,
        width: rect.width() - 12.0,
        height: rect.height() - 12.0,
        end: end.max(1),
    };
    painter.rect_filled(rect, 0.0, theme::lane());
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, theme::line()),
        StrokeKind::Inside,
    );
    let mid_y = mapping.y + mapping.height * 0.5;
    painter.line_segment(
        [
            Pos2::new(mapping.x, mid_y),
            Pos2::new(mapping.x + mapping.width, mid_y),
        ],
        Stroke::new(1.0_f32, theme::line().gamma_multiply(0.55)),
    );
    painter.line_segment(
        [
            Pos2::new(mapping.x, mapping.y + mapping.height),
            Pos2::new(mapping.x + mapping.width, mapping.y + mapping.height),
        ],
        Stroke::new(1.0_f32, theme::line()),
    );
    let mut points = lane.points.clone();
    points.sort_by_key(|point| point.tick);

    let drag_id = ui.id().with("automation-drag-index");
    let mut dragged_index = ui.data(|data| data.get_temp::<usize>(drag_id));
    if response.drag_started()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        dragged_index = points.iter().position(|point| {
            let position = mapping.position(point);
            (position.x - pointer.x).abs() < 7.0 && (position.y - pointer.y).abs() < 7.0
        });
        if let Some(index) = dragged_index {
            ui.data_mut(|data| data.insert_temp(drag_id, index));
        }
    }

    let mut preview_points = points.clone();
    if let Some(index) = dragged_index
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(point) = preview_points.get_mut(index)
    {
        point.tick = mapping.tick_at(pointer.x).max(0);
        point.value = mapping.value_at(pointer.y);
    }

    let mut previous = None;
    for (index, point) in preview_points.iter().enumerate() {
        let position = mapping.position(point);
        if let Some(last) = previous {
            painter.line_segment([last, position], Stroke::new(1.4_f32, theme::accent()));
        }
        let radius = if dragged_index == Some(index) {
            4.5
        } else {
            3.5
        };
        painter.circle_filled(position, radius, theme::accent());
        previous = Some(position);
    }
    let label = target_label(&lane.target);
    painter.text(
        Pos2::new(mapping.x + 4.0, rect.top() + 2.0),
        egui::Align2::LEFT_TOP,
        label,
        egui::FontId::proportional(9.0),
        theme::text_dim(),
    );

    if response.drag_stopped()
        && let Some(index) = ui.data_mut(|data| {
            let value = data.get_temp::<usize>(drag_id);
            data.remove::<usize>(drag_id);
            value
        })
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(point) = points.get_mut(index)
    {
        point.tick = mapping.tick_at(pointer.x).max(0);
        point.value = mapping.value_at(pointer.y);
        points.sort_by_key(|entry| entry.tick);
        return Some(Automation {
            points,
            ..lane.clone()
        });
    }

    let response = response.on_hover_text(
        "Click empty space to add a point, drag a point to move it, or click a point to remove it",
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
        assert_eq!(target_label(&AutomationTarget::Volume), "volume automation");
        assert_eq!(target_label(&AutomationTarget::Pan), "pan automation");
        assert_eq!(target_label(&AutomationTarget::Send), "send automation");
        assert_eq!(
            target_label(&AutomationTarget::Brightness),
            "brightness automation"
        );
        assert_eq!(
            target_label(&AutomationTarget::PluginParam {
                slot: 1,
                parameter: 3
            }),
            "plugin 1:3 automation"
        );
    }
}
