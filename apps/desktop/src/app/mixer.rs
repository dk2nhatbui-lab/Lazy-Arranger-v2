//! The mixer, effects buses and plugin windows of [`ShellApp`].

use super::*;
use crate::widgets::{MIXER_KNOB_SIZE, mixer_panel_background, mixer_strip};

/// Height of every mixer fader. Track, bus and master strips keep the same
/// rows above it (two knob rows, one button row) so their faders line up.
const FADER_HEIGHT: f32 = 96.0;
/// Height of the M/S/R button row, fixed so an empty row matches a full one.
const BUTTON_ROW_HEIGHT: f32 = 20.0;

/// Reserves a row of `height` so a strip without that control keeps its
/// fader level with the other strips.
fn empty_strip_slot(ui: &mut egui::Ui, height: f32) {
    ui.allocate_exact_size(egui::vec2(1.0, height), egui::Sense::hover());
}

impl ShellApp {
    pub(super) fn bottom_mixer(&mut self, context: &Context) {
        if self.settings.kids_mode || !self.mixer_visible {
            return;
        }
        let tracks = self.song().tracks.clone();
        let buses = self.song().buses.clone();
        let mut edits: Vec<(TrackId, u8, u8, bool, bool, u8, bool)> = Vec::new();
        let mut bus_edits: Vec<(u8, u8, u8, bool, bool)> = Vec::new();
        let bus_levels: Vec<f32> = buses
            .iter()
            .map(|bus| {
                if !self.playing {
                    return 0.0;
                }
                tracks
                    .iter()
                    .filter(|track| track.bus == Some(bus.id) || track.send == Some(bus.id))
                    .map(|track| {
                        let level =
                            track_meter_level(track, self.position, &self.timeline_waveforms);
                        if track.bus == Some(bus.id) {
                            level
                        } else {
                            level * f32::from(track.send_level) / 127.0
                        }
                    })
                    .sum::<f32>()
                    .clamp(0.0, 1.0)
            })
            .collect();
        let mut master_gain = self.settings.master_gain_db;
        egui::TopBottomPanel::bottom("bottom-mixer")
            .resizable(true)
            .default_height(236.0)
            .height_range(120.0..=420.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(context, |ui| {
                mixer_panel_background(ui);
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        for (index, track) in tracks.iter().enumerate() {
                            let color = crate::timeline::lane_color(index);
                            let mut volume = f32::from(track.volume);
                            let mut pan = f32::from(track.pan);
                            let mut send = f32::from(track.send_level);
                            let mut muted = track.muted;
                            let mut soloed = track.soloed;
                            let mut armed = track.armed;
                            mixer_strip(ui, egui::vec2(108.0, 220.0), |ui| {
                                ui.vertical(|ui| {
                                    painter_strip(ui, color);
                                    ui.label(
                                        egui::RichText::new(if track.name.is_empty() {
                                            "untitled"
                                        } else {
                                            &track.name
                                        })
                                        .size(11.0)
                                        .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            if track.name.to_ascii_lowercase().contains("drum") {
                                                "Electronic Kit"
                                            } else {
                                                "Built-in SoundFont"
                                            },
                                        )
                                        .size(9.0)
                                        .color(theme::text_dim()),
                                    );
                                    ui.horizontal(|ui| {
                                        let rev =
                                            mixer_knob(ui, &mut send, 0.0, 127.0, "Rev", true);
                                        ui.add_space(28.0);
                                        if rev {
                                            edits.push((
                                                track.id,
                                                track.volume,
                                                track.pan,
                                                muted,
                                                soloed,
                                                u8::try_from(send.round() as i32)
                                                    .unwrap_or(track.send_level),
                                                armed,
                                            ));
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        let label = pan_label(pan);
                                        let changed =
                                            mixer_knob(ui, &mut pan, 0.0, 127.0, &label, true);
                                        if changed {
                                            edits.push((
                                                track.id,
                                                track.volume,
                                                u8::try_from(pan.round() as i32)
                                                    .unwrap_or(track.pan),
                                                muted,
                                                soloed,
                                                track.send_level,
                                                armed,
                                            ));
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.set_min_height(BUTTON_ROW_HEIGHT);
                                        if ui.small_button("M").clicked() {
                                            muted = !muted;
                                            edits.push((
                                                track.id,
                                                track.volume,
                                                track.pan,
                                                muted,
                                                soloed,
                                                track.send_level,
                                                armed,
                                            ));
                                        }
                                        if ui.small_button("S").clicked() {
                                            soloed = !soloed;
                                            edits.push((
                                                track.id,
                                                track.volume,
                                                track.pan,
                                                muted,
                                                soloed,
                                                track.send_level,
                                                armed,
                                            ));
                                        }
                                        if ui.small_button("R").clicked() {
                                            armed = !armed;
                                            edits.push((
                                                track.id,
                                                track.volume,
                                                track.pan,
                                                muted,
                                                soloed,
                                                track.send_level,
                                                armed,
                                            ));
                                        }
                                    });
                                    let level = if self.playing {
                                        track_meter_level(
                                            track,
                                            self.position,
                                            &self.timeline_waveforms,
                                        )
                                    } else {
                                        0.0
                                    };
                                    if mixer_fader(
                                        ui,
                                        &mut volume,
                                        0.0,
                                        127.0,
                                        FADER_HEIGHT,
                                        color,
                                        "Volume",
                                        100.0,
                                        level,
                                    ) {
                                        edits.push((
                                            track.id,
                                            u8::try_from(volume.round() as i32)
                                                .unwrap_or(track.volume),
                                            track.pan,
                                            muted,
                                            soloed,
                                            track.send_level,
                                            armed,
                                        ));
                                    }
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{:.1} dB",
                                            f32::from(track.volume) - 100.0
                                        ))
                                        .size(9.0)
                                        .color(theme::text_muted()),
                                    );
                                });
                            });
                        }
                        for (bus_index, bus) in buses.iter().enumerate() {
                            let mut volume = f32::from(bus.volume);
                            let mut pan = f32::from(bus.pan);
                            let mut bus_volume = bus.volume;
                            let mut bus_pan = bus.pan;
                            let mut bus_muted = bus.muted;
                            let mut bus_soloed = bus.soloed;
                            mixer_strip(ui, egui::vec2(96.0, 220.0), |ui| {
                                ui.vertical(|ui| {
                                    painter_strip(ui, theme::accent());
                                    ui.label(egui::RichText::new(&bus.name).size(11.0).strong());
                                    ui.label(
                                        egui::RichText::new("Bus")
                                            .size(9.0)
                                            .color(theme::text_dim()),
                                    );
                                    ui.horizontal(|ui| {
                                        let pan_text = pan_label(pan);
                                        if mixer_knob(ui, &mut pan, 0.0, 127.0, &pan_text, true) {
                                            bus_pan =
                                                u8::try_from(pan.round() as i32).unwrap_or(bus.pan);
                                            bus_edits.push((
                                                bus.id, bus_volume, bus_pan, bus_muted, bus_soloed,
                                            ));
                                        }
                                    });
                                    empty_strip_slot(ui, MIXER_KNOB_SIZE.y);
                                    ui.horizontal(|ui| {
                                        ui.set_min_height(BUTTON_ROW_HEIGHT);
                                        if ui.small_button("M").clicked() {
                                            bus_muted = !bus_muted;
                                            bus_edits.push((
                                                bus.id, bus_volume, bus_pan, bus_muted, bus_soloed,
                                            ));
                                        }
                                        if ui.small_button("S").clicked() {
                                            bus_soloed = !bus_soloed;
                                            bus_edits.push((
                                                bus.id, bus_volume, bus_pan, bus_muted, bus_soloed,
                                            ));
                                        }
                                    });
                                    let level =
                                        bus_levels.get(bus_index).copied().unwrap_or_default();
                                    if mixer_fader(
                                        ui,
                                        &mut volume,
                                        0.0,
                                        127.0,
                                        FADER_HEIGHT,
                                        theme::accent(),
                                        "Bus volume",
                                        100.0,
                                        level,
                                    ) {
                                        bus_volume = u8::try_from(volume.round() as i32)
                                            .unwrap_or(bus.volume);
                                        bus_edits.push((
                                            bus.id, bus_volume, bus_pan, bus_muted, bus_soloed,
                                        ));
                                    }
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{:.1} dB",
                                            f32::from(bus.volume) - 100.0
                                        ))
                                        .size(9.0)
                                        .color(theme::text_muted()),
                                    );
                                });
                            });
                        }
                        let master_level = self.player.as_ref().map_or_else(
                            || {
                                if self.playing {
                                    self.song()
                                        .tracks
                                        .iter()
                                        .map(|track| {
                                            track_meter_level(
                                                track,
                                                self.position,
                                                &self.timeline_waveforms,
                                            )
                                        })
                                        .fold(0.0_f32, f32::max)
                                } else {
                                    0.0
                                }
                            },
                            |player| player.meter().peak.clamp(0.0, 1.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            mixer_strip(ui, egui::vec2(110.0, 220.0), |ui| {
                                ui.vertical(|ui| {
                                    painter_strip(ui, theme::text_muted());
                                    ui.label(egui::RichText::new("Master").size(11.0).strong());
                                    ui.label(
                                        egui::RichText::new(if self.player.is_some() {
                                            "cpal"
                                        } else {
                                            "offline"
                                        })
                                        .size(9.0)
                                        .color(theme::text_dim()),
                                    );
                                    empty_strip_slot(ui, MIXER_KNOB_SIZE.y);
                                    empty_strip_slot(ui, MIXER_KNOB_SIZE.y);
                                    empty_strip_slot(ui, BUTTON_ROW_HEIGHT);
                                    mixer_fader(
                                        ui,
                                        &mut master_gain,
                                        -24.0,
                                        12.0,
                                        FADER_HEIGHT,
                                        theme::accent(),
                                        "Master",
                                        0.0,
                                        master_level,
                                    );
                                    ui.label(
                                        egui::RichText::new(format!("{master_gain:.1} dB"))
                                            .size(9.0)
                                            .color(theme::text_muted()),
                                    );
                                });
                            });
                        });
                    });
                });
            });
        self.settings.master_gain_db = master_gain;
        if !bus_edits.is_empty() {
            let mut updated = self.song().buses.clone();
            for (id, volume, pan, muted, soloed) in &bus_edits {
                if let Some(bus) = updated.iter_mut().find(|bus| bus.id == *id) {
                    bus.volume = *volume;
                    bus.pan = *pan;
                    bus.muted = *muted;
                    bus.soloed = *soloed;
                }
            }
            match self.editor.set_buses(updated) {
                Ok(()) => self.status = Some(String::from("bus levels updated")),
                Err(error) => self.status = Some(error),
            }
        }
        for (id, volume, pan, muted, soloed, send_level, armed) in edits {
            let _ = self.editor.set_track_gain(id, volume, pan);
            let _ = self.editor.set_track_flags(id, muted, soloed, armed);
            if let Some(track) = self.song().tracks.iter().find(|track| track.id == id) {
                let _ = self
                    .editor
                    .set_track_routing(id, track.bus, track.send, send_level);
            }
        }
    }

    pub(super) fn bus_fx_window(&mut self, context: &Context) {
        if !self.bus_fx_open {
            return;
        }
        let mut open = self.bus_fx_open;
        let mut buses = self.song().buses.clone();
        let catalog = self
            .plugin_scan
            .clone()
            .or_else(|| plugins::load_catalog(std::path::Path::new(plugins::CATALOG_FILE)))
            .unwrap_or_default();
        let mut changed = false;
        egui::Window::new("Bus effects")
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(context, |ui| {
                if buses.is_empty() {
                    ui.label("No buses. Add one in the Mixer.");
                    return;
                }
                self.bus_fx_choice = self.bus_fx_choice.min(buses.len() - 1);
                egui::ComboBox::from_id_salt("bus-fx-bus")
                    .selected_text(&buses[self.bus_fx_choice].name)
                    .show_ui(ui, |ui| {
                        for (index, bus) in buses.iter().enumerate() {
                            ui.selectable_value(&mut self.bus_fx_choice, index, &bus.name);
                        }
                    });
                let plugins = &mut buses[self.bus_fx_choice].plugins;
                if plugins.is_empty() {
                    ui.label("No effects on this bus.");
                }
                let mut remove = None;
                let mut move_up = None;
                for (index, slot) in plugins.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut slot.bypassed, "").changed() {
                            changed = true;
                        }
                        ui.label(&slot.name);
                        if index > 0 && ui.small_button("↑").clicked() {
                            move_up = Some(index);
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                        if slot.format == project::song::PluginFormat::Vst3
                            && !slot.identifier.is_empty()
                        {
                            if ui
                                .small_button("Capture")
                                .on_hover_text("Save the plug-in's current state")
                                .clicked()
                            {
                                match plugin_host::Vst3Effect::new(
                                    std::path::Path::new(&slot.path),
                                    &slot.identifier,
                                    48_000.0,
                                    512,
                                )
                                .and_then(|effect| effect.save_state_base64())
                                {
                                    Ok(state) => {
                                        slot.state = state;
                                        changed = true;
                                        self.status =
                                            Some(String::from("master plug-in state captured"));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                            if !slot.state.is_empty() && ui.small_button("Clear").clicked() {
                                slot.state.clear();
                                changed = true;
                            }
                        }
                    });
                }
                if let Some(index) = move_up {
                    plugins.swap(index, index - 1);
                    changed = true;
                }
                if let Some(index) = remove {
                    plugins.remove(index);
                    changed = true;
                }
                egui::ComboBox::from_id_salt("bus-fx-plugin")
                    .selected_text(
                        catalog
                            .get(self.bus_fx_plugin_choice)
                            .map_or("No cached plug-ins", |plugin| plugin.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for (index, plugin) in catalog.iter().enumerate().take(20) {
                            ui.selectable_value(
                                &mut self.bus_fx_plugin_choice,
                                index,
                                &plugin.name,
                            );
                        }
                    });
                if ui.button("Add plug-in").clicked()
                    && let Some(plugin) = catalog.get(self.bus_fx_plugin_choice)
                {
                    let id = plugins.iter().map(|slot| slot.id).max().unwrap_or(0) + 1;
                    plugins.push(project::song::PluginSlot {
                        id,
                        name: plugin.name.clone(),
                        format: match plugin.format {
                            plugins::Format::Vst3 => project::song::PluginFormat::Vst3,
                            plugins::Format::AudioUnit => project::song::PluginFormat::AudioUnit,
                        },
                        path: plugin.path.display().to_string(),
                        identifier: plugin.identifier.clone(),
                        bypassed: false,
                        state: String::new(),
                    });
                    changed = true;
                }
            });
        if changed {
            match self.editor.set_buses(buses) {
                Ok(()) => self.status = Some(String::from("bus effects updated")),
                Err(error) => self.status = Some(error),
            }
        }
        self.bus_fx_open = open;
    }

    pub(super) fn master_fx_window(&mut self, context: &Context) {
        if !self.master_fx_open {
            return;
        }
        let mut open = self.master_fx_open;
        let mut changed = false;
        let mut plugins = self.song().master_plugins.clone();
        let catalog = self
            .plugin_scan
            .clone()
            .or_else(|| plugins::load_catalog(std::path::Path::new(plugins::CATALOG_FILE)))
            .unwrap_or_default();
        egui::Window::new("Master effects")
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(context, |ui| {
                if plugins.is_empty() {
                    ui.label("No master effects.");
                }
                let mut remove = None;
                let mut move_up = None;
                for (index, slot) in plugins.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut slot.bypassed, "").changed() {
                            changed = true;
                        }
                        ui.label(&slot.name);
                        if index > 0 && ui.small_button("↑").clicked() {
                            move_up = Some(index);
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                        if slot.format == project::song::PluginFormat::Vst3
                            && !slot.identifier.is_empty()
                        {
                            if ui
                                .small_button("Capture")
                                .on_hover_text("Save the plug-in's current state")
                                .clicked()
                            {
                                match plugin_host::Vst3Effect::new(
                                    std::path::Path::new(&slot.path),
                                    &slot.identifier,
                                    48_000.0,
                                    512,
                                )
                                .and_then(|effect| effect.save_state_base64())
                                {
                                    Ok(state) => {
                                        slot.state = state;
                                        changed = true;
                                        self.status =
                                            Some(String::from("bus plug-in state captured"));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                            if !slot.state.is_empty() && ui.small_button("Clear").clicked() {
                                slot.state.clear();
                                changed = true;
                            }
                        }
                    });
                }
                if let Some(index) = move_up {
                    plugins.swap(index, index - 1);
                    changed = true;
                }
                if let Some(index) = remove {
                    plugins.remove(index);
                    changed = true;
                }
                egui::ComboBox::from_id_salt("master-fx-plugin")
                    .selected_text(
                        catalog
                            .get(self.master_fx_choice)
                            .map_or("No cached plug-ins", |plugin| plugin.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for (index, plugin) in catalog.iter().enumerate().take(20) {
                            ui.selectable_value(&mut self.master_fx_choice, index, &plugin.name);
                        }
                    });
                if ui.button("Add plug-in").clicked()
                    && let Some(plugin) = catalog.get(self.master_fx_choice)
                {
                    let id = plugins.iter().map(|slot| slot.id).max().unwrap_or(0) + 1;
                    plugins.push(project::song::PluginSlot {
                        id,
                        name: plugin.name.clone(),
                        format: match plugin.format {
                            plugins::Format::Vst3 => project::song::PluginFormat::Vst3,
                            plugins::Format::AudioUnit => project::song::PluginFormat::AudioUnit,
                        },
                        path: plugin.path.display().to_string(),
                        identifier: plugin.identifier.clone(),
                        bypassed: false,
                        state: String::new(),
                    });
                    changed = true;
                }
                ui.label(
                    egui::RichText::new(
                        "Master effects run after the bus sum and before the master limiter.",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
            });
        if changed {
            match self.editor.set_master_plugins(plugins) {
                Ok(()) => self.status = Some(String::from("master effects updated")),
                Err(error) => self.status = Some(error),
            }
        }
        self.master_fx_open = open;
    }

    pub(super) fn start_plugin_scan(&mut self) {
        if self.plugin_scanning {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let blocklist_path = std::path::Path::new(plugins::BLOCKLIST_FILE);
            let mut blocked = plugins::load_blocklist(blocklist_path);
            let before = blocked.len();
            let report = plugins::scan_installed_report(&blocked);
            for path in &report.failed {
                if !blocked.contains(path) {
                    blocked.push(path.clone());
                }
            }
            if blocked.len() > before {
                let _ = plugins::save_blocklist(blocklist_path, &blocked);
            }
            let _ = sender.send((report.plugins, blocked.len().saturating_sub(before)));
        });
        self.plugin_scan_rx = Some(receiver);
        self.plugin_scanning = true;
        self.plugin_error = None;
    }

    pub(super) fn plugins_window(&mut self, context: &egui::Context) {
        if !self.plugins_open {
            return;
        }
        if self.plugin_scan.is_none() {
            self.plugin_scan = plugins::load_catalog(std::path::Path::new(plugins::CATALOG_FILE));
            if self.plugin_scan.is_none() {
                self.start_plugin_scan();
            }
        }
        let mut open = self.plugins_open;
        let mut add = None;
        egui::Window::new("My Plug-Ins")
            .open(&mut open)
            .default_width(520.0)
            .default_height(420.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("My Plug-Ins")
                            .size(20.0)
                            .strong(),
                    );
                    ui.add_space(10.0);
                    if ui
                        .add_enabled(!self.plugin_scanning, egui::Button::new("Rescan"))
                        .clicked()
                    {
                        self.start_plugin_scan();
                    }
                    if self.plugin_scanning {
                        ui.label(
                            egui::RichText::new("Scanning…")
                                .size(10.0)
                                .color(theme::text_muted()),
                        );
                    }
                    let blocked = plugins::load_blocklist(std::path::Path::new(
                        plugins::BLOCKLIST_FILE,
                    ));
                    if !blocked.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("{} blocked", blocked.len()))
                                .size(10.0)
                                .color(theme::text_dim()),
                        );
                        if ui.small_button("Clear blocklist").clicked() {
                            let _ = std::fs::remove_file(plugins::BLOCKLIST_FILE);
                            self.plugin_error = None;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.plugin_search)
                            .hint_text("Search plug-ins")
                            .desired_width(240.0),
                    );
                    ui.selectable_value(&mut self.plugin_filter, None, "All");
                    ui.selectable_value(
                        &mut self.plugin_filter,
                        Some(plugins::Format::AudioUnit),
                        "AU",
                    );
                    ui.selectable_value(
                        &mut self.plugin_filter,
                        Some(plugins::Format::Vst3),
                        "VST3",
                    );
                });
                ui.label(
                    egui::RichText::new(
                        "Installed bundles are listed here. Hosting and editors are not implemented.",
                    )
                    .size(10.0)
                    .color(theme::text_dim()),
                );
                ui.separator();
                let scan = self.plugin_scan.clone().unwrap_or_default();
                ui.label(
                    egui::RichText::new(format!(
                        "{} plug-in(s) · cache {}",
                        scan.len(),
                        plugins::CATALOG_FILE
                    ))
                    .size(10.0)
                    .color(theme::text_dim()),
                );
                if scan.is_empty() {
                    ui.label("no plug-in bundles found");
                }
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for plugin in scan
                            .iter()
                            .filter(|plugin| {
                                plugin_matches(
                                    plugin,
                                    &self.plugin_search,
                                    self.plugin_filter,
                                )
                            })
                        {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_sized(
                                        [260.0, 28.0],
                                        egui::Button::new(egui::RichText::new(&plugin.name).size(13.0)),
                                    )
                                    .on_hover_text(plugin.path.display().to_string())
                                    .clicked()
                                {
                                    add = Some(plugin.clone());
                                }
                                ui.label(plugin.format.label());
                                ui.label(
                                    egui::RichText::new(match plugin.role {
                                        plugins::Role::Instrument => "instrument",
                                        plugins::Role::Effect => "effect",
                                        plugins::Role::Unknown => "",
                                    })
                                    .size(10.0)
                                    .color(theme::text_muted()),
                                );
                                if !plugin.vendor.is_empty() {
                                    ui.label(
                                        egui::RichText::new(&plugin.vendor)
                                            .size(10.0)
                                            .color(theme::text_dim()),
                                    );
                                }
                            });
                        }
                    });
            });
        self.plugins_open = open;
        if let Some(plugin) = add {
            let track = self
                .roll
                .selected
                .or_else(|| self.song().tracks.first().map(|track| track.id));
            if let Some(track) = track {
                let format = match plugin.format {
                    plugins::Format::Vst3 => project::song::PluginFormat::Vst3,
                    plugins::Format::AudioUnit => project::song::PluginFormat::AudioUnit,
                };
                match self.editor.add_plugin(
                    track,
                    &plugin.name,
                    format,
                    &plugin.path.display().to_string(),
                    &plugin.identifier,
                ) {
                    Ok(id) => {
                        self.status = Some(format!(
                            "added {} to track {track} (slot {id})",
                            plugin.name
                        ));
                    }
                    Err(error) => self.status = Some(error),
                }
            } else {
                self.status = Some(String::from("select a track first"));
            }
        }
    }

    pub(super) fn close_native_plugin_editor(&mut self, save_state: bool) {
        if let Some(mut open) = self.native_plugin_editor.take() {
            if save_state
                && let Ok(state) = open.capture_state()
                && !state.is_empty()
            {
                let _ = self
                    .editor
                    .set_plugin_state(open.track, open.slot_id, &state);
                self.status = Some(format!("saved native UI state for {}", open.name));
            }
            open.close();
        }
    }

    pub(super) fn poll_native_plugin_editor(&mut self) {
        if self
            .native_plugin_editor
            .as_ref()
            .is_some_and(|open| !open.is_open())
        {
            self.close_native_plugin_editor(true);
        }
    }

    pub(super) fn open_native_plugin_editor(&mut self, track: TrackId, slot: u32) {
        self.close_native_plugin_editor(true);
        let Some(plugin) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.plugins.iter().find(|entry| entry.id == slot))
            .cloned()
        else {
            self.status = Some(String::from("plug-in slot not found"));
            return;
        };
        let opened = match plugin.format {
            project::song::PluginFormat::Vst3 => plugin_host::Vst3Editor::open(
                &plugin.name,
                std::path::Path::new(&plugin.path),
                &plugin.identifier,
                &plugin.state,
                48_000.0,
            )
            .map(|editor| {
                let (w, h) = editor.size();
                (
                    NativeEditorKind::Vst3(editor),
                    format!("opened native VST3 UI for {} ({w}x{h})", plugin.name),
                )
            }),
            project::song::PluginFormat::AudioUnit => plugin_host::AuEditor::open(
                &plugin.name,
                &plugin.identifier,
                &plugin.state,
                48_000.0,
            )
            .map(|editor| {
                let kind = if editor.used_custom_view() {
                    "custom Cocoa"
                } else {
                    "AUGenericView"
                };
                (
                    NativeEditorKind::Au(editor),
                    format!("opened native AU UI ({kind}) for {}", plugin.name),
                )
            }),
        };
        match opened {
            Ok((kind, message)) => {
                self.native_plugin_editor = Some(OpenNativeEditor {
                    track,
                    slot_id: slot,
                    name: plugin.name,
                    kind,
                });
                self.status = Some(message);
            }
            Err(error) => self.status = Some(format!("native UI: {error}")),
        }
    }

    pub(super) fn capture_vst3_state(&mut self, track: TrackId, slot: u32) {
        let Some(plugin) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.plugins.iter().find(|entry| entry.id == slot))
            .cloned()
        else {
            self.status = Some(String::from("plug-in slot not found"));
            return;
        };
        let effect = match plugin_host::Vst3Effect::new(
            std::path::Path::new(&plugin.path),
            &plugin.identifier,
            48_000.0,
            512,
        ) {
            Ok(effect) => effect,
            Err(error) => {
                self.status = Some(format!("VST3: {error}"));
                return;
            }
        };
        match effect.save_state_base64() {
            Ok(state) => match self.editor.set_plugin_state(track, slot, &state) {
                Ok(()) => self.status = Some(String::from("VST3 state captured")),
                Err(error) => self.status = Some(error),
            },
            Err(error) => self.status = Some(format!("VST3: {error}")),
        }
    }

    pub(super) fn open_vst3_parameters(&mut self, track: TrackId, slot: u32) {
        let Some(plugin) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.plugins.iter().find(|entry| entry.id == slot))
            .cloned()
        else {
            self.status = Some(String::from("plug-in slot not found"));
            return;
        };
        let effect = match plugin_host::Vst3Effect::new(
            std::path::Path::new(&plugin.path),
            &plugin.identifier,
            48_000.0,
            512,
        ) {
            Ok(effect) => effect,
            Err(error) => {
                self.status = Some(format!("VST3: {error}"));
                return;
            }
        };
        let mut effect = effect;
        if !plugin.state.is_empty() {
            let _ = effect.load_state_base64(&plugin.state);
        }
        let params = effect.parameters();
        if params.is_empty() {
            self.status = Some(format!("{} exposes no VST3 parameters", plugin.name));
            return;
        }
        self.vst3_params = params;
        self.vst3_effect = Some(effect);
        self.vst3_params_track = Some(track);
        self.vst3_params_slot = Some(slot);
        self.vst3_params_open = true;
    }

    pub(super) fn vst3_parameters_window(&mut self, context: &Context) {
        if !self.vst3_params_open {
            return;
        }
        let mut open = self.vst3_params_open;
        egui::Window::new("VST3 parameters")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                if let (Some(track), Some(slot)) = (self.vst3_params_track, self.vst3_params_slot) {
                    ui.label(
                        egui::RichText::new(format!("track {track} · slot {slot}"))
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                let slot = self.vst3_params_slot;
                let mut automate = None;
                if let Some(effect) = self.vst3_effect.as_mut() {
                    for (id, name, value) in &mut self.vst3_params {
                        let label = if name.is_empty() {
                            format!("Param {id}")
                        } else {
                            name.clone()
                        };
                        ui.horizontal(|ui| {
                            let changed = ui
                                .add(egui::Slider::new(value, 0.0..=1.0).text(label))
                                .changed();
                            if changed {
                                effect.set_parameter(*id, *value);
                            }
                            if ui
                                .small_button("A")
                                .on_hover_text("Use this parameter for automation")
                                .clicked()
                                && let Some(slot) = slot
                            {
                                automate = Some((slot, *id));
                            }
                        });
                    }
                }
                if let Some((slot, parameter)) = automate {
                    self.automation_target =
                        project::song::AutomationTarget::PluginParam { slot, parameter };
                    self.status = Some(format!(
                        "automation target: slot {slot} parameter {parameter}"
                    ));
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply state").clicked()
                        && let (Some(effect), Some(track), Some(slot)) = (
                            self.vst3_effect.as_ref(),
                            self.vst3_params_track,
                            self.vst3_params_slot,
                        )
                    {
                        match effect.save_state_base64() {
                            Ok(state) => match self.editor.set_plugin_state(track, slot, &state) {
                                Ok(()) => self.status = Some(String::from("VST3 state saved")),
                                Err(error) => self.status = Some(error),
                            },
                            Err(error) => self.status = Some(format!("VST3: {error}")),
                        }
                    }
                });
            });
        self.vst3_params_open = open;
        if !open {
            self.vst3_effect = None;
            self.vst3_params.clear();
        }
    }

    pub(super) fn open_au_parameters(&mut self, track: TrackId, slot: u32) {
        let Some(plugin) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.plugins.iter().find(|entry| entry.id == slot))
            .cloned()
        else {
            self.status = Some(String::from("plug-in slot not found"));
            return;
        };
        if plugin.format != project::song::PluginFormat::AudioUnit {
            self.status = Some(String::from("only AU parameters are supported"));
            return;
        }
        let unit = if let Ok(effect) = plugin_host::AuEffect::new(&plugin.identifier, 48_000.0) {
            effect.apply_state(&plugin.state);
            AuEditorUnit::Effect(effect)
        } else if let Ok(instrument) = plugin_host::AuInstrument::new(&plugin.identifier, 48_000.0)
        {
            instrument.apply_state(&plugin.state);
            AuEditorUnit::Instrument(instrument)
        } else {
            self.status = Some(format!(
                "AU: {} could not be opened as an effect or instrument",
                plugin.name
            ));
            return;
        };
        self.au_params = unit.parameters();
        self.au_effect = Some(unit);
        self.au_editor_track = Some(track);
        self.au_editor_slot = Some(slot);
        self.au_editor_open = true;
    }

    pub(super) fn au_parameters_window(&mut self, context: &Context) {
        let mut open = self.au_editor_open;
        egui::Window::new("AU parameters")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                if let (Some(track), Some(slot)) = (self.au_editor_track, self.au_editor_slot) {
                    ui.label(
                        egui::RichText::new(format!("track {track} · slot {slot}"))
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                let slot = self.au_editor_slot;
                let mut automate = None;
                if let Some(effect) = self.au_effect.as_ref() {
                    for parameter in &mut self.au_params {
                        let (id, name, value, min, max, _) = parameter;
                        if *min >= *max {
                            continue;
                        }
                        let label = if name.is_empty() {
                            format!("Param {id}")
                        } else {
                            name.clone()
                        };
                        ui.horizontal(|ui| {
                            let changed = ui
                                .add(egui::Slider::new(value, *min..=*max).text(label))
                                .changed();
                            if changed {
                                effect.set_parameter(*id, *value);
                            }
                            if ui
                                .small_button("A")
                                .on_hover_text("Use this parameter for automation")
                                .clicked()
                                && let Some(slot) = slot
                            {
                                automate = Some((slot, *id));
                            }
                        });
                    }
                } else {
                    ui.label("No AU instance.");
                }
                if let Some((slot, parameter)) = automate {
                    self.automation_target =
                        project::song::AutomationTarget::PluginParam { slot, parameter };
                    self.status = Some(format!(
                        "automation target: slot {slot} parameter {parameter}"
                    ));
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked()
                        && let (Some(track), Some(slot)) =
                            (self.au_editor_track, self.au_editor_slot)
                    {
                        let state: String = self
                            .au_params
                            .iter()
                            .map(|(id, _, value, _, _, _)| format!("{id}={value};"))
                            .collect();
                        match self.editor.set_plugin_state(track, slot, &state) {
                            Ok(()) => self.status = Some(String::from("AU parameter state saved")),
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Defaults").clicked()
                        && let Some(effect) = self.au_effect.as_ref()
                    {
                        for (id, _, value, _, _, default) in &mut self.au_params {
                            *value = *default;
                            effect.set_parameter(*id, *default);
                        }
                    }
                });
            });
        self.au_editor_open = open;
        if !open {
            self.au_effect = None;
            self.au_params.clear();
        }
    }

    /// Mixer strips: one per track with a fader, pan and mute/solo.
    pub(super) fn mixer_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mixer");
        if self.stem_levels.is_empty()
            && self
                .player
                .as_ref()
                .is_some_and(audio::player::Player::is_playing)
        {
            self.stem_levels = audio::render_stems(self.song(), audio::SAMPLE_RATE)
                .into_iter()
                .map(|stem| (stem.track, stem.audio.samples))
                .collect();
        }
        ui.horizontal(|ui| {
            ui.label("Master");
            ui.add(
                egui::Slider::new(&mut self.settings.master_gain_db, -24.0..=12.0)
                    .suffix(" dB")
                    .fixed_decimals(1),
            );
            ui.checkbox(&mut self.settings.master_limiter, "Limiter");
            ui.add(
                egui::Slider::new(&mut self.settings.master_reverb, 0.0..=1.0)
                    .text("reverb")
                    .fixed_decimals(2),
            );
            ui.add(
                egui::Slider::new(&mut self.settings.master_chorus, 0.0..=1.0)
                    .text("chorus")
                    .fixed_decimals(2),
            );
        });
        let meter = self
            .player
            .as_ref()
            .filter(|player| player.is_playing())
            .map_or(
                audio::master::Meter::default(),
                audio::player::Player::meter,
            );
        ui.horizontal(|ui| {
            ui.label("Peak");
            ui.add(
                egui::ProgressBar::new(meter.peak.clamp(0.0, 1.0))
                    .desired_width(140.0)
                    .text(format!("{:.0}%", meter.peak * 100.0)),
            );
            ui.label("RMS");
            ui.add(
                egui::ProgressBar::new(meter.rms.clamp(0.0, 1.0))
                    .desired_width(140.0)
                    .text(format!("{:.0}%", meter.rms * 100.0)),
            );
        });
        ui.separator();
        let mut buses = self.song().buses.clone();
        let mut buses_edited = false;
        let mut remove_bus = None;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Buses").strong());
            if ui.small_button("Add bus").clicked() {
                let id = buses
                    .iter()
                    .map(|bus| bus.id)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1);
                buses.push(project::song::Bus {
                    id,
                    name: format!("Bus {id}"),
                    volume: 100,
                    pan: 64,
                    muted: false,
                    soloed: false,
                    plugins: Vec::new(),
                });
                buses_edited = true;
            }
        });
        for bus in &mut buses {
            let bus_meter = if self
                .player
                .as_ref()
                .is_some_and(audio::player::Player::is_playing)
            {
                let stem_rate = self.stem_sample_rate.max(1);
                let position = self.player.as_ref().map_or(0, |player| {
                    ((player.seconds() * f64::from(stem_rate)).round() as usize).saturating_mul(2)
                });
                self.stem_levels
                    .iter()
                    .filter(|(id, _)| {
                        self.song().tracks.iter().any(|track| {
                            track.id == *id
                                && (track.bus == Some(bus.id) || track.send == Some(bus.id))
                        })
                    })
                    .map(|(_, samples)| {
                        audio::master::meter_at(samples, position, stem_rate as usize / 20)
                    })
                    .fold(audio::master::Meter::default(), |left, right| {
                        audio::master::Meter {
                            peak: left.peak.max(right.peak),
                            rms: left.rms.max(right.rms),
                        }
                    })
            } else {
                audio::master::Meter::default()
            };
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} {}", bus.id, bus.name))
                        .size(10.0)
                        .color(theme::text_muted()),
                );
                ui.add(
                    egui::ProgressBar::new(bus_meter.peak.clamp(0.0, 1.0))
                        .desired_width(42.0)
                        .text(format!("{:.0}%", bus_meter.peak * 100.0)),
                );
                ui.add(
                    egui::ProgressBar::new(bus_meter.rms.clamp(0.0, 1.0))
                        .desired_width(42.0)
                        .text(format!("rms {:.0}%", bus_meter.rms * 100.0)),
                );
                if ui
                    .add(egui::Slider::new(&mut bus.volume, 0..=127).text("vol"))
                    .changed()
                {
                    buses_edited = true;
                }
                if ui
                    .add(egui::Slider::new(&mut bus.pan, 0..=127).text("pan"))
                    .changed()
                {
                    buses_edited = true;
                }
                if ui
                    .small_button(if bus.muted { "M●" } else { "M" })
                    .on_hover_text("Mute bus")
                    .clicked()
                {
                    bus.muted = !bus.muted;
                    buses_edited = true;
                }
                if ui
                    .small_button(if bus.soloed { "S●" } else { "S" })
                    .on_hover_text("Solo bus")
                    .clicked()
                {
                    bus.soloed = !bus.soloed;
                    buses_edited = true;
                }
                if ui.small_button("Remove").clicked() {
                    remove_bus = Some(bus.id);
                }
            });
        }
        if let Some(id) = remove_bus {
            buses.retain(|bus| bus.id != id);
            buses_edited = true;
        }
        if buses_edited && let Err(error) = self.editor.set_buses(buses) {
            self.status = Some(error);
        }
        ui.collapsing("L6max takes", |ui| {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.l6max.folder);
                if ui.button("Scan").clicked() {
                    match audio::l6max::scan(std::path::Path::new(&self.l6max.folder)) {
                        Ok(plan) => {
                            self.l6max.error = None;
                            self.l6max.selected = vec![true; plan.takes.len()];
                            self.l6max.plan = Some(plan);
                        }
                        Err(error) => {
                            self.l6max.plan = None;
                            self.l6max.error = Some(error);
                        }
                    }
                }
            });
            if let Some(error) = &self.l6max.error {
                ui.label(egui::RichText::new(error).color(theme::recording()));
            }
            if let Some(plan) = &self.l6max.plan {
                ui.label(format!(
                    "{} take(s), {} at 32-bit float, {:.1} s total",
                    plan.takes.len(),
                    plan.float32(),
                    plan.seconds()
                ));
                if self.l6max.selected.len() != plan.takes.len() {
                    self.l6max.selected = vec![true; plan.takes.len()];
                }
                for (index, take) in plan.takes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let selected = self
                            .l6max
                            .selected
                            .get_mut(index)
                            .is_some_and(|selected| *selected);
                        let mut chosen = selected;
                        ui.checkbox(&mut chosen, "");
                        if let Some(selected) = self.l6max.selected.get_mut(index) {
                            *selected = chosen;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {:.1} s · {} ch",
                                take.name, take.seconds, take.channels
                            ))
                            .size(10.0)
                            .color(theme::text_dim()),
                        );
                    });
                }
                if ui.button("Import selected at playhead").clicked() {
                    let takes: Vec<audio::l6max::Take> = plan
                        .takes
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| {
                            self.l6max.selected.get(*index).copied().unwrap_or(true)
                        })
                        .map(|(_, take)| take.clone())
                        .collect();
                    if takes.is_empty() {
                        self.status = Some(String::from("select at least one L6max take"));
                    } else {
                        let selected = audio::l6max::Plan {
                            takes,
                            problems: Vec::new(),
                        };
                        match self.editor.import_l6max(&selected, self.position.max(0)) {
                            Ok(count) => {
                                self.status = Some(format!("imported {count} L6max take(s)"));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                }
            }
        });
        if let Some(track_id) = self
            .roll
            .selected
            .or_else(|| self.song().tracks.first().map(|track| track.id))
        {
            ui.collapsing("Plugins", |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Track {track_id}"))
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                    if ui
                        .add_enabled(!self.plugin_scanning, egui::Button::new("Scan installed"))
                        .clicked()
                    {
                        self.start_plugin_scan();
                    }
                });
                if let Some(error) = &self.plugin_error {
                    ui.label(egui::RichText::new(error).color(theme::recording()));
                }
                let slots = self
                    .song()
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .map(|track| track.plugins.clone())
                    .unwrap_or_default();
                for slot in slots {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} · {}",
                            slot.name,
                            match slot.format {
                                project::song::PluginFormat::Vst3 => "VST3",
                                project::song::PluginFormat::AudioUnit => "AU",
                            }
                        ));
                        if ui.small_button("↑").on_hover_text("Move earlier").clicked()
                            && let Err(error) = self.editor.move_plugin(track_id, slot.id, true)
                        {
                            self.status = Some(error);
                        }
                        if ui.small_button("↓").on_hover_text("Move later").clicked()
                            && let Err(error) = self.editor.move_plugin(track_id, slot.id, false)
                        {
                            self.status = Some(error);
                        }
                        let is_native_open = self
                            .native_plugin_editor
                            .as_ref()
                            .is_some_and(|open| open.track == track_id && open.slot_id == slot.id);
                        if is_native_open {
                            if ui
                                .small_button("Close UI")
                                .on_hover_text("Save state and close the native plug-in window")
                                .clicked()
                            {
                                self.close_native_plugin_editor(true);
                            }
                        } else if ui
                            .small_button("UI")
                            .on_hover_text("Open native plug-in editor window")
                            .clicked()
                        {
                            self.open_native_plugin_editor(track_id, slot.id);
                        }
                        if slot.format == project::song::PluginFormat::AudioUnit
                            && ui.small_button("Params").clicked()
                        {
                            self.open_au_parameters(track_id, slot.id);
                        }
                        if slot.format == project::song::PluginFormat::Vst3 {
                            if ui.small_button("Params").clicked() {
                                self.open_vst3_parameters(track_id, slot.id);
                            }
                            if ui.small_button("Capture").clicked() {
                                self.capture_vst3_state(track_id, slot.id);
                            }
                        }
                        if ui
                            .small_button(if slot.bypassed { "Enable" } else { "Bypass" })
                            .clicked()
                            && let Err(error) = self.editor.toggle_plugin(track_id, slot.id)
                        {
                            self.status = Some(error);
                        }
                        if ui.small_button("Remove").clicked()
                            && let Err(error) = self.editor.remove_plugin(track_id, slot.id)
                        {
                            self.status = Some(error);
                        }
                    });
                }
                if let Some(found) = self.plugin_scan.clone() {
                    ui.separator();
                    for plugin in found.iter().take(10) {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · {}",
                                    plugin.name,
                                    plugin.format.label()
                                ))
                                .size(10.0)
                                .color(theme::text_muted()),
                            );
                            if ui.small_button("Add").clicked() {
                                let format = match plugin.format {
                                    plugins::Format::Vst3 => project::song::PluginFormat::Vst3,
                                    plugins::Format::AudioUnit => {
                                        project::song::PluginFormat::AudioUnit
                                    }
                                };
                                let path = plugin.path.display().to_string();
                                match self.editor.add_plugin(
                                    track_id,
                                    &plugin.name,
                                    format,
                                    &path,
                                    &plugin.identifier,
                                ) {
                                    Ok(id) => {
                                        self.status =
                                            Some(format!("added {} (slot {id})", plugin.name));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                        });
                    }
                    if found.len() > 10 {
                        ui.label(
                            egui::RichText::new(format!("... {} more", found.len() - 10))
                                .size(10.0)
                                .color(theme::text_dim()),
                        );
                    }
                }
            });
        }
        if let Some(track_id) = self
            .roll
            .selected
            .or_else(|| self.song().tracks.first().map(|track| track.id))
        {
            ui.collapsing("SoundFont", |ui| {
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.sf2_path);
                    ui.label("bank");
                    ui.add(egui::DragValue::new(&mut self.sf2_bank).range(0..=16383));
                    ui.label("program");
                    ui.add(egui::DragValue::new(&mut self.sf2_program).range(0..=127));
                    if ui.button("Presets").clicked() {
                        let path = self.sf2_path.trim().to_string();
                        match std::fs::read(&path)
                            .map_err(|error| format!("{path}: {error}"))
                            .and_then(|bytes| {
                                audio::soundfont::parse(&bytes)
                                    .map_err(|error| format!("{path}: {error}"))
                            }) {
                            Ok(font) => {
                                self.sf2_presets = font
                                    .presets
                                    .iter()
                                    .map(|preset| {
                                        (preset.bank, preset.program, preset.name.clone())
                                    })
                                    .collect();
                                self.status = Some(format!("{} preset(s)", self.sf2_presets.len()));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
                if !self.sf2_presets.is_empty() {
                    let selected = format!("{}:{}", self.sf2_bank, self.sf2_program);
                    egui::ComboBox::from_id_salt("sf2-preset")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            for (bank, program, name) in self.sf2_presets.clone() {
                                if ui
                                    .selectable_label(
                                        self.sf2_bank == bank && self.sf2_program == program,
                                        format!("{name} ({bank}:{program})"),
                                    )
                                    .clicked()
                                {
                                    self.sf2_bank = bank;
                                    self.sf2_program = program;
                                }
                            }
                        });
                }
                let current = self
                    .song()
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .and_then(|track| track.soundfont.as_ref())
                    .map(|font| format!("{}:{} {}", font.bank, font.program, font.path))
                    .unwrap_or_else(|| String::from("built-in timbre"));
                ui.label(
                    egui::RichText::new(format!("Track {track_id}: {current}"))
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                ui.horizontal(|ui| {
                    if ui.button("Use preset").clicked() {
                        let path = self.sf2_path.trim().to_string();
                        let result = if path.is_empty() {
                            Err(String::from("enter an SF2 path first"))
                        } else {
                            std::fs::read(&path)
                                .map_err(|error| format!("{path}: {error}"))
                                .and_then(|bytes| {
                                    audio::soundfont::parse(&bytes)
                                        .map_err(|error| format!("{path}: {error}"))
                                })
                                .and_then(|font| {
                                    let preset = font
                                        .preset(self.sf2_bank, self.sf2_program)
                                        .map(|preset| preset.name.clone())
                                        .unwrap_or_else(|| String::from("unknown preset"));
                                    self.editor
                                        .set_soundfont(
                                            track_id,
                                            Some(project::song::SoundFontRef {
                                                path: path.clone(),
                                                bank: self.sf2_bank,
                                                program: self.sf2_program,
                                            }),
                                        )
                                        .map(|()| {
                                            format!(
                                                "{} -> {}:{} ({preset})",
                                                font.name, self.sf2_bank, self.sf2_program
                                            )
                                        })
                                })
                        };
                        match result {
                            Ok(message) => self.status = Some(message),
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Load current").clicked()
                        && let Some(font) = self
                            .song()
                            .tracks
                            .iter()
                            .find(|track| track.id == track_id)
                            .and_then(|track| track.soundfont.clone())
                    {
                        self.sf2_path = font.path;
                        self.sf2_bank = font.bank;
                        self.sf2_program = font.program;
                    }
                    if ui.button("Clear").clicked() {
                        match self.editor.set_soundfont(track_id, None) {
                            Ok(()) => self.status = Some(String::from("cleared soundfont")),
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
            });
        }
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
            ui.label(egui::RichText::new("No tracks to mix.").color(theme::text_muted()));
            return;
        }
        let buses = self.song().buses.clone();
        for bus in &buses {
            ui.horizontal(|ui| {
                let mut name = bus.name.clone();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut name)
                            .desired_width(80.0)
                            .font(egui::TextStyle::Small),
                    )
                    .changed()
                {
                    let mut updated = buses.clone();
                    if let Some(target) = updated.iter_mut().find(|entry| entry.id == bus.id) {
                        target.name = name;
                    }
                    let _ = self.editor.set_buses(updated);
                }
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
            let index = self
                .song()
                .tracks
                .iter()
                .position(|track| track.id == id)
                .unwrap_or(0);
            let meter = if self
                .player
                .as_ref()
                .is_some_and(audio::player::Player::is_playing)
            {
                let stem_rate = self.stem_sample_rate.max(1);
                let position = self.player.as_ref().map_or(0, |player| {
                    ((player.seconds() * f64::from(stem_rate)).round() as usize).saturating_mul(2)
                });
                self.stem_levels
                    .iter()
                    .find(|(track, _)| *track == id)
                    .map_or_else(audio::master::Meter::default, |(_, samples)| {
                        audio::master::meter_at(samples, position, stem_rate as usize / 20)
                    })
            } else {
                audio::master::Meter::default()
            };
            ui.horizontal(|ui| {
                if index > 0 && ui.small_button("↑").clicked() {
                    let _ = self.editor.move_track(id, index - 1);
                }
                if index + 1 < self.song().tracks.len() && ui.small_button("↓").clicked() {
                    let _ = self.editor.move_track(id, index + 1);
                }
                if ui.small_button("Dup").clicked()
                    && let Ok(copy) = self.editor.duplicate_track(id)
                {
                    self.roll.selected = Some(copy);
                }
                if ui.small_button("Del").clicked() {
                    if self.roll.selected == Some(id) {
                        self.roll.selected = None;
                    }
                    let _ = self.editor.remove_track(id);
                }
                ui.label(
                    egui::RichText::new(if name.is_empty() { "untitled" } else { &name })
                        .size(11.0),
                );
                ui.add(
                    egui::ProgressBar::new(meter.peak.clamp(0.0, 1.0))
                        .desired_width(54.0)
                        .text(format!("{:.0}", meter.peak * 100.0)),
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
                if ui
                    .small_button("Freeze")
                    .on_hover_text("Render this track to audio and clear its notes")
                    .clicked()
                {
                    let folder = crate::settings::Settings::data_dir();
                    let _ = std::fs::create_dir_all(&folder);
                    let path = folder.join(format!("freeze-{id}.wav"));
                    match self.editor.freeze_track(id, &path) {
                        Ok(()) => {
                            self.status = Some(format!("froze {id} -> {}", path.display()));
                        }
                        Err(error) => self.status = Some(error),
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
                .color(theme::text_dim()),
        );
    }

    pub(super) fn patch_window(&mut self, context: &Context) {
        let mut open = self.patch_open;
        egui::Window::new("Synth patch")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                let track = self
                    .roll
                    .selected
                    .or_else(|| self.song().tracks.first().map(|track| track.id));
                let Some(track) = track else {
                    ui.label("Add a track first.");
                    return;
                };
                if self.patch_track != Some(track) || self.patch_draft.is_none() {
                    self.patch_track = Some(track);
                    self.patch_draft = Some(
                        self.song()
                            .tracks
                            .iter()
                            .find(|entry| entry.id == track)
                            .and_then(|entry| entry.patch)
                            .unwrap_or_default(),
                    );
                }
                let Some(mut patch) = self.patch_draft else {
                    return;
                };
                let waveform_name = |waveform: project::song::Waveform| match waveform {
                    project::song::Waveform::Sine => "Sine",
                    project::song::Waveform::Saw => "Saw",
                    project::song::Waveform::Square => "Square",
                    project::song::Waveform::Triangle => "Triangle",
                };
                egui::ComboBox::from_id_salt("patch-waveform")
                    .selected_text(waveform_name(patch.waveform))
                    .show_ui(ui, |ui| {
                        for waveform in [
                            project::song::Waveform::Sine,
                            project::song::Waveform::Saw,
                            project::song::Waveform::Square,
                            project::song::Waveform::Triangle,
                        ] {
                            ui.selectable_value(
                                &mut patch.waveform,
                                waveform,
                                waveform_name(waveform),
                            );
                        }
                    });
                ui.add(
                    egui::Slider::new(&mut patch.attack, 0.001..=2.0)
                        .text("Attack")
                        .logarithmic(true),
                );
                ui.add(egui::Slider::new(&mut patch.decay, 0.01..=3.0).text("Decay"));
                ui.add(egui::Slider::new(&mut patch.sustain, 0.0..=1.0).text("Sustain"));
                ui.add(egui::Slider::new(&mut patch.release, 0.0..=3.0).text("Release"));
                ui.add(egui::Slider::new(&mut patch.brightness, 0.0..=1.0).text("Brightness"));
                self.patch_draft = Some(patch);
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        match self.editor.set_track_patch(track, Some(patch)) {
                            Ok(()) => self.status = Some(String::from("synth patch applied")),
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Default").clicked() {
                        self.patch_draft = Some(project::song::Patch::default());
                    }
                    if ui.button("Clear").clicked() {
                        match self.editor.set_track_patch(track, None) {
                            Ok(()) => {
                                self.patch_draft = Some(project::song::Patch::default());
                                self.status = Some(String::from("synth patch cleared"));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
            });
        self.patch_open = open;
    }
}
