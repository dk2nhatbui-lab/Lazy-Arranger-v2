//! The menu bar.
//!
//! One table of commands feeds two menu bars: the native macOS menu bar
//! (see `native_menu`) and the in-window egui menu bar on Windows and Linux.
//! The C++ app did the same with JUCE: a native bar on macOS, a drawn bar
//! elsewhere.
//!
//! Commands that already have a keyboard path (undo, copy, delete and so on)
//! are run by sending that key press to egui on the next frame. A text field
//! with focus then gets it like a real key press, and the app's own key
//! handling gets it otherwise, so both stay in one place.

use crate::instruments::Instrument;
use egui::{Context, Key, KeyboardShortcut, Modifiers};

use super::{FileMode, ShellApp, View};
use crate::settings::{Binding, parse_shortcut};

/// A window the menu can open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Panel {
    Inspector,
    Library,
    Pads,
    Lyrics,
    Markers,
    ScoreMarks,
    AudioTakes,
    MidiMap,
    Controllers,
    Plugins,
    SynthPatch,
    MasterEffects,
    BusEffects,
    XgPartMixer,
    MidiOut,
    MidiMonitor,
    DeviceEditor,
    L6maxMixer,
    L6maxDeviceMixer,
    RolandD05,
    PsrRemote,
    Assistant,
    GeneratePart,
    StyleFromClips,
    StyleFromAudio,
    StyleFromLink,
    StyleFilePlayer,
    ChordsFromAudio,
    GrooveLibrary,
    Theory,
    Lessons,
    Songs,
    Help,
    Shortcuts,
    About,
    Settings,
    InstrumentEditors,
}

/// The windows in the Window menu, in groups split by separators.
pub(super) const WINDOW_GROUPS: &[&[(Panel, &str)]] = &[
    &[
        (Panel::Inspector, "Inspector"),
        (Panel::Library, "Library"),
        (Panel::Pads, "Pads"),
        (Panel::Lyrics, "Lyrics"),
        (Panel::Markers, "Markers"),
        (Panel::ScoreMarks, "Score Marks"),
        (Panel::AudioTakes, "Audio Takes"),
        (Panel::MidiMap, "MIDI Map"),
        (Panel::Controllers, "Controllers"),
    ],
    &[
        (Panel::Plugins, "Plugins"),
        (Panel::SynthPatch, "Synth Patch"),
        (Panel::MasterEffects, "Master Effects"),
        (Panel::BusEffects, "Bus Effects"),
        (Panel::XgPartMixer, "XG Part Mixer"),
        (Panel::MidiOut, "MIDI Out"),
        (Panel::MidiMonitor, "MIDI Monitor"),
        (Panel::DeviceEditor, "Device Editor"),
        (Panel::InstrumentEditors, "Instrument Editors…"),
    ],
    &[
        (Panel::L6maxMixer, "L6max Mixer"),
        (Panel::L6maxDeviceMixer, "L6max Device Mixer"),
        (Panel::RolandD05, "Roland D-05 Editor"),
        (Panel::PsrRemote, "PSR-S970 Remote"),
    ],
    &[
        (Panel::Assistant, "Assistant"),
        (Panel::GeneratePart, "Generate Part"),
        (Panel::StyleFromClips, "Style from Clips"),
        (Panel::StyleFromAudio, "Style from Audio"),
        (Panel::StyleFromLink, "Style from Link"),
        (Panel::StyleFilePlayer, "Style File Player"),
        (Panel::ChordsFromAudio, "Chords from Audio"),
        (Panel::GrooveLibrary, "Groove Library"),
    ],
    &[
        (Panel::Theory, "Theory"),
        (Panel::Lessons, "Lessons"),
        (Panel::Songs, "Songs & Tutorial"),
    ],
];

/// Something a menu item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuCommand {
    New,
    Open,
    OpenRecent(usize),
    ClearRecent,
    Save,
    SaveAs,
    ImportAudio,
    ImportL6max,
    ExportMidi,
    ExportMusicXml,
    ExportWav,
    ExportStems,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
    PlayPause,
    Stop,
    Record,
    Loop,
    Metronome,
    KidsMode,
    ShowView(View),
    Mixer,
    LightTheme,
    Show(Panel),
}

/// One row of a menu.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum MenuEntry {
    Item {
        command: MenuCommand,
        label: String,
        shortcut: Option<Binding>,
        enabled: bool,
        /// `Some` for an item with a tick.
        checked: Option<bool>,
    },
    Submenu {
        label: String,
        entries: Vec<MenuEntry>,
    },
    Separator,
}

/// One menu in the bar.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Menu {
    pub(super) title: &'static str,
    pub(super) entries: Vec<MenuEntry>,
}

fn item(command: MenuCommand, label: impl Into<String>) -> MenuEntry {
    MenuEntry::Item {
        command,
        label: label.into(),
        shortcut: None,
        enabled: true,
        checked: None,
    }
}

impl MenuEntry {
    fn key(mut self, binding: Option<Binding>) -> Self {
        if let Self::Item { shortcut, .. } = &mut self {
            *shortcut = binding;
        }
        self
    }

    fn enabled(mut self, on: bool) -> Self {
        if let Self::Item { enabled, .. } = &mut self {
            *enabled = on;
        }
        self
    }

    fn checked(mut self, on: bool) -> Self {
        if let Self::Item { checked, .. } = &mut self {
            *checked = Some(on);
        }
        self
    }
}

const fn binding(modifiers: Modifiers, key: Key) -> Option<Binding> {
    Some(Binding { modifiers, key })
}

/// A user binding from settings, or the default when it does not parse.
fn user_binding(text: &str, fallback: &str) -> Option<Binding> {
    parse_shortcut(text).or_else(|| parse_shortcut(fallback))
}

const NEW_KEYS: Option<Binding> = binding(Modifiers::COMMAND, Key::N);
pub(super) const SETTINGS_KEYS: Option<Binding> = binding(Modifiers::COMMAND, Key::Comma);
pub(super) const QUIT_KEYS: Option<Binding> = binding(Modifiers::COMMAND, Key::Q);
const KIDS_KEYS: Option<Binding> = Some(Binding {
    modifiers: Modifiers {
        alt: true,
        ctrl: false,
        shift: false,
        mac_cmd: false,
        command: true,
    },
    key: Key::K,
});

/// The view shortcuts, as in the C++ app (Cmd+3 is the score here).
const VIEW_KEYS: [(View, &str, Key); 5] = [
    (View::Arrange, "Timeline", Key::Num1),
    (View::Score, "Score", Key::Num3),
    (View::Drums, "Drums", Key::Num4),
    (View::Loops, "Loops", Key::Num5),
    (View::Map, "MIDI Map", Key::Num6),
];
const MIXER_KEY: Key = Key::Num2;

/// The instrument a panel belongs to, for panels that only make sense with
/// one hardware instrument.
pub(super) const fn instrument_for(panel: Panel) -> Option<Instrument> {
    match panel {
        Panel::XgPartMixer => Some(Instrument::YamahaXg),
        Panel::L6maxMixer | Panel::L6maxDeviceMixer => Some(Instrument::ZoomL6max),
        Panel::RolandD05 => Some(Instrument::RolandD05),
        Panel::PsrRemote => Some(Instrument::YamahaPsrS970),
        _ => None,
    }
}

impl ShellApp {
    /// The whole menu bar for the current state.
    ///
    /// `native` leaves out the items the macOS app menu holds instead
    /// (Settings, About and Quit).
    pub(super) fn menu_model(&self, native: bool) -> Vec<Menu> {
        let settings = &self.settings;
        let kids = settings.kids_mode;

        let mut recent: Vec<MenuEntry> = self
            .recent
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("project");
                item(MenuCommand::OpenRecent(index), name)
            })
            .collect();
        if !recent.is_empty() {
            recent.push(MenuEntry::Separator);
        }
        recent.push(item(MenuCommand::ClearRecent, "Clear List").enabled(!self.recent.is_empty()));

        let mut file = vec![
            item(MenuCommand::New, "New").key(NEW_KEYS),
            item(MenuCommand::Open, "Open…").key(user_binding(&settings.open_keys, "cmd+o")),
            MenuEntry::Submenu {
                label: String::from("Open Recent"),
                entries: recent,
            },
            MenuEntry::Separator,
            item(MenuCommand::Save, "Save").key(user_binding(&settings.save_keys, "cmd+s")),
            item(MenuCommand::SaveAs, "Save As…")
                .key(user_binding(&settings.save_as_keys, "cmd+shift+s")),
            MenuEntry::Separator,
            item(MenuCommand::ImportAudio, "Import Audio File…"),
        ];
        if self.instruments.has(Instrument::ZoomL6max) {
            file.push(item(MenuCommand::ImportL6max, "Import L6max Recording…"));
        }
        file.extend([
            MenuEntry::Separator,
            item(MenuCommand::ExportMidi, "Export MIDI File…"),
            item(MenuCommand::ExportMusicXml, "Export MusicXML File…"),
            item(MenuCommand::ExportWav, "Export Audio File…"),
            item(MenuCommand::ExportStems, "Export Stems…"),
        ]);
        if !native {
            file.extend([
                MenuEntry::Separator,
                item(MenuCommand::Show(Panel::Settings), "Settings…").key(SETTINGS_KEYS),
                MenuEntry::Separator,
                item(MenuCommand::Quit, "Quit").key(QUIT_KEYS),
            ]);
        }

        let edit = vec![
            item(MenuCommand::Undo, "Undo")
                .key(user_binding(&settings.undo_keys, "cmd+z"))
                .enabled(self.editor.undo_depth() > 0),
            item(MenuCommand::Redo, "Redo")
                .key(user_binding(&settings.redo_keys, "cmd+shift+z"))
                .enabled(self.editor.redo_depth() > 0),
            MenuEntry::Separator,
            item(MenuCommand::Cut, "Cut").key(binding(Modifiers::COMMAND, Key::X)),
            item(MenuCommand::Copy, "Copy").key(binding(Modifiers::COMMAND, Key::C)),
            item(MenuCommand::Paste, "Paste").key(binding(Modifiers::COMMAND, Key::V)),
            item(MenuCommand::Delete, "Delete").key(binding(Modifiers::NONE, Key::Delete)),
            MenuEntry::Separator,
            item(MenuCommand::SelectAll, "Select All").key(binding(Modifiers::COMMAND, Key::A)),
        ];

        let transport = vec![
            item(
                MenuCommand::PlayPause,
                if self.playing { "Pause" } else { "Play" },
            )
            .key(user_binding(&settings.play_keys, "cmd+p")),
            item(MenuCommand::Stop, "Stop"),
            item(MenuCommand::Record, "Record").checked(self.recording.is_some()),
            MenuEntry::Separator,
            item(MenuCommand::Loop, "Loop").checked(self.loop_enabled),
            item(MenuCommand::Metronome, "Metronome").checked(settings.metronome),
            MenuEntry::Separator,
            item(MenuCommand::KidsMode, "Kids Mode")
                .key(KIDS_KEYS)
                .checked(kids),
        ];

        let allowed = View::allowed(kids);
        let mut view = Vec::new();
        if kids {
            view.push(
                item(MenuCommand::ShowView(View::Perform), "Perform")
                    .checked(self.view == View::Perform),
            );
        }
        for (index, (target, label, key)) in VIEW_KEYS.iter().enumerate() {
            view.push(
                item(MenuCommand::ShowView(*target), *label)
                    .key(binding(Modifiers::COMMAND, *key))
                    .enabled(allowed.contains(target))
                    .checked(self.view == *target),
            );
            if index == 0 {
                view.push(
                    item(MenuCommand::Mixer, "Mixer")
                        .key(binding(Modifiers::COMMAND, MIXER_KEY))
                        .enabled(!kids)
                        .checked(self.mixer_visible && !kids),
                );
            }
        }
        view.extend([
            MenuEntry::Separator,
            item(MenuCommand::LightTheme, "Light Theme").checked(settings.light_theme),
        ]);

        let mut window = Vec::new();
        for group in WINDOW_GROUPS {
            let shown: Vec<MenuEntry> = group
                .iter()
                .filter(|(panel, _)| {
                    instrument_for(*panel).is_none_or(|instrument| self.instruments.has(instrument))
                })
                .map(|(panel, label)| item(MenuCommand::Show(*panel), *label))
                .collect();
            if shown.is_empty() {
                continue;
            }
            if !window.is_empty() {
                window.push(MenuEntry::Separator);
            }
            window.extend(shown);
        }

        let mut help = vec![
            item(MenuCommand::Show(Panel::Help), "Lazy Arranger Help"),
            item(MenuCommand::Show(Panel::Shortcuts), "Keyboard Shortcuts")
                .key(binding(Modifiers::NONE, Key::F1)),
            item(MenuCommand::Show(Panel::Songs), "Songs & Tutorial"),
        ];
        if !native {
            help.extend([
                MenuEntry::Separator,
                item(
                    MenuCommand::Show(Panel::About),
                    "About Lazy Arranger Midi DAW",
                ),
            ]);
        }

        vec![
            Menu {
                title: "File",
                entries: file,
            },
            Menu {
                title: "Edit",
                entries: edit,
            },
            Menu {
                title: "Transport",
                entries: transport,
            },
            Menu {
                title: "View",
                entries: view,
            },
            Menu {
                title: "Window",
                entries: window,
            },
            Menu {
                title: "Help",
                entries: help,
            },
        ]
    }

    /// The in-window menu bar (Windows and Linux).
    pub(super) fn menu_bar(&mut self, context: &Context) {
        if self.native_menu_active() {
            return;
        }
        let menus = self.menu_model(false);
        let mut chosen = None;
        egui::TopBottomPanel::top("menu-bar").show(context, |ui| {
            egui::menu::bar(ui, |ui| {
                for menu in &menus {
                    ui.menu_button(menu.title, |ui| {
                        if let Some(command) = draw_entries(ui, &menu.entries) {
                            chosen = Some(command);
                        }
                    });
                }
            });
        });
        if let Some(command) = chosen {
            self.run_menu_command(context, command);
        }
    }

    /// Shortcuts that only the menu has: the other shortcuts already have
    /// key handling of their own. Runs before that key handling, so Cmd+N
    /// does not also add a clip.
    ///
    /// With the native macOS menu, the menu takes these keys first and this
    /// finds nothing.
    pub(super) fn handle_menu_keys(&mut self, context: &Context) {
        let mut pressed = Vec::new();
        for (keys, command) in [
            (NEW_KEYS, MenuCommand::New),
            (SETTINGS_KEYS, MenuCommand::Show(Panel::Settings)),
            (QUIT_KEYS, MenuCommand::Quit),
            (binding(Modifiers::COMMAND, MIXER_KEY), MenuCommand::Mixer),
        ]
        .into_iter()
        .chain(VIEW_KEYS.iter().map(|(view, _, key)| {
            (
                binding(Modifiers::COMMAND, *key),
                MenuCommand::ShowView(*view),
            )
        })) {
            if let Some(keys) = keys
                && context.input_mut(|input| input.consume_key(keys.modifiers, keys.key))
            {
                pressed.push(command);
            }
        }
        for command in pressed {
            self.run_menu_command(context, command);
        }
    }

    /// Does what a menu item says.
    pub(super) fn run_menu_command(&mut self, context: &Context, command: MenuCommand) {
        match command {
            MenuCommand::New => self.replace_project(
                project::song::ProjectFile::new(project::song::Song::new("Untitled", 120.0)),
                "new project",
            ),
            MenuCommand::Open => self.choose_file(FileMode::Open),
            MenuCommand::OpenRecent(index) => {
                if let Some(path) = self.recent.get(index).cloned() {
                    self.open(&path);
                }
            }
            MenuCommand::ClearRecent => match project::recent::clear(&self.recent_file) {
                Ok(()) => {
                    self.recent.clear();
                    self.status = Some(String::from("recent list cleared"));
                }
                Err(error) => self.status = Some(error),
            },
            MenuCommand::Save => match self.project_path.clone() {
                Some(path) => self.save_to(path),
                None => self.choose_file(FileMode::SaveAs),
            },
            MenuCommand::SaveAs => self.choose_file(FileMode::SaveAs),
            MenuCommand::ImportAudio => self.choose_file(FileMode::ImportAudio),
            MenuCommand::ImportL6max => self.choose_file(FileMode::ImportL6max),
            MenuCommand::ExportMidi => self.choose_file(FileMode::ExportMidi),
            MenuCommand::ExportMusicXml => self.choose_file(FileMode::ExportMusicXml),
            MenuCommand::ExportWav => self.choose_file(FileMode::ExportWav),
            MenuCommand::ExportStems => self.choose_file(FileMode::ExportStems),
            MenuCommand::Quit => context.send_viewport_cmd(egui::ViewportCommand::Close),
            MenuCommand::Undo => {
                self.press(user_binding(&self.settings.undo_keys, "cmd+z"), None);
            }
            MenuCommand::Redo => {
                self.press(user_binding(&self.settings.redo_keys, "cmd+shift+z"), None);
            }
            MenuCommand::Cut => {
                self.press(binding(Modifiers::COMMAND, Key::X), Some(egui::Event::Cut))
            }
            MenuCommand::Copy => {
                self.press(binding(Modifiers::COMMAND, Key::C), Some(egui::Event::Copy))
            }
            MenuCommand::Paste => {
                let text = clipboard_text();
                self.press(
                    binding(Modifiers::COMMAND, Key::V),
                    (!text.is_empty()).then_some(egui::Event::Paste(text)),
                );
            }
            MenuCommand::Delete => self.press(binding(Modifiers::NONE, Key::Delete), None),
            MenuCommand::SelectAll => self.press(binding(Modifiers::COMMAND, Key::A), None),
            MenuCommand::PlayPause => self.toggle_playback(),
            MenuCommand::Stop => self.stop_playback(),
            MenuCommand::Record => self.start_midi_recording(),
            MenuCommand::Loop => self.toggle_loop(),
            MenuCommand::Metronome => self.settings.metronome = !self.settings.metronome,
            MenuCommand::KidsMode => self.toggle_kids_mode(),
            MenuCommand::ShowView(view) => {
                if View::allowed(self.settings.kids_mode).contains(&view)
                    || (view == View::Perform && self.settings.kids_mode)
                {
                    self.view = view;
                }
            }
            MenuCommand::Mixer => {
                if !self.settings.kids_mode {
                    self.mixer_visible = !self.mixer_visible;
                }
            }
            MenuCommand::LightTheme => {
                let light = !self.settings.light_theme;
                self.settings.light_theme = light;
                crate::theme::set_light(light);
                crate::theme::refresh(context);
                let _ = self.settings.save(&crate::settings::Settings::path());
            }
            MenuCommand::Show(panel) => self.open_panel(panel),
        }
        context.request_repaint();
    }

    /// Opens one of the app's windows.
    pub(super) fn open_panel(&mut self, panel: Panel) {
        match panel {
            Panel::Inspector => self.inspector_open = true,
            Panel::Library => self.library_open = true,
            Panel::Pads => self.pads_open = true,
            Panel::Lyrics => self.lyrics_open = true,
            Panel::Markers => self.markers_open = true,
            Panel::ScoreMarks => self.marks_open = true,
            Panel::AudioTakes => self.takes_open = true,
            Panel::MidiMap => self.midi_map_open = true,
            Panel::Controllers => self.controller_open = true,
            Panel::Plugins => self.plugins_open = true,
            Panel::SynthPatch => {
                self.patch_open = true;
                self.patch_draft = None;
            }
            Panel::MasterEffects => self.master_fx_open = true,
            Panel::BusEffects => self.bus_fx_open = true,
            Panel::XgPartMixer => self.xg.open = true,
            Panel::MidiOut => self.midi_out.open = true,
            Panel::MidiMonitor => self.midi_monitor_open = true,
            Panel::DeviceEditor => self.device_editor_open = true,
            Panel::L6maxMixer => self.l6max.mixer_open = true,
            Panel::L6maxDeviceMixer => self.l6max.device_open = true,
            Panel::RolandD05 => self.d05.open = true,
            Panel::PsrRemote => self.psr.open = true,
            Panel::InstrumentEditors => self.instruments_open = true,
            Panel::Assistant => self.assistant.open = true,
            Panel::GeneratePart => self.generate_open = true,
            Panel::StyleFromClips => self.style.clips_open = true,
            Panel::StyleFromAudio => self.audio_style.open = true,
            Panel::StyleFromLink => self.style.link_open = true,
            Panel::StyleFilePlayer => self.style.file_open = true,
            Panel::ChordsFromAudio => self.audio_chords.open = true,
            Panel::GrooveLibrary => self.groove_open = true,
            Panel::Theory => self.theory_open = true,
            Panel::Lessons => self.lessons_open = true,
            Panel::Songs => self.songs_open = true,
            Panel::Help => self.help_open = true,
            Panel::Shortcuts => self.shortcuts_open = true,
            Panel::About => self.about_open = true,
            Panel::Settings => self.settings_open = true,
        }
    }

    /// Turns Kids Mode on or off and moves to a view it allows.
    pub(super) fn toggle_kids_mode(&mut self) {
        self.settings.kids_mode = !self.settings.kids_mode;
        self.view = if self.settings.kids_mode {
            View::Perform
        } else {
            View::Arrange
        };
    }

    /// Queues a key press (and an optional text event) for the next frame.
    fn press(&mut self, keys: Option<Binding>, extra: Option<egui::Event>) {
        if let Some(keys) = keys {
            self.menu.pending.push(key_event(keys));
        }
        if let Some(extra) = extra {
            self.menu.pending.push(extra);
        }
    }

    /// Adds the queued key presses to this frame's input.
    pub(super) fn feed_menu_input(&mut self, raw: &mut egui::RawInput) {
        raw.events.append(&mut self.menu.pending);
    }

    /// True when the native macOS menu bar is showing.
    pub(super) fn native_menu_active(&self) -> bool {
        self.menu.native_active()
    }

    /// Builds the native menu on the first frame and keeps it up to date;
    /// runs the commands chosen in it.
    pub(super) fn sync_native_menu(&mut self, context: &Context) {
        if !super::native_menu::SUPPORTED {
            return;
        }
        // Run what was chosen before the menu can be rebuilt, so the ids
        // still match.
        for command in self.menu.native_commands() {
            self.run_menu_command(context, command);
        }
        let model = self.menu_model(true);
        self.menu.sync_native(context, &model);
    }
}

/// Draws one menu's rows; returns the chosen command.
fn draw_entries(ui: &mut egui::Ui, entries: &[MenuEntry]) -> Option<MenuCommand> {
    // A menu with ticks keeps room for the tick on every row, so the
    // labels line up.
    let ticks = entries.iter().any(|entry| {
        matches!(
            entry,
            MenuEntry::Item {
                checked: Some(_),
                ..
            }
        )
    });
    let mut chosen = None;
    for entry in entries {
        match entry {
            MenuEntry::Separator => {
                ui.separator();
            }
            MenuEntry::Submenu { label, entries } => {
                ui.menu_button(label.as_str(), |ui| {
                    if let Some(command) = draw_entries(ui, entries) {
                        chosen = Some(command);
                    }
                });
            }
            MenuEntry::Item {
                command,
                label,
                shortcut,
                enabled,
                checked,
            } => {
                let text = if ticks {
                    let mut job = egui::text::LayoutJob::default();
                    let font = egui::TextStyle::Button.resolve(ui.style());
                    let colour = if *checked == Some(true) {
                        ui.visuals().text_color()
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    job.append("✔ ", 0.0, egui::TextFormat::simple(font.clone(), colour));
                    job.append(
                        label,
                        0.0,
                        egui::TextFormat::simple(font, ui.visuals().text_color()),
                    );
                    egui::WidgetText::LayoutJob(job)
                } else {
                    egui::WidgetText::from(label.as_str())
                };
                let mut button = egui::Button::new(text);
                if let Some(keys) = shortcut {
                    button = button.shortcut_text(shortcut_label(ui.ctx(), *keys));
                }
                if ui.add_enabled(*enabled, button).clicked() {
                    chosen = Some(*command);
                    ui.close_menu();
                }
            }
        }
    }
    chosen
}

/// How a shortcut reads in the menu, such as "Ctrl+Shift+S".
fn shortcut_label(context: &Context, keys: Binding) -> String {
    context
        .format_shortcut(&KeyboardShortcut::new(keys.modifiers, keys.key))
        .replace("Comma", ",")
}

/// The key event a real press of `keys` would make.
fn key_event(keys: Binding) -> egui::Event {
    let mut modifiers = keys.modifiers;
    if modifiers.command {
        if cfg!(target_os = "macos") {
            modifiers.mac_cmd = true;
        } else {
            modifiers.ctrl = true;
        }
    }
    egui::Event::Key {
        key: keys.key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// The system clipboard's text, for Paste from the menu.
fn clipboard_text() -> String {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.get_text())
        .unwrap_or_default()
}

/// Menu state kept between frames.
#[derive(Default)]
pub(super) struct MenuState {
    /// Key presses to add to the next frame's input.
    pending: Vec<egui::Event>,
    /// The native macOS menu bar, once built.
    native: Option<super::native_menu::NativeMenu>,
}

impl std::fmt::Debug for MenuState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MenuState")
            .field("pending", &self.pending.len())
            .field("native", &self.native.is_some())
            .finish()
    }
}

impl MenuState {
    fn native_active(&self) -> bool {
        self.native.is_some()
    }

    fn sync_native(&mut self, context: &Context, model: &[Menu]) {
        match &mut self.native {
            Some(native) => native.update(model),
            None => self.native = super::native_menu::NativeMenu::build(context, model),
        }
    }

    fn native_commands(&mut self) -> Vec<MenuCommand> {
        self.native
            .as_mut()
            .map(super::native_menu::NativeMenu::commands)
            .unwrap_or_default()
    }
}
