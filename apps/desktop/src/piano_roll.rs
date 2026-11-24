//! Read-only piano roll for the shell.
//!
//! Geometry is a pure value with tests so hit testing and drawing cannot drift
//! apart: both use the same transform (ADR-010's rule for the score applies here
//! too). Editing arrives in a later Stage 6 increment.

use egui::{Pos2, Rect, Ui, Vec2};
use music_core::{Tick, TimeSignature};
use project::TrackId;
use project::song::Clip;
use project::song::{Song, Track};

use crate::theme;

/// Height of the clip lane above the notes.
pub const CLIP_LANE_HEIGHT: f32 = 18.0;
/// Height of the chord lane under the clip lane.
pub const CHORD_LANE_HEIGHT: f32 = 16.0;
/// Height of the ruler above the lanes.
pub const RULER_HEIGHT: f32 = 18.0;
/// Height of the lyric lane under the chord lane.
pub const LYRIC_LANE_HEIGHT: f32 = 14.0;

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

fn geometry_hint(state: &RollState, song: &Song, rect: Rect) -> RollGeometry {
    RollGeometry {
        origin: Pos2::new(
            rect.left() + 48.0,
            rect.top()
                + RULER_HEIGHT
                + CLIP_LANE_HEIGHT
                + CHORD_LANE_HEIGHT
                + LYRIC_LANE_HEIGHT
                + 6.0,
        ),
        ticks_per_point: state.ticks_per_point,
        row_height: 10.0,
        top_pitch: state.top_pitch,
        meter: song.time_signature,
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
    /// Tick under the pointer last frame, when it was over the roll.
    pub hover: Option<Tick>,
    /// Snap drags and insertions to a sixteenth.
    pub snap: bool,
    /// Tick band from the last marquee drag, for the app to apply.
    pub band: Option<(Tick, Tick)>,
    drag: Option<NoteDrag>,
    marquee: Option<Pos2>,
}

impl Default for RollState {
    fn default() -> Self {
        Self {
            ticks_per_point: 12.0,
            top_pitch: 96,
            selected: None,
            hover: None,
            snap: true,
            band: None,
            drag: None,
            marquee: None,
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
    /// Clip that was hit, if any.
    pub clip: Option<(TrackId, project::ClipId)>,
    /// True when Shift was held at the click.
    pub shift: bool,
}

/// What a drag on a note does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragMode {
    /// Moves start and pitch.
    Move,
    /// Changes duration.
    Resize,
}

/// A note change produced by a drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RollEdit {
    /// Track holding the note.
    pub track: TrackId,
    /// Note being changed.
    pub note: project::NoteId,
    /// New start.
    pub start: Tick,
    /// New pitch.
    pub pitch: u8,
    /// New duration.
    pub duration: Tick,
    /// What the drag did.
    pub mode: DragMode,
}

/// What the roll produced this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RollOutcome {
    /// Primary click, when one happened.
    pub click: Option<RollClick>,
    /// Note change from a finished drag.
    pub edit: Option<RollEdit>,
    /// Pitch clicked on the keyboard strip, for audition.
    pub preview: Option<u8>,
    /// Velocity change clicked in the lane.
    pub velocity: Option<(TrackId, project::NoteId, u8)>,
    /// Chord tick clicked in the chord lane.
    pub chord: Option<Tick>,
    /// Tick clicked on the ruler, for seeking.
    pub seek: Option<Tick>,
    /// Lyric tick clicked in the lyric lane.
    pub lyric: Option<Tick>,
}

#[derive(Clone, Copy, Debug)]
struct NoteDrag {
    track: TrackId,
    note: project::NoteId,
    mode: DragMode,
    anchor: Pos2,
    start: Tick,
    pitch: u8,
    duration: Tick,
}

/// Height of the velocity lane below the roll.
pub const VELOCITY_LANE_HEIGHT: f32 = 44.0;

/// Velocity from a y position inside the lane: top is 127, bottom is 1.
#[must_use]
pub fn velocity_at(lane_top: f32, lane_height: f32, y: f32) -> u8 {
    let ratio = ((lane_top + lane_height - y) / lane_height.max(1.0)).clamp(0.0, 1.0);
    (1.0 + ratio * 126.0).round() as u8
}

/// The note whose span covers a tick, else the nearest note by start.
#[must_use]
pub fn note_at_tick(track: &Track, tick: Tick) -> Option<project::NoteId> {
    track
        .notes
        .iter()
        .find(|note| tick >= note.start && tick < note.start + note.duration.max(1))
        .or_else(|| {
            track
                .notes
                .iter()
                .min_by_key(|note| (note.start - tick).abs())
        })
        .map(|note| note.id)
}

/// The note under a pointer on the selected track, with its drag mode.
#[must_use]
pub fn hit_note(
    geometry: &RollGeometry,
    track: &Track,
    pointer: Pos2,
) -> Option<(project::NoteId, DragMode)> {
    for note in track.notes.iter().rev() {
        let rect = geometry.note_rect(note.start, note.duration, note.pitch.as_u8());
        if rect.contains(pointer) {
            let mode = if pointer.x > rect.right() - 6.0 {
                DragMode::Resize
            } else {
                DragMode::Move
            };
            return Some((note.id, mode));
        }
    }
    None
}

/// The note values a drag starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragOrigin {
    /// Original start.
    pub start: Tick,
    /// Original pitch.
    pub pitch: u8,
    /// Original duration.
    pub duration: Tick,
}

/// Applies a pointer delta to a note, snapping starts to a 1/16 grid.
#[must_use]
pub fn drag_result(
    mode: DragMode,
    anchor: Pos2,
    pointer: Pos2,
    ticks_per_point: f64,
    row_height: f32,
    origin: DragOrigin,
    snap: bool,
) -> (Tick, u8, Tick) {
    let delta_ticks = f64::from(pointer.x - anchor.x) * ticks_per_point;
    let quantise = |value: f64| -> Tick {
        if !snap {
            return value.round().max(0.0) as Tick;
        }
        let grid = (music_core::PPQ / 4).max(1);
        let snapped = (value / grid as f64).round() as Tick * grid;
        snapped.max(0)
    };
    match mode {
        DragMode::Move => {
            let rows = ((anchor.y - pointer.y) / row_height.max(1.0)).round() as i32;
            let new_pitch = (i32::from(origin.pitch) + rows).clamp(0, 127) as u8;
            (
                quantise(origin.start as f64 + delta_ticks),
                new_pitch,
                origin.duration,
            )
        }
        DragMode::Resize => (
            origin.start,
            origin.pitch,
            quantise(origin.duration as f64 + delta_ticks).max(music_core::PPQ / 8),
        ),
    }
}

/// Draws the roll and handles zoom, scroll and note drags.
pub fn show(
    ui: &mut Ui,
    song: &Song,
    state: &mut RollState,
    playhead: Tick,
    selected_clips: &[(TrackId, project::ClipId)],
) -> RollOutcome {
    let available = ui.available_size();
    let main_height = (available.y - VELOCITY_LANE_HEIGHT).max(60.0);
    let (response, painter) = ui.allocate_painter(
        egui::Vec2::new(available.x, main_height),
        egui::Sense::click_and_drag(),
    );
    let rect = response.rect;
    let lane = Rect::from_min_size(
        Pos2::new(rect.left(), rect.bottom()),
        Vec2::new(rect.width(), VELOCITY_LANE_HEIGHT),
    );
    let (lane_response, lane_painter) =
        ui.allocate_painter(lane.size(), egui::Sense::click_and_drag());
    let lane = lane_response.rect;
    lane_painter.rect_filled(lane, 0.0, theme::lane());
    lane_painter.line_segment(
        [lane.left_top(), lane.right_top()],
        egui::Stroke::new(1.0_f32, theme::line()),
    );
    lane_painter.text(
        Pos2::new(lane.left() + 6.0, lane.center().y),
        egui::Align2::LEFT_CENTER,
        "Velocity",
        egui::FontId::proportional(9.5),
        theme::text_dim(),
    );
    if let Some(track_id) = state.selected
        && let Some(track) = song.tracks.iter().find(|track| track.id == track_id)
    {
        for note in &track.notes {
            let x = geometry_hint(state, song, rect).x_at(note.start);
            if x < lane.left() + 48.0 || x > lane.right() {
                continue;
            }
            let height = f32::from(note.velocity.as_u8()) / 127.0 * (lane.height() - 6.0);
            lane_painter.line_segment(
                [
                    Pos2::new(x, lane.bottom() - 2.0),
                    Pos2::new(x, lane.bottom() - 2.0 - height),
                ],
                egui::Stroke::new(2.0_f32, theme::accent()),
            );
        }
    }
    let velocity = if lane_response.dragged() || lane_response.clicked() {
        lane_response.interact_pointer_pos().and_then(|pointer| {
            let track_id = state.selected?;
            let track = song.tracks.iter().find(|track| track.id == track_id)?;
            let geometry = geometry_hint(state, song, rect);
            let tick = geometry.tick_at(pointer.x);
            let note = note_at_tick(track, tick)?;
            Some((
                track_id,
                note,
                velocity_at(lane.top(), lane.height(), pointer.y),
            ))
        })
    } else {
        None
    };

    let clip_lane = Rect::from_min_size(
        Pos2::new(rect.left(), rect.top() + RULER_HEIGHT),
        Vec2::new(rect.width(), CLIP_LANE_HEIGHT),
    );
    if response.drag_started()
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(track_id) = state.selected
        && let Some(track) = song.tracks.iter().find(|track| track.id == track_id)
        && let Some((note, mode)) = hit_note(&geometry_hint(state, song, rect), track, pointer)
        && let Some(found) = track.notes.iter().find(|entry| entry.id == note)
    {
        state.drag = Some(NoteDrag {
            track: track_id,
            note,
            mode,
            anchor: pointer,
            start: found.start,
            pitch: found.pitch.as_u8(),
            duration: found.duration,
        });
    }
    if response.drag_started()
        && clip_lane.contains(response.interact_pointer_pos().unwrap_or(Pos2::ZERO))
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(track_id) = state.selected
        && let Some(track) = song.tracks.iter().find(|track| track.id == track_id)
        && !track
            .clips
            .iter()
            .any(|clip| clip_rect(rect, &geometry_hint(state, song, rect), clip).contains(pointer))
    {
        state.marquee = Some(pointer);
    }
    if response.dragged() && state.drag.is_none() && state.marquee.is_none() {
        state.ticks_per_point *= 1.0 + response.drag_delta().x * 0.001;
        let scroll = response.drag_delta().y / theme::GRID;
        state.top_pitch = u8::try_from((i32::from(state.top_pitch) + scroll as i32).clamp(12, 127))
            .unwrap_or(state.top_pitch);
        state.clamp();
    }
    if response.drag_stopped()
        && let Some(start) = state.marquee.take()
    {
        let now = response.interact_pointer_pos().unwrap_or(start);
        let geometry = geometry_hint(state, song, rect);
        let first = geometry.tick_at(start.x.min(now.x)).max(0);
        let last = geometry.tick_at(start.x.max(now.x)).max(0);
        state.band = Some((first.min(last), first.max(last)));
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
        origin: Pos2::new(
            rect.left() + 48.0,
            rect.top()
                + RULER_HEIGHT
                + CLIP_LANE_HEIGHT
                + CHORD_LANE_HEIGHT
                + LYRIC_LANE_HEIGHT
                + 6.0,
        ),
        ticks_per_point: state.ticks_per_point,
        row_height: 10.0,
        top_pitch: state.top_pitch,
        meter: song.time_signature,
    };

    painter.rect_filled(rect, 0.0, theme::timeline());
    draw_ruler(&painter, rect, &geometry, song);
    let content = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + RULER_HEIGHT), rect.max);
    if let Some(start) = state.marquee
        && let Some(now) = response.interact_pointer_pos()
    {
        let band = Rect::from_two_pos(start, now).intersect(clip_lane);
        painter.rect_filled(band, 0.0, theme::accent().gamma_multiply(0.15));
        painter.rect_stroke(
            band,
            0.0,
            egui::Stroke::new(1.0_f32, theme::accent()),
            egui::StrokeKind::Inside,
        );
    }
    draw_clip_lane(
        &painter,
        content,
        &geometry,
        song,
        state.selected,
        selected_clips,
    );
    draw_chord_lane(&painter, content, &geometry, song);
    draw_lyric_lane(&painter, content, &geometry, song);
    draw_grid(&painter, content, &geometry, song);
    for (index, track) in song.tracks.iter().enumerate() {
        let colour = theme::track_colour(index);
        let selected = state.selected == Some(track.id);
        draw_track(&painter, content, &geometry, track, colour, selected);
    }
    draw_playhead(&painter, content, &geometry, playhead);
    draw_keys(&painter, content, &geometry);
    draw_pitch_guide(&painter, content, &geometry);

    if response.drag_stopped()
        && let Some(drag) = state.drag.take()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let (start, pitch, duration) = drag_result(
            drag.mode,
            drag.anchor,
            pointer,
            f64::from(state.ticks_per_point),
            10.0,
            DragOrigin {
                start: drag.start,
                pitch: drag.pitch,
                duration: drag.duration,
            },
            state.snap,
        );
        return RollOutcome {
            edit: Some(RollEdit {
                track: drag.track,
                note: drag.note,
                start,
                pitch,
                duration,
                mode: drag.mode,
            }),
            ..RollOutcome::default()
        };
    }
    if velocity.is_some() {
        return RollOutcome {
            velocity,
            ..RollOutcome::default()
        };
    }
    let Some(pointer) = response.interact_pointer_pos() else {
        return RollOutcome::default();
    };
    if response.clicked() && pointer.y < rect.top() + RULER_HEIGHT {
        return RollOutcome {
            seek: Some(geometry.tick_at(pointer.x).max(0)),
            ..RollOutcome::default()
        };
    }
    if response.clicked() && pointer.x < geometry.origin.x && pointer.y >= geometry.origin.y {
        return RollOutcome {
            preview: Some(geometry.pitch_at(pointer.y)),
            ..RollOutcome::default()
        };
    }
    if response.clicked()
        && pointer.y >= content.top() + CLIP_LANE_HEIGHT + CHORD_LANE_HEIGHT
        && pointer.y < content.top() + CLIP_LANE_HEIGHT + CHORD_LANE_HEIGHT + LYRIC_LANE_HEIGHT
    {
        let tick = geometry.tick_at(pointer.x).max(0);
        let nearest = song
            .lyrics
            .iter()
            .min_by_key(|event| (event.tick - tick).abs())
            .map(|event| event.tick);
        return RollOutcome {
            lyric: nearest,
            ..RollOutcome::default()
        };
    }
    if response.clicked()
        && pointer.y >= content.top() + CLIP_LANE_HEIGHT
        && pointer.y < content.top() + CLIP_LANE_HEIGHT + CHORD_LANE_HEIGHT
    {
        let tick = geometry.tick_at(pointer.x).max(0);
        let nearest = song
            .chords
            .iter()
            .min_by_key(|event| (event.tick - tick).abs())
            .map(|event| event.tick);
        return RollOutcome {
            chord: nearest,
            ..RollOutcome::default()
        };
    }
    if !response.clicked() {
        return RollOutcome::default();
    }
    let tick = geometry.tick_at(pointer.x).max(0);
    let pitch = geometry.pitch_at(pointer.y);
    let mut click = RollClick {
        tick,
        pitch,
        note: None,
        clip: None,
        shift: false,
    };
    if let Some(track_id) = state.selected
        && let Some(track) = song.tracks.iter().find(|track| track.id == track_id)
    {
        for clip in &track.clips {
            if clip_rect(rect, &geometry, clip).contains(pointer) {
                return RollOutcome {
                    click: Some(RollClick {
                        tick,
                        pitch,
                        note: None,
                        clip: Some((track.id, clip.id)),
                        shift: ui.input(|input| input.modifiers.shift),
                    }),
                    edit: None,
                    velocity: None,
                    preview: None,
                    chord: None,
                    seek: None,
                    lyric: None,
                };
            }
        }
    }
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
                return RollOutcome {
                    click: Some(RollClick {
                        tick,
                        pitch: note.pitch.as_u8(),
                        note: Some((track.id, note.id)),
                        clip: None,
                        shift: false,
                    }),
                    edit: None,
                    velocity: None,
                    preview: None,
                    chord: None,
                    seek: None,
                    lyric: None,
                };
            }
        }
    }
    click.note = None;
    click.clip = None;
    RollOutcome {
        click: Some(click),
        edit: None,
        velocity,
        preview: None,
        chord: None,
        seek: None,
        lyric: None,
    }
}

fn draw_ruler(painter: &egui::Painter, rect: Rect, geometry: &RollGeometry, song: &Song) {
    let ruler = Rect::from_min_size(rect.min, Vec2::new(rect.width(), RULER_HEIGHT));
    painter.rect_filled(ruler, 0.0, theme::panel());
    let bar = song.time_signature.bar_ticks().max(1);
    let first = geometry.tick_at(rect.left()).div_euclid(bar);
    let last = geometry.tick_at(rect.right()).div_euclid(bar);
    for index in first..=last {
        let x = geometry.x_at(index * bar);
        if x < rect.left() || x > rect.right() {
            continue;
        }
        painter.line_segment(
            [
                Pos2::new(x, ruler.bottom() - 5.0),
                Pos2::new(x, ruler.bottom()),
            ],
            egui::Stroke::new(0.8_f32, theme::grid_bar()),
        );
        painter.text(
            Pos2::new(x + 3.0, ruler.top() + 2.0),
            egui::Align2::LEFT_TOP,
            format!("{}", index + 1),
            egui::FontId::proportional(9.0),
            theme::text_dim(),
        );
    }
    for marker in &song.markers {
        let x = geometry.x_at(marker.tick);
        if x < rect.left() || x > rect.right() - 4.0 {
            continue;
        }
        painter.line_segment(
            [Pos2::new(x, ruler.top()), Pos2::new(x, ruler.bottom())],
            egui::Stroke::new(1.4_f32, theme::accent()),
        );
        painter.text(
            Pos2::new(x + 2.0, ruler.bottom() - 2.0),
            egui::Align2::LEFT_BOTTOM,
            &marker.name,
            egui::FontId::proportional(8.0),
            theme::accent(),
        );
    }
}

fn draw_grid(painter: &egui::Painter, rect: Rect, geometry: &RollGeometry, song: &Song) {
    let grid_top = geometry.origin.y.max(rect.top());
    let grid_left = geometry.origin.x.max(rect.left());
    let count = ((rect.bottom() - grid_top) / geometry.row_height).ceil() as i32;
    for offset in 0..count.clamp(0, 128) {
        let pitch = i32::from(geometry.top_pitch) - offset;
        if !(0..=127).contains(&pitch) {
            continue;
        }
        let y = geometry.y_at(pitch as u8);
        if y >= rect.bottom() {
            break;
        }
        if matches!(pitch % 12, 1 | 3 | 6 | 8 | 10) {
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(grid_left, y),
                    Pos2::new(rect.right(), (y + geometry.row_height).min(rect.bottom())),
                ),
                0.0,
                theme::lane_alt().gamma_multiply(0.75),
            );
        }
        if pitch % 12 == 0 {
            let line_y = (y + geometry.row_height).min(rect.bottom());
            painter.line_segment(
                [
                    Pos2::new(grid_left, line_y),
                    Pos2::new(rect.right(), line_y),
                ],
                egui::Stroke::new(1.0_f32, theme::line()),
            );
        }
    }
    let bar = song.time_signature.bar_ticks().max(1);
    let beat = song.time_signature.beat_ticks().max(1);
    let first_bar = geometry.tick_at(grid_left).div_euclid(bar);
    let last_bar = geometry.tick_at(rect.right()).div_euclid(bar);
    for index in first_bar..=last_bar {
        let tick = index * bar;
        let x = geometry.x_at(tick);
        if x >= grid_left && x <= rect.right() {
            painter.line_segment(
                [Pos2::new(x, grid_top), Pos2::new(x, rect.bottom())],
                egui::Stroke::new(1.0_f32, theme::grid_bar()),
            );
        }
        for beat_index in 1..i64::from(song.time_signature.numerator) {
            let beat_x = geometry.x_at(tick + beat_index * beat);
            if beat_x > grid_left && beat_x < rect.right() {
                painter.line_segment(
                    [
                        Pos2::new(beat_x, grid_top),
                        Pos2::new(beat_x, rect.bottom()),
                    ],
                    egui::Stroke::new(1.0_f32, theme::grid_beat()),
                );
            }
        }
    }
}

/// Clips intersecting a tick band, for marquee selection.
#[must_use]
pub fn clips_in_band(track: &Track, first: Tick, last: Tick) -> Vec<project::ClipId> {
    let low = first.min(last);
    let high = first.max(last);
    track
        .clips
        .iter()
        .filter(|clip| clip.start < high && clip.end() > low)
        .map(|clip| clip.id)
        .collect()
}

/// Rectangle of one clip in the lane above the notes.
#[must_use]
pub fn clip_rect(rect: Rect, geometry: &RollGeometry, clip: &Clip) -> Rect {
    let left = geometry.x_at(clip.start).max(rect.left());
    let right = geometry.x_at(clip.end()).min(rect.right());
    Rect::from_min_max(
        Pos2::new(left, rect.top() + 3.0),
        Pos2::new(right.max(left + 2.0), rect.top() + CLIP_LANE_HEIGHT - 2.0),
    )
}

fn draw_clip_lane(
    painter: &egui::Painter,
    rect: Rect,
    geometry: &RollGeometry,
    song: &Song,
    selected_track: Option<TrackId>,
    selected_clips: &[(TrackId, project::ClipId)],
) {
    let Some(track) = selected_track.and_then(|id| song.tracks.iter().find(|track| track.id == id))
    else {
        return;
    };
    painter.rect_filled(
        Rect::from_min_size(rect.min, Vec2::new(rect.width(), CLIP_LANE_HEIGHT)),
        0.0,
        theme::lane_alt(),
    );
    for clip in &track.clips {
        let clip_rect = clip_rect(rect, geometry, clip);
        let selected = selected_clips.contains(&(track.id, clip.id));
        painter.rect_filled(
            clip_rect,
            0.0,
            theme::accent().gamma_multiply(if selected { 0.35 } else { 0.18 }),
        );
        if selected {
            painter.rect_stroke(
                clip_rect,
                0.0,
                egui::Stroke::new(1.0_f32, theme::accent()),
                egui::StrokeKind::Inside,
            );
        }
        if clip_rect.width() > 28.0 {
            painter.text(
                Pos2::new(clip_rect.left() + 4.0, clip_rect.center().y),
                egui::Align2::LEFT_CENTER,
                &clip.name,
                egui::FontId::proportional(9.0),
                theme::text_muted(),
            );
        }
    }
}

fn draw_lyric_lane(painter: &egui::Painter, rect: Rect, geometry: &RollGeometry, song: &Song) {
    let top = rect.top() + CLIP_LANE_HEIGHT + CHORD_LANE_HEIGHT;
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.left(), top),
            Vec2::new(rect.width(), LYRIC_LANE_HEIGHT),
        ),
        0.0,
        theme::raised(),
    );
    for event in &song.lyrics {
        let x = geometry.x_at(event.tick);
        if x < rect.left() || x > rect.right() - 8.0 {
            continue;
        }
        painter.text(
            Pos2::new(x + 2.0, top + LYRIC_LANE_HEIGHT / 2.0),
            egui::Align2::LEFT_CENTER,
            &event.text,
            egui::FontId::proportional(9.0),
            theme::text_muted(),
        );
    }
}

fn draw_chord_lane(painter: &egui::Painter, rect: Rect, geometry: &RollGeometry, song: &Song) {
    let top = rect.top() + CLIP_LANE_HEIGHT;
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.left(), top),
            Vec2::new(rect.width(), CHORD_LANE_HEIGHT),
        ),
        0.0,
        theme::panel(),
    );
    let spelling = {
        let key = score::spelling::detect_key(song);
        if key.signature().iter().any(|alter| *alter < 0) {
            music_core::Spelling::Flat
        } else {
            music_core::Spelling::Sharp
        }
    };
    for event in &song.chords {
        let x = geometry.x_at(event.tick);
        if x < rect.left() || x > rect.right() - 6.0 {
            continue;
        }
        painter.text(
            Pos2::new(x + 4.0, top + CHORD_LANE_HEIGHT * 0.5),
            egui::Align2::LEFT_CENTER,
            event.chord.format(spelling),
            egui::FontId::proportional(10.0),
            theme::accent(),
        );
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
    let base_alpha = if selected { 1.0_f32 } else { 0.65_f32 };
    for note in &track.notes {
        let note_rect = geometry.note_rect(note.start, note.duration, note.pitch.as_u8());
        if !rect.intersects(note_rect) {
            continue;
        }
        let vel_shade = 0.55 + 0.45 * f32::from(note.velocity.as_u8()) / 127.0;
        let fill = colour.gamma_multiply((base_alpha * vel_shade).clamp(0.25, 1.0));
        painter.rect_filled(note_rect, 0.0, fill);
        if selected {
            painter.rect_stroke(
                note_rect,
                0.0,
                egui::Stroke::new(1.0_f32, theme::text()),
                egui::StrokeKind::Inside,
            );
        }
    }
}

fn draw_playhead(painter: &egui::Painter, rect: Rect, geometry: &RollGeometry, tick: Tick) {
    let x = geometry.x_at(tick);
    if x >= rect.left() && x <= rect.right() {
        let color = theme::playhead();
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(x - 4.0, rect.top()),
                Pos2::new(x + 4.0, rect.top()),
                Pos2::new(x, rect.top() + 6.0),
            ],
            color,
            egui::Stroke::NONE,
        ));
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            egui::Stroke::new(1.5_f32, color),
        );
    }
}

/// Rectangle for one piano key aligned to its pitch row in `geometry`.
#[must_use]
pub fn key_row_rect(left: f32, geometry: &RollGeometry, pitch: u8) -> Rect {
    let black = matches!(pitch % 12, 1 | 3 | 6 | 8 | 10);
    let width = if black { 30.0 } else { 46.0 };
    Rect::from_min_size(
        Pos2::new(left, geometry.y_at(pitch)),
        Vec2::new(width, (geometry.row_height - 1.0).max(1.0)),
    )
}

fn draw_keys(painter: &egui::Painter, rect: Rect, geometry: &RollGeometry) {
    let keys_top = geometry.origin.y.max(rect.top());
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(rect.left(), keys_top),
            Pos2::new(geometry.origin.x, rect.bottom()),
        ),
        0.0,
        theme::raised(),
    );
    let top = geometry.top_pitch;
    let count = ((rect.bottom() - keys_top) / geometry.row_height).ceil() as i32 + 1;
    for offset in 0..count.clamp(1, 128) {
        let pitch = i32::from(top) - offset;
        if !(0..=127).contains(&pitch) {
            continue;
        }
        let pitch_u8 = pitch as u8;
        let key_rect = key_row_rect(rect.left(), geometry, pitch_u8);
        if key_rect.top() < keys_top || key_rect.top() >= rect.bottom() {
            continue;
        }
        let black = matches!(pitch_u8 % 12, 1 | 3 | 6 | 8 | 10);
        let colour = if black {
            egui::Color32::from_rgb(0x2A, 0x26, 0x22)
        } else if theme::light() {
            egui::Color32::from_rgb(0xFF, 0xFF, 0xFF)
        } else {
            egui::Color32::from_rgb(0xD8, 0xD2, 0xC6)
        };
        painter.rect_filled(key_rect, 0.0, colour);
    }
    painter.line_segment(
        [
            Pos2::new(geometry.origin.x, keys_top),
            Pos2::new(geometry.origin.x, rect.bottom()),
        ],
        egui::Stroke::new(1.0_f32, theme::line()),
    );
}

fn draw_pitch_guide(painter: &egui::Painter, rect: Rect, geometry: &RollGeometry) {
    let keys_top = geometry.origin.y.max(rect.top());
    for octave in 0..=8_i32 {
        let pitch = (octave + 1) * 12;
        if !(0..=127).contains(&pitch) {
            continue;
        }
        let y = geometry.y_at(pitch as u8);
        if y >= keys_top && y + geometry.row_height <= rect.bottom() {
            painter.text(
                Pos2::new(geometry.origin.x - 5.0, y + geometry.row_height * 0.5),
                egui::Align2::RIGHT_CENTER,
                format!("C{octave}"),
                egui::FontId::proportional(9.0),
                egui::Color32::from_rgb(0x4A, 0x44, 0x3B),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_band_selects_intersecting_clips() {
        use super::*;
        use music_core::PPQ;
        use project::song::Clip;
        use project::{ClipId, TrackId};
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Clips");
        for (id, start) in [(1, 0), (2, PPQ * 4)] {
            track.clips.push(Clip {
                id: ClipId::new(id).expect("valid"),
                name: format!("Clip {id}"),
                start,
                length: PPQ * 4,
            });
        }
        assert_eq!(clips_in_band(&track, PPQ * 2, PPQ * 6).len(), 2);
        assert_eq!(clips_in_band(&track, 0, PPQ).len(), 1);
        assert!(clips_in_band(&track, PPQ * 10, PPQ * 12).is_empty());
    }

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
    fn clips_map_to_the_lane() {
        let geometry = RollGeometry {
            origin: Pos2::new(48.0, 20.0),
            ticks_per_point: 10.0,
            row_height: 10.0,
            top_pitch: 96,
            meter: TimeSignature::default(),
        };
        let canvas = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 400.0));
        let clip = project::song::Clip {
            id: project::ClipId::new(1).expect("valid"),
            name: String::from("A"),
            start: 0,
            length: 1920,
        };
        let lane = clip_rect(canvas, &geometry, &clip);
        assert!((lane.left() - 48.0).abs() < 0.5);
        assert!((lane.width() - 192.0).abs() < 0.5);
        assert!(lane.top() >= canvas.top());
        assert!(lane.bottom() <= canvas.top() + CLIP_LANE_HEIGHT);
        let later = clip_rect(
            canvas,
            &geometry,
            &project::song::Clip {
                start: 1920,
                length: 1920,
                ..clip
            },
        );
        assert!(later.left() > lane.left());
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

    #[test]
    fn piano_keys_align_with_note_rows() {
        let geometry = geometry();
        for pitch in [36_u8, 48, 60, 61, 72, 84] {
            let key = key_row_rect(0.0, &geometry, pitch);
            let note = geometry.note_rect(0, 480, pitch);
            assert!(
                (key.top() - geometry.y_at(pitch)).abs() < f32::EPSILON,
                "key for pitch {pitch} must start at geometry.y_at"
            );
            assert!(
                note.top() >= key.top() && note.bottom() <= key.top() + geometry.row_height,
                "note for pitch {pitch} must lie inside its key row"
            );
        }
    }

    #[test]
    fn piano_roll_draws_without_panicking() {
        let context = egui::Context::default();
        let song = arranging::demo::demo_song();
        let mut state = RollState {
            selected: song.tracks.first().map(|track| track.id),
            ..RollState::default()
        };
        let mut ran = false;
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let _ = show(ui, &song, &mut state, 960, &[]);
                ran = true;
            });
        });
        assert!(ran);
    }
}

#[cfg(test)]
mod drag_tests {
    use super::*;
    use music_core::{Channel, Pitch, Velocity};
    use project::song::{Note, Track};
    use project::{NoteId, TrackId};

    fn geometry() -> RollGeometry {
        RollGeometry {
            origin: Pos2::new(100.0, 50.0),
            ticks_per_point: 10.0,
            row_height: 10.0,
            top_pitch: 96,
            meter: music_core::TimeSignature::new(4, 4).expect("valid"),
        }
    }

    fn track() -> Track {
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 200,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        track
    }

    #[test]
    fn the_right_edge_resizes_and_the_body_moves() {
        let geometry = geometry();
        let track = track();
        let rect = geometry.note_rect(0, 200, 60);
        assert_eq!(
            hit_note(
                &geometry,
                &track,
                Pos2::new(rect.left() + 2.0, rect.center().y)
            ),
            Some((NoteId::new(1).expect("valid"), DragMode::Move))
        );
        assert_eq!(
            hit_note(
                &geometry,
                &track,
                Pos2::new(rect.right() - 2.0, rect.center().y)
            ),
            Some((NoteId::new(1).expect("valid"), DragMode::Resize))
        );
        assert_eq!(
            hit_note(
                &geometry,
                &track,
                Pos2::new(rect.right() + 50.0, rect.center().y)
            ),
            None
        );
    }

    #[test]
    fn drag_math_snaps_and_clamps() {
        let origin = DragOrigin {
            start: 0,
            pitch: 60,
            duration: 200,
        };
        let anchor = Pos2::new(0.0, 0.0);
        let moved = drag_result(
            DragMode::Move,
            anchor,
            Pos2::new(24.0, -20.0),
            10.0,
            10.0,
            origin,
            true,
        );
        assert_eq!(moved, (240, 62, 200));
        let resized = drag_result(
            DragMode::Resize,
            anchor,
            Pos2::new(28.0, 0.0),
            10.0,
            10.0,
            origin,
            true,
        );
        assert_eq!(resized, (0, 60, 480));
        let tiny = drag_result(
            DragMode::Resize,
            anchor,
            Pos2::new(-1000.0, 0.0),
            10.0,
            10.0,
            origin,
            true,
        );
        assert_eq!(tiny.2, music_core::PPQ / 8);
        let free = drag_result(
            DragMode::Move,
            anchor,
            Pos2::new(1.3, 0.0),
            10.0,
            10.0,
            origin,
            false,
        );
        assert_eq!(free.0, 13);
    }
}

#[cfg(test)]
mod lane_tests {
    use super::*;
    use music_core::{Channel, Pitch, Velocity};
    use project::song::{Note, Track};
    use project::{NoteId, TrackId};

    #[test]
    fn velocity_maps_from_the_lane_edges() {
        assert_eq!(velocity_at(0.0, 40.0, 0.0), 127);
        assert_eq!(velocity_at(0.0, 40.0, 40.0), 1);
        assert_eq!(velocity_at(0.0, 40.0, 20.0), 64);
        assert_eq!(velocity_at(0.0, 40.0, 999.0), 1, "below the lane clamps");
    }

    #[test]
    fn the_note_under_a_tick_is_found_or_nearest() {
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 200,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        assert_eq!(
            note_at_tick(&track, 100),
            Some(NoteId::new(1).expect("valid"))
        );
        assert_eq!(
            note_at_tick(&track, 5000),
            Some(NoteId::new(1).expect("valid"))
        );
        let empty = Track::new(TrackId::new(2).expect("valid"), "Empty");
        assert!(note_at_tick(&empty, 0).is_none());
    }
}
