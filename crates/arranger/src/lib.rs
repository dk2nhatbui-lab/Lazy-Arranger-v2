//! Chord-rule transposition for arranger styles.
//!
//! Stage 3 of the blueprint owns the arranger engine. This crate starts with
//! the part that the audit specifies completely: the Yamaha chord vocabulary,
//! per-channel rules, the eight-step chord-to-note conversion and the part
//! gate. Section playback builds on these functions.
//!
//! The behaviour is transcribed from the C++ oracle
//! (`src/core/src/StyleChord.cpp`), and the specification, including its
//! ambiguities, is in `docs/audit/notes/cpp-style-arranger.md` §3. Golden
//! parity against a running C++ build is still blocked; the tests here pin the
//! cases the C++ test suite already pins.

pub mod chord_type;
pub mod rules;
pub mod style;
pub mod transition;
pub mod transpose;

pub use chord_type::{ArrangerChord, ChordType};
pub use rules::{ChordRule, Ntr, Ntt, Rtr, TransposeRange};
pub use style::{ChordAt, CompiledNote, Style, StyleError, StyleNote, StylePart, StyleSection};
pub use transition::{SectionRequest, TransitionMode, TransitionState};
pub use transpose::{mod12, plays, transpose};

use thiserror::Error;

/// Errors raised by the arranger.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArrangerError {
    /// A chord-type number was outside the 35 known types.
    #[error("chord type {0} is outside 0..34")]
    ChordType(usize),
}
