//! The main layout of [`ShellApp`]: top bar, sidebar, track rows, grid,
//! inspector, status bar and the window dispatcher.

use super::*;

/// The score view: note entry, paging, zoom and full-score options.
pub(super) struct ScoreUi {
    pub(super) entry: bool,
    pub(super) octave: i32,
    pub(super) note_length: music_core::Tick,
    pub(super) dotted: bool,
    pub(super) cursor: music_core::Tick,
    pub(super) page: usize,
    pub(super) zoom: f32,
    pub(super) grand: bool,
    pub(super) preview: bool,
    pub(super) all_tracks: bool,
    pub(super) follow: bool,
    pub(super) full_frames: bool,
    pub(super) full_hidden: std::collections::HashSet<TrackId>,
    pub(super) full_cache: Option<(u64, Vec<(String, score::Engraved)>)>,
    pub(super) pdf_path: String,
    pub(super) track: Option<TrackId>,
    pub(super) state: crate::score_view::ScoreState,
}

impl ShellApp {
    pub(super) fn top_bar(&mut self, context: &Context) {
        egui::TopBottomPanel::top("header")
            .exact_height(theme::HEADER_HEIGHT)
            .frame(egui::Frame::NONE.fill(theme::panel()))
            .show(context, |ui| {
                if self.settings.kids_mode {
                    ui.spacing_mut().button_padding = egui::Vec2::new(10.0, 7.0);
                }
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    egui::Stroke::new(1.0_f32, theme::line()),
                );
                let mut area = rect.shrink2(egui::vec2(16.0, 0.0));
                // Title area: badge, title, subtitle (HeaderBar::paint).
                let title_area = Rect::from_min_size(area.min, egui::vec2(200.0, area.height()));
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
                    egui::FontId::proportional(14.0),
                    theme::text(),
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
                    theme::text_muted(),
                );

                // Transport buttons: stop 44, play 54, 8 px gaps.
                let buttons_rect = Rect::from_min_size(area.min, egui::vec2(214.0, area.height()));
                area = Rect::from_min_max(egui::pos2(buttons_rect.right(), area.top()), area.max);
                let mut x = buttons_rect.left();
                let stop = Rect::from_center_size(
                    egui::pos2(x + 19.0, buttons_rect.center().y),
                    egui::vec2(38.0, 38.0),
                );
                x += 44.0;
                let play = Rect::from_center_size(
                    egui::pos2(x + 22.0, buttons_rect.center().y),
                    egui::vec2(44.0, 44.0),
                );
                if transport_button(ui, stop, "\u{25A0}", false) {
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
                x += 50.0;
                let record = Rect::from_center_size(
                    egui::pos2(x + 19.0, buttons_rect.center().y),
                    egui::vec2(38.0, 38.0),
                );
                if transport_button(ui, record, "\u{25CF}", self.recording.is_some()) {
                    self.start_midi_recording();
                }
                x += 44.0;
                let undo = Rect::from_center_size(
                    egui::pos2(x + 16.0, buttons_rect.center().y),
                    egui::vec2(32.0, 32.0),
                );
                if transport_button(ui, undo, "\u{21B6}", false) && !self.editor.undo() {
                    self.status = Some(String::from("nothing to undo"));
                }
                x += 38.0;
                let metronome = Rect::from_center_size(
                    egui::pos2(x + 16.0, buttons_rect.center().y),
                    egui::vec2(32.0, 32.0),
                );
                if metronome_button(ui, metronome, self.settings.metronome) {
                    self.settings.metronome = !self.settings.metronome;
                }
                // LCD, 320 x 52, then the view tabs on the right.
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
                if self.settings.kids_mode {
                    let settings_rect = Rect::from_center_size(
                        egui::pos2(rect.right() - 16.0 - 40.0, rect.center().y),
                        egui::vec2(80.0, 34.0),
                    );
                    let response = ui.interact(
                        settings_rect,
                        ui.id().with("kids-settings"),
                        egui::Sense::click(),
                    );
                    ui.painter()
                        .rect_filled(settings_rect, 4.0, theme::raised());
                    ui.painter().rect_stroke(
                        settings_rect,
                        4.0,
                        egui::Stroke::new(1.0_f32, theme::line()),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        settings_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Settings",
                        egui::FontId::proportional(11.0),
                        theme::text(),
                    );
                    if response.on_hover_text("Adult settings").clicked() {
                        self.settings_open = true;
                    }
                    return;
                }
                let wide = rect.width() >= 1380.0;
                let ai_rect = Rect::from_center_size(
                    egui::pos2(rect.right() - 16.0 - 38.0, rect.center().y),
                    egui::vec2(76.0, 34.0),
                );
                let (mixer_rect, audio_rect, minus_rect, plus_rect) = if wide {
                    let plus_rect = Rect::from_center_size(
                        egui::pos2(ai_rect.left() - 6.0 - 12.0, rect.center().y),
                        egui::vec2(24.0, 28.0),
                    );
                    let minus_rect = Rect::from_center_size(
                        egui::pos2(plus_rect.left() - 4.0 - 12.0, rect.center().y),
                        egui::vec2(24.0, 28.0),
                    );
                    let audio_rect = Rect::from_center_size(
                        egui::pos2(minus_rect.left() - 8.0 - 40.0, rect.center().y),
                        egui::vec2(80.0, 32.0),
                    );
                    let mixer_rect = Rect::from_center_size(
                        egui::pos2(audio_rect.left() - 8.0 - 27.0, rect.center().y),
                        egui::vec2(54.0, 34.0),
                    );
                    (
                        mixer_rect,
                        Some(audio_rect),
                        Some(minus_rect),
                        Some(plus_rect),
                    )
                } else {
                    let mixer_rect = Rect::from_center_size(
                        egui::pos2(ai_rect.left() - 8.0 - 27.0, rect.center().y),
                        egui::vec2(54.0, 34.0),
                    );
                    (mixer_rect, None, None, None)
                };
                let tabs_rect = Rect::from_center_size(
                    egui::pos2(
                        mixer_rect.left() - 8.0 - widgets::TABS_SIZE.x * 0.5,
                        rect.center().y,
                    ),
                    widgets::TABS_SIZE,
                );
                let mut tabs_ui = ui.new_child(egui::UiBuilder::new().max_rect(tabs_rect));
                let views: Vec<View> = View::allowed(self.settings.kids_mode)
                    .into_iter()
                    .filter(|view| self.settings.kids_mode || *view != View::Mix)
                    .collect();
                let names: Vec<&str> = views
                    .iter()
                    .map(|view| match view {
                        View::Arrange => "Timeline",
                        View::Mix => "Mixer",
                        View::Drums => "Drums",
                        View::Map => "Map",
                        other => crate::strings::text(
                            crate::strings::Language::parse(&self.settings.language),
                            other.key(),
                        ),
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
                    if *view == View::Drums {
                        self.load_beat();
                    }
                }
                if let (Some(audio_rect), Some(minus_rect), Some(plus_rect)) =
                    (audio_rect, minus_rect, plus_rect)
                {
                    let audio_response = ui.interact(
                        audio_rect,
                        ui.id().with("audio-settings"),
                        egui::Sense::click(),
                    );
                    ui.painter().rect_filled(audio_rect, 4.0, theme::raised());
                    ui.painter().rect_stroke(
                        audio_rect,
                        4.0,
                        egui::Stroke::new(1.0_f32, theme::line()),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().circle_filled(
                        egui::pos2(audio_rect.left() + 10.0, audio_rect.center().y),
                        3.5,
                        egui::Color32::from_rgb(0x4c, 0xaf, 0x50),
                    );
                    ui.painter().text(
                        egui::pos2(audio_rect.left() + 20.0, audio_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "CoreAudio",
                        egui::FontId::proportional(11.0),
                        theme::text(),
                    );
                    if audio_response
                        .on_hover_text("Audio and MIDI settings")
                        .clicked()
                    {
                        self.settings_open = true;
                    }
                    for (rect, glyph, direction) in
                        [(minus_rect, "-", 1.25_f32), (plus_rect, "+", 0.8_f32)]
                    {
                        let response = ui.interact(rect, ui.id().with(glyph), egui::Sense::click());
                        ui.painter().rect_filled(
                            rect,
                            4.0,
                            if response.hovered() {
                                theme::line()
                            } else {
                                theme::raised()
                            },
                        );
                        ui.painter().rect_stroke(
                            rect,
                            4.0,
                            egui::Stroke::new(1.0_f32, theme::line()),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            glyph,
                            egui::FontId::proportional(14.0),
                            theme::text(),
                        );
                        if response
                            .on_hover_text(if direction < 1.0 {
                                "Zoom in"
                            } else {
                                "Zoom out"
                            })
                            .clicked()
                        {
                            if self.view == View::Arrange && self.timeline_mode {
                                self.timeline_zoom =
                                    (self.timeline_zoom * direction).clamp(0.5, 3.0);
                            } else {
                                self.roll.ticks_per_point =
                                    (self.roll.ticks_per_point * direction).clamp(2.0, 64.0);
                            }
                        }
                    }
                }
                let mixer_response = ui.interact(
                    mixer_rect,
                    ui.id().with("mixer-toggle"),
                    egui::Sense::click(),
                );
                ui.painter().rect_filled(
                    mixer_rect,
                    4.0,
                    if self.mixer_visible {
                        theme::accent().gamma_multiply(0.18)
                    } else {
                        theme::raised()
                    },
                );
                ui.painter().rect_stroke(
                    mixer_rect,
                    4.0,
                    egui::Stroke::new(1.0_f32, theme::line()),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    mixer_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Mixer",
                    egui::FontId::proportional(11.0),
                    theme::text(),
                );
                if mixer_response
                    .on_hover_text("Show or hide the mixer")
                    .clicked()
                {
                    self.mixer_visible = !self.mixer_visible;
                }
                let ai_response =
                    ui.interact(ai_rect, ui.id().with("ask-ai"), egui::Sense::click());
                ui.painter().rect_filled(
                    ai_rect,
                    16.0,
                    theme::accent().gamma_multiply(if ai_response.hovered() { 1.0 } else { 0.9 }),
                );
                ui.painter().text(
                    ai_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Ask AI",
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
                if ai_response.on_hover_text("Open the assistant").clicked() {
                    self.assistant.open = true;
                }
                let title = Self::window_title(&self.song().name, self.editor.dirty());
                context.send_viewport_cmd(egui::ViewportCommand::Title(title));
            });
    }

    pub(super) fn sidebar(&mut self, context: &Context) {
        if self.settings.kids_mode && self.view == View::Perform {
            return;
        }
        egui::SidePanel::left("tracks")
            .resizable(true)
            .default_width(theme::SIDEBAR_WIDTH)
            .width_range(180.0..=420.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Tracks").size(13.0).strong());
                    ui.label(
                        egui::RichText::new(self.song().tracks.len().to_string())
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("+").on_hover_text("Add a track").clicked() {
                            match self.editor.add_empty_track() {
                                Ok(id) => {
                                    self.roll.selected = Some(id);
                                    self.status = Some(String::from("track added"));
                                }
                                Err(error) => self.status = Some(error),
                            }
                        }
                    });
                });
                let chord_track = self
                    .roll
                    .selected
                    .and_then(|id| self.song().tracks.iter().find(|track| track.id == id))
                    .or_else(|| {
                        self.song()
                            .tracks
                            .iter()
                            .max_by_key(|track| track.notes.len())
                    })
                    .map(|track| track.name.clone())
                    .unwrap_or_else(|| String::from("no track"));
                ui.label(
                    egui::RichText::new(format!("Chords on {chord_track}"))
                        .size(9.0)
                        .color(theme::text_dim()),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.loop_enabled, "Loop")
                        .on_hover_text("Repeat the loop region")
                        .clicked()
                    {
                        self.loop_enabled = !self.loop_enabled;
                        if self.loop_enabled && self.loop_region.is_none() {
                            let bar = self.song().time_signature.bar_ticks().max(1);
                            self.loop_region =
                                Some(crate::transport::bar_region(self.position, bar));
                        }
                        self.sync_player_loop();
                    }
                    if ui
                        .small_button("From bar")
                        .on_hover_text("Set the loop to the current bar")
                        .clicked()
                    {
                        let bar = self.song().time_signature.bar_ticks().max(1);
                        self.loop_region = Some(crate::transport::bar_region(self.position, bar));
                        self.loop_enabled = true;
                        self.sync_player_loop();
                    }
                });
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
                        audio: track.audio.is_some(),
                        muted: track.muted,
                        soloed: track.soloed,
                        armed: track.armed,
                    })
                    .collect();
                egui::ScrollArea::vertical()
                    .id_salt("track-list")
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - 110.0).max(120.0))
                    .show(ui, |ui| {
                        for row in rows {
                            self.track_row(ui, &row);
                        }
                        if self.song().tracks.is_empty() {
                            ui.label(egui::RichText::new("no tracks").color(theme::text_dim()));
                        }
                    });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if self.settings.kids_mode {
                        ui.horizontal(|ui| {
                            let size = egui::Vec2::new(74.0, 30.0);
                            if ui.add_sized(size, egui::Button::new("Undo")).clicked() {
                                let _ = self.editor.undo();
                            }
                            if ui.add_sized(size, egui::Button::new("Redo")).clicked() {
                                let _ = self.editor.redo();
                            }
                            let play = if self.playing { "Pause" } else { "Play" };
                            if ui.add_sized(size, egui::Button::new(play)).clicked() {
                                self.toggle_playback();
                            }
                            if ui.add_sized(size, egui::Button::new("Stop")).clicked() {
                                self.stop_playback();
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Settings").clicked() {
                                self.settings_open = true;
                            }
                            if ui.button("Lessons").clicked() {
                                self.lessons_open = true;
                            }
                            if ui.button("Theory").clicked() {
                                self.theory_open = true;
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            let label = if self.recording.is_some() {
                                "Recording…"
                            } else {
                                "Record"
                            };
                            if ui.small_button(label).clicked() && self.recording.is_none() {
                                self.start_midi_recording();
                            }
                            ui.add(
                                egui::DragValue::new(&mut self.record_seconds)
                                    .range(1.0..=60.0)
                                    .suffix(" s"),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.settings.record_count_in)
                                    .range(0..=2)
                                    .suffix(" bar"),
                            )
                            .on_hover_text("Count-in before MIDI or audio recording");
                            let audio_label = if self.audio_recording.is_some() {
                                "Recording audio…"
                            } else {
                                "Rec audio"
                            };
                            if ui
                                .add_enabled(
                                    self.audio_recording.is_none(),
                                    egui::Button::new(audio_label),
                                )
                                .clicked()
                            {
                                let folder = crate::settings::Settings::data_dir().join("audio");
                                let stamp = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|duration| duration.as_nanos())
                                    .unwrap_or(0);
                                let path = folder.join(format!("take-{stamp}.wav"));
                                let _ = std::fs::create_dir_all(&folder);
                                let seconds = self.audio_record_seconds;
                                let start = self.position.max(0);
                                let input = self.settings.audio_input.clone();
                                let input_channel = (self.settings.audio_input_channel > 0)
                                    .then_some(usize::from(self.settings.audio_input_channel - 1));
                                let bpm = self.song().tempo_map.bpm_at(0).max(1.0);
                                let meter = self.song().time_signature;
                                let count_in = self.settings.record_count_in;
                                let delay = count_in_seconds(bpm, count_in, meter);
                                let (sender, receiver) = std::sync::mpsc::channel();
                                std::thread::spawn(move || {
                                    if delay > 0.0 {
                                        if let Ok(player) = audio::player::Player::new(
                                            count_in_audio(bpm, count_in, meter),
                                        ) {
                                            player.play();
                                            std::thread::sleep(std::time::Duration::from_secs_f64(
                                                delay,
                                            ));
                                            player.release();
                                        } else {
                                            std::thread::sleep(std::time::Duration::from_secs_f64(
                                                delay,
                                            ));
                                        }
                                    }
                                    let result = audio::record::record_seconds_from_channel(
                                        input.as_deref(),
                                        input_channel,
                                        seconds,
                                    )
                                    .and_then(|rendered| {
                                        std::fs::write(&path, rendered.to_wav24())
                                            .map_err(|error| format!("{}: {error}", path.display()))
                                    })
                                    .map(|()| path);
                                    let _ = sender.send(result);
                                });
                                self.audio_recording = Some(receiver);
                                self.audio_record_start = start;
                                self.audio_record_target = self
                                    .song()
                                    .tracks
                                    .iter()
                                    .find(|track| track.armed)
                                    .map(|track| track.id);
                                self.status = Some(if count_in == 0 {
                                    format!("recording {:.1} s of audio", seconds)
                                } else {
                                    format!("count-in {count_in} bar(s), then {seconds:.1} s audio")
                                });
                            }
                            ui.add(
                                egui::DragValue::new(&mut self.audio_record_seconds)
                                    .range(1.0..=60.0)
                                    .suffix(" s"),
                            );
                        });
                        ui.menu_button("Windows", |ui| {
                            if ui.button("Inspector").clicked() {
                                self.inspector_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Library").clicked() {
                                self.library_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Pads").clicked() {
                                self.pads_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Assistant").clicked() {
                                self.assistant.open = true;
                                ui.close_menu();
                            }
                            if ui.button("MIDI map").clicked() {
                                self.midi_map_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Controllers").clicked() {
                                self.controller_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Lyrics").clicked() {
                                self.lyrics_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Markers").clicked() {
                                self.markers_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Theory").clicked() {
                                self.theory_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Lessons").clicked() {
                                self.lessons_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Songs & tutorial").clicked() {
                                self.songs_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Help").clicked() {
                                self.help_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Plugins").clicked() {
                                self.plugins_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Synth patch").clicked() {
                                self.patch_open = true;
                                self.patch_draft = None;
                                ui.close_menu();
                            }
                            if ui.button("Style from clips").clicked() {
                                self.style.clips_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Style from audio").clicked() {
                                self.audio_style.open = true;
                                ui.close_menu();
                            }
                            if ui.button("MIDI monitor").clicked() {
                                self.midi_monitor_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Audio takes").clicked() {
                                self.takes_open = true;
                                ui.close_menu();
                            }
                            if ui.button("XG part mixer").clicked() {
                                self.xg.open = true;
                                ui.close_menu();
                            }
                            if ui.button("Score marks").clicked() {
                                self.marks_open = true;
                                ui.close_menu();
                            }
                            if ui.button("L6max device mixer").clicked() {
                                self.l6max.device_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Roland D-05 editor").clicked() {
                                self.d05.open = true;
                                ui.close_menu();
                            }
                            if ui.button("L6max mixer").clicked() {
                                self.l6max.mixer_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Generate part").clicked() {
                                self.generate_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Style from link").clicked() {
                                self.style.link_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Chords from audio").clicked() {
                                self.audio_chords.open = true;
                                ui.close_menu();
                            }
                            if ui.button("Style file player").clicked() {
                                self.style.file_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Master effects").clicked() {
                                self.master_fx_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Bus effects").clicked() {
                                self.bus_fx_open = true;
                                ui.close_menu();
                            }
                            if ui.button("MIDI out").clicked() {
                                self.midi_out.open = true;
                                ui.close_menu();
                            }
                            if ui.button("PSR-S970 remote").clicked() {
                                self.psr.open = true;
                                ui.close_menu();
                            }
                            if ui.button("Shortcuts").clicked() {
                                self.shortcuts_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Device editor").clicked() {
                                self.device_editor_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Groove library").clicked() {
                                self.groove_open = true;
                                ui.close_menu();
                            }
                            if ui.button("About Lazy Arranger Midi DAW").clicked() {
                                self.about_open = true;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Import L6max").clicked() {
                                self.file_path_input.clear();
                                self.file_mode = Some(FileMode::ImportL6max);
                                ui.close_menu();
                            }
                            if ui.button("Import audio").clicked() {
                                self.file_path_input.clear();
                                self.file_mode = Some(FileMode::ImportAudio);
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Export MIDI").clicked() {
                                self.file_path_input = self
                                    .project_path
                                    .as_ref()
                                    .map(|path| path.with_extension("mid").display().to_string())
                                    .unwrap_or_else(|| String::from("project.mid"));
                                self.file_mode = Some(FileMode::ExportMidi);
                                ui.close_menu();
                            }
                            if ui.button("Export stems").clicked() {
                                self.file_path_input = self
                                    .project_path
                                    .as_ref()
                                    .and_then(|path| path.parent())
                                    .map(|parent| parent.join("stems").display().to_string())
                                    .unwrap_or_else(|| String::from("stems"));
                                self.file_mode = Some(FileMode::ExportStems);
                                ui.close_menu();
                            }
                            if ui.button("Export WAV").clicked() {
                                self.file_path_input = self
                                    .project_path
                                    .as_ref()
                                    .map(|path| path.with_extension("wav").display().to_string())
                                    .unwrap_or_else(|| String::from("project.wav"));
                                self.file_mode = Some(FileMode::ExportWav);
                                ui.close_menu();
                            }
                            if ui.button("Export XML").clicked() {
                                self.file_path_input = self
                                    .project_path
                                    .as_ref()
                                    .map(|path| {
                                        path.with_extension("musicxml").display().to_string()
                                    })
                                    .unwrap_or_else(|| String::from("project.musicxml"));
                                self.file_mode = Some(FileMode::ExportMusicXml);
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Settings").clicked() {
                                self.settings_open = true;
                                ui.close_menu();
                            }
                        });
                    }
                    if !self.recent.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("RECENT")
                                    .size(9.0)
                                    .color(theme::text_dim()),
                            );
                            if ui.small_button("Clear").clicked() {
                                match project::recent::clear(&self.recent_file) {
                                    Ok(()) => {
                                        self.recent.clear();
                                        self.status = Some(String::from("recent list cleared"));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                        });
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
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button("New")
                            .on_hover_text("Start an empty project")
                            .clicked()
                        {
                            self.replace_project(
                                ProjectFile::new(project::song::Song::new("Untitled", 120.0)),
                                "new project",
                            );
                        }
                        if ui.button("Open project…").clicked()
                            && let Some(path) = rfd_pick()
                        {
                            self.open(&path);
                        }
                        if let Some(last) = self.settings.last_project.clone()
                            && std::path::Path::new(&last).is_file()
                        {
                            let name = std::path::Path::new(&last)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or(&last);
                            if ui.button(format!("Reopen {name}")).clicked() {
                                self.open(&std::path::PathBuf::from(&last));
                            }
                        }
                        if ui.button("Save").clicked() {
                            if let Some(path) = self.project_path.clone() {
                                self.save_to(path);
                            } else {
                                self.begin_save_as();
                            }
                        }
                        if ui.button("Save as…").clicked() {
                            self.begin_save_as();
                        }
                    });
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

    pub(super) fn track_row(&mut self, ui: &mut egui::Ui, row: &Row) {
        let selected = self.roll.selected == Some(row.id);
        let colour = theme::track_colour(row.index);
        let mut flags_changed = false;
        let response = egui::Frame::new()
            .fill(if selected {
                theme::raised()
            } else {
                egui::Color32::TRANSPARENT
            })
            .inner_margin(egui::Margin::symmetric(2, 2))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(6.0, 60.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 0.0, colour);
                    let (icon_rect, _) =
                        ui.allocate_exact_size(egui::vec2(28.0, 34.0), egui::Sense::hover());
                    draw_track_icon(ui, icon_rect, &row.name, row.program, row.audio, colour);
                    ui.allocate_ui_with_layout(
                        egui::vec2((ui.available_width() - 62.0).max(40.0), 60.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            let name = if row.name.is_empty() {
                                "untitled"
                            } else {
                                row.name.as_str()
                            };
                            ui.label(egui::RichText::new(name).size(12.0).color(if selected {
                                theme::text()
                            } else {
                                theme::text_muted()
                            }));
                            let detail = if row.name.to_ascii_lowercase().contains("drum") {
                                String::from("Electronic Kit")
                            } else if let Some(program) = row.program {
                                format!("SoundFont · {}", midi_devices::programs::name(program))
                            } else {
                                format!("SoundFont · {} notes", row.notes)
                            };
                            ui.label(
                                egui::RichText::new(detail)
                                    .size(9.0)
                                    .color(theme::text_dim()),
                            );
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .selectable_label(row.armed, "R")
                            .on_hover_text("Record-arm this track")
                            .clicked()
                        {
                            flags_changed = true;
                            let _ = self
                                .editor
                                .set_track_flags(row.id, row.muted, row.soloed, !row.armed);
                        }
                        if ui
                            .selectable_label(row.soloed, "S")
                            .on_hover_text("Solo")
                            .clicked()
                        {
                            flags_changed = true;
                            let _ =
                                self.editor
                                    .set_track_flags(row.id, row.muted, !row.soloed, false);
                        }
                        if ui
                            .selectable_label(row.muted, "M")
                            .on_hover_text("Mute")
                            .clicked()
                        {
                            flags_changed = true;
                            let _ = self
                                .editor
                                .set_track_flags(row.id, !row.muted, row.soloed, false);
                        }
                    });
                });
            })
            .response;
        if !flags_changed && response.interact(egui::Sense::click()).clicked() {
            self.roll.selected = if selected { None } else { Some(row.id) };
        }
    }

    pub(super) fn status_bar(&mut self, context: &Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(theme::STATUS_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    let notes: usize = self
                        .song()
                        .tracks
                        .iter()
                        .map(|track| track.notes.len())
                        .sum();
                    let issues = self.editor.project().validate();
                    let errors = issues
                        .iter()
                        .filter(|issue| issue.severity == project::Severity::Error)
                        .count();
                    let label = if errors == 0 {
                        let midi = self.midi_input.as_ref().map_or_else(
                            || String::from("No MIDI inputs"),
                            |inputs| format!("{} MIDI input(s)", inputs.count()),
                        );
                        format!(
                            "schema v{}  ·  {} track(s)  ·  {notes} note(s)  ·  {}  ·  {midi}",
                            self.editor.project().version,
                            self.song().tracks.len(),
                            audio_engine_label()
                        )
                    } else {
                        format!("{errors} validation error(s)")
                    };
                    ui.label(egui::RichText::new(label).size(10.0).color(if errors == 0 {
                        theme::text_muted()
                    } else {
                        theme::recording()
                    }));
                    if let Some(status) = &self.status {
                        ui.label(
                            egui::RichText::new(status)
                                .size(10.0)
                                .color(theme::text_muted()),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(path) = &self.project_path {
                            ui.label(
                                egui::RichText::new(path.display().to_string())
                                    .size(10.0)
                                    .color(theme::text_dim()),
                            );
                        }
                        if self.view == View::Arrange
                            && self.timeline_mode
                            && ui.small_button("Piano roll").clicked()
                        {
                            self.timeline_mode = false;
                        }
                        ui.label(
                            egui::RichText::new("Space Play · C Metronome · Home Start")
                                .size(9.0)
                                .color(theme::text_dim()),
                        );
                    });
                });
            });
    }

    pub(super) fn right_inspector(&mut self, context: &egui::Context) {
        if self.settings.kids_mode {
            return;
        }
        let track_id = self
            .score
            .track
            .or(self.roll.selected)
            .or_else(|| self.song().tracks.first().map(|track| track.id));
        if self.midi_ports.is_none()
            && let Ok(ports) = midi_devices::devices::list_ports()
        {
            self.midi_ports = Some((ports.inputs, ports.outputs));
        }
        let track = track_id.and_then(|id| {
            self.song()
                .tracks
                .iter()
                .find(|track| track.id == id)
                .cloned()
        });
        let mut meter_change = None;
        let held_chord = self.midi_chord.clone();
        egui::SidePanel::right("right-inspector")
            .resizable(true)
            .default_width(300.0)
            .width_range(240.0..=480.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(context, |ui| {
                let full = ui.max_rect();
                let chat_height = 190.0_f32.min((full.height() * 0.42).max(110.0));
                let top_rect = egui::Rect::from_min_max(
                    full.min,
                    egui::pos2(full.right(), full.bottom() - chat_height - 6.0),
                );
                let mut top_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(top_rect)
                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                );
                egui::ScrollArea::vertical().show(&mut top_ui, |ui| {
                    ui.label(
                        egui::RichText::new("TRACK")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    let Some(track) = &track else {
                        ui.label("No track");
                        return;
                    };
                    if self.inspector_name_track != Some(track.id) {
                        self.inspector_name_track = Some(track.id);
                        self.inspector_name = track.name.clone();
                    }
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.inspector_name)
                                .desired_width(150.0)
                                .font(egui::TextStyle::Heading),
                        );
                        if response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            let name = self.inspector_name.trim().to_string();
                            match self.editor.rename_track(track.id, &name) {
                                Ok(()) => self.status = Some(format!("track renamed to {name:?}")),
                                Err(error) => self.status = Some(error),
                            }
                        }
                        let channel = track
                            .notes
                            .first()
                            .map_or(1, |note| note.channel.one_based());
                        egui::ComboBox::from_id_salt("track-channel")
                            .selected_text(format!("Ch {channel}"))
                            .width(64.0)
                            .show_ui(ui, |ui| {
                                for value in 1..=16 {
                                    if ui
                                        .selectable_label(channel == value, format!("Ch {value}"))
                                        .clicked()
                                        && let Ok(channel) =
                                            music_core::Channel::from_one_based(value)
                                        && let Err(error) =
                                            self.editor.set_track_channel(track.id, channel)
                                    {
                                        self.status = Some(error);
                                    }
                                }
                            });
                    });
                    let instrument = track.soundfont.as_ref().map_or_else(
                        || {
                            track.program.map_or_else(
                                || String::from("Built-in SoundFont"),
                                |program| format!("Program {program}"),
                            )
                        },
                        |font| format!("{}:{} {}", font.bank, font.program, font.path),
                    );
                    ui.label(
                        egui::RichText::new(instrument)
                            .size(10.0)
                            .color(theme::text_muted()),
                    );
                    if track.soundfont.is_none() {
                        let selected = track
                            .program
                            .map_or("Built-in SoundFont", midi_devices::programs::name);
                        egui::ComboBox::from_id_salt("track-program")
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(track.program.is_none(), "Built-in SoundFont")
                                    .clicked()
                                {
                                    let _ = self.editor.set_track_program(track.id, None);
                                }
                                for (value, name) in
                                    midi_devices::programs::GM_NAMES.iter().enumerate()
                                {
                                    let program = u8::try_from(value).unwrap_or(0);
                                    if ui
                                        .selectable_label(track.program == Some(program), *name)
                                        .clicked()
                                    {
                                        let _ =
                                            self.editor.set_track_program(track.id, Some(program));
                                    }
                                }
                            });
                    }
                    if track.soundfont.is_none() {
                        ui.label(
                            egui::RichText::new("General MIDI · 128 sounds")
                                .size(9.0)
                                .color(theme::text_dim()),
                        );
                    }
                    ui.horizontal(|ui| {
                        let mut muted = track.muted;
                        let mut soloed = track.soloed;
                        let mut armed = track.armed;
                        let changed = ui.checkbox(&mut muted, "M").changed()
                            | ui.checkbox(&mut soloed, "S").changed()
                            | ui.checkbox(&mut armed, "R").changed();
                        if changed
                            && let Err(error) =
                                self.editor.set_track_flags(track.id, muted, soloed, armed)
                        {
                            self.status = Some(error);
                        }
                    });
                    if ui.button("Effects…").clicked() {
                        self.view = View::Mix;
                    }
                    if ui.button("SoundFont…").clicked() {
                        self.view = View::Mix;
                        self.status =
                            Some(String::from("SoundFont controls are in the Mixer view"));
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new("SONG")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.song_name_input)
                                .desired_width(160.0),
                        )
                        .lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        let name = self.song_name_input.trim().to_string();
                        match self.editor.rename_song(&name) {
                            Ok(()) => self.status = Some(format!("song renamed to {name:?}")),
                            Err(error) => self.status = Some(error),
                        }
                    }
                    let key = score::spelling::detect_key(self.song());
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} {}",
                            if key.minor {
                                key.tonic.name_flat()
                            } else {
                                key.tonic.name_sharp()
                            },
                            if key.minor { "minor" } else { "major" }
                        ));
                        if ui.small_button("Find").on_hover_text("Find songs").clicked() {
                            self.songs_open = true;
                        }
                    });
                    let meter = self.song().time_signature;
                    ui.horizontal(|ui| {
                        ui.label("Meter");
                        egui::ComboBox::from_id_salt("song-meter")
                            .selected_text(format!(
                                "{}/{}",
                                meter.numerator, meter.denominator
                            ))
                            .show_ui(ui, |ui| {
                                for (numerator, denominator) in
                                    [(4_u8, 4_u8), (3, 4), (2, 4), (6, 8), (5, 4), (7, 8)]
                                {
                                    if ui
                                        .selectable_label(
                                            meter.numerator == numerator
                                                && meter.denominator == denominator,
                                            format!("{numerator}/{denominator}"),
                                        )
                                        .clicked()
                                    {
                                        meter_change = Some((numerator, denominator));
                                    }
                                }
                            });
                    });
                    ui.separator();
                    ui.label(
                        egui::RichText::new("METRONOME")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.settings.metronome, "Click on");
                        let beats = usize::from(self.song().time_signature.numerator.clamp(1, 12));
                        let (dots, _) = ui.allocate_exact_size(
                            egui::vec2(6.0 + beats as f32 * 13.0, 14.0),
                            egui::Sense::hover(),
                        );
                        for beat in 0..beats {
                            let centre = egui::pos2(
                                dots.left() + 6.0 + beat as f32 * 13.0,
                                dots.center().y,
                            );
                            ui.painter().circle_filled(
                                centre,
                                4.0,
                                if beat == 0 {
                                    theme::accent()
                                } else {
                                    theme::line_strong()
                                },
                            );
                        }
                    });
                    ui.add(
                        egui::Slider::new(&mut self.settings.metronome_accent, 0.0..=1.0)
                            .text("Accent"),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new("AUDIO OUTPUT")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    if let Some(player) = &self.player {
                        let buffer = player.buffer_frames().map_or_else(
                            || String::from("device buffer"),
                            |frames| {
                                format!(
                                    "{frames} frames · {:.1} ms",
                                    player.buffer_latency_ms().unwrap_or_default()
                                )
                            },
                        );
                        ui.label(format!(
                            "{} · {} Hz · {buffer}",
                            self.settings
                                .audio_output
                                .as_deref()
                                .unwrap_or("Default output"),
                            player.sample_rate()
                        ));
                    } else {
                        ui.label(
                            self.settings
                                .audio_output
                                .clone()
                                .unwrap_or_else(audio_engine_label),
                        );
                    }
                    if ui.button("Audio and MIDI settings").clicked() {
                        self.settings_open = true;
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new("MIDI INPUTS")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    let inputs = self
                        .midi_ports
                        .as_ref()
                        .map(|(inputs, _)| inputs.clone())
                        .unwrap_or_default();
                    if inputs.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "No MIDI inputs found. Plug in a keyboard, then open Audio and MIDI settings.",
                            )
                            .size(10.0)
                            .color(theme::text_dim()),
                        );
                    }
                    for input in inputs.iter().take(4) {
                        ui.label(
                            egui::RichText::new(input)
                                .size(10.0)
                                .color(theme::text_muted()),
                        );
                    }
                    if let Some(chord) = &held_chord {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Held chord: {chord}"))
                                    .size(11.0)
                                    .color(theme::accent()),
                            );
                            if ui
                                .small_button("Write at playhead")
                                .on_hover_text("Add this chord to the song at the playhead")
                                .clicked()
                            {
                                match self.editor.set_chord(self.position.max(0), chord) {
                                    Ok(()) => {
                                        self.status = Some(format!(
                                            "wrote chord {chord} at {}",
                                            self.position
                                        ));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                        });
                    }
                });
                let chat_rect = egui::Rect::from_min_max(
                    egui::pos2(full.left(), full.bottom() - chat_height),
                    full.max,
                );
                let mut chat_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(chat_rect)
                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                );
                self.assistant_inline(&mut chat_ui, chat_height);
            });
        if let Some((numerator, denominator)) = meter_change {
            match self.editor.set_time_signature(numerator, denominator) {
                Ok(()) => {
                    self.status = Some(format!("meter {numerator}/{denominator}"));
                }
                Err(error) => self.status = Some(error),
            }
        }
    }

    /// The clip grid: scenes as rows, tracks as columns, launch and record.
    pub(super) fn grid_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading(format!(
            "Clip grid · {} scene(s) of two bars",
            self.editor.grid(2).rows.len()
        ));
        let grid = self.editor.grid(2);
        let bar = self.song().time_signature.bar_ticks().max(1);
        let track_ids = grid.tracks.clone();
        let track_names: Vec<String> = track_ids
            .iter()
            .map(|track| {
                self.song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == *track)
                    .map_or_else(|| format!("track {track}"), |entry| entry.name.clone())
            })
            .collect();
        enum SlotAction {
            Clear,
            Length(Tick),
            Copy(Tick),
        }
        let mut slot_action: Option<(usize, TrackId, SlotAction)> = None;
        egui::Grid::new("clip-grid")
            .num_columns(2 + track_ids.len())
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("");
                ui.label(egui::RichText::new("Scene").strong());
                for name in &track_names {
                    ui.label(egui::RichText::new(name).strong());
                }
                ui.end_row();
                for (scene_index, row) in grid.rows.iter().enumerate() {
                    if ui
                        .small_button("Rename")
                        .on_hover_text("Rename scene")
                        .clicked()
                    {
                        self.rename_scene = Some(scene_index);
                        self.rename_scene_input = row.scene.name.clone();
                    }
                    let playing = self.launcher.current == Some(scene_index);
                    let waiting = self.launcher.waiting == Some(scene_index);
                    let label = if playing {
                        format!("▶ {}", row.scene.name)
                    } else if waiting {
                        format!("… {}", row.scene.name)
                    } else {
                        row.scene.name.clone()
                    };
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
                        let Some(track) = track_ids.get(column).copied() else {
                            ui.label("");
                            continue;
                        };
                        let name = slot.clip.map_or_else(
                            || String::from("—"),
                            |clip| {
                                self.song()
                                    .tracks
                                    .iter()
                                    .find(|entry| entry.id == track)
                                    .and_then(|entry| {
                                        entry.clips.iter().find(|entry| entry.id == clip)
                                    })
                                    .map_or_else(
                                        || format!("clip {clip}"),
                                        |entry| entry.name.clone(),
                                    )
                            },
                        );
                        ui.button(name).context_menu(|ui| {
                            if ui.button("Clear").clicked() {
                                slot_action = Some((scene_index, track, SlotAction::Clear));
                                ui.close_menu();
                            }
                            if ui.button("Length: 1 bar").clicked() {
                                slot_action = Some((scene_index, track, SlotAction::Length(bar)));
                                ui.close_menu();
                            }
                            if ui.button("Length: 2 bars").clicked() {
                                slot_action =
                                    Some((scene_index, track, SlotAction::Length(bar * 2)));
                                ui.close_menu();
                            }
                            if ui.button("Copy to playhead").clicked() {
                                slot_action = Some((
                                    scene_index,
                                    track,
                                    SlotAction::Copy(self.position.max(0)),
                                ));
                                ui.close_menu();
                            }
                        });
                    }
                    ui.end_row();
                }
            });
        if let Some((scene, track, action)) = slot_action {
            let result = match action {
                SlotAction::Clear => self.editor.clear_slot(scene, track),
                SlotAction::Length(length) => self.editor.set_slot_length(scene, track, length),
                SlotAction::Copy(target) => self.editor.copy_slot(scene, track, target).map(|_| ()),
            };
            if let Err(error) = result {
                self.status = Some(error);
            }
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
            .color(theme::text_dim()),
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

    pub(super) fn windows(&mut self, context: &egui::Context) {
        self.about_window(context);
        self.groove_window(context);
        self.device_editor_window(context);
        if self.settings_open {
            let mut open = self.settings_open;
            egui::Window::new("Settings")
                .open(&mut open)
                .collapsible(false)
                .show(context, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Language");
                        egui::ComboBox::from_id_salt("settings-language")
                            .selected_text(if self.settings.language == "system" {
                                "System default"
                            } else {
                                &self.settings.language
                            })
                            .show_ui(ui, |ui| {
                                for tag in [
                                    "system", "en", "da", "fr", "vi", "de", "es", "it", "pt",
                                ] {
                                    ui.selectable_value(
                                        &mut self.settings.language,
                                        tag.to_string(),
                                        if tag == "system" { "System default" } else { tag },
                                    );
                                }
                            });
                        ui.label(
                            egui::RichText::new(
                                "English, Danish, French, Vietnamese, German, Spanish, Italian, Portuguese or System default",
                            )
                            .size(9.0)
                            .color(theme::text_dim()),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Device profile");
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
                                            self.settings.device_profile.as_ref()
                                                == Some(&profile.id),
                                            &profile.name,
                                        )
                                        .clicked()
                                    {
                                        self.settings.device_profile =
                                            Some(profile.id.clone());
                                    }
                                }
                            });
                        if ui.button("Reload").clicked() {
                            self.device_profiles = load_device_profiles();
                        }
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
                    ui.horizontal(|ui| {
                        if ui.button("Find audio inputs").clicked() {
                            let inputs = audio::record::input_devices();
                            self.status = Some(format!("{} audio input(s)", inputs.len()));
                            self.audio_inputs = Some(inputs);
                        }
                        if ui
                            .selectable_label(
                                self.settings.audio_input.is_none(),
                                "Default input",
                            )
                            .clicked()
                        {
                            self.settings.audio_input = None;
                            let _ = self.settings.save(&crate::settings::Settings::path());
                        }
                        if let Some(inputs) = &self.audio_inputs {
                            for name in inputs {
                                if ui
                                    .selectable_label(
                                        self.settings.audio_input.as_deref() == Some(name.as_str()),
                                        name,
                                    )
                                    .clicked()
                                {
                                    self.settings.audio_input = Some(name.clone());
                                    let _ = self.settings.save(&crate::settings::Settings::path());
                                    self.status = Some(format!("audio input: {name}"));
                                }
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Find audio outputs").clicked() {
                            let outputs = audio::player::output_devices();
                            self.status = Some(format!("{} audio output(s)", outputs.len()));
                            self.audio_outputs = Some(outputs);
                        }
                        if ui
                            .selectable_label(
                                self.settings.audio_output.is_none(),
                                "Default output",
                            )
                            .clicked()
                        {
                            self.settings.audio_output = None;
                            let _ = self.settings.save(&crate::settings::Settings::path());
                        }
                        if let Some(outputs) = &self.audio_outputs {
                            egui::ComboBox::from_id_salt("audio-output")
                                .selected_text(
                                    self.settings
                                        .audio_output
                                        .as_deref()
                                        .unwrap_or("Choose output"),
                                )
                                .show_ui(ui, |ui| {
                                    for name in outputs {
                                        if ui
                                            .selectable_label(
                                                self.settings.audio_output.as_deref()
                                                    == Some(name.as_str()),
                                                name,
                                            )
                                            .clicked()
                                        {
                                            self.settings.audio_output = Some(name.clone());
                                            let _ = self.settings.save(
                                                &crate::settings::Settings::path(),
                                            );
                                            self.status =
                                                Some(format!("audio output: {name}"));
                                        }
                                    }
                                });
                        }
                        ui.label("buffer");
                        let mut frames = self.settings.audio_buffer_frames;
                        egui::ComboBox::from_id_salt("audio-buffer")
                            .selected_text(if frames == 0 {
                                String::from("Default")
                            } else {
                                format!("{frames}")
                            })
                            .show_ui(ui, |ui| {
                                for option in [0_u32, 128, 256, 512, 1024] {
                                    let label = if option == 0 {
                                        String::from("Default")
                                    } else {
                                        format!("{option}")
                                    };
                                    if ui
                                        .selectable_label(frames == option, label)
                                        .clicked()
                                    {
                                        frames = option;
                                    }
                                }
                            });
                        if frames != self.settings.audio_buffer_frames {
                            self.settings.audio_buffer_frames = frames;
                            let _ = self.settings.save(&crate::settings::Settings::path());
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Input channel");
                        egui::ComboBox::from_id_salt("audio-input-channel")
                            .selected_text(if self.settings.audio_input_channel == 0 {
                                String::from("Mix")
                            } else {
                                format!("CH{}", self.settings.audio_input_channel)
                            })
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        self.settings.audio_input_channel == 0,
                                        "Mix",
                                    )
                                    .clicked()
                                {
                                    self.settings.audio_input_channel = 0;
                                }
                                for channel in 1..=16 {
                                    if ui
                                        .selectable_label(
                                            self.settings.audio_input_channel == channel,
                                            format!("CH{channel}"),
                                        )
                                        .clicked()
                                    {
                                        self.settings.audio_input_channel = channel;
                                    }
                                }
                            });
                    });
                    if ui.button("Check for updates").clicked() {
                        match crate::update::fetch_release() {
                            Ok(release) => {
                                let current = env!("CARGO_PKG_VERSION");
                                if crate::update::should_offer(
                                    &release.tag,
                                    current,
                                    &self.settings.skipped_update,
                                ) {
                                    self.status = Some(crate::update::describe_release(
                                        current,
                                        &release.tag,
                                        &release.url,
                                    ));
                                    self.available_update = Some(release);
                                } else if release.tag == self.settings.skipped_update {
                                    self.status = Some(format!(
                                        "Update {} skipped in Settings",
                                        release.tag
                                    ));
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
                    if let Some(release) = self.available_update.clone() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Release {}", release.tag));
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
                                    let _ = sender.send(crate::update::download_asset(
                                        &asset, &directory,
                                    ));
                                });
                            }
                            if let Some(path) = &self.downloaded_update
                                && ui.button("Open download").clicked()
                            {
                                crate::update::open_path(path);
                            }
                            if ui.button("Skip this version").clicked() {
                                self.settings.skipped_update = release.tag.clone();
                                let _ = self.settings.save(&crate::settings::Settings::path());
                                self.available_update = None;
                                self.status = Some(format!("Update {} skipped", release.tag));
                            }
                        });
                    }
                    if ui.button("Show device profiles folder").clicked() {
                        let folder = crate::settings::Settings::path()
                            .parent()
                            .map(|path| path.join("profiles"))
                            .unwrap_or_else(|| std::path::PathBuf::from("profiles"));
                        if let Err(error) = std::fs::create_dir_all(&folder) {
                            self.status = Some(format!("{}: {error}", folder.display()));
                        } else {
                            self.status = Some(format!("device profiles: {}", folder.display()));
                            #[cfg(target_os = "macos")]
                            {
                                let _ = std::process::Command::new("open")
                                    .arg(&folder)
                                    .spawn();
                            }
                        }
                    }
                    ui.checkbox(&mut self.settings.metronome, "metronome with playback");
                    ui.checkbox(
                        &mut self.settings.record_chords,
                        "detect chords after MIDI recording",
                    );
                    ui.checkbox(
                        &mut self.settings.monitor,
                        "monitor input through output (feedback risk)",
                    );
                    ui.checkbox(
                        &mut self.settings.record_quantise,
                        "quantise recorded notes to 1/16",
                    );
                    ui.checkbox(&mut self.settings.reduced_motion, "reduced motion");
                    if ui
                        .checkbox(&mut self.settings.kids_mode, "Kids Mode")
                        .changed()
                    {
                        self.view = if self.settings.kids_mode {
                            View::Perform
                        } else {
                            View::Arrange
                        };
                    }
                    if self.settings.kids_mode {
                        ui.add(
                            egui::Slider::new(&mut self.settings.kids_pause_seconds, 0.5..=5.0)
                                .text("Kids pause (s)"),
                        );
                        ui.checkbox(
                            &mut self.settings.kids_keys_play,
                            "Keys also play their own notes",
                        );
                    }
                    let mut light_theme = self.settings.light_theme;
                    if ui.checkbox(&mut light_theme, "Light theme").changed() {
                        self.settings.light_theme = light_theme;
                        theme::set_light(light_theme);
                        theme::refresh(ui.ctx());
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new("AI endpoint (OpenAI-compatible)")
                            .size(11.0)
                            .color(theme::text_muted()),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.ai_url)
                            .hint_text("http://localhost:11434/v1/chat/completions")
                            .desired_width(340.0),
                    );
                    ui.horizontal(|ui| {
                        ui.label("model");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings.ai_model)
                                .desired_width(180.0),
                        );
                        ui.label("key");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings.ai_api_key)
                                .password(true)
                                .desired_width(140.0),
                        );
                    });
                    ui.label(
                        egui::RichText::new(
                            "The key is stored in plain text in the settings file. Leave the URL empty to stay local.",
                        )
                        .size(9.0)
                        .color(theme::text_dim()),
                    );
                    ui.label(
                        egui::RichText::new("Local assistant command (use {prompt})")
                            .size(11.0)
                            .color(theme::text_muted()),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.assistant_command)
                            .hint_text("codex exec {prompt}")
                            .desired_width(340.0),
                    );
                    ui.label(
                        egui::RichText::new("Speak preset or command (use {text})")
                            .size(11.0)
                            .color(theme::text_muted()),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for preset in [
                            crate::speech::TtsPreset::Auto,
                            crate::speech::TtsPreset::System,
                            crate::speech::TtsPreset::Kokoro,
                            crate::speech::TtsPreset::Kitten,
                            crate::speech::TtsPreset::Piper,
                        ] {
                            if ui.small_button(preset.label()).clicked() {
                                self.settings.speak_command =
                                    String::from(preset.setting_value());
                            }
                        }
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.speak_command)
                            .hint_text("auto | kokoro | kitten | piper | custom {text}")
                            .desired_width(340.0),
                    );
                    ui.label(
                        egui::RichText::new("Dictate STT preset or command (use {file})")
                            .size(11.0)
                            .color(theme::text_muted()),
                    );
                    ui.horizontal_wrapped(|ui| {
                        let dir = crate::dictate::models_dir();
                        for preset in [
                            crate::dictate::SttPreset::Auto,
                            crate::dictate::SttPreset::WhisperCpp,
                            crate::dictate::SttPreset::SherpaMoonshine,
                        ] {
                            if ui.small_button(preset.label()).clicked() {
                                self.settings.speech_command = preset.template(&dir);
                            }
                        }
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.speech_command)
                            .hint_text("auto | whisper-cli -m model.bin -f {file} -nt")
                            .desired_width(340.0),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "Offline models dir: {}",
                            crate::dictate::models_dir().display()
                        ))
                        .size(9.5)
                        .color(theme::text_dim()),
                    );
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
            let mut undo_keys = self.settings.undo_keys.clone();
            let mut redo_keys = self.settings.redo_keys.clone();
            let mut save_keys = self.settings.save_keys.clone();
            let mut save_as_keys = self.settings.save_as_keys.clone();
            let mut open_keys = self.settings.open_keys.clone();
            let mut play_keys = self.settings.play_keys.clone();
            let mut save_bindings = false;
            egui::Window::new("Shortcuts")
                .open(&mut open)
                .collapsible(false)
                .default_width(420.0)
                .show(context, |ui| {
                    ui.label(egui::RichText::new("Editable bindings").strong());
                    for (label, value) in [
                        ("Undo", &mut undo_keys),
                        ("Redo", &mut redo_keys),
                        ("Save", &mut save_keys),
                        ("Save as", &mut save_as_keys),
                        ("Open", &mut open_keys),
                        ("Play/pause", &mut play_keys),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            ui.text_edit_singleline(value);
                        });
                    }
                    let conflicts = crate::settings::binding_conflicts(&[
                        ("Undo", &undo_keys),
                        ("Redo", &redo_keys),
                        ("Save", &save_keys),
                        ("Save as", &save_as_keys),
                        ("Open", &open_keys),
                        ("Play/pause", &play_keys),
                    ]);
                    if conflicts.is_empty() {
                        ui.label(
                            egui::RichText::new("No duplicate bindings.")
                                .size(10.0)
                                .color(theme::text_dim()),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(format!("Conflict: {}", conflicts.join(", ")))
                                .size(10.0)
                                .color(theme::recording()),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Save bindings").clicked() {
                            save_bindings = true;
                        }
                        if ui.button("Reset defaults").clicked() {
                            let defaults = crate::settings::Settings::default();
                            undo_keys = defaults.undo_keys;
                            redo_keys = defaults.redo_keys;
                            save_keys = defaults.save_keys;
                            save_as_keys = defaults.save_as_keys;
                            open_keys = defaults.open_keys;
                            play_keys = defaults.play_keys;
                        }
                    });
                    ui.label(
                        egui::RichText::new(
                            "Use cmd/ctrl/shift/alt plus a key, for example cmd+shift+z.",
                        )
                        .size(10.0)
                        .color(theme::text_dim()),
                    );
                    ui.separator();
                    ui.label(egui::RichText::new("Other shortcuts").strong());
                    for shortcut in crate::settings::SHORTCUTS {
                        if matches!(
                            shortcut.action,
                            "toggle playback" | "save project" | "undo" | "redo"
                        ) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(shortcut.keys).monospace().size(10.0));
                            ui.label(shortcut.action);
                            ui.label(
                                egui::RichText::new(shortcut.context)
                                    .size(9.0)
                                    .color(theme::text_dim()),
                            );
                        });
                    }
                });
            if save_bindings {
                let values = [
                    &undo_keys,
                    &redo_keys,
                    &save_keys,
                    &save_as_keys,
                    &open_keys,
                    &play_keys,
                ];
                let labels = [
                    ("Undo", undo_keys.as_str()),
                    ("Redo", redo_keys.as_str()),
                    ("Save", save_keys.as_str()),
                    ("Save as", save_as_keys.as_str()),
                    ("Open", open_keys.as_str()),
                    ("Play/pause", play_keys.as_str()),
                ];
                if values
                    .iter()
                    .any(|value| crate::settings::parse_shortcut(value).is_none())
                {
                    self.status = Some(String::from("invalid shortcut binding"));
                } else if !crate::settings::binding_conflicts(&labels).is_empty() {
                    self.status = Some(String::from("shortcut conflict; change one binding"));
                } else {
                    self.settings.undo_keys = undo_keys;
                    self.settings.redo_keys = redo_keys;
                    self.settings.save_keys = save_keys;
                    self.settings.save_as_keys = save_as_keys;
                    self.settings.open_keys = open_keys;
                    self.settings.play_keys = play_keys;
                    match self.settings.save(&crate::settings::Settings::path()) {
                        Ok(()) => self.status = Some(String::from("shortcuts saved")),
                        Err(error) => self.status = Some(error),
                    }
                }
            }
            if !open {
                self.shortcuts_open = false;
            }
        }
    }

    pub(super) fn inspector_window(&mut self, context: &egui::Context) {
        if !self.inspector_open {
            return;
        }
        let track_id = self
            .roll
            .selected
            .or_else(|| self.song().tracks.first().map(|track| track.id));
        let Some(track_id) = track_id else {
            self.inspector_open = false;
            return;
        };
        let info = self
            .song()
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .map(|track| {
                (
                    track.name.clone(),
                    track.volume,
                    track.pan,
                    track.muted,
                    track.soloed,
                    track.armed,
                    track.program,
                    track.bank,
                    track.notes.len(),
                    track.clips.len(),
                    track.audio.clone(),
                    track.soundfont.clone(),
                    track.plugins.len(),
                    track.automation.len(),
                    track.bus,
                    track.send,
                    track.send_level,
                )
            });
        let Some((
            name,
            mut volume,
            mut pan,
            mut muted,
            mut soloed,
            mut armed,
            program,
            bank,
            notes,
            clips,
            audio,
            soundfont,
            plugins,
            automation,
            mut bus,
            mut send,
            mut send_level,
        )) = info
        else {
            self.inspector_open = false;
            return;
        };
        if self.inspector_track != Some(track_id) {
            self.inspector_track = Some(track_id);
            self.inspector_name = name.clone();
        }
        let mut open = self.inspector_open;
        let mut rename = false;
        egui::Window::new("Inspector")
            .open(&mut open)
            .default_width(320.0)
            .show(context, |ui| {
                let tracks: Vec<(TrackId, String)> = self
                    .song()
                    .tracks
                    .iter()
                    .map(|track| (track.id, track.name.clone()))
                    .collect();
                let mut chosen = track_id;
                egui::ComboBox::from_id_salt("inspector-track")
                    .selected_text(&self.inspector_name)
                    .show_ui(ui, |ui| {
                        for (id, label) in &tracks {
                            ui.selectable_value(&mut chosen, *id, label);
                        }
                    });
                if chosen != track_id {
                    self.roll.selected = Some(chosen);
                    self.inspector_track = None;
                }
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.inspector_name);
                    if ui.button("Rename").clicked() {
                        rename = true;
                    }
                });
                if rename {
                    match self.editor.rename_track(track_id, &self.inspector_name) {
                        Ok(()) => self.status = Some(String::from("track renamed")),
                        Err(error) => self.status = Some(error),
                    }
                }
                let language = crate::strings::Language::parse(&self.settings.language);
                let nouns = crate::strings::nouns(language);
                let counts = format!(
                    "{} · {} · {}",
                    crate::strings::count(language, notes, nouns.note.0, nouns.note.1),
                    crate::strings::count(language, clips, nouns.clip.0, nouns.clip.1),
                    crate::strings::count(language, automation, nouns.lane.0, nouns.lane.1)
                );
                ui.label(
                    egui::RichText::new(counts)
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                let volume_changed = ui
                    .add(
                        egui::Slider::new(&mut volume, 0..=127)
                            .text("volume")
                            .fixed_decimals(0),
                    )
                    .changed();
                let pan_changed = ui
                    .add(
                        egui::Slider::new(&mut pan, 0..=127)
                            .text("pan")
                            .fixed_decimals(0),
                    )
                    .changed();
                if (volume_changed || pan_changed)
                    && let Err(error) = self.editor.set_track_gain(track_id, volume, pan)
                {
                    self.status = Some(error);
                }
                ui.horizontal(|ui| {
                    let changed = ui.checkbox(&mut muted, "Mute").changed()
                        | ui.checkbox(&mut soloed, "Solo").changed()
                        | ui.checkbox(&mut armed, "Arm").changed();
                    if changed
                        && let Err(error) =
                            self.editor.set_track_flags(track_id, muted, soloed, armed)
                    {
                        self.status = Some(error);
                    }
                });
                ui.horizontal(|ui| {
                    let mut enabled = program.is_some();
                    let mut number = i32::from(program.unwrap_or(0));
                    let toggled = ui.checkbox(&mut enabled, "Program").changed();
                    let changed = ui
                        .add(egui::DragValue::new(&mut number).range(0..=127))
                        .changed();
                    if toggled || changed {
                        let value = enabled.then_some(number as u8);
                        if let Err(error) = self.editor.set_track_program(track_id, value) {
                            self.status = Some(error);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let mut enabled = bank.is_some();
                    let mut msb = i32::from(bank.map_or(0, |bank| bank.msb));
                    let mut lsb = i32::from(bank.map_or(0, |bank| bank.lsb));
                    let toggled = ui.checkbox(&mut enabled, "Bank").changed();
                    let changed = ui
                        .add(egui::DragValue::new(&mut msb).range(0..=127))
                        .changed()
                        | ui.add(egui::DragValue::new(&mut lsb).range(0..=127))
                            .changed();
                    if toggled || changed {
                        let value = enabled.then_some(project::song::BankSelect {
                            msb: u8::try_from(msb).unwrap_or(0),
                            lsb: u8::try_from(lsb).unwrap_or(0),
                        });
                        if let Err(error) = self.editor.set_track_bank(track_id, value) {
                            self.status = Some(error);
                        }
                    }
                });
                ui.separator();
                ui.label(
                    egui::RichText::new("Tempo changes")
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                for change in self.song().tempo_map.changes().iter().take(5) {
                    ui.label(
                        egui::RichText::new(format!("{} bpm at tick {}", change.bpm, change.tick))
                            .size(10.0)
                            .color(theme::text_muted()),
                    );
                }
                let meter = self.song().time_signature;
                let mut numerator = meter.numerator as u32;
                let mut denominator = meter.denominator as u32;
                ui.horizontal(|ui| {
                    ui.label("meter");
                    ui.add(egui::DragValue::new(&mut numerator).range(1..=32));
                    ui.label("/");
                    egui::ComboBox::from_id_salt("meter-denominator")
                        .selected_text(denominator.to_string())
                        .show_ui(ui, |ui| {
                            for choice in [1_u32, 2, 4, 8, 16, 32] {
                                ui.selectable_value(&mut denominator, choice, choice.to_string());
                            }
                        });
                    if ui.button("Set meter").clicked() {
                        match self.editor.set_time_signature(
                            u8::try_from(numerator).unwrap_or(4),
                            u8::try_from(denominator).unwrap_or(4),
                        ) {
                            Ok(()) => {
                                self.status = Some(format!("meter {numerator}/{denominator}"));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.tempo_input)
                            .range(20.0..=300.0)
                            .suffix(" bpm"),
                    );
                    if ui.small_button("Current").clicked() {
                        self.tempo_input = self.song().tempo_map.bpm_at(self.position);
                    }
                    if ui.button("Set at playhead").clicked() {
                        match self
                            .editor
                            .set_tempo_at(self.position.max(0), self.tempo_input)
                        {
                            Ok(()) => {
                                self.status = Some(format!(
                                    "tempo {:.1} bpm at {}",
                                    self.tempo_input, self.position
                                ));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
                let bus_options: Vec<(u8, String)> = self
                    .song()
                    .buses
                    .iter()
                    .map(|bus| (bus.id, bus.name.clone()))
                    .collect();
                let mut routing_changed = false;
                ui.horizontal(|ui| {
                    ui.label("Bus");
                    egui::ComboBox::from_id_salt("track-bus")
                        .selected_text(
                            bus.and_then(|id| {
                                bus_options
                                    .iter()
                                    .find(|(option, _)| *option == id)
                                    .map(|(_, name)| name.as_str())
                            })
                            .unwrap_or("None"),
                        )
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(bus.is_none(), "None").clicked() {
                                bus = None;
                                routing_changed = true;
                            }
                            for (id, name) in &bus_options {
                                if ui.selectable_label(bus == Some(*id), name).clicked() {
                                    bus = Some(*id);
                                    routing_changed = true;
                                }
                            }
                        });
                    ui.label("Send");
                    egui::ComboBox::from_id_salt("track-send")
                        .selected_text(
                            send.and_then(|id| {
                                bus_options
                                    .iter()
                                    .find(|(option, _)| *option == id)
                                    .map(|(_, name)| name.as_str())
                            })
                            .unwrap_or("None"),
                        )
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(send.is_none(), "None").clicked() {
                                send = None;
                                routing_changed = true;
                            }
                            for (id, name) in &bus_options {
                                if ui.selectable_label(send == Some(*id), name).clicked() {
                                    send = Some(*id);
                                    routing_changed = true;
                                }
                            }
                        });
                    if ui
                        .add(egui::Slider::new(&mut send_level, 0..=127).text("send level"))
                        .changed()
                    {
                        routing_changed = true;
                    }
                });
                if routing_changed {
                    match self
                        .editor
                        .set_track_routing(track_id, bus, send, send_level)
                    {
                        Ok(()) => self.status = Some(String::from("routing updated")),
                        Err(error) => self.status = Some(error),
                    }
                }
                if let Some(reference) = &audio {
                    ui.label(
                        egui::RichText::new(format!(
                            "audio {} (fades {} / {})",
                            reference.path, reference.fade_in, reference.fade_out
                        ))
                        .size(10.0)
                        .color(theme::text_muted()),
                    );
                }
                if let Some(font) = &soundfont {
                    ui.label(
                        egui::RichText::new(format!(
                            "soundfont {}:{} {}",
                            font.bank, font.program, font.path
                        ))
                        .size(10.0)
                        .color(theme::text_muted()),
                    );
                }
                ui.label(
                    egui::RichText::new(plural(plugins, "plug-in slot"))
                        .size(10.0)
                        .color(theme::text_muted()),
                );
            });
        self.inspector_open = open;
    }

    /// The central view and the edits it collected this frame.
    pub(super) fn central_panel(&mut self, context: &Context) {
        let kids = self.settings.kids_mode;
        if kids {
            self.lesson
                .sync(self.song().chords.iter().map(|event| event.chord).collect());
        }
        let lesson = self.lesson.clone();
        let pads = music_core::roman::pads(self.key);
        let waveform = self.waveform.clone();
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::timeline()))
            .show(context, |ui| {
                let view = self.view;
                let position = self.position;
                let mut clicked = None;
                let mut automation_edit: Option<(TrackId, project::song::Automation)> = None;
                let mut audio_trim: Option<(TrackId, Tick)> = None;
                let mut audio_move: Option<(TrackId, Tick)> = None;
                let mut audio_split: Option<(TrackId, Tick)> = None;
                let mut audio_length: Option<(TrackId, Tick)> = None;
                let mut audio_fades: Option<(TrackId, Tick, Tick)> = None;
                let mut audio_stretch: Option<(TrackId, bool)> = None;
                let mut audio_normalize: Option<TrackId> = None;
                let mut audio_gain: Option<(TrackId, i16)> = None;
                let mut audio_remove: Option<TrackId> = None;
                let mut score_status: Option<String> = None;
                let entry_enabled_flag = self.score.entry;
                let mut perform_chord: Option<music_core::Chord> = None;
                let mut lesson_next = false;
                let mut lesson_reset = false;
                let mut kids_load = false;
                let mut scroll_edit: Option<crate::piano_roll::RollEdit> = None;
                let mut velocity_edit: Option<(TrackId, project::NoteId, u8)> = None;
                let mut preview_note: Option<u8> = None;
                let mut chord_click: Option<Tick> = None;
                let mut lyric_click: Option<Tick> = None;
                let mut seek: Option<Tick> = None;
                let timeline_mode = self.timeline_mode;
                let timeline_zoom = self.timeline_zoom;
                let Self { editor, roll, .. } = self;
                let project = editor.project();
                match view {
                    View::Arrange => {
                        if !timeline_mode {
                            ui.horizontal(|ui| {
                                if ui
                                    .button("Timeline")
                                    .on_hover_text("Back to the coloured track lanes")
                                    .clicked()
                                {
                                    self.timeline_mode = true;
                                }
                                ui.label(
                                    egui::RichText::new("Piano roll")
                                        .size(11.0)
                                        .color(theme::text_muted()),
                                );
                                ui.checkbox(&mut roll.snap, "Snap 1/16").on_hover_text(
                                    "Snap note drags and insertions to a sixteenth",
                                );
                            });
                        }
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
                                    .color(theme::text_muted()),
                                );
                                if ui.button("Trim 1 beat").clicked() {
                                    let bar = project.song.time_signature.bar_ticks().max(1);
                                    audio_trim = Some((*track_id, bar / 4));
                                }
                                if ui.button("Move 1 beat").clicked() {
                                    let bar = project.song.time_signature.bar_ticks().max(1);
                                    audio_move = Some((*track_id, bar / 4));
                                }
                                if ui.button("Move back 1 beat").clicked() {
                                    let bar = project.song.time_signature.bar_ticks().max(1);
                                    audio_move = Some((*track_id, -(bar / 4)));
                                }
                                if ui.button("Split at playhead").clicked() {
                                    audio_split = Some((*track_id, position));
                                }
                                if ui.button("Remove audio").clicked() {
                                    audio_remove = Some(*track_id);
                                }
                                if ui
                                    .button("Start at playhead")
                                    .on_hover_text("Trim the front to the playhead")
                                    .clicked()
                                    && let Some(reference) = project
                                        .song
                                        .tracks
                                        .iter()
                                        .find(|track| track.id == *track_id)
                                        .and_then(|track| track.audio.as_ref())
                                {
                                    audio_trim = Some((
                                        *track_id,
                                        (position - reference.start).max(1),
                                    ));
                                }
                                if ui
                                    .button(format!("Takes ({})", project.song.tracks.iter().find(|track| track.id == *track_id).map_or(0, |track| track.takes.len())))
                                    .on_hover_text("Store, use or remove audio takes")
                                    .clicked()
                                {
                                    self.takes_open = true;
                                }
                                if ui
                                    .button("End at playhead")
                                    .on_hover_text("Trim the audio end to the playhead")
                                    .clicked()
                                    && let Some(reference) = project
                                        .song
                                        .tracks
                                        .iter()
                                        .find(|track| track.id == *track_id)
                                        .and_then(|track| track.audio.as_ref())
                                {
                                    audio_length =
                                        Some((*track_id, (position - reference.start).max(1)));
                                }
                                let bar = project.song.time_signature.bar_ticks().max(1);
                                let current = project
                                    .song
                                    .tracks
                                    .iter()
                                    .find(|track| track.id == *track_id)
                                    .and_then(|track| track.audio.as_ref())
                                    .map_or((0, 0), |reference| {
                                        (reference.fade_in, reference.fade_out)
                                    });
                                if ui.small_button("Fade in bar").clicked() {
                                    audio_fades = Some((*track_id, bar, current.1));
                                }
                                if ui.small_button("Fade out bar").clicked() {
                                    audio_fades = Some((*track_id, current.0, bar));
                                }
                                if ui.small_button("No fades").clicked() {
                                    audio_fades = Some((*track_id, 0, 0));
                                }
                                let mut gain_db = project
                                    .song
                                    .tracks
                                    .iter()
                                    .find(|track| track.id == *track_id)
                                    .and_then(|track| track.audio.as_ref())
                                    .map_or(0.0, |reference| f32::from(reference.gain_mb) / 1_000.0);
                                if ui
                                    .add(
                                        egui::Slider::new(&mut gain_db, -24.0..=12.0)
                                            .text("gain")
                                            .suffix(" dB")
                                            .fixed_decimals(1),
                                    )
                                    .changed()
                                {
                                    audio_gain = Some((*track_id, (gain_db * 1_000.0) as i16));
                                }
                                let mut stretch = project
                                    .song
                                    .tracks
                                    .iter()
                                    .find(|track| track.id == *track_id)
                                    .and_then(|track| track.audio.as_ref())
                                    .is_some_and(|reference| reference.stretch);
                                if ui
                                    .checkbox(&mut stretch, "Stretch")
                                    .on_hover_text(
                                        "Resample the source to the clip length (pitch shifts)",
                                    )
                                    .changed()
                                {
                                    audio_stretch = Some((*track_id, stretch));
                                }
                                if ui
                                    .small_button("Normalize")
                                    .on_hover_text("Write a normalized copy and switch to it")
                                    .clicked()
                                {
                                    audio_normalize = Some(*track_id);
                                }
                                if ui
                                    .small_button("Reverse")
                                    .on_hover_text("Write a reversed copy and switch to it")
                                    .clicked()
                                {
                                    self.pending_reverse = Some(*track_id);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("waveform")
                                        .size(10.0)
                                        .color(theme::text_dim()),
                                );
                                ui.add(
                                    egui::Slider::new(&mut self.waveform_zoom, 1.0..=8.0)
                                        .text("zoom")
                                        .fixed_decimals(1),
                                );
                                ui.add(
                                    egui::Slider::new(&mut self.waveform_position, 0.0..=1.0)
                                        .text("position")
                                        .fixed_decimals(2),
                                );
                            });
                            if let Some(fraction) =
                                Self::draw_waveform(
                                    ui,
                                    peaks,
                                    self.waveform_zoom,
                                    self.waveform_position,
                                )
                                && let Some(reference) = project
                                    .song
                                    .tracks
                                    .iter()
                                    .find(|track| track.id == *track_id)
                                    .and_then(|track| track.audio.as_ref())
                            {
                                seek = Some(
                                    reference.start
                                        + (reference.length as f32 * fraction) as i64,
                                );
                            }
                        }
                        if timeline_mode {
                            ui.horizontal(|ui| {
                                if let Some(track) = roll.selected
                                    && ui
                                        .button("Add clip")
                                        .on_hover_text("Add a one-bar clip at the playhead")
                                        .clicked()
                                {
                                    self.pending_clip_add = Some((
                                        track,
                                        position,
                                        project.song.time_signature.bar_ticks().max(1),
                                    ));
                                }
                            });
                        }
                        let selected_clips = self.selected_clips.clone();
                        let (outcome, timeline_track, clip_edit) = if timeline_mode {
                            crate::timeline::show(
                                ui,
                                &project.song,
                                position,
                                roll.selected,
                                timeline_zoom,
                                self.playing,
                                &self.timeline_waveforms,
                            )
                        } else {
                            (
                                piano_roll::show(
                                    ui,
                                    &project.song,
                                    roll,
                                    position,
                                    &selected_clips,
                                ),
                                None,
                                None,
                            )
                        };
                        if let Some(track) = timeline_track {
                            roll.selected = Some(track);
                        }
                        if clip_edit.is_some() {
                            self.pending_clip_edit = clip_edit;
                        }
                        if let Some((first, last)) = roll.band.take()
                            && let Some(track_id) = roll.selected
                            && let Some(track) = project
                                .song
                                .tracks
                                .iter()
                                .find(|track| track.id == track_id)
                        {
                            let clips = piano_roll::clips_in_band(track, first, last);
                            self.selected_clips =
                                clips.iter().map(|clip| (track_id, *clip)).collect();
                            self.selected_clip = clips.last().map(|clip| (track_id, *clip));
                        }
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
                        if let Some(tick) = outcome.chord {
                            chord_click = Some(tick);
                        }
                        if let Some(tick) = outcome.lyric {
                            lyric_click = Some(tick);
                        }
                        if let Some(tick) = outcome.seek {
                            seek = Some(tick);
                        }
                        if let Some(track_id) = roll.selected
                            && let Some(track) = project
                                .song
                                .tracks
                                .iter()
                                .find(|track| track.id == track_id)
                        {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Automation")
                                        .size(10.0)
                                        .color(theme::text_dim()),
                                );
                                let mut target = self.automation_target;
                                egui::ComboBox::from_id_salt("automation-target")
                                    .selected_text(format!("{target:?}"))
                                    .show_ui(ui, |ui| {
                                        use project::song::AutomationTarget;
                                        for option in [
                                            AutomationTarget::Volume,
                                            AutomationTarget::Pan,
                                            AutomationTarget::Send,
                                            AutomationTarget::Brightness,
                                        ] {
                                            ui.selectable_value(
                                                &mut target,
                                                option,
                                                format!("{option:?}"),
                                            );
                                        }
                                    });
                                self.automation_target = target;
                                ui.label("slot");
                                ui.add(
                                    egui::DragValue::new(&mut self.automation_plugin_slot)
                                        .range(0..=9999),
                                );
                                ui.label("param");
                                ui.add(
                                    egui::DragValue::new(&mut self.automation_plugin_param)
                                        .range(0..=9999),
                                );
                                if ui.button("Plugin").clicked() {
                                    self.automation_target =
                                        project::song::AutomationTarget::PluginParam {
                                            slot: self.automation_plugin_slot,
                                            parameter: self.automation_plugin_param,
                                        };
                                }
                            });
                            let target = self.automation_target;
                            let lane = track
                                .automation
                                .iter()
                                .find(|lane| lane.target == target)
                                .cloned()
                                .unwrap_or(project::song::Automation {
                                    target,
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
                    View::Drums => {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Beat").size(16.0).strong());
                            if ui
                                .button("Open floating editor")
                                .on_hover_text("Open the same grid in a window")
                                .clicked()
                            {
                                self.beat_dialog = true;
                            }
                            if ui.small_button("◀").on_hover_text("Previous bar").clicked() {
                                self.pending_beat_start =
                                    Some((self.beat_start - self.beat_bar).max(0));
                            }
                            if ui.small_button("▶").on_hover_text("Next bar").clicked() {
                                self.pending_beat_start =
                                    Some((self.beat_start + self.beat_bar).max(0));
                            }
                            if let Some(track) = self.beat_track {
                                let name = project
                                    .song
                                    .tracks
                                    .iter()
                                    .find(|entry| entry.id == track)
                                    .map_or_else(
                                        || format!("track {track}"),
                                        |entry| entry.name.clone(),
                                    );
                                ui.label(
                                    egui::RichText::new(name)
                                        .size(10.0)
                                        .color(theme::text_dim()),
                                );
                            }
                        });
                        if self.beat_track.is_none() {
                            ui.label("Select a track to edit its beat.");
                        } else {
                            let mut reload = None;
                            ui.horizontal(|ui| {
                                for bars in [1_usize, 2, 4] {
                                    if ui
                                        .selectable_label(
                                            self.beat_bars == bars,
                                            format!("{bars} bar"),
                                        )
                                        .clicked()
                                    {
                                        reload = Some(bars);
                                    }
                                }
                            });
                            for (row, (_, label)) in arranging::beat::ROWS.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(*label)
                                            .size(11.0)
                                            .color(theme::text_dim()),
                                    );
                                    let steps = self.beat_pattern.grid[row].len();
                                    for slot in 0..steps {
                                        let level = self.beat_pattern.grid[row][slot];
                                        let text = if level == 0 {
                                            String::from("·")
                                        } else {
                                            level.to_string()
                                        };
                                        if ui
                                            .add_sized([24.0, 24.0], egui::Button::new(text))
                                            .clicked()
                                        {
                                            self.beat_pattern.cycle(row, slot);
                                        }
                                    }
                                });
                            }
                            if let Some(bars) = reload
                                && let Some(track) = self.beat_track
                                && let Some((bar, pattern)) = self.editor.beat_pattern_bars(
                                    track,
                                    self.beat_start,
                                    bars,
                                )
                            {
                                self.beat_bar = bar;
                                self.beat_pattern = pattern;
                                self.beat_bars = bars;
                            }
                            if ui.button("Apply beat").clicked()
                                && let Some(track) = self.beat_track
                            {
                                match self.editor.apply_beat(
                                    track,
                                    self.beat_start,
                                    self.beat_bar,
                                    &self.beat_pattern,
                                ) {
                                    Ok(added) => {
                                        self.status = Some(format!("beat: {added} hit(s)"))
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                        }
                    }
                    View::Map => {
                        ui.heading("MIDI Map");
                        ui.label(
                            egui::RichText::new(
                                "Controller bindings and track controls. Learn bindings in Windows > Controllers.",
                            )
                            .size(10.0)
                            .color(theme::text_dim()),
                        );
                        if ui.button("Open Controllers").clicked() {
                            self.controller_open = true;
                        }
                        ui.separator();
                        egui::Grid::new("midi-map-grid")
                            .striped(true)
                            .num_columns(4)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Track").strong());
                                ui.label(egui::RichText::new("Volume").strong());
                                ui.label(egui::RichText::new("Pan").strong());
                                ui.label(egui::RichText::new("Send").strong());
                                ui.end_row();
                                for track in &project.song.tracks {
                                    ui.label(if track.name.is_empty() {
                                        "untitled"
                                    } else {
                                        &track.name
                                    });
                                    ui.label(track.volume.to_string());
                                    ui.label(track.pan.to_string());
                                    ui.label(track.send_level.to_string());
                                    ui.end_row();
                                }
                            });
                        ui.separator();
                        ui.label(egui::RichText::new("Controller bindings").strong());
                        if self.midi_map.controller_bindings().is_empty() {
                            ui.label(
                                egui::RichText::new("no controller bindings yet")
                                    .size(10.0)
                                    .color(theme::text_dim()),
                            );
                        }
                        for (number, action) in self.midi_map.controller_bindings() {
                            ui.label(format!("CC {number}  →  {}", action.label()));
                        }
                    }
                    View::Perform => {
                        if kids {
                            let time = ui.input(|input| input.time) as f32;
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 190.0),
                                egui::Sense::hover(),
                            );
                            let pulse = if self.playing && !self.settings.reduced_motion {
                                (time * 3.0).sin() * 6.0
                            } else {
                                0.0
                            };
                            let center = rect.center();
                            ui.painter().circle_filled(
                                center,
                                72.0 + pulse,
                                theme::accent().gamma_multiply(if self.playing { 0.30 } else { 0.14 }),
                            );
                            ui.painter().circle_stroke(
                                center,
                                72.0 + pulse,
                                egui::Stroke::new(2.0_f32, theme::accent()),
                            );
                            ui.painter().text(
                                center,
                                egui::Align2::CENTER_CENTER,
                                if self.playing { "Playing" } else { "Press any key" },
                                egui::FontId::proportional(20.0),
                                theme::text(),
                            );
                        }
                        if !kids {
                            let current = project
                                .song
                                .chords
                                .iter()
                                .filter(|event| event.tick <= position)
                                .max_by_key(|event| event.tick)
                                .map(|event| event.chord.format(music_core::Spelling::Sharp));
                            let next = project
                                .song
                                .chords
                                .iter()
                                .filter(|event| event.tick > position)
                                .min_by_key(|event| event.tick)
                                .map(|event| event.chord.format(music_core::Spelling::Sharp));
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Now")
                                        .size(11.0)
                                        .color(theme::text_dim()),
                                );
                                ui.label(
                                    egui::RichText::new(
                                        current.unwrap_or_else(|| String::from("—")),
                                    )
                                    .size(28.0),
                                );
                                ui.add_space(18.0);
                                ui.label(
                                    egui::RichText::new("Next")
                                        .size(11.0)
                                        .color(theme::text_dim()),
                                );
                                ui.label(
                                    egui::RichText::new(next.unwrap_or_else(|| String::from("—")))
                                        .size(18.0),
                                );
                            });
                            ui.separator();
                        }
                        if let Some(chord) = &self.midi_chord {
                            ui.label(
                                egui::RichText::new(format!("Keys: {chord}"))
                                    .size(13.0)
                                    .color(theme::accent()),
                            );
                        }
                        if kids && !lesson.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("Play the next chord")
                                        .size(14.0)
                                        .color(theme::text_muted()),
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
                                .color(theme::text_dim()),
                        );
                        let pad_width = ((ui.available_width() - 24.0) / 4.0).clamp(96.0, 190.0);
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
                                            [pad_width, 52.0],
                                            egui::Button::new(
                                                egui::RichText::new(&text).size(14.0),
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
                        let mut save_pdf = false;
                        if !self.printers_loaded {
                            self.printers = available_printers();
                            self.printers_loaded = true;
                        }
                        if self.score.track.is_none() {
                            self.score.track = project
                                .song
                                .tracks
                                .iter()
                                .find(|track| needs_grand_staff(track))
                                .or_else(|| {
                                    project.song.tracks.iter().find(|track| {
                                        !track
                                            .notes
                                            .iter()
                                            .any(|note| note.channel.zero_based() == 9)
                                    })
                                })
                                .or_else(|| project.song.tracks.first())
                                .map(|track| track.id);
                        }
                        let score_tracks: Vec<(TrackId, String)> = project
                            .song
                            .tracks
                            .iter()
                            .map(|track| (track.id, track.name.clone()))
                            .collect();
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new("Score").strong());
                            let pdf_title = if project.song.name.is_empty() {
                                "Lazy Arranger score"
                            } else {
                                project.song.name.as_str()
                            };
                            if let Some(text) = score::pdf::unencodable_text(&project.song, pdf_title) {
                                ui.label(
                                    egui::RichText::new("PDF: ?")
                                        .size(9.0)
                                        .color(theme::recording()),
                                )
                                .on_hover_text(format!(
                                    "WinAnsi PDF cannot encode {text:?}; it will print as ?"
                                ));
                            }
                            ui.checkbox(&mut self.score.all_tracks, "All tracks");
                            if self.score.all_tracks {
                                if ui.checkbox(&mut self.score.follow, "Follow").changed() {
                                    self.settings.score_follow = self.score.follow;
                                    let _ = self.settings.save(&crate::settings::Settings::path());
                                }
                                if ui
                                    .checkbox(&mut self.score.full_frames, "Frames")
                                    .changed()
                                {
                                    self.settings.score_frames = self.score.full_frames;
                                    let _ = self.settings.save(&crate::settings::Settings::path());
                                }
                                let tracks: Vec<(TrackId, String)> = project
                                    .song
                                    .tracks
                                    .iter()
                                    .map(|entry| (entry.id, entry.name.clone()))
                                    .collect();
                                let mut toggle = None;
                                ui.menu_button("Tracks", |ui| {
                                    for (id, name) in &tracks {
                                        let mut shown = !self.score.full_hidden.contains(id);
                                        if ui.checkbox(&mut shown, name).changed() {
                                            toggle = Some((*id, shown));
                                        }
                                    }
                                });
                                if let Some((id, shown)) = toggle {
                                    if shown {
                                        self.score.full_hidden.remove(&id);
                                    } else {
                                        self.score.full_hidden.insert(id);
                                    }
                                }
                            }
                            let mut write = self.score.entry;
                            if ui.checkbox(&mut write, "Write notes").changed() {
                                self.score.entry = write;
                                self.score.preview = !write;
                            }
                            let selected = self
                                .score.track
                                .or_else(|| score_tracks.first().map(|(id, _)| *id));
                            egui::ComboBox::from_id_salt("score-track")
                                .selected_text(
                                    selected
                                        .and_then(|id| {
                                            score_tracks
                                                .iter()
                                                .find(|(track, _)| *track == id)
                                                .map(|(_, name)| name.as_str())
                                        })
                                        .unwrap_or("Selected track"),
                                )
                                .show_ui(ui, |ui| {
                                    for (id, name) in &score_tracks {
                                        ui.selectable_value(&mut self.score.track, Some(*id), name);
                                    }
                                });
                            ui.label("Octave");
                            ui.add(
                                egui::DragValue::new(&mut self.score.octave).range(0..=8),
                            );
                            ui.label("Length");
                            egui::ComboBox::from_id_salt("score-length")
                                .selected_text(match self.score.note_length {
                                    value if value >= music_core::PPQ * 4 => "Whole",
                                    value if value >= music_core::PPQ * 2 => "Half",
                                    value if value >= music_core::PPQ => "Quarter",
                                    value if value >= music_core::PPQ / 2 => "Eighth",
                                    _ => "16th",
                                })
                                .show_ui(ui, |ui| {
                                    for (value, label) in [
                                        (music_core::PPQ * 4, "Whole"),
                                        (music_core::PPQ * 2, "Half"),
                                        (music_core::PPQ, "Quarter"),
                                        (music_core::PPQ / 2, "Eighth"),
                                        (music_core::PPQ / 4, "16th"),
                                    ] {
                                        ui.selectable_value(
                                            &mut self.score.note_length,
                                            value,
                                            label,
                                        );
                                    }
                                });
                            ui.checkbox(&mut self.score.dotted, "Dot");
                            if ui.button("Rest").on_hover_text("Advance the entry cursor").clicked() {
                                self.score.cursor += entry_length(
                                    self.score.note_length.max(1),
                                    self.score.dotted,
                                );
                            }
                            ui.label(format!("cursor {}", self.score.cursor));
                            if ui.small_button("Reset").clicked() {
                                self.score.cursor = 0;
                            }
                            ui.label("Staves: automatic");
                            ui.checkbox(&mut self.score.grand, "Grand staff");
                            if ui.button("−").clicked() {
                                self.score.zoom = (self.score.zoom - 0.1).max(0.5);
                            }
                            ui.label(format!("{:.1}", self.score.zoom));
                            if ui.button("+").clicked() {
                                self.score.zoom = (self.score.zoom + 0.1).min(2.5);
                            }
                            if ui.button("◀").clicked() {
                                self.score.page = self.score.page.saturating_sub(1);
                            }
                            if ui.button("▶").clicked() {
                                self.score.page += 1;
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}/{}",
                                    self.score.page + 1,
                                    self.score.state.cecilia_page_count.max(1)
                                ))
                                .size(10.0)
                                .color(theme::text_dim()),
                            );
                            if ui.button("Export PDF…").clicked() {
                                save_pdf = true;
                            }
                            if ui.button("Print…").clicked() {
                                self.print_dialog = true;
                            }
                        });
                        let track = self
                            .score.track
                            .filter(|track| project.song.tracks.iter().any(|entry| entry.id == *track))
                            .or_else(|| {
                                roll.selected.filter(|track| {
                                    project.song.tracks.iter().any(|entry| entry.id == *track)
                                })
                            })
                            .or_else(|| project.song.tracks.first().map(|track| track.id));
                        if let Some(track_song) = track.and_then(|track| {
                            project.song.tracks.iter().find(|entry| entry.id == track)
                        }) {
                            let track = track_song.id;
                            let grand_staff = self.score.grand || needs_grand_staff(track_song);
                            let score_settings = score::Settings {
                                grand_staff,
                                ..score::Settings::default()
                            };
                            let engraved = score::engrave(&project.song, track, score_settings);
                            if !self.score.preview {
                                if self.playing
                                    && let Some(page) =
                                        crate::score_view::page_at_tick(&engraved, position)
                                {
                                    self.score.page = page;
                                }
                                self.score.page =
                                    crate::score_view::clamp_page(&engraved, self.score.page);
                            }
                            let title = if project.song.name.is_empty() {
                                "Lazy Arranger score"
                            } else {
                                &project.song.name
                            };
                            let track_index = project
                                .song
                                .tracks
                                .iter()
                                .position(|entry| entry.id == track)
                                .unwrap_or(0);
                            if save_pdf {
                                let pdf = if self.score.all_tracks {
                                    let tracks: Vec<TrackId> = project
                                        .song
                                        .tracks
                                        .iter()
                                        .filter(|entry| !self.score.full_hidden.contains(&entry.id))
                                        .map(|entry| entry.id)
                                        .collect();
                                    let tracks = if tracks.is_empty() {
                                        project.song.tracks.iter().map(|entry| entry.id).collect()
                                    } else {
                                        tracks
                                    };
                                    Ok(score::pdf::to_full_pdf(
                                        &project.song,
                                        score_settings,
                                        title,
                                        &tracks,
                                    ))
                                } else if self.score.preview {
                                    score::cecilia::render_pdf(
                                        &project.song,
                                        track_index,
                                        title,
                                        grand_staff,
                                    )
                                } else {
                                    Ok(score::pdf::to_pdf(&engraved, score_settings, title))
                                };
                                match pdf {
                                    Ok(pdf) => match std::fs::write(&self.score.pdf_path, pdf) {
                                        Ok(()) => {
                                            score_status =
                                                Some(format!("wrote {}", self.score.pdf_path));
                                        }
                                        Err(error) => {
                                            score_status =
                                                Some(format!("{}: {error}", self.score.pdf_path));
                                        }
                                    },
                                    Err(error) => {
                                        score_status = Some(format!("Cecilia: {error}"));
                                    }
                                }
                            }
                            let zoom = self.score.zoom;
                            let page = self.score.page;
                            let full_tracks: Vec<(String, score::Engraved)> = if self.score.all_tracks {
                                let selected: Vec<TrackId> = project
                                    .song
                                    .tracks
                                    .iter()
                                    .filter(|entry| !self.score.full_hidden.contains(&entry.id))
                                    .map(|entry| entry.id)
                                    .collect();
                                let selected = if selected.is_empty() {
                                    project.song.tracks.iter().map(|entry| entry.id).collect()
                                } else {
                                    selected
                                };
                                let mut key = crate::score_view::full_score_hash(&project.song, title);
                                for id in &selected {
                                    key = key
                                        .wrapping_mul(1_099_511_628_211)
                                        .wrapping_add(u64::from(id.get()));
                                }
                                if self
                                    .score.full_cache
                                    .as_ref()
                                    .is_none_or(|(cached, _)| *cached != key)
                                {
                                    self.score.full_cache = Some((
                                        key,
                                        project
                                            .song
                                            .tracks
                                            .iter()
                                            .filter(|entry| selected.contains(&entry.id))
                                            .map(|entry| {
                                                (
                                                    entry.name.clone(),
                                                    score::engrave(
                                                        &project.song,
                                                        entry.id,
                                                        score_settings,
                                                    ),
                                                )
                                            })
                                            .collect(),
                                    ));
                                }
                                self.score.full_cache
                                    .as_ref()
                                    .map(|(_, tracks)| tracks.clone())
                                    .unwrap_or_default()
                            } else {
                                Vec::new()
                            };
                            if self.score.all_tracks
                                && self.score.follow
                                && self
                                    .player
                                    .as_ref()
                                    .is_some_and(audio::player::Player::is_playing)
                                && let Some(page) = crate::score_view::full_score_page_for_tick(
                                    &full_tracks,
                                    position,
                                )
                            {
                                self.score.page = page;
                            }
                            let outcome = if self.score.all_tracks {
                                let (count, score_seek) = crate::score_view::show_full(
                                    ui,
                                    &full_tracks,
                                    self.score.page,
                                    zoom,
                                    title,
                                    &mut self.score.state,
                                    position,
                                    self.score.full_frames,
                                    project.song.tempo_map.bpm_at(position),
                                    &project.song.markers,
                                );
                                if let Some(tick) = score_seek {
                                    seek = Some(tick);
                                }
                                self.score.page = self.score.page.min(count.saturating_sub(1));
                                self.score.state.cecilia_page_count = count;
                                crate::score_view::ScoreOutcome::default()
                            } else if self.score.preview {
                                self.score.state.ensure_cecilia(
                                    ui.ctx(),
                                    &project.song,
                                    track_index,
                                    title,
                                    grand_staff,
                                );
                                if let Some(error) = &self.score.state.cecilia_error {
                                    score_status = Some(format!("Cecilia: {error}"));
                                }
                                let pages = self.score.state.cecilia_page_count.max(1);
                                self.score.page = self.score.page.min(pages - 1);
                                if let Some(tick) = crate::score_view::show_preview(
                                    ui,
                                    &mut self.score.state,
                                    self.score.page,
                                    zoom,
                                    &engraved,
                                    position,
                                    self.score.entry.then_some(self.score.cursor),
                                ) {
                                    seek = Some(tick);
                                }
                                crate::score_view::ScoreOutcome::default()
                            } else {
                                crate::score_view::show(
                                    ui,
                                    &engraved,
                                    page,
                                    zoom,
                                    title,
                                    &mut self.score.state,
                                    position,
                                    self.score.entry.then_some(self.score.cursor),
                                )
                            };
                            if let Some(drag) = outcome.drag {
                                if let Some(found) = project
                                    .song
                                    .tracks
                                    .iter()
                                    .find(|entry| entry.id == track)
                                    .and_then(|entry| {
                                        entry.notes.iter().find(|note| {
                                            note.start == drag.from_tick
                                                && note.pitch.as_u8() == drag.from_pitch
                                        })
                                    })
                                    .map(|note| note.id)
                                {
                                    if drag.resize {
                                        match self.editor.set_note_duration(
                                            track,
                                            found,
                                            drag.duration,
                                        ) {
                                            Ok(()) => {
                                                score_status = Some(format!(
                                                    "duration set to {} ticks",
                                                    drag.duration
                                                ));
                                            }
                                            Err(error) => score_status = Some(error),
                                        }
                                    } else {
                                        match music_core::Pitch::new(drag.to_pitch) {
                                            Ok(pitch) => {
                                                match editor.move_note(
                                                    track,
                                                    found,
                                                    drag.to_tick,
                                                    pitch,
                                                ) {
                                                    Ok(()) => {
                                                        score_status = Some(format!(
                                                            "moved to tick {} pitch {}",
                                                            drag.to_tick, drag.to_pitch
                                                        ));
                                                    }
                                                    Err(error) => {
                                                        score_status = Some(error.to_string());
                                                    }
                                                }
                                            }
                                            Err(error) => {
                                                score_status = Some(error.to_string());
                                            }
                                        }
                                    }
                                }
                            } else if let Some((tick, pitch)) = outcome.note {
                                if let Some(entry) =
                                    project.song.tracks.iter().find(|entry| entry.id == track)
                                    && let Some(found) = entry.notes.iter().find(|note| {
                                        note.start == tick && note.pitch.as_u8() == pitch
                                    })
                                {
                                    self.selected_note = Some((track, found.id));
                                    self.selected_notes.clear();
                                    score_status = Some(format!("selected note {}", found.id));
                                }
                            } else if let Some(hit) = outcome.insert
                                && entry_enabled_flag
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
                                        .color(theme::text_muted()),
                                );
                            });
                        }
                    }
                }
                (
                    clicked,
                    automation_edit,
                    audio_trim,
                    audio_move,
                    audio_gain,
                    audio_split,
                    audio_length,
                    audio_fades,
                    audio_stretch,
                    audio_normalize,
                    audio_remove,
                    score_status,
                    perform_chord,
                    lesson_next,
                    lesson_reset,
                    kids_load,
                    scroll_edit,
                    velocity_edit,
                    preview_note,
                    chord_click,
                    seek,
                    lyric_click,
                )
            });
        let (
            clicked,
            automation_edit,
            audio_trim,
            audio_move,
            audio_gain,
            audio_split,
            audio_length,
            audio_fades,
            audio_stretch,
            audio_normalize,
            audio_remove,
            score_status,
            perform_chord,
            lesson_next,
            lesson_reset,
            kids_load,
            scroll_edit,
            velocity_edit,
            preview_note,
            chord_click,
            seek,
            lyric_click,
        ) = panel.inner;
        if let Some(tick) = seek {
            self.seek_to(tick);
        }
        if let Some(tick) = lyric_click {
            self.lyrics_open = true;
            self.lyric_selected = Some(tick);
            self.lyric_text = self
                .song()
                .lyrics
                .iter()
                .find(|event| event.tick == tick)
                .map(|event| event.text.clone())
                .unwrap_or_default();
        }
        if let Some(tick) = chord_click {
            self.selected_chord = Some(tick);
            self.status = Some(format!("selected chord at tick {tick}"));
        }
        if let Some(click) = clicked {
            self.edit_click(click);
        }
        if let Some(pitch) = preview_note {
            self.preview_pitch(pitch);
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
        if let Some((track, length)) = audio_length
            && let Err(error) = self.editor.trim_audio_length(track, length)
        {
            self.status = Some(error);
        }
        if let Some((track, delta)) = audio_trim {
            if let Err(error) = self.editor.trim_audio_start(track, delta) {
                self.status = Some(error);
            } else {
                self.status = Some(format!("trimmed {delta} tick(s)"));
            }
        }
        if let Some((track, delta)) = audio_move
            && let Err(error) = self.editor.move_audio(track, delta)
        {
            self.status = Some(error);
        }
        if let Some((track, at)) = audio_split {
            match self.editor.split_audio(track, at) {
                Ok(()) => self.status = Some(String::from("split audio at playhead")),
                Err(error) => self.status = Some(error),
            }
        }
        if let Some((track, fade_in, fade_out)) = audio_fades
            && let Err(error) = self.editor.set_audio_fades(track, fade_in, fade_out)
        {
            self.status = Some(error);
        }
        if let Some((track, stretch)) = audio_stretch
            && let Err(error) = self.editor.set_audio_stretch(track, stretch)
        {
            self.status = Some(error);
        }
        if let Some(track) = audio_normalize {
            match self.editor.normalize_audio(track, 0.9) {
                Ok(path) => self.status = Some(format!("normalized to {}", path.display())),
                Err(error) => self.status = Some(error),
            }
        }
        if let Some(track) = audio_remove {
            match self.editor.remove_audio(track) {
                Ok(()) => self.status = Some(String::from("removed audio reference")),
                Err(error) => self.status = Some(error),
            }
        }
        if let Some((track, gain)) = audio_gain
            && let Err(error) = self.editor.set_audio_gain(track, gain)
        {
            self.status = Some(error);
        }
        if let Some((track, lane)) = automation_edit
            && let Err(error) = self.editor.merge_automation(track, lane)
        {
            self.status = Some(error);
        }
        if let Some(track) = self.pending_reverse.take() {
            match self.editor.reverse_audio(track) {
                Ok(path) => self.status = Some(format!("reversed to {}", path.display())),
                Err(error) => self.status = Some(error),
            }
        }
        if let Some((track, start, length)) = self.pending_clip_add.take() {
            match self.editor.add_clip(track, start, length) {
                Ok(()) => self.status = Some(String::from("clip added")),
                Err(error) => self.status = Some(error),
            }
        }
    }
}
