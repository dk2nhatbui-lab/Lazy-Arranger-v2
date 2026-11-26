//! Right-click menus on notes in the piano roll and the score, as in the
//! C++ app.
//!
//! The roll and the score report a right-click in their outcome; the app
//! keeps it in [`ShellApp::notes_menu`] and this module shows the menu and
//! runs what is chosen. The clipboard items go through the same code as the
//! keyboard shortcuts.

use egui::Context;
use music_core::Tick;
use project::NoteId;

use super::{KeyPresses, ShellApp};
use crate::editor::snap;

/// Where a notes menu was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NotesPlace {
    /// The piano roll.
    Roll,
    /// The score.
    Score,
}

/// An open notes menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct NotesMenu {
    pub(super) place: NotesPlace,
    pub(super) pos: egui::Pos2,
    /// Tick under the pointer, where Paste Here pastes. `None` pastes at
    /// the playhead.
    pub(super) tick: Option<Tick>,
}

impl NotesMenu {
    /// A menu at the pointer.
    pub(super) fn at_pointer(context: &Context, place: NotesPlace, tick: Option<Tick>) -> Self {
        let pos = context
            .input(|input| input.pointer.interact_pos())
            .unwrap_or_default();
        Self { place, pos, tick }
    }
}

/// Something a notes menu item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoteAction {
    Cut,
    Copy,
    Paste(Option<Tick>),
    Delete,
    SelectAll,
    Quantise,
    ChordSymbol,
    Lyrics,
    ScoreMarks,
    FindChords,
}

impl ShellApp {
    /// Shows the open notes menu, if any.
    pub(super) fn notes_menu(&mut self, context: &Context) {
        let Some(menu) = self.notes_menu else {
            return;
        };
        let (chosen, open) = super::popup_menu::show(
            context,
            egui::Id::new("notes-context-menu"),
            menu.pos,
            |ui| self.notes_menu_items(ui, menu),
        );
        if let Some(action) = chosen.flatten() {
            self.notes_menu = None;
            self.run_note_action(action);
        } else if !open {
            self.notes_menu = None;
        }
    }

    fn notes_menu_items(&self, ui: &mut egui::Ui, menu: NotesMenu) -> Option<NoteAction> {
        let selection = !self.selected_notes.is_empty() || self.selected_note.is_some();
        let mut chosen = None;
        let mut item = |ui: &mut egui::Ui, label: &str, enabled: bool, action: NoteAction| {
            if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                chosen = Some(action);
            }
        };
        item(ui, "Cut", selection, NoteAction::Cut);
        item(ui, "Copy", selection, NoteAction::Copy);
        item(
            ui,
            if menu.tick.is_some() {
                "Paste Here"
            } else {
                "Paste at Playhead"
            },
            self.editor.has_note_clipboard(),
            NoteAction::Paste(menu.tick),
        );
        item(ui, "Delete", selection, NoteAction::Delete);
        ui.separator();
        item(
            ui,
            "Select All",
            self.menu_track().is_some(),
            NoteAction::SelectAll,
        );
        item(
            ui,
            "Quantise to 1/16",
            self.menu_track().is_some(),
            NoteAction::Quantise,
        );
        if menu.place == NotesPlace::Score {
            ui.separator();
            item(ui, "Chord Symbol…  (K)", true, NoteAction::ChordSymbol);
            item(ui, "Lyrics…", true, NoteAction::Lyrics);
            item(
                ui,
                "Dynamics and Articulations…",
                true,
                NoteAction::ScoreMarks,
            );
            ui.separator();
            item(ui, "Find Chords in the Notes", true, NoteAction::FindChords);
        }
        chosen
    }

    /// Runs a notes menu item.
    pub(super) fn run_note_action(&mut self, action: NoteAction) {
        let keys = KeyPresses::default();
        match action {
            NoteAction::Cut => self.handle_clipboard_keys(KeyPresses { cut: true, ..keys }),
            NoteAction::Copy => self.handle_clipboard_keys(KeyPresses { copy: true, ..keys }),
            NoteAction::Delete => self.handle_clipboard_keys(KeyPresses {
                delete: true,
                ..keys
            }),
            NoteAction::Paste(tick) => {
                let hover = self.roll.hover;
                self.roll.hover = tick;
                self.handle_clipboard_keys(KeyPresses {
                    paste: true,
                    ..keys
                });
                self.roll.hover = hover;
            }
            NoteAction::SelectAll => self.select_all_notes(),
            NoteAction::Quantise => {
                let Some(track) = self.menu_track() else {
                    return;
                };
                let mut ids: Vec<NoteId> = self
                    .selected_notes
                    .iter()
                    .chain(self.selected_note.iter())
                    .filter(|(entry, _)| *entry == track)
                    .map(|(_, note)| *note)
                    .collect();
                if ids.is_empty() {
                    ids = self
                        .song()
                        .tracks
                        .iter()
                        .find(|entry| entry.id == track)
                        .map(|entry| entry.notes.iter().map(|note| note.id).collect())
                        .unwrap_or_default();
                }
                match self.quantise_notes(track, &ids) {
                    Ok(Some(message)) => self.status = Some(message),
                    Ok(None) => {}
                    Err(error) => self.status = Some(error),
                }
            }
            NoteAction::ChordSymbol => {
                let at = self
                    .selected_note
                    .and_then(|(track, note)| self.note_start(track, note))
                    .unwrap_or(self.position);
                self.chord_dialog_target = snap(at, self.song().time_signature.bar_ticks().max(1));
                self.chord_dialog = true;
            }
            NoteAction::Lyrics => self.lyrics_open = true,
            NoteAction::ScoreMarks => self.marks_open = true,
            NoteAction::FindChords => match self.editor.find_chords() {
                Ok(0) => self.status = Some(String::from("no chords found")),
                Ok(count) => self.status = Some(format!("found {count} chord(s)")),
                Err(error) => self.status = Some(error),
            },
        }
    }

    fn note_start(&self, track: project::TrackId, note: NoteId) -> Option<Tick> {
        self.song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)?
            .notes
            .iter()
            .find(|entry| entry.id == note)
            .map(|entry| entry.start)
    }
}
