//! The Stage 6 shell: Perform / Arrange / Mix over the existing models.

use std::path::PathBuf;
use std::time::Duration;

use egui::{Context, Rect, ViewportCommand};
use music_core::{PPQ, Tick, Velocity};
use project::song::{Note, ProjectFile, Song, Track};
use project::{NoteId, ProjectError, TrackId};

use crate::editor::{Editor, snap};
use crate::persistence::{self, Autosave};

use crate::piano_roll::{self, RollState};
use crate::theme;
use crate::widgets;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Perform,
    Arrange,
    Mix,
}

impl View {
    fn index(self) -> usize {
        match self {
            Self::Perform => 0,
            Self::Arrange => 1,
            Self::Mix => 2,
        }
    }

    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Perform,
            2 => Self::Mix,
            _ => Self::Arrange,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Perform => "Perform",
            Self::Arrange => "Arrange",
            Self::Mix => "Mix",
        }
    }
}

pub struct ShellApp {
    editor: Editor,
    selected_note: Option<(TrackId, NoteId)>,
    selected_clip: Option<(TrackId, project::ClipId)>,
    chord_dialog: bool,
    chord_dialog_target: Tick,
    reharmonize_dialog: bool,
    chord_input: String,
    key: music_core::PitchClass,
    project_path: Option<PathBuf>,
    view: View,
    roll: RollState,
    playing: bool,
    position: Tick,
    icon: egui::TextureHandle,
    player: Option<audio::player::Player>,
    autosave: Autosave,
    recovery: Option<std::path::PathBuf>,
    shown: bool,
    load_error: Option<String>,
}

impl ShellApp {
    #[must_use]
    pub fn new(context: &Context, project: ProjectFile, project_path: Option<PathBuf>) -> Self {
        theme::install(context);
        let icon = context.load_texture(
            "app-icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [256, 256],
                include_bytes!("../assets/app-icon-256.rgba"),
            ),
            egui::TextureOptions::LINEAR,
        );
        let editor = match Editor::new(project) {
            Ok(editor) => editor,
            Err(_) => Editor::new(demo_project()).expect("the demo project is valid"),
        };
        Self {
            editor,
            icon,
            player: None,
            autosave: Autosave::new(std::time::Duration::from_secs(10)),
            recovery: project_path
                .as_ref()
                .filter(|path| persistence::recovery_available(path))
                .map(|path| persistence::autosave_path(path)),
            selected_note: None,
            selected_clip: None,
            chord_dialog: false,
            chord_dialog_target: 0,
            reharmonize_dialog: false,
            chord_input: String::from("Am F C G"),
            key: music_core::PitchClass::new(0),
            project_path,
            view: View::Arrange,
            roll: RollState::default(),
            playing: false,
            position: 0,
            shown: false,
            load_error: None,
        }
    }

    fn song(&self) -> &Song {
        self.editor.song()
    }

    /// Applies a click from the piano roll: select a note, or insert one.
    fn edit_click(&mut self, click: piano_roll::RollClick) {
        if let Some((track, clip)) = click.clip {
            self.roll.selected = Some(track);
            self.selected_clip = Some((track, clip));
            self.selected_note = None;
            return;
        }
        if let Some((track, note)) = click.note {
            self.roll.selected = Some(track);
            self.selected_note = Some((track, note));
            return;
        }
        let Some(track) = self
            .roll
            .selected
            .or_else(|| self.song().tracks.first().map(|track| track.id))
        else {
            return;
        };
        let Ok(pitch) = music_core::Pitch::new(click.pitch) else {
            return;
        };
        let start = snap(click.tick, PPQ);
        let _ = self.editor.insert(track, start, pitch);
        self.selected_note = None;
    }

    /// Undo, redo, delete and save, from the keyboard.
    fn handle_keys(&mut self, context: &Context) {
        let undo =
            context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
        let redo = context.input_mut(|input| {
            input.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            )
        });
        let save =
            context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        let delete = context.input(|input| {
            input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace)
        });
        if undo {
            self.editor.undo();
            self.selected_note = None;
        }
        if redo {
            self.editor.redo();
            self.selected_note = None;
        }
        if delete {
            if let Some((track, clip)) = self.selected_clip.take() {
                let _ = self.editor.remove_clip(track, clip);
            } else if let Some((track, note)) = self.selected_note.take() {
                let _ = self.editor.delete(track, note);
            }
        }
        if context.input(|input| input.key_pressed(egui::Key::N))
            && self.selected_clip.is_none()
            && let Some(track) = self
                .roll
                .selected
                .or_else(|| self.song().tracks.first().map(|track| track.id))
        {
            let bar = self.song().time_signature.bar_ticks().max(1);
            let start = snap(self.position, bar);
            let next = self
                .song()
                .tracks
                .iter()
                .find(|candidate| candidate.id == track)
                .and_then(|candidate| candidate.clips.iter().map(|clip| clip.id.get()).max())
                .unwrap_or(0)
                .saturating_add(1);
            if let Ok(id) = project::ClipId::new(next) {
                let clip = project::song::Clip {
                    id,
                    name: format!("Clip {next}"),
                    start,
                    length: bar,
                };
                let _ = self.editor.insert_clip(track, clip);
                self.selected_clip = Some((track, id));
            }
        }
        if let Some((track, clip)) = self.selected_clip {
            let bar = self.song().time_signature.bar_ticks().max(1);
            let right = context.input(|input| input.key_pressed(egui::Key::ArrowRight));
            let left = context.input(|input| input.key_pressed(egui::Key::ArrowLeft));
            let shift = context.input(|input| input.modifiers.shift);
            if (right || left)
                && let Some(found) = self
                    .song()
                    .tracks
                    .iter()
                    .find(|candidate| candidate.id == track)
                    .and_then(|candidate| candidate.clips.iter().find(|cl| cl.id == clip))
                    .cloned()
            {
                if shift {
                    let length = if right {
                        found.length + bar
                    } else {
                        (found.length - bar).max(1)
                    };
                    let _ = self.editor.resize_clip(track, clip, length);
                } else {
                    let target = if right {
                        found.start + bar
                    } else {
                        (found.start - bar).max(0)
                    };
                    let _ = self.editor.move_clip(track, clip, target);
                }
            }
            if context.input(|input| input.key_pressed(egui::Key::J))
                && let Some(next) = self
                    .song()
                    .tracks
                    .iter()
                    .find(|candidate| candidate.id == track)
                    .and_then(|candidate| {
                        candidate
                            .clips
                            .iter()
                            .find(|cl| {
                                cl.start
                                    > candidate
                                        .clips
                                        .iter()
                                        .find(|sel| sel.id == clip)
                                        .map_or(i64::MAX, |sel| sel.start)
                            })
                            .map(|cl| cl.id)
                    })
            {
                let _ = self.editor.join_clips(track, clip, next);
                self.selected_clip = Some((track, clip));
            }
        }
        if context.input(|input| input.key_pressed(egui::Key::K)) {
            let bar = self.song().time_signature.bar_ticks().max(1);
            self.chord_dialog_target = snap(self.position, bar);
            self.chord_dialog = true;
        }
        if context.input(|input| input.key_pressed(egui::Key::H))
            && let Some(found) = self
                .song()
                .chords
                .iter()
                .filter(|event| event.tick <= self.position)
                .max_by_key(|event| event.tick)
        {
            self.chord_dialog_target = found.tick;
            self.reharmonize_dialog = true;
        }
        if save {
            let path = self
                .project_path
                .clone()
                .unwrap_or_else(|| std::path::PathBuf::from("lazy-arranger.json"));
            if let Err(error) = self.editor.save(&path) {
                self.load_error = Some(error);
            } else {
                let _ = std::fs::remove_file(persistence::autosave_path(&path));
                self.project_path = Some(path);
                self.recovery = None;
            }
        }
    }

    fn advance(&mut self, context: &Context) {
        if let Some(player) = &self.player {
            let seconds = player.frame() as f64 / f64::from(audio::SAMPLE_RATE);
            self.position = self.song().tempo_map.tick_at_seconds(seconds);
            self.playing = player.is_playing();
            if self.playing {
                context.request_repaint_after(Duration::from_millis(16));
            }
            return;
        }
        if !self.playing {
            return;
        }
        let dt = context.input(|input| input.stable_dt).min(0.1);
        let bpm = self.song().tempo_map.bpm_at(self.position);
        let ticks = (f64::from(dt) * bpm / 60.0 * PPQ as f64).round() as Tick;
        self.position += ticks.max(1);
        let end = self.song().end_tick().max(1);
        if self.position > end {
            self.position = 0;
            self.playing = false;
        }
        context.request_repaint_after(Duration::from_millis(16));
    }

    /// Appends a chord symbol to the chord text field.
    fn append_chord(&mut self, text: &str) {
        if self.chord_input.trim().is_empty() {
            self.chord_input = text.to_string();
        } else {
            self.chord_input.push(' ');
            self.chord_input.push_str(text);
        }
    }

    /// The chord lane entry dialog: type a progression, apply or cancel.
    fn chord_dialog(&mut self, context: &Context) {
        if !self.chord_dialog {
            return;
        }
        let mut open = self.chord_dialog;
        egui::Window::new("Chords")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label("Am F C G · vi IV I V · C Am | F G   (| bar, % repeat, / hold)");
                ui.horizontal(|ui| {
                    ui.label("Key:");
                    egui::ComboBox::from_id_salt("chord-key")
                        .selected_text(self.key.name_sharp())
                        .show_ui(ui, |ui| {
                            for semitone in 0..12_u8 {
                                let candidate = music_core::PitchClass::new(i32::from(semitone));
                                ui.selectable_value(
                                    &mut self.key,
                                    candidate,
                                    candidate.name_sharp(),
                                );
                            }
                        });
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.chord_input)
                        .desired_width(320.0)
                        .hint_text("Am F C G"),
                );
                let pad_chords = music_core::roman::pads(self.key);
                egui::Grid::new("chord-pads")
                    .num_columns(4)
                    .spacing([4.0, 4.0])
                    .show(ui, |ui| {
                        for (index, chord) in pad_chords.iter().enumerate() {
                            let label = chord.format(music_core::Spelling::Sharp);
                            if ui.button(label.clone()).clicked() {
                                self.append_chord(&label);
                            }
                            if index % 4 == 3 {
                                ui.end_row();
                            }
                        }
                    });
                let suggestions = music_core::progression::parse_in_key(
                    &self.chord_input,
                    0,
                    self.song().time_signature,
                    self.key,
                )
                .ok()
                .and_then(|placed| placed.last().map(|entry| entry.chord))
                .map(music_core::roman::suggest_next);
                if let Some(list) = suggestions {
                    ui.horizontal(|ui| {
                        ui.label("Next:");
                        for chord in list {
                            let label = chord.format(music_core::Spelling::Sharp);
                            if ui.button(label.clone()).clicked() {
                                self.append_chord(&label);
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        let bar = self.song().time_signature.bar_ticks().max(1);
                        let start = snap(self.position, bar);
                        match self.editor.apply_progression_in_key(
                            &self.chord_input,
                            start,
                            self.key,
                        ) {
                            Ok(count) => {
                                self.load_error = None;
                                let _ = count;
                                self.chord_dialog = false;
                            }
                            Err(error) => self.load_error = Some(error),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.chord_dialog = false;
                    }
                });
            });
        self.chord_dialog = open && self.chord_dialog;
    }

    /// The reharmonize dialog: four rule-based proposals for the chord at the
    /// playhead, each applied over one bar.
    fn reharmonize_dialog(&mut self, context: &Context) {
        if !self.reharmonize_dialog {
            return;
        }
        let target = self.chord_dialog_target;
        let Some(original) = self
            .song()
            .chords
            .iter()
            .find(|event| event.tick == target)
            .map(|event| event.chord)
        else {
            self.reharmonize_dialog = false;
            return;
        };
        let mut open = self.reharmonize_dialog;
        egui::Window::new("Reharmonize")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(format!(
                    "{} at bar {}",
                    original.format(music_core::Spelling::Sharp),
                    target / 3840 + 1
                ));
                for (kind, chords) in music_core::reharmonize::candidates(original) {
                    let label = chords
                        .iter()
                        .map(|chord| chord.format(music_core::Spelling::Sharp))
                        .collect::<Vec<_>>()
                        .join(" ");
                    if ui.button(format!("{}: {label}", kind.label())).clicked() {
                        match self.editor.reharmonize(target, &chords) {
                            Ok(()) => self.reharmonize_dialog = false,
                            Err(error) => self.load_error = Some(error),
                        }
                    }
                }
            });
        self.reharmonize_dialog = open && self.reharmonize_dialog;
    }

    /// Writes an autosave when the project is dirty and the interval elapsed.
    fn run_autosave(&mut self) {
        if !self.editor.dirty() {
            return;
        }
        let Some(path) = self.project_path.clone() else {
            return;
        };
        let Ok(json) = self.editor.project().to_json_pretty() else {
            return;
        };
        let _ = self.autosave.maybe_save(Some(&path), true, &json);
    }

    /// Offers to restore an autosave that is newer than the project.
    fn recovery_dialog(&mut self, context: &Context) {
        let Some(autosave) = self.recovery.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new("Recovered edits found")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(format!("{} is newer than the project.", autosave.display()));
                ui.horizontal(|ui| {
                    if ui.button("Restore").clicked() {
                        match load_project(&autosave) {
                            Ok(project) => match Editor::new(project) {
                                Ok(editor) => {
                                    self.editor = editor;
                                    let original = self
                                        .project_path
                                        .clone()
                                        .unwrap_or_else(|| autosave.clone());
                                    self.project_path = Some(original);
                                    let _ = std::fs::remove_file(&autosave);
                                    self.recovery = None;
                                }
                                Err(error) => self.load_error = Some(error.to_string()),
                            },
                            Err(error) => self.load_error = Some(error.to_string()),
                        }
                    }
                    if ui.button("Discard").clicked() {
                        let _ = std::fs::remove_file(&autosave);
                        self.recovery = None;
                    }
                });
            });
        if !open {
            self.recovery = None;
        }
    }

    /// Starts, pauses or resumes the audio stream.
    fn toggle_playback(&mut self) {
        if let Some(player) = &self.player {
            if player.is_playing() {
                player.pause();
                self.playing = false;
            } else {
                player.play();
                self.playing = true;
            }
            return;
        }
        let audio = audio::render(self.song(), audio::SAMPLE_RATE);
        match audio::player::Player::new(audio) {
            Ok(player) => {
                player.play();
                self.player = Some(player);
                self.playing = true;
            }
            Err(error) => self.load_error = Some(format!("audio: {error}")),
        }
    }

    /// Stops playback and releases the device.
    fn stop_playback(&mut self) {
        if let Some(player) = self.player.take() {
            player.stop();
            player.release();
        }
        self.playing = false;
        self.position = 0;
    }

    fn top_bar(&mut self, context: &Context) {
        egui::TopBottomPanel::top("header")
            .exact_height(theme::HEADER_HEIGHT)
            .frame(egui::Frame::NONE.fill(theme::PANEL))
            .show(context, |ui| {
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    egui::Stroke::new(1.0_f32, theme::LINE),
                );
                let mut area = rect.shrink2(egui::vec2(16.0, 0.0));
                // Title area: badge, title, subtitle (HeaderBar::paint).
                let title_area = Rect::from_min_size(area.min, egui::vec2(220.0, area.height()));
                area = Rect::from_min_max(egui::pos2(title_area.right(), area.top()), area.max);
                let badge = Rect::from_center_size(
                    egui::pos2(title_area.left() + 18.0, title_area.center().y),
                    egui::vec2(36.0, 36.0),
                );
                ui.painter().image(
                    self.icon.id(),
                    badge,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                let title_rect = Rect::from_min_max(
                    egui::pos2(title_area.left() + 36.0 + 14.0, title_area.top() + 6.0),
                    egui::pos2(title_area.right(), title_area.center().y),
                );
                let name = if self.song().name.is_empty() {
                    "Untitled"
                } else {
                    self.song().name.as_str()
                };
                ui.painter().text(
                    title_rect.left_bottom(),
                    egui::Align2::LEFT_BOTTOM,
                    name,
                    egui::FontId::proportional(16.0),
                    theme::TEXT,
                );
                let subtitle = format!(
                    "{} BPM \u{b7} {}/{}",
                    self.song().tempo_map.bpm_at(self.position).round() as i64,
                    self.song().time_signature.numerator,
                    self.song().time_signature.denominator
                );
                ui.painter().text(
                    egui::pos2(title_rect.left(), title_rect.bottom() + 2.0),
                    egui::Align2::LEFT_TOP,
                    subtitle,
                    egui::FontId::proportional(12.5),
                    theme::TEXT_MUTED,
                );

                // Transport buttons: stop 44, play 54, 8 px gaps.
                let buttons_rect = Rect::from_min_size(
                    area.min,
                    egui::vec2(54.0 + 4.0 * 44.0 + 4.0 * 8.0, area.height()),
                );
                area = Rect::from_min_max(egui::pos2(buttons_rect.right(), area.top()), area.max);
                let mut x = buttons_rect.left();
                let stop = Rect::from_center_size(
                    egui::pos2(x + 22.0, buttons_rect.center().y),
                    egui::vec2(44.0, 44.0),
                );
                x += 52.0;
                let play = Rect::from_center_size(
                    egui::pos2(x + 27.0, buttons_rect.center().y),
                    egui::vec2(54.0, 54.0),
                );
                if transport_button(ui, stop, "\u{25A0}", !self.playing) {
                    self.stop_playback();
                }
                if transport_button(
                    ui,
                    play,
                    if self.playing { "\u{2016}" } else { "\u{25B6}" },
                    self.playing,
                ) {
                    self.toggle_playback();
                    context.request_repaint_after(std::time::Duration::from_millis(16));
                }

                // LCD, 350 x 52, then the view tabs on the right.
                let lcd_rect = Rect::from_center_size(
                    egui::pos2(area.left() + 16.0 + 175.0, rect.center().y),
                    widgets::LCD_SIZE,
                );
                let mut lcd_ui = ui.new_child(egui::UiBuilder::new().max_rect(lcd_rect));
                widgets::lcd(
                    &mut lcd_ui,
                    self.position,
                    self.song().tempo_map.bpm_at(self.position),
                    self.song().time_signature,
                );
                let tabs_rect = Rect::from_center_size(
                    egui::pos2(
                        rect.right() - 16.0 - widgets::TABS_SIZE.x * 0.5,
                        rect.center().y,
                    ),
                    widgets::TABS_SIZE,
                );
                let mut tabs_ui = ui.new_child(egui::UiBuilder::new().max_rect(tabs_rect));
                let names = [
                    View::Perform.label(),
                    View::Arrange.label(),
                    View::Mix.label(),
                ];
                if let Some(index) = widgets::view_tabs(&mut tabs_ui, self.view.index(), &names) {
                    self.view = View::from_index(index);
                }
            });
    }

    fn sidebar(&mut self, context: &Context) {
        egui::SidePanel::left("tracks")
            .exact_width(theme::SIDEBAR_WIDTH)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(context, |ui| {
                ui.label(
                    egui::RichText::new("TRACKS")
                        .size(10.0)
                        .color(theme::TEXT_MUTED),
                );
                ui.add_space(4.0);
                let rows: Vec<(usize, TrackId, String, usize)> = self
                    .song()
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(index, track)| (index, track.id, track.name.clone(), track.notes.len()))
                    .collect();
                for (index, id, name, notes) in rows {
                    self.track_row(ui, index, id, &name, notes);
                }
                if self.song().tracks.is_empty() {
                    ui.label(egui::RichText::new("no tracks").color(theme::TEXT_DIM));
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if ui.button("Open project…").clicked()
                        && let Some(path) = rfd_pick()
                    {
                        self.open(&path);
                    }
                });
            });
        if let Some(error) = self.load_error.clone() {
            egui::Window::new("Could not open project")
                .collapsible(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("Dismiss").clicked() {
                        self.load_error = None;
                    }
                });
        }
    }

    fn track_row(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        id: TrackId,
        name: &str,
        notes: usize,
    ) {
        let selected = self.roll.selected == Some(id);
        let colour = theme::track_colour(index);
        let response = ui
            .horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(6.0, 28.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, colour);
                ui.vertical(|ui| {
                    let name = if name.is_empty() { "untitled" } else { name };
                    ui.label(egui::RichText::new(name).size(12.0).color(if selected {
                        theme::TEXT
                    } else {
                        theme::TEXT_MUTED
                    }));
                    ui.label(
                        egui::RichText::new(format!("{notes} notes"))
                            .size(9.0)
                            .color(theme::TEXT_DIM),
                    );
                });
            })
            .response;
        if response.interact(egui::Sense::click()).clicked() {
            self.roll.selected = if selected { None } else { Some(id) };
        }
    }

    fn status_bar(&self, context: &Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(theme::STATUS_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    let notes: usize = self.song().tracks.iter().map(|track| track.notes.len()).sum();
                    let issues = self.editor.project().validate();
                    let errors = issues
                        .iter()
                        .filter(|issue| issue.severity == project::Severity::Error)
                        .count();
                    let label = if errors == 0 {
                        format!(
                            "schema v{}  ·  {} track(s)  ·  {notes} note(s)  ·  no audio engine (shell only)",
                            self.editor.project().version,
                            self.song().tracks.len()
                        )
                    } else {
                        format!("{errors} validation error(s)")
                    };
                    ui.label(egui::RichText::new(label).size(10.0).color(if errors == 0 { theme::TEXT_MUTED } else { theme::RECORDING }));
                    if let Some(path) = &self.project_path {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(path.display().to_string())
                                    .size(10.0)
                                    .color(theme::TEXT_DIM),
                            );
                        });
                    }
                });
            });
    }

    fn open(&mut self, path: &PathBuf) {
        match load_project(path) {
            Ok(project) => {
                match Editor::new(project) {
                    Ok(editor) => self.editor = editor,
                    Err(error) => {
                        self.load_error = Some(error.to_string());
                        return;
                    }
                }
                self.selected_note = None;
                self.project_path = Some(path.clone());
                self.position = 0;
                self.playing = false;
                self.load_error = None;
            }
            Err(error) => self.load_error = Some(error.to_string()),
        }
    }
}

impl std::fmt::Debug for ShellApp {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShellApp")
            .field("tracks", &self.song().tracks.len())
            .field("view", &self.view)
            .field("playing", &self.playing)
            .field("position", &self.position)
            .field("dirty", &self.editor.dirty())
            .finish()
    }
}

impl eframe::App for ShellApp {
    fn update(&mut self, context: &Context, frame: &mut eframe::Frame) {
        let _ = frame;
        // ADR-011: paint the first themed frame before the window becomes visible.
        let mut style = (*context.style()).clone();
        if style.visuals.panel_fill != theme::PANEL {
            style.visuals = theme::visuals();
            context.set_style(style);
        }
        self.handle_keys(context);
        self.run_autosave();
        self.recovery_dialog(context);
        self.chord_dialog(context);
        self.reharmonize_dialog(context);
        self.advance(context);
        self.top_bar(context);
        self.status_bar(context);
        self.sidebar(context);
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::TIMELINE))
            .show(context, |ui| {
                let view = self.view;
                let position = self.position;
                let mut clicked = None;
                let Self { editor, roll, .. } = self;
                let selected_clip = self.selected_clip;
                let project = editor.project();
                match view {
                View::Arrange => {
                    if let Some(click) = piano_roll::show(
                        ui,
                        &project.song,
                        roll,
                        position,
                        selected_clip.map(|(_, clip)| clip),
                    ) {
                        clicked = Some(click);
                    }
                }
                View::Perform => {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("Perform view arrives with the arranger keyboard controls (Stage 7).")
                                .color(theme::TEXT_MUTED),
                        );
                    });
                }
                View::Mix => {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("Mix view arrives with routing and SoundFont playback (Stage 7).")
                                .color(theme::TEXT_MUTED),
                        );
                    });
                }
                }
                clicked
            });
        if let Some(click) = panel.inner {
            self.edit_click(click);
        }
        if !self.shown {
            self.shown = true;
            context.send_viewport_cmd(ViewportCommand::Visible(true));
        }
    }
}

/// A round transport button drawn like v1's `RoundButton`.
fn transport_button(ui: &mut egui::Ui, rect: Rect, glyph: &str, active: bool) -> bool {
    let response = ui.interact(rect, ui.id().with(glyph), egui::Sense::click());
    let fill = if active {
        theme::ACCENT.gamma_multiply(0.25)
    } else if response.hovered() {
        theme::LINE
    } else {
        theme::RAISED
    };
    ui.painter()
        .circle_filled(rect.center(), rect.width() * 0.5, fill);
    ui.painter().circle_stroke(
        rect.center(),
        rect.width() * 0.5,
        egui::Stroke::new(1.0_f32, if active { theme::ACCENT } else { theme::LINE }),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(rect.width() * 0.32),
        if active { theme::ACCENT } else { theme::TEXT },
    );
    response.clicked()
}

/// Minimal file dialog: the shell must not pull a dialog crate in prematurely.
fn rfd_pick() -> Option<PathBuf> {
    // Without a dialog dependency, opening from the command line is the path.
    None
}

pub fn load_project(path: &PathBuf) -> Result<ProjectFile, ProjectError> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| ProjectError::Legacy(format!("{}: {error}", path.display())))?;
    ProjectFile::from_json(&text)
}

/// A four-bar demo used when no project path is given.
#[must_use]
pub fn demo_project() -> ProjectFile {
    let mut song = Song::new("Four bars", 120.0);
    let bar = song.time_signature.bar_ticks();
    let chords = ["C", "Am", "F", "G"];
    let mut next_note = 0u32;
    for (index, name) in chords.iter().enumerate() {
        let Ok(chord) = music_core::Chord::parse(name) else {
            continue;
        };
        song.chords.push(project::ChordEvent {
            tick: index as Tick * bar,
            chord,
        });
        let mut track = Track::new(
            TrackId::new(u32::try_from(index + 1).unwrap_or(1))
                .unwrap_or_else(|_| TrackId::new(1).expect("valid")),
            (*name).to_string(),
        );
        let start = index as Tick * bar;
        for pitch in chord.tones(4) {
            next_note += 1;
            track.notes.push(Note {
                id: NoteId::new(next_note).unwrap_or_else(|_| NoteId::new(1).expect("valid")),
                start,
                duration: bar,
                pitch,
                velocity: Velocity::new(80).unwrap_or_else(|_| Velocity::new(80).expect("valid")),
                channel: music_core::Channel::first(),
            });
        }
        for beat in [0, 2] {
            if let Some(root) = chord.bass_tone(2) {
                next_note += 1;
                track.notes.push(Note {
                    id: NoteId::new(next_note).unwrap_or_else(|_| NoteId::new(1).expect("valid")),
                    start: start + Tick::from(beat) * PPQ,
                    duration: PPQ,
                    pitch: root,
                    velocity: Velocity::new(100)
                        .unwrap_or_else(|_| Velocity::new(100).expect("valid")),
                    channel: music_core::Channel::first(),
                });
            }
        }
        song.tracks.push(track);
    }
    ProjectFile::new(song)
}
