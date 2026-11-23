//! C++-style timeline: one coloured clip lane per track.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::piano_roll::{RollClick, RollOutcome};
use crate::theme;
use music_core::Tick;
use project::TrackId;
use project::song::Song;

const LABEL_WIDTH: f32 = 112.0;
const RULER_HEIGHT: f32 = 28.0;
const CHORD_HEIGHT: f32 = 22.0;
const LANE_HEIGHT: f32 = 54.0;

/// Palette color for a track lane.
#[must_use]
pub fn lane_color(index: usize) -> Color32 {
    LANE_COLORS[index % LANE_COLORS.len()]
}

const LANE_COLORS: [Color32; 8] = [
    Color32::from_rgb(232, 106, 74),
    Color32::from_rgb(60, 140, 122),
    Color32::from_rgb(181, 154, 90),
    Color32::from_rgb(91, 127, 176),
    Color32::from_rgb(123, 111, 168),
    Color32::from_rgb(196, 132, 74),
    Color32::from_rgb(92, 154, 98),
    Color32::from_rgb(176, 96, 128),
];

/// Draws the timeline and returns the same outcome shape as the piano roll.
pub fn show(
    ui: &mut egui::Ui,
    song: &Song,
    position: Tick,
    selected: Option<TrackId>,
    zoom: f32,
) -> (RollOutcome, Option<TrackId>) {
    let bar = song.time_signature.bar_ticks().max(1);
    let bars = ((song.end_tick().max(bar * 8) + bar - 1) / bar).max(8);
    let width =
        (LABEL_WIDTH + bars as f32 * 100.0 * zoom.clamp(0.5, 3.0)).max(ui.available_width());
    let mut result = (RollOutcome::default(), None);
    egui::ScrollArea::horizontal()
        .id_salt("timeline-scroll")
        .show(ui, |ui| {
            result = draw(ui, song, position, selected, width);
        });
    result
}

fn draw(
    ui: &mut egui::Ui,
    song: &Song,
    position: Tick,
    selected: Option<TrackId>,
    width: f32,
) -> (RollOutcome, Option<TrackId>) {
    let tracks = song.tracks.len().max(1);
    let height = RULER_HEIGHT + CHORD_HEIGHT + tracks as f32 * LANE_HEIGHT;
    let (response, painter) = ui.allocate_painter(vec2(width, height), Sense::click());
    let rect = response.rect;
    let content = Rect::from_min_max(pos2(rect.left() + LABEL_WIDTH, rect.top()), rect.max);
    let bar = song.time_signature.bar_ticks().max(1);
    let end = song.end_tick().max(bar * 4).max(1);
    let x_for = |tick: Tick| content.left() + tick as f32 / end as f32 * content.width();
    let tick_for = |x: f32| {
        (((x - content.left()) / content.width().max(1.0) * end as f32).round() as Tick).max(0)
    };

    painter.rect_filled(rect, 0.0, theme::background());
    painter.rect_filled(
        Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + RULER_HEIGHT)),
        0.0,
        theme::raised(),
    );
    painter.line_segment(
        [
            pos2(rect.left(), rect.top() + RULER_HEIGHT),
            pos2(rect.right(), rect.top() + RULER_HEIGHT),
        ],
        Stroke::new(1.0_f32, theme::line()),
    );
    painter.rect_filled(
        Rect::from_min_max(
            pos2(rect.left(), rect.top() + RULER_HEIGHT),
            pos2(rect.right(), rect.top() + RULER_HEIGHT + CHORD_HEIGHT),
        ),
        0.0,
        theme::panel(),
    );

    let mut tick = 0;
    while tick <= end {
        let x = x_for(tick);
        painter.line_segment(
            [pos2(x, rect.top() + RULER_HEIGHT), pos2(x, rect.bottom())],
            Stroke::new(1.0_f32, theme::line().gamma_multiply(0.55)),
        );
        let number = tick / bar + 1;
        painter.text(
            pos2(x + 4.0, rect.top() + 5.0),
            Align2::LEFT_TOP,
            number.to_string(),
            FontId::proportional(11.0),
            theme::text_muted(),
        );
        tick += bar;
    }
    for event in &song.chords {
        if event.tick > end {
            continue;
        }
        painter.text(
            pos2(x_for(event.tick) + 3.0, rect.top() + RULER_HEIGHT + 4.0),
            Align2::LEFT_TOP,
            event.chord.format(music_core::Spelling::Sharp),
            FontId::proportional(11.0),
            theme::text(),
        );
    }

    let lane_top = rect.top() + RULER_HEIGHT + CHORD_HEIGHT;
    let pointer = response.interact_pointer_pos();
    let mut clicked_track = None;
    let mut clicked_clip = None;
    for (index, track) in song.tracks.iter().enumerate() {
        let top = lane_top + index as f32 * LANE_HEIGHT;
        let lane = Rect::from_min_max(
            pos2(rect.left(), top),
            pos2(rect.right(), top + LANE_HEIGHT),
        );
        let color = lane_color(index);
        let is_selected = selected == Some(track.id);
        painter.rect_filled(
            Rect::from_min_max(
                pos2(content.left(), top),
                pos2(content.right(), top + LANE_HEIGHT),
            ),
            0.0,
            color.gamma_multiply(if is_selected { 0.42 } else { 0.30 }),
        );
        if is_selected {
            painter.rect_stroke(
                lane,
                0.0,
                Stroke::new(2.0_f32, theme::accent()),
                StrokeKind::Inside,
            );
        }
        painter.rect_filled(
            Rect::from_min_max(
                pos2(rect.left(), top),
                pos2(content.left(), top + LANE_HEIGHT),
            ),
            0.0,
            if is_selected {
                theme::raised()
            } else {
                theme::panel()
            },
        );
        painter.line_segment(
            [pos2(rect.left(), top), pos2(rect.right(), top)],
            Stroke::new(1.0_f32, theme::line()),
        );
        painter.rect_filled(
            Rect::from_min_size(
                pos2(rect.left() + 6.0, top + 12.0),
                vec2(4.0, LANE_HEIGHT - 24.0),
            ),
            1.0,
            color,
        );
        painter.text(
            pos2(rect.left() + 16.0, top + 10.0),
            Align2::LEFT_TOP,
            if track.name.is_empty() {
                "untitled"
            } else {
                &track.name
            },
            FontId::proportional(12.0),
            theme::text(),
        );
        painter.text(
            pos2(rect.left() + 16.0, top + 28.0),
            Align2::LEFT_TOP,
            format!("{} notes", track.notes.len()),
            FontId::proportional(9.0),
            theme::text_dim(),
        );
        for clip in &track.clips {
            let clip_rect = Rect::from_min_max(
                pos2(x_for(clip.start), top + 3.0),
                pos2(
                    x_for(clip.end()).max(x_for(clip.start) + 2.0),
                    top + LANE_HEIGHT - 3.0,
                ),
            );
            painter.rect_filled(clip_rect, 3.0, color.gamma_multiply(0.55));
            painter.rect_stroke(
                clip_rect,
                3.0,
                Stroke::new(1.0_f32, color),
                StrokeKind::Inside,
            );
        }
        for note in &track.notes {
            if note.start > end {
                continue;
            }
            let x = x_for(note.start);
            let note_width = (x_for(note.start + note.duration).max(x + 2.0) - x).max(2.0);
            let offset = (f32::from(note.pitch.as_u8()) - 60.0) * 0.55;
            let y = (top + LANE_HEIGHT * 0.55 + offset).clamp(top + 8.0, top + LANE_HEIGHT - 8.0);
            painter.rect_filled(
                Rect::from_min_size(pos2(x, y), vec2(note_width.min(80.0), 3.0)),
                1.0,
                color.gamma_multiply(0.95),
            );
        }
        if let Some(pointer) = pointer
            && lane.contains(pointer)
        {
            clicked_track = Some(track.id);
            let hit_tick = tick_for(pointer.x);
            if let Some(clip) = track.clips.iter().find(|clip| clip.contains(hit_tick)) {
                clicked_clip = Some((track.id, clip.id));
            }
        }
    }

    let playhead_x = x_for(position);
    painter.line_segment(
        [
            pos2(playhead_x, rect.top() + RULER_HEIGHT),
            pos2(playhead_x, rect.bottom()),
        ],
        Stroke::new(1.5_f32, theme::recording()),
    );
    let mut outcome = RollOutcome::default();
    let mut selected_track = clicked_track;
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
        && pointer.x >= content.left()
    {
        let tick = tick_for(pointer.x);
        outcome.seek = Some(tick);
        outcome.click = Some(RollClick {
            tick,
            pitch: 60,
            note: None,
            clip: clicked_clip,
            shift: ui.input(|input| input.modifiers.shift),
        });
        selected_track = clicked_track.or(selected);
    }
    (outcome, selected_track)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_timeline_draws_without_panicking() {
        let context = egui::Context::default();
        let mut ran = false;
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let song = arranging::demo::demo_song();
                let _ = show(ui, &song, 0, None, 1.0);
                ran = true;
            });
        });
        assert!(ran);
    }
}
