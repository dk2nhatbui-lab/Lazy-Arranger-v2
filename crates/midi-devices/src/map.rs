//! A MIDI control map: notes and controllers bound to app actions.
//!
//! The map persists as plain text (`note 36 = play`), and a learn session
//! captures the next note-on or control change for a chosen action.

use music_core::MidiMessage;
use project::TrackId;
use std::fmt::Write as _;

/// A control that can trigger an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// A note number.
    Note(u8),
    /// A controller number.
    Controller(u8),
}

impl Binding {
    /// The control as shown in help and the command line.
    #[must_use]
    pub fn control_label(self) -> String {
        self.control.label()
    }
}

impl Control {
    fn label(self) -> String {
        match self {
            Self::Note(key) => format!("note {key}"),
            Self::Controller(number) => format!("cc {number}"),
        }
    }
}

/// An action a control can trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Start playback.
    Play,
    /// Stop playback.
    Stop,
    /// Undo one edit.
    Undo,
    /// Launch a clip-grid scene.
    Scene(usize),
    /// Set the fader of a track, using the incoming value.
    TrackFader(TrackId),
    /// Audition a drum pad by MIDI note.
    Pad(u8),
}

impl Action {
    fn label(self) -> String {
        match self {
            Self::Play => String::from("play"),
            Self::Stop => String::from("stop"),
            Self::Undo => String::from("undo"),
            Self::Scene(index) => format!("scene {index}"),
            Self::TrackFader(track) => format!("fader {track}"),
            Self::Pad(note) => format!("pad {note}"),
        }
    }
}

/// One binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    /// What is pressed or turned.
    pub control: Control,
    /// What it triggers.
    pub action: Action,
}

/// The control map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MidiMap {
    /// Bindings, one per control.
    pub bindings: Vec<Binding>,
}

impl MidiMap {
    /// Binds a control, replacing any earlier binding for it.
    pub fn bind(&mut self, control: Control, action: Action) {
        self.bindings.retain(|binding| binding.control != control);
        self.bindings.push(Binding { control, action });
    }

    /// Removes a binding.
    pub fn unbind(&mut self, control: Control) {
        self.bindings.retain(|binding| binding.control != control);
    }

    /// The action for an incoming message, when bound.
    #[must_use]
    pub fn action_for(&self, message: &MidiMessage) -> Option<Action> {
        let control = control_of(message)?;
        self.bindings
            .iter()
            .find(|binding| binding.control == control)
            .map(|binding| binding.action)
    }

    /// Serialises the map, one binding per line.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::from("# Lazy Arranger MIDI map\n");
        let mut bindings = self.bindings.clone();
        bindings.sort_by_key(|binding| binding.control.label());
        for binding in bindings {
            let _ = writeln!(
                text,
                "{} = {}",
                binding.control.label(),
                binding.action.label()
            );
        }
        text
    }

    /// Binds from text, for the command line.
    ///
    /// # Errors
    ///
    /// Returns a message for an unreadable control or action.
    pub fn bind_text(&mut self, control: &str, action: &str) -> Result<(), String> {
        let control = parse_control(control)?;
        let action = parse_action(action)?;
        self.bind(control, action);
        Ok(())
    }

    /// Parses a map; a malformed line is an error naming the line.
    ///
    /// # Errors
    ///
    /// Returns a message for an unreadable control or action.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut map = Self::default();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (control, action) = line
                .split_once('=')
                .ok_or_else(|| format!("line {}: expected 'control = action'", number + 1))?;
            let control = parse_control(control.trim())
                .map_err(|error| format!("line {}: {error}", number + 1))?;
            let action = parse_action(action.trim())
                .map_err(|error| format!("line {}: {error}", number + 1))?;
            map.bind(control, action);
        }
        Ok(map)
    }
}

/// The 7-bit value of a control change, when the message is one.
#[must_use]
pub fn value_of(message: &MidiMessage) -> Option<u8> {
    match message {
        MidiMessage::ControlChange { value, .. } => Some(*value),
        _ => None,
    }
}

/// A learn session for one action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Learn {
    /// The action waiting for a control.
    pub action: Action,
}

impl Learn {
    /// Starts learning.
    #[must_use]
    pub fn begin(action: Action) -> Self {
        Self { action }
    }

    /// Captures the first note-on or control change.
    pub fn capture(&self, message: &MidiMessage) -> Option<(Control, Action)> {
        control_of(message).map(|control| (control, self.action))
    }
}

fn control_of(message: &MidiMessage) -> Option<Control> {
    match message {
        MidiMessage::NoteOn { key, velocity, .. } if velocity.as_u8() > 0 => {
            Some(Control::Note(key.as_u8()))
        }
        MidiMessage::ControlChange {
            controller, value, ..
        } if *value > 0 => Some(Control::Controller(controller.as_u8())),
        _ => None,
    }
}

fn parse_control(text: &str) -> Result<Control, String> {
    let mut parts = text.split_whitespace();
    let kind = parts.next().ok_or("missing control")?;
    let number: u8 = parts
        .next()
        .ok_or("missing control number")?
        .parse()
        .map_err(|_| String::from("control number must be 0..=127"))?;
    if number > 127 {
        return Err(String::from("control number must be 0..=127"));
    }
    match kind {
        "note" => Ok(Control::Note(number)),
        "cc" => Ok(Control::Controller(number)),
        other => Err(format!("unknown control type {other:?}")),
    }
}

fn parse_action(text: &str) -> Result<Action, String> {
    let mut parts = text.split_whitespace();
    let kind = parts.next().ok_or("missing action")?;
    match kind {
        "play" => Ok(Action::Play),
        "stop" => Ok(Action::Stop),
        "undo" => Ok(Action::Undo),
        "scene" => {
            let index: usize = parts
                .next()
                .ok_or("scene needs an index")?
                .parse()
                .map_err(|_| String::from("scene index must be a number"))?;
            Ok(Action::Scene(index))
        }
        "fader" => {
            let raw = parts.next().ok_or("fader needs a track id")?;
            let digits = raw.strip_prefix("track-").unwrap_or(raw);
            let track: u32 = digits
                .parse()
                .map_err(|_| String::from("fader needs a track id"))?;
            let track = TrackId::new(track).map_err(|error| error.to_string())?;
            Ok(Action::TrackFader(track))
        }
        "pad" => {
            let note: u8 = parts
                .next()
                .ok_or("pad needs a note")?
                .parse()
                .map_err(|_| String::from("pad note must be 0..=127"))?;
            if note > 127 {
                return Err(String::from("pad note must be 0..=127"));
            }
            Ok(Action::Pad(note))
        }
        other => Err(format!("unknown action {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, Pitch, Velocity};

    fn note_on(key: u8) -> MidiMessage {
        MidiMessage::note_on(
            Channel::first(),
            Pitch::new(key).expect("valid"),
            Velocity::new(100).expect("valid"),
        )
    }

    fn control(number: u8, value: u8) -> MidiMessage {
        MidiMessage::ControlChange {
            channel: Channel::first(),
            controller: music_core::ControlNumber::new(number).expect("valid"),
            value,
        }
    }

    #[test]
    fn bindings_replace_per_control() {
        let mut map = MidiMap::default();
        map.bind(Control::Note(36), Action::Play);
        map.bind(Control::Note(36), Action::Stop);
        assert_eq!(map.bindings.len(), 1);
        assert_eq!(map.action_for(&note_on(36)), Some(Action::Stop));
        assert_eq!(map.action_for(&note_on(37)), None);
        map.unbind(Control::Note(36));
        assert!(map.bindings.is_empty());
    }

    #[test]
    fn a_map_round_trips_through_text() {
        let mut map = MidiMap::default();
        map.bind(Control::Note(36), Action::Play);
        map.bind(
            Control::Controller(7),
            Action::TrackFader(TrackId::new(2).expect("valid")),
        );
        map.bind(Control::Note(40), Action::Scene(3));
        let text = map.to_text();
        let back = MidiMap::from_text(&text).expect("parse");
        assert_eq!(back.bindings.len(), 3);
        assert_eq!(
            back.action_for(&control(7, 90)),
            Some(Action::TrackFader(TrackId::new(2).expect("valid")))
        );
        assert!(MidiMap::from_text("note 200 = play").is_err());
        assert!(MidiMap::from_text("pad 1 = play").is_err());
        assert!(MidiMap::from_text("note 1 = explode").is_err());
        assert!(MidiMap::from_text("note 1 play").is_err());
    }

    #[test]
    fn control_changes_expose_their_value() {
        assert_eq!(value_of(&control(7, 90)), Some(90));
        assert_eq!(value_of(&note_on(60)), None);
    }

    #[test]
    fn learning_captures_the_first_press() {
        let learn = Learn::begin(Action::Play);
        assert_eq!(
            learn.capture(&note_on(60)),
            Some((Control::Note(60), Action::Play))
        );
        assert_eq!(
            learn.capture(&control(64, 127)),
            Some((Control::Controller(64), Action::Play))
        );
        let release = MidiMessage::note_off(
            Channel::first(),
            Pitch::new(60).expect("valid"),
            Velocity::new(0).expect("valid"),
        );
        assert_eq!(learn.capture(&release), None);
        assert_eq!(learn.capture(&control(64, 0)), None);
    }
}
