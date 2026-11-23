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

/// Persistent score canvas state.
#[derive(Clone, Default)]
pub struct ScoreState {
    drag: Option<(Tick, u8, Tick, bool)>,
    clef_treble: Option<egui::TextureHandle>,
    clef_bass: Option<egui::TextureHandle>,
}

impl std::fmt::Debug for ScoreState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScoreState")
            .field("dragging", &self.drag.is_some())
            .field("treble_loaded", &self.clef_treble.is_some())
            .field("bass_loaded", &self.clef_bass.is_some())
            .finish()
    }
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
pub fn show(
    ui: &mut Ui,
    engraved: &Engraved,
    page: usize,
    zoom: f32,
    title: &str,
    state: &mut ScoreState,
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
                    let image = score::clef::treble();
                    let height = 7.0 * space;
                    let width = height * image.width as f32 / image.height as f32;
                    let rect = egui::Rect::from_min_size(
                        egui::Pos2::new(
                            origin.x + (settings.margin - 42.0) * zoom,
                            origin.y + (system.y - 2.0 * space) * zoom,
                        ),
                        egui::Vec2::new(width * zoom, height * zoom),
                    );
                    painter.image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                        Color32::from_rgb(0x14, 0x14, 0x14),
                    );
                }
                if bass && let Some(texture) = &state.clef_bass {
                    let image = score::clef::bass();
                    let height = 3.2 * space;
                    let width = height * image.width as f32 / image.height as f32;
                    let offset = if engraved.staves == 2 {
                        6.0 * space
                    } else {
                        0.0
                    };
                    let rect = egui::Rect::from_min_size(
                        egui::Pos2::new(
                            origin.x + (settings.margin - 48.0) * zoom,
                            origin.y + (system.y + offset - 0.3 * space) * zoom,
                        ),
                        egui::Vec2::new(width * zoom, height * zoom),
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
                    let _ = show(ui, &engraved, 0, 1.0, "Canvas & Test", &mut state);
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
    fn page_indices_are_clamped() {
        let engraved = score::engrave(
            &project::song::Song::new("Empty", 120.0),
            project::TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        assert_eq!(clamp_page(&engraved, 5), 0);
    }
}
