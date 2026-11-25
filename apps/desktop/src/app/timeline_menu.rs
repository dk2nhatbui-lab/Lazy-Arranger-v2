//! Right-click menus on the timeline, as in the C++ arrangement view.
//!
//! The timeline reports where a right-click landed
//! ([`crate::timeline::ContextHit`]); this module shows the menu for it and
//! runs what is chosen.

use egui::Context;
use music_core::{PPQ, Tick};
use project::{ClipId, TrackId};

use super::ShellApp;
use crate::timeline::{ContextHit, ContextTarget};

/// Gains offered for audio, in dB.
const GAINS: [i16; 7] = [-12, -6, -3, 0, 3, 6, 12];

/// Something a timeline menu item does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TimelineAction {
    EditNotes(TrackId),
    CutClip(TrackId, ClipId),
    CopyClip(TrackId, ClipId),
    Paste(TrackId, Tick),
    DuplicateClip(TrackId, ClipId),
    SplitClip(TrackId, ClipId, Tick),
    JoinClip(TrackId, ClipId),
    QuantiseClip(TrackId, ClipId),
    DeleteClip(TrackId, ClipId),
    AddClip(TrackId, Tick),
    SelectAllClips(TrackId),
    AudioGain(TrackId, i16),
    AudioFades(TrackId, Option<Tick>, Option<Tick>),
    AudioFollowTempo(TrackId, bool),
    NormalizeAudio(TrackId),
    AudioToMelody(TrackId),
    AudioToDrums(TrackId),
    AudioChords(TrackId),
    AudioTakes(TrackId),
    SplitAudio(TrackId, Tick),
    DeleteAudio(TrackId),
    TypeChords(Tick),
    Reharmonize(Tick),
    DeleteChord(Tick),
    FindChords,
    LoopBar(Tick),
    DuplicateTrack(TrackId),
    /// Up when true.
    MoveTrack(TrackId, bool),
    DeleteTrack(TrackId),
}

impl ShellApp {
    /// Shows the menu for the timeline's last right-click, if one is open.
    pub(super) fn timeline_menu(&mut self, context: &Context) {
        if let Some(hit) = crate::timeline::take_context_hit(context) {
            self.select_for_menu(hit.target);
            self.timeline_menu_hit = Some(hit);
        }
        let Some(hit) = self.timeline_menu_hit else {
            return;
        };
        let mut chosen = None;
        let area = egui::Area::new(egui::Id::new("timeline-context-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(hit.pos)
            .constrain(true)
            .show(context, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(200.0);
                    // Rows look like menu rows, not buttons.
                    ui.style_mut().visuals.button_frame = false;
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                        chosen = self.timeline_menu_items(ui, hit);
                    });
                });
            });
        let escape = context.input(|input| input.key_pressed(egui::Key::Escape));
        if let Some(action) = chosen {
            self.timeline_menu_hit = None;
            self.run_timeline_action(action);
        } else if escape || area.response.clicked_elsewhere() {
            self.timeline_menu_hit = None;
        }
    }

    /// Selects what was right-clicked, as a left click would.
    fn select_for_menu(&mut self, target: ContextTarget) {
        match target {
            ContextTarget::Clip(track, clip) => {
                self.roll.selected = Some(track);
                self.selected_clip = Some((track, clip));
            }
            ContextTarget::Audio(track) | ContextTarget::Lane(track) => {
                self.roll.selected = Some(track);
            }
            ContextTarget::Chords | ContextTarget::Ruler | ContextTarget::Empty => {}
        }
    }

    /// Draws the rows for `hit`; returns the chosen action.
    fn timeline_menu_items(&self, ui: &mut egui::Ui, hit: ContextHit) -> Option<TimelineAction> {
        let mut chosen = None;
        macro_rules! item {
            ($enabled:expr, $label:expr, $action:expr $(,)?) => {
                if ui
                    .add_enabled($enabled, egui::Button::new($label))
                    .clicked()
                {
                    chosen = Some($action);
                }
            };
        }
        let tick = hit.tick;
        let bar = self.song().time_signature.bar_ticks().max(1);
        match hit.target {
            ContextTarget::Clip(track, clip) => {
                item!(true, "Edit Notes", TimelineAction::EditNotes(track));
                ui.separator();
                item!(true, "Cut", TimelineAction::CutClip(track, clip));
                item!(true, "Copy", TimelineAction::CopyClip(track, clip));
                item!(
                    !self.clip_clipboard.is_empty(),
                    "Paste Here",
                    TimelineAction::Paste(track, tick),
                );
                ui.separator();
                item!(
                    true,
                    "Duplicate",
                    TimelineAction::DuplicateClip(track, clip)
                );
                let inside = self.clip_of(track, clip).is_some_and(|entry| {
                    self.position > entry.start && self.position < entry.end()
                });
                item!(
                    inside,
                    "Split at Playhead",
                    TimelineAction::SplitClip(track, clip, self.position),
                );
                item!(
                    self.next_clip(track, clip).is_some(),
                    "Join with Next",
                    TimelineAction::JoinClip(track, clip),
                );
                item!(true, "Quantise", TimelineAction::QuantiseClip(track, clip));
                ui.separator();
                item!(true, "Delete", TimelineAction::DeleteClip(track, clip));
            }
            ContextTarget::Audio(track) => {
                let audio = self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == track)
                    .and_then(|entry| entry.audio.clone());
                let gain = audio.as_ref().map_or(0, |audio| audio.gain_mb / 100);
                let (fade_in, fade_out) = audio
                    .as_ref()
                    .map_or((0, 0), |audio| (audio.fade_in, audio.fade_out));
                let stretch = audio.as_ref().is_some_and(|audio| audio.stretch);
                ui.menu_button("Gain", |ui| {
                    for db in GAINS {
                        let label = format!("{}{db} dB", if db > 0 { "+" } else { "" });
                        if ui.radio(gain == db, label).clicked() {
                            chosen = Some(TimelineAction::AudioGain(track, db));
                        }
                    }
                });
                for (title, current, fade_in_menu) in
                    [("Fade In", fade_in, true), ("Fade Out", fade_out, false)]
                {
                    ui.menu_button(title, |ui| {
                        for (label, length) in fade_lengths(bar) {
                            if ui.radio(current == length, label).clicked() {
                                chosen = Some(if fade_in_menu {
                                    TimelineAction::AudioFades(track, Some(length), None)
                                } else {
                                    TimelineAction::AudioFades(track, None, Some(length))
                                });
                            }
                        }
                    });
                }
                if ui
                    .checkbox(&mut stretch.clone(), "Follow Song Tempo")
                    .clicked()
                {
                    chosen = Some(TimelineAction::AudioFollowTempo(track, !stretch));
                }
                item!(true, "Normalize", TimelineAction::NormalizeAudio(track));
                ui.separator();
                ui.menu_button("Convert to MIDI", |ui| {
                    if ui.button("Melody (one note at a time)").clicked() {
                        chosen = Some(TimelineAction::AudioToMelody(track));
                    }
                    if ui.button("Drums (kick, snare, hi-hat)").clicked() {
                        chosen = Some(TimelineAction::AudioToDrums(track));
                    }
                });
                item!(
                    true,
                    "Find Chords in the Audio",
                    TimelineAction::AudioChords(track)
                );
                item!(true, "Takes…", TimelineAction::AudioTakes(track));
                ui.separator();
                let inside = audio.as_ref().is_some_and(|audio| {
                    self.position > audio.start && self.position < audio.start + audio.length
                });
                item!(
                    inside,
                    "Split at Playhead",
                    TimelineAction::SplitAudio(track, self.position),
                );
                item!(true, "Delete", TimelineAction::DeleteAudio(track));
            }
            ContextTarget::Lane(track) => {
                item!(
                    !self.clip_clipboard.is_empty(),
                    "Paste Here",
                    TimelineAction::Paste(track, tick),
                );
                item!(true, "Add Clip Here", TimelineAction::AddClip(track, tick));
                ui.separator();
                item!(
                    true,
                    "Select All Clips",
                    TimelineAction::SelectAllClips(track)
                );
            }
            ContextTarget::Chords => {
                let here = self.chord_at(tick);
                let bar_number = tick / bar + 1;
                item!(
                    true,
                    &format!("Type Chords from Bar {bar_number}…"),
                    TimelineAction::TypeChords((tick / bar) * bar),
                );
                let name = here.map(|(_, chord)| chord.format(music_core::Spelling::Sharp));
                item!(
                    here.is_some(),
                    &format!("Reharmonize {}…", name.as_deref().unwrap_or("Chord")),
                    TimelineAction::Reharmonize(here.map_or(0, |(at, _)| at)),
                );
                item!(
                    here.is_some(),
                    &format!("Delete {}", name.as_deref().unwrap_or("Chord")),
                    TimelineAction::DeleteChord(here.map_or(0, |(at, _)| at)),
                );
                ui.separator();
                item!(
                    !self.song().tracks.is_empty(),
                    "Find Chords from the Music",
                    TimelineAction::FindChords,
                );
            }
            ContextTarget::Ruler => {
                item!(
                    true,
                    &format!("Loop Bar {}", tick / bar + 1),
                    TimelineAction::LoopBar(tick),
                );
            }
            ContextTarget::Empty => return None,
        }
        chosen
    }

    /// Runs a timeline menu item.
    pub(super) fn run_timeline_action(&mut self, action: TimelineAction) {
        let bar = self.song().time_signature.bar_ticks().max(1);
        let result: Result<Option<String>, String> = match action {
            TimelineAction::EditNotes(track) => {
                self.roll.selected = Some(track);
                self.timeline_mode = false;
                Ok(None)
            }
            TimelineAction::CopyClip(track, clip) => {
                self.copy_clip(track, clip);
                Ok(Some(String::from("copied the clip")))
            }
            TimelineAction::CutClip(track, clip) => {
                self.copy_clip(track, clip);
                self.delete_clip_with_notes(track, clip)
                    .map(|()| Some(String::from("cut the clip")))
            }
            TimelineAction::Paste(track, at) => self.paste_clips(track, at),
            TimelineAction::DuplicateClip(track, clip) => self.duplicate_clip(track, clip),
            TimelineAction::SplitClip(track, clip, at) => self.split_clip(track, clip, at),
            TimelineAction::JoinClip(track, clip) => match self.next_clip(track, clip) {
                Some(next) => self
                    .editor
                    .join_clips(track, clip, next)
                    .map(|()| Some(String::from("joined the clips")))
                    .map_err(|error| error.to_string()),
                None => Ok(None),
            },
            TimelineAction::QuantiseClip(track, clip) => self.quantise_clip(track, clip),
            TimelineAction::DeleteClip(track, clip) => {
                self.selected_clip = None;
                self.delete_clip_with_notes(track, clip)
                    .map(|()| Some(String::from("deleted the clip")))
            }
            TimelineAction::AddClip(track, at) => self
                .editor
                .add_clip(track, (at / bar) * bar, bar)
                .map(|()| Some(String::from("added a clip"))),
            TimelineAction::SelectAllClips(track) => {
                self.selected_clips = self
                    .song()
                    .tracks
                    .iter()
                    .filter(|entry| entry.id == track)
                    .flat_map(|entry| entry.clips.iter().map(move |clip| (entry.id, clip.id)))
                    .collect();
                Ok(Some(format!(
                    "selected {} clip(s)",
                    self.selected_clips.len()
                )))
            }
            TimelineAction::AudioGain(track, db) => self
                .editor
                .set_audio_gain(track, db * 100)
                .map(|()| Some(format!("audio gain {db} dB"))),
            TimelineAction::AudioFades(track, fade_in, fade_out) => {
                let (now_in, now_out) = self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == track)
                    .and_then(|entry| entry.audio.as_ref())
                    .map_or((0, 0), |audio| (audio.fade_in, audio.fade_out));
                self.editor
                    .set_audio_fades(
                        track,
                        fade_in.unwrap_or(now_in),
                        fade_out.unwrap_or(now_out),
                    )
                    .map(|()| None)
            }
            TimelineAction::AudioFollowTempo(track, on) => {
                self.editor.set_audio_stretch(track, on).map(|()| None)
            }
            TimelineAction::NormalizeAudio(track) => self
                .editor
                .normalize_audio(track, 0.9)
                .map(|_| Some(String::from("normalized the audio"))),
            TimelineAction::AudioToMelody(track) => self
                .transcribe_audio_track(track)
                .map(|()| Some(String::from("converted the audio to a melody track"))),
            TimelineAction::AudioToDrums(track) => self
                .transcribe_audio_drums(track)
                .map(|()| Some(String::from("converted the audio to a drum track"))),
            TimelineAction::AudioChords(track) => self
                .transcribe_audio_chords(track)
                .map(|count| Some(format!("found {count} chord(s) in the audio"))),
            TimelineAction::AudioTakes(track) => {
                self.roll.selected = Some(track);
                self.takes_open = true;
                Ok(None)
            }
            TimelineAction::SplitAudio(track, at) => self
                .editor
                .split_audio(track, at)
                .map(|()| Some(String::from("split the audio"))),
            TimelineAction::DeleteAudio(track) => self
                .editor
                .remove_audio(track)
                .map(|()| Some(String::from("removed the audio"))),
            TimelineAction::TypeChords(at) => {
                self.chord_dialog_target = at;
                self.chord_dialog = true;
                Ok(None)
            }
            TimelineAction::Reharmonize(at) => {
                self.chord_dialog_target = at;
                self.reharmonize_dialog = true;
                Ok(None)
            }
            TimelineAction::DeleteChord(at) => {
                let end = self
                    .song()
                    .chords
                    .iter()
                    .map(|event| event.tick)
                    .find(|other| *other > at)
                    .unwrap_or(at + bar);
                self.selected_chord = None;
                self.editor
                    .remove_chords(at, end)
                    .map(|count| Some(format!("deleted {count} chord(s)")))
            }
            TimelineAction::FindChords => self
                .editor
                .find_chords()
                .map(|count| Some(format!("found {count} chord(s)"))),
            TimelineAction::LoopBar(at) => {
                self.loop_region = Some(crate::transport::bar_region(at, bar));
                self.loop_enabled = true;
                self.sync_player_loop();
                Ok(None)
            }
            TimelineAction::DuplicateTrack(track) => {
                self.editor.duplicate_track(track).map(|copy| {
                    self.roll.selected = Some(copy);
                    Some(String::from("duplicated the track"))
                })
            }
            TimelineAction::MoveTrack(track, up) => {
                let index = self
                    .song()
                    .tracks
                    .iter()
                    .position(|entry| entry.id == track);
                match index {
                    Some(index) if up && index > 0 => {
                        self.editor.move_track(track, index - 1).map(|()| None)
                    }
                    Some(index) if !up => self.editor.move_track(track, index + 1).map(|()| None),
                    _ => Ok(None),
                }
            }
            TimelineAction::DeleteTrack(track) => {
                if self.roll.selected == Some(track) {
                    self.roll.selected = None;
                }
                self.selected_clip = None;
                self.selected_clips.clear();
                self.selected_note = None;
                self.selected_notes.clear();
                self.editor
                    .remove_track(track)
                    .map(|()| Some(String::from("deleted the track (Undo brings it back)")))
            }
        };
        match result {
            Ok(Some(message)) => self.status = Some(message),
            Ok(None) => {}
            Err(error) => self.status = Some(error),
        }
    }

    fn clip_of(&self, track: TrackId, clip: ClipId) -> Option<&project::song::Clip> {
        self.song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.clips.iter().find(|entry| entry.id == clip))
    }

    /// The clip after `clip` on its track, if any.
    fn next_clip(&self, track: TrackId, clip: ClipId) -> Option<ClipId> {
        let start = self.clip_of(track, clip)?.start;
        self.song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)?
            .clips
            .iter()
            .filter(|entry| entry.start > start)
            .min_by_key(|entry| entry.start)
            .map(|entry| entry.id)
    }

    /// The chord sounding at `tick`.
    fn chord_at(&self, tick: Tick) -> Option<(Tick, music_core::Chord)> {
        self.song()
            .chords
            .iter()
            .filter(|event| event.tick <= tick)
            .max_by_key(|event| event.tick)
            .map(|event| (event.tick, event.chord))
    }

    /// The notes that start inside a clip.
    fn notes_in_clip(&self, track: TrackId, clip: ClipId) -> Vec<project::NoteId> {
        let Some(range) = self
            .clip_of(track, clip)
            .map(|entry| (entry.start, entry.end()))
        else {
            return Vec::new();
        };
        self.song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| {
                entry
                    .notes
                    .iter()
                    .filter(|note| note.start >= range.0 && note.start < range.1)
                    .map(|note| note.id)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Copies a clip and its notes.
    fn copy_clip(&mut self, track: TrackId, clip: ClipId) {
        if let Some(entry) = self.clip_of(track, clip).cloned() {
            let notes = self.notes_in_clip(track, clip);
            self.editor.copy_notes(track, &notes);
            self.clip_note_offset = self
                .song()
                .tracks
                .iter()
                .find(|other| other.id == track)
                .and_then(|other| {
                    other
                        .notes
                        .iter()
                        .filter(|note| notes.contains(&note.id))
                        .map(|note| note.start)
                        .min()
                })
                .map_or(0, |first| first - entry.start);
            self.clip_clipboard = vec![(track, entry)];
        }
    }

    /// Pastes the copied clip (and its notes) at `at` on `track`.
    fn paste_clips(&mut self, track: TrackId, at: Tick) -> Result<Option<String>, String> {
        let Some((_, clip)) = self.clip_clipboard.first().cloned() else {
            return Ok(None);
        };
        let bar = self.song().time_signature.bar_ticks().max(1);
        let start = (at / bar) * bar;
        self.editor.add_clip(track, start, clip.length)?;
        let notes = self
            .editor
            .paste_notes(track, start + self.clip_note_offset)
            .unwrap_or(0);
        Ok(Some(format!("pasted a clip with {notes} note(s)")))
    }

    /// Copies a clip and its notes right after itself.
    fn duplicate_clip(&mut self, track: TrackId, clip: ClipId) -> Result<Option<String>, String> {
        let Some(entry) = self.clip_of(track, clip).cloned() else {
            return Ok(None);
        };
        let saved = (self.clip_clipboard.clone(), self.clip_note_offset);
        self.copy_clip(track, clip);
        let at = entry.end();
        let result = self.editor.add_clip(track, at, entry.length).map(|()| {
            let notes = self
                .editor
                .paste_notes(track, at + self.clip_note_offset)
                .unwrap_or(0);
            Some(format!("duplicated the clip with {notes} note(s)"))
        });
        (self.clip_clipboard, self.clip_note_offset) = saved;
        result
    }

    /// Cuts a clip in two at `at`; the notes stay where they are.
    fn split_clip(
        &mut self,
        track: TrackId,
        clip: ClipId,
        at: Tick,
    ) -> Result<Option<String>, String> {
        let Some(entry) = self.clip_of(track, clip).cloned() else {
            return Ok(None);
        };
        if at <= entry.start || at >= entry.end() {
            return Ok(None);
        }
        self.editor
            .resize_clip(track, clip, at - entry.start)
            .map_err(|error| error.to_string())?;
        self.editor.add_clip(track, at, entry.end() - at)?;
        Ok(Some(String::from("split the clip at the playhead")))
    }

    /// Snaps the starts of the notes in a clip to sixteenths.
    fn quantise_clip(&mut self, track: TrackId, clip: ClipId) -> Result<Option<String>, String> {
        let ids = self.notes_in_clip(track, clip);
        self.quantise_notes(track, &ids)
    }

    /// Snaps the starts of some of a track's notes to a sixteenth.
    pub(super) fn quantise_notes(
        &mut self,
        track: TrackId,
        ids: &[project::NoteId],
    ) -> Result<Option<String>, String> {
        let grid = PPQ / 4;
        let Some(entry) = self.song().tracks.iter().find(|entry| entry.id == track) else {
            return Ok(None);
        };
        let mut notes = entry.notes.clone();
        let mut changed = 0;
        for note in notes.iter_mut().filter(|note| ids.contains(&note.id)) {
            let snapped = (note.start + grid / 2) / grid * grid;
            if snapped != note.start {
                note.start = snapped;
                changed += 1;
            }
        }
        if changed == 0 {
            return Ok(Some(String::from("the notes are already on the grid")));
        }
        notes.sort_by_key(|note| note.start);
        self.editor.replace_track_notes(track, notes)?;
        Ok(Some(format!("quantised {changed} note(s)")))
    }

    /// Deletes a clip and the notes inside it.
    fn delete_clip_with_notes(&mut self, track: TrackId, clip: ClipId) -> Result<(), String> {
        let notes = self.notes_in_clip(track, clip);
        if !notes.is_empty() {
            self.editor.delete_notes(track, &notes)?;
        }
        self.editor
            .remove_clip(track, clip)
            .map_err(|error| error.to_string())
    }
}

/// The fade lengths offered for audio.
fn fade_lengths(bar: Tick) -> [(&'static str, Tick); 5] {
    [
        ("None", 0),
        ("1/16 note", PPQ / 4),
        ("1/8 note", PPQ / 2),
        ("1 beat", PPQ),
        ("1 bar", bar),
    ]
}
