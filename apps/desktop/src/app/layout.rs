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
    /// "One staff" in the staves box: never split a track over two staves.
    pub(super) one_staff: bool,
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

/// What the central view asked for while it was drawn, applied after the
/// panel closes so the editor is no longer borrowed.
struct PanelEdits {
    clicked: Option<piano_roll::RollClick>,
    automation_edit: Option<(TrackId, project::song::Automation)>,
    audio_trim: Option<(TrackId, Tick)>,
    audio_move: Option<(TrackId, Tick)>,
    audio_gain: Option<(TrackId, i16)>,
    audio_split: Option<(TrackId, Tick)>,
    audio_length: Option<(TrackId, Tick)>,
    audio_fades: Option<(TrackId, Tick, Tick)>,
    audio_stretch: Option<(TrackId, bool)>,
    audio_normalize: Option<TrackId>,
    audio_remove: Option<TrackId>,
    score_status: Option<String>,
    perform_chord: Option<music_core::Chord>,
    lesson_next: bool,
    lesson_reset: bool,
    kids_load: bool,
    scroll_edit: Option<crate::piano_roll::RollEdit>,
    velocity_edit: Option<(TrackId, project::NoteId, u8)>,
    preview_note: Option<u8>,
    chord_click: Option<Tick>,
    seek: Option<Tick>,
    lyric_click: Option<Tick>,
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
                // HeaderBar::resized: 16 px margins, a 220 px title area, the
                // transport (stop, play, record, loop, click), then the LCD.
                let mut area = rect.shrink2(egui::vec2(16.0, 0.0));
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
                let text_left = badge.right() + 14.0;
                let text_width = (title_area.right() - text_left).max(0.0);
                let name = if self.song().name.is_empty() {
                    "Untitled"
                } else {
                    self.song().name.as_str()
                };
                let title = ui.painter().layout(
                    name.to_owned(),
                    theme::semibold(16.0),
                    theme::text(),
                    text_width,
                );
                ui.painter().galley(
                    egui::pos2(text_left, rect.center().y - title.size().y),
                    title,
                    theme::text(),
                );
                let (tonic, minor) = self.song().key.map_or_else(
                    || {
                        let key = score::spelling::detect_key(self.song());
                        (key.tonic, key.minor)
                    },
                    |key| (key.tonic, key.minor),
                );
                let subtitle = format!(
                    "{} BPM \u{b7} {}/{} \u{b7} {} {}",
                    self.song().tempo_map.bpm_at(self.position).round() as i64,
                    self.song().time_signature.numerator,
                    self.song().time_signature.denominator,
                    if minor {
                        tonic.name_flat()
                    } else {
                        tonic.name_sharp()
                    },
                    if minor { "minor" } else { "major" }
                );
                ui.painter().text(
                    egui::pos2(text_left, rect.center().y + 2.0),
                    egui::Align2::LEFT_TOP,
                    subtitle,
                    egui::FontId::proportional(12.5),
                    theme::text_muted(),
                );

                // Transport: stop 44, play 54, record, loop and click 44, 8 px gaps.
                let mut x = area.left();
                let mut next_button = |size: f32| {
                    let button = Rect::from_center_size(
                        egui::pos2(x + size * 0.5, rect.center().y),
                        egui::vec2(size, size),
                    );
                    x += size + 8.0;
                    button
                };
                let stop = next_button(44.0);
                let play = next_button(54.0);
                let record = next_button(44.0);
                let looping = next_button(44.0);
                let metronome = next_button(44.0);
                area = Rect::from_min_max(egui::pos2(x - 8.0, area.top()), area.max);
                if transport_button(ui, stop, crate::transport::STOP, false) {
                    self.stop_playback();
                }
                if transport_button(ui, play, crate::transport::PLAY, self.playing) {
                    self.toggle_playback();
                    context.request_repaint_after(std::time::Duration::from_millis(16));
                }
                if transport_button(
                    ui,
                    record,
                    crate::transport::RECORD,
                    self.recording.is_some(),
                ) {
                    self.start_midi_recording();
                }
                if transport_button(ui, looping, crate::transport::LOOP, self.loop_enabled) {
                    self.toggle_loop();
                }
                if metronome_button(ui, metronome, self.settings.metronome) {
                    self.settings.metronome = !self.settings.metronome;
                }
                // LCD, 350 x 52, 16 px after the transport.
                let lcd_rect = Rect::from_min_size(
                    egui::pos2(
                        area.left() + 16.0,
                        rect.center().y - widgets::LCD_SIZE.y * 0.5,
                    ),
                    widgets::LCD_SIZE,
                );
                area = Rect::from_min_max(egui::pos2(lcd_rect.right(), area.top()), area.max);
                let mut lcd_ui = ui.new_child(egui::UiBuilder::new().max_rect(lcd_rect));
                let lcd = widgets::lcd(
                    &mut lcd_ui,
                    self.position,
                    self.song().tempo_map.bpm_at(self.position),
                    self.song().time_signature,
                );
                // LcdDisplay::mouseDown: the meter cell opens a list of meters.
                let meter_cell = egui::Rect::from_min_max(
                    egui::pos2(lcd.rect.right() - widgets::LCD_METER_WIDTH, lcd.rect.top()),
                    lcd.rect.max,
                );
                let meter_response = ui
                    .interact(meter_cell, ui.id().with("lcd-meter"), egui::Sense::click())
                    .on_hover_text(
                        "Drag or double-click the tempo to change it. Click the meter to pick a time signature.",
                    );
                let meter = self.song().time_signature;
                let meter_popup = ui.make_persistent_id("lcd-meter-popup");
                if meter_response.clicked() {
                    ui.memory_mut(|memory| memory.toggle_popup(meter_popup));
                }
                egui::popup_below_widget(
                    ui,
                    meter_popup,
                    &meter_response,
                    egui::PopupCloseBehavior::CloseOnClick,
                    |ui| {
                        ui.set_min_width(80.0);
                    for (numerator, denominator) in
                        [(2, 4), (3, 4), (4, 4), (5, 4), (6, 8), (7, 8), (9, 8), (12, 8)]
                    {
                        let current =
                            meter.numerator == numerator && meter.denominator == denominator;
                        if ui
                            .selectable_label(current, format!("{numerator}/{denominator}"))
                            .clicked()
                        {
                            match self.editor.set_time_signature(numerator, denominator) {
                                Ok(()) => {
                                    self.status = Some(format!("meter {numerator}/{denominator}"));
                                }
                                Err(error) => self.status = Some(error),
                            }
                        }
                    }
                    },
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
                // Right side, from the right edge: Ask AI, zoom - / +, the
                // audio pill, then the tab strip with the Mixer button.
                let ai_width = 112.0_f32.max(
                    ui.painter()
                        .layout_no_wrap(
                            String::from("Ask AI"),
                            theme::semibold(14.5),
                            theme::text(),
                        )
                        .size()
                        .x
                        + 64.0,
                );
                let ai_rect = Rect::from_min_size(
                    egui::pos2(area.right() - ai_width, rect.center().y - 20.0),
                    egui::vec2(ai_width, 40.0),
                );
                let zoom_right = ai_rect.left() - 10.0;
                let plus_rect = Rect::from_min_size(
                    egui::pos2(zoom_right - 28.0, rect.center().y - 15.0),
                    egui::vec2(28.0, 30.0),
                );
                let minus_rect = plus_rect.translate(egui::vec2(-36.0, 0.0));
                let rest = Rect::from_min_max(
                    egui::pos2(area.left(), area.top()),
                    egui::pos2(minus_rect.left() - 10.0, area.bottom()),
                );
                let status_width = 260.0_f32.min(rest.width() / 3.0);
                let status_area = Rect::from_min_max(
                    egui::pos2(rest.right() - status_width, rest.top()),
                    rest.max,
                );
                let rest =
                    Rect::from_min_max(rest.min, egui::pos2(status_area.left(), rest.bottom()));
                let tab_width = (rest.width() - 16.0).clamp(0.0, 440.0);
                let tab_area = Rect::from_center_size(rest.center(), egui::vec2(tab_width, 44.0));
                let mixer_rect = Rect::from_center_size(
                    egui::pos2(tab_area.right() - 36.0, rect.center().y),
                    egui::vec2(72.0, 34.0),
                );
                let tabs_rect = Rect::from_min_max(
                    tab_area.min,
                    egui::pos2(mixer_rect.left() - 10.0, tab_area.bottom()),
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

                if widgets::text_button(ui, mixer_rect, "mixer-toggle", "Mixer", self.mixer_visible)
                    .on_hover_text("Show or hide the mixer under the view")
                    .clicked()
                {
                    self.mixer_visible = !self.mixer_visible;
                }

                // Audio device pill (HeaderBar::paint, status pills).
                let (host, has_output) = self
                    .audio_host
                    .get_or_insert_with(|| {
                        (
                            audio::player::host_name(),
                            !audio::player::output_devices().is_empty(),
                        )
                    })
                    .clone();
                let running = has_output;
                let pill_text = match self
                    .player
                    .as_ref()
                    .and_then(|player| player.buffer_frames())
                {
                    Some(frames) => format!("{host} \u{b7} {frames}"),
                    None if has_output => {
                        format!("{host} \u{b7} {} kHz", audio::SAMPLE_RATE / 1000)
                    }
                    None => String::from("No audio device"),
                };
                let pill_galley = ui.painter().layout_no_wrap(
                    pill_text.clone(),
                    egui::FontId::proportional(13.0),
                    theme::text(),
                );
                let pill_width = (pill_galley.size().x.ceil() + 50.0).min(status_area.width());
                if pill_width > 40.0 {
                    let pill = Rect::from_min_size(
                        egui::pos2(status_area.right() - pill_width, rect.center().y - 16.0),
                        egui::vec2(pill_width, 32.0),
                    );
                    let response =
                        ui.interact(pill, ui.id().with("audio-settings"), egui::Sense::click());
                    ui.painter().rect_filled(
                        pill,
                        theme::RADIUS,
                        crate::transport::blend(theme::panel(), egui::Color32::WHITE, 0.03),
                    );
                    ui.painter().rect_stroke(
                        pill.shrink(0.5),
                        theme::RADIUS,
                        egui::Stroke::new(1.0_f32, theme::line_strong().gamma_multiply(0.7)),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().circle_filled(
                        egui::pos2(pill.left() + 17.0, pill.center().y),
                        4.0,
                        if running {
                            theme::ok()
                        } else {
                            theme::warning()
                        },
                    );
                    let text_rect = Rect::from_min_max(
                        egui::pos2(pill.left() + 28.0, pill.top()),
                        egui::pos2(pill.right() - 13.0, pill.bottom()),
                    );
                    let galley = ui.painter().layout(
                        pill_text,
                        egui::FontId::proportional(13.0),
                        theme::text(),
                        f32::INFINITY,
                    );
                    ui.painter().with_clip_rect(text_rect).galley(
                        egui::pos2(
                            text_rect.left(),
                            text_rect.center().y - galley.size().y * 0.5,
                        ),
                        galley,
                        theme::text(),
                    );
                    if response.on_hover_text("Audio and MIDI settings").clicked() {
                        self.settings_open = true;
                    }
                }

                for (button, glyph, factor, hint) in [
                    (minus_rect, "-", 1.2_f32, "Zoom out in time"),
                    (plus_rect, "+", 1.0 / 1.2, "Zoom in in time"),
                ] {
                    if widgets::text_button(ui, button, glyph, glyph, false)
                        .on_hover_text(hint)
                        .clicked()
                    {
                        if self.view == View::Arrange && self.timeline_mode {
                            self.timeline_zoom = (self.timeline_zoom * factor).clamp(0.5, 3.0);
                        } else {
                            self.roll.ticks_per_point =
                                (self.roll.ticks_per_point * factor).clamp(2.0, 64.0);
                        }
                    }
                }

                // Ask AI (AiButton::paintButton): accent gradient pill, sparkle, label.
                let ai_response =
                    ui.interact(ai_rect, ui.id().with("ask-ai"), egui::Sense::click());
                let mut accent = theme::accent();
                if ai_response.is_pointer_button_down_on() {
                    accent = crate::transport::blend(accent, egui::Color32::BLACK, 0.15);
                } else if ai_response.hovered() {
                    accent = crate::transport::blend(accent, egui::Color32::WHITE, 0.1);
                }
                let radius = ai_rect.height() / 2.0;
                let top = crate::transport::blend(accent, egui::Color32::WHITE, 0.18);
                let bottom = crate::transport::blend(accent, egui::Color32::BLACK, 0.18);
                let mut mesh = egui::Mesh::default();
                mesh.colored_vertex(ai_rect.left_top(), top);
                mesh.colored_vertex(ai_rect.right_top(), top);
                mesh.colored_vertex(ai_rect.right_bottom(), bottom);
                mesh.colored_vertex(ai_rect.left_bottom(), bottom);
                mesh.add_triangle(0, 1, 2);
                mesh.add_triangle(0, 2, 3);
                ui.painter().with_clip_rect(ai_rect).add(mesh);
                ui.painter().rect_stroke(
                    ai_rect.shrink(0.5),
                    theme::RADIUS,
                    egui::Stroke::new(
                        1.0_f32,
                        crate::transport::blend(accent, egui::Color32::BLACK, 0.35),
                    ),
                    egui::StrokeKind::Inside,
                );
                let ink = theme::accent_text();
                let inner_left = ai_rect.left() + radius * 0.7;
                crate::transport::draw_sparkle(
                    ui.painter(),
                    Rect::from_min_size(
                        egui::pos2(inner_left, ai_rect.center().y - 10.0),
                        egui::vec2(20.0, 20.0),
                    ),
                    ink,
                );
                ui.painter().text(
                    egui::pos2(inner_left + 27.0, ai_rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "Ask AI",
                    theme::semibold(14.5),
                    ink,
                );
                if ai_response
                    .on_hover_text("AI Chat: tell the AI what to make or change, in plain words")
                    .clicked()
                {
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
        // v1 shows the track list with the timeline only; the score and the
        // loops use the whole width.
        if matches!(self.view, View::Score | View::Loops | View::Drums) {
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
                        self.toggle_loop();
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
        // StatusBar::paint: panel with a 1 px top line, MIDI inputs on the
        // left, key hints (or what the app is doing) on the right, 12.5 pt.
        egui::TopBottomPanel::bottom("status")
            .exact_height(theme::STATUS_HEIGHT)
            .frame(egui::Frame::NONE.fill(theme::panel()))
            .show(context, |ui| {
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [rect.left_top(), rect.right_top()],
                    egui::Stroke::new(1.0_f32, theme::line()),
                );
                let font = egui::FontId::proportional(12.5);
                let inner = rect.shrink2(egui::vec2(16.0, 0.0));
                let errors = self
                    .editor
                    .project()
                    .validate()
                    .iter()
                    .filter(|issue| issue.severity == project::Severity::Error)
                    .count();
                let (hint, colour) = if errors > 0 {
                    (format!("{errors} validation error(s)"), theme::recording())
                } else if let Some(status) = &self.status {
                    (status.clone(), theme::text())
                } else {
                    (
                        String::from("Space Play \u{b7} C Metronome \u{b7} Home Start"),
                        theme::text_dim(),
                    )
                };
                let hint_width = if colour == theme::text_dim() {
                    340.0
                } else {
                    420.0
                };
                let hint_rect = Rect::from_min_max(
                    egui::pos2((inner.right() - hint_width).max(inner.left()), inner.top()),
                    inner.max,
                );
                let galley = ui.painter().layout_no_wrap(hint, font.clone(), colour);
                ui.painter().with_clip_rect(hint_rect).galley(
                    egui::pos2(
                        (hint_rect.right() - galley.size().x).max(hint_rect.left()),
                        hint_rect.center().y - galley.size().y * 0.5,
                    ),
                    galley,
                    colour,
                );

                let mut left = inner.left();
                let mut right_limit = hint_rect.left();
                if self.view == View::Arrange && self.timeline_mode {
                    let button = Rect::from_min_size(
                        egui::pos2(hint_rect.left() - 92.0, rect.center().y - 10.0),
                        egui::vec2(84.0, 20.0),
                    );
                    right_limit = button.left();
                    if widgets::text_button(ui, button, "piano-roll", "Piano roll", false)
                        .on_hover_text("Edit the notes of the selected track")
                        .clicked()
                    {
                        self.timeline_mode = false;
                    }
                }
                let names: Vec<String> = self
                    .midi_input
                    .as_ref()
                    .map(|inputs| inputs.names().into_iter().map(str::to_owned).collect())
                    .unwrap_or_default();
                if names.is_empty() {
                    ui.painter().text(
                        egui::pos2(left, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "No MIDI devices",
                        font,
                        theme::text_dim(),
                    );
                } else {
                    for name in names {
                        let galley =
                            ui.painter()
                                .layout_no_wrap(name, font.clone(), theme::text_muted());
                        let width = galley.size().x + 34.0;
                        if left + width > right_limit {
                            break;
                        }
                        ui.painter().circle_filled(
                            egui::pos2(left + 7.0, rect.center().y),
                            3.5,
                            theme::ok(),
                        );
                        ui.painter().galley(
                            egui::pos2(left + 14.0, rect.center().y - galley.size().y * 0.5),
                            galley,
                            theme::text_muted(),
                        );
                        left += width;
                    }
                }
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
        let held_chord = self.midi_chord.clone();
        egui::SidePanel::right("right-inspector")
            .resizable(true)
            .default_width(theme::INSPECTOR_WIDTH)
            .width_range(theme::INSPECTOR_WIDTH..=480.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .inner_margin(egui::Margin::symmetric(20, 4)),
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
                    // Inspector::resized: 20 px sides, 32 px rows, 6 px gaps.
                    let ui = &mut *ui;
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    ui.spacing_mut().interact_size.y = 30.0;
                    ui.spacing_mut().button_padding = egui::vec2(8.0, 6.0);
                    theme::v1_fields(ui.visuals_mut());
                    let width = ui.available_width();
                    widgets::section_label(ui, "Track", 26.0);
                    let Some(track) = &track else {
                        ui.label(
                            egui::RichText::new("No track selected")
                                .size(13.0)
                                .color(theme::text_muted()),
                        );
                        return;
                    };
                    if self.inspector_name_track != Some(track.id) {
                        self.inspector_name_track = Some(track.id);
                        self.inspector_name = track.name.clone();
                    }
                    let response = ui.add_sized(
                        [width, 32.0],
                        egui::TextEdit::singleline(&mut self.inspector_name)
                            .font(egui::FontId::proportional(14.0))
                            .margin(egui::vec2(8.0, 7.0)),
                    );
                    if response.lost_focus() {
                        let name = self.inspector_name.trim().to_string();
                        if name != track.name {
                            match self.editor.rename_track(track.id, &name) {
                                Ok(()) => {
                                    self.status = Some(format!("track renamed to {name:?}"));
                                }
                                Err(error) => self.status = Some(error),
                            }
                        }
                    }
                    let output = track
                        .soundfont
                        .as_ref()
                        .map_or_else(|| String::from("Built-in SoundFont"), |font| font.path.clone());
                    egui::ComboBox::from_id_salt("track-output")
                        .selected_text(output.clone())
                        .width(width)
                        .show_ui(ui, |ui| {
                            let _ = ui.selectable_label(true, output);
                        });
                    let channel = track
                        .notes
                        .first()
                        .map_or(1, |note| note.channel.one_based());
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("track-channel")
                            .selected_text(format!("Ch {channel}"))
                            .width(92.0 - 6.0)
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
                        let drums = channel == 10;
                        let sound = sound_name(track.program, drums);
                        egui::ComboBox::from_id_salt("track-program")
                            .selected_text(egui::RichText::new(sound).strong())
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for value in 0..=127_u8 {
                                    let name = sound_name(Some(value), drums);
                                    if drums && name.starts_with("Drum Kit") {
                                        continue;
                                    }
                                    if ui
                                        .selectable_label(track.program == Some(value), name)
                                        .clicked()
                                    {
                                        let _ = self.editor.set_track_program(track.id, Some(value));
                                    }
                                }
                            });
                    });
                    let (button_rect, _) =
                        ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::hover());
                    if widgets::text_button(
                        ui,
                        button_rect,
                        "inspector-effects",
                        "Effects...",
                        false,
                    )
                    .on_hover_text("The plugins on this track: its instrument and its effects")
                    .clicked()
                    {
                        self.view = View::Mix;
                    }
                    let (font_row, _) =
                        ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::hover());
                    let font_button = egui::Rect::from_min_max(
                        egui::pos2(font_row.right() - 96.0, font_row.top()),
                        font_row.max,
                    );
                    let font_info = track.soundfont.as_ref().map_or("No SoundFont loaded", |font| {
                        std::path::Path::new(&font.path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or(&font.path)
                    });
                    // Left-aligned: a box only as wide as the text, cut to fit.
                    let info_width = ui
                        .painter()
                        .layout_no_wrap(
                            font_info.to_owned(),
                            egui::FontId::proportional(12.0),
                            theme::text_muted(),
                        )
                        .size()
                        .x
                        .min(font_button.left() - 6.0 - font_row.left());
                    widgets::fitted_text(
                        ui.painter(),
                        egui::Rect::from_min_size(
                            font_row.min,
                            egui::vec2(info_width, font_row.height()),
                        ),
                        font_info,
                        egui::FontId::proportional(12.0),
                        theme::text_muted(),
                    );
                    if widgets::text_button(ui, font_button, "inspector-soundfont", "SoundFont...", false)
                        .on_hover_text("Load an SF2 SoundFont for the built-in instrument")
                        .clicked()
                    {
                        self.view = View::Mix;
                        self.status = Some(String::from("SoundFont controls are in the Mixer view"));
                    }

                    widgets::section_label(ui, "Song", 32.0);
                    let response = ui.add_sized(
                        [width, 32.0],
                        egui::TextEdit::singleline(&mut self.song_name_input)
                            .font(egui::FontId::proportional(14.0))
                            .margin(egui::vec2(8.0, 7.0)),
                    );
                    if response.lost_focus() {
                        let name = self.song_name_input.trim().to_string();
                        if name != self.song().name {
                            match self.editor.rename_song(&name) {
                                Ok(()) => self.status = Some(format!("song renamed to {name:?}")),
                                Err(error) => self.status = Some(error),
                            }
                        }
                    }
                    let (tonic, minor) = self.song().key.map_or_else(
                        || {
                            let key = score::spelling::detect_key(self.song());
                            (key.tonic, key.minor)
                        },
                        |key| (key.tonic, key.minor),
                    );
                    let key_name = |tonic: music_core::PitchClass, minor: bool| {
                        format!(
                            "{} {}",
                            if minor { tonic.name_flat() } else { tonic.name_sharp() },
                            if minor { "minor" } else { "major" }
                        )
                    };
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("song-key")
                            .selected_text(key_name(tonic, minor))
                            .width(ui.available_width() - 56.0 - 6.0)
                            .show_ui(ui, |ui| {
                                for wanted_minor in [false, true] {
                                    for semitone in 0..12 {
                                        let pitch = music_core::PitchClass::new(semitone);
                                        if ui
                                            .selectable_label(
                                                pitch == tonic && wanted_minor == minor,
                                                key_name(pitch, wanted_minor),
                                            )
                                            .clicked()
                                            && let Err(error) = self.editor.set_song_key(Some(
                                                project::song::KeySignature {
                                                    tonic: pitch,
                                                    minor: wanted_minor,
                                                },
                                            ))
                                        {
                                            self.status = Some(error);
                                        }
                                    }
                                }
                            });
                        let (find, _) =
                            ui.allocate_exact_size(egui::vec2(56.0, 30.0), egui::Sense::hover());
                        if widgets::text_button(ui, find, "find-key", "Find", false)
                            .on_hover_text("Find the key from the notes of the song (drums left out)")
                            .clicked()
                        {
                            let key = score::spelling::detect_key(self.song());
                            match self.editor.set_song_key(Some(project::song::KeySignature {
                                tonic: key.tonic,
                                minor: key.minor,
                            })) {
                                Ok(()) => {
                                    self.status =
                                        Some(format!("key: {}", key_name(key.tonic, key.minor)));
                                }
                                Err(error) => self.status = Some(error),
                            }
                        }
                    });

                    widgets::section_label(ui, "Metronome", 32.0);
                    let (click_row, _) =
                        ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::hover());
                    if widgets::toggle_switch(
                        ui,
                        click_row,
                        "click-on",
                        "Click on",
                        self.settings.metronome,
                    ) {
                        self.settings.metronome = !self.settings.metronome;
                    }
                    let beats = usize::from(self.song().time_signature.numerator.clamp(1, 12));
                    let current_beat = self.playing.then(|| {
                        let meter = self.song().time_signature;
                        let in_bar = self.position.rem_euclid(meter.bar_ticks().max(1));
                        usize::try_from(in_bar / meter.beat_ticks().max(1)).unwrap_or(0)
                    });
                    for beat in 0..beats {
                        let centre = egui::pos2(
                            click_row.left() + 90.0 + 5.0 + beat as f32 * 16.0,
                            click_row.center().y,
                        );
                        let lit = current_beat == Some(beat);
                        ui.painter().circle_filled(
                            centre,
                            5.0,
                            match (lit, beat) {
                                (true, 0) => theme::accent(),
                                (true, _) => theme::text(),
                                _ => theme::line_strong(),
                            },
                        );
                    }
                    let (accent_row, _) =
                        ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::hover());
                    let toggle_rect = egui::Rect::from_min_size(
                        accent_row.min,
                        egui::vec2(accent_row.width() / 2.0 + 20.0, accent_row.height()),
                    );
                    let accent_on = self.settings.metronome_accent > 0.0;
                    if widgets::toggle_switch(ui, toggle_rect, "accent-on", "Accent", accent_on) {
                        self.settings.metronome_accent = if accent_on { 0.0 } else { 1.0 };
                    }
                    let slider_rect = egui::Rect::from_min_max(
                        egui::pos2(toggle_rect.right() + 6.0, accent_row.top()),
                        accent_row.max,
                    );
                    widgets::flat_slider(
                        ui,
                        slider_rect,
                        "accent-level",
                        &mut self.settings.metronome_accent,
                    );

                    widgets::section_label(ui, "Audio output", 32.0);
                    let device = self
                        .settings
                        .audio_output
                        .clone()
                        .unwrap_or_else(|| String::from("Default output"));
                    let (audio_text, audio_colour) = if let Some(player) = &self.player {
                        let mut details = format!("{:.1} kHz", f64::from(player.sample_rate()) / 1000.0);
                        if let Some(frames) = player.buffer_frames() {
                            details.push_str(&format!(" \u{b7} {frames} samples"));
                        }
                        if let Some(latency) = player.buffer_latency_ms() {
                            details.push_str(&format!(" \u{b7} {latency:.1} ms"));
                        }
                        (format!("{device}\n{details}"), theme::text())
                    } else {
                        (
                            format!("{device}\n{:.1} kHz", f64::from(audio::SAMPLE_RATE) / 1000.0),
                            theme::text_muted(),
                        )
                    };
                    ui.add(egui::Label::new(egui::RichText::new(audio_text).size(13.0).color(audio_colour)).wrap());
                    let (button_rect, _) =
                        ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::hover());
                    if widgets::text_button(
                        ui,
                        button_rect,
                        "inspector-audio",
                        "Audio and MIDI settings",
                        false,
                    )
                    .clicked()
                    {
                        self.settings_open = true;
                    }

                    widgets::section_label(ui, "MIDI inputs", 32.0);
                    let inputs = self
                        .midi_ports
                        .as_ref()
                        .map(|(inputs, _)| inputs.clone())
                        .unwrap_or_default();
                    if inputs.is_empty() {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(
                                    "No MIDI inputs found. Plug in a keyboard, then open Audio and MIDI settings.",
                                )
                                .size(13.0)
                                .color(theme::text_muted()),
                            )
                            .wrap(),
                        );
                    }
                    for input in inputs.iter().take(6) {
                        let (row, _) =
                            ui.allocate_exact_size(egui::vec2(width, 38.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            egui::pos2(row.left() + 7.0, row.top() + 10.0),
                            4.0,
                            theme::ok().gamma_multiply(0.45),
                        );
                        ui.painter().text(
                            egui::pos2(row.left() + 18.0, row.top() + 10.0),
                            egui::Align2::LEFT_CENTER,
                            input,
                            egui::FontId::proportional(13.0),
                            theme::text(),
                        );
                        ui.painter().text(
                            egui::pos2(row.left() + 18.0, row.top() + 28.0),
                            egui::Align2::LEFT_CENTER,
                            "Waiting for notes",
                            egui::FontId::proportional(12.0),
                            theme::text_muted(),
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
    }

    /// The score toolbar, laid out like `ScoreView::resized`: a 52 pt bar
    /// with the title on the left and the controls from the right, 32 pt
    /// high: Print, Export PDF, zoom, staves, tracks, dot, note value,
    /// Write notes and Add. What v1 does not have sits in the space between.
    #[allow(clippy::too_many_arguments)]
    fn score_toolbar(
        score: &mut ScoreUi,
        settings: &mut crate::settings::Settings,
        print_dialog: &mut bool,
        lyrics_open: &mut bool,
        ui: &mut egui::Ui,
        song: &project::song::Song,
        score_tracks: &[(TrackId, String)],
        save_pdf: &mut bool,
    ) {
        let (bar, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 52.0), egui::Sense::hover());
        let mut row =
            egui::Rect::from_center_size(bar.center(), egui::vec2(bar.width() - 24.0, 32.0));
        let mut from_right = |width: f32, gap: f32| {
            let rect =
                egui::Rect::from_min_max(egui::pos2(row.right() - width, row.top()), row.max);
            row.max.x -= width + gap;
            rect
        };
        let print = from_right(90.0, 8.0);
        let pdf = from_right(120.0, 8.0);
        let zoom_in = from_right(32.0, 4.0);
        let zoom_out = from_right(32.0, 12.0);
        let staves = from_right(150.0, 8.0);
        let tracks = from_right(140.0, 12.0);
        let dot = from_right(28.0, 4.0);
        let value = from_right(130.0, 8.0);
        let input = from_right(104.0, 8.0);
        let mark = from_right(64.0, 8.0);
        let title = egui::Rect::from_min_size(row.min, egui::vec2(70.0, row.height()));
        let info = egui::Rect::from_min_max(egui::pos2(title.right(), row.top()), row.max);

        ui.painter().text(
            title.left_center(),
            egui::Align2::LEFT_CENTER,
            "Score",
            theme::semibold(15.0),
            theme::text(),
        );

        if widgets::text_button(ui, print, "score-print", "Print...", false)
            .on_hover_text("Open the score as a PDF, ready to print")
            .clicked()
        {
            *print_dialog = true;
        }
        if widgets::text_button(ui, pdf, "score-pdf", "Export PDF...", false)
            .on_hover_text("Save the score of this track as a PDF file")
            .clicked()
        {
            *save_pdf = true;
        }
        if widgets::text_button(ui, zoom_in, "score-zoom-in", "+", false)
            .on_hover_text("Make the pages bigger")
            .clicked()
        {
            score.zoom = (score.zoom * 1.2).min(2.5);
        }
        if widgets::text_button(ui, zoom_out, "score-zoom-out", "-", false)
            .on_hover_text("Make the pages smaller")
            .clicked()
        {
            score.zoom = (score.zoom / 1.2).max(0.5);
        }

        let staves_text = if score.grand {
            "Grand staff"
        } else if score.one_staff {
            "One staff"
        } else {
            "Staves: automatic"
        };
        egui::ComboBox::from_id_salt("score-staves")
            .selected_text(staves_text)
            .width(staves.width())
            .show_ui(&mut toolbar_child(ui, staves), |ui| {
                for (grand, one, label) in [
                    (false, false, "Staves: automatic"),
                    (false, true, "One staff"),
                    (true, false, "Grand staff"),
                ] {
                    if ui
                        .selectable_label(score.grand == grand && score.one_staff == one, label)
                        .clicked()
                    {
                        score.grand = grand;
                        score.one_staff = one;
                    }
                }
            })
            .response
            .on_hover_text(
                "Show the track on one staff, or on two staves (treble and bass) like piano music",
            );

        let mut toggle = None;
        let mut save_settings = false;
        egui::ComboBox::from_id_salt("score-tracks")
            .selected_text(if score.all_tracks {
                "All tracks"
            } else {
                "Selected track"
            })
            .width(tracks.width())
            .show_ui(&mut toolbar_child(ui, tracks), |ui| {
                ui.selectable_value(&mut score.all_tracks, true, "All tracks");
                ui.selectable_value(&mut score.all_tracks, false, "Selected track");
                if score.all_tracks {
                    ui.separator();
                    for track in &song.tracks {
                        let mut shown = !score.full_hidden.contains(&track.id);
                        if ui.checkbox(&mut shown, &track.name).changed() {
                            toggle = Some((track.id, shown));
                        }
                    }
                    ui.separator();
                    save_settings |= ui.checkbox(&mut score.follow, "Follow").changed();
                    save_settings |=
                        ui.checkbox(&mut score.full_frames, "Frames").changed();
                }
            })
            .response
            .on_hover_text(
                "Show all tracks as a full score, only the selected track, or the tracks you choose",
            );
        if let Some((id, shown)) = toggle {
            if shown {
                score.full_hidden.remove(&id);
            } else {
                score.full_hidden.insert(id);
            }
        }
        if save_settings {
            settings.score_follow = score.follow;
            settings.score_frames = score.full_frames;
            let _ = settings.save(&crate::settings::Settings::path());
        }

        if widgets::text_button(ui, dot, "score-dot", ".", score.dotted)
            .on_hover_text("Dotted note (.): one and a half times as long")
            .clicked()
        {
            score.dotted = !score.dotted;
        }
        const VALUES: [(music_core::Tick, &str); 6] = [
            (music_core::PPQ * 4, "Whole note"),
            (music_core::PPQ * 2, "Half note"),
            (music_core::PPQ, "Quarter note"),
            (music_core::PPQ / 2, "Eighth note"),
            (music_core::PPQ / 4, "16th note"),
            (music_core::PPQ / 8, "32nd note"),
        ];
        let value_text = VALUES
            .iter()
            .find(|(length, _)| score.note_length >= *length)
            .map_or("32nd note", |(_, label)| label);
        egui::ComboBox::from_id_salt("score-length")
            .selected_text(value_text)
            .width(value.width())
            .show_ui(&mut toolbar_child(ui, value), |ui| {
                for (length, label) in VALUES {
                    ui.selectable_value(&mut score.note_length, length, label);
                }
            })
            .response
            .on_hover_text("The length of new notes (keys 2 to 7)");

        if widgets::text_button(ui, input, "score-input", "Write notes", score.entry)
            .on_hover_text(
                "Note input (N): click a staff, push A to G, or play your MIDI keyboard to write notes",
            )
            .clicked()
        {
            score.entry = !score.entry;
            score.preview = !score.entry;
        }
        let mark_response = widgets::text_button(ui, mark, "score-mark", "Add...", false)
            .on_hover_text("Add lyrics at the selected note");
        let mark_popup = ui.make_persistent_id("score-mark-popup");
        if mark_response.clicked() {
            ui.memory_mut(|memory| memory.toggle_popup(mark_popup));
        }
        egui::popup_below_widget(
            ui,
            mark_popup,
            &mark_response,
            egui::PopupCloseBehavior::CloseOnClick,
            |ui| {
                ui.set_min_width(140.0);
                if ui.button("Lyrics...  (L)").clicked() {
                    *lyrics_open = true;
                }
            },
        );

        // The space between: the track to write on, the octave, rests and
        // pages, which v1 does through the selection and scrolling.
        let pages = score.state.cecilia_page_count.max(1);
        let pdf_title = if song.name.is_empty() {
            "Lazy Arranger score"
        } else {
            song.name.as_str()
        };
        let unencodable = score::pdf::unencodable_text(song, pdf_title);
        let extras = !score.all_tracks || score.entry || pages > 1 || unencodable.is_some();
        if !extras {
            return;
        }
        // In a narrow window the extras get a row of their own under the bar.
        let info = if info.width() >= 280.0 {
            info
        } else {
            let (row, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::hover());
            row.shrink2(egui::vec2(12.0, 2.0))
        };
        let mut info_ui = toolbar_child(ui, info);
        info_ui.spacing_mut().item_spacing.x = 6.0;
        if !score.all_tracks {
            let selected = score
                .track
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
                .width(120.0)
                .show_ui(&mut info_ui, |ui| {
                    for (id, name) in score_tracks {
                        ui.selectable_value(&mut score.track, Some(*id), name);
                    }
                });
        }
        if score.entry {
            info_ui
                .add(
                    egui::DragValue::new(&mut score.octave)
                        .range(0..=8)
                        .prefix("Oct "),
                )
                .on_hover_text("The octave of notes typed with A to G");
            if info_ui
                .button("Rest")
                .on_hover_text("Write a rest: move the cursor on (0)")
                .clicked()
            {
                score.cursor += entry_length(score.note_length.max(1), score.dotted);
            }
        }
        if pages > 1 {
            if info_ui.small_button("\u{25c0}").clicked() {
                score.page = score.page.saturating_sub(1);
            }
            info_ui.label(
                egui::RichText::new(format!("{}/{pages}", score.page + 1))
                    .size(13.0)
                    .color(theme::text_muted()),
            );
            if info_ui.small_button("\u{25b6}").clicked() {
                score.page += 1;
            }
        }
        if let Some(text) = unencodable {
            info_ui
                .label(
                    egui::RichText::new("PDF: ?")
                        .size(12.0)
                        .color(theme::recording()),
                )
                .on_hover_text(format!(
                    "WinAnsi PDF cannot encode {text:?}; it will print as ?"
                ));
        }
    }

    /// The Loops view, drawn like v1 `ClipGridView`: a 52 pt header, track
    /// names over the columns, a scene column with launch buttons, 46 pt
    /// slots in the track colour with a picture of the notes, and a stop
    /// row under the grid. Each track holds its own clip per scene.
    pub(super) fn grid_panel(&mut self, ui: &mut egui::Ui) {
        const HEADER: f32 = 52.0;
        const NAMES: f32 = 36.0;
        const SCENE_COLUMN: f32 = 132.0;
        const ROW: f32 = 46.0;
        const STOP_ROW: f32 = 36.0;
        const BUTTON: f32 = 30.0;
        let song = self.song().clone();
        let tracks = &song.tracks;
        let scenes = song.scene_count();
        let bar = song.time_signature.bar_ticks().max(1);
        let selected_track = self.roll.selected;
        if self
            .slot_edit
            .is_some_and(|(scene, track)| track_slot(&song, track, scene).is_none())
        {
            self.slot_edit = None;
        }
        // MainComponent: the editor docks under the view, 45% of the height.
        let total = ui.available_height();
        let editor_height = if self.slot_edit.is_some() {
            (total * 0.45).clamp(240.0_f32.min(total / 2.0), 520.0)
        } else {
            0.0
        };
        let (full, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), total - editor_height),
            egui::Sense::hover(),
        );
        let mut changed = false;

        // Header: title, Loops start, From Timeline, Record to Timeline, Stop all.
        let header = egui::Rect::from_min_size(full.min, egui::vec2(full.width(), HEADER))
            .shrink2(egui::vec2(14.0, 10.0));
        let stop_all =
            egui::Rect::from_min_max(egui::pos2(header.right() - 84.0, header.top()), header.max);
        let record = egui::Rect::from_min_max(
            egui::pos2(stop_all.left() - 8.0 - 156.0, header.top()),
            egui::pos2(stop_all.left() - 8.0, header.bottom()),
        );
        let from_timeline = egui::Rect::from_min_max(
            egui::pos2(record.left() - 8.0 - 128.0, header.top()),
            egui::pos2(record.left() - 8.0, header.bottom()),
        );
        let launch_box = egui::Rect::from_min_max(
            egui::pos2(from_timeline.left() - 16.0 - 140.0, header.top()),
            egui::pos2(from_timeline.left() - 16.0, header.bottom()),
        );
        ui.painter().text(
            header.left_center(),
            egui::Align2::LEFT_CENTER,
            "Loops",
            egui::FontId::proportional(15.0),
            theme::text(),
        );
        ui.painter().text(
            egui::pos2(launch_box.left() - 6.0, launch_box.center().y),
            egui::Align2::RIGHT_CENTER,
            "Loops start:",
            egui::FontId::proportional(13.0),
            theme::text_muted(),
        );
        let quantise_label = |bars: u32| match bars {
            1 => String::from("Next bar"),
            bars => format!("Next {bars} bars"),
        };
        let mut launch_ui = toolbar_child(ui, launch_box);
        egui::ComboBox::from_id_salt("loops-start")
            .width(launch_box.width() - 8.0)
            .selected_text(quantise_label(self.loops_quantise_bars))
            .show_ui(&mut launch_ui, |ui| {
                for bars in [1, 2, 4] {
                    ui.selectable_value(&mut self.loops_quantise_bars, bars, quantise_label(bars));
                }
            })
            .response
            .on_hover_text("When a clip or scene starts after you click it");
        if widgets::text_button(
            ui,
            from_timeline,
            "loops-from-timeline",
            "From Timeline...",
            false,
        )
        .on_hover_text("Put the Timeline in Loops: each part of the song becomes a scene")
        .clicked()
        {
            self.loops_fill = Some(4);
        }
        let recording = self.jam_recording;
        let record_response =
            widgets::text_button(ui, record, "loops-record", "Record to Timeline", recording);
        if recording {
            ui.painter()
                .rect_filled(record, theme::RADIUS, theme::recording());
            ui.painter().text(
                record.center(),
                egui::Align2::CENTER_CENTER,
                "Record to Timeline",
                egui::FontId::proportional(13.0),
                egui::Color32::WHITE,
            );
        }
        if record_response
            .on_hover_text(
                "Write what the Loops view plays to the Timeline. Click again, or stop, to finish.",
            )
            .clicked()
        {
            self.set_jam_recording(!recording);
        }
        if widgets::text_button(ui, stop_all, "loops-stop-all", "Stop all", false)
            .on_hover_text("Stop all clips. The Timeline plays again.")
            .clicked()
        {
            self.launcher.stop_all();
            changed = true;
        }
        if tracks.is_empty() {
            ui.painter().text(
                egui::pos2(full.center().x, full.top() + HEADER + 60.0),
                egui::Align2::CENTER_CENTER,
                "Add a track to use Loops.",
                egui::FontId::proportional(15.0),
                theme::text_dim(),
            );
            self.loops_dialogs(ui);
            return;
        }

        // The grid area and scrolling.
        let area = egui::Rect::from_min_max(
            egui::pos2(full.left() + SCENE_COLUMN, full.top() + HEADER + NAMES),
            egui::pos2(full.right() - 10.0, full.bottom() - STOP_ROW - 8.0),
        );
        let column = (area.width() / tracks.len() as f32).clamp(118.0, 200.0);
        let scroll_id = ui.id().with("loops-scroll");
        let mut scroll: egui::Vec2 = ui.data(|data| data.get_temp(scroll_id)).unwrap_or_default();
        if ui.rect_contains_pointer(full) {
            let delta = ui.input(|input| input.smooth_scroll_delta);
            scroll -= delta;
        }
        scroll.x = scroll
            .x
            .clamp(0.0, (tracks.len() as f32 * column - area.width()).max(0.0));
        scroll.y = scroll
            .y
            .clamp(0.0, (scenes as f32 * ROW - area.height()).max(0.0));
        ui.data_mut(|data| data.insert_temp(scroll_id, scroll));
        let cell = |track: usize, scene: usize| {
            egui::Rect::from_min_size(
                egui::pos2(
                    area.left() + track as f32 * column - scroll.x,
                    area.top() + scene as f32 * ROW - scroll.y,
                ),
                egui::vec2(column, ROW),
            )
            .shrink(1.0)
        };
        let ink = if theme::light() {
            theme::text()
        } else {
            egui::Color32::from_rgb(0x11, 0x14, 0x17)
        };
        let blink = (ui.input(|input| input.time) * 2.0).fract() < 0.5;
        if self.launcher.active() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50));
        }
        enum SlotAction {
            New,
            Edit,
            Rename,
            Length(Tick),
            CopyToTimeline,
            CopyToFree,
            Cut,
            Copy,
            Paste,
            Delete,
        }
        let mut slot_action: Option<(usize, TrackId, SlotAction)> = None;
        let mut rename_scene = None;

        // Slots.
        let slots = ui.painter().with_clip_rect(area);
        for scene in 0..scenes {
            for (index, track) in tracks.iter().enumerate() {
                let rect = cell(index, scene);
                if !rect.intersects(area) {
                    continue;
                }
                let colour = theme::track_colour(index);
                let clip = track.slot(scene);
                let playing = self.launcher.playing.get(&track.id) == Some(&scene);
                let pending = self.launcher.pending.get(&track.id).copied();
                let waiting = pending == Some(Some(scene));
                let active = self.launcher.playing.contains_key(&track.id)
                    || matches!(pending, Some(Some(_)));
                let chosen = self.chosen_slot == Some((scene, track.id));
                let button = egui::Rect::from_min_size(rect.min, egui::vec2(BUTTON, rect.height()));
                if let Some(clip) = clip {
                    slots.rect_filled(
                        rect,
                        theme::RADIUS,
                        if playing || chosen {
                            colour.lerp_to_gamma(egui::Color32::WHITE, 0.12)
                        } else {
                            colour
                        },
                    );
                    let length = clip.length.max(1);
                    let picture = egui::Rect::from_min_max(
                        egui::pos2(rect.left() + 34.0, rect.top() + rect.height() * 0.55),
                        egui::pos2(rect.right() - 6.0, rect.bottom() - 5.0),
                    );
                    if let (Some(low), Some(high)) = (
                        clip.notes.iter().map(|note| note.pitch.as_u8()).min(),
                        clip.notes.iter().map(|note| note.pitch.as_u8()).max(),
                    ) {
                        let range = f32::from((high - low).max(1));
                        for note in clip.notes.iter().filter(|note| note.start < length) {
                            let x = picture.left()
                                + picture.width() * note.start as f32 / length as f32;
                            let width = (picture.width() * note.duration as f32 / length as f32)
                                .max(2.0)
                                .min(picture.right() - x);
                            let y = picture.bottom()
                                - 2.0
                                - (picture.height() - 2.0) * f32::from(note.pitch.as_u8() - low)
                                    / range;
                            slots.rect_filled(
                                egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, 2.0)),
                                0.0,
                                ink.gamma_multiply(0.82),
                            );
                        }
                    }
                    let name = if clip.name.is_empty() {
                        "Clip"
                    } else {
                        clip.name.as_str()
                    };
                    let name_width = slots
                        .layout_no_wrap(name.to_owned(), theme::semibold(13.0), ink)
                        .size()
                        .x
                        .min(rect.width() - 40.0);
                    widgets::fitted_text(
                        &slots,
                        egui::Rect::from_min_size(
                            egui::pos2(rect.left() + 34.0, rect.top()),
                            egui::vec2(name_width, rect.height() * 0.58),
                        ),
                        name,
                        theme::semibold(13.0),
                        ink,
                    );
                    if playing {
                        // How far the loop is.
                        let since = self.launcher.since.get(&track.id).copied().unwrap_or(0);
                        let phase =
                            (self.position - since).rem_euclid(length) as f32 / length as f32;
                        let progress = egui::Rect::from_min_max(
                            egui::pos2(rect.left() + 34.0, rect.bottom() - 5.0),
                            egui::pos2(rect.right() - 6.0, rect.bottom() - 2.0),
                        );
                        slots.rect_filled(
                            egui::Rect::from_min_size(
                                progress.min,
                                egui::vec2(progress.width() * phase, progress.height()),
                            ),
                            0.0,
                            ink.gamma_multiply(0.6),
                        );
                    }
                    let lit = playing || (waiting && blink);
                    crate::transport::draw_transport_icon(
                        &slots,
                        button.shrink2(egui::vec2(8.0, (button.height() - 14.0) / 2.0)),
                        crate::transport::PLAY,
                        if playing {
                            ink
                        } else if lit {
                            theme::ok()
                        } else {
                            ink.gamma_multiply(0.75)
                        },
                    );
                    if waiting {
                        slots.rect_stroke(
                            rect,
                            theme::RADIUS,
                            egui::Stroke::new(
                                2.0_f32,
                                theme::ok().gamma_multiply(if blink { 1.0 } else { 0.35 }),
                            ),
                            egui::StrokeKind::Inside,
                        );
                    } else if chosen {
                        slots.rect_stroke(
                            rect,
                            theme::RADIUS,
                            egui::Stroke::new(1.5_f32, theme::accent()),
                            egui::StrokeKind::Inside,
                        );
                    }
                } else {
                    slots.rect_filled(rect, theme::RADIUS, theme::background());
                    slots.rect_stroke(
                        rect,
                        theme::RADIUS,
                        egui::Stroke::new(
                            if chosen { 1.5_f32 } else { 1.0 },
                            if chosen { colour } else { theme::line() },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    // An empty slot stops the track, as on a hardware pad grid.
                    if active {
                        crate::transport::draw_transport_icon(
                            &slots,
                            button.shrink2(egui::vec2(9.0, (button.height() - 12.0) / 2.0)),
                            crate::transport::STOP,
                            theme::text_dim(),
                        );
                    }
                }
                let response = ui.interact(
                    rect.intersect(area),
                    ui.id().with(("loop-slot", scene, index)),
                    egui::Sense::click(),
                );
                let on_button = response
                    .interact_pointer_pos()
                    .or(response.hover_pos())
                    .is_some_and(|pos| button.contains(pos));
                let response = response.on_hover_text(if clip.is_some() {
                    "Click the play button to start or stop this clip; right-click for more"
                } else {
                    "Double-click to make a clip; an empty slot's button stops the track"
                });
                if response.double_clicked() && !on_button {
                    if clip.is_none() {
                        slot_action = Some((scene, track.id, SlotAction::New));
                    } else {
                        slot_action = Some((scene, track.id, SlotAction::Edit));
                    }
                } else if response.clicked() {
                    self.chosen_slot = Some((scene, track.id));
                    self.roll.selected = Some(track.id);
                    if on_button {
                        if clip.is_some() {
                            self.launcher.toggle(track.id, scene);
                        } else {
                            self.launcher.stop(track.id);
                        }
                        changed = true;
                    }
                }
                if response.secondary_clicked() {
                    self.chosen_slot = Some((scene, track.id));
                    self.roll.selected = Some(track.id);
                }
                let filled = clip.is_some();
                let clip_length = clip.map(|clip| clip.length);
                let can_paste = self.slot_clipboard.is_some();
                response.context_menu(|ui| {
                    let track = track.id;
                    let pick = |ui: &mut egui::Ui, label: &str, enabled: bool| {
                        let clicked = ui.add_enabled(enabled, egui::Button::new(label)).clicked();
                        if clicked {
                            ui.close_menu();
                        }
                        clicked
                    };
                    if pick(ui, "Cut", filled) {
                        slot_action = Some((scene, track, SlotAction::Cut));
                    }
                    if pick(ui, "Copy", filled) {
                        slot_action = Some((scene, track, SlotAction::Copy));
                    }
                    if pick(ui, "Paste", can_paste) {
                        slot_action = Some((scene, track, SlotAction::Paste));
                    }
                    ui.separator();
                    if filled {
                        if ui.button("Start").clicked() {
                            self.launcher.toggle(track, scene);
                            changed = true;
                            ui.close_menu();
                        }
                        if pick(ui, "Edit notes", true) {
                            slot_action = Some((scene, track, SlotAction::Edit));
                        }
                        if pick(ui, "Rename", true) {
                            slot_action = Some((scene, track, SlotAction::Rename));
                        }
                        ui.separator();
                        ui.menu_button("Length", |ui| {
                            for bars in [1, 2, 4, 8] {
                                let label = if bars == 1 {
                                    String::from("1 bar")
                                } else {
                                    format!("{bars} bars")
                                };
                                let length = bar * bars;
                                if ui
                                    .add(
                                        egui::Button::new(label)
                                            .selected(clip_length == Some(length)),
                                    )
                                    .clicked()
                                {
                                    slot_action = Some((scene, track, SlotAction::Length(length)));
                                    ui.close_menu();
                                }
                            }
                        });
                        if pick(ui, "Copy to the Timeline at the playhead", true) {
                            slot_action = Some((scene, track, SlotAction::CopyToTimeline));
                        }
                        if pick(ui, "Copy to the next free slot", true) {
                            slot_action = Some((scene, track, SlotAction::CopyToFree));
                        }
                        ui.separator();
                        if pick(ui, "Delete", true) {
                            slot_action = Some((scene, track, SlotAction::Delete));
                        }
                    } else {
                        if pick(ui, "New clip", true) {
                            slot_action = Some((scene, track, SlotAction::New));
                        }
                    }
                });
            }
        }

        // Scene names, with a launch button.
        let scene_clip = egui::Rect::from_min_max(
            egui::pos2(full.left(), area.top()),
            egui::pos2(full.left() + SCENE_COLUMN, area.bottom()),
        );
        let names = ui.painter().with_clip_rect(scene_clip);
        for scene in 0..scenes {
            let rect = egui::Rect::from_min_size(
                egui::pos2(full.left(), area.top() + scene as f32 * ROW - scroll.y),
                egui::vec2(SCENE_COLUMN, ROW),
            )
            .shrink(1.0);
            let rect =
                egui::Rect::from_min_max(egui::pos2(rect.left() + 11.0, rect.top()), rect.max);
            if !rect.intersects(scene_clip) {
                continue;
            }
            let filled: Vec<TrackId> = tracks
                .iter()
                .filter(|track| track.slot(scene).is_some())
                .map(|track| track.id)
                .collect();
            let any = !filled.is_empty();
            let all_playing = any
                && filled
                    .iter()
                    .all(|track| self.launcher.playing.get(track) == Some(&scene));
            names.rect_filled(rect, theme::RADIUS, theme::raised());
            crate::transport::draw_transport_icon(
                &names,
                egui::Rect::from_min_size(rect.min, egui::vec2(BUTTON, rect.height()))
                    .shrink2(egui::vec2(8.0, (rect.height() - 14.0) / 2.0)),
                crate::transport::PLAY,
                if all_playing {
                    theme::ok()
                } else if any {
                    theme::text().gamma_multiply(0.8)
                } else {
                    theme::text_dim().gamma_multiply(0.5)
                },
            );
            let label = song.scene_name(scene);
            let label_width = names
                .layout_no_wrap(
                    label.clone(),
                    egui::FontId::proportional(13.0),
                    theme::text(),
                )
                .size()
                .x
                .min(rect.width() - 36.0);
            widgets::fitted_text(
                &names,
                egui::Rect::from_min_size(
                    egui::pos2(rect.left() + 32.0, rect.top()),
                    egui::vec2(label_width, rect.height()),
                ),
                &label,
                egui::FontId::proportional(13.0),
                if any {
                    theme::text()
                } else {
                    theme::text_dim()
                },
            );
            let response = ui
                .interact(
                    rect.intersect(scene_clip),
                    ui.id().with(("loop-scene", scene)),
                    egui::Sense::click(),
                )
                .on_hover_text("Click to start the scene; double-click to rename it");
            if response.double_clicked() {
                rename_scene = Some(scene);
            } else if response.clicked() {
                self.launcher.launch_scene(&song, scene);
                changed = true;
            }
            response.context_menu(|ui| {
                if ui.button("Start").clicked() {
                    self.launcher.launch_scene(&song, scene);
                    changed = true;
                    ui.close_menu();
                }
                if ui.button("Rename").clicked() {
                    rename_scene = Some(scene);
                    ui.close_menu();
                }
            });
        }

        // Track names above, stop buttons below.
        let columns_clip = egui::Rect::from_min_max(
            egui::pos2(area.left(), full.top() + HEADER),
            egui::pos2(area.right(), full.bottom()),
        );
        let heads = ui.painter().with_clip_rect(columns_clip);
        let stop_top = (area.top() + scenes as f32 * ROW - scroll.y).min(area.bottom()) + 4.0;
        for (index, track) in tracks.iter().enumerate() {
            let left = area.left() + index as f32 * column - scroll.x;
            let name = egui::Rect::from_min_size(
                egui::pos2(left, full.top() + HEADER),
                egui::vec2(column, NAMES),
            )
            .shrink2(egui::vec2(1.0, 4.0));
            heads.rect_filled(
                egui::Rect::from_min_max(egui::pos2(name.left(), name.bottom() - 3.0), name.max),
                theme::RADIUS,
                theme::track_colour(index),
            );
            let chosen = selected_track == Some(track.id);
            heads.text(
                egui::pos2(name.left() + 4.0, name.center().y - 2.0),
                egui::Align2::LEFT_CENTER,
                &track.name,
                if chosen {
                    theme::semibold(13.0)
                } else {
                    egui::FontId::proportional(13.0)
                },
                if chosen {
                    theme::text()
                } else {
                    theme::text_muted()
                },
            );
            let stop =
                egui::Rect::from_min_size(egui::pos2(left, stop_top), egui::vec2(column, STOP_ROW))
                    .shrink(1.0);
            let pending = self.launcher.pending.get(&track.id).copied();
            let playing = self.launcher.playing.contains_key(&track.id);
            let active = playing || matches!(pending, Some(Some(_)));
            heads.rect_filled(stop, theme::RADIUS, theme::raised());
            crate::transport::draw_transport_icon(
                &heads,
                egui::Rect::from_min_size(stop.min, egui::vec2(BUTTON, stop.height()))
                    .shrink2(egui::vec2(8.0, (stop.height() - 14.0) / 2.0)),
                crate::transport::STOP,
                if active {
                    theme::text()
                } else {
                    theme::text_dim().gamma_multiply(0.5)
                },
            );
            heads.text(
                egui::pos2(stop.left() + 32.0, stop.center().y),
                egui::Align2::LEFT_CENTER,
                if pending == Some(None) {
                    "Stops soon"
                } else if playing {
                    "Stop"
                } else {
                    "Timeline"
                },
                egui::FontId::proportional(12.0),
                if active {
                    theme::text_muted()
                } else {
                    theme::text_dim()
                },
            );
            if ui
                .interact(
                    stop.intersect(columns_clip),
                    ui.id().with(("loop-stop", index)),
                    egui::Sense::click(),
                )
                .on_hover_text("Stop this track's clip at the next bar")
                .clicked()
            {
                self.launcher.stop(track.id);
                changed = true;
            }
        }

        if let Some(scene) = rename_scene {
            self.rename_scene = Some(scene);
            self.rename_scene_input = song.scene_name(scene);
        }
        if let Some((scene, track, action)) = slot_action {
            let clip = track_slot(&song, track, scene).cloned();
            let result = match action {
                SlotAction::New => self.editor.new_slot(scene, track, bar).map(|()| {
                    let _ = self.editor.rename_slot(scene, track, "Clip");
                    self.open_slot_editor(scene, track);
                }),
                SlotAction::Edit => {
                    self.open_slot_editor(scene, track);
                    Ok(())
                }
                SlotAction::Rename => {
                    self.rename_slot = Some((scene, track));
                    self.rename_scene_input = clip.map(|clip| clip.name).unwrap_or_default();
                    Ok(())
                }
                SlotAction::Length(length) => self.editor.set_slot_length(scene, track, length),
                SlotAction::CopyToTimeline => match clip {
                    Some(clip) => {
                        let at = self.position.max(0).div_euclid(bar) * bar;
                        self.editor
                            .record_loops(
                                &[
                                    arranging::loops::Change {
                                        track,
                                        scene: Some(scene),
                                        tick: at,
                                        origin: at,
                                    },
                                    arranging::loops::Change {
                                        track,
                                        scene: None,
                                        tick: at + clip.length,
                                        origin: at + clip.length,
                                    },
                                ],
                                at + clip.length,
                            )
                            .map(|_| ())
                    }
                    None => Ok(()),
                },
                SlotAction::CopyToFree => {
                    let free = (scene + 1..)
                        .find(|candidate| track_slot(&song, track, *candidate).is_none())
                        .unwrap_or(scene + 1);
                    self.editor.set_slot(free, track, clip)
                }
                SlotAction::Cut => {
                    self.slot_clipboard = clip;
                    self.editor.clear_slot(scene, track)
                }
                SlotAction::Copy => {
                    self.slot_clipboard = clip;
                    Ok(())
                }
                SlotAction::Paste => {
                    let pasted = self.slot_clipboard.clone();
                    self.editor.set_slot(scene, track, pasted)
                }
                SlotAction::Delete => self.editor.clear_slot(scene, track),
            };
            if let Err(error) = result {
                self.status = Some(error);
            }
        }
        if changed {
            self.loops_changed();
        }
        if self.slot_edit.is_some() {
            self.slot_editor(ui);
        }
        self.loops_dialogs(ui);
    }

    /// Opens a Loops clip in the docked piano roll.
    fn open_slot_editor(&mut self, scene: usize, track: TrackId) {
        self.slot_edit = Some((scene, track));
        self.slot_roll.fitted = None;
        self.slot_note = None;
        self.chosen_slot = Some((scene, track));
    }

    /// The piano roll docked under the Loops grid, editing one clip's notes.
    /// Notes are measured from the clip start; each change is one undo step.
    fn slot_editor(&mut self, ui: &mut egui::Ui) {
        let Some((scene, track_id)) = self.slot_edit else {
            return;
        };
        let song = self.song().clone();
        let Some(index) = song.tracks.iter().position(|track| track.id == track_id) else {
            self.slot_edit = None;
            return;
        };
        let Some(clip) = song.tracks[index].slot(scene).cloned() else {
            self.slot_edit = None;
            return;
        };
        // A one-track song holding the clip, so the roll draws it unchanged.
        let mut view = song.clone();
        let channel = song.tracks[index]
            .notes
            .first()
            .or(clip.notes.first())
            .map_or_else(music_core::Channel::first, |note| note.channel);
        let mut track = view.tracks[index].clone();
        track.notes = clip.notes.clone();
        track.clips = Vec::new();
        view.tracks = vec![track];
        view.chords.clear();
        view.lyrics.clear();
        view.markers.clear();
        let since = self.launcher.since.get(&track_id).copied();
        let playhead = match since {
            Some(since) if self.launcher.playing.get(&track_id) == Some(&scene) => {
                (self.position - since).rem_euclid(clip.length.max(1))
            }
            _ => -1,
        };

        let (line, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter().rect_filled(line, 0.0, theme::line_strong());
        let (bar, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), egui::Sense::hover());
        let mut bar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar.shrink2(egui::vec2(12.0, 3.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        theme::v1_fields(bar_ui.visuals_mut());
        let (dot, _) = bar_ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
        bar_ui
            .painter()
            .circle_filled(dot.center(), 5.0, theme::track_colour(index));
        let name = if clip.name.is_empty() {
            "Clip"
        } else {
            clip.name.as_str()
        };
        bar_ui.label(
            egui::RichText::new(name)
                .font(theme::semibold(14.0))
                .color(theme::text()),
        );
        bar_ui.add_space(24.0);
        bar_ui.label(
            egui::RichText::new(format!(
                "{} \u{b7} {} \u{b7} {} notes",
                song.tracks[index].name,
                song.scene_name(scene),
                clip.notes.len()
            ))
            .size(12.0)
            .color(theme::text_muted()),
        );
        let mut close = false;
        bar_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = ui
                .add_sized([64.0, 28.0], egui::Button::new("Close"))
                .on_hover_text("Close the piano roll")
                .clicked();
            ui.add_space(4.0);
            egui::ComboBox::from_id_salt("slot-roll-grid")
                .selected_text(if self.slot_roll.snap { "1/16" } else { "Off" })
                .width(72.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.slot_roll.snap, true, "1/16");
                    ui.selectable_value(&mut self.slot_roll.snap, false, "Off");
                });
        });
        if close {
            self.slot_edit = None;
            return;
        }

        self.slot_roll.selected = Some(track_id);
        if self.slot_roll.fitted != Some(track_id) {
            self.slot_roll
                .fit(&view, track_id, ui.available_width() - 60.0);
        }
        let outcome = crate::piano_roll::show(ui, &view, &mut self.slot_roll, playhead, &[]);
        self.slot_roll.band = None;

        let mut notes = clip.notes.clone();
        let mut dirty = false;
        if let Some(click) = outcome.click {
            if let Some((_, note)) = click.note {
                self.slot_note = Some(note);
            } else if let Ok(pitch) = music_core::Pitch::new(click.pitch) {
                let start = if self.slot_roll.snap {
                    (click.tick.max(0) / (PPQ / 4)) * (PPQ / 4)
                } else {
                    click.tick.max(0)
                };
                if start < clip.length {
                    let id = notes.iter().map(|note| note.id.get()).max().unwrap_or(0) + 1;
                    if let Ok(id) = project::NoteId::new(id)
                        && let Ok(velocity) = music_core::Velocity::new(100)
                    {
                        notes.push(project::song::Note {
                            id,
                            start,
                            duration: PPQ.min(clip.length - start),
                            pitch,
                            velocity,
                            channel,
                        });
                        self.slot_note = Some(id);
                        dirty = true;
                    }
                }
            }
        }
        if let Some(edit) = outcome.edit
            && let Some(note) = notes.iter_mut().find(|note| note.id == edit.note)
        {
            match edit.mode {
                crate::piano_roll::DragMode::Resize => note.duration = edit.duration.max(1),
                crate::piano_roll::DragMode::Move => {
                    if let Ok(pitch) = music_core::Pitch::new(edit.pitch) {
                        note.pitch = pitch;
                    }
                    note.start = edit.start.clamp(0, clip.length - 1);
                }
            }
            dirty = true;
        }
        if let Some((_, id, velocity)) = outcome.velocity
            && let Some(note) = notes.iter_mut().find(|note| note.id == id)
            && let Ok(velocity) = music_core::Velocity::new(velocity.clamp(1, 127))
        {
            note.velocity = velocity;
            dirty = true;
        }
        if let Some(pitch) = outcome.preview {
            self.preview_pitch(pitch);
        }
        if let Some(selected) = self.slot_note
            && !ui.ctx().wants_keyboard_input()
            && ui.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
            })
        {
            notes.retain(|note| note.id != selected);
            self.slot_note = None;
            dirty = true;
        }
        if dirty && let Err(error) = self.editor.set_slot_notes(scene, track_id, notes) {
            self.status = Some(error);
        }
    }

    /// The Loops view's dialogs: From Timeline, rename a scene or a clip.
    fn loops_dialogs(&mut self, ui: &mut egui::Ui) {
        if let Some(mut bars) = self.loops_fill {
            let mut open = true;
            let mut fill = false;
            let mut cancel = false;
            egui::Window::new("Put the Timeline in Loops")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    ui.label(
                        "Each part of the song becomes a scene. Clips already in those \
                         scenes are replaced.",
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Bars in each scene");
                        egui::ComboBox::from_id_salt("loops-fill-bars")
                            .selected_text(if bars == 1 {
                                String::from("1 bar")
                            } else {
                                format!("{bars} bars")
                            })
                            .show_ui(ui, |ui| {
                                for choice in arranging::loops::SCENE_BARS {
                                    let label = if choice == 1 {
                                        String::from("1 bar")
                                    } else {
                                        format!("{choice} bars")
                                    };
                                    ui.selectable_value(&mut bars, choice, label);
                                }
                            });
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        fill = ui.button("Fill").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            self.loops_fill = (open && !cancel).then_some(bars);
            if fill {
                self.loops_fill = None;
                if self.song().end_tick() == 0 {
                    self.status = Some(String::from("the Timeline has no notes yet"));
                } else {
                    match self.editor.loops_from_timeline(bars) {
                        Ok(scenes) => {
                            self.status = Some(format!("{scenes} scene(s) from the Timeline"))
                        }
                        Err(error) => self.status = Some(error),
                    }
                }
            }
        }
        let renaming = self
            .rename_scene
            .map(|scene| (String::from("Rename scene"), Some(scene), None))
            .or_else(|| {
                self.rename_slot
                    .map(|slot| (String::from("Rename clip"), None, Some(slot)))
            });
        if let Some((title, scene, slot)) = renaming {
            let mut open = true;
            let mut apply = false;
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.rename_scene_input)
                            .desired_width(200.0),
                    );
                    edit.request_focus();
                    apply = (edit.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                        | ui.button("Apply").clicked();
                });
            if apply {
                let name = self.rename_scene_input.clone();
                let result = match (scene, slot) {
                    (Some(scene), _) => self.editor.rename_scene(scene, &name),
                    (None, Some((scene, track))) => self.editor.rename_slot(scene, track, &name),
                    (None, None) => Ok(()),
                };
                if let Err(error) = result {
                    self.status = Some(error);
                }
                open = false;
            }
            if !open {
                self.rename_scene = None;
                self.rename_slot = None;
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
                            // MainComponent::resized: the editor docks under the
                            // timeline, 45% of the height (240 to 520 pt).
                            let total = ui.available_height();
                            let editor_height =
                                (total * 0.45).clamp(240.0_f32.min(total / 2.0), 520.0);
                            let timeline_height = (total - editor_height - 6.0).max(60.0);
                            let (_, docked_track, docked_edit) = egui::ScrollArea::vertical()
                                .id_salt("docked-timeline")
                                .max_height(timeline_height)
                                .min_scrolled_height(timeline_height)
                                .show(ui, |ui| {
                                    crate::timeline::show(
                                        ui,
                                        &project.song,
                                        position,
                                        roll.selected,
                                        timeline_zoom,
                                        self.playing,
                                        &self.timeline_waveforms,
                                    )
                                })
                                .inner;
                            let (line, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 1.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(line, 0.0, theme::line_strong());
                            // The editor bar: the track's dot and name, its
                            // note count, the grid and Close, like v1.
                            let edited = roll.selected.and_then(|id| {
                                project
                                    .song
                                    .tracks
                                    .iter()
                                    .position(|track| track.id == id)
                                    .map(|index| (index, &project.song.tracks[index]))
                            });
                            let (bar, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 36.0),
                                egui::Sense::hover(),
                            );
                            let mut bar_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(bar.shrink2(egui::vec2(12.0, 3.0)))
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            );
                            theme::v1_fields(bar_ui.visuals_mut());
                            if let Some((index, track)) = edited {
                                let (dot, _) = bar_ui
                                    .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                                bar_ui.painter().circle_filled(
                                    dot.center(),
                                    5.0,
                                    theme::track_colour(index),
                                );
                                bar_ui.label(
                                    egui::RichText::new(&track.name)
                                        .font(theme::semibold(14.0))
                                        .color(theme::text()),
                                );
                                bar_ui.add_space(24.0);
                                bar_ui.label(
                                    egui::RichText::new(format!(
                                        "{} \u{b7} {} notes",
                                        track.name,
                                        track.notes.len()
                                    ))
                                    .size(12.0)
                                    .color(theme::text_muted()),
                                );
                            } else {
                                bar_ui.label(
                                    egui::RichText::new("Select a track to edit its notes")
                                        .size(12.0)
                                        .color(theme::text_muted()),
                                );
                            }
                            bar_ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_sized([64.0, 28.0], egui::Button::new("Close"))
                                        .on_hover_text("Close the piano roll")
                                        .clicked()
                                    {
                                        self.timeline_mode = true;
                                        roll.fitted = None;
                                    }
                                    ui.add_space(4.0);
                                    egui::ComboBox::from_id_salt("roll-grid")
                                        .selected_text(if roll.snap { "1/16" } else { "Off" })
                                        .width(72.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut roll.snap, true, "1/16");
                                            ui.selectable_value(&mut roll.snap, false, "Off");
                                        })
                                        .response
                                        .on_hover_text(
                                            "Snap note drags and insertions to a sixteenth",
                                        );
                                },
                            );
                            if let Some(track) = roll.selected
                                && roll.fitted != Some(track)
                            {
                                roll.fit(&project.song, track, ui.available_width() - 60.0);
                            }
                            (
                                piano_roll::show(
                                    ui,
                                    &project.song,
                                    roll,
                                    position,
                                    &selected_clips,
                                ),
                                docked_track,
                                docked_edit,
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
                        self.drums_panel(ui);
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
                        Self::score_toolbar(
                            &mut self.score,
                            &mut self.settings,
                            &mut self.print_dialog,
                            &mut self.lyrics_open,
                            ui,
                            &project.song,
                            &score_tracks,
                            &mut save_pdf,
                        );
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
                            let grand_staff = self.score.grand
                                || (!self.score.one_staff && needs_grand_staff(track_song));
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
        self.apply_panel_edits(PanelEdits {
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
        });
    }

    /// Applies the edits the central view collected this frame.
    fn apply_panel_edits(&mut self, edits: PanelEdits) {
        let PanelEdits {
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
        } = edits;
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

/// The name of a track's sound: a General MIDI program, or on channel 10 a
/// GS drum kit (`generic-gs.yaml`, "Drum Sets"), like `Session::soundNameFor`.
fn sound_name(program: Option<u8>, drums: bool) -> String {
    let Some(program) = program else {
        return String::from(if drums {
            "Standard Kit"
        } else {
            "Choose sound..."
        });
    };
    if drums {
        let kit = match program {
            0 => "Standard",
            8 => "Room",
            16 => "Power",
            24 => "Electronic",
            25 => "TR-808",
            32 => "Jazz",
            40 => "Brush",
            48 => "Orchestra",
            56 => "SFX",
            _ => return format!("Drum Kit {}", u16::from(program) + 1),
        };
        return format!("{kit} Kit");
    }
    String::from(midi_devices::programs::name(program))
}

/// A child of a toolbar for one control: v1 field colours, 30 pt tall.
pub(super) fn toolbar_child(ui: &mut egui::Ui, rect: egui::Rect) -> egui::Ui {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    theme::v1_fields(child.visuals_mut());
    child.spacing_mut().interact_size.y = 30.0;
    child.spacing_mut().button_padding = egui::vec2(8.0, 6.0);
    child
}

/// The clip in one Loops slot.
fn track_slot(song: &Song, track: TrackId, scene: usize) -> Option<&project::song::SlotClip> {
    arranging::loops::slot(song, track, scene)
}
