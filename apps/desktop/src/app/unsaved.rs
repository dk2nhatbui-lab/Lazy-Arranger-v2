//! New, Open, demo loads and Quit all pass through one Save / Discard /
//! Cancel question when the project has unsaved edits. A project that was
//! never saved has no autosave, so without this a menu click could lose it.

use std::path::PathBuf;

use project::song::ProjectFile;

use super::{FileMode, ShellApp};

/// What waits for the user's answer.
pub(crate) enum PendingSwap {
    /// Open this file.
    Open(PathBuf),
    /// Load this project, with the label the status line shows.
    Replace(Box<ProjectFile>, String),
    /// Close the window.
    Quit,
}

impl ShellApp {
    /// Opens a project file, asking first when there are unsaved edits.
    pub(super) fn open(&mut self, path: &std::path::Path) {
        self.when_saved_or_discarded(PendingSwap::Open(path.to_path_buf()));
    }

    /// Loads a project with no file, asking first when there are unsaved edits.
    pub(super) fn replace_project(&mut self, project: ProjectFile, label: &str) {
        self.when_saved_or_discarded(PendingSwap::Replace(Box::new(project), label.to_string()));
    }

    /// Closes the window, asking first when there are unsaved edits.
    pub(super) fn quit(&mut self, context: &egui::Context) {
        if self.asks_before_losing_edits() && !self.quit_confirmed {
            self.pending_swap = Some(PendingSwap::Quit);
        } else {
            self.quit_confirmed = true;
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// True when there are unsaved edits and the user wants to be asked.
    /// With the question turned off the autosave is kept, so the edits can
    /// still be recovered.
    fn asks_before_losing_edits(&self) -> bool {
        self.editor.dirty() && self.settings.confirm_unsaved
    }

    fn when_saved_or_discarded(&mut self, swap: PendingSwap) {
        if self.asks_before_losing_edits() {
            self.pending_swap = Some(swap);
        } else {
            self.run_swap(swap);
        }
    }

    fn run_swap(&mut self, swap: PendingSwap) {
        match swap {
            PendingSwap::Open(path) => self.open_now(&path),
            PendingSwap::Replace(project, label) => self.replace_project_now(*project, &label),
            PendingSwap::Quit => {
                self.quit_confirmed = true;
                self.close_requested = true;
            }
        }
    }

    /// Turns the window's close button into the same question.
    pub(super) fn guard_close(&mut self, context: &egui::Context) {
        if self.close_requested {
            self.close_requested = false;
            context.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if context.input(|input| input.viewport().close_requested())
            && self.asks_before_losing_edits()
            && !self.quit_confirmed
        {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending_swap = Some(PendingSwap::Quit);
        }
    }

    /// Runs the waiting action once a save finished.
    pub(super) fn after_save(&mut self) {
        if !self.editor.dirty()
            && let Some(swap) = self.after_save_swap.take()
        {
            self.run_swap(swap);
        }
    }

    /// The Save / Discard / Cancel question.
    pub(super) fn unsaved_dialog(&mut self, context: &egui::Context) {
        if self.pending_swap.is_none() {
            return;
        }
        let name = self.project_path.as_ref().map_or_else(
            || self.song().name.clone(),
            |path| {
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                )
            },
        );
        let mut choice = None;
        let modal = crate::dialog::modal(context, "unsaved-changes", 420.0, |ui| {
            crate::dialog::title(ui, "Save changes?");
            crate::dialog::body(
                ui,
                format!(
                    "\u{201c}{name}\u{201d} has unsaved changes. Save them before you continue?"
                ),
            );
            ui.add_space(4.0);
            crate::dialog::note(ui, "You can turn this question off in Settings.");
            crate::dialog::buttons(ui, |ui| {
                if crate::dialog::primary(ui, "Save").clicked() || crate::dialog::enter_pressed(ui)
                {
                    choice = Some(Choice::Save);
                }
                if crate::dialog::secondary(ui, "Cancel").clicked() {
                    choice = Some(Choice::Cancel);
                }
                if crate::dialog::secondary(ui, "Don\u{2019}t Save").clicked() {
                    choice = Some(Choice::Discard);
                }
            });
        });
        if modal.should_close() && choice.is_none() {
            // Escape or a click on the backdrop.
            choice = Some(Choice::Cancel);
        }
        let Some(choice) = choice else { return };
        let Some(swap) = self.pending_swap.take() else {
            return;
        };
        match choice {
            Choice::Cancel => {}
            Choice::Discard => {
                // Discarding drops the autosave too, so it is not offered back.
                if let Some(path) = &self.project_path {
                    let _ = std::fs::remove_file(crate::persistence::autosave_path(path));
                }
                self.run_swap(swap);
            }
            Choice::Save => {
                self.after_save_swap = Some(swap);
                match self.project_path.clone() {
                    Some(path) => self.save_to(path),
                    None => self.choose_file(FileMode::SaveAs),
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Choice {
    Save,
    Discard,
    Cancel,
}
