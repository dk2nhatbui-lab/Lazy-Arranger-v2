//! Versioned song and project schema.
//!
//! The file format is JSON so that a project can be diffed, merged and
//! inspected without the application. The top level is
//!
//! ```json
//! { "format": "lazy-arranger-project", "version": 1, "song": { ... } }
//! ```
//!
//! Unknown keys are preserved at song, track and project level. A future
//! writer therefore never silently deletes data it does not understand, which
//! is the requirement for importing legacy C++ projects (ADR-004).
//!
//! Schema version 1 covers the headless MVP: tracks of notes, chords, arranger
//! intent, markers and a tempo map. Clips, audio lanes, automation and
//! provenance are later schema versions with explicit migrations.

pub mod ids;
pub mod legacy;
pub mod song;
pub mod validate;

pub use ids::{Marker, NoteId, StyleId, TrackId};
pub use song::{ArrangerEvent, ChordEvent, Note, ProjectFile, Song, Track};
pub use validate::{Severity, ValidationIssue};

/// Value in the `format` key of a project file.
pub const PROJECT_FORMAT: &str = "lazy-arranger-project";

/// Schema version this crate writes.
pub const SCHEMA_VERSION: u32 = 1;

use thiserror::Error;

/// Errors raised while reading or writing a project file.
#[derive(Debug, Error)]
pub enum ProjectError {
    /// The JSON document was not well formed.
    #[error("project JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    /// The `format` key named another application or product.
    #[error("not a Lazy Arranger project: format {found:?}, expected {PROJECT_FORMAT:?}")]
    WrongFormat {
        /// The format string that was found.
        found: String,
    },
    /// The file was written by a newer schema than this build understands.
    #[error("project schema version {found} is newer than supported version {supported}")]
    NewerSchema {
        /// The version found in the file.
        found: u32,
        /// The newest version this build writes.
        supported: u32,
    },
    /// A legacy C++ project document could not be interpreted.
    #[error("legacy project is invalid: {0}")]
    Legacy(String),
    /// No migration path exists from the file's schema version.
    #[error("no migration from project schema {from} to {to}")]
    Migration {
        /// The version found in the file.
        from: u32,
        /// The version this build writes.
        to: u32,
    },
    /// The song failed validation.
    #[error("project is invalid: {0}")]
    Invalid(String),
}
