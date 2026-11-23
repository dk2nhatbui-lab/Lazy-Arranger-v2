//! C++-style timeline: one coloured clip lane per track.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::piano_roll::{RollClick, RollOutcome};
use crate::theme;
use music_core::Tick;
use project::ClipId;
use project::TrackId;
use project::song::Song;

const LABEL_WIDTH: f32 = 0.0;
const DRAG_GRID: Tick = 240;

/// How a timeline clip drag changes the clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipEditKind {
    /// Move the clip start.
    Move,
    /// Change the clip length.
    Resize,
}

/// A finished timeline clip drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipEdit {
    /// Track that owns the clip.
    pub track: TrackId,
    /// Clip being edited.
    pub clip: ClipId,
    /// New start for a move.
    pub start: Tick,
    /// New length for a resize.
    pub length: Tick,
    /// Which field changed.
    pub kind: ClipEditKind,
}

#[derive(Clone, Debug)]
struct ClipDrag {
    track: TrackId,
    clip: ClipId,
    anchor_tick: Tick,
    start: Tick,
    length: Tick,
    kind: ClipEditKind,
}

fn dragged_edit(drag: &ClipDrag, pointer_tick: Tick) -> ClipEdit {
    match drag.kind {
        ClipEditKind::Move => {
            let delta = pointer_tick - drag.anchor_tick;
            let start = (drag.start + delta).max(0);
            ClipEdit {
                track: drag.track,
                clip: drag.clip,
                start: (start + DRAG_GRID / 2) / DRAG_GRID * DRAG_GRID,
                length: drag.length,
                kind: ClipEditKind::Move,
            }
        }
        ClipEditKind::Resize => {
            let length = pointer_tick.saturating_sub(drag.start).max(DRAG_GRID);
            ClipEdit {
                track: drag.track,
                clip: drag.clip,
                start: drag.start,
                length: (length + DRAG_GRID / 2) / DRAG_GRID * DRAG_GRID,
                kind: ClipEditKind::Resize,
            }
        }
    }
}
const RULER_HEIGHT: f32 = 28.0;
const CHORD_HEIGHT: f32 = 22.0;
const LANE_HEIGHT: f32 = 54.0;

/// Palette color for a track lane.
#[must_use]
pub fn lane_color(index: usize) -> Color32 {
    theme::track_colour(index)
}

/// Draws the timeline and returns the same outcome shape as the piano roll.
pub fn show(
    ui: &mut egui::Ui,
    song: &Song,
    position: Tick,
    selected: Option<TrackId>,
    zoom: f32,
    follow_playhead: bool,
) -> (RollOutcome, Option<TrackId>, Option<ClipEdit>) {
    let bar = song.time_signature.bar_ticks().max(1);
    let bars = ((song.end_tick().max(bar * 8) + bar - 1) / bar).max(8);
    let width =
        (LABEL_WIDTH + bars as f32 * 100.0 * zoom.clamp(0.5, 3.0)).max(ui.available_width());
    let mut result = (RollOutcome::default(), None, None);
    let drag_id = ui.id().with("timeline-clip-drag");
    let edit_id = ui.id().with("timeline-clip-edit");
    egui::ScrollArea::both()
        .id_salt("timeline-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            result = draw(ui, song, position, selected, width, drag_id, edit_id);
            result.2 = ui.data_mut(|data| {
                let edit = data.get_temp::<ClipEdit>(edit_id);
                data.remove::<ClipEdit>(edit_id);
                edit
            });
            if follow_playhead {
                let end = song.end_tick().max(bar * 8).max(1);
                let fraction = position.min(end) as f32 / end as f32;
                let x = ui.min_rect().left() + LABEL_WIDTH + fraction * (width - LABEL_WIDTH);
                let playhead = Rect::from_min_size(
                    pos2(x, ui.min_rect().top()),
                    vec2(2.0, ui.min_rect().height()),
                );
                if !ui.clip_rect().contains_rect(playhead) {
                    ui.scroll_to_rect(playhead, Some(egui::Align::Center));
                }
            }
        });
    result
}

fn draw(
    ui: &mut egui::Ui,
    song: &Song,
    position: Tick,
    selected: Option<TrackId>,
    width: f32,
    drag_id: egui::Id,
    edit_id: egui::Id,
) -> (RollOutcome, Option<TrackId>, Option<ClipEdit>) {
    let tracks = song.tracks.len().max(1);
    let height = RULER_HEIGHT + CHORD_HEIGHT + tracks as f32 * LANE_HEIGHT;
    let (response, painter) = ui.allocate_painter(vec2(width, height), Sense::click_and_drag());
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
    let mut clip_rects = Vec::new();
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
        painter.line_segment(
            [pos2(rect.left(), top), pos2(rect.right(), top)],
            Stroke::new(1.0_f32, theme::line()),
        );
        for clip in &track.clips {
            let clip_rect = Rect::from_min_max(
                pos2(x_for(clip.start), top + 3.0),
                pos2(
                    x_for(clip.end()).max(x_for(clip.start) + 2.0),
                    top + LANE_HEIGHT - 3.0,
                ),
            );
            clip_rects.push((track.id, clip.id, clip_rect, clip.start, clip.length));
            painter.rect_filled(clip_rect, 3.0, color.gamma_multiply(0.55));
            painter.rect_stroke(
                clip_rect,
                3.0,
                Stroke::new(1.0_f32, color),
                StrokeKind::Inside,
            );
        }
        if let Some(audio) = &track.audio {
            let audio_start = audio.start.max(0);
            let audio_end = (audio_start + audio.length.max(1)).min(end.max(audio_start + 1));
            let block = Rect::from_min_max(
                pos2(x_for(audio_start), top + 4.0),
                pos2(
                    x_for(audio_end).max(x_for(audio_start) + 2.0),
                    top + LANE_HEIGHT - 4.0,
                ),
            );
            painter.rect_filled(block, 3.0, color.gamma_multiply(0.58));
            painter.rect_stroke(block, 3.0, Stroke::new(1.0_f32, color), StrokeKind::Inside);
            let name = std::path::Path::new(&audio.path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("audio");
            painter.text(
                pos2(block.left() + 6.0, block.center().y),
                Align2::LEFT_CENTER,
                format!("audio · {name}"),
                FontId::proportional(9.0),
                theme::text(),
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

    if response.drag_started()
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some((track, clip, clip_rect, start, length)) = clip_rects
            .iter()
            .find(|(_, _, rect, _, _)| rect.contains(pointer))
    {
        let kind = if pointer.x >= clip_rect.right() - 10.0 {
            ClipEditKind::Resize
        } else {
            ClipEditKind::Move
        };
        let drag = ClipDrag {
            track: *track,
            clip: *clip,
            anchor_tick: tick_for(pointer.x),
            start: *start,
            length: *length,
            kind,
        };
        ui.data_mut(|data| data.insert_temp(drag_id, drag));
    }
    if response.drag_stopped()
        && let Some(pointer) = ui.input(|input| input.pointer.latest_pos())
        && let Some(drag) = ui.data_mut(|data| {
            let drag = data.get_temp::<ClipDrag>(drag_id);
            data.remove::<ClipDrag>(drag_id);
            drag
        })
    {
        let edit = dragged_edit(&drag, tick_for(pointer.x));
        ui.data_mut(|data| data.insert_temp(edit_id, edit));
    }

    if let Some(drag) = ui.data(|data| data.get_temp::<ClipDrag>(drag_id))
        && let Some(pointer) = ui.input(|input| input.pointer.latest_pos())
        && let Some(index) = song.tracks.iter().position(|track| track.id == drag.track)
    {
        let edit = dragged_edit(&drag, tick_for(pointer.x));
        let top = rect.top() + RULER_HEIGHT + CHORD_HEIGHT + index as f32 * LANE_HEIGHT;
        let start = match edit.kind {
            ClipEditKind::Move => edit.start,
            ClipEditKind::Resize => drag.start,
        };
        let length = match edit.kind {
            ClipEditKind::Move => drag.length,
            ClipEditKind::Resize => edit.length,
        };
        let ghost = Rect::from_min_max(
            pos2(x_for(start), top + 4.0),
            pos2(x_for(start + length), top + LANE_HEIGHT - 4.0),
        );
        painter.rect_filled(ghost, 3.0, theme::accent().gamma_multiply(0.18));
        painter.rect_stroke(
            ghost,
            3.0,
            Stroke::new(1.5_f32, theme::accent()),
            StrokeKind::Inside,
        );
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
    (outcome, selected_track, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_drags_move_and_resize_on_a_grid() {
        let drag = ClipDrag {
            track: TrackId::new(1).expect("track"),
            clip: ClipId::new(1).expect("clip"),
            anchor_tick: 960,
            start: 960,
            length: 480,
            kind: ClipEditKind::Move,
        };
        let moved = dragged_edit(&drag, 1440);
        assert_eq!(moved.start, 1440);
        assert_eq!(moved.kind, ClipEditKind::Move);
        let resized = dragged_edit(
            &ClipDrag {
                kind: ClipEditKind::Resize,
                ..drag
            },
            1680,
        );
        assert_eq!(resized.length, 720);
        assert_eq!(resized.kind, ClipEditKind::Resize);
    }

    #[test]
    fn the_timeline_draws_without_panicking() {
        let context = egui::Context::default();
        let mut ran = false;
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let song = arranging::demo::demo_song();
                let _ = show(ui, &song, 0, None, 1.0, false);
                ran = true;
            });
        });
        assert!(ran);
    }
}
