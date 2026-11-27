//! The Settings window: one tab per subject, each a two-column form.

use super::devices::load_device_profiles;
use super::{ShellApp, View};
use crate::dialog;
use crate::theme;

/// The Settings tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SettingsTab {
    /// Language, theme, Kids Mode.
    #[default]
    General,
    /// Output, input, buffer, metronome.
    Audio,
    /// MIDI input, device profile, recording.
    Midi,
    /// AI endpoint, local assistant, speech.
    Assistant,
    /// Release checks.
    Updates,
}

impl SettingsTab {
    const ALL: [Self; 5] = [
        Self::General,
        Self::Audio,
        Self::Midi,
        Self::Assistant,
        Self::Updates,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Audio => "Audio",
            Self::Midi => "MIDI & recording",
            Self::Assistant => "Assistant & voice",
            Self::Updates => "Updates",
        }
    }
}

/// Language tags with the names the menu shows.
const LANGUAGES: [(&str, &str); 9] = [
    ("system", "System default"),
    ("en", "English"),
    ("da", "Danish"),
    ("fr", "French"),
    ("vi", "Vietnamese"),
    ("de", "German"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
];

/// Width of the label column.
const LABEL_WIDTH: f32 = 150.0;
/// Width of combo boxes and text fields.
const FIELD_WIDTH: f32 = 300.0;

/// One form row: a label on the left, the controls on the right.
fn row<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_WIDTH, ui.spacing().interact_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(LABEL_WIDTH);
                ui.label(egui::RichText::new(label).color(theme::text_muted()));
            },
        );
        add(ui)
    })
    .inner
}

/// A hint under a row, lined up with the controls.
fn hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.add_space(LABEL_WIDTH + ui.spacing().item_spacing.x);
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .size(12.5)
                    .color(theme::text_muted()),
            )
            .wrap(),
        );
    });
}

/// A checkbox lined up with the controls.
fn check(ui: &mut egui::Ui, value: &mut bool, text: &str) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_space(LABEL_WIDTH + ui.spacing().item_spacing.x);
        ui.checkbox(value, text)
    })
    .inner
}

fn save(settings: &crate::settings::Settings) {
    let _ = settings.save(&crate::settings::Settings::path());
}

impl ShellApp {
    /// Draws the Settings window when it is open.
    pub(super) fn settings_window(&mut self, context: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        let mut close = false;
        dialog::window("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(620.0)
            .show(context, |ui| {
                ui.set_width(620.0);
                ui.horizontal(|ui| {
                    for tab in SettingsTab::ALL {
                        let selected = self.settings_tab == tab;
                        let text = egui::RichText::new(tab.label()).color(if selected {
                            theme::accent_text()
                        } else {
                            theme::text()
                        });
                        let button = egui::Button::new(text)
                            .fill(if selected {
                                theme::accent()
                            } else {
                                theme::line()
                            })
                            .stroke(egui::Stroke::new(1.0_f32, theme::line_strong()));
                        if ui.add(button).clicked() {
                            self.settings_tab = tab;
                        }
                    }
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .min_scrolled_height(400.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.settings_tab {
                        SettingsTab::General => self.settings_general(ui),
                        SettingsTab::Audio => self.settings_audio(ui),
                        SettingsTab::Midi => self.settings_midi(ui),
                        SettingsTab::Assistant => self.settings_assistant(ui),
                        SettingsTab::Updates => self.settings_updates(ui),
                    });
                ui.separator();
                dialog::buttons(ui, |ui| {
                    if dialog::primary(ui, "Save").clicked() {
                        match self.settings.save(&crate::settings::Settings::path()) {
                            Ok(()) => {
                                self.status = Some(String::from("settings saved"));
                                close = true;
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if dialog::secondary(ui, "Close").clicked() {
                        close = true;
                    }
                    dialog::note(ui, "Changes apply now. Save keeps them for next time.");
                });
            });
        if !open || close {
            self.settings_open = false;
        }
    }

    fn settings_general(&mut self, ui: &mut egui::Ui) {
        dialog::heading(ui, "Look");
        row(ui, "Language", |ui| {
            let current = LANGUAGES
                .iter()
                .find(|(tag, _)| *tag == self.settings.language)
                .map_or(self.settings.language.as_str(), |(_, name)| name)
                .to_string();
            egui::ComboBox::from_id_salt("settings-language")
                .width(FIELD_WIDTH)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (tag, name) in LANGUAGES {
                        ui.selectable_value(&mut self.settings.language, tag.to_string(), name);
                    }
                });
        });
        row(ui, "Theme", |ui| {
            let mut light = self.settings.light_theme;
            ui.selectable_value(&mut light, false, "Dark");
            ui.selectable_value(&mut light, true, "Light");
            if light != self.settings.light_theme {
                self.settings.light_theme = light;
                theme::set_light(light);
                theme::refresh(ui.ctx());
            }
        });
        check(ui, &mut self.settings.reduced_motion, "Reduced motion");
        dialog::heading(ui, "Projects");
        check(
            ui,
            &mut self.settings.confirm_unsaved,
            "Ask to save unsaved changes before closing a project",
        );
        dialog::heading(ui, "Kids Mode");
        if check(
            ui,
            &mut self.settings.kids_mode,
            "Kids Mode: large text and a simpler header",
        )
        .changed()
        {
            self.view = if self.settings.kids_mode {
                View::Perform
            } else {
                View::Arrange
            };
        }
        if self.settings.kids_mode {
            row(ui, "Pause between plays", |ui| {
                ui.add(
                    egui::Slider::new(&mut self.settings.kids_pause_seconds, 0.5..=5.0)
                        .suffix(" s"),
                );
            });
            check(
                ui,
                &mut self.settings.kids_keys_play,
                "Keys also play their own notes",
            );
        }
    }

    fn settings_audio(&mut self, ui: &mut egui::Ui) {
        if self.audio_outputs.is_none() {
            self.audio_outputs = Some(audio::player::output_devices());
        }
        if self.audio_inputs.is_none() {
            self.audio_inputs = Some(audio::record::input_devices());
        }
        dialog::heading(ui, "Output");
        row(ui, "Output device", |ui| {
            let outputs = self.audio_outputs.clone().unwrap_or_default();
            let mut chosen = self.settings.audio_output.clone();
            egui::ComboBox::from_id_salt("audio-output")
                .width(FIELD_WIDTH)
                .selected_text(chosen.as_deref().unwrap_or("System default"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, "System default");
                    for name in outputs {
                        let label = name.clone();
                        ui.selectable_value(&mut chosen, Some(name), label);
                    }
                });
            if ui.button("Refresh").clicked() {
                let outputs = audio::player::output_devices();
                self.status = Some(format!("{} audio output(s)", outputs.len()));
                self.audio_outputs = Some(outputs);
            }
            if chosen != self.settings.audio_output {
                if let Some(name) = &chosen {
                    self.status = Some(format!("audio output: {name}"));
                }
                self.settings.audio_output = chosen;
                save(&self.settings);
            }
        });
        row(ui, "Buffer size", |ui| {
            let mut frames = self.settings.audio_buffer_frames;
            let label = |frames: u32| {
                if frames == 0 {
                    String::from("Device default")
                } else {
                    format!("{frames} samples")
                }
            };
            egui::ComboBox::from_id_salt("audio-buffer")
                .width(FIELD_WIDTH)
                .selected_text(label(frames))
                .show_ui(ui, |ui| {
                    for option in [0_u32, 128, 256, 512, 1024] {
                        ui.selectable_value(&mut frames, option, label(option));
                    }
                });
            if frames != self.settings.audio_buffer_frames {
                self.settings.audio_buffer_frames = frames;
                save(&self.settings);
            }
        });
        check(ui, &mut self.settings.metronome, "Metronome with playback");
        dialog::heading(ui, "Input");
        row(ui, "Input device", |ui| {
            let inputs = self.audio_inputs.clone().unwrap_or_default();
            let mut chosen = self.settings.audio_input.clone();
            egui::ComboBox::from_id_salt("audio-input")
                .width(FIELD_WIDTH)
                .selected_text(chosen.as_deref().unwrap_or("System default"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, "System default");
                    for name in inputs {
                        let label = name.clone();
                        ui.selectable_value(&mut chosen, Some(name), label);
                    }
                });
            if ui.button("Refresh").clicked() {
                let inputs = audio::record::input_devices();
                self.status = Some(format!("{} audio input(s)", inputs.len()));
                self.audio_inputs = Some(inputs);
            }
            if chosen != self.settings.audio_input {
                if let Some(name) = &chosen {
                    self.status = Some(format!("audio input: {name}"));
                }
                self.settings.audio_input = chosen;
                save(&self.settings);
            }
        });
        row(ui, "Input channel", |ui| {
            let label = |channel: u8| {
                if channel == 0 {
                    String::from("All channels (mix)")
                } else {
                    format!("Channel {channel}")
                }
            };
            egui::ComboBox::from_id_salt("audio-input-channel")
                .width(FIELD_WIDTH)
                .selected_text(label(self.settings.audio_input_channel))
                .show_ui(ui, |ui| {
                    for channel in 0..=16 {
                        ui.selectable_value(
                            &mut self.settings.audio_input_channel,
                            channel,
                            label(channel),
                        );
                    }
                });
        });
        check(
            ui,
            &mut self.settings.monitor,
            "Hear the input through the output",
        );
        hint(ui, "Use headphones: with speakers this can feed back.");
    }

    fn settings_midi(&mut self, ui: &mut egui::Ui) {
        if self.midi_ports.is_none()
            && let Ok(names) = midi_devices::devices::list_ports()
        {
            self.midi_ports = Some((names.inputs, names.outputs));
        }
        dialog::heading(ui, "Keyboard");
        row(ui, "MIDI input", |ui| {
            let inputs = self
                .midi_ports
                .as_ref()
                .map(|(inputs, _)| inputs.clone())
                .unwrap_or_default();
            let mut chosen = self.settings.keyboard.clone();
            egui::ComboBox::from_id_salt("settings-midi-input")
                .width(FIELD_WIDTH)
                .selected_text(chosen.as_deref().unwrap_or("None"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut chosen, None, "None");
                    for name in inputs {
                        let label = name.clone();
                        ui.selectable_value(&mut chosen, Some(name), label);
                    }
                });
            if ui.button("Refresh").clicked() {
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
            if chosen != self.settings.keyboard {
                self.status = Some(chosen.as_ref().map_or_else(
                    || String::from("MIDI input off"),
                    |name| format!("MIDI input: {name}"),
                ));
                self.settings.keyboard = chosen;
                self.midi_input = None;
                save(&self.settings);
            }
        });
        row(ui, "Device profile", |ui| {
            let selected = self
                .settings
                .device_profile
                .as_ref()
                .and_then(|id| {
                    self.device_profiles
                        .iter()
                        .find(|profile| &profile.id == id)
                })
                .map(|profile| profile.name.clone())
                .unwrap_or_else(|| String::from("Built-in PSR-S970"));
            egui::ComboBox::from_id_salt("device-profile")
                .width(FIELD_WIDTH)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            self.settings.device_profile.is_none(),
                            "Built-in PSR-S970",
                        )
                        .clicked()
                    {
                        self.settings.device_profile = None;
                    }
                    for profile in &self.device_profiles {
                        if ui
                            .selectable_label(
                                self.settings.device_profile.as_ref() == Some(&profile.id),
                                &profile.name,
                            )
                            .clicked()
                        {
                            self.settings.device_profile = Some(profile.id.clone());
                        }
                    }
                });
            if ui.button("Reload").clicked() {
                self.device_profiles = load_device_profiles();
            }
        });
        row(ui, "", |ui| {
            if ui.button("Show profiles folder").clicked() {
                self.show_profiles_folder();
            }
            if ui.button("Instrument editors\u{2026}").clicked() {
                self.instruments_open = true;
            }
        });
        row(ui, "Chord grade", |ui| {
            ui.add(egui::Slider::new(&mut self.settings.grade, 1..=9));
        });
        dialog::heading(ui, "Recording");
        check(
            ui,
            &mut self.settings.record_quantise,
            "Quantise recorded notes to 1/16",
        );
        check(
            ui,
            &mut self.settings.record_chords,
            "Detect chords after MIDI recording",
        );
    }

    fn show_profiles_folder(&mut self) {
        let folder = crate::settings::Settings::path()
            .parent()
            .map(|path| path.join("profiles"))
            .unwrap_or_else(|| std::path::PathBuf::from("profiles"));
        if let Err(error) = std::fs::create_dir_all(&folder) {
            self.status = Some(format!("{}: {error}", folder.display()));
        } else {
            self.status = Some(format!("device profiles: {}", folder.display()));
            crate::update::open_path(&folder);
        }
    }

    fn settings_assistant(&mut self, ui: &mut egui::Ui) {
        let field = |ui: &mut egui::Ui, value: &mut String, hint_text: &str, password: bool| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .hint_text(egui::RichText::new(hint_text).color(theme::text_dim()))
                    .password(password)
                    .desired_width(FIELD_WIDTH + 80.0),
            );
        };
        dialog::heading(ui, "AI endpoint (OpenAI-compatible)");
        row(ui, "URL", |ui| {
            field(
                ui,
                &mut self.settings.ai_url,
                "http://localhost:11434/v1/chat/completions",
                false,
            );
        });
        row(ui, "Model", |ui| {
            field(ui, &mut self.settings.ai_model, "", false);
        });
        row(ui, "API key", |ui| {
            field(ui, &mut self.settings.ai_api_key, "", true);
        });
        hint(
            ui,
            "Leave the URL empty to stay local. The key is kept in plain text in the settings file.",
        );
        dialog::heading(ui, "Local assistant");
        row(ui, "Command", |ui| {
            field(
                ui,
                &mut self.settings.assistant_command,
                "codex exec {prompt}",
                false,
            );
        });
        hint(
            ui,
            if cfg!(windows) {
                "{prompt} is replaced by your question. Runs the program directly (no shell); {prompt} is one argument."
            } else {
                "{prompt} is replaced by your question."
            },
        );
        dialog::heading(ui, "Voice");
        row(ui, "Speak with", |ui| {
            for preset in [
                crate::speech::TtsPreset::Auto,
                crate::speech::TtsPreset::System,
                crate::speech::TtsPreset::Kokoro,
                crate::speech::TtsPreset::Kitten,
                crate::speech::TtsPreset::Piper,
            ] {
                let on = self.settings.speak_command == preset.setting_value();
                if ui.selectable_label(on, preset.label()).clicked() {
                    self.settings.speak_command = String::from(preset.setting_value());
                }
            }
        });
        row(ui, "", |ui| {
            field(
                ui,
                &mut self.settings.speak_command,
                "auto | kokoro | kitten | piper | custom {text}",
                false,
            );
        });
        let dir = crate::dictate::models_dir();
        row(ui, "Dictate with", |ui| {
            for preset in [
                crate::dictate::SttPreset::Auto,
                crate::dictate::SttPreset::WhisperCpp,
                crate::dictate::SttPreset::SherpaMoonshine,
            ] {
                let template = preset.template(&dir);
                let on = self.settings.speech_command == template;
                if ui.selectable_label(on, preset.label()).clicked() {
                    self.settings.speech_command = template;
                }
            }
        });
        row(ui, "", |ui| {
            field(
                ui,
                &mut self.settings.speech_command,
                "auto | whisper-cli -m model.bin -f {file} -nt",
                false,
            );
        });
        hint(ui, format!("Offline models folder: {}", dir.display()));
    }

    fn settings_updates(&mut self, ui: &mut egui::Ui) {
        let current = env!("CARGO_PKG_VERSION");
        dialog::heading(ui, "Version");
        row(ui, "This version", |ui| {
            ui.label(current);
            if ui.button("Check for updates").clicked() {
                self.check_for_updates();
            }
        });
        if let Some(release) = self.available_update.clone() {
            row(ui, "Newer release", |ui| {
                ui.label(egui::RichText::new(&release.tag).strong());
            });
            row(ui, "", |ui| {
                if ui.button("Open release page").clicked() {
                    crate::update::open_url(&release.url);
                }
                if let Some(asset) = crate::update::preferred_asset(&release)
                    && ui.button(format!("Download {}", asset.name)).clicked()
                {
                    let asset = asset.clone();
                    let directory = std::env::var_os("HOME")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(std::env::temp_dir)
                        .join("Downloads");
                    let (sender, receiver) = std::sync::mpsc::channel();
                    self.update_download = Some(receiver);
                    self.downloaded_update = None;
                    self.status = Some(format!("downloading {}", asset.name));
                    std::thread::spawn(move || {
                        let _ = sender.send(crate::update::download_asset(&asset, &directory));
                    });
                }
            });
            row(ui, "", |ui| {
                if let Some(path) = &self.downloaded_update
                    && ui.button("Open download").clicked()
                {
                    crate::update::open_path(path);
                }
                if ui.button("Skip this version").clicked() {
                    self.settings.skipped_update = release.tag.clone();
                    save(&self.settings);
                    self.available_update = None;
                    self.status = Some(format!("Update {} skipped", release.tag));
                }
            });
        } else if !self.settings.skipped_update.is_empty() {
            hint(
                ui,
                format!("Skipped release: {}", self.settings.skipped_update),
            );
        }
    }

    fn check_for_updates(&mut self) {
        match crate::update::fetch_release() {
            Ok(release) => {
                let current = env!("CARGO_PKG_VERSION");
                if crate::update::should_offer(&release.tag, current, &self.settings.skipped_update)
                {
                    self.status = Some(crate::update::describe_release(
                        current,
                        &release.tag,
                        &release.url,
                    ));
                    self.available_update = Some(release);
                } else if release.tag == self.settings.skipped_update {
                    self.status = Some(format!("Update {} skipped in Settings", release.tag));
                } else {
                    self.status = Some(crate::update::describe_release(
                        current,
                        &release.tag,
                        &release.url,
                    ));
                }
            }
            Err(error) => {
                self.status = Some(format!("Update check failed: {error}"));
            }
        }
    }
}
