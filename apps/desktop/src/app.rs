//! The Stage 6 shell: Perform / Arrange / Mix over the existing models.

use std::path::PathBuf;
use std::time::Duration;

use egui::{Context, Rect, ViewportCommand};
use music_core::{PPQ, Tick};
use project::song::{ProjectFile, Song};
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
    Loops,
    Score,
}

impl View {
    /// Every view, in tab order.
    const ALL: [Self; 5] = [
        Self::Perform,
        Self::Arrange,
        Self::Mix,
        Self::Loops,
        Self::Score,
    ];

    /// The views Kids Mode offers.
    #[must_use]
    fn allowed(kids_mode: bool) -> Vec<Self> {
        if kids_mode {
            vec![Self::Perform, Self::Arrange]
        } else {
            Self::ALL.to_vec()
        }
    }

    /// The translatable label key.
    fn key(self) -> crate::strings::Key {
        match self {
            Self::Perform => crate::strings::Key::Perform,
            Self::Arrange => crate::strings::Key::Arrange,
            Self::Mix => crate::strings::Key::Mix,
            Self::Loops => crate::strings::Key::Loops,
            Self::Score => crate::strings::Key::Score,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Perform => 0,
            Self::Arrange => 1,
            Self::Mix => 2,
            Self::Loops => 3,
            Self::Score => 4,
        }
    }
}

/// One row of the track list, snapshotted before drawing so the borrow ends.
struct Row {
    index: usize,
    id: TrackId,
    name: String,
    notes: usize,
    program: Option<u8>,
    muted: bool,
    soloed: bool,
}

pub struct ShellApp {
    editor: Editor,
    selected_note: Option<(TrackId, NoteId)>,
    selected_notes: Vec<(TrackId, NoteId)>,
    preview_player: Option<audio::player::Player>,
    selected_clip: Option<(TrackId, project::ClipId)>,
    chord_dialog: bool,
    chord_dialog_target: Tick,
    reharmonize_dialog: bool,
    chord_input: String,
    key: music_core::PitchClass,
    status: Option<String>,
    doctor_dialog: bool,
    doctor_proposals: Vec<arranging::Proposal>,
    form_dialog: bool,
    beat_dialog: bool,
    beat_track: Option<TrackId>,
    beat_start: Tick,
    beat_bar: Tick,
    beat_pattern: arranging::beat::BeatPattern,
    launcher: arranging::grid::Launcher,
    recent: Vec<std::path::PathBuf>,
    recent_file: std::path::PathBuf,
    midi_input: Option<midi_devices::devices::LiveInput>,
    midi_map: midi_devices::map::MidiMap,
    loop_enabled: bool,
    loop_region: Option<(Tick, Tick)>,
    loop_buffer: Option<audio::RenderedAudio>,
    lesson: crate::kids::Lesson,
    perform_player: Option<audio::player::Player>,
    library_open: bool,
    library_path: String,
    library_query: String,
    library: Option<crate::library_view::Browser>,
    pads_open: bool,
    pad_state: Option<crate::pads::Pads>,
    pad_path: String,
    pad_player: Option<audio::player::Player>,
    assistant_open: bool,
    assistant_input: String,
    assistant_log: Vec<String>,
    waveform: Option<(TrackId, String, Vec<audio::waveform::Peak>)>,
    midi_ports: Option<(Vec<String>, Vec<String>)>,
    settings: crate::settings::Settings,
    settings_open: bool,
    shortcuts_open: bool,
    score_entry: bool,
    score_page: usize,
    score_zoom: f32,
    rename_scene: Option<usize>,
    rename_scene_input: String,
    last_bar: i64,
    synth: arranging::SynthSpec,
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
            selected_notes: Vec::new(),
            preview_player: None,
            selected_clip: None,
            chord_dialog: false,
            chord_dialog_target: 0,
            reharmonize_dialog: false,
            chord_input: String::from("Am F C G"),
            key: music_core::PitchClass::new(0),
            status: None,
            doctor_dialog: false,
            doctor_proposals: Vec::new(),
            form_dialog: false,
            beat_dialog: false,
            beat_track: None,
            beat_start: 0,
            beat_bar: 3840,
            beat_pattern: arranging::beat::BeatPattern::default(),
            launcher: arranging::grid::Launcher::default(),
            recent_file: std::path::PathBuf::from("recent-projects.txt"),
            recent: project::recent::read(std::path::Path::new("recent-projects.txt")),
            midi_input: None,
            loop_enabled: false,
            loop_region: None,
            loop_buffer: None,
            lesson: crate::kids::Lesson::default(),
            perform_player: None,
            library_open: false,
            library_path: String::new(),
            library_query: String::new(),
            library: None,
            pads_open: false,
            pad_state: None,
            pad_path: String::new(),
            pad_player: None,
            assistant_open: false,
            assistant_input: String::new(),
            assistant_log: Vec::new(),
            waveform: None,
            midi_ports: None,
            midi_map: std::fs::read_to_string("lazy-arranger-midi-map.txt")
                .ok()
                .and_then(|text| midi_devices::map::MidiMap::from_text(&text).ok())
                .unwrap_or_default(),
            settings: crate::settings::Settings::load(&crate::settings::Settings::path()),
            settings_open: false,
            shortcuts_open: false,
            score_entry: false,
            score_page: 0,
            score_zoom: 1.0,
            rename_scene: None,
            rename_scene_input: String::new(),
            last_bar: 0,
            synth: arranging::SynthSpec::default(),
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
        let copy =
            context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::C));
        let cut =
            context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::X));
        let paste =
            context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::V));
        let select_all =
            context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::A));
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
        if select_all
            && let Some(track) = self
                .roll
                .selected
                .or_else(|| self.song().tracks.first().map(|track| track.id))
        {
            self.selected_notes = self
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .map(|entry| entry.notes.iter().map(|note| (track, note.id)).collect())
                .unwrap_or_default();
            self.status = Some(format!("selected {} note(s)", self.selected_notes.len()));
        }
        if let Some((track, note)) = self.selected_note {
            if copy {
                self.editor.copy_note(track, note);
                self.status = Some(String::from("note copied"));
            }
            if cut && self.editor.cut_note(track, note) {
                self.selected_note = None;
                self.status = Some(String::from("note cut"));
            }
        }
        if paste {
            let track = self
                .roll
                .selected
                .or_else(|| self.selected_note.map(|(track, _)| track))
                .or_else(|| self.song().tracks.first().map(|track| track.id));
            if let Some(track) = track {
                let at = snap(self.position, PPQ);
                match self.editor.paste_notes(track, at) {
                    Ok(0) => self.status = Some(String::from("clipboard is empty")),
                    Ok(count) => self.status = Some(format!("pasted {count} note(s)")),
                    Err(error) => self.status = Some(error),
                }
            }
        }
        if delete {
            if !self.selected_notes.is_empty() {
                let mut tracks: Vec<TrackId> = self
                    .selected_notes
                    .iter()
                    .map(|(track, _)| *track)
                    .collect();
                tracks.sort_by_key(|track| track.get());
                tracks.dedup();
                for track in tracks {
                    let ids: Vec<project::NoteId> = self
                        .selected_notes
                        .iter()
                        .filter(|(entry, _)| *entry == track)
                        .map(|(_, note)| *note)
                        .collect();
                    let _ = self.editor.delete_notes(track, &ids);
                }
                self.status = Some(format!("deleted {} note(s)", self.selected_notes.len()));
                self.selected_notes.clear();
            } else if let Some((track, clip)) = self.selected_clip.take() {
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
        if context.input(|input| input.key_pressed(egui::Key::G))
            && let Some(source) = self.roll.selected
        {
            match self
                .editor
                .apply_groove(source, arranging::groove::Amounts::default())
            {
                Ok(0) => self.status = Some(String::from("no other track to groove")),
                Ok(count) => self.status = Some(format!("groove applied to {count} track(s)")),
                Err(error) => self.status = Some(error),
            }
        }
        if context.input(|input| input.key_pressed(egui::Key::T)) {
            let bar = self.song().time_signature.bar_ticks().max(1);
            let join = snap(self.position, bar).max(bar);
            match self
                .editor
                .apply_transition(join, arranging::forms::Transition::Automatic)
            {
                Ok(0) => self.status = Some(String::from("nothing to add at that join")),
                Ok(added) => self.status = Some(format!("transition: {added} note(s)")),
                Err(error) => self.status = Some(error),
            }
        }
        if context.input(|input| input.key_pressed(egui::Key::B))
            && let Some(track) = self
                .roll
                .selected
                .or_else(|| self.song().tracks.first().map(|track| track.id))
        {
            let bar = self.song().time_signature.bar_ticks().max(1);
            let start = snap(self.position, bar);
            if let Some((bar, pattern)) = self.editor.beat_pattern(track, start) {
                self.beat_track = Some(track);
                self.beat_start = start;
                self.beat_bar = bar;
                self.beat_pattern = pattern;
                self.beat_dialog = true;
            }
        }
        if context.input(|input| input.key_pressed(egui::Key::Y)) {
            self.form_dialog = true;
        }
        if context.input(|input| input.key_pressed(egui::Key::D)) {
            self.doctor_proposals = self.editor.proposals();
            self.doctor_proposals
                .extend(self.editor.fit_proposals(self.synth));
            self.doctor_dialog = true;
        }
        if context.input(|input| input.key_pressed(egui::Key::F)) {
            match self.editor.find_chords() {
                Ok(0) => self.status = Some(String::from("no chords found")),
                Ok(count) => self.status = Some(format!("found {count} chord(s)")),
                Err(error) => self.status = Some(error),
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
                self.recent = project::recent::record(&self.recent_file, &path)
                    .unwrap_or_else(|_| self.recent.clone());
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
            if self.loop_enabled
                && self.playing
                && let Some(region) = self.loop_region
                && self.position >= region.1
            {
                self.position = region.0;
                if let Some(buffer) = self.loop_buffer.clone()
                    && let Ok(restart) = audio::player::Player::new(buffer)
                {
                    restart.play();
                    self.player = Some(restart);
                }
                self.advance_launcher();
                context.request_repaint_after(Duration::from_millis(16));
                return;
            }
            self.advance_launcher();
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

    /// Commits a queued scene when the playhead crosses a bar line.
    fn advance_launcher(&mut self) {
        let bar = self.song().time_signature.bar_ticks().max(1);
        let bar_index = self.position.div_euclid(bar);
        if self.launcher.playing && bar_index != self.last_bar {
            self.last_bar = bar_index;
            if let Some(scene) = self.launcher.advance(bar_index * bar) {
                self.status = Some(format!("scene {}", scene + 1));
            }
        }
    }

    /// Loads and caches the selected audio track's waveform peaks.
    fn refresh_waveform(&mut self) {
        if self.view != View::Arrange {
            return;
        }
        let Some(track) = self
            .roll
            .selected
            .or_else(|| {
                self.song()
                    .tracks
                    .iter()
                    .find(|track| track.audio.is_some())
                    .map(|track| track.id)
            })
            .and_then(|id| self.song().tracks.iter().find(|track| track.id == id))
        else {
            self.waveform = None;
            return;
        };
        let Some(reference) = &track.audio else {
            self.waveform = None;
            return;
        };
        if self
            .waveform
            .as_ref()
            .is_some_and(|(id, path, _)| *id == track.id && *path == reference.path)
        {
            return;
        }
        let peaks = std::fs::read(&reference.path)
            .ok()
            .and_then(|bytes| audio::wav::decode_mono(&bytes).ok())
            .map(|decoded| audio::waveform::peaks(&decoded.samples, 240))
            .unwrap_or_default();
        self.waveform = Some((track.id, reference.path.clone(), peaks));
    }

    fn draw_waveform(ui: &mut egui::Ui, peaks: &[audio::waveform::Peak]) {
        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(ui.available_width(), 44.0),
            egui::Sense::hover(),
        );
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, theme::LANE);
        if peaks.is_empty() {
            return;
        }
        let centre = rect.center().y;
        let height = rect.height() * 0.45;
        for (index, peak) in peaks.iter().enumerate() {
            let x = rect.left() + rect.width() * index as f32 / peaks.len() as f32;
            painter.line_segment(
                [
                    egui::Pos2::new(x, centre - peak.max * height),
                    egui::Pos2::new(x, centre - peak.min * height),
                ],
                egui::Stroke::new(1.0_f32, theme::ACCENT),
            );
        }
        painter.line_segment(
            [
                egui::Pos2::new(rect.left(), centre),
                egui::Pos2::new(rect.right(), centre),
            ],
            egui::Stroke::new(0.5_f32, theme::LINE),
        );
    }

    /// Opens the configured keyboard once and applies mapped controls.
    fn poll_midi(&mut self) {
        if self.midi_input.is_none()
            && let Some(name) = self.settings.keyboard.clone()
        {
            match midi_devices::devices::LiveInput::open(&name) {
                Ok(input) => self.midi_input = Some(input),
                Err(error) => {
                    self.status = Some(format!("MIDI input {name}: {error}"));
                    self.settings.keyboard = None;
                }
            }
        }
        let mut actions = Vec::new();
        if let Some(input) = &self.midi_input {
            while let Some(message) = input.next() {
                if let Some(action) = self.midi_map.action_for(&message) {
                    let value = midi_devices::map::value_of(&message);
                    actions.push((action, value));
                }
            }
        }
        for (action, value) in actions {
            self.apply_midi_action(action, value);
        }
    }

    /// Runs one mapped action.
    fn apply_midi_action(&mut self, action: midi_devices::map::Action, value: Option<u8>) {
        use midi_devices::map::Action;
        match action {
            Action::Play => {
                if !self.playing {
                    self.toggle_playback();
                }
            }
            Action::Stop => {
                if self.playing {
                    self.stop_playback();
                }
            }
            Action::Undo => {
                if !self.editor.undo() {
                    self.status = Some(String::from("nothing to undo"));
                }
            }
            Action::Scene(index) => {
                self.launcher.launch(index);
                self.status = Some(format!("scene {}", index + 1));
            }
            Action::Pad(note) => {
                let Some(pads) = &self.pad_state else {
                    self.status = Some(String::from("load a loop into Drum pads first"));
                    return;
                };
                let Some(pad) = pads.kit.pads.iter().find(|pad| pad.note == note).copied() else {
                    self.status = Some(format!("pad {note} is empty"));
                    return;
                };
                match audio::player::Player::new(pads.audio(pad)) {
                    Ok(player) => {
                        self.pad_player = Some(player);
                        self.status = Some(format!("pad {note}"));
                    }
                    Err(error) => self.status = Some(error),
                }
            }
            Action::TrackFader(track) => {
                let Some(value) = value else {
                    return;
                };
                let pan = self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == track)
                    .map(|entry| entry.pan)
                    .unwrap_or(64);
                if let Err(error) = self.editor.set_track_gain(track, value.min(127), pan) {
                    self.status = Some(error);
                }
            }
        }
    }

    /// The Music Finder and playlist browser.
    fn library_window(&mut self, context: &egui::Context) {
        if !self.library_open {
            return;
        }
        let mut open = self.library_open;
        egui::Window::new("Library")
            .open(&mut open)
            .default_width(520.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.library_path)
                            .hint_text("Music Finder .json/.csv/.mfd or playlist .tsv")
                            .desired_width(320.0),
                    );
                    if ui.button("Load").clicked() {
                        match crate::library_view::load(std::path::Path::new(&self.library_path)) {
                            Ok(browser) => {
                                self.status = Some(format!("{} row(s) loaded", browser.rows.len()));
                                self.library = Some(browser);
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
                if let Some(browser) = &self.library {
                    if let Some(action) =
                        crate::library_view::show(ui, browser, &mut self.library_query)
                    {
                        let index = match action {
                            crate::library_view::Action::Apply(index)
                            | crate::library_view::Action::Import(index) => index,
                        };
                        let row = &browser.rows[index];
                        match action {
                            crate::library_view::Action::Apply(_) => match row.tempo {
                                Some(bpm) => match self.editor.set_tempo(bpm) {
                                    Ok(()) => {
                                        self.status =
                                            Some(format!("{}: tempo {bpm} bpm", row.title))
                                    }
                                    Err(error) => self.status = Some(error),
                                },
                                None => {
                                    self.status = Some(format!("{}: playlist entry", row.title))
                                }
                            },
                            crate::library_view::Action::Import(_) => {
                                let root = if self.settings.style_library.is_empty() {
                                    std::env::var("ARRANGER_STYLE_LIBRARY").unwrap_or_default()
                                } else {
                                    self.settings.style_library.clone()
                                };
                                let hint = row
                                    .style
                                    .as_deref()
                                    .unwrap_or(&row.title)
                                    .rsplit_once('.')
                                    .map_or_else(
                                        || row.style.as_deref().unwrap_or(&row.title).to_string(),
                                        |(stem, _)| stem.to_string(),
                                    );
                                let chosen = if root.is_empty() {
                                    Err(String::from(
                                        "set a style library folder in Settings first",
                                    ))
                                } else {
                                    style_formats::catalog::catalog(std::path::Path::new(&root))
                                        .map_err(|error| error.to_string())
                                        .and_then(|entries| {
                                            style_formats::catalog::resolve(
                                                &entries,
                                                row.style_number,
                                                &hint,
                                            )
                                            .map(|entry| entry.path.clone())
                                            .ok_or_else(|| format!("no style matched {hint:?}"))
                                        })
                                };
                                match chosen {
                                    Ok(path) => {
                                        match self.editor.import_style_tracks(&path, "Style") {
                                            Ok(imported) => {
                                                self.status = Some(format!(
                                                    "imported {} track(s), {} note(s) from {}",
                                                    imported.tracks,
                                                    imported.notes,
                                                    path.display()
                                                ));
                                            }
                                            Err(error) => self.status = Some(error),
                                        }
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                        }
                    }
                } else {
                    ui.label(
                        egui::RichText::new("Load a book to browse it.")
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                }
            });
        if !open {
            self.library_open = false;
        }
    }

    /// The drum pad window.
    fn pads_window(&mut self, context: &egui::Context) {
        if !self.pads_open {
            return;
        }
        let mut open = self.pads_open;
        egui::Window::new("Drum pads")
            .open(&mut open)
            .default_width(380.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pad_path)
                            .hint_text("loop .wav path")
                            .desired_width(240.0),
                    );
                    if ui.button("Load").clicked() {
                        match crate::pads::Pads::load(std::path::Path::new(&self.pad_path)) {
                            Ok(pads) => {
                                self.status =
                                    Some(format!("{} pad(s) loaded", pads.kit.pads.len()));
                                self.pad_state = Some(pads);
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
                if let Some(pads) = &self.pad_state {
                    if let Some(pad) = crate::pads::show(ui, pads) {
                        match audio::player::Player::new(pads.audio(pad)) {
                            Ok(player) => self.pad_player = Some(player),
                            Err(error) => self.status = Some(error),
                        }
                    }
                } else {
                    ui.label(
                        egui::RichText::new("Load a loop to slice it into pads.")
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                }
            });
        if !open {
            self.pads_open = false;
        }
    }

    /// The local assistant panel.
    fn assistant_window(&mut self, context: &egui::Context) {
        if !self.assistant_open {
            return;
        }
        let mut open = self.assistant_open;
        egui::Window::new("Assistant")
            .open(&mut open)
            .default_width(420.0)
            .show(context, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.assistant_log.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "Ask for help, a song summary, undo, or run any tool:\n                                     tool add_track {\"name\":\"Bass\"}",
                                )
                                .size(10.0)
                                .color(theme::TEXT_DIM),
                            );
                        }
                        for line in &self.assistant_log {
                            ui.label(egui::RichText::new(line).monospace().size(10.0));
                        }
                    });
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.assistant_input)
                            .hint_text("help")
                            .desired_width(320.0),
                    );
                    let enter = response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if (ui.button("Send").clicked() || enter) && !self.assistant_input.is_empty() {
                        let input = self.assistant_input.trim().to_string();
                        if input == "undo" || input.starts_with("tool ") {
                            self.selected_note = None;
                            self.selected_clip = None;
                        }
                        let reply = self.editor.assistant(&input);
                        self.assistant_log.push(format!("> {input}"));
                        if self.settings.speech
                            && let Err(error) = crate::speech::speak(&reply.text)
                        {
                            self.status = Some(error);
                        }
                        self.assistant_log.push(reply.text);
                        self.assistant_input.clear();
                    }
                });
                let mut speech = self.settings.speech;
                if ui.checkbox(&mut speech, "Speak replies").changed() {
                    self.settings.speech = speech;
                    let _ = self.settings.save(&crate::settings::Settings::path());
                }
                ui.label(
                    egui::RichText::new(
                        "Local commands over the same 26 tools; no network or model.\
                         Speech out uses the macOS say command; speech input is not implemented.",
                    )
                    .size(9.0)
                    .color(theme::TEXT_DIM),
                );
            });
        if !open {
            self.assistant_open = false;
        }
    }

    /// Settings and shortcut windows.
    fn windows(&mut self, context: &egui::Context) {
        if self.settings_open {
            let mut open = self.settings_open;
            egui::Window::new("Settings")
                .open(&mut open)
                .collapsible(false)
                .show(context, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Language");
                        egui::ComboBox::from_id_salt("settings-language")
                            .selected_text(&self.settings.language)
                            .show_ui(ui, |ui| {
                                for tag in ["en", "da", "fr", "vi"] {
                                    ui.selectable_value(
                                        &mut self.settings.language,
                                        tag.to_string(),
                                        tag,
                                    );
                                }
                            });
                        ui.label(
                            egui::RichText::new(
                                "English, Danish, French and Vietnamese ship strings",
                            )
                            .size(9.0)
                            .color(theme::TEXT_DIM),
                        );
                    });
                    ui.add(egui::Slider::new(&mut self.settings.grade, 1..=9).text("chord grade"));
                    let mut keyboard = self.settings.keyboard.clone().unwrap_or_default();
                    if ui
                        .add(egui::TextEdit::singleline(&mut keyboard).hint_text("MIDI keyboard"))
                        .changed()
                    {
                        self.settings.keyboard = if keyboard.trim().is_empty() {
                            None
                        } else {
                            Some(keyboard)
                        };
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Find MIDI inputs").clicked() {
                            match midi_devices::devices::list_ports() {
                                Ok(names) => {
                                    self.status = Some(crate::settings::ports_summary(
                                        &names.inputs,
                                        &names.outputs,
                                    ));
                                    self.midi_ports = Some((names.inputs, names.outputs));
                                }
                                Err(error) => self.status = Some(error.to_string()),
                            }
                        }
                        if let Some((inputs, _)) = &self.midi_ports {
                            for name in inputs {
                                if ui
                                    .selectable_label(
                                        self.settings.keyboard.as_deref() == Some(name.as_str()),
                                        name,
                                    )
                                    .clicked()
                                {
                                    self.settings.keyboard = Some(name.clone());
                                    self.midi_input = None;
                                    let _ = self.settings.save(&crate::settings::Settings::path());
                                    self.status = Some(format!("MIDI input: {name}"));
                                }
                            }
                        }
                    });
                    if ui.button("Check for updates").clicked() {
                        self.status = Some(String::from(crate::settings::update_status()));
                    }
                    ui.checkbox(&mut self.settings.metronome, "metronome with playback");
                    ui.checkbox(&mut self.settings.reduced_motion, "reduced motion");
                    ui.checkbox(&mut self.settings.kids_mode, "Kids Mode");
                    if ui.button("Save settings").clicked() {
                        match self.settings.save(&crate::settings::Settings::path()) {
                            Ok(()) => {
                                self.status = Some(String::from("settings saved"));
                                self.settings_open = false;
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
            if !open {
                self.settings_open = false;
            }
        }
        if self.shortcuts_open {
            let mut open = self.shortcuts_open;
            egui::Window::new("Shortcuts")
                .open(&mut open)
                .collapsible(false)
                .show(context, |ui| {
                    for shortcut in crate::settings::SHORTCUTS {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(shortcut.keys).monospace().size(10.0));
                            ui.label(shortcut.action);
                            ui.label(
                                egui::RichText::new(shortcut.context)
                                    .size(9.0)
                                    .color(theme::TEXT_DIM),
                            );
                        });
                    }
                });
            if !open {
                self.shortcuts_open = false;
            }
        }
    }

    /// Mixer strips: one per track with a fader, pan and mute/solo.
    fn mixer_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mixer");
        let strips: Vec<(TrackId, String, u8, u8, bool, bool)> = self
            .song()
            .tracks
            .iter()
            .map(|track| {
                (
                    track.id,
                    track.name.clone(),
                    track.volume,
                    track.pan,
                    track.muted,
                    track.soloed,
                )
            })
            .collect();
        if strips.is_empty() {
            ui.label(egui::RichText::new("No tracks to mix.").color(theme::TEXT_MUTED));
            return;
        }
        let buses = self.song().buses.clone();
        for bus in &buses {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("BUS {}", bus.name))
                        .size(11.0)
                        .color(theme::ACCENT),
                );
                let mut level = i32::from(bus.volume);
                if ui
                    .add(egui::Slider::new(&mut level, 0..=127).text("vol"))
                    .changed()
                {
                    let mut updated = buses.clone();
                    if let Some(target) = updated.iter_mut().find(|entry| entry.id == bus.id) {
                        target.volume = u8::try_from(level).unwrap_or(bus.volume);
                    }
                    let _ = self.editor.set_buses(updated);
                }
                let mut position = i32::from(bus.pan);
                if ui
                    .add(egui::Slider::new(&mut position, 0..=127).text("pan"))
                    .changed()
                {
                    let mut updated = buses.clone();
                    if let Some(target) = updated.iter_mut().find(|entry| entry.id == bus.id) {
                        target.pan = u8::try_from(position).unwrap_or(bus.pan);
                    }
                    let _ = self.editor.set_buses(updated);
                }
            });
        }
        for (id, name, volume, pan, muted, soloed) in strips {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if name.is_empty() { "untitled" } else { &name })
                        .size(11.0),
                );
                let mut level = i32::from(volume);
                let response = ui.add(
                    egui::Slider::new(&mut level, 0..=127)
                        .text("vol")
                        .show_value(true),
                );
                if response.changed() {
                    let _ =
                        self.editor
                            .set_track_gain(id, u8::try_from(level).unwrap_or(volume), pan);
                }
                let mut position = i32::from(pan);
                let pan_response = ui.add(
                    egui::Slider::new(&mut position, 0..=127)
                        .text("pan")
                        .show_value(true),
                );
                if pan_response.changed() {
                    let _ = self.editor.set_track_gain(
                        id,
                        volume,
                        u8::try_from(position).unwrap_or(pan),
                    );
                }
                let routing = self
                    .song()
                    .tracks
                    .iter()
                    .find(|track| track.id == id)
                    .map(|track| (track.bus, track.send, track.send_level))
                    .unwrap_or((None, None, 100));
                let mut bus_choice = routing.0;
                egui::ComboBox::from_id_salt(format!("bus-{id}"))
                    .selected_text(match bus_choice {
                        Some(bus) => format!("Bus {bus}"),
                        None => String::from("Master"),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut bus_choice, None, "Master");
                        for bus in &buses {
                            ui.selectable_value(
                                &mut bus_choice,
                                Some(bus.id),
                                format!("{} ({})", bus.name, bus.id),
                            );
                        }
                    });
                if bus_choice != routing.0 {
                    let _ = self
                        .editor
                        .set_track_routing(id, bus_choice, routing.1, routing.2);
                }
                let mut send_choice = routing.1;
                egui::ComboBox::from_id_salt(format!("send-{id}"))
                    .selected_text(match send_choice {
                        Some(bus) => format!("Send {bus}"),
                        None => String::from("No send"),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut send_choice, None, "No send");
                        for bus in &buses {
                            ui.selectable_value(
                                &mut send_choice,
                                Some(bus.id),
                                format!("Send {}", bus.id),
                            );
                        }
                    });
                if send_choice != routing.1 {
                    let _ = self
                        .editor
                        .set_track_routing(id, routing.0, send_choice, routing.2);
                }
                if let Some(_send) = send_choice {
                    let mut level = i32::from(routing.2);
                    if ui
                        .add(egui::Slider::new(&mut level, 0..=127).text("send"))
                        .changed()
                    {
                        let _ = self.editor.set_track_routing(
                            id,
                            routing.0,
                            send_choice,
                            u8::try_from(level).unwrap_or(routing.2),
                        );
                    }
                }
                if ui.selectable_label(muted, "M").clicked() {
                    let _ = self.editor.set_track_flags(id, !muted, soloed, false);
                }
                if ui.selectable_label(soloed, "S").clicked() {
                    let _ = self.editor.set_track_flags(id, muted, !soloed, false);
                }
            });
        }
        ui.label(
            egui::RichText::new("Fader and pan write through one undoable command each.")
                .size(10.0)
                .color(theme::TEXT_DIM),
        );
    }

    /// The clip grid: scenes as rows, tracks as columns, launch and record.
    fn grid_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading(format!(
            "Clip grid · {} scene(s) of two bars",
            self.editor.grid(2).rows.len()
        ));
        let grid = self.editor.grid(2);
        for (scene_index, row) in grid.rows.iter().enumerate() {
            ui.horizontal(|ui| {
                let playing = self.launcher.current == Some(scene_index);
                let waiting = self.launcher.waiting == Some(scene_index);
                let label = if playing {
                    format!("▶ {}", row.scene.name)
                } else if waiting {
                    format!("… {}", row.scene.name)
                } else {
                    row.scene.name.clone()
                };
                if ui.small_button("✎").on_hover_text("Rename scene").clicked() {
                    self.rename_scene = Some(scene_index);
                    self.rename_scene_input = row.scene.name.clone();
                }
                if ui.selectable_label(playing || waiting, label).clicked() {
                    if self.launcher.playing && self.launcher.current == Some(scene_index) {
                        self.launcher.stop();
                        self.status = Some(String::from("stopped"));
                    } else {
                        self.launcher.launch(scene_index);
                        self.status = Some(format!("scene {} queued", scene_index + 1));
                    }
                }
                for (column, slot) in row.slots.iter().enumerate() {
                    let name = slot.clip.map_or_else(
                        || String::from("—"),
                        |clip| {
                            grid.tracks
                                .get(column)
                                .and_then(|track| {
                                    self.song().tracks.iter().find(|entry| entry.id == *track)
                                })
                                .and_then(|track| track.clips.iter().find(|entry| entry.id == clip))
                                .map_or_else(|| format!("clip {clip}"), |entry| entry.name.clone())
                        },
                    );
                    let _ = ui.button(name);
                }
            });
        }
        ui.horizontal(|ui| {
            if ui
                .button(format!(
                    "Record take ({} scene(s))",
                    self.launcher.log.len()
                ))
                .clicked()
            {
                let log = self.launcher.log.clone();
                match self.editor.record_grid(&log) {
                    Ok(copied) => {
                        self.status = Some(format!("take: {copied} note(s)"));
                        self.launcher.log.clear();
                    }
                    Err(error) => self.status = Some(error),
                }
            }
            if ui.button("Stop").clicked() {
                self.launcher.stop();
            }
        });
        ui.label(
            egui::RichText::new(
                "Click a scene to queue it; right-click a slot to clear, resize or copy it.",
            )
            .size(10.0)
            .color(theme::TEXT_DIM),
        );
        if let Some(index) = self.rename_scene {
            let mut open = true;
            egui::Window::new("Rename scene")
                .collapsible(false)
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rename_scene_input)
                            .desired_width(180.0),
                    );
                    if ui.button("Apply").clicked() {
                        let name = self.rename_scene_input.clone();
                        match self.editor.rename_scene(index, &name) {
                            Ok(()) => self.rename_scene = None,
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
            if !open {
                self.rename_scene = None;
            }
        }
    }

    /// The Beat view: four drum rows of sixteen steps for one bar.
    fn beat_dialog(&mut self, context: &Context) {
        if !self.beat_dialog {
            return;
        }
        let mut open = self.beat_dialog;
        egui::Window::new("Beat")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(format!(
                    "bar {} · click to cycle off/soft/normal/loud",
                    self.beat_start / self.beat_bar.max(1) + 1
                ));
                for (row, (_, label)) in arranging::beat::ROWS.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(*label).size(10.0));
                        for slot in 0..arranging::beat::STEPS {
                            let level = self.beat_pattern.grid[row][slot];
                            let text = if level == 0 {
                                String::from("·")
                            } else {
                                level.to_string()
                            };
                            if ui.small_button(text).clicked() {
                                self.beat_pattern.cycle(row, slot);
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked()
                        && let Some(track) = self.beat_track
                    {
                        let bar = self.beat_bar;
                        let start = self.beat_start;
                        match self
                            .editor
                            .apply_beat(track, start, bar, &self.beat_pattern)
                        {
                            Ok(added) => {
                                self.status = Some(format!("beat: {added} hit(s)"));
                                self.beat_dialog = false;
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Close").clicked() {
                        self.beat_dialog = false;
                    }
                });
            });
        self.beat_dialog = open && self.beat_dialog;
    }

    /// Song Forms: four preset forms built from the song's markers.
    fn form_dialog(&mut self, context: &Context) {
        if !self.form_dialog {
            return;
        }
        let mut open = self.form_dialog;
        egui::Window::new("Song Forms")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                let sections = self.editor.sections();
                if sections.is_empty() {
                    ui.label("No sections: add markers to the song first.");
                }
                for (label, form) in [
                    ("Short", arranging::forms::short(&sections)),
                    ("Gig", arranging::forms::gig(&sections)),
                    ("Long intro", arranging::forms::long_intro(&sections)),
                    ("Instrumental", arranging::forms::instrumental(&sections)),
                ] {
                    if ui.button(label).clicked() {
                        match self.editor.apply_form(&form) {
                            Ok(copied) => {
                                self.status = Some(format!("{label}: {copied} note(s) placed"));
                                self.form_dialog = false;
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                }
            });
        self.form_dialog = open && self.form_dialog;
    }

    /// The Arrangement Doctor's proposal cards.
    fn doctor_dialog(&mut self, context: &Context) {
        if !self.doctor_dialog {
            return;
        }
        let mut open = self.doctor_dialog;
        egui::Window::new("Arrangement Doctor")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                if self.doctor_proposals.is_empty() {
                    ui.label("Nothing to suggest: no crowded or repeated bars.");
                }
                let mut used = None;
                for (index, proposal) in self.doctor_proposals.iter().enumerate() {
                    ui.group(|ui| {
                        ui.label(egui::RichText::new(&proposal.title).strong());
                        ui.label(&proposal.detail);
                        if ui.button("Use").clicked() {
                            used = Some(index);
                        }
                    });
                }
                if let Some(index) = used {
                    let proposal = self.doctor_proposals[index].clone();
                    match self.editor.apply_proposal(&proposal) {
                        Ok(changed) => {
                            self.status = Some(format!("applied: {changed} note(s) changed"));
                            self.doctor_proposals.remove(index);
                        }
                        Err(error) => self.load_error = Some(error),
                    }
                }
            });
        self.doctor_dialog = open && self.doctor_dialog;
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
        let mut audio = audio::render(self.song(), audio::SAMPLE_RATE);
        let problems = audio::mix::mix_audio_references(&mut audio, self.song(), 0.9);
        if self.settings.metronome {
            audio::metronome::mix(&mut audio, self.song(), 1.0);
        }
        if let Some(problem) = problems.first() {
            self.status = Some(format!("audio {}: {}", problem.path, problem.reason));
        }
        if self.loop_enabled
            && let Some((start, end)) = self.loop_region
        {
            let from = self.song().tempo_map.seconds_at(start);
            let to = self.song().tempo_map.seconds_at(end);
            let sliced = audio.slice(from, to);
            self.loop_buffer = Some(sliced.clone());
            match audio::player::Player::new(sliced) {
                Ok(player) => {
                    self.position = start;
                    self.player = Some(player);
                    self.playing = true;
                    if let Some(player) = &self.player {
                        player.play();
                    }
                    return;
                }
                Err(error) => {
                    self.status = Some(error);
                    return;
                }
            }
        }
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
                if self.settings.kids_mode {
                    ui.spacing_mut().button_padding = egui::Vec2::new(10.0, 7.0);
                }
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
                if ui
                    .selectable_label(self.loop_enabled, "Loop")
                    .on_hover_text("Repeat the loop region")
                    .clicked()
                {
                    self.loop_enabled = !self.loop_enabled;
                    if self.loop_enabled && self.loop_region.is_none() {
                        let bar = self.song().time_signature.bar_ticks().max(1);
                        self.loop_region = Some(crate::transport::bar_region(self.position, bar));
                    }
                }
                if ui
                    .small_button("From bar")
                    .on_hover_text("Set the loop to the current bar")
                    .clicked()
                {
                    let bar = self.song().time_signature.bar_ticks().max(1);
                    self.loop_region = Some(crate::transport::bar_region(self.position, bar));
                    self.loop_enabled = true;
                    self.loop_buffer = None;
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
                let views = View::allowed(self.settings.kids_mode);
                let names: Vec<&str> = views
                    .iter()
                    .map(|view| {
                        crate::strings::text(
                            crate::strings::Language::parse(&self.settings.language),
                            view.key(),
                        )
                    })
                    .collect();
                let selected = views
                    .iter()
                    .position(|view| *view == self.view)
                    .unwrap_or(0);
                if let Some(index) = widgets::view_tabs(&mut tabs_ui, selected, &names)
                    && let Some(view) = views.get(index)
                {
                    self.view = *view;
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
                let rows: Vec<Row> = self
                    .song()
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(index, track)| Row {
                        index,
                        id: track.id,
                        name: track.name.clone(),
                        notes: track.notes.len(),
                        program: track.program,
                        muted: track.muted,
                        soloed: track.soloed,
                    })
                    .collect();
                for row in rows {
                    self.track_row(ui, &row);
                }
                if self.song().tracks.is_empty() {
                    ui.label(egui::RichText::new("no tracks").color(theme::TEXT_DIM));
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if !self.recent.is_empty() {
                        ui.label(
                            egui::RichText::new("RECENT")
                                .size(9.0)
                                .color(theme::TEXT_DIM),
                        );
                        let entries = self.recent.clone();
                        for entry in entries {
                            let label = entry
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("project")
                                .to_string();
                            if ui
                                .button(label)
                                .on_hover_text(entry.display().to_string())
                                .clicked()
                            {
                                self.open(&entry);
                            }
                        }
                    }
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

    fn track_row(&mut self, ui: &mut egui::Ui, row: &Row) {
        let selected = self.roll.selected == Some(row.id);
        let colour = theme::track_colour(row.index);
        let response = ui
            .horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(6.0, 28.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, colour);
                if ui
                    .selectable_label(row.muted, "M")
                    .on_hover_text("Mute")
                    .clicked()
                {
                    let _ = self
                        .editor
                        .set_track_flags(row.id, !row.muted, row.soloed, false);
                }
                if ui
                    .selectable_label(row.soloed, "S")
                    .on_hover_text("Solo")
                    .clicked()
                {
                    let _ = self
                        .editor
                        .set_track_flags(row.id, row.muted, !row.soloed, false);
                }
                ui.vertical(|ui| {
                    let name = if row.name.is_empty() {
                        "untitled"
                    } else {
                        row.name.as_str()
                    };
                    ui.label(egui::RichText::new(name).size(12.0).color(if selected {
                        theme::TEXT
                    } else {
                        theme::TEXT_MUTED
                    }));
                    let detail = match row.program {
                        Some(program) => format!("{} notes · prog {program}", row.notes),
                        None => format!("{} notes", row.notes),
                    };
                    ui.label(egui::RichText::new(detail).size(9.0).color(theme::TEXT_DIM));
                });
            })
            .response;
        if response.interact(egui::Sense::click()).clicked() {
            self.roll.selected = if selected { None } else { Some(row.id) };
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
                    if let Some(status) = &self.status {
                        ui.label(
                            egui::RichText::new(status)
                                .size(10.0)
                                .color(theme::TEXT_MUTED),
                        );
                    }
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
                self.recent = project::recent::record(&self.recent_file, path)
                    .unwrap_or_else(|_| self.recent.clone());
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
        self.poll_midi();
        self.refresh_waveform();
        if self.settings.kids_mode && self.view.index() > 1 {
            self.view = View::Arrange;
        }
        self.run_autosave();
        self.recovery_dialog(context);
        self.chord_dialog(context);
        self.reharmonize_dialog(context);
        self.doctor_dialog(context);
        self.form_dialog(context);
        self.beat_dialog(context);
        self.advance(context);
        self.top_bar(context);
        self.windows(context);
        self.assistant_window(context);
        self.pads_window(context);
        self.library_window(context);
        self.status_bar(context);
        self.sidebar(context);
        let kids = self.settings.kids_mode;
        if kids {
            self.lesson
                .sync(self.song().chords.iter().map(|event| event.chord).collect());
        }
        let lesson = self.lesson.clone();
        let pads = music_core::roman::pads(self.key);
        let waveform = self.waveform.clone();
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::TIMELINE))
            .show(context, |ui| {
                let view = self.view;
                let position = self.position;
                let mut clicked = None;
                let mut automation_edit: Option<(TrackId, project::song::Automation)> = None;
                let mut audio_trim: Option<(TrackId, Tick)> = None;
                let mut score_status: Option<String> = None;
                let mut perform_chord: Option<music_core::Chord> = None;
                let mut lesson_next = false;
                let mut lesson_reset = false;
                let mut kids_load = false;
                let mut scroll_edit: Option<crate::piano_roll::RollEdit> = None;
                let mut velocity_edit: Option<(TrackId, project::NoteId, u8)> = None;
                let mut preview_note: Option<u8> = None;
                let Self { editor, roll, .. } = self;
                let selected_clip = self.selected_clip;
                let project = editor.project();
                match view {
                    View::Arrange => {
                        if let Some((track_id, path, peaks)) = &waveform
                            && roll.selected == Some(*track_id)
                        {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(
                                        std::path::Path::new(path)
                                            .file_name()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or("audio"),
                                    )
                                    .size(10.0)
                                    .color(theme::TEXT_MUTED),
                                );
                                if ui.button("Trim 1 beat").clicked() {
                                    let bar = project.song.time_signature.bar_ticks().max(1);
                                    audio_trim = Some((*track_id, bar / 4));
                                }
                            });
                            Self::draw_waveform(ui, peaks);
                        }
                        let outcome = piano_roll::show(
                            ui,
                            &project.song,
                            roll,
                            position,
                            selected_clip.map(|(_, clip)| clip),
                        );
                        clicked = outcome.click;
                        if let Some(edit) = outcome.edit {
                            scroll_edit = Some(edit);
                        }
                        if let Some(velocity) = outcome.velocity {
                            velocity_edit = Some(velocity);
                        }
                        if let Some(preview) = outcome.preview {
                            preview_note = Some(preview);
                        }
                        if let Some(track_id) = roll.selected
                            && let Some(track) = project
                                .song
                                .tracks
                                .iter()
                                .find(|track| track.id == track_id)
                        {
                            let lane = track
                                .automation
                                .iter()
                                .find(|lane| lane.target == project::song::AutomationTarget::Volume)
                                .cloned()
                                .unwrap_or(project::song::Automation {
                                    target: project::song::AutomationTarget::Volume,
                                    points: Vec::new(),
                                });
                            if let Some(edited) = crate::automation_view::show(
                                ui,
                                &lane,
                                project.song.end_tick().max(1),
                            ) {
                                automation_edit = Some((track_id, edited));
                            }
                        }
                    }
                    View::Perform => {
                        if kids && !lesson.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("Play the next chord")
                                        .size(14.0)
                                        .color(theme::TEXT_MUTED),
                                );
                                let current = lesson
                                    .current()
                                    .map(|chord| chord.format(music_core::Spelling::Sharp))
                                    .unwrap_or_else(|| String::from("done"));
                                ui.label(egui::RichText::new(current).size(34.0));
                                let (at, total) = lesson.progress();
                                ui.label(format!("chord {at} of {total}"));
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_sized([140.0, 40.0], egui::Button::new("Next chord"))
                                        .clicked()
                                    {
                                        lesson_next = true;
                                    }
                                    if ui
                                        .add_sized([140.0, 40.0], egui::Button::new("Start over"))
                                        .clicked()
                                    {
                                        lesson_reset = true;
                                    }
                                    if ui
                                        .add_sized(
                                            [150.0, 40.0],
                                            egui::Button::new("Load kids demo"),
                                        )
                                        .clicked()
                                    {
                                        kids_load = true;
                                    }
                                });
                            });
                            ui.separator();
                        }
                        ui.label(
                            egui::RichText::new("Chord pads")
                                .size(12.0)
                                .color(theme::TEXT_DIM),
                        );
                        for row in 0..4 {
                            ui.horizontal(|ui| {
                                for column in 0..4 {
                                    let index = row * 4 + column;
                                    let Some(chord) = pads.get(index) else {
                                        continue;
                                    };
                                    let text = chord.format(music_core::Spelling::Sharp);
                                    if ui
                                        .add_sized(
                                            [86.0, 40.0],
                                            egui::Button::new(
                                                egui::RichText::new(&text).size(12.0),
                                            ),
                                        )
                                        .clicked()
                                    {
                                        perform_chord = Some(*chord);
                                    }
                                }
                            });
                        }
                    }
                    View::Mix => {
                        self.mixer_panel(ui);
                    }
                    View::Loops => {
                        self.grid_panel(ui);
                    }
                    View::Score => {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.score_entry, "Note entry");
                            if ui.button("◀ Page").clicked() {
                                self.score_page = self.score_page.saturating_sub(1);
                            }
                            if ui.button("Page ▶").clicked() {
                                self.score_page += 1;
                            }
                            ui.add(
                                egui::Slider::new(&mut self.score_zoom, 0.5..=2.5)
                                    .text("zoom")
                                    .fixed_decimals(1),
                            );
                        });
                        if let Some(track) = roll
                            .selected
                            .or_else(|| project.song.tracks.first().map(|track| track.id))
                        {
                            let engraved =
                                score::engrave(&project.song, track, score::Settings::default());
                            self.score_page =
                                crate::score_view::clamp_page(&engraved, self.score_page);
                            let zoom = self.score_zoom;
                            let page = self.score_page;
                            if let Some(hit) = crate::score_view::show(ui, &engraved, page, zoom)
                                && self.score_entry
                            {
                                match music_core::Pitch::new(hit.pitch) {
                                    Ok(pitch) => match editor.insert(track, hit.tick, pitch) {
                                        Ok(_) => {
                                            score_status = Some(format!(
                                                "inserted {} at tick {}",
                                                pitch.as_u8(),
                                                hit.tick
                                            ));
                                        }
                                        Err(error) => score_status = Some(error.to_string()),
                                    },
                                    Err(error) => score_status = Some(error.to_string()),
                                }
                            }
                        } else {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    egui::RichText::new("Add a track to see a score.")
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                        }
                    }
                }
                (
                    clicked,
                    automation_edit,
                    audio_trim,
                    score_status,
                    perform_chord,
                    lesson_next,
                    lesson_reset,
                    kids_load,
                    scroll_edit,
                    velocity_edit,
                    preview_note,
                )
            });
        let (
            clicked,
            automation_edit,
            audio_trim,
            score_status,
            perform_chord,
            lesson_next,
            lesson_reset,
            kids_load,
            scroll_edit,
            velocity_edit,
            preview_note,
        ) = panel.inner;
        if let Some(click) = clicked {
            self.edit_click(click);
        }
        if let Some(pitch) = preview_note {
            let mut song = Song::new("Preview", 120.0);
            let mut track = project::song::Track::new(TrackId::new(1).expect("valid"), "Preview");
            if let (Ok(pitch), Ok(id)) = (music_core::Pitch::new(pitch), NoteId::new(1)) {
                track.notes.push(project::song::Note {
                    id,
                    start: 0,
                    duration: music_core::PPQ / 2,
                    pitch,
                    velocity: music_core::Velocity::new(100).expect("valid"),
                    channel: music_core::Channel::first(),
                });
            }
            song.tracks.push(track);
            let rendered = audio::render(&song, audio::SAMPLE_RATE);
            self.preview_player = audio::player::Player::new(rendered).ok();
        }
        if let Some((track, note, velocity)) = velocity_edit
            && let Err(error) = self.editor.set_velocity(track, note, velocity)
        {
            self.status = Some(error);
        }
        if let Some(edit) = scroll_edit {
            let result = match edit.mode {
                crate::piano_roll::DragMode::Resize => {
                    self.editor
                        .resize_note(edit.track, edit.note, edit.duration)
                }
                crate::piano_roll::DragMode::Move => match music_core::Pitch::new(edit.pitch) {
                    Ok(pitch) => self
                        .editor
                        .move_note(edit.track, edit.note, edit.start, pitch)
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                },
            };
            if let Err(error) = result {
                self.status = Some(error);
            }
        }
        if lesson_next {
            self.lesson.advance();
        }
        if lesson_reset {
            self.lesson.reset();
        }
        if kids_load {
            match self.editor.load_kids_demo() {
                Ok(()) => {
                    self.status = Some(String::from("Kids demo loaded"));
                    self.lesson.reset();
                }
                Err(error) => self.status = Some(error),
            }
        }
        if let Some(chord) = perform_chord {
            let mut song = Song::new("Pads", 120.0);
            let mut track = project::song::Track::new(TrackId::new(1).expect("valid"), "Pad");
            for (index, pitch) in chord.tones(4).iter().enumerate() {
                if let Ok(id) = NoteId::new(index as u32 + 1) {
                    track.notes.push(project::song::Note {
                        id,
                        start: 0,
                        duration: music_core::PPQ * 4,
                        pitch: *pitch,
                        velocity: music_core::Velocity::new(90).expect("valid"),
                        channel: music_core::Channel::first(),
                    });
                }
            }
            song.tracks.push(track);
            let rendered = audio::render(&song, audio::SAMPLE_RATE);
            match audio::player::Player::new(rendered) {
                Ok(player) => {
                    self.perform_player = Some(player);
                    self.status = Some(chord.format(music_core::Spelling::Sharp));
                }
                Err(error) => self.status = Some(error),
            }
        }
        if let Some(status) = score_status {
            self.status = Some(status);
        }
        if let Some((track, delta)) = audio_trim {
            if let Err(error) = self.editor.trim_audio_start(track, delta) {
                self.status = Some(error);
            } else {
                self.status = Some(format!("trimmed {delta} tick(s)"));
            }
        }
        if let Some((track, lane)) = automation_edit {
            let lanes = vec![lane];
            if let Err(error) = self.editor.set_automation(track, lanes) {
                self.status = Some(error);
            }
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

/// The built-in demo used when no project path is given.
#[must_use]
pub fn demo_project() -> ProjectFile {
    ProjectFile::new(arranging::demo::demo_song())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kids_mode_offers_two_views_and_languages_change_labels() {
        assert_eq!(View::allowed(true), vec![View::Perform, View::Arrange]);
        assert_eq!(View::allowed(false).len(), 5);
        let danish = crate::strings::Language::parse("da");
        assert_eq!(crate::strings::text(danish, View::Score.key()), "Partitur");
        assert_eq!(crate::strings::text(danish, View::Perform.key()), "Spil");
    }
}
