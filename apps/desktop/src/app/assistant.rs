//! The assistant panel and its AI requests.

use super::*;

impl ShellApp {
    pub(super) fn assistant_history(&self) -> Vec<(String, String)> {
        self.assistant_log
            .iter()
            .filter_map(|line| {
                line.strip_prefix("> ").map_or_else(
                    || (!line.is_empty()).then(|| (String::from("assistant"), line.clone())),
                    |user| Some((String::from("user"), user.to_string())),
                )
            })
            .collect()
    }

    pub(super) fn request_assistant(&mut self, prompt: String) {
        let config = crate::ai::Config {
            url: self.settings.ai_url.clone(),
            model: self.settings.ai_model.clone(),
            api_key: self.settings.ai_api_key.clone(),
        };
        let command = self.settings.assistant_command.clone();
        if !config.is_ready() && command.trim().is_empty() {
            return;
        }
        let history = self.assistant_history();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.ai_pending = Some(receiver);
        std::thread::spawn(move || {
            let result = if config.is_ready() {
                crate::ai::chat_with_history(&config, &history, &prompt)
            } else {
                crate::ai::local_chat(&command, &prompt)
            };
            let _ = sender.send(result);
        });
    }

    pub(super) fn poll_ai(&mut self) {
        let Some(receiver) = &self.ai_pending else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(reply)) => {
                self.ai_pending = None;
                self.finish_ai_reply(reply);
            }
            Ok(Err(error)) => {
                self.ai_pending = None;
                self.assistant_log.push(format!("AI: {error}"));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.ai_pending = None;
            }
        }
    }

    pub(super) fn finish_ai_reply(&mut self, reply: String) {
        let mut text = reply.clone();
        let commands = crate::ai::tool_commands(&reply);
        let mut follow_up = false;
        if let Some(command) = commands
            .iter()
            .find(|command| crate::ai::changes_project(command))
        {
            let label = crate::ai::tool_name(command).unwrap_or("tool").to_string();
            self.assistant_proposal = Some(AssistantProposal {
                command: (*command).to_string(),
                label: label.clone(),
            });
            text = format!("Proposed: {label}. Apply or cancel below.");
        } else {
            for command in &commands {
                self.selected_note = None;
                self.selected_clip = None;
                self.selected_clips.clear();
                let result = self.editor.assistant(command);
                text.push('\n');
                text.push_str(&result.text);
            }
            let ready = crate::ai::Config {
                url: self.settings.ai_url.clone(),
                model: self.settings.ai_model.clone(),
                api_key: self.settings.ai_api_key.clone(),
            }
            .is_ready();
            follow_up = crate::ai::can_follow_up(ready, commands.len(), self.assistant_tool_rounds);
        }
        if self.settings.speech
            && let Err(error) = crate::speech::speak_with(&text, &self.settings.speak_command)
        {
            self.status = Some(error);
        }
        self.assistant_log.push(text.clone());
        if follow_up {
            self.assistant_tool_rounds += 1;
            self.request_assistant(format!(
                "Tool results:\n{}\nContinue if you need another tool, otherwise answer.",
                crate::ai::trim_tool_result(&text)
            ));
        }
    }

    /// The local assistant panel.
    pub(super) fn assistant_window(&mut self, context: &egui::Context) {
        if !self.assistant_open {
            return;
        }
        let mut open = self.assistant_open;
        let mut apply_proposal = false;
        let mut cancel_proposal = false;
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
                                .color(theme::text_dim()),
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
                    if ui
                        .button("Dictate")
                        .on_hover_text(
                            "Record 5 s (16 kHz mono WAV) and run local Whisper / Moonshine STT",
                        )
                        .clicked()
                    {
                        let command = self.settings.speech_command.clone();
                        let (sender, receiver) = std::sync::mpsc::channel();
                        self.dictation_pending = Some(receiver);
                        self.status = Some(String::from("listening for 5 s…"));
                        std::thread::spawn(move || {
                            let result = crate::dictate::record_to_wav(5.0).and_then(|path| {
                                let text = crate::dictate::transcribe(&path, &command);
                                let _ = std::fs::remove_file(path);
                                text
                            });
                            let _ = sender.send(result);
                        });
                    }
                    if (ui.button("Send").clicked() || enter) && !self.assistant_input.is_empty() {
                        let input = self.assistant_input.trim().to_string();
                        self.assistant_log.push(format!("> {input}"));
                        self.assistant_tool_rounds = 0;
                        self.assistant_input.clear();
                        let config = crate::ai::Config {
                            url: self.settings.ai_url.clone(),
                            model: self.settings.ai_model.clone(),
                            api_key: self.settings.ai_api_key.clone(),
                        };
                        if config.is_ready()
                            || !self.settings.assistant_command.trim().is_empty()
                        {
                            self.request_assistant(input);
                        } else {
                            if input == "undo" || input.starts_with("tool ") {
                                self.selected_note = None;
                                self.selected_clip = None;
                                self.selected_clips.clear();
                            }
                            let reply = self.editor.assistant(&input);
                            if self.settings.speech
                                && let Err(error) = crate::speech::speak_with(&reply.text, &self.settings.speak_command)
                            {
                                self.status = Some(error);
                            }
                            self.assistant_log.push(reply.text);
                        }
                    }
                });
                if let Some(proposal) = self.assistant_proposal.clone() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(format!("Proposed change: {}", proposal.label))
                                .size(11.0)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(&proposal.command)
                                .monospace()
                                .size(9.0)
                                .color(theme::text_dim()),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() {
                                apply_proposal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_proposal = true;
                            }
                        });
                    });
                }
                if self.ai_pending.is_some() {
                    ui.label(
                        egui::RichText::new("Waiting for the AI endpoint…")
                            .size(10.0)
                            .color(theme::accent()),
                    );
                }
                if self.dictation_pending.is_some() {
                    ui.label(
                        egui::RichText::new("Listening and transcribing…")
                            .size(10.0)
                            .color(theme::accent()),
                    );
                }
                let mut speech = self.settings.speech;
                if ui.checkbox(&mut speech, "Speak replies").changed() {
                    self.settings.speech = speech;
                    let _ = self.settings.save(&crate::settings::Settings::path());
                }
                ui.label(
                    egui::RichText::new(
                        "With an AI endpoint or local assistant command set in Settings, replies come from that model or CLI; \
                         a `tool ...` reply runs one app tool. Otherwise local commands over \
                         the same tools are used. Speech supports local System / Kokoro-82M / KittenTTS / Piper TTS and \
                         whisper.cpp / sherpa-onnx Moonshine 16 kHz STT (configure in Settings).",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
            });
        if apply_proposal && let Some(proposal) = self.assistant_proposal.take() {
            self.selected_note = None;
            self.selected_clip = None;
            self.selected_clips.clear();
            let result = self.editor.assistant(&proposal.command);
            self.assistant_log.push(result.text);
        }
        if cancel_proposal && self.assistant_proposal.take().is_some() {
            self.assistant_log
                .push(String::from("proposal cancelled; no change"));
        }
        if !open {
            self.assistant_open = false;
        }
    }

    pub(super) fn assistant_inline(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("ASSISTANT")
                    .size(9.0)
                    .color(theme::text_dim()),
            );
            if ui
                .small_button("Open")
                .on_hover_text("Open the larger chat window")
                .clicked()
            {
                self.assistant_open = true;
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("inspector-chat")
            .max_height((height - 62.0).max(40.0))
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.assistant_log.is_empty() {
                    ui.label(
                        egui::RichText::new("Ask for help or run a tool.")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                for line in &self.assistant_log {
                    ui.label(egui::RichText::new(line).monospace().size(9.0));
                }
            });
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.assistant_input)
                    .hint_text("help")
                    .desired_width(130.0),
            );
            let enter =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (ui.small_button("Send").clicked() || enter) && !self.assistant_input.is_empty() {
                let input = self.assistant_input.trim().to_string();
                if input == "undo" || input.starts_with("tool ") {
                    self.selected_note = None;
                    self.selected_clip = None;
                    self.selected_clips.clear();
                }
                let reply = self.editor.assistant(&input);
                self.assistant_log.push(format!("> {input}"));
                self.assistant_log.push(reply.text);
                self.assistant_input.clear();
            }
        });
    }

    /// The Arrangement Doctor's proposal cards.
    pub(super) fn audition_proposal(&mut self, proposal: &arranging::Proposal, after: bool) {
        let mut project = self.editor.project().clone();
        if after
            && let Ok(mut session) = app_commands::EditSession::new(project.clone())
            && arranging::apply(&mut session, proposal).is_ok()
        {
            project = session.project().clone();
        }
        let rendered = audio::render(&project.song, audio::SAMPLE_RATE);
        let _ = audio::mix::check_audio_references(&project.song);
        self.audition_player = audio::player::Player::new(rendered).ok();
        self.status = Some(String::from(if after {
            "audition: after"
        } else {
            "audition: before"
        }));
    }
}
