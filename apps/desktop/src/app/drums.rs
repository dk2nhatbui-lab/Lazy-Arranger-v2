//! The Drums view, drawn like v1 `BeatView`: a 52 pt header with the track
//! and pattern name, the step length, the pattern length, New pattern, Put in
//! bars and Clear, then a grid of square steps that scrolls when the pattern
//! is long, with beat numbers under it.

use super::*;
use crate::theme::ThemedMenu;
use arranging::drums;

const HEADER: f32 = 52.0;
const NAMES: f32 = 150.0;

/// The pattern the grid edits.
#[derive(Clone, Debug)]
struct Target {
    track: TrackId,
    index: usize,
    /// A Loops slot open in the editor, else a timeline region.
    slot: Option<usize>,
    /// The timeline clip, when the region is one.
    clip: Option<project::ClipId>,
    start: Tick,
    length: Tick,
    name: String,
}

/// Drums view state that lives between frames.
#[derive(Clone, Debug)]
pub(super) struct DrumsUi {
    steps_per_beat: Tick,
    zoom: f32,
    scroll: egui::Vec2,
    paint: Option<u8>,
    clipboard: Option<Vec<project::song::Note>>,
    place: Option<(String, String, bool)>,
}

impl DrumsUi {
    /// `BeatView::zoomBy`: bigger steps for `factor` > 1.
    pub(super) fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(0.5, 4.0);
    }
}

impl Default for DrumsUi {
    fn default() -> Self {
        Self {
            steps_per_beat: 4,
            zoom: 1.0,
            scroll: egui::Vec2::ZERO,
            paint: None,
            clipboard: None,
            place: None,
        }
    }
}

fn bars_label(bars: Tick) -> String {
    if bars == 1 {
        String::from("1 bar")
    } else {
        format!("{bars} bars")
    }
}

impl ShellApp {
    /// The track the Drums view edits: the selected one, else the first
    /// track on the drum channel, else the first track.
    fn drums_track(&self) -> Option<usize> {
        let song = self.song();
        self.roll
            .selected
            .and_then(|id| song.tracks.iter().position(|track| track.id == id))
            .or_else(|| {
                song.tracks.iter().position(|track| {
                    track
                        .notes
                        .first()
                        .is_some_and(|note| note.channel.zero_based() == 9)
                })
            })
            .or(if song.tracks.is_empty() {
                None
            } else {
                Some(0)
            })
    }

    fn drums_target(&self) -> Option<Target> {
        let song = self.song();
        let index = self.drums_track()?;
        let track = &song.tracks[index];
        let bar = song.time_signature.bar_ticks().max(1);
        // The Loops clip open in the editor, as in v1.
        if let Some((scene, id)) = self.slot_edit
            && id == track.id
            && let Some(clip) = track.slot(scene)
        {
            return Some(Target {
                track: track.id,
                index,
                slot: Some(scene),
                clip: None,
                start: 0,
                length: clip.length.max(1),
                name: if clip.name.is_empty() {
                    String::from("Clip")
                } else {
                    clip.name.clone()
                },
            });
        }
        // The clip under the playhead, else the first one.
        let at = self.position.max(0);
        if let Some(clip) = track
            .clips
            .iter()
            .find(|clip| clip.contains(at))
            .or(track.clips.first())
        {
            return Some(Target {
                track: track.id,
                index,
                slot: None,
                clip: Some(clip.id),
                start: clip.start,
                length: clip.length.max(1),
                name: if clip.name.is_empty() {
                    String::from("Pattern")
                } else {
                    clip.name.clone()
                },
            });
        }
        // A track with notes and no clips is one part, as on the timeline.
        let end = track.notes.iter().map(project::song::Note::end).max()?;
        Some(Target {
            track: track.id,
            index,
            slot: None,
            clip: None,
            start: 0,
            length: ((end + bar - 1) / bar).max(1) * bar,
            name: track.name.clone(),
        })
    }

    /// The pattern's notes, measured from its start.
    fn drums_notes(&self, target: &Target) -> Vec<project::song::Note> {
        let track = &self.song().tracks[target.index];
        if let Some(scene) = target.slot {
            return track
                .slot(scene)
                .map(|clip| clip.notes.clone())
                .unwrap_or_default();
        }
        track
            .notes
            .iter()
            .filter(|note| note.start >= target.start && note.start < target.start + target.length)
            .map(|note| {
                let mut copy = note.clone();
                copy.start -= target.start;
                copy
            })
            .collect()
    }

    /// Writes the pattern's notes back, as one undo step.
    fn drums_write(&mut self, target: &Target, mut pattern: Vec<project::song::Note>) {
        let result = if let Some(scene) = target.slot {
            self.editor.set_slot_notes(scene, target.track, pattern)
        } else {
            let track = &self.song().tracks[target.index];
            let end = target.start + target.length;
            let mut notes: Vec<project::song::Note> = track
                .notes
                .iter()
                .filter(|note| note.start < target.start || note.start >= end)
                .cloned()
                .collect();
            for note in &mut pattern {
                note.start += target.start;
            }
            notes.extend(pattern);
            drums::renumber(&mut notes);
            self.editor.replace_track_notes(target.track, notes)
        };
        if let Err(error) = result {
            self.status = Some(error);
        }
    }

    /// Plays one drum hit on the track's own sound.
    fn preview_drum(&mut self, index: usize, pitch: u8, velocity: u8) {
        let Some(mut track) = self.song().tracks.get(index).cloned() else {
            return;
        };
        let channel = track.notes.first().map_or_else(
            || music_core::Channel::new(9).unwrap_or(music_core::Channel::first()),
            |note| note.channel,
        );
        track.notes.clear();
        track.clips.clear();
        track.slots.clear();
        track.muted = false;
        if let (Ok(pitch), Ok(velocity), Ok(id)) = (
            music_core::Pitch::new(pitch),
            music_core::Velocity::new(velocity.clamp(1, 127)),
            NoteId::new(1),
        ) {
            track.notes.push(project::song::Note {
                id,
                start: 0,
                duration: PPQ / 4,
                pitch,
                velocity,
                channel,
            });
        }
        let mut song = Song::new("Preview", 120.0);
        song.tracks.push(track);
        let rendered = audio::live::render(&song, audio::SAMPLE_RATE);
        self.preview_player = audio::player::Player::new_live(rendered).ok();
    }

    /// The Drums view.
    pub(super) fn drums_panel(&mut self, ui: &mut egui::Ui) {
        let (full, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let song = self.song().clone();
        let target = self.drums_target();
        let bar = song.time_signature.bar_ticks().max(1);
        let painter = ui.painter().clone();
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(full.left(), full.top() + HEADER - 1.0),
                egui::pos2(full.right(), full.top() + HEADER),
            ),
            0.0,
            theme::line(),
        );

        // Header, laid out from the right like v1.
        let top = egui::Rect::from_min_size(full.min, egui::vec2(full.width(), HEADER))
            .shrink2(egui::vec2(14.0, 10.0));
        let mut right = top.right();
        let mut take = |width: f32, gap: f32| {
            let rect = egui::Rect::from_min_max(
                egui::pos2(right - width, top.top()),
                egui::pos2(right, top.bottom()),
            );
            right -= width + gap;
            rect
        };
        let has = target.is_some();
        let clear = take(70.0, 8.0);
        let place = if has { Some(take(116.0, 8.0)) } else { None };
        let new = take(if has { 110.0 } else { 190.0 }, 8.0);
        let bars_box = take(90.0, 8.0);
        let steps_box = take(120.0, 12.0);
        let title = egui::Rect::from_min_max(top.min, egui::pos2(right, top.bottom()));

        // Title: the track (a menu of tracks) and the pattern name.
        let track_index = self.drums_track();
        let track_name = track_index
            .and_then(|index| song.tracks.get(index))
            .map_or_else(
                || String::from("Add a track to make a beat."),
                |track| track.name.clone(),
            );
        let mut title_ui = layout::toolbar_child(ui, title);
        {
            let widgets = &mut title_ui.visuals_mut().widgets;
            widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
            widgets.inactive.bg_stroke = egui::Stroke::NONE;
        }
        title_ui.horizontal_centered(|ui| {
            let mut chosen = None;
            ui.menu_button(
                egui::RichText::new(&track_name)
                    .font(theme::semibold(15.0))
                    .color(theme::text()),
                |ui| {
                    for track in &song.tracks {
                        if ui.button(&track.name).clicked() {
                            chosen = Some(track.id);
                            ui.close_menu();
                        }
                    }
                },
            )
            .response
            .on_hover_text("Choose the track to edit");
            if let Some(target) = &target {
                ui.label(
                    egui::RichText::new(format!("/  {}", target.name))
                        .font(theme::semibold(15.0))
                        .color(theme::text()),
                );
            }
            if let Some(track) = chosen {
                self.roll.selected = Some(track);
                self.drums.scroll = egui::Vec2::ZERO;
            }
        });

        if let Some(target) = &target {
            let mut steps_ui = layout::toolbar_child(ui, steps_box);
            let step_label = |steps: Tick| match steps {
                2 => "1/8 steps",
                8 => "1/32 steps",
                3 => "1/8 triplets",
                6 => "1/16 triplets",
                _ => "1/16 steps",
            };
            egui::ComboBox::from_id_salt("drum-steps")
                .width(steps_box.width() - 8.0)
                .selected_text(step_label(self.drums.steps_per_beat))
                .show_ui(&mut steps_ui, |ui| {
                    for steps in [2, 4, 8, 3, 6] {
                        ui.selectable_value(
                            &mut self.drums.steps_per_beat,
                            steps,
                            step_label(steps),
                        );
                    }
                })
                .response
                .on_hover_text("The length of one step");
            let mut bars_ui = layout::toolbar_child(ui, bars_box);
            let current = (target.length / bar).max(1);
            let mut wanted = current;
            egui::ComboBox::from_id_salt("drum-bars")
                .width(bars_box.width() - 8.0)
                .selected_text(bars_label(current))
                .show_ui(&mut bars_ui, |ui| {
                    for bars in [1, 2, 4, 8] {
                        ui.selectable_value(&mut wanted, bars, bars_label(bars));
                    }
                })
                .response
                .on_hover_text("The length of the pattern");
            if wanted != current {
                let length = wanted * bar;
                let result = match (target.slot, target.clip) {
                    (Some(scene), _) => self.editor.set_slot_length(scene, target.track, length),
                    (None, Some(clip)) => self
                        .editor
                        .resize_clip(target.track, clip, length)
                        .map_err(|error| error.to_string()),
                    (None, None) => {
                        self.editor
                            .add_named_clip(target.track, target.start, length, "Pattern")
                    }
                };
                if let Err(error) = result {
                    self.status = Some(error);
                }
            }
        }
        if let Some(index) = track_index
            && widgets::text_button(
                ui,
                new,
                "drum-new",
                if has {
                    "New pattern"
                } else {
                    "New pattern on this track"
                },
                false,
            )
            .on_hover_text("Make a one-bar pattern at the playhead on the selected track")
            .clicked()
        {
            self.drums_new_pattern(index);
        }
        if let Some(place) = place
            && widgets::text_button(ui, place, "drum-place", "Put in bars...", false)
                .on_hover_text(
                    "Play the pattern from one bar to another, on this track or on a new drum track",
                )
                .clicked()
        {
            let first = self.position.max(0) / bar + 1;
            let bars = target.as_ref().map_or(1, |target| (target.length / bar).max(1));
            self.drums.place = Some((first.to_string(), (first + bars.max(4) - 1).to_string(), false));
        }
        if let Some(target) = &target
            && widgets::text_button(ui, clear, "drum-clear", "Clear", false)
                .on_hover_text("Remove all notes from the pattern")
                .clicked()
        {
            self.drums_write(target, Vec::new());
        }

        let body = egui::Rect::from_min_max(egui::pos2(full.left(), full.top() + HEADER), full.max);
        let Some(target) = target else {
            painter.text(
                body.center(),
                egui::Align2::CENTER_CENTER,
                if track_index.is_none() {
                    "Add a track first."
                } else {
                    "This track has no pattern yet.\nClick \"New pattern on this track\"."
                },
                egui::FontId::proportional(15.0),
                theme::text_dim(),
            );
            self.drums_place_dialog(ui, None);
            return;
        };

        // The grid: square steps, the largest that fit, scaled by the zoom.
        let notes = self.drums_notes(&target);
        let rows = drums::rows(&notes);
        let step = (PPQ / self.drums.steps_per_beat.max(1)).max(1);
        let steps = ((target.length + step - 1) / step).max(1);
        let steps_per_bar = (bar / step).max(1);
        let area = egui::Rect::from_min_max(
            egui::pos2(body.left() + NAMES, body.top() + 4.0),
            egui::pos2(body.right() - 26.0, body.bottom() - 16.0),
        );
        let fit = ((area.width() / steps as f32)
            .min((area.height() - 26.0).max(1.0) / rows.len() as f32))
        .max(1.0);
        // A long pattern starts at the 14 pt floor; zoom from there, so the
        // wheel still makes its steps bigger.
        let size = (fit.max(14.0) * self.drums.zoom).clamp(14.0, 160.0).floor();
        let content = egui::vec2(steps as f32 * size, rows.len() as f32 * size + 26.0);
        if ui.rect_contains_pointer(body) {
            let (delta, zoom, command) = ui.input(|input| {
                (
                    input.smooth_scroll_delta,
                    input.zoom_delta(),
                    input.modifiers.command,
                )
            });
            if command || zoom != 1.0 {
                let factor = if zoom == 1.0 {
                    1.15_f32.powf(delta.y / 40.0)
                } else {
                    zoom
                };
                self.drums.zoom_by(factor);
            } else {
                self.drums.scroll -= delta;
            }
        }
        self.drums.scroll.x = self
            .drums
            .scroll
            .x
            .clamp(0.0, (content.x - area.width()).max(0.0));
        self.drums.scroll.y = self
            .drums
            .scroll
            .y
            .clamp(0.0, (content.y - area.height()).max(0.0));
        let grid_x = area.left() - self.drums.scroll.x;
        let grid_y = area.top() - self.drums.scroll.y;

        // The playing step, followed by the scroll.
        let playing_step = if self.playing {
            match target.slot {
                Some(_) => self
                    .launcher
                    .since
                    .get(&target.track)
                    .filter(|_| self.launcher.playing.get(&target.track) == target.slot.as_ref())
                    .map(|since| (self.position - since).rem_euclid(target.length) / step),
                None => (self.position >= target.start
                    && self.position < target.start + target.length)
                    .then(|| (self.position - target.start) / step),
            }
        } else {
            None
        };
        if let Some(playing) = playing_step {
            let left = playing as f32 * size;
            if left < self.drums.scroll.x {
                self.drums.scroll.x = left;
            } else if left + size > self.drums.scroll.x + area.width() {
                self.drums.scroll.x = left + size - area.width();
            }
            ui.ctx().request_repaint();
        }

        let colour = theme::track_colour(target.index);
        let ink = if theme::light() {
            theme::text()
        } else {
            egui::Color32::from_rgb(0x11, 0x14, 0x17)
        };
        let inset = (size * 0.12).min(2.5);
        let names = painter.with_clip_rect(egui::Rect::from_min_max(
            egui::pos2(body.left(), area.top()),
            egui::pos2(area.left(), area.bottom()),
        ));
        for (row, (_, name)) in rows.iter().enumerate() {
            names.text(
                egui::pos2(body.left() + 16.0, grid_y + (row as f32 + 0.5) * size),
                egui::Align2::LEFT_CENTER,
                name,
                egui::FontId::proportional(13.0),
                theme::text_muted(),
            );
        }
        let grid = painter.with_clip_rect(area);
        let first_step = ((self.drums.scroll.x / size).floor() as Tick).max(0);
        let last_step = (((self.drums.scroll.x + area.width()) / size).ceil() as Tick).min(steps);
        for (row, (pitch, _)) in rows.iter().enumerate() {
            for index in first_step..last_step {
                let cell = egui::Rect::from_min_size(
                    egui::pos2(grid_x + index as f32 * size, grid_y + row as f32 * size),
                    egui::vec2(size, size),
                )
                .shrink(inset);
                let velocity = drums::velocity_at(&notes, *pitch, index * step, step);
                let on_beat = index % self.drums.steps_per_beat == 0;
                let other_bar = (index / steps_per_bar) % 2 == 1;
                if velocity > 0 {
                    let fill = if velocity >= drums::ACCENT {
                        colour.lerp_to_gamma(egui::Color32::WHITE, 0.12)
                    } else {
                        colour
                    };
                    let alpha = if velocity >= drums::ACCENT {
                        1.0
                    } else if velocity >= drums::NORMAL {
                        0.85
                    } else {
                        0.65
                    };
                    grid.rect_filled(cell, theme::RADIUS, fill.gamma_multiply(alpha));
                    grid.rect_stroke(
                        cell,
                        theme::RADIUS,
                        egui::Stroke::new(1.0_f32, ink.gamma_multiply(0.22)),
                        egui::StrokeKind::Inside,
                    );
                } else {
                    grid.rect_filled(
                        cell,
                        theme::RADIUS,
                        colour.gamma_multiply(if on_beat { 0.12 } else { 0.07 }),
                    );
                    grid.rect_stroke(
                        cell,
                        theme::RADIUS,
                        egui::Stroke::new(
                            1.0_f32,
                            colour.gamma_multiply(if other_bar { 0.22 } else { 0.32 }),
                        ),
                        egui::StrokeKind::Inside,
                    );
                }
                if playing_step == Some(index) {
                    grid.rect_stroke(
                        cell.expand(1.0),
                        theme::RADIUS,
                        egui::Stroke::new(
                            1.5_f32,
                            theme::playhead().gamma_multiply(if velocity > 0 { 0.9 } else { 0.35 }),
                        ),
                        egui::StrokeKind::Outside,
                    );
                }
            }
        }
        // Beat numbers under the grid.
        let numbers_y = grid_y + rows.len() as f32 * size + 2.0;
        let mut index = first_step - first_step % self.drums.steps_per_beat;
        while index < last_step {
            let bar_number = index / steps_per_bar + 1;
            let beat = (index % steps_per_bar) / self.drums.steps_per_beat + 1;
            grid.text(
                egui::pos2(grid_x + index as f32 * size, numbers_y),
                egui::Align2::LEFT_TOP,
                format!("{bar_number}.{beat}"),
                egui::FontId::proportional(11.0),
                theme::text_dim(),
            );
            index += self.drums.steps_per_beat;
        }

        // Steps: click toggles (Shift accents, Alt is soft), right-click
        // cycles soft, normal, accent; dragging paints the same value.
        let _ = ui.interact(
            area,
            ui.id().with("drum-grid"),
            egui::Sense::click_and_drag(),
        );
        let hit = |pos: egui::Pos2| {
            let row = ((pos.y - grid_y) / size).floor();
            let index = ((pos.x - grid_x) / size).floor();
            (row >= 0.0 && (row as usize) < rows.len() && index >= 0.0 && (index as Tick) < steps)
                .then_some((row as usize, index as Tick))
        };
        let channel = song.tracks[target.index].notes.first().map_or_else(
            || music_core::Channel::new(9).unwrap_or(music_core::Channel::first()),
            |note| note.channel,
        );
        let (primary, secondary, origin, latest, down, modifiers) = ui.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.secondary_pressed(),
                input.pointer.press_origin(),
                input.pointer.latest_pos(),
                input.pointer.any_down(),
                input.modifiers,
            )
        });
        let mut changed = None;
        let mut paint_at = None;
        if (primary || secondary)
            && let Some(origin) = origin.or(latest).filter(|origin| area.contains(*origin))
            && let Some((row, index)) = hit(origin)
        {
            let now = drums::velocity_at(&notes, rows[row].0, index * step, step);
            self.drums.paint = Some(if secondary {
                drums::cycle(now)
            } else if now > 0 {
                0
            } else if modifiers.shift {
                drums::ACCENT
            } else if modifiers.alt {
                drums::SOFT
            } else {
                drums::NORMAL
            });
            paint_at = Some((row, index));
        } else if down
            && self.drums.paint.is_some()
            && let Some(pos) = latest.filter(|pos| area.contains(*pos))
        {
            paint_at = hit(pos);
        }
        if let (Some(velocity), Some((row, index))) = (self.drums.paint, paint_at) {
            let pitch = rows[row].0;
            let mut edited = notes.clone();
            if drums::set_step(&mut edited, pitch, index * step, step, velocity, channel) {
                changed = Some(edited);
                if velocity > 0 && !self.playing {
                    self.preview_drum(target.index, pitch, velocity);
                }
            }
        }
        if !down && !primary && !secondary {
            self.drums.paint = None;
        }
        if let Some(edited) = changed {
            self.drums_write(&target, edited);
        }

        // Names: a click plays the sound; right-click for the row menu.
        let names_rect = egui::Rect::from_min_max(
            egui::pos2(body.left(), area.top()),
            egui::pos2(area.left(), area.bottom()),
        );
        let names_response =
            ui.interact(names_rect, ui.id().with("drum-names"), egui::Sense::click());
        let row_at = |pos: egui::Pos2| {
            let row = ((pos.y - grid_y) / size).floor();
            (row >= 0.0 && (row as usize) < rows.len()).then_some(row as usize)
        };
        if names_response.clicked()
            && let Some(row) = names_response.interact_pointer_pos().and_then(row_at)
        {
            self.preview_drum(target.index, rows[row].0, drums::NORMAL);
        }
        let menu_row_id = ui.id().with("drum-menu-row");
        if names_response.secondary_clicked()
            && let Some(row) = names_response.interact_pointer_pos().and_then(row_at)
        {
            ui.data_mut(|data| data.insert_temp(menu_row_id, row));
        }
        let menu_row: Option<usize> = ui.data(|data| data.get_temp(menu_row_id));
        let mut action = None;
        names_response.themed_context_menu(|ui| {
            let Some(row) = menu_row.filter(|row| *row < rows.len()) else {
                return;
            };
            ui.label(egui::RichText::new(&rows[row].1).strong());
            for (label, id, enabled) in [
                ("Cut Row", 1, true),
                ("Copy Row", 2, true),
                ("Paste to Row", 3, self.drums.clipboard.is_some()),
                ("Cut Pattern", 4, true),
                ("Copy Pattern", 5, true),
                ("Paste Pattern", 6, self.drums.clipboard.is_some()),
            ] {
                if id == 4 {
                    ui.separator();
                }
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    action = Some((id, row));
                    ui.close_menu();
                }
            }
        });
        if let Some((id, row)) = action {
            let pitch = rows[row].0;
            let in_row = |note: &project::song::Note| note.pitch.as_u8() == pitch;
            match id {
                1 | 2 => {
                    self.drums.clipboard =
                        Some(notes.iter().filter(|note| in_row(note)).cloned().collect());
                    if id == 1 {
                        self.drums_write(
                            &target,
                            notes.iter().filter(|note| !in_row(note)).cloned().collect(),
                        );
                    }
                }
                4 | 5 => {
                    self.drums.clipboard = Some(notes.clone());
                    if id == 4 {
                        self.drums_write(&target, Vec::new());
                    }
                }
                3 | 6 => {
                    if let Some(copied) = self.drums.clipboard.clone() {
                        let mut edited = notes.clone();
                        for mut note in copied.into_iter().filter(|note| note.start < target.length)
                        {
                            if id == 3 {
                                match music_core::Pitch::new(pitch) {
                                    Ok(pitch) => note.pitch = pitch,
                                    Err(_) => continue,
                                }
                            }
                            edited.retain(|other| {
                                !(other.start == note.start && other.pitch == note.pitch)
                            });
                            edited.push(note);
                        }
                        drums::renumber(&mut edited);
                        self.drums_write(&target, edited);
                    }
                }
                _ => {}
            }
        }
        self.drums_place_dialog(ui, Some(&target));
    }

    /// A one-bar pattern at the playhead, after any clip in the way.
    fn drums_new_pattern(&mut self, index: usize) {
        let song = self.song();
        let bar = song.time_signature.bar_ticks().max(1);
        let track = &song.tracks[index];
        let mut start = self.position.max(0) / bar * bar;
        let mut clips = track.clips.clone();
        clips.sort_by_key(|clip| clip.start);
        for clip in &clips {
            if start < clip.end() && start + bar > clip.start {
                start = (clip.end() + bar - 1) / bar * bar;
            }
        }
        // Without clips the notes are one part; start after them.
        if clips.is_empty()
            && let Some(end) = track.notes.iter().map(project::song::Note::end).max()
            && start < end
        {
            start = (end + bar - 1) / bar * bar;
        }
        let id = track.id;
        match self.editor.add_named_clip(id, start, bar, "Pattern") {
            Ok(()) => {
                self.roll.selected = Some(id);
                self.slot_edit = None;
                self.position = start;
                self.drums.scroll = egui::Vec2::ZERO;
            }
            Err(error) => self.status = Some(error),
        }
    }

    /// "Put the pattern in bars": the pattern repeats from one bar to another
    /// on this track or on a new drum track.
    fn drums_place_dialog(&mut self, ui: &mut egui::Ui, target: Option<&Target>) {
        let Some((mut from, mut to, mut new_track)) = self.drums.place.clone() else {
            return;
        };
        let Some(target) = target else {
            self.drums.place = None;
            return;
        };
        let name = self.song().tracks[target.index].name.clone();
        let mut put = false;
        let mut cancel = false;
        let modal = crate::dialog::modal(ui.ctx(), "drum-place", 440.0, |ui| {
            crate::dialog::title(ui, "Put the pattern in bars");
            crate::dialog::body(
                ui,
                "The pattern repeats from the first bar to the last bar. What is there now \
                 on that track is replaced.",
            );
            ui.add_space(8.0);
            egui::Grid::new("drum-place-grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("From bar");
                    ui.add(egui::TextEdit::singleline(&mut from).desired_width(80.0));
                    ui.end_row();
                    ui.label("To bar");
                    ui.add(egui::TextEdit::singleline(&mut to).desired_width(80.0));
                    ui.end_row();
                    ui.label("Where");
                    egui::ComboBox::from_id_salt("drum-place-where")
                        .selected_text(if new_track {
                            String::from("On a new drum track")
                        } else {
                            format!("On this track: {name}")
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut new_track,
                                false,
                                format!("On this track: {name}"),
                            );
                            ui.selectable_value(&mut new_track, true, "On a new drum track");
                        });
                    ui.end_row();
                });
            crate::dialog::buttons(ui, |ui| {
                put = crate::dialog::primary(ui, "Put").clicked();
                cancel = crate::dialog::secondary(ui, "Cancel").clicked();
            });
        });
        let open = !modal.should_close();
        self.drums.place = (open && !cancel && !put).then(|| (from.clone(), to.clone(), new_track));
        if !put {
            return;
        }
        let (Ok(first), Ok(last)) = (from.trim().parse::<Tick>(), to.trim().parse::<Tick>()) else {
            self.status = Some(String::from("bars must be numbers"));
            return;
        };
        if first < 1 || last < first {
            self.status = Some(String::from("the last bar must come after the first"));
            return;
        }
        let bar = self.song().time_signature.bar_ticks().max(1);
        let pattern = self.drums_notes(target);
        let mut track = target.track;
        if new_track {
            match self.editor.duplicate_track(track) {
                Ok(copy) => {
                    track = copy;
                    let label = format!("{name} {first}-{last}");
                    let _ = self.editor.rename_track(copy, &label);
                }
                Err(error) => {
                    self.status = Some(error);
                    return;
                }
            }
        }
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return;
        };
        let mut notes = if new_track {
            Vec::new()
        } else {
            entry.notes.clone()
        };
        drums::place(
            &mut notes,
            &pattern,
            target.length.min((last - first + 1) * bar),
            (first - 1) * bar,
            last * bar,
        );
        match self.editor.replace_track_notes(track, notes) {
            Ok(()) => {
                self.roll.selected = Some(track);
                self.status = Some(format!("pattern in bars {first}-{last}"));
            }
            Err(error) => self.status = Some(error),
        }
    }
}
