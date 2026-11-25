//! C++-style timeline: one coloured clip lane per track.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::piano_roll::{RollClick, RollOutcome};
use crate::theme;
use music_core::Tick;
use project::ClipId;
use project::TrackId;
use project::song::Song;
use std::collections::HashMap;

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
/// Combined height of the bar ruler and chord lane above track row 0.
pub const HEADER_HEIGHT: f32 = RULER_HEIGHT + CHORD_HEIGHT;
/// Height of one track lane in the timeline (matches the sidebar track rows).
pub const LANE_HEIGHT: f32 = 64.0;
/// Gap between a part and its lane edge, from v1 `Arrangement::clipBounds`.
const PART_INSET: f32 = 0.25;

/// Computes the vertical Y coordinate for a note preview bar inside a lane.
///
/// Higher MIDI pitches map closer to the top of the lane (smaller Y).
#[must_use]
pub fn note_lane_y(top: f32, pitch: u8) -> f32 {
    let offset = (f32::from(pitch) - 60.0) * 0.55;
    (top + LANE_HEIGHT * 0.55 - offset).clamp(top + 6.0, top + LANE_HEIGHT - 8.0)
}

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
    waveforms: &HashMap<TrackId, (String, Vec<audio::waveform::Peak>)>,
) -> (RollOutcome, Option<TrackId>, Option<ClipEdit>) {
    show_with_loop(
        ui,
        song,
        position,
        selected,
        zoom,
        follow_playhead,
        None,
        waveforms,
    )
}

/// Draws the timeline with an optional highlighted loop region.
#[allow(clippy::too_many_arguments)]
pub fn show_with_loop(
    ui: &mut egui::Ui,
    song: &Song,
    position: Tick,
    selected: Option<TrackId>,
    zoom: f32,
    follow_playhead: bool,
    loop_region: Option<(Tick, Tick)>,
    waveforms: &HashMap<TrackId, (String, Vec<audio::waveform::Peak>)>,
) -> (RollOutcome, Option<TrackId>, Option<ClipEdit>) {
    let bar = song.time_signature.bar_ticks().max(1);
    let bars = ((song.end_tick().max(bar * 8) + bar - 1) / bar).max(8);
    let width =
        (LABEL_WIDTH + bars as f32 * 100.0 * zoom.clamp(0.5, 3.0)).max(ui.available_width());
    let mut result = (RollOutcome::default(), None, None);
    let drag_id = ui.id().with("timeline-clip-drag");
    let edit_id = ui.id().with("timeline-clip-edit");
    let request = ui
        .ctx()
        .data_mut(|data| data.remove_temp::<f32>(egui::Id::new(SCROLL_REQUEST)));
    let mut area = egui::ScrollArea::both()
        .id_salt("timeline-scroll")
        .auto_shrink([false, false]);
    if let Some(offset) = request {
        area = area.vertical_scroll_offset(offset.max(0.0));
    }
    let output = area.show(ui, |ui| {
        result = draw(
            ui,
            song,
            position,
            selected,
            width,
            drag_id,
            edit_id,
            loop_region,
            waveforms,
        );
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
    let geometry = Geometry {
        top: output.inner_rect.top(),
        bottom: output.inner_rect.bottom(),
        offset: output.state.offset.y,
        pass: ui.ctx().cumulative_pass_nr(),
    };
    let moved = ui.ctx().data_mut(|data| {
        let before = data.get_temp::<Geometry>(egui::Id::new(GEOMETRY));
        data.insert_temp(egui::Id::new(GEOMETRY), geometry);
        before.is_none_or(|before| {
            before.top != geometry.top
                || before.bottom != geometry.bottom
                || before.offset != geometry.offset
        })
    });
    if moved {
        // The track list drew with the old place; draw it again.
        ui.ctx().request_repaint();
    }
    result
}

const GEOMETRY: &str = "timeline-geometry";
const CONTEXT_HIT: &str = "timeline-context-hit";

/// What a right-click on the timeline landed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextTarget {
    /// The chord strip under the ruler.
    Chords,
    /// The ruler.
    Ruler,
    /// A MIDI clip.
    Clip(TrackId, ClipId),
    /// A track's audio.
    Audio(TrackId),
    /// An empty place in a track's lane.
    Lane(TrackId),
    /// Below the last lane.
    Empty,
}

/// A right-click on the timeline, for the caller to show a menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextHit {
    /// What was hit.
    pub target: ContextTarget,
    /// The tick under the pointer.
    pub tick: Tick,
    /// Screen position of the click.
    pub pos: egui::Pos2,
}

/// Takes the right-click the timeline saw since the last call, if any.
#[must_use]
pub fn take_context_hit(context: &egui::Context) -> Option<ContextHit> {
    context.data_mut(|data| {
        let id = egui::Id::new(CONTEXT_HIT);
        let hit = data.get_temp::<ContextHit>(id);
        data.remove::<ContextHit>(id);
        hit
    })
}

/// What sits at `y` (from the timeline's top) and `tick`.
#[must_use]
pub fn context_target(song: &Song, y: f32, tick: Tick) -> ContextTarget {
    if y < RULER_HEIGHT {
        return ContextTarget::Ruler;
    }
    if y < HEADER_HEIGHT {
        return ContextTarget::Chords;
    }
    let index = ((y - HEADER_HEIGHT) / LANE_HEIGHT).floor() as usize;
    let Some(track) = song.tracks.get(index) else {
        return ContextTarget::Empty;
    };
    if let Some(clip) = track.clips.iter().find(|clip| clip.contains(tick)) {
        return ContextTarget::Clip(track.id, clip.id);
    }
    if let Some(audio) = &track.audio
        && tick >= audio.start
        && tick < audio.start + audio.length.max(1)
    {
        return ContextTarget::Audio(track.id);
    }
    ContextTarget::Lane(track.id)
}
const SCROLL_REQUEST: &str = "timeline-scroll-request";

/// Where the timeline lanes sit on screen, so the track list beside it can
/// line its rows up with them, as in v1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Screen top of the visible timeline area.
    pub top: f32,
    /// Screen bottom of the visible timeline area.
    pub bottom: f32,
    /// Vertical scroll offset.
    pub offset: f32,
    /// Pass it was measured in.
    pass: u64,
}

impl Geometry {
    /// Screen top of the first lane.
    #[must_use]
    pub fn lanes_top(&self) -> f32 {
        self.top + HEADER_HEIGHT - self.offset
    }
}

/// The timeline's geometry from this pass or the one before, if it drew.
#[must_use]
pub fn geometry(context: &egui::Context) -> Option<Geometry> {
    let geometry = context.data(|data| data.get_temp::<Geometry>(egui::Id::new(GEOMETRY)))?;
    (geometry.pass + 1 >= context.cumulative_pass_nr()).then_some(geometry)
}

/// Asks the timeline to scroll its lanes to `offset` on the next pass.
pub fn scroll_lanes_to(context: &egui::Context, offset: f32) {
    context.data_mut(|data| data.insert_temp(egui::Id::new(SCROLL_REQUEST), offset));
    context.request_repaint();
}

#[allow(clippy::too_many_arguments)]
fn draw(
    ui: &mut egui::Ui,
    song: &Song,
    position: Tick,
    selected: Option<TrackId>,
    width: f32,
    drag_id: egui::Id,
    edit_id: egui::Id,
    loop_region: Option<(Tick, Tick)>,
    waveforms: &HashMap<TrackId, (String, Vec<audio::waveform::Peak>)>,
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

    let beat = song.time_signature.beat_ticks().max(1);
    if let Some((loop_start, loop_end)) = loop_region
        && loop_end > loop_start
    {
        let left = x_for(loop_start.max(0)).clamp(content.left(), content.right());
        let right = x_for(loop_end.min(end)).clamp(content.left(), content.right());
        if right > left {
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(left, rect.top()),
                    pos2(right, rect.top() + RULER_HEIGHT),
                ),
                0.0,
                theme::accent().gamma_multiply(0.20),
            );
            painter.line_segment(
                [pos2(left, rect.top() + 1.0), pos2(right, rect.top() + 1.0)],
                Stroke::new(2.0_f32, theme::accent()),
            );
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(left, rect.top() + RULER_HEIGHT + CHORD_HEIGHT),
                    pos2(right, rect.bottom()),
                ),
                0.0,
                theme::accent().gamma_multiply(0.05),
            );
        }
    }

    let mut tick = 0;
    while tick <= end {
        let x = x_for(tick);
        painter.line_segment(
            [pos2(x, rect.top() + RULER_HEIGHT), pos2(x, rect.bottom())],
            Stroke::new(1.0_f32, theme::grid_bar()),
        );
        let mut beat_tick = tick + beat;
        while beat_tick < tick + bar && beat_tick <= end {
            let bx = x_for(beat_tick);
            painter.line_segment(
                [
                    pos2(bx, rect.top() + RULER_HEIGHT + CHORD_HEIGHT),
                    pos2(bx, rect.bottom()),
                ],
                Stroke::new(1.0_f32, theme::grid_beat()),
            );
            beat_tick += beat;
        }
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
    for marker in &song.markers {
        if marker.tick > end {
            continue;
        }
        let mx = x_for(marker.tick);
        painter.line_segment(
            [pos2(mx, rect.top()), pos2(mx, rect.top() + RULER_HEIGHT)],
            Stroke::new(1.5_f32, theme::accent()),
        );
        if !marker.name.is_empty() {
            painter.text(
                pos2(mx + 4.0, rect.top() + 15.0),
                Align2::LEFT_TOP,
                &marker.name,
                FontId::proportional(9.5),
                theme::accent(),
            );
        }
    }
    let spelling = {
        let key = score::spelling::detect_key(song);
        if key.signature().iter().any(|alter| *alter < 0) {
            music_core::Spelling::Flat
        } else {
            music_core::Spelling::Sharp
        }
    };
    if song.chords.is_empty() {
        painter.text(
            pos2(
                content.left() + 10.0,
                rect.top() + RULER_HEIGHT + CHORD_HEIGHT / 2.0,
            ),
            Align2::LEFT_CENTER,
            "Double-click to type chords (Am F C G or vi IV I V)",
            FontId::proportional(11.5),
            theme::text_dim(),
        );
    }
    for event in &song.chords {
        if event.tick > end {
            continue;
        }
        painter.text(
            pos2(x_for(event.tick) + 3.0, rect.top() + RULER_HEIGHT + 4.0),
            Align2::LEFT_TOP,
            event.chord.format(spelling),
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
        // Arrangement::paint: lanes on the timeline colour, every other one
        // tinted, the selected one lifted.
        painter.rect_filled(
            Rect::from_min_max(
                pos2(content.left(), top),
                pos2(content.right(), top + LANE_HEIGHT),
            ),
            0.0,
            if is_selected {
                theme::raised()
            } else if index % 2 == 1 {
                theme::lane_alt()
            } else {
                theme::timeline()
            },
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
                pos2(x_for(clip.start), top + PART_INSET),
                pos2(
                    x_for(clip.end()).max(x_for(clip.start) + 4.0),
                    top + LANE_HEIGHT - PART_INSET,
                ),
            );
            clip_rects.push((track.id, clip.id, clip_rect, clip.start, clip.length));
        }
        // Parts drawn like Arrangement::drawClip: solid track colour, a faint
        // outline, a thin left accent, the name and the notes in dark ink.
        // A track with notes but no clips shows one part over the whole
        // song, as v1 does after a MIDI import.
        let mut parts: Vec<(Tick, Tick, &str)> = track
            .clips
            .iter()
            .map(|clip| (clip.start, clip.end(), clip.name.as_str()))
            .collect();
        if parts.is_empty() && !track.notes.is_empty() {
            let to = ((song.end_tick() + bar - 1).div_euclid(bar) * bar).max(bar);
            parts.push((0, to, track.name.as_str()));
        }
        let ink = if theme::light() {
            theme::text()
        } else {
            Color32::from_rgb(0x11, 0x14, 0x17)
        };
        for (start, stop, name) in &parts {
            // Arrangement::clipBounds: flush with the lane, 0.25 pt inset.
            let part = Rect::from_min_max(
                pos2(x_for(*start), top + PART_INSET),
                pos2(
                    x_for(*stop).max(x_for(*start) + 4.0),
                    top + LANE_HEIGHT - PART_INSET,
                ),
            );
            painter.rect_filled(part, theme::RADIUS, color);
            painter.rect_stroke(
                part,
                theme::RADIUS,
                Stroke::new(1.0_f32, darken(color, 0.35).gamma_multiply(0.32)),
                StrokeKind::Inside,
            );
            painter.rect_filled(
                Rect::from_min_size(
                    pos2(part.left(), part.top() + 1.0),
                    vec2(2.0, (part.height() - 2.0).max(1.0)),
                ),
                0.0,
                darken(color, 0.2).gamma_multiply(0.6),
            );
            let clipped = painter.with_clip_rect(part);
            clipped.text(
                pos2(part.left() + 8.0, part.top() + 3.0),
                Align2::LEFT_TOP,
                *name,
                crate::theme::semibold(11.0),
                ink.gamma_multiply(0.95),
            );
            let notes: Vec<_> = track
                .notes
                .iter()
                .filter(|note| note.start >= *start && note.start < *stop)
                .collect();
            if let (Some(low), Some(high)) = (
                notes.iter().map(|note| note.pitch.as_u8()).min(),
                notes.iter().map(|note| note.pitch.as_u8()).max(),
            ) {
                let area = Rect::from_min_max(
                    pos2(part.left() + 5.0, part.top() + 17.0),
                    pos2(part.right() - 5.0, part.bottom() - 5.0),
                );
                let range = f32::from((high - low + 1).max(12));
                let row = (area.height() / range).clamp(1.0, 2.0);
                for note in notes {
                    let x1 = x_for(note.start);
                    let x2 = x_for((note.start + note.duration).min(*stop));
                    let y = area.bottom()
                        - (f32::from(note.pitch.as_u8() - low) + 1.0) / range * area.height();
                    clipped.rect_filled(
                        Rect::from_min_size(pos2(x1, y), vec2((x2 - x1 - 1.0).max(1.5), row)),
                        0.0,
                        ink.gamma_multiply(0.88),
                    );
                }
            }
        }
        if let Some(audio) = &track.audio {
            let audio_start = audio.start.max(0);
            let audio_end = (audio_start + audio.length.max(1)).min(end.max(audio_start + 1));
            let block = Rect::from_min_max(
                pos2(x_for(audio_start), top + PART_INSET),
                pos2(
                    x_for(audio_end).max(x_for(audio_start) + 4.0),
                    top + LANE_HEIGHT - PART_INSET,
                ),
            );
            painter.rect_filled(block, 3.0, color.gamma_multiply(0.58));
            painter.rect_stroke(block, 3.0, Stroke::new(1.0_f32, color), StrokeKind::Inside);
            let name = std::path::Path::new(&audio.path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("audio");
            if let Some((_, peaks)) = waveforms.get(&track.id)
                && !peaks.is_empty()
            {
                let mut x = block.left() + 1.0;
                while x < block.right() - 1.0 {
                    let fraction = ((x - block.left()) / block.width().max(1.0)).clamp(0.0, 1.0);
                    let index = (fraction * (peaks.len() - 1) as f32).round() as usize;
                    let amplitude = peaks[index]
                        .min
                        .abs()
                        .max(peaks[index].max.abs())
                        .clamp(0.0, 1.0);
                    let half = amplitude * (block.height() / 2.0 - 3.0);
                    painter.line_segment(
                        [
                            pos2(x, block.center().y - half),
                            pos2(x, block.center().y + half),
                        ],
                        Stroke::new(1.0_f32, theme::text().gamma_multiply(0.45)),
                    );
                    x += 2.0;
                }
            }
            for comp in &track.comps {
                let start = comp.start.max(audio_start);
                let end = comp.start.saturating_add(comp.length).min(audio_end);
                if end <= start {
                    continue;
                }
                let left = x_for(start).clamp(block.left(), block.right());
                let right = x_for(end).clamp(block.left(), block.right());
                painter.line_segment(
                    [
                        pos2(left, block.top() + 1.0),
                        pos2(left, block.bottom() - 1.0),
                    ],
                    Stroke::new(1.5_f32, theme::accent()),
                );
                painter.line_segment(
                    [
                        pos2(right, block.top() + 1.0),
                        pos2(right, block.bottom() - 1.0),
                    ],
                    Stroke::new(1.5_f32, theme::accent()),
                );
            }
            painter.text(
                pos2(block.left() + 6.0, block.center().y),
                Align2::LEFT_CENTER,
                format!("audio · {name}"),
                FontId::proportional(9.0),
                theme::text(),
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
    let playhead_color = theme::playhead();
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(playhead_x - 5.0, rect.top()),
            pos2(playhead_x + 5.0, rect.top()),
            pos2(playhead_x, rect.top() + 8.0),
        ],
        playhead_color,
        Stroke::NONE,
    ));
    painter.line_segment(
        [
            pos2(playhead_x, rect.top()),
            pos2(playhead_x, rect.bottom()),
        ],
        Stroke::new(1.5_f32, playhead_color),
    );
    if response.secondary_clicked()
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
        && pointer.x >= content.left()
    {
        let tick = tick_for(pointer.x);
        let target = context_target(song, pointer.y - rect.top(), tick);
        let hit = ContextHit {
            target,
            tick,
            pos: pointer,
        };
        ui.ctx()
            .data_mut(|data| data.insert_temp(egui::Id::new(CONTEXT_HIT), hit));
    }
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

/// `juce::Colour::darker`: scales the colour towards black.
fn darken(colour: Color32, amount: f32) -> Color32 {
    let keep = 1.0 / (1.0 + amount);
    Color32::from_rgb(
        (f32::from(colour.r()) * keep) as u8,
        (f32::from(colour.g()) * keep) as u8,
        (f32::from(colour.b()) * keep) as u8,
    )
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
    fn higher_pitch_draws_higher_in_lane() {
        let top = 100.0;
        let low_y = note_lane_y(top, 48);
        let mid_y = note_lane_y(top, 60);
        let high_y = note_lane_y(top, 72);
        assert!(high_y < mid_y, "high {high_y} should be above mid {mid_y}");
        assert!(mid_y < low_y, "mid {mid_y} should be above low {low_y}");
    }

    #[test]
    fn a_right_click_finds_what_is_under_it() {
        let song = arranging::demo::demo_song();
        assert_eq!(context_target(&song, 5.0, 0), ContextTarget::Ruler);
        assert_eq!(
            context_target(&song, RULER_HEIGHT + 2.0, 0),
            ContextTarget::Chords
        );
        let first = &song.tracks[0];
        let lane_y = HEADER_HEIGHT + LANE_HEIGHT / 2.0;
        let expected = first
            .clips
            .iter()
            .find(|clip| clip.contains(0))
            .map_or(ContextTarget::Lane(first.id), |clip| {
                ContextTarget::Clip(first.id, clip.id)
            });
        assert_eq!(context_target(&song, lane_y, 0), expected);
        let below = HEADER_HEIGHT + LANE_HEIGHT * (song.tracks.len() as f32 + 1.0);
        assert_eq!(context_target(&song, below, 0), ContextTarget::Empty);
    }

    #[test]
    fn the_timeline_draws_without_panicking() {
        let context = egui::Context::default();
        crate::theme::install(&context);
        let mut ran = false;
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let song = arranging::demo::demo_song();
                let _ = show(ui, &song, 0, None, 1.0, false, &HashMap::new());
                let _ = show_with_loop(
                    ui,
                    &song,
                    960,
                    None,
                    1.0,
                    false,
                    Some((0, 3840)),
                    &HashMap::new(),
                );
                ran = true;
            });
        });
        assert!(ran);
    }
}
