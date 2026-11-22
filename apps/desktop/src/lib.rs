//! Stage 6 desktop shell over the existing crates.
//!
//! The shell is a client of `project`, `music-core` and `app-commands`; no
//! domain type knows about egui. The v1 "Studio Console" colours live in
//! [`theme`]; the piano roll is a read-only canvas with tested geometry.

pub mod app;
pub mod editor;
pub mod piano_roll;
pub mod theme;
pub mod widgets;

pub use app::{ShellApp, demo_project, load_project};
