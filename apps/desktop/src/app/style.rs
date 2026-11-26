//! Style, groove and beat windows of [`ShellApp`].

use super::*;

/// Style building, preview, file import and live section playback.
pub(super) struct StyleState {
    pub(super) clips_open: bool,
    pub(super) name: String,
    pub(super) chord: String,
    pub(super) length: u32,
    pub(super) from_markers: bool,
    pub(super) path: String,
    pub(super) sff1: bool,
    pub(super) draft: Option<arranger::style::Style>,
    pub(super) rules_open: bool,
    pub(super) preview: Option<audio::player::Player>,
    pub(super) link_open: bool,
    pub(super) link_url: String,
    pub(super) link_pending: Option<std::sync::mpsc::Receiver<Result<std::path::PathBuf, String>>>,
    pub(super) file_open: bool,
    pub(super) file_path: String,
    pub(super) import: Option<style_formats::ImportedStyle>,
    pub(super) section: usize,
    pub(super) file_loop: bool,
    pub(super) auto_load: bool,
    pub(super) queued_section: Option<usize>,
    pub(super) queued_chord: Option<String>,
    pub(super) fill_return: Option<usize>,
    pub(super) ending: bool,
    pub(super) held: [bool; 128],
    pub(super) last_chord: String,
    pub(super) queue_at: f64,
}

/// The build-a-style-from-audio window.
pub(super) struct AudioStyleState {
    pub(super) open: bool,
    pub(super) path: String,
    pub(super) bpm: f64,
    pub(super) chord: String,
    pub(super) section: String,
    pub(super) split: bool,
    pub(super) section_bars: u32,
    pub(super) drums: bool,
    pub(super) detect_tempo: bool,
    pub(super) out: String,
    pub(super) sff1: bool,
}

impl ShellApp {
    pub(super) fn style_rules_window(&mut self, context: &Context) {
        if !self.style.rules_open {
            return;
        }
        let mut open = self.style.rules_open;
        egui::Window::new("Style rules")
            .open(&mut open)
            .resizable(true)
            .default_width(620.0)
            .show(context, |ui| {
                let Some(style) = self.style.draft.as_mut() else {
                    ui.label("No style draft.");
                    return;
                };
                ui.label(
                    egui::RichText::new(
                        "Rules apply to the exported style; notes stay as recorded.",
                    )
                    .size(10.0)
                    .color(theme::text_dim()),
                );
                let roots = [
                    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                ];
                for section in &mut style.sections {
                    ui.separator();
                    ui.label(egui::RichText::new(&section.name).strong());
                    for part in &mut section.parts {
                        ui.group(|ui| {
                            ui.label(
                                egui::RichText::new(&part.name)
                                    .size(11.0)
                                    .color(theme::text_muted()),
                            );
                            ui.horizontal(|ui| {
                                ui.label("dest ch");
                                ui.add(
                                    egui::DragValue::new(&mut part.rule.dest_channel).range(9..=16),
                                );
                                ui.label("written root");
                                let root = usize::from(part.rule.source_root.min(11));
                                egui::ComboBox::from_id_salt(format!(
                                    "rule-root-{}-{}",
                                    section.name, part.name
                                ))
                                .selected_text(roots[root])
                                .show_ui(ui, |ui| {
                                    for (index, name) in roots.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut part.rule.source_root,
                                            index as u8,
                                            *name,
                                        );
                                    }
                                });
                                egui::ComboBox::from_id_salt(format!(
                                    "rule-type-{}-{}",
                                    section.name, part.name
                                ))
                                .selected_text(part.rule.source_chord_type.suffix())
                                .show_ui(ui, |ui| {
                                    for chord_type in arranger::chord_type::ALL_TYPES {
                                        ui.selectable_value(
                                            &mut part.rule.source_chord_type,
                                            chord_type,
                                            if chord_type.suffix().is_empty() {
                                                "major"
                                            } else {
                                                chord_type.suffix()
                                            },
                                        );
                                    }
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label("NTR");
                                egui::ComboBox::from_id_salt(format!(
                                    "rule-ntr-{}-{}",
                                    section.name, part.name
                                ))
                                .selected_text(format!("{:?}", part.rule.transpose.ntr))
                                .show_ui(ui, |ui| {
                                    for value in [
                                        arranger::rules::Ntr::RootTransposition,
                                        arranger::rules::Ntr::RootFixed,
                                        arranger::rules::Ntr::Guitar,
                                    ] {
                                        ui.selectable_value(
                                            &mut part.rule.transpose.ntr,
                                            value,
                                            format!("{value:?}"),
                                        );
                                    }
                                });
                                ui.label("NTT");
                                egui::ComboBox::from_id_salt(format!(
                                    "rule-ntt-{}-{}",
                                    section.name, part.name
                                ))
                                .selected_text(format!("{:?}", part.rule.transpose.ntt))
                                .show_ui(ui, |ui| {
                                    for value in [
                                        arranger::rules::Ntt::Bypass,
                                        arranger::rules::Ntt::Melody,
                                        arranger::rules::Ntt::Chord,
                                    ] {
                                        ui.selectable_value(
                                            &mut part.rule.transpose.ntt,
                                            value,
                                            format!("{value:?}"),
                                        );
                                    }
                                });
                                ui.checkbox(&mut part.rule.transpose.bass, "follow bass");
                            });
                            ui.horizontal(|ui| {
                                ui.label("held notes");
                                egui::ComboBox::from_id_salt(format!(
                                    "rule-rtr-{}-{}",
                                    section.name, part.name
                                ))
                                .selected_text(format!("{:?}", part.rule.transpose.rtr))
                                .show_ui(ui, |ui| {
                                    for value in [
                                        arranger::rules::Rtr::Stop,
                                        arranger::rules::Rtr::PitchShift,
                                        arranger::rules::Rtr::PitchShiftToRoot,
                                        arranger::rules::Rtr::Retrigger,
                                        arranger::rules::Rtr::RetriggerToRoot,
                                        arranger::rules::Rtr::NoteGenerator,
                                    ] {
                                        ui.selectable_value(
                                            &mut part.rule.transpose.rtr,
                                            value,
                                            format!("{value:?}"),
                                        );
                                    }
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label("high key");
                                ui.add(
                                    egui::DragValue::new(&mut part.rule.transpose.high_key)
                                        .range(0..=11),
                                );
                                ui.label("low");
                                ui.add(
                                    egui::DragValue::new(&mut part.rule.transpose.low_limit)
                                        .range(0..=127),
                                );
                                ui.label("high");
                                ui.add(
                                    egui::DragValue::new(&mut part.rule.transpose.high_limit)
                                        .range(0..=127),
                                );
                            });
                        });
                    }
                }
            });
        self.style.rules_open = open;
    }

    pub(super) fn build_audio_style(&self) -> Result<style_formats::BuiltAudioStyle, String> {
        let path = std::path::Path::new(&self.audio_style.path);
        let decoded = audio::decode::decode_mono(path).or_else(|_| {
            let bytes =
                std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
            audio::wav::decode_mono(&bytes)
        })?;
        let source = if self.audio_style.chord.eq_ignore_ascii_case("auto") {
            None
        } else {
            let chord = music_core::Chord::parse(&self.audio_style.chord)
                .map_err(|error| error.to_string())?;
            Some(
                arranger::chord_type::ArrangerChord::try_from(&chord)
                    .map_err(|_| format!("unsupported chord {:?}", self.audio_style.chord))?,
            )
        };
        style_formats::style_from_audio(
            &decoded,
            style_formats::AudioStyleOptions {
                bpm: self.audio_style.bpm,
                section: &self.audio_style.section,
                source_chord: source,
                split_parts: self.audio_style.split,
                section_bars: self.audio_style.section_bars,
                include_drums: self.audio_style.drums,
                detect_tempo: self.audio_style.detect_tempo,
            },
        )
    }

    pub(super) fn audio_style_window(&mut self, context: &Context) {
        if !self.audio_style.open {
            return;
        }
        let mut open = self.audio_style.open;
        egui::Window::new("Style from audio")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Audio file");
                ui.add(
                    egui::TextEdit::singleline(&mut self.audio_style.path)
                        .hint_text("WAV, FLAC, MP3, M4A")
                        .desired_width(320.0),
                );
                ui.horizontal(|ui| {
                    ui.label("BPM");
                    ui.add(egui::DragValue::new(&mut self.audio_style.bpm).range(20.0..=300.0));
                    ui.label("chord");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.audio_style.chord).desired_width(60.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("section");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.audio_style.section)
                            .desired_width(100.0),
                    );
                    ui.checkbox(&mut self.audio_style.split, "split bass/melody");
                });
                ui.horizontal(|ui| {
                    ui.label("section bars");
                    ui.add(egui::DragValue::new(&mut self.audio_style.section_bars).range(0..=16));
                    ui.label("output");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.audio_style.out).desired_width(150.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.audio_style.drums, "add drums from onsets");
                    ui.checkbox(&mut self.audio_style.detect_tempo, "detect BPM");
                    ui.checkbox(&mut self.audio_style.sff1, "SFF1 instead of SFF2");
                });
                ui.horizontal(|ui| {
                    if ui.button("Export style").clicked() {
                        match self.build_audio_style() {
                            Ok(built) => {
                                let version = if self.audio_style.sff1 {
                                    style_formats::casm::SffVersion::Sff1
                                } else {
                                    style_formats::casm::SffVersion::Sff2
                                };
                                match style_formats::write_native_style(&built.style, version)
                                    .map_err(|error| error.to_string())
                                    .and_then(|bytes| {
                                        std::fs::write(&self.audio_style.out, &bytes)
                                            .map(|()| bytes.len())
                                            .map_err(|error| error.to_string())
                                    }) {
                                    Ok(bytes) => {
                                        self.status = Some(format!(
                                            "wrote {}: source {}, {} note(s), {} bytes",
                                            self.audio_style.out,
                                            built.source_chord.name(),
                                            built.notes,
                                            bytes
                                        ));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Audition").clicked() {
                        match self
                            .build_audio_style()
                            .and_then(|built| style_preview_song(&built.style))
                            .and_then(|song| {
                                audio::player::Player::new(audio::render(&song, audio::SAMPLE_RATE))
                            }) {
                            Ok(player) => {
                                player.play();
                                self.style.preview = Some(player);
                                self.status = Some(String::from("auditioning audio style"));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Stop").clicked() {
                        self.style.preview = None;
                    }
                });
            });
        self.audio_style.open = open;
    }

    pub(super) fn audition_style_section(&mut self) {
        match self.style_file_preview_song().and_then(|song| {
            audio::player::Player::new(audio::render(&song, audio::SAMPLE_RATE))
                .map_err(|error| error.to_string())
        }) {
            Ok(player) => {
                player.play();
                self.style.preview = Some(player);
                self.status = Some(String::from("auditioning style section"));
            }
            Err(error) => self.status = Some(error),
        }
    }

    pub(super) fn queue_style_section(&mut self, index: usize) {
        let Some(imported) = self.style.import.as_ref() else {
            return;
        };
        let current_role = imported
            .section_names()
            .get(self.style.section)
            .map_or(StyleSectionRole::Main, |name| style_section_role(name));
        let role = imported
            .section_names()
            .get(index)
            .map_or(StyleSectionRole::Main, |name| style_section_role(name));
        match role {
            StyleSectionRole::Fill => {
                if current_role == StyleSectionRole::Main {
                    self.style.fill_return = Some(self.style.section);
                }
                self.style.ending = false;
            }
            StyleSectionRole::Ending => {
                self.style.fill_return = None;
                self.style.ending = true;
            }
            StyleSectionRole::Main => {
                self.style.fill_return = None;
                self.style.ending = false;
            }
        }
        let Some(player) = self.style.preview.as_ref() else {
            self.style.section = index;
            self.audition_style_section();
            return;
        };
        self.style.section = index;
        if !player.is_playing() {
            self.audition_style_section();
            return;
        }
        let bar_ticks = imported.time_signature.bar_ticks().max(1);
        let seconds = 60.0 / (imported.tempo_bpm.max(30.0) * music_core::PPQ as f64);
        let bar_seconds = bar_ticks as f64 * seconds;
        let now = player.seconds();
        self.style.queued_section = Some(index);
        self.style.queue_at = next_bar_seconds(now, bar_seconds);
        self.status = Some(String::from("section queued at the next bar"));
    }

    pub(super) fn queue_style_chord(&mut self, chord: &str) {
        let Some(imported) = self.style.import.as_ref() else {
            return;
        };
        let Some(player) = self.style.preview.as_ref() else {
            self.style.chord = chord.to_string();
            self.audition_style_section();
            return;
        };
        if !player.is_playing() {
            self.style.chord = chord.to_string();
            self.audition_style_section();
            return;
        }
        let bar_ticks = imported.time_signature.bar_ticks().max(1);
        let seconds = 60.0 / (imported.tempo_bpm.max(30.0) * music_core::PPQ as f64);
        let bar_seconds = bar_ticks as f64 * seconds;
        let now = player.seconds();
        self.style.chord = chord.to_string();
        self.style.queued_chord = Some(chord.to_string());
        self.style.queue_at = next_bar_seconds(now, bar_seconds);
        self.status = Some(format!("chord {chord} queued at the next bar"));
    }

    pub(super) fn load_style_file(&mut self) {
        match std::fs::read(&self.style.file_path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                style_formats::import_style(&bytes).map_err(|error| error.to_string())
            }) {
            Ok(imported) => {
                self.style.section = 0;
                self.style.fill_return = None;
                self.style.ending = false;
                self.status = Some(format!(
                    "style loaded: {} section(s)",
                    imported.sections.len()
                ));
                self.style.import = Some(imported);
            }
            Err(error) => self.status = Some(error),
        }
    }

    pub(super) fn style_link_window(&mut self, context: &Context) {
        if !self.style.link_open {
            return;
        }
        let mut open = self.style.link_open;
        egui::Window::new("Style from link")
            .open(&mut open)
            .resizable(false)
            .default_width(380.0)
            .show(context, |ui| {
                ui.label("Link");
                ui.add(
                    egui::TextEdit::singleline(&mut self.style.link_url)
                        .hint_text("https://…")
                        .desired_width(300.0),
                );
                ui.label(
                    egui::RichText::new(
                        "Downloads the best audio with yt-dlp and opens Style from audio. yt-dlp and FFmpeg must be installed.",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
                if ui.button("Download audio").clicked() {
                    if self.style.link_url.trim().is_empty() {
                        self.status = Some(String::from("enter a link first"));
                    } else {
                        let url = self.style.link_url.clone();
                        let (sender, receiver) = std::sync::mpsc::channel();
                        self.style.link_pending = Some(receiver);
                        self.status = Some(String::from("downloading link audio…"));
                        std::thread::spawn(move || {
                            let _ = sender.send(audio::link::download_mono_wav(&url));
                        });
                    }
                }
                if self.style.link_pending.is_some() {
                    ui.label(
                        egui::RichText::new("Downloading…")
                            .size(10.0)
                            .color(theme::accent()),
                    );
                }
            });
        self.style.link_open = open;
    }

    pub(super) fn style_file_window(&mut self, context: &Context) {
        if !self.style.file_open {
            return;
        }
        if self.style.auto_load {
            self.style.auto_load = false;
            self.load_style_file();
        }
        let mut open = self.style.file_open;
        let mut queue_choice = None;
        let mut chord_choice = None;
        egui::Window::new("Style file player")
            .open(&mut open)
            .resizable(false)
            .default_width(360.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label("File");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.style.file_path)
                            .desired_width(220.0),
                    );
                    if ui.button("Load").clicked() {
                        self.load_style_file();
                    }
                });
                let Some(imported) = &self.style.import else {
                    ui.label(
                        egui::RichText::new("Load a Yamaha style to audition sections.")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                    return;
                };
                let names = imported.section_names();
                if names.is_empty() {
                    ui.label("No sections.");
                    return;
                }
                self.style.section = self.style.section.min(names.len() - 1);
                egui::ComboBox::from_id_salt("style-file-section")
                    .selected_text(names[self.style.section])
                    .show_ui(ui, |ui| {
                        for (index, name) in names.iter().enumerate() {
                            if ui
                                .selectable_label(self.style.section == index, *name)
                                .clicked()
                                && self.style.section != index
                            {
                                queue_choice = Some(index);
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    ui.label("Chord");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.style.chord).desired_width(70.0),
                    );
                    for chord in ["C", "Dm", "Em", "F", "G", "Am", "G7"] {
                        if ui.small_button(chord).clicked() {
                            chord_choice = Some(chord);
                        }
                    }
                });
                ui.label(format!(
                    "style written in {} · {:.1} BPM · {} ticks",
                    imported.native.source_chord.name(),
                    imported.tempo_bpm,
                    imported.native.sections[self.style.section].length_ticks
                ));
                ui.horizontal(|ui| {
                    if ui.button("Audition").clicked() {
                        self.style.queued_section = None;
                        self.style.queued_chord = None;
                        self.style.fill_return = None;
                        self.style.ending = false;
                        self.audition_style_section();
                    }
                    if ui.button("Stop").clicked() {
                        self.style.preview = None;
                    }
                    ui.checkbox(&mut self.style.file_loop, "Loop");
                });
                ui.label(
                    egui::RichText::new(
                        "Section and chord changes queue at the next bar; held MIDI notes are recognised; fills return to the main section and endings stop looping.",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
            });
        self.style.file_open = open;
        if let Some(chord) = chord_choice {
            self.queue_style_chord(chord);
        }
        if let Some(index) = queue_choice {
            self.queue_style_section(index);
        }
    }

    pub(super) fn style_file_preview_song(&self) -> Result<project::Song, String> {
        let imported = self
            .style
            .import
            .as_ref()
            .ok_or_else(|| String::from("load a style first"))?;
        style_section_song(imported, self.style.section, &self.style.chord)
    }

    pub(super) fn style_clips_window(&mut self, context: &Context) {
        if !self.style.clips_open {
            return;
        }
        let mut open = self.style.clips_open;
        egui::Window::new("Style from clips")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Section name");
                ui.text_edit_singleline(&mut self.style.name);
                ui.horizontal(|ui| {
                    ui.label("Chord");
                    ui.add(egui::TextEdit::singleline(&mut self.style.chord).desired_width(80.0));
                });
                ui.checkbox(&mut self.style.from_markers, "split at markers");
                if !self.style.from_markers {
                    ui.add(
                        egui::DragValue::new(&mut self.style.length)
                            .range(1..=960 * 128)
                            .suffix(" ticks"),
                    );
                }
                ui.horizontal(|ui| {
                    ui.label("Output");
                    ui.add(egui::TextEdit::singleline(&mut self.style.path).desired_width(220.0));
                });
                ui.checkbox(&mut self.style.sff1, "SFF1 instead of SFF2");
                ui.horizontal(|ui| {
                    if ui.button("Export style").clicked() {
                        match self.export_style_from_clips() {
                            Ok(message) => self.status = Some(message),
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Audition").clicked() {
                        match self.build_style_from_clips() {
                            Ok(style) => match style_preview_song(&style).and_then(|song| {
                                audio::player::Player::new(audio::render(&song, audio::SAMPLE_RATE))
                            }) {
                                Ok(player) => {
                                    player.play();
                                    self.style.preview = Some(player);
                                    self.status = Some(String::from("auditioning style"));
                                }
                                Err(error) => self.status = Some(error),
                            },
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Stop").clicked() {
                        self.style.preview = None;
                    }
                    if ui.button("Edit rules").clicked() {
                        match self.build_style_from_clips() {
                            Ok(style) => {
                                self.style.draft = Some(style);
                                self.style.rules_open = true;
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
            });
        self.style.clips_open = open;
    }

    pub(super) fn build_style_from_clips(&self) -> Result<arranger::style::Style, String> {
        if let Some(draft) = &self.style.draft {
            return Ok(draft.clone());
        }
        let chord =
            music_core::Chord::parse(&self.style.chord).map_err(|error| error.to_string())?;
        let source = arranger::chord_type::ArrangerChord::try_from(&chord)
            .map_err(|_| format!("unsupported style chord {:?}", self.style.chord))?;
        let mut style = if self.style.from_markers {
            arranger::style::from_song_sections(self.song(), &self.song().name, source)
        } else {
            arranger::style::from_song(
                self.song(),
                &self.song().name,
                source,
                music_core::Tick::from(self.style.length.max(1)),
            )
        };
        if !self.style.from_markers
            && let Some(first) = style.sections.first_mut()
        {
            first.name = self.style.name.clone();
        }
        Ok(style)
    }

    pub(super) fn export_style_from_clips(&self) -> Result<String, String> {
        let style = self.build_style_from_clips()?;
        let version = if self.style.sff1 {
            style_formats::casm::SffVersion::Sff1
        } else {
            style_formats::casm::SffVersion::Sff2
        };
        let bytes = style_formats::write_native_style(&style, version)
            .map_err(|error| error.to_string())?;
        if let Some(parent) = std::path::Path::new(&self.style.path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&self.style.path, &bytes)
            .map_err(|error| format!("{}: {error}", self.style.path))?;
        Ok(format!(
            "wrote {}: {} section(s), {} byte(s)",
            self.style.path,
            style.sections.len(),
            bytes.len()
        ))
    }

    fn audition_groove(
        &mut self,
        track_id: TrackId,
        groove: &arranging::groove::Groove,
        amounts: arranging::groove::Amounts,
        after: bool,
    ) {
        let mut song = self.editor.project().song.clone();
        if after && let Some(track) = song.tracks.iter_mut().find(|entry| entry.id == track_id) {
            track.notes = arranging::groove::apply(&track.notes, groove, amounts);
        }
        let rendered = audio::render(&song, audio::SAMPLE_RATE);
        self.audition_player = audio::player::Player::new(rendered).ok();
        if let Some(player) = &self.audition_player {
            player.play();
        }
        self.status = Some(String::from(if after {
            "groove audition: after"
        } else {
            "groove audition: before"
        }));
    }

    pub(super) fn groove_window(&mut self, context: &egui::Context) {
        if !self.groove_open {
            return;
        }
        let mut open = self.groove_open;
        let mut audition: Option<bool> = None;
        let mut stop_audition = false;
        egui::Window::new("Groove library")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                if ui.button("Refresh").clicked() || self.groove_entries.is_empty() {
                    self.groove_entries =
                        arranging::groove_file::list(&crate::settings::data_file("grooves"))
                            .unwrap_or_default();
                }
                let selected = self
                    .groove_entries
                    .get(self.groove_selected)
                    .map(|entry| entry.0.clone())
                    .unwrap_or_else(|| String::from("No groove files"));
                egui::ComboBox::from_id_salt("groove-file")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (index, (name, _)) in self.groove_entries.iter().enumerate() {
                            ui.selectable_value(&mut self.groove_selected, index, name);
                        }
                    });
                if ui.button("Load").clicked()
                    && let Some((_, path)) = self.groove_entries.get(self.groove_selected)
                {
                    match arranging::groove_file::load(path) {
                        Ok((_, groove)) => {
                            self.groove_loaded = Some(groove);
                            self.status = Some(String::from("groove loaded"));
                        }
                        Err(error) => self.status = Some(error),
                    }
                }
                ui.add(egui::Slider::new(&mut self.groove_timing, 0.0..=1.0).text("timing"));
                ui.add(egui::Slider::new(&mut self.groove_accents, 0.0..=1.0).text("accents"));
                if let Some(groove) = &self.groove_loaded {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} slots · grid {}",
                            groove.offsets.len(),
                            groove.grid
                        ))
                        .size(10.0)
                        .color(theme::text_dim()),
                    );
                }
                let can_preview = self.groove_loaded.is_some() && self.roll.selected.is_some();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_preview, egui::Button::new("Play before"))
                        .on_hover_text("Audition the song before applying this groove")
                        .clicked()
                    {
                        audition = Some(false);
                    }
                    if ui
                        .add_enabled(can_preview, egui::Button::new("Play after"))
                        .on_hover_text("Audition the selected track with this groove applied")
                        .clicked()
                    {
                        audition = Some(true);
                    }
                    if self.audition_player.is_some() && ui.button("Stop").clicked() {
                        stop_audition = true;
                    }
                });
                if ui
                    .add_enabled(can_preview, egui::Button::new("Apply to selected track"))
                    .clicked()
                    && let Some(groove) = &self.groove_loaded
                    && let Some(track) = self.roll.selected
                {
                    let amounts = arranging::groove::Amounts {
                        timing: self.groove_timing,
                        accents: self.groove_accents,
                    };
                    match self.editor.apply_stored_groove(track, groove, amounts) {
                        Ok(count) => {
                            self.status = Some(format!("groove applied to {count} note(s)"))
                        }
                        Err(error) => self.status = Some(error),
                    }
                }
            });
        if stop_audition {
            if let Some(player) = self.audition_player.take() {
                player.stop();
                player.release();
            }
            self.status = Some(String::from("groove audition stopped"));
        } else if let Some(after) = audition
            && let Some(groove) = self.groove_loaded.clone()
            && let Some(track) = self.roll.selected
        {
            let amounts = arranging::groove::Amounts {
                timing: self.groove_timing,
                accents: self.groove_accents,
            };
            self.audition_groove(track, &groove, amounts, after);
        }
        self.groove_open = open;
    }

    /// The Beat view: four drum rows of sixteen steps for one bar.
    pub(super) fn load_beat(&mut self) {
        let bar = self.song().time_signature.bar_ticks().max(1);
        self.load_beat_from(snap(self.position, bar));
    }

    pub(super) fn load_beat_from(&mut self, start: Tick) {
        let Some(track) = self
            .roll
            .selected
            .or_else(|| self.song().tracks.first().map(|track| track.id))
        else {
            return;
        };
        if let Some((bar, pattern)) = self.editor.beat_pattern(track, start) {
            self.beat_track = Some(track);
            self.beat_start = start;
            self.beat_bar = bar;
            self.beat_pattern = pattern;
        }
    }

    pub(super) fn beat_dialog(&mut self, context: &Context) {
        if !self.beat_dialog {
            return;
        }
        let mut open = self.beat_dialog;
        let bar = self.song().time_signature.bar_ticks().max(1);
        let mut nav = 0_i32;
        egui::Window::new("Beat")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(format!(
                    "bar {} · {}, {} bars · click to cycle off/soft/normal/loud",
                    self.beat_start / self.beat_bar.max(1) + 1,
                    self.beat_pattern.bars(),
                    self.beat_bars
                ));
                let mut reload_bars = None;
                ui.horizontal(|ui| {
                    if ui.small_button("◀").on_hover_text("Previous bar").clicked() {
                        nav = -1;
                    }
                    if ui.small_button("▶").on_hover_text("Next bar").clicked() {
                        nav = 1;
                    }
                    for bars in [1_usize, 2, 4] {
                        if ui
                            .selectable_label(self.beat_bars == bars, format!("{bars} bar"))
                            .clicked()
                        {
                            reload_bars = Some(bars);
                        }
                    }
                    ui.add_space(8.0);
                    if ui
                        .small_button("Clear all")
                        .on_hover_text("Clear every row in the pattern")
                        .clicked()
                    {
                        for row in 0..arranging::beat::ROWS.len() {
                            self.beat_pattern.clear_row(row);
                        }
                    }
                });
                egui::ScrollArea::horizontal()
                    .id_salt("beat-grid")
                    .show(ui, |ui| {
                        for (row, (_, label)) in arranging::beat::ROWS.iter().enumerate() {
                            let row_color = theme::track_colour(row);
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [44.0, 22.0],
                                    egui::Label::new(
                                        egui::RichText::new(*label)
                                            .size(10.0)
                                            .color(theme::text_muted()),
                                    ),
                                );
                                let steps = self.beat_pattern.grid[row].len();
                                for slot in 0..steps {
                                    if slot > 0 && slot % 4 == 0 {
                                        ui.add_space(5.0);
                                    }
                                    let level = self.beat_pattern.grid[row][slot];
                                    let (text, fill) = if level == 0 {
                                        (String::from("·"), theme::raised())
                                    } else if level < 85 {
                                        (level.to_string(), row_color.gamma_multiply(0.32))
                                    } else if level < 115 {
                                        (level.to_string(), row_color.gamma_multiply(0.58))
                                    } else {
                                        (level.to_string(), row_color.gamma_multiply(0.88))
                                    };
                                    if ui
                                        .add_sized(
                                            [24.0, 22.0],
                                            egui::Button::new(egui::RichText::new(text).size(9.5))
                                                .fill(fill),
                                        )
                                        .clicked()
                                    {
                                        self.beat_pattern.cycle(row, slot);
                                    }
                                }
                                ui.add_space(5.0);
                                let copied_row = self.beat_pattern.row(row).map(<[u8]>::to_vec);
                                if ui.small_button("C").on_hover_text("Copy row").clicked() {
                                    self.beat_clipboard = copied_row;
                                }
                                let can_paste = self.beat_clipboard.is_some();
                                if ui
                                    .add_enabled(can_paste, egui::Button::new("P"))
                                    .on_hover_text("Paste row")
                                    .clicked()
                                    && let Some(values) = self.beat_clipboard.clone()
                                {
                                    self.beat_pattern.set_row(row, &values);
                                }
                                if ui.small_button("X").on_hover_text("Clear row").clicked() {
                                    self.beat_pattern.clear_row(row);
                                }
                            });
                        }
                    });
                if let Some(bars) = reload_bars
                    && let (Some(track), Some(_)) = (self.beat_track, Some(self.beat_start))
                    && let Some((bar, pattern)) =
                        self.editor.beat_pattern_bars(track, self.beat_start, bars)
                {
                    self.beat_bar = bar;
                    self.beat_pattern = pattern;
                    self.beat_bars = bars;
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
                    if ui
                        .button("Put in 4 bars")
                        .on_hover_text("Repeat the pattern across four bars as one edit")
                        .clicked()
                        && let Some(track) = self.beat_track
                    {
                        let bar = self.beat_bar;
                        let start = self.beat_start;
                        match self
                            .editor
                            .put_in_bars(track, start, bar, &self.beat_pattern, 4)
                        {
                            Ok(added) => {
                                self.status = Some(format!("beat: {added} hit(s) in 4 bars"));
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
        if nav != 0 {
            self.load_beat_from((self.beat_start + Tick::from(nav) * bar).max(0));
        }
    }
}
