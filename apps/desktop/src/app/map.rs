//! The MIDI Map view, drawn like v1 `MidiMapView`: a title with the
//! controller and the track bank, a controller summary on the left, a table
//! of tracks by app controls where each cell shows the knob, button or pad
//! bound to it, a Global row, and the channel check and MIDI learn notes.

use super::*;
use midi_devices::map::{Action, Control};

/// Says whether an action belongs to a summary row.
type ActionTest = fn(&Action) -> bool;

const LEFT: f32 = 318.0;
const ROW: f32 = 36.0;
const TRACK_COLUMN: f32 = 196.0;
const COLUMN: f32 = 68.0;
const BANK: usize = 8;
const CLIP_COLUMNS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Column {
    Volume,
    Pan,
    Send,
    Mute,
    Solo,
    Arm,
    Clip(usize),
}

impl Column {
    fn title(self) -> String {
        match self {
            Self::Volume => String::from("VOLUME"),
            Self::Pan => String::from("PAN"),
            Self::Send => String::from("SEND"),
            Self::Mute => String::from("MUTE"),
            Self::Solo => String::from("SOLO"),
            Self::Arm => String::from("ARM"),
            Self::Clip(scene) => format!("CLIP {}", scene + 1),
        }
    }

    fn action(self, track: TrackId) -> Action {
        match self {
            Self::Volume => Action::TrackFader(track),
            Self::Pan => Action::TrackPan(track),
            Self::Send => Action::TrackSend(track),
            Self::Mute => Action::TrackMute(track),
            Self::Solo => Action::TrackSolo(track),
            Self::Arm => Action::TrackArm(track),
            Self::Clip(scene) => Action::Clip(track, scene),
        }
    }
}

const GLOBAL: [(Action, &str); 7] = [
    (Action::Master, "Master"),
    (Action::Play, "Play"),
    (Action::Stop, "Stop"),
    (Action::Record, "Record"),
    (Action::Loop, "Loop"),
    (Action::Undo, "Undo"),
    (Action::StopClips, "Stop clips"),
];

fn control_label(control: Control) -> String {
    match control {
        Control::Controller(number) => format!("CC {number}"),
        Control::Note(number) => format!("NOTE {number}"),
    }
}

impl ShellApp {
    fn save_midi_map(&mut self) {
        if let Err(error) =
            std::fs::write(crate::settings::midi_map_path(), self.midi_map.to_text())
        {
            self.status = Some(format!("midi map: {error}"));
        }
    }

    fn bound_control(&self, action: Action) -> Option<Control> {
        self.midi_map
            .bindings
            .iter()
            .find(|binding| binding.action == action)
            .map(|binding| binding.control)
    }

    /// One table cell: the bound control, "+" when free, or the learn prompt.
    fn map_cell(&mut self, ui: &mut egui::Ui, rect: egui::Rect, action: Action, label: &str) {
        let rect = rect.shrink2(egui::vec2(3.0, 5.0));
        let bound = self.bound_control(action);
        let learning = self.midi_learn.is_some_and(|learn| learn.action == action);
        let response = ui.interact(
            rect,
            ui.id().with(("map-cell", action.label())),
            egui::Sense::click(),
        );
        let painter = ui.painter();
        let (fill, stroke, text, colour) = if learning {
            (
                theme::accent().gamma_multiply(0.25),
                theme::accent(),
                String::from("Move..."),
                theme::text(),
            )
        } else if let Some(control) = bound {
            (
                theme::recording().gamma_multiply(0.28),
                theme::recording().gamma_multiply(0.75),
                control_label(control),
                theme::text(),
            )
        } else {
            (
                theme::raised(),
                theme::line(),
                String::from("+"),
                theme::text_dim(),
            )
        };
        let fill = if response.hovered() {
            fill.lerp_to_gamma(theme::text(), 0.06)
        } else {
            fill
        };
        painter.rect_filled(rect, theme::RADIUS, fill);
        painter.rect_stroke(
            rect,
            theme::RADIUS,
            egui::Stroke::new(1.0_f32, stroke),
            egui::StrokeKind::Inside,
        );
        widgets::fitted_text(
            painter,
            rect,
            &text,
            if bound.is_some() || learning {
                theme::semibold(12.0)
            } else {
                egui::FontId::proportional(14.0)
            },
            colour,
        );
        let response = response.on_hover_text(match bound {
            Some(control) => format!(
                "{label}: {}. Click to learn again or clear.",
                control.label()
            ),
            None => format!("{label}: click, then move a knob or press a pad"),
        });
        if response.clicked() {
            self.midi_learn = if learning {
                None
            } else {
                Some(midi_devices::map::Learn::begin(action))
            };
        }
        let mut clear = false;
        let mut learn = false;
        response.context_menu(|ui| {
            learn = ui.button("Learn this cell").clicked();
            clear = ui
                .add_enabled(bound.is_some(), egui::Button::new("Clear"))
                .clicked();
            if learn || clear {
                ui.close_menu();
            }
        });
        if learn {
            self.midi_learn = Some(midi_devices::map::Learn::begin(action));
        }
        if clear && let Some(control) = bound {
            self.midi_map.unbind(control);
            self.save_midi_map();
        }
    }

    /// The MIDI Map view.
    pub(super) fn map_panel(&mut self, ui: &mut egui::Ui) {
        let (full, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let song = self.song().clone();
        let painter = ui.painter().clone();

        // Title and subtitle, with the accent rule under them.
        painter.text(
            egui::pos2(full.left() + 4.0, full.top() + 6.0),
            egui::Align2::LEFT_TOP,
            "MIDI Map",
            theme::semibold(22.0),
            theme::text(),
        );
        painter.text(
            egui::pos2(full.left() + 4.0, full.top() + 38.0),
            egui::Align2::LEFT_TOP,
            "All instruments by all app controls. Click a cell, then move a knob or press a pad.",
            egui::FontId::proportional(13.0),
            theme::text_muted(),
        );
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(full.left() + 4.0, full.top() + 60.0),
                egui::vec2(60.0, 2.0),
            ),
            0.0,
            theme::accent(),
        );

        // Right of the title: presets, the bank and its arrows.
        let pages = song.tracks.len().div_ceil(BANK).max(1);
        self.map_bank = self.map_bank.min(pages - 1);
        let first = self.map_bank * BANK;
        let last = (first + BANK).min(song.tracks.len());
        let top = egui::Rect::from_min_max(
            egui::pos2(full.left(), full.top() + 10.0),
            egui::pos2(full.right() - 8.0, full.top() + 42.0),
        );
        let next = egui::Rect::from_min_max(egui::pos2(top.right() - 26.0, top.top()), top.max);
        let previous = next.translate(egui::vec2(-30.0, 0.0));
        let bank = egui::Rect::from_min_max(
            egui::pos2(previous.left() - 110.0, top.top()),
            egui::pos2(previous.left() - 10.0, top.bottom()),
        );
        let preset = egui::Rect::from_min_max(
            egui::pos2(bank.left() - 220.0, top.top()),
            egui::pos2(bank.left() - 16.0, top.bottom()),
        );
        painter.text(
            bank.center(),
            egui::Align2::CENTER_CENTER,
            if song.tracks.is_empty() {
                String::from("No tracks")
            } else {
                format!("Tracks {}\u{2013}{}", first + 1, last)
            },
            egui::FontId::proportional(14.0),
            theme::text_muted(),
        );
        if widgets::text_button(ui, previous, "map-bank-left", "<", false)
            .on_hover_text("Earlier tracks")
            .clicked()
        {
            self.map_bank = self.map_bank.saturating_sub(1);
        }
        if widgets::text_button(ui, next, "map-bank-right", ">", false)
            .on_hover_text("Later tracks")
            .clicked()
        {
            self.map_bank = (self.map_bank + 1).min(pages - 1);
        }
        let mut preset_ui = layout::toolbar_child(ui, preset);
        let mut load = None;
        egui::ComboBox::from_id_salt("map-preset")
            .width(preset.width() - 8.0)
            .selected_text("Your MIDI map")
            .show_ui(&mut preset_ui, |ui| {
                if ui.button("Launchkey faders: CC 21-28").clicked() {
                    load = Some(midi_devices::map::launchkey_mixer_map());
                }
                if ui.button("Launchkey pads: notes 36-51").clicked() {
                    load = Some(midi_devices::map::launchkey_pad_map());
                }
                ui.separator();
                if ui.button("Clear the map").clicked() {
                    load = Some(midi_devices::map::MidiMap::default());
                }
            })
            .response
            .on_hover_text("Start from a ready-made map; it adds to yours");
        if let Some(map) = load {
            if map.bindings.is_empty() {
                self.midi_map = map;
            } else {
                for binding in map.bindings {
                    self.midi_map.bind(binding.control, binding.action);
                }
            }
            self.save_midi_map();
        }

        let body_top = full.top() + 76.0;
        let notes_height = 96.0;

        // Left: the controller summary.
        let left = egui::Rect::from_min_max(
            egui::pos2(full.left(), body_top),
            egui::pos2(full.left() + LEFT, full.bottom()),
        );
        painter.rect_filled(left, theme::RADIUS, theme::panel());
        let mut y = left.top() + 22.0;
        painter.text(
            egui::pos2(left.left() + 18.0, y),
            egui::Align2::LEFT_TOP,
            "CONTROLLER",
            theme::semibold(11.0),
            theme::accent(),
        );
        y += 22.0;
        painter.text(
            egui::pos2(left.left() + 18.0, y),
            egui::Align2::LEFT_TOP,
            "Your MIDI map",
            theme::semibold(20.0),
            theme::text(),
        );
        y += 30.0;
        painter.text(
            egui::pos2(left.left() + 18.0, y),
            egui::Align2::LEFT_TOP,
            format!(
                "{} controls \u{b7} learned with MIDI learn",
                self.midi_map.bindings.len()
            ),
            egui::FontId::proportional(13.0),
            theme::text_muted(),
        );
        y += 36.0;
        let kinds: [(&str, ActionTest); 9] = [
            ("Clip", |action| {
                matches!(action, Action::Clip(..) | Action::Scene(_))
            }),
            ("Volume", |action| matches!(action, Action::TrackFader(_))),
            ("Pan", |action| matches!(action, Action::TrackPan(_))),
            ("Send", |action| matches!(action, Action::TrackSend(_))),
            ("Mute", |action| matches!(action, Action::TrackMute(_))),
            ("Solo", |action| matches!(action, Action::TrackSolo(_))),
            ("Arm", |action| matches!(action, Action::TrackArm(_))),
            ("Master", |action| matches!(action, Action::Master)),
            ("Transport", |action| {
                matches!(
                    action,
                    Action::Play
                        | Action::Stop
                        | Action::Record
                        | Action::Loop
                        | Action::Undo
                        | Action::StopClips
                )
            }),
        ];
        for (name, test) in kinds {
            let controls: Vec<Control> = self
                .midi_map
                .bindings
                .iter()
                .filter(|binding| test(&binding.action))
                .map(|binding| binding.control)
                .collect();
            let any = !controls.is_empty();
            let mark = egui::Rect::from_center_size(
                egui::pos2(left.left() + 30.0, y + 9.0),
                egui::vec2(16.0, 16.0),
            );
            if any {
                painter.rect_filled(mark, theme::RADIUS, theme::ok());
            } else {
                painter.rect_stroke(
                    mark,
                    theme::RADIUS,
                    egui::Stroke::new(1.5_f32, theme::text_dim()),
                    egui::StrokeKind::Inside,
                );
            }
            painter.text(
                egui::pos2(left.left() + 50.0, y + 9.0),
                egui::Align2::LEFT_CENTER,
                name,
                theme::semibold(14.0),
                if any {
                    theme::text()
                } else {
                    theme::text_muted()
                },
            );
            let knobs = controls
                .iter()
                .filter(|control| matches!(control, Control::Controller(_)))
                .count();
            let count = match (knobs, controls.len() - knobs) {
                (0, 0) => String::from("none"),
                (knobs, 0) => format!("{knobs} knob{}", if knobs == 1 { "" } else { "s" }),
                (0, buttons) => format!("{buttons} button{}", if buttons == 1 { "" } else { "s" }),
                (knobs, buttons) => format!("{knobs} knobs, {buttons} buttons"),
            };
            painter.text(
                egui::pos2(left.left() + 138.0, y + 9.0),
                egui::Align2::LEFT_CENTER,
                count,
                egui::FontId::proportional(13.0),
                theme::text_dim(),
            );
            y += 33.0;
        }
        painter.text(
            egui::pos2(left.left() + 18.0, left.bottom() - 24.0),
            egui::Align2::LEFT_CENTER,
            "Knobs = value \u{b7} buttons = switch \u{b7} pads = clip",
            egui::FontId::proportional(12.0),
            theme::text_dim(),
        );

        // The table.
        let table = egui::Rect::from_min_max(
            egui::pos2(left.right() + 14.0, body_top),
            egui::pos2(full.right(), full.bottom() - notes_height - 10.0),
        );
        painter.rect_filled(table, theme::RADIUS, theme::panel());
        let mut columns = vec![
            Column::Volume,
            Column::Pan,
            Column::Send,
            Column::Mute,
            Column::Solo,
            Column::Arm,
        ];
        columns.extend((0..CLIP_COLUMNS).map(Column::Clip));
        let width = TRACK_COLUMN + columns.len() as f32 * (COLUMN + 6.0) + 16.0;
        let mut table_ui = layout::toolbar_child(ui, table);
        egui::ScrollArea::both()
            .id_salt("map-table")
            .show(&mut table_ui, |ui| {
                let (area, _) = ui.allocate_exact_size(
                    egui::vec2(
                        width.max(table.width() - 12.0),
                        54.0 + 6.0 + (last - first) as f32 * ROW + 14.0 + ROW + 24.0,
                    ),
                    egui::Sense::hover(),
                );
                let header = egui::Rect::from_min_size(area.min, egui::vec2(area.width(), 54.0));
                ui.painter()
                    .rect_filled(header, theme::RADIUS, theme::raised());
                let column_x =
                    |index: usize| area.left() + TRACK_COLUMN + index as f32 * (COLUMN + 6.0);
                ui.painter().text(
                    egui::pos2(area.left() + 16.0, header.center().y),
                    egui::Align2::LEFT_CENTER,
                    "TRACK",
                    theme::semibold(11.0),
                    theme::text_dim(),
                );
                for (index, column) in columns.iter().enumerate() {
                    ui.painter().text(
                        egui::pos2(column_x(index) + COLUMN / 2.0, header.center().y),
                        egui::Align2::CENTER_CENTER,
                        column.title(),
                        theme::semibold(11.0),
                        theme::text_dim(),
                    );
                }
                let mut y = header.bottom() + 6.0;
                for (offset, track) in song.tracks[first..last].iter().enumerate() {
                    let index = first + offset;
                    let row = egui::Rect::from_min_size(
                        egui::pos2(area.left(), y),
                        egui::vec2(area.width(), ROW),
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(row.left() + 6.0, row.top() + 6.0),
                            egui::vec2(3.0, ROW - 12.0),
                        ),
                        0.0,
                        theme::track_colour(index),
                    );
                    let channel = track
                        .notes
                        .first()
                        .map_or(index as u8 % 16 + 1, |note| note.channel.zero_based() + 1);
                    let name_width = ui
                        .painter()
                        .layout_no_wrap(
                            track.name.clone(),
                            egui::FontId::proportional(14.0),
                            theme::text(),
                        )
                        .size()
                        .x
                        .min(TRACK_COLUMN - 60.0);
                    widgets::fitted_text(
                        ui.painter(),
                        egui::Rect::from_min_size(
                            egui::pos2(row.left() + 16.0, row.top() + 1.0),
                            egui::vec2(name_width, 20.0),
                        ),
                        &track.name,
                        egui::FontId::proportional(14.0),
                        theme::text(),
                    );
                    ui.painter().text(
                        egui::pos2(row.left() + 16.0, row.top() + 22.0),
                        egui::Align2::LEFT_TOP,
                        if track.audio.is_some() {
                            String::from("Audio")
                        } else {
                            format!("SoundFont \u{b7} Ch {channel}")
                        },
                        egui::FontId::proportional(11.0),
                        theme::text_dim(),
                    );
                    ui.painter().text(
                        egui::pos2(row.left() + TRACK_COLUMN - 14.0, row.center().y),
                        egui::Align2::RIGHT_CENTER,
                        format!("{:02}", index + 1),
                        egui::FontId::proportional(13.0),
                        theme::text_dim(),
                    );
                    for (column_index, column) in columns.iter().enumerate() {
                        let cell = egui::Rect::from_min_size(
                            egui::pos2(column_x(column_index), row.top()),
                            egui::vec2(COLUMN, ROW),
                        );
                        let label = format!("{} {}", track.name, column.title().to_lowercase());
                        self.map_cell(ui, cell, column.action(track.id), &label);
                    }
                    y += ROW;
                }
                // Global controls.
                y += 6.0;
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(area.left(), y),
                        egui::vec2(area.width(), 1.0),
                    ),
                    0.0,
                    theme::line(),
                );
                y += 8.0;
                ui.painter().text(
                    egui::pos2(area.left() + 16.0, y + ROW / 2.0),
                    egui::Align2::LEFT_CENTER,
                    "GLOBAL",
                    theme::semibold(11.0),
                    theme::text_dim(),
                );
                for (index, (action, name)) in GLOBAL.iter().enumerate() {
                    let cell = egui::Rect::from_min_size(
                        egui::pos2(column_x(index), y),
                        egui::vec2(COLUMN, ROW),
                    );
                    ui.painter().text(
                        egui::pos2(cell.center().x, cell.bottom() + 6.0),
                        egui::Align2::CENTER_TOP,
                        name.to_uppercase(),
                        egui::FontId::proportional(9.0),
                        theme::text_dim(),
                    );
                    self.map_cell(ui, cell, *action, name);
                }
            });

        // Channel check and MIDI learn.
        let notes = egui::Rect::from_min_max(
            egui::pos2(table.left(), full.bottom() - notes_height),
            full.max,
        );
        painter.rect_filled(notes, theme::RADIUS, theme::panel());
        let half = notes.width() / 2.0;
        painter.text(
            notes.min + egui::vec2(18.0, 14.0),
            egui::Align2::LEFT_TOP,
            "CHANNEL CHECK",
            theme::semibold(11.0),
            theme::accent(),
        );
        let mut shared = None;
        let channels: Vec<(String, u8)> = song
            .tracks
            .iter()
            .filter(|track| track.audio.is_none())
            .filter_map(|track| {
                track
                    .notes
                    .first()
                    .map(|note| (track.name.clone(), note.channel.zero_based() + 1))
            })
            .collect();
        'outer: for (index, (name, channel)) in channels.iter().enumerate() {
            for (other, other_channel) in &channels[index + 1..] {
                if channel == other_channel && *channel != 10 {
                    shared = Some((name.clone(), other.clone(), *channel));
                    break 'outer;
                }
            }
        }
        let wrap = |text: &str, colour: egui::Color32| {
            ui.painter().layout(
                text.to_string(),
                egui::FontId::proportional(12.0),
                colour,
                half - 36.0,
            )
        };
        painter.galley(
            notes.min + egui::vec2(18.0, 32.0),
            wrap(
                "The app plays each track on its own MIDI channel. Set each instrument to the same channel.",
                theme::text_muted(),
            ),
            theme::text_muted(),
        );
        let (check, colour) = match shared {
            Some((first, second, channel)) => (
                format!("{first} and {second} share channel {channel}."),
                theme::warning(),
            ),
            None => (
                String::from("No two MIDI tracks share a channel."),
                theme::ok(),
            ),
        };
        painter.galley(
            notes.min + egui::vec2(18.0, 68.0),
            wrap(&check, colour),
            colour,
        );
        painter.text(
            notes.min + egui::vec2(half + 10.0, 14.0),
            egui::Align2::LEFT_TOP,
            "MIDI LEARN",
            theme::semibold(11.0),
            theme::accent(),
        );
        painter.galley(
            notes.min + egui::vec2(half + 10.0, 32.0),
            wrap(
                if self.midi_learn.is_some() {
                    "Waiting: move a knob or press a pad on your controller. Click the cell again to cancel."
                } else {
                    "1. Click a cell.  2. Move a knob or press a pad.  Right-click a cell to clear it."
                },
                theme::text_muted(),
            ),
            theme::text_muted(),
        );
    }
}
