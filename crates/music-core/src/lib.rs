//! Platform-independent musical primitives.
//!
//! This crate holds no audio, MIDI-port or UI types. It is the shared
//! vocabulary for the project model, the arranger and the sequencer.
//!
//! Conventions (see `docs/architecture/contracts.md`):
//!
//! * `Tick` is the edit timeline unit; one quarter note is [`PPQ`] ticks.
//! * `SamplePosition` is the playback timeline unit at an explicit sample rate.
//! * Pitch classes are integers `0..12` with `0` = C, independent of spelling.
//! * Display spellings are chosen at format time, never stored in a pitch.

pub mod chord;
pub mod midi;
pub mod pitch;
pub mod progression;
pub mod reharmonize;
pub mod roman;
pub mod tempo;
pub mod theory;
pub mod time;

pub use chord::{Chord, ChordQuality, Spelling};
pub use midi::{Channel, ControlNumber, MidiMessage, Velocity};
pub use pitch::{Pitch, PitchClass};
pub use tempo::{TempoChange, TempoMap};
pub use time::{PPQ, SamplePosition, Tick, TimeSignature};

use thiserror::Error;

/// Errors raised by musical primitives when an input is outside the domain.
#[derive(Debug, Error, PartialEq)]
pub enum MusicError {
    /// A pitch class, note name or octave could not be interpreted.
    #[error("invalid pitch: {0}")]
    Pitch(String),
    /// A chord symbol could not be interpreted.
    #[error("invalid chord symbol: {0}")]
    Chord(String),
    /// A tempo value was zero, negative or not finite.
    #[error("invalid tempo {bpm}: must be finite and greater than zero")]
    Tempo {
        /// The rejected tempo in beats per minute.
        bpm: f64,
    },
    /// A tick position was negative.
    #[error("invalid tick {tick}: must be zero or greater")]
    Tick {
        /// The rejected tick position.
        tick: i64,
    },
    /// A tempo map broke one of its invariants.
    #[error("invalid tempo map: {0}")]
    TempoMap(String),
    /// A MIDI value was outside its legal range.
    #[error("invalid MIDI value: {0}")]
    Midi(String),
    /// A time signature had a zero numerator or a non power-of-two denominator.
    #[error("invalid time signature {numerator}/{denominator}")]
    TimeSignature {
        /// The numerator.
        numerator: u8,
        /// The denominator.
        denominator: u8,
    },
}
