//! The instrument chooser: which hardware-specific editors are installed.

use super::ShellApp;
use super::devices::load_device_profiles_for;
use crate::instruments::{Installed, Instrument};
use crate::theme;
use egui::Context;

impl ShellApp {
    /// Shows the chooser on first launch and from Settings or the menu.
    pub(super) fn instruments_window(&mut self, context: &Context) {
        if !self.instruments_open {
            return;
        }
        let mut open = true;
        let mut choice = self.instruments;
        let mut done = false;
        egui::Window::new("Instrument editors")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label("Tick the instruments you own. Editors for the others stay hidden.");
                ui.label(
                    egui::RichText::new("You can change this any time in Settings.")
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                ui.add_space(6.0);
                for instrument in Instrument::ALL {
                    let mut on = choice.has(instrument);
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut on, instrument.name()).changed() {
                            choice.set(instrument, on);
                        }
                        ui.label(
                            egui::RichText::new(instrument.adds())
                                .size(10.0)
                                .color(theme::text_dim()),
                        );
                    });
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("All").clicked() {
                        choice = Installed::all();
                    }
                    if ui.button("None").clicked() {
                        choice = Installed::none();
                    }
                    if ui.button("Done").clicked() {
                        done = true;
                    }
                });
            });
        if choice != self.instruments {
            self.apply_instruments(choice);
            self.save_instruments();
        }
        if done || !open {
            // Closing also records the choice, so a fresh install asks once.
            self.save_instruments();
            self.instruments_open = false;
        }
    }

    /// Switches to `choice`, closing the windows of removed instruments and
    /// hiding their device profiles. The caller saves.
    pub(super) fn apply_instruments(&mut self, choice: Installed) {
        self.instruments = choice;
        if !choice.has(Instrument::RolandD05) {
            self.d05.open = false;
        }
        if !choice.has(Instrument::ZoomL6max) {
            self.l6max.device_open = false;
            self.l6max.mixer_open = false;
        }
        if !choice.has(Instrument::YamahaPsrS970) {
            self.psr.open = false;
        }
        if !choice.has(Instrument::YamahaXg) {
            self.xg.open = false;
        }
        self.device_profiles = load_device_profiles_for(&choice);
        self.device_editor_profile = 0;
        self.device_editor_values.clear();
    }

    fn save_instruments(&mut self) {
        if let Err(error) = self.instruments.save(&crate::instruments::path()) {
            self.status = Some(format!("instrument editors not saved: {error}"));
        }
    }
}
