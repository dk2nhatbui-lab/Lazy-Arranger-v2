//! The score canvas: the first renderer over `score::Engraved` (ADR-010).
//!
//! Draws page, staff lines, barlines and noteheads from the layout only; it
//! makes no musical decisions, so a PDF renderer can reuse the same layout.

use egui::{Color32, Pos2, Rect, Stroke, Ui, Vec2};
use music_core::Tick;
use score::{Engraved, ScoreHit, Settings};

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

/// What a score click produced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScoreOutcome {
    /// Insertion position when empty staff was clicked.
    pub insert: Option<ScoreHit>,
    /// Existing note (tick, pitch) when a notehead was clicked.
    pub note: Option<(Tick, u8)>,
    /// A finished drag from one note position to another.
    pub drag: Option<ScoreDrag>,
    /// A right-click, for the app's menu; `note` then holds the notehead
    /// under the pointer, if any.
    pub menu: bool,
}

/// A drag from one score position to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoreDrag {
    /// Tick of the grabbed note.
    pub from_tick: Tick,
    /// Pitch of the grabbed note.
    pub from_pitch: u8,
    /// Destination tick.
    pub to_tick: Tick,
    /// Destination pitch.
    pub to_pitch: u8,
    /// True when Shift held at drag start: resize instead of move.
    pub resize: bool,
    /// New duration when resizing; zero otherwise.
    pub duration: Tick,
}

type CeciliaReceiver = std::sync::mpsc::Receiver<Result<Vec<score::cecilia::PageImage>, String>>;

/// Persistent score canvas state.
#[derive(Default)]
pub struct ScoreState {
    drag: Option<(Tick, u8, Tick, bool)>,
    clef_treble: Option<egui::TextureHandle>,
    clef_bass: Option<egui::TextureHandle>,
    cecilia_pages: Vec<egui::TextureHandle>,
    cecilia_key: Option<String>,
    cecilia_pending: Option<(String, CeciliaReceiver)>,
    /// Pages produced by the Cecilia preview.
    pub cecilia_page_count: usize,
    /// Last Cecilia preview error, if any.
    pub cecilia_error: Option<String>,
}

impl std::fmt::Debug for ScoreState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScoreState")
            .field("dragging", &self.drag.is_some())
            .field("treble_loaded", &self.clef_treble.is_some())
            .field("bass_loaded", &self.clef_bass.is_some())
            .field("cecilia_pages", &self.cecilia_pages.len())
            .finish()
    }
}

fn cecilia_cache_key(
    song: &project::Song,
    track_index: usize,
    title: &str,
    grand_staff: bool,
) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    track_index.hash(&mut hasher);
    title.hash(&mut hasher);
    grand_staff.hash(&mut hasher);
    song.end_tick().hash(&mut hasher);
    for change in song.tempo_map.changes() {
        change.tick.hash(&mut hasher);
        change.bpm.to_bits().hash(&mut hasher);
    }
    song.time_signature.numerator.hash(&mut hasher);
    song.time_signature.denominator.hash(&mut hasher);
    song.key
        .map(|key| (key.tonic.as_u8(), key.minor))
        .hash(&mut hasher);
    if let Some(track) = song.tracks.get(track_index) {
        for note in &track.notes {
            note.start.hash(&mut hasher);
            note.duration.hash(&mut hasher);
            note.pitch.as_u8().hash(&mut hasher);
            note.velocity.as_u8().hash(&mut hasher);
            note.channel.zero_based().hash(&mut hasher);
        }
        for slur in &track.slurs {
            slur.start.hash(&mut hasher);
            slur.end.hash(&mut hasher);
        }
    }
    for event in &song.chords {
        event.tick.hash(&mut hasher);
        event.chord.root.as_u8().hash(&mut hasher);
        format!("{:?}", event.chord.quality).hash(&mut hasher);
    }
    for lyric in &song.lyrics {
        lyric.tick.hash(&mut hasher);
        lyric.text.hash(&mut hasher);
    }
    for mark in &song.marks {
        mark.tick.hash(&mut hasher);
        format!("{:?}", mark.kind).hash(&mut hasher);
    }
    format!("{track_index}:{title}:{grand_staff}:{:x}", hasher.finish())
}

impl ScoreState {
    /// Rebuilds the Cecilia preview when the track, title or note count changes.
    pub fn ensure_cecilia(
        &mut self,
        context: &egui::Context,
        song: &project::Song,
        track_index: usize,
        title: &str,
        grand_staff: bool,
    ) {
        let key = cecilia_cache_key(song, track_index, title, grand_staff);
        if self.cecilia_key.as_deref() == Some(key.as_str()) {
            return;
        }
        if let Some((pending_key, receiver)) = &self.cecilia_pending {
            // Keep at most one engraver running. A worker for an older key is
            // still drained, so the latest key starts as soon as it finishes.
            let result = match receiver.try_recv() {
                Ok(result) => result,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(100));
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Err(String::from("the score engraver stopped unexpectedly"))
                }
            };
            let finished_current = pending_key == &key;
            self.cecilia_pending = None;
            if finished_current {
                match result {
                    Ok(pages) => {
                        self.cecilia_pages.clear();
                        for (index, page) in pages.into_iter().enumerate() {
                            self.cecilia_pages.push(context.load_texture(
                                format!("cecilia-page-{index}"),
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [page.width as usize, page.height as usize],
                                    &page.rgba,
                                ),
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                        self.cecilia_page_count = self.cecilia_pages.len();
                        self.cecilia_error = None;
                    }
                    Err(error) => {
                        self.cecilia_page_count = 0;
                        self.cecilia_pages.clear();
                        self.cecilia_error = Some(error);
                    }
                }
                self.cecilia_key = Some(key);
                return;
            }
        }
        let song = song.clone();
        let title = title.to_string();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.cecilia_pending = Some((key, receiver));
        context.request_repaint_after(std::time::Duration::from_millis(100));
        std::thread::spawn(move || {
            let result =
                score::cecilia::render_page_images(&song, track_index, &title, grand_staff, 1.6);
            let _ = sender.send(result);
        });
    }

    /// Forces the next preview to re-engrave.
    pub fn invalidate_cecilia(&mut self) {
        self.cecilia_key = None;
        self.cecilia_pending = None;
    }
}

/// Maps a tick on one preview page to its horizontal screen position.
#[must_use]
pub fn preview_cursor_x(
    engraved: &Engraved,
    page_index: usize,
    tick: Tick,
    page_rect: egui::Rect,
    settings: Settings,
) -> Option<f32> {
    let page = engraved.pages.get(page_index)?;
    for system in &page.systems {
        for measure in &system.measures {
            if tick >= measure.start && tick <= measure.start.saturating_add(measure.length) {
                let ratio = if measure.length > 0 {
                    (tick - measure.start) as f32 / measure.length as f32
                } else {
                    0.0
                };
                let x = measure.x + (measure.x_end - measure.x) * ratio.clamp(0.0, 1.0);
                return Some(page_rect.left() + x / settings.page_width * page_rect.width());
            }
        }
    }
    None
}

/// Cheap content hash for caching an all-track engraving.
#[must_use]
pub fn full_score_hash(song: &project::Song, title: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    song.name.hash(&mut hasher);
    title.hash(&mut hasher);
    song.time_signature.numerator.hash(&mut hasher);
    song.time_signature.denominator.hash(&mut hasher);
    song.key
        .map(|key| (key.tonic.as_u8(), key.minor))
        .hash(&mut hasher);
    song.end_tick().hash(&mut hasher);
    for change in song.tempo_map.changes() {
        change.tick.hash(&mut hasher);
        change.bpm.to_bits().hash(&mut hasher);
    }
    for event in &song.chords {
        event.tick.hash(&mut hasher);
        event.chord.root.as_u8().hash(&mut hasher);
        format!("{:?}", event.chord.quality).hash(&mut hasher);
    }
    for mark in &song.marks {
        mark.tick.hash(&mut hasher);
        format!("{:?}", mark.kind).hash(&mut hasher);
    }
    for lyric in &song.lyrics {
        lyric.tick.hash(&mut hasher);
        lyric.text.hash(&mut hasher);
    }
    for track in &song.tracks {
        track.id.get().hash(&mut hasher);
        track.name.hash(&mut hasher);
        track.volume.hash(&mut hasher);
        track.pan.hash(&mut hasher);
        track.muted.hash(&mut hasher);
        track.soloed.hash(&mut hasher);
        track.program.hash(&mut hasher);
        track
            .bank
            .map(|bank| (bank.msb, bank.lsb))
            .hash(&mut hasher);
        track.bus.hash(&mut hasher);
        track.send.hash(&mut hasher);
        track.send_level.hash(&mut hasher);
        for note in &track.notes {
            note.start.hash(&mut hasher);
            note.duration.hash(&mut hasher);
            note.pitch.as_u8().hash(&mut hasher);
            note.channel.zero_based().hash(&mut hasher);
        }
        for clip in &track.clips {
            clip.start.hash(&mut hasher);
            clip.length.hash(&mut hasher);
        }
        if let Some(audio) = &track.audio {
            audio.path.hash(&mut hasher);
            audio.start.hash(&mut hasher);
            audio.length.hash(&mut hasher);
        }
        for slur in &track.slurs {
            slur.start.hash(&mut hasher);
            slur.end.hash(&mut hasher);
        }
    }
    hasher.finish()
}

mod full;

pub use full::{full_score_page_for_tick, show_full};

/// Loads the clef images once.
fn ensure_clefs(context: &egui::Context, state: &mut ScoreState) {
    for (slot, name, image) in [
        (
            &mut state.clef_treble,
            "score-treble-clef",
            score::clef::treble(),
        ),
        (&mut state.clef_bass, "score-bass-clef", score::clef::bass()),
    ] {
        if slot.is_none() {
            *slot = Some(context.load_texture(
                name,
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width as usize, image.height as usize],
                    &image.rgba,
                ),
                egui::TextureOptions::LINEAR,
            ));
        }
    }
}

fn tick_at_system_x(system: &score::System, x: f32) -> Option<music_core::Tick> {
    for measure in &system.measures {
        if x >= measure.x && x <= measure.x_end {
            let ratio = if measure.length > 0 && measure.x_end > measure.x {
                ((x - measure.x) / (measure.x_end - measure.x)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return Some(measure.start + (measure.length as f32 * ratio) as i64);
        }
    }
    None
}

/// Draws a Cecilia preview page, scaled to the score viewport.
pub fn show_preview(
    ui: &mut Ui,
    state: &mut ScoreState,
    page: usize,
    zoom: f32,
    engraved: &Engraved,
    position: Tick,
    entry_cursor: Option<Tick>,
) -> Option<Tick> {
    let settings = Settings::default();
    let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click());
    let available = response.rect.size();
    let fit = ((available.x - 24.0).max(120.0) / settings.page_width)
        .min((available.y - 24.0).max(120.0) / settings.page_height)
        .clamp(0.15, 1.0);
    let page_rect = page_rect(response.rect.center(), settings, fit * zoom);
    painter.rect_filled(page_rect, 0.0, Color32::WHITE);
    let mut image_rect = page_rect;
    if let Some(texture) = state.cecilia_pages.get(page) {
        let size = texture.size_vec2();
        if size.x > 0.0 && size.y > 0.0 {
            let scale = (page_rect.width() / size.x).min(page_rect.height() / size.y);
            image_rect = egui::Rect::from_center_size(page_rect.center(), size * scale);
        }
        painter.image(
            texture.id(),
            image_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        painter.text(
            page_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Cecilia preview unavailable",
            egui::FontId::proportional(14.0),
            Color32::from_rgb(0x80, 0x40, 0x40),
        );
    }
    let settings = Settings::default();
    for (tick, color) in [
        (Some(position), Color32::from_rgb(0xc0, 0x30, 0x30)),
        (entry_cursor, Color32::from_rgb(0x30, 0x60, 0xd0)),
    ] {
        let Some(tick) = tick else { continue };
        if let Some(x) = preview_cursor_x(engraved, page, tick, image_rect, settings) {
            painter.line_segment(
                [
                    egui::pos2(x, image_rect.top()),
                    egui::pos2(x, image_rect.bottom()),
                ],
                egui::Stroke::new(1.5_f32, color),
            );
        }
    }
    let click = response
        .interact_pointer_pos()
        .filter(|_| response.clicked())?;
    if !image_rect.contains(click) {
        return None;
    }
    let x = (click.x - image_rect.left()) / image_rect.width() * settings.page_width;
    let y = (click.y - image_rect.top()) / image_rect.height() * settings.page_height;
    let page_data = engraved.pages.get(page)?;
    let system = page_data.systems.iter().find(|system| {
        let height = if engraved.staves == 2 { 8.0 } else { 4.0 } * settings.staff_space;
        y >= system.y - 2.5 * settings.staff_space
            && y <= system.y + height + 2.5 * settings.staff_space
    })?;
    tick_at_system_x(system, x)
}

/// Tick and pitch under a page position.
#[must_use]
pub fn drag_target(
    engraved: &Engraved,
    page_index: usize,
    settings: Settings,
    zoom: f32,
    origin: Pos2,
    position: Pos2,
) -> Option<(Tick, u8)> {
    let page = engraved.pages.get(page_index)?;
    let local_x = (position.x - origin.x) / zoom;
    let local_y = (position.y - origin.y) / zoom;
    for system in &page.systems {
        let height = if engraved.staves == 2 { 8.0 } else { 4.0 } * settings.staff_space;
        if local_y < system.y - 2.5 * settings.staff_space
            || local_y > system.y + height + 2.5 * settings.staff_space
        {
            continue;
        }
        for measure in &system.measures {
            if local_x >= measure.x && local_x <= measure.x_end {
                let tick = score::tick_at_x(measure, local_x, music_core::PPQ / 4);
                let pitch = score::pitch_at_y_staff(system.y, local_y, settings, engraved.staves);
                return Some((tick, pitch));
            }
        }
    }
    None
}

/// The notehead nearest a page position, within a notehead's reach.
#[must_use]
pub fn hit_note(
    engraved: &Engraved,
    page_index: usize,
    zoom: f32,
    origin: Pos2,
    position: Pos2,
) -> Option<(Tick, u8)> {
    let page = engraved.pages.get(page_index)?;
    let settings = Settings::default();
    let local = Pos2::new(
        (position.x - origin.x) / zoom,
        (position.y - origin.y) / zoom,
    );
    let reach = settings.staff_space * 1.6;
    page.systems
        .iter()
        .flat_map(|system| &system.measures)
        .find(|measure| local.x >= measure.x && local.x <= measure.x_end)?;
    engraved
        .notes
        .iter()
        .filter(|note| note.page == page_index)
        .find(|note| (note.x - local.x).abs() <= reach && (note.y - local.y).abs() <= reach)
        .map(|note| (note.tick, note.pitch.as_u8()))
}

/// Draws one page and maps a click to a note or an insertion position.
/// Page that contains a tick, when the engraved layout has one.
#[must_use]
pub fn page_at_tick(engraved: &Engraved, tick: music_core::Tick) -> Option<usize> {
    for (index, page) in engraved.pages.iter().enumerate() {
        for system in &page.systems {
            for measure in &system.measures {
                if tick >= measure.start && tick < measure.start + measure.length.max(1) {
                    return Some(index);
                }
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    engraved: &Engraved,
    page: usize,
    zoom: f32,
    title: &str,
    state: &mut ScoreState,
    playhead: music_core::Tick,
    entry_cursor: Option<music_core::Tick>,
) -> ScoreOutcome {
    let settings = Settings::default();
    let page_index = clamp_page(engraved, page);
    if state.clef_treble.is_none() {
        let image = score::clef::treble();
        state.clef_treble = Some(ui.ctx().load_texture(
            "score-treble-clef",
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width as usize, image.height as usize],
                &image.rgba,
            ),
            egui::TextureOptions::LINEAR,
        ));
    }
    if state.clef_bass.is_none() {
        let image = score::clef::bass();
        state.clef_bass = Some(ui.ctx().load_texture(
            "score-bass-clef",
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width as usize, image.height as usize],
                &image.rgba,
            ),
            egui::TextureOptions::LINEAR,
        ));
    }
    let average_pitch = if engraved.notes.is_empty() {
        60.0
    } else {
        engraved
            .notes
            .iter()
            .map(|note| f32::from(note.pitch.as_u8()))
            .sum::<f32>()
            / engraved.notes.len() as f32
    };
    let single_bass = average_pitch < 52.0;
    let (response, painter) =
        ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let centre = response.rect.center();
    let page_rect = page_rect(centre, settings, zoom);
    painter.rect_filled(page_rect, 0.0, Color32::from_rgb(0xF4, 0xF1, 0xEA));
    painter.rect_stroke(
        page_rect,
        0.0,
        Stroke::new(1.0_f32, theme::line_strong()),
        egui::StrokeKind::Inside,
    );

    let origin = page_rect.min;
    painter.text(
        Pos2::new(
            page_rect.left() + settings.margin * zoom,
            page_rect.top() + 6.0 * zoom,
        ),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(12.0 * zoom),
        Color32::from_rgb(0x14, 0x14, 0x14),
    );
    if engraved.pages.len() > 1 {
        painter.text(
            Pos2::new(page_rect.center().x, page_rect.bottom() - 8.0 * zoom),
            egui::Align2::CENTER_BOTTOM,
            format!("{}", page_index + 1),
            egui::FontId::proportional(8.0 * zoom),
            Color32::from_rgb(0x50, 0x50, 0x50),
        );
    }
    if let Some(page) = engraved.pages.get(page_index) {
        for system in &page.systems {
            if engraved.percussion {
                let y = origin.y + (system.y - 2.0 * settings.staff_space) * zoom;
                let left = origin.x + settings.margin * zoom;
                let right = page_rect.max.x - settings.margin * zoom;
                painter.line_segment(
                    [Pos2::new(left, y), Pos2::new(right, y)],
                    Stroke::new(1.0_f32, Color32::from_rgb(0x3A, 0x3A, 0x3A)),
                );
            } else {
                for line in 0..5 {
                    let y = origin.y + staff_line_y(system.y, line, settings, zoom);
                    let left = origin.x + settings.margin * zoom;
                    let right = page_rect.max.x - settings.margin * zoom;
                    painter.line_segment(
                        [Pos2::new(left, y), Pos2::new(right, y)],
                        Stroke::new(0.8_f32, Color32::from_rgb(0x3A, 0x3A, 0x3A)),
                    );
                }
            }
            if engraved.staves == 2 && !engraved.percussion {
                for line in 4..=8 {
                    let y = origin.y + staff_line_y(system.y, line, settings, zoom);
                    let left = origin.x + settings.margin * zoom;
                    let right = page_rect.max.x - settings.margin * zoom;
                    painter.line_segment(
                        [Pos2::new(left, y), Pos2::new(right, y)],
                        Stroke::new(0.8_f32, Color32::from_rgb(0x3A, 0x3A, 0x3A)),
                    );
                }
                let left = origin.x + settings.margin * zoom - 10.0 * zoom;
                let top = origin.y + staff_line_y(system.y, 0, settings, zoom);
                let bottom = origin.y + staff_line_y(system.y, 8, settings, zoom);
                painter.line_segment(
                    [Pos2::new(left, top), Pos2::new(left, bottom)],
                    Stroke::new(1.6_f32, Color32::from_rgb(0x14, 0x14, 0x14)),
                );
            }
            if !engraved.percussion {
                let space = settings.staff_space;
                let (treble, bass) = if engraved.staves == 2 {
                    (true, true)
                } else {
                    (!single_bass, single_bass)
                };
                if treble && let Some(texture) = &state.clef_treble {
                    // The curl sits on the G line, the second line from the bottom.
                    let glyph = score::clef::treble_glyph();
                    let rect = egui::Rect::from_min_size(
                        egui::Pos2::new(
                            origin.x + (settings.margin - 42.0) * zoom,
                            origin.y + (system.y + (3.0 + glyph.top) * space) * zoom,
                        ),
                        egui::Vec2::new(glyph.width * space * zoom, glyph.height * space * zoom),
                    );
                    painter.image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                        Color32::from_rgb(0x14, 0x14, 0x14),
                    );
                }
                if bass && let Some(texture) = &state.clef_bass {
                    // The dots sit around the F line, the second line from the top.
                    let glyph = score::clef::bass_glyph();
                    let offset = if engraved.staves == 2 {
                        6.0 * space
                    } else {
                        0.0
                    };
                    let rect = egui::Rect::from_min_size(
                        egui::Pos2::new(
                            origin.x + (settings.margin - 42.0) * zoom,
                            origin.y + (system.y + offset + (1.0 + glyph.top) * space) * zoom,
                        ),
                        egui::Vec2::new(glyph.width * space * zoom, glyph.height * space * zoom),
                    );
                    painter.image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                        Color32::from_rgb(0x14, 0x14, 0x14),
                    );
                }
            }
            let signature_x = origin.x + (settings.margin - 4.0) * zoom;
            let numerator_y = origin.y + staff_line_y(system.y, 1, settings, zoom);
            let denominator_y = origin.y + staff_line_y(system.y, 3, settings, zoom);
            painter.text(
                Pos2::new(signature_x, numerator_y),
                egui::Align2::CENTER_CENTER,
                engraved.time_signature.numerator.to_string(),
                egui::FontId::proportional(10.0 * zoom),
                Color32::from_rgb(0x14, 0x14, 0x14),
            );
            painter.text(
                Pos2::new(signature_x, denominator_y),
                egui::Align2::CENTER_CENTER,
                engraved.time_signature.denominator.to_string(),
                egui::FontId::proportional(10.0 * zoom),
                Color32::from_rgb(0x14, 0x14, 0x14),
            );
            let top = origin.y + staff_line_y(system.y, 0, settings, zoom);
            let bottom = origin.y + staff_line_y(system.y, 4, settings, zoom);
            for measure in &system.measures {
                let x = origin.x + measure.x * zoom;
                if engraved.percussion {
                    let centre = origin.y + (system.y - 2.0 * settings.staff_space) * zoom;
                    painter.line_segment(
                        [
                            Pos2::new(x, centre - 2.0 * settings.staff_space * zoom),
                            Pos2::new(x, centre + 2.0 * settings.staff_space * zoom),
                        ],
                        Stroke::new(1.2_f32, Color32::from_rgb(0x24, 0x24, 0x24)),
                    );
                } else {
                    painter.line_segment(
                        [Pos2::new(x, top), Pos2::new(x, bottom)],
                        Stroke::new(1.2_f32, Color32::from_rgb(0x24, 0x24, 0x24)),
                    );
                }
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
                if engraved.percussion {
                    let centre = origin.y + (system.y - 2.0 * settings.staff_space) * zoom;
                    painter.line_segment(
                        [
                            Pos2::new(x, centre - 2.0 * settings.staff_space * zoom),
                            Pos2::new(x, centre + 2.0 * settings.staff_space * zoom),
                        ],
                        Stroke::new(1.2_f32, Color32::from_rgb(0x24, 0x24, 0x24)),
                    );
                } else {
                    painter.line_segment(
                        [Pos2::new(x, top), Pos2::new(x, bottom)],
                        Stroke::new(1.2_f32, Color32::from_rgb(0x24, 0x24, 0x24)),
                    );
                }
            }
        }
        for symbol in engraved
            .chord_symbols
            .iter()
            .filter(|symbol| symbol.page == page_index)
        {
            painter.text(
                Pos2::new(origin.x + symbol.x * zoom, origin.y + symbol.y * zoom),
                egui::Align2::LEFT_BOTTOM,
                &symbol.name,
                egui::FontId::proportional(10.0 * zoom),
                Color32::from_rgb(0x14, 0x14, 0x14),
            );
        }
        for frame in engraved
            .frames
            .iter()
            .filter(|frame| frame.page == page_index)
        {
            let width = 2.8 * settings.staff_space * zoom;
            let height = 2.6 * settings.staff_space * zoom;
            let left = origin.x + frame.x * zoom;
            let bottom = origin.y + frame.y * zoom;
            let top = bottom - height;
            let stroke = Stroke::new(0.7_f32, Color32::from_rgb(0x33, 0x33, 0x33));
            for string in 0..6 {
                let x = left + width * string as f32 / 5.0;
                painter.line_segment([Pos2::new(x, top), Pos2::new(x, bottom)], stroke);
            }
            for fret in 0..=5 {
                let y = top + height * fret as f32 / 5.0;
                painter.line_segment(
                    [Pos2::new(left, y), Pos2::new(left + width, y)],
                    if fret == 0 {
                        Stroke::new(1.2_f32, Color32::from_rgb(0x14, 0x14, 0x14))
                    } else {
                        stroke
                    },
                );
            }
            painter.text(
                Pos2::new(left, top - 2.0 * zoom),
                egui::Align2::LEFT_BOTTOM,
                &frame.name,
                egui::FontId::proportional(8.0 * zoom),
                Color32::from_rgb(0x14, 0x14, 0x14),
            );
            for (string, fret) in frame.frets.iter().enumerate() {
                let x = left + width * string as f32 / 5.0;
                match fret {
                    None => {
                        let s = Stroke::new(0.9_f32, Color32::from_rgb(0x14, 0x14, 0x14));
                        painter.line_segment(
                            [
                                Pos2::new(x - 1.8 * zoom, top - 5.0 * zoom),
                                Pos2::new(x + 1.8 * zoom, top - 1.0 * zoom),
                            ],
                            s,
                        );
                        painter.line_segment(
                            [
                                Pos2::new(x - 1.8 * zoom, top - 1.0 * zoom),
                                Pos2::new(x + 1.8 * zoom, top - 5.0 * zoom),
                            ],
                            s,
                        );
                    }
                    Some(0) => {
                        painter.circle_stroke(
                            Pos2::new(x, top - 3.0 * zoom),
                            1.7 * zoom,
                            Stroke::new(0.8_f32, Color32::from_rgb(0x14, 0x14, 0x14)),
                        );
                    }
                    Some(value) if *value >= frame.base_fret => {
                        let row = (*value - frame.base_fret) as f32 + 0.5;
                        painter.circle_filled(
                            Pos2::new(x, top + height * row / 5.0),
                            1.9 * zoom,
                            Color32::from_rgb(0x14, 0x14, 0x14),
                        );
                    }
                    Some(_) => {}
                }
            }
        }
        for rest in engraved.rests.iter().filter(|rest| rest.page == page_index) {
            let x = origin.x + rest.x * zoom;
            let y = origin.y + rest.y * zoom;
            let space = settings.staff_space * zoom;
            let (width, height) = match rest.kind {
                score::RestKind::Whole | score::RestKind::Half => (space * 0.9, space * 0.45),
                score::RestKind::Quarter => (space * 0.45, space * 0.9),
                score::RestKind::Eighth => (space * 0.35, space * 0.7),
            };
            let colour = Color32::from_rgb(0x14, 0x14, 0x14);
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(x, y), Vec2::new(width, height)),
                0.0,
                colour,
            );
            let dot_space = engraved
                .system_for(page_index, rest.measure)
                .map_or(rest.y, |system| {
                    score::dot_y(rest.y, system.y, settings.staff_space)
                });
            for dot in 0..rest.dots {
                painter.circle_filled(
                    Pos2::new(
                        x + width / 2.0 + space * (0.35 + f32::from(dot) * 0.45),
                        origin.y + dot_space * zoom,
                    ),
                    space * 0.1,
                    colour,
                );
            }
        }
        for note in engraved.notes.iter().filter(|note| note.page == page_index) {
            let x = origin.x + note.x * zoom;
            let y = origin.y + note.y * zoom;
            let radius = 0.58 * settings.staff_space * zoom;
            if engraved.percussion {
                let stroke = Stroke::new(1.4_f32, Color32::from_rgb(0x14, 0x14, 0x14));
                painter.line_segment(
                    [
                        Pos2::new(x - radius, y - radius),
                        Pos2::new(x + radius, y + radius),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        Pos2::new(x - radius, y + radius),
                        Pos2::new(x + radius, y - radius),
                    ],
                    stroke,
                );
                continue;
            }
            if let Some(alter) = note.spelled.accidental {
                let glyph = match alter {
                    1 => "\u{266F}",
                    -1 => "\u{266D}",
                    _ => "\u{266E}",
                };
                painter.text(
                    Pos2::new(x - radius * 2.4 - note.accidental_shift * zoom, y),
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
            let dot_space = engraved
                .system_for(page_index, note.measure)
                .map_or(note.y, |system| {
                    score::dot_y(note.y, system.y, settings.staff_space)
                });
            for dot in 0..note.dots {
                painter.circle_filled(
                    Pos2::new(
                        x + radius * (1.9 + f32::from(dot) * 1.3),
                        origin.y + dot_space * zoom,
                    ),
                    radius * 0.34,
                    Color32::from_rgb(0x14, 0x14, 0x14),
                );
            }
        }
        for beam in engraved.beams.iter().filter(|beam| beam.page == page_index) {
            let start = Pos2::new(origin.x + beam.start_x * zoom, origin.y + beam.y * zoom);
            let end = Pos2::new(origin.x + beam.end_x * zoom, origin.y + beam.y * zoom);
            painter.line_segment(
                [start, end],
                Stroke::new(2.2_f32 * zoom, Color32::from_rgb(0x14, 0x14, 0x14)),
            );
            if beam.sixteenth {
                painter.line_segment(
                    [
                        start + Vec2::new(0.0, 3.0 * zoom),
                        end + Vec2::new(0.0, 3.0 * zoom),
                    ],
                    Stroke::new(1.6_f32 * zoom, Color32::from_rgb(0x14, 0x14, 0x14)),
                );
            }
        }
        for tuplet in engraved
            .tuplets
            .iter()
            .filter(|tuplet| tuplet.page == page_index)
        {
            let y = origin.y + tuplet.y * zoom;
            let left = origin.x + tuplet.start_x * zoom;
            let right = origin.x + tuplet.end_x * zoom;
            let stroke = Stroke::new(1.0_f32, Color32::from_rgb(0x33, 0x33, 0x33));
            painter.line_segment(
                [Pos2::new(left, y), Pos2::new(left + 6.0 * zoom, y)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(right - 6.0 * zoom, y), Pos2::new(right, y)],
                stroke,
            );
            painter.text(
                Pos2::new((left + right) / 2.0, y),
                egui::Align2::CENTER_CENTER,
                format!("{}", tuplet.count),
                egui::FontId::proportional(9.0 * zoom),
                Color32::from_rgb(0x33, 0x33, 0x33),
            );
        }
        painter.text(
            Pos2::new(centre.x, page_rect.max.y - 12.0),
            egui::Align2::CENTER_BOTTOM,
            format!("Page {} / {}", page_index + 1, engraved.pages.len()),
            egui::FontId::proportional(10.0),
            Color32::from_rgb(0x55, 0x55, 0x55),
        );
        if let Some(entry) = entry_cursor {
            'entry: for system in &page.systems {
                for measure in &system.measures {
                    if entry < measure.start || entry >= measure.start + measure.length {
                        continue;
                    }
                    let ratio = ((entry - measure.start) as f32 / measure.length.max(1) as f32)
                        .clamp(0.0, 1.0);
                    let x = origin.x + (measure.x + ratio * (measure.x_end - measure.x)) * zoom;
                    let top = origin.y + staff_line_y(system.y, 0, settings, zoom);
                    let bottom_line = if engraved.staves == 2 && !engraved.percussion {
                        8
                    } else {
                        4
                    };
                    let bottom = origin.y + staff_line_y(system.y, bottom_line, settings, zoom);
                    let blue = Color32::from_rgb(0x2A, 0x6F, 0xB5);
                    painter.line_segment(
                        [Pos2::new(x, top), Pos2::new(x, bottom)],
                        Stroke::new(2.0_f32, blue),
                    );
                    painter.circle_filled(Pos2::new(x, top - 6.0), 3.0, blue);
                    break 'entry;
                }
            }
        }
        'cursor: for system in &page.systems {
            for measure in &system.measures {
                if playhead < measure.start || playhead >= measure.start + measure.length {
                    continue;
                }
                let ratio = ((playhead - measure.start) as f32 / measure.length.max(1) as f32)
                    .clamp(0.0, 1.0);
                let x = origin.x + (measure.x + ratio * (measure.x_end - measure.x)) * zoom;
                let top = origin.y + staff_line_y(system.y, 0, settings, zoom);
                let bottom_line = if engraved.staves == 2 && !engraved.percussion {
                    8
                } else {
                    4
                };
                let bottom = origin.y + staff_line_y(system.y, bottom_line, settings, zoom);
                painter.line_segment(
                    [Pos2::new(x, top), Pos2::new(x, bottom)],
                    Stroke::new(1.5_f32, theme::accent()),
                );
                break 'cursor;
            }
        }
        if response.drag_started()
            && let Some(position) = response.interact_pointer_pos()
            && let Some((tick, pitch)) = hit_note(engraved, page_index, zoom, origin, position)
        {
            let duration = engraved
                .notes
                .iter()
                .find(|note| note.tick == tick && note.pitch.as_u8() == pitch)
                .map_or(music_core::PPQ, |note| note.duration);
            let resize = ui.input(|input| input.modifiers.shift);
            state.drag = Some((tick, pitch, duration, resize));
        }
        if response.drag_stopped()
            && let Some((from_tick, from_pitch, duration, resize)) = state.drag.take()
            && let Some(position) = response.interact_pointer_pos()
            && let Some((to_tick, to_pitch)) =
                drag_target(engraved, page_index, settings, zoom, origin, position)
        {
            let new_duration = if resize {
                (to_tick - from_tick).max(music_core::PPQ / 4)
            } else {
                duration
            };
            return ScoreOutcome {
                drag: Some(ScoreDrag {
                    from_tick,
                    from_pitch,
                    to_tick,
                    to_pitch,
                    resize,
                    duration: new_duration,
                }),
                ..ScoreOutcome::default()
            };
        }
        if let Some(position) = response.interact_pointer_pos()
            && response.secondary_clicked()
        {
            return ScoreOutcome {
                note: hit_note(engraved, page_index, zoom, origin, position),
                menu: true,
                ..ScoreOutcome::default()
            };
        }
        if let Some(position) = response.interact_pointer_pos()
            && response.clicked()
        {
            if let Some(note) = hit_note(engraved, page_index, zoom, origin, position) {
                return ScoreOutcome {
                    note: Some(note),
                    ..ScoreOutcome::default()
                };
            }
            return ScoreOutcome {
                insert: hit_test(engraved, page_index, &settings, zoom, origin, position),
                ..ScoreOutcome::default()
            };
        }
    }
    ScoreOutcome::default()
}

fn hit_test(
    engraved: &Engraved,
    page_index: usize,
    settings: &Settings,
    zoom: f32,
    origin: Pos2,
    position: Pos2,
) -> Option<ScoreHit> {
    let page = engraved.pages.get(page_index)?;
    let local_x = (position.x - origin.x) / zoom;
    let local_y = (position.y - origin.y) / zoom;
    for system in &page.systems {
        let height = if engraved.staves == 2 { 8.0 } else { 4.0 } * settings.staff_space;
        if local_y < system.y - 2.5 * settings.staff_space
            || local_y > system.y + height + 2.5 * settings.staff_space
        {
            continue;
        }
        for measure in &system.measures {
            if local_x < measure.x || local_x > measure.x_end {
                continue;
            }
            return Some(ScoreHit {
                tick: score::tick_at_x(measure, local_x, music_core::PPQ / 4),
                pitch: score::pitch_at_y_staff(system.y, local_y, *settings, engraved.staves),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, Song, Track};

    fn score_song() -> Song {
        let mut song = Song::new("Score", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        song
    }

    #[test]
    fn the_cecilia_key_changes_when_note_content_changes() {
        let mut song = score_song();
        let before = cecilia_cache_key(&song, 0, "Score", false);
        song.tracks[0].notes[0].pitch = Pitch::from_name("D4").expect("valid");
        let after = cecilia_cache_key(&song, 0, "Score", false);
        assert_ne!(before, after);
    }

    #[test]
    fn the_cecilia_key_changes_with_the_selected_key() {
        let mut song = score_song();
        let before = cecilia_cache_key(&song, 0, "Score", false);
        song.key = Some(project::song::KeySignature {
            tonic: music_core::PitchClass::new(2),
            minor: false,
        });
        assert_ne!(before, cecilia_cache_key(&song, 0, "Score", false));
    }

    #[test]
    fn the_full_score_hash_changes_with_notes() {
        let mut song = score_song();
        let before = full_score_hash(&song, "Full");
        song.tracks[0].notes[0].pitch = Pitch::from_name("E4").expect("valid");
        assert_ne!(before, full_score_hash(&song, "Full"));
        let before = full_score_hash(&song, "Full");
        song.tracks[0].notes[0].channel = Channel::new(9).expect("valid");
        assert_ne!(before, full_score_hash(&song, "Full"));
    }

    #[test]
    fn the_full_score_canvas_draws_every_track() {
        let song = score_song();
        let track = project::TrackId::new(1).expect("valid");
        let engraved = score::engrave(&song, track, Settings::default());
        let tracks = vec![
            (String::from("Piano"), engraved.clone()),
            (String::from("Bass"), engraved),
        ];
        let context = egui::Context::default();
        let mut state = ScoreState::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
                ..egui::RawInput::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let _ = show_full(
                        ui,
                        &tracks,
                        0,
                        1.0,
                        "Full",
                        &mut state,
                        0,
                        false,
                        120.0,
                        &[],
                    );
                });
            },
        );
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut texts);
        }
        assert!(texts.iter().any(|text| text == "Piano"));
        assert!(texts.iter().any(|text| text == "Bass"));
    }

    #[test]
    fn the_full_score_canvas_draws_track_names() {
        let song = score_song();
        let engraved = score::engrave(
            &song,
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        let tracks = vec![(String::from("Piano"), engraved)];
        let context = egui::Context::default();
        let mut state = ScoreState::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
                ..egui::RawInput::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let _ = show_full(
                        ui,
                        &tracks,
                        0,
                        1.0,
                        "Names",
                        &mut state,
                        0,
                        false,
                        120.0,
                        &[],
                    );
                });
            },
        );
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut texts);
        }
        assert!(texts.iter().any(|text| text == "Piano"));
    }

    #[test]
    fn the_full_score_canvas_draws_section_markers() {
        let song = score_song();
        let engraved = score::engrave(
            &song,
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        let tracks = vec![(String::from("Piano"), engraved)];
        let markers = vec![project::Marker {
            tick: 0,
            name: String::from("Intro"),
        }];
        let context = egui::Context::default();
        let mut state = ScoreState::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
                ..egui::RawInput::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let _ = show_full(
                        ui, &tracks, 0, 1.0, "Markers", &mut state, 0, false, 120.0, &markers,
                    );
                });
            },
        );
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut texts);
        }
        assert!(texts.iter().any(|text| text == "Intro"));
    }

    #[test]
    fn percussion_track_names_are_recognised() {
        use score::full::is_percussion;
        assert!(is_percussion("Drums"));
        assert!(is_percussion("Drum Kit"));
        assert!(is_percussion("Percussion"));
        assert!(!is_percussion("Piano"));
        assert!(!is_percussion(""));
    }

    #[test]
    fn full_score_draws_each_chord_once() {
        use project::song::{ChordEvent, Note, Song, Track};
        let mut song = Song::new("Chords", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        for bar in 0..8 {
            track.notes.push(Note {
                id: project::NoteId::new(bar + 1).expect("valid"),
                start: music_core::PPQ * 4 * i64::from(bar),
                duration: music_core::PPQ * 4,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
            song.chords.push(ChordEvent {
                tick: music_core::PPQ * 4 * i64::from(bar),
                chord: music_core::Chord::parse("C").expect("valid"),
            });
        }
        song.tracks.push(track);
        let engraved = score::engrave(
            &song,
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        let tracks = vec![(String::from("Piano"), engraved)];
        let context = egui::Context::default();
        let mut state = ScoreState::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
                ..egui::RawInput::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let _ = show_full(
                        ui,
                        &tracks,
                        0,
                        1.0,
                        "Chords",
                        &mut state,
                        0,
                        false,
                        120.0,
                        &[],
                    );
                });
            },
        );
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut texts);
        }
        assert_eq!(
            texts.iter().filter(|text| text.as_str() == "C").count(),
            8,
            "one chord text per bar"
        );
    }

    #[test]
    fn a_long_one_track_full_score_fits_one_page() {
        let mut song = score_song();
        let bar = song.time_signature.bar_ticks();
        for index in 1..16 {
            let mut note = song.tracks[0].notes[0].clone();
            note.id = project::NoteId::new(index as u32 + 1).expect("valid");
            note.start = index * bar;
            song.tracks[0].notes.push(note);
        }
        let engraved = score::engrave(
            &song,
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        let tracks = vec![(String::from("Piano"), engraved)];
        assert_eq!(full_score_page_for_tick(&tracks, 0, false, &[]), Some(0));
        // One staff leaves room for many systems on a page.
        assert_eq!(
            full_score_page_for_tick(&tracks, 15 * bar, false, &[]),
            Some(0)
        );
        let context = egui::Context::default();
        let mut state = ScoreState::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
                ..egui::RawInput::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let _ = show_full(
                        ui,
                        &tracks,
                        0,
                        1.0,
                        "Long",
                        &mut state,
                        0,
                        false,
                        120.0,
                        &[],
                    );
                });
            },
        );
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut texts);
        }
        assert!(texts.iter().any(|text| text == "1 / 1"), "{texts:?}");
        assert!(texts.iter().any(|text| text == "Piano"));
        let mut playheads = 0;
        for clipped in &output.shapes {
            count_stroke(
                &clipped.shape,
                Color32::from_rgb(0xD0, 0x30, 0x30),
                &mut playheads,
            );
        }
        assert!(playheads >= 1, "full score playhead");
    }

    #[test]
    fn a_multi_page_canvas_shows_page_numbers() {
        let mut song = score_song();
        let bar = song.time_signature.bar_ticks();
        for index in 1..40 {
            let mut note = song.tracks[0].notes[0].clone();
            note.id = project::NoteId::new(index as u32 + 1).expect("valid");
            note.start = index * bar;
            song.tracks[0].notes.push(note);
        }
        let engraved = score::engrave(
            &song,
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        assert!(engraved.pages.len() > 1);
        let context = egui::Context::default();
        let mut state = ScoreState::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
                ..egui::RawInput::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let _ = show(ui, &engraved, 1, 1.0, "Pages", &mut state, 0, None);
                });
            },
        );
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut texts);
        }
        assert!(texts.iter().any(|text| text == "2"), "{texts:?}");
    }

    #[test]
    fn a_point_maps_to_a_tick_in_the_system() {
        let song = score_song();
        let engraved = score::engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        let system = &engraved.pages[0].systems[0];
        let measure = &system.measures[0];
        let tick = tick_at_system_x(system, measure.x).expect("start");
        assert_eq!(tick, measure.start);
        let middle = tick_at_system_x(system, (measure.x + measure.x_end) / 2.0).expect("middle");
        assert!(middle > measure.start && middle < measure.start + measure.length);
    }

    #[test]
    fn clicking_the_full_score_returns_a_seek_tick() {
        let song = score_song();
        let engraved = score::engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        let tracks = vec![(String::from("Piano"), engraved)];
        let context = egui::Context::default();
        let mut state = ScoreState::default();
        let point = Pos2::new(210.0, 150.0);
        eprintln!(
            "page {:?}",
            page_rect(Pos2::new(450.0, 450.0), Settings::default(), 1.0)
        );
        let mut press = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
            events: vec![egui::Event::PointerMoved(point)],
            ..egui::RawInput::default()
        };
        let _ = context.run(press.clone(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let _ = show_full(
                    ui,
                    &tracks,
                    0,
                    1.0,
                    "Click",
                    &mut state,
                    0,
                    false,
                    120.0,
                    &[],
                );
            });
        });
        press.events = vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }];
        let _ = context.run(press.clone(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                let _ = show_full(
                    ui,
                    &tracks,
                    0,
                    1.0,
                    "Click",
                    &mut state,
                    0,
                    false,
                    120.0,
                    &[],
                );
            });
        });
        press.events = vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }];
        let mut seek = None;
        let _ = context.run(press, |context| {
            egui::CentralPanel::default().show(context, |ui| {
                seek = show_full(
                    ui,
                    &tracks,
                    0,
                    1.0,
                    "Click",
                    &mut state,
                    0,
                    false,
                    120.0,
                    &[],
                )
                .1;
            });
        });
        assert!(seek.is_some(), "full-score click");
    }

    #[test]
    fn clicking_the_preview_returns_a_seek_tick() {
        let settings = Settings::default();
        let song = score_song();
        let engraved = score::engrave(&song, TrackId::new(1).expect("valid"), settings);
        let system = &engraved.pages[0].systems[0];
        let measure = &system.measures[0];
        let page = page_rect(Pos2::new(450.0, 450.0), settings, 1.0);
        let point = Pos2::new(
            page.left() + (measure.x + 8.0) / settings.page_width * page.width(),
            page.top() + (system.y + settings.staff_space) / settings.page_height * page.height(),
        );
        let context = egui::Context::default();
        let mut state = ScoreState::default();
        let mut run = |events: Vec<egui::Event>, seek: &mut Option<Tick>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
                events,
                ..egui::RawInput::default()
            };
            let _ = context.run(input, |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    *seek = show_preview(ui, &mut state, 0, 1.0, &engraved, 0, None);
                });
            });
        };
        run(vec![egui::Event::PointerMoved(point)], &mut None);
        run(
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut None,
        );
        let mut seek = None;
        run(
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut seek,
        );
        assert!(seek.is_some(), "preview click");
    }

    #[test]
    fn a_tick_maps_into_the_preview_page() {
        let song = score_song();
        let settings = Settings::default();
        let engraved = score::engrave(&song, TrackId::new(1).expect("valid"), settings);
        let rect = page_rect(Pos2::new(300.0, 400.0), settings, 1.0);
        let first = preview_cursor_x(&engraved, 0, 0, rect, settings).expect("first tick");
        let later = preview_cursor_x(&engraved, 0, PPQ, rect, settings).expect("later tick");
        assert!(later > first);
        assert!(first >= rect.left() && later <= rect.right());
    }

    #[test]
    fn pages_scale_with_zoom() {
        let settings = Settings::default();
        let rect = page_rect(Pos2::ZERO, settings, 2.0);
        assert!((rect.width() - settings.page_width * 2.0).abs() < 0.01);
        let line = staff_line_y(100.0, 4, settings, 2.0);
        assert!((line - (200.0 + 4.0 * settings.staff_space * 2.0)).abs() < 0.01);
    }

    fn collect_text(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, out);
                }
            }
            _ => {}
        }
    }

    fn count_stroke(shape: &egui::Shape, colour: Color32, count: &mut usize) {
        match shape {
            egui::Shape::LineSegment { stroke, .. } if stroke.color == colour => *count += 1,
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    count_stroke(shape, colour, count);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn the_entry_cursor_draws() {
        use project::song::{Note, Song, Track};
        let mut song = Song::new("Entry", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        let engraved = score::engrave(
            &song,
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        let context = egui::Context::default();
        let mut state = ScoreState::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
                ..egui::RawInput::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let _ = show(ui, &engraved, 0, 1.0, "Entry", &mut state, 0, Some(PPQ));
                });
            },
        );
        let blue = Color32::from_rgb(0x2A, 0x6F, 0xB5);
        let mut count = 0;
        for clipped in &output.shapes {
            count_stroke(&clipped.shape, blue, &mut count);
        }
        assert!(count > 0, "entry cursor line is drawn");
    }

    #[test]
    fn the_canvas_draws_the_title_chords_and_measure_numbers() {
        use project::song::{ChordEvent, Note, Song, Track};
        let mut song = Song::new("Canvas", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: music_core::PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        song.chords.push(ChordEvent {
            tick: 0,
            chord: music_core::Chord::parse("C").expect("valid"),
        });
        let engraved = score::engrave(
            &song,
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        let context = egui::Context::default();
        let mut state = ScoreState::default();
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 900.0))),
                ..egui::RawInput::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let _ = show(ui, &engraved, 0, 1.0, "Canvas & Test", &mut state, 0, None);
                });
            },
        );
        let mut text = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut text);
        }
        assert!(
            text.iter().any(|value| value == "Canvas & Test"),
            "title drawn"
        );
        assert!(text.iter().any(|value| value == "C"), "chord name drawn");
        assert!(
            text.iter().any(|value| value == "1"),
            "measure number drawn"
        );
    }

    #[test]
    fn dragging_maps_to_a_quantized_tick_and_pitch() {
        let settings = Settings::default();
        let score = score_song();
        let engraved = score::engrave(&score, project::TrackId::new(1).expect("valid"), settings);
        let system = &engraved.pages[0].systems[0];
        let measure = &system.measures[0];
        let x = measure.x + (measure.x_end - measure.x) / 2.0;
        let position = Pos2::new(x, system.y - settings.staff_space);
        let (tick, pitch) =
            drag_target(&engraved, 0, settings, 1.0, Pos2::ZERO, position).expect("target");
        assert_eq!(tick, PPQ * 2);
        assert_eq!(pitch, 62);
    }

    #[test]
    fn a_click_near_a_notehead_finds_it() {
        let score = score_song();
        let engraved = score::engrave(
            &score,
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        let note = &engraved.notes[0];
        let origin = Pos2::ZERO;
        let found = hit_note(&engraved, 0, 1.0, origin, Pos2::new(note.x, note.y)).expect("hit");
        assert_eq!(found.0, note.tick);
        assert!(hit_note(&engraved, 0, 1.0, origin, Pos2::new(note.x + 500.0, note.y)).is_none());
    }

    #[test]
    fn a_tick_selects_its_page() {
        let score = score_song();
        let engraved = score::engrave(
            &score,
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        assert_eq!(page_at_tick(&engraved, 0), Some(0));
        assert!(page_at_tick(&engraved, 10_000_000).is_none());
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
