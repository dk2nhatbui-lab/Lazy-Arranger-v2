//! A project names its VST3 plug-ins by path. Bundles outside the standard
//! VST3 folders load only after the user allows them; the answer is saved
//! in the data folder so it is asked once per bundle.

use std::path::{Path, PathBuf};

use super::ShellApp;

/// Saved approvals, one bundle path per entry.
const APPROVED_FILE: &str = "approved-plugins.json";

fn approved_path() -> PathBuf {
    crate::settings::data_file(APPROVED_FILE)
}

fn read_approved(path: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<PathBuf>>(&text).ok())
        .unwrap_or_default()
}

/// Approves the bundles the user allowed before. Call once at startup.
pub(super) fn load_approvals() {
    for bundle in read_approved(&approved_path()) {
        plugin_host::approve_vst3_bundle(&bundle);
    }
}

/// The VST3 bundles in `song` that would not load yet, without repeats.
pub(super) fn untrusted(song: &project::song::Song) -> Vec<PathBuf> {
    let slots = song
        .tracks
        .iter()
        .flat_map(|track| &track.plugins)
        .chain(song.buses.iter().flat_map(|bus| &bus.plugins))
        .chain(&song.master_plugins);
    let mut found: Vec<PathBuf> = Vec::new();
    for slot in slots {
        if slot.format != project::song::PluginFormat::Vst3 || slot.path.is_empty() {
            continue;
        }
        let path = PathBuf::from(&slot.path);
        if !plugin_host::is_vst3_bundle_trusted(&path) && !found.contains(&path) {
            found.push(path);
        }
    }
    found
}

fn remember(bundles: &[PathBuf]) -> Result<(), String> {
    let path = approved_path();
    let mut approved = read_approved(&path);
    for bundle in bundles {
        if !approved.contains(bundle) {
            approved.push(bundle.clone());
        }
    }
    let json = serde_json::to_string_pretty(&approved).map_err(|error| error.to_string())?;
    crate::persistence::save_atomic(&path, &json)
}

impl ShellApp {
    /// Asks whether the open project's unknown plug-ins may run.
    pub(super) fn plugin_trust_dialog(&mut self, context: &egui::Context) {
        if self.untrusted_plugins.is_empty() {
            return;
        }
        let mut open = true;
        let mut answer = None;
        egui::Window::new("Allow plug-ins from this project?")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(
                    "This project uses plug-ins outside your VST3 folders. A plug-in \
                     runs with this app's permissions, so allow only ones you trust.",
                );
                for bundle in &self.untrusted_plugins {
                    ui.monospace(bundle.display().to_string());
                }
                ui.label("Until you allow them, these plug-ins stay silent.");
                ui.horizontal(|ui| {
                    if ui.button("Allow").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Not now").clicked() {
                        answer = Some(false);
                    }
                });
            });
        if answer == Some(true) {
            for bundle in &self.untrusted_plugins {
                plugin_host::approve_vst3_bundle(bundle);
            }
            if let Err(error) = remember(&self.untrusted_plugins) {
                self.status = Some(format!("plug-ins allowed for now; not saved: {error}"));
            }
            // Players built before the answer skipped these plug-ins.
            self.stop_playback();
        }
        if answer.is_some() || !open {
            self.untrusted_plugins.clear();
        }
    }
}
