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
}

/// Persistent score canvas state.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScoreState {
    drag: Option<(Tick, u8)>,
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
    state: &mut ScoreState,
) -> ScoreOutcome {
    let settings = Settings::default();
    let page_index = clamp_page(engraved, page);
    let (response, painter) =
        ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let centre = response.rect.center();
    let page_rect = page_rect(centre, settings, zoom);
    painter.rect_filled(page_rect, 0.0, Color32::from_rgb(0xF4, 0xF1, 0xEA));
    painter.rect_stroke(
        page_rect,
        0.0,
        Stroke::new(1.0_f32, theme::LINE_STRONG),
        egui::StrokeKind::Inside,
    );

    let origin = page_rect.min;
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
            let top = origin.y + staff_line_y(system.y, 0, settings, zoom);
            let bottom = origin.y + staff_line_y(system.y, 4, settings, zoom);
            for measure in &system.measures {
                let x = origin.x + measure.x * zoom;
                if !engraved.percussion {
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
            if let Some(last) = system.measures.last()
                && !engraved.percussion
            {
                let x = origin.x + last.x_end * zoom;
                painter.line_segment(
                    [Pos2::new(x, top), Pos2::new(x, bottom)],
                    Stroke::new(1.2_f32, Color32::from_rgb(0x24, 0x24, 0x24)),
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
            state.drag = Some((tick, pitch));
        }
        if response.drag_stopped()
            && let Some((from_tick, from_pitch)) = state.drag.take()
            && let Some(position) = response.interact_pointer_pos()
            && let Some((to_tick, to_pitch)) =
                drag_target(engraved, page_index, settings, zoom, origin, position)
        {
            return ScoreOutcome {
                drag: Some(ScoreDrag {
                    from_tick,
                    from_pitch,
                    to_tick,
                    to_pitch,
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
