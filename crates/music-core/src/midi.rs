use serde::{Deserialize, Serialize};

use crate::MusicError;
use crate::pitch::Pitch;

/// A MIDI channel, stored zero-based (`0..=15`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Channel(u8);

impl Channel {
    /// Creates a channel from a zero-based number.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::Midi`] when `zero_based` is above 15.
    pub fn new(zero_based: u8) -> Result<Self, MusicError> {
        if zero_based < 16 {
            Ok(Self(zero_based))
        } else {
            Err(MusicError::Midi(format!(
                "channel {zero_based} is outside 0..=15"
            )))
        }
    }

    /// Creates a channel from a one-based number, as printed on hardware.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::Midi`] when `one_based` is outside `1..=16`.
    pub fn from_one_based(one_based: u8) -> Result<Self, MusicError> {
        one_based
            .checked_sub(1)
            .ok_or_else(|| {
                MusicError::Midi(String::from(
                    "channel 0 does not exist; channels are 1..=16",
                ))
            })
            .and_then(Self::new)
    }

    /// The zero-based channel number.
    #[must_use]
    pub const fn zero_based(self) -> u8 {
        self.0
    }

    /// The one-based channel number used in the UI and on hardware.
    #[must_use]
    pub const fn one_based(self) -> u8 {
        self.0 + 1
    }

    /// Channel 1, the default for a new track.
    #[must_use]
    pub const fn first() -> Self {
        Self(0)
    }

    /// The drum channel, zero-based 9 (MIDI channel 10).
    #[must_use]
    pub const fn drums() -> Self {
        Self(9)
    }
}

impl Default for Channel {
    fn default() -> Self {
        Self::first()
    }
}

impl TryFrom<u8> for Channel {
    type Error = MusicError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Channel> for u8 {
    fn from(value: Channel) -> Self {
        value.0
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.one_based())
    }
}

/// Note velocity, `0..=127`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Velocity(u8);

impl Velocity {
    /// Creates a velocity.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::Midi`] when `value` is above 127.
    pub fn new(value: u8) -> Result<Self, MusicError> {
        if value <= 127 {
            Ok(Self(value))
        } else {
            Err(MusicError::Midi(format!(
                "velocity {value} is outside 0..=127"
            )))
        }
    }

    /// The raw velocity byte.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Velocity {
    type Error = MusicError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Velocity> for u8 {
    fn from(value: Velocity) -> Self {
        value.0
    }
}

/// A MIDI controller number, `0..=127`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct ControlNumber(u8);

impl ControlNumber {
    /// Creates a controller number.
    ///
    /// # Errors
    ///
    /// Returns [`MusicError::Midi`] when `value` is above 127.
    pub fn new(value: u8) -> Result<Self, MusicError> {
        if value <= 127 {
            Ok(Self(value))
        } else {
            Err(MusicError::Midi(format!(
                "controller {value} is outside 0..=127"
            )))
        }
    }

    /// The raw controller number.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// Channel volume, controller 7.
    #[must_use]
    pub const fn channel_volume() -> Self {
        Self(7)
    }

    /// Pan, controller 10.
    #[must_use]
    pub const fn pan() -> Self {
        Self(10)
    }

    /// All notes off, controller 123.
    #[must_use]
    pub const fn all_notes_off() -> Self {
        Self(123)
    }

    /// All sound off, controller 120.
    #[must_use]
    pub const fn all_sound_off() -> Self {
        Self(120)
    }
}

impl TryFrom<u8> for ControlNumber {
    type Error = MusicError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ControlNumber> for u8 {
    fn from(value: ControlNumber) -> Self {
        value.0
    }
}

/// A channel MIDI message, without SysEx.
///
/// SysEx is deliberately absent: it is not sample-accurate, it needs an
/// allocation-sized buffer, and device profiles own it. See ADR-009.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MidiMessage {
    /// Note on.
    NoteOn {
        /// Target channel.
        channel: Channel,
        /// Note number.
        key: Pitch,
        /// On velocity; 0 is normalised to a note-off by
        /// [`MidiMessage::normalized`].
        velocity: Velocity,
    },
    /// Note off.
    NoteOff {
        /// Target channel.
        channel: Channel,
        /// Note number.
        key: Pitch,
        /// Release velocity.
        velocity: Velocity,
    },
    /// Control change.
    ControlChange {
        /// Target channel.
        channel: Channel,
        /// Controller number.
        controller: ControlNumber,
        /// Controller value.
        value: u8,
    },
    /// Program change.
    ProgramChange {
        /// Target channel.
        channel: Channel,
        /// Program number.
        program: u8,
    },
    /// Pitch bend, `-8192..=8191` with 0 as centre.
    PitchBend {
        /// Target channel.
        channel: Channel,
        /// Bend value.
        value: i16,
    },
}

impl MidiMessage {
    /// Creates a note-on.
    #[must_use]
    pub const fn note_on(channel: Channel, key: Pitch, velocity: Velocity) -> Self {
        Self::NoteOn {
            channel,
            key,
            velocity,
        }
    }

    /// Creates a note-off.
    #[must_use]
    pub const fn note_off(channel: Channel, key: Pitch, velocity: Velocity) -> Self {
        Self::NoteOff {
            channel,
            key,
            velocity,
        }
    }

    /// Returns the message channel.
    #[must_use]
    pub const fn channel(self) -> Channel {
        match self {
            Self::NoteOn { channel, .. }
            | Self::NoteOff { channel, .. }
            | Self::ControlChange { channel, .. }
            | Self::ProgramChange { channel, .. }
            | Self::PitchBend { channel, .. } => channel,
        }
    }

    /// Applies the MIDI rule that a note-on with velocity 0 is a note-off.
    #[must_use]
    pub const fn normalized(self) -> Self {
        match self {
            Self::NoteOn {
                channel,
                key,
                velocity,
            } if velocity.as_u8() == 0 => Self::NoteOff {
                channel,
                key,
                velocity,
            },
            other => other,
        }
    }

    /// Returns true when this message starts a note after normalisation.
    #[must_use]
    pub const fn is_note_on(self) -> bool {
        matches!(self, Self::NoteOn { .. })
    }

    /// Sorting rank for simultaneous messages.
    ///
    /// Note-offs sort before controllers and note-ons so that a repeated note
    /// is released before it is struck again. Ordering policy lives here, in
    /// one place, so every consumer agrees.
    #[must_use]
    pub const fn sort_rank(self) -> u8 {
        match self {
            Self::NoteOff { .. } => 0,
            Self::ControlChange { .. } | Self::ProgramChange { .. } | Self::PitchBend { .. } => 1,
            Self::NoteOn { .. } => 2,
        }
    }

    /// Number of bytes this message encodes to.
    #[must_use]
    pub const fn encoded_len(self) -> usize {
        match self {
            Self::ProgramChange { .. } => 2,
            _ => 3,
        }
    }

    /// Encodes the message as MIDI wire bytes.
    ///
    /// The returned array holds [`MidiMessage::encoded_len`] meaningful bytes.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 3] {
        match self {
            Self::NoteOff {
                channel,
                key,
                velocity,
            } => [0x80 | channel.zero_based(), key.as_u8(), velocity.as_u8()],
            Self::NoteOn {
                channel,
                key,
                velocity,
            } => [0x90 | channel.zero_based(), key.as_u8(), velocity.as_u8()],
            Self::ControlChange {
                channel,
                controller,
                value,
            } => [
                0xB0 | channel.zero_based(),
                controller.as_u8(),
                value & 0x7F,
            ],
            Self::ProgramChange { channel, program } => {
                [0xC0 | channel.zero_based(), program & 0x7F, 0]
            }
            Self::PitchBend { channel, value } => {
                let raw = i32::from(value.clamp(-8192, 8191)) + 8192;
                let raw = u16::try_from(raw).unwrap_or(8192);
                [
                    0xE0 | channel.zero_based(),
                    (raw & 0x7F) as u8,
                    ((raw >> 7) & 0x7F) as u8,
                ]
            }
        }
    }

    /// Decodes the channel messages a sequencer needs to record.
    ///
    /// Returns `None` for system messages and for status bytes without data.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let status = *bytes.first()?;
        let channel = Channel::new(status & 0x0F).ok()?;
        let data = bytes.get(1..)?;
        match status & 0xF0 {
            0x80 => Some(Self::note_off(
                channel,
                Pitch::new(*data.first()?).ok()?,
                Velocity::new(*data.get(1)?).ok()?,
            )),
            0x90 => Some(Self::note_on(
                channel,
                Pitch::new(*data.first()?).ok()?,
                Velocity::new(*data.get(1)?).ok()?,
            )),
            0xB0 => Some(Self::ControlChange {
                channel,
                controller: ControlNumber::new(*data.first()?).ok()?,
                value: *data.get(1)?,
            }),
            0xC0 => Some(Self::ProgramChange {
                channel,
                program: *data.first()?,
            }),
            0xE0 => {
                let lsb = u16::from(*data.first()?);
                let msb = u16::from(*data.get(1)?);
                let raw = i32::from((msb << 7) | lsb) - 8192;
                Some(Self::PitchBend {
                    channel,
                    value: i16::try_from(raw).ok()?,
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pitch(name: &str) -> Pitch {
        Pitch::from_name(name).expect("valid pitch")
    }

    fn velocity(value: u8) -> Velocity {
        Velocity::new(value).expect("valid velocity")
    }

    #[test]
    fn channel_and_velocity_validate() {
        assert_eq!(Channel::new(15).expect("valid").one_based(), 16);
        assert!(Channel::new(16).is_err());
        assert_eq!(Channel::from_one_based(10).expect("valid").zero_based(), 9);
        assert!(Channel::from_one_based(0).is_err());
        assert!(Velocity::new(128).is_err());
        assert!(ControlNumber::new(128).is_err());
        assert_eq!(Channel::drums().zero_based(), 9);
    }

    #[test]
    fn note_messages_encode_to_wire_bytes() {
        let ch = Channel::first();
        let on = MidiMessage::note_on(ch, pitch("C4"), velocity(100));
        assert_eq!(on.to_bytes(), [0x90, 60, 100]);
        let off = MidiMessage::note_off(ch, pitch("C4"), velocity(64));
        assert_eq!(off.to_bytes(), [0x80, 60, 64]);
        assert_eq!(
            MidiMessage::note_on(Channel::new(9).expect("valid"), pitch("C4"), velocity(1))
                .to_bytes(),
            [0x99, 60, 1]
        );
    }

    #[test]
    fn controllers_program_and_bend_encode() {
        let ch = Channel::new(3).expect("valid");
        assert_eq!(
            MidiMessage::ControlChange {
                channel: ch,
                controller: ControlNumber::channel_volume(),
                value: 127
            }
            .to_bytes(),
            [0xB3, 7, 127]
        );
        assert_eq!(
            MidiMessage::ProgramChange {
                channel: ch,
                program: 40
            }
            .to_bytes(),
            [0xC3, 40, 0]
        );
        assert_eq!(
            MidiMessage::ProgramChange {
                channel: ch,
                program: 40
            }
            .encoded_len(),
            2
        );
        assert_eq!(
            MidiMessage::PitchBend {
                channel: ch,
                value: 0
            }
            .to_bytes(),
            [0xE3, 0x00, 0x40]
        );
        assert_eq!(
            MidiMessage::PitchBend {
                channel: ch,
                value: 8191
            }
            .to_bytes(),
            [0xE3, 0x7F, 0x7F]
        );
        assert_eq!(
            MidiMessage::PitchBend {
                channel: ch,
                value: -8192
            }
            .to_bytes(),
            [0xE3, 0x00, 0x00]
        );
    }

    #[test]
    fn decoding_inverts_encoding() {
        let ch = Channel::new(5).expect("valid");
        let messages = [
            MidiMessage::note_on(ch, pitch("D#4"), velocity(90)),
            MidiMessage::note_off(ch, pitch("D#4"), velocity(0)),
            MidiMessage::ControlChange {
                channel: ch,
                controller: ControlNumber::pan(),
                value: 12,
            },
            MidiMessage::ProgramChange {
                channel: ch,
                program: 3,
            },
            MidiMessage::PitchBend {
                channel: ch,
                value: -1234,
            },
        ];
        for message in messages {
            let bytes = message.to_bytes();
            let decoded = MidiMessage::from_bytes(&bytes[..message.encoded_len()])
                .expect("channel message should decode");
            assert_eq!(decoded, message);
        }
    }

    #[test]
    fn decoding_rejects_system_messages_and_truncation() {
        assert!(MidiMessage::from_bytes(&[0xF8]).is_none());
        assert!(MidiMessage::from_bytes(&[0x90]).is_none());
        assert!(MidiMessage::from_bytes(&[0x90, 60]).is_none());
        assert!(MidiMessage::from_bytes(&[]).is_none());
    }

    #[test]
    fn note_on_with_zero_velocity_normalises_to_note_off() {
        let ch = Channel::first();
        let message = MidiMessage::note_on(ch, pitch("A4"), velocity(0)).normalized();
        assert!(matches!(message, MidiMessage::NoteOff { .. }));
        assert!(!message.is_note_on());
        assert_eq!(
            MidiMessage::note_on(ch, pitch("A4"), velocity(1)).normalized(),
            MidiMessage::note_on(ch, pitch("A4"), velocity(1))
        );
    }

    #[test]
    fn ordering_puts_note_offs_first() {
        let ch = Channel::first();
        let p = pitch("C4");
        let off = MidiMessage::note_off(ch, p, velocity(0));
        let cc = MidiMessage::ControlChange {
            channel: ch,
            controller: ControlNumber::channel_volume(),
            value: 100,
        };
        let on = MidiMessage::note_on(ch, p, velocity(100));
        assert!(off.sort_rank() < cc.sort_rank());
        assert!(cc.sort_rank() < on.sort_rank());
    }
}
