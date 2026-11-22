//! The Stage 6 shell: Perform / Arrange / Mix over the existing models.

use std::path::PathBuf;
use std::time::Duration;

use egui::{Context, ViewportCommand};
use music_core::{PPQ, Tick, Velocity};
use project::song::{Note, ProjectFile, Song, Track};
use project::{NoteId, ProjectError, TrackId};

use crate::piano_roll::{self, RollState};
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Perform,
    Arrange,
    Mix,
}

impl View {
    fn label(self) -> &'static str {
        match self {
            Self::Perform => "Perform",
            Self::Arrange => "Arrange",
            Self::Mix => "Mix",
        }
    }
}

#[derive(Debug)]
pub struct ShellApp {
    project: ProjectFile,
    project_path: Option<PathBuf>,
    view: View,
    roll: RollState,
    playing: bool,
    position: Tick,
    shown: bool,
    load_error: Option<String>,
}

impl ShellApp {
    #[must_use]
    pub fn new(context: &Context, project: ProjectFile, project_path: Option<PathBuf>) -> Self {
        theme::install(context);
        Self {
            project,
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
        &self.project.song
    }

    fn advance(&mut self, context: &Context) {
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

    fn top_bar(&mut self, context: &Context) {
        egui::TopBottomPanel::top("header")
            .exact_height(theme::HEADER_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("LAZY ARRANGER").strong().size(15.0));
                    ui.add_space(12.0);
                    self.lcd(
                        ui,
                        "TEMPO",
                        &format!("{:.1}", self.song().tempo_map.bpm_at(self.position)),
                    );
                    self.lcd(
                        ui,
                        "METER",
                        &format!(
                            "{}/{}",
                            self.song().time_signature.numerator,
                            self.song().time_signature.denominator
                        ),
                    );
                    self.lcd(ui, "POSITION", &self.position_label());
                    ui.add_space(12.0);
                    let play = egui::Button::new(if self.playing {
                        "■  Stop"
                    } else {
                        "▶  Play"
                    });
                    if ui.add(play).clicked() {
                        if self.playing {
                            self.playing = false;
                        } else {
                            self.playing = true;
                            context.request_repaint_after(Duration::from_millis(16));
                        }
                    }
                    if ui.button("⏮  Return").clicked() {
                        self.position = 0;
                        self.playing = false;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for view in [View::Mix, View::Arrange, View::Perform] {
                            let selected = self.view == view;
                            if ui.selectable_label(selected, view.label()).clicked() {
                                self.view = view;
                            }
                        }
                    });
                });
            });
    }

    fn lcd(&self, ui: &mut egui::Ui, label: &str, value: &str) {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(label).size(9.0).color(theme::TEXT_DIM));
            egui::Frame::new()
                .fill(theme::LCD)
                .corner_radius(theme::RADIUS_SMALL)
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(value)
                            .monospace()
                            .size(16.0)
                            .color(theme::LCD_TEXT),
                    );
                });
        });
    }

    fn position_label(&self) -> String {
        let signature = self.song().time_signature;
        let bar = signature.bar_index_at(self.position) + 1;
        let beat = signature.beat_in_bar(self.position);
        let ticks = self.position.rem_euclid(signature.beat_ticks());
        format!("{bar:>3}.{beat}.{ticks:03}")
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
                ui.painter().rect_filled(rect, 2.0, colour);
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
                    let issues = self.project.validate();
                    let errors = issues
                        .iter()
                        .filter(|issue| issue.severity == project::Severity::Error)
                        .count();
                    let label = if errors == 0 {
                        format!(
                            "schema v{}  ·  {} track(s)  ·  {notes} note(s)  ·  no audio engine (shell only)",
                            self.project.version,
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
                self.project = project;
                self.project_path = Some(path.clone());
                self.position = 0;
                self.playing = false;
                self.load_error = None;
            }
            Err(error) => self.load_error = Some(error.to_string()),
        }
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
        self.advance(context);
        self.top_bar(context);
        self.status_bar(context);
        self.sidebar(context);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::TIMELINE))
            .show(context, |ui| {
                let view = self.view;
                let position = self.position;
                let Self { project, roll, .. } = self;
                match view {
                View::Arrange => {
                    piano_roll::show(ui, &project.song, roll, position);
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
            });
        if !self.shown {
            self.shown = true;
            context.send_viewport_cmd(ViewportCommand::Visible(true));
        }
    }
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
