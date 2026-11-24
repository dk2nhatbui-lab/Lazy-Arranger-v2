//! Non-macOS placeholder native plug-in editor types.

use std::path::Path;

/// Placeholder VST3 native editor on platforms without a native host window harness.
#[derive(Debug)]
pub struct Vst3Editor;

impl Vst3Editor {
    /// Returns an error because native editor windows are currently macOS-only.
    ///
    /// # Errors
    ///
    /// Always returns an error on non-macOS targets.
    pub fn open(
        _title: &str,
        _bundle: &Path,
        _class_id: &str,
        _initial_state_base64: &str,
        _sample_rate: f64,
    ) -> Result<Self, String> {
        Err(String::from(
            "native VST3 editor windows are currently implemented on macOS only",
        ))
    }

    /// Initial editor dimensions.
    #[must_use]
    pub fn size(&self) -> (i32, i32) {
        (0, 0)
    }

    /// Always false on stub platforms.
    #[must_use]
    pub fn is_open(&self) -> bool {
        false
    }

    /// Always returns an error on stub platforms.
    ///
    /// # Errors
    ///
    /// Always returns an error.
    pub fn save_state_base64(&self) -> Result<String, String> {
        Err(String::from("native VST3 editor is unavailable"))
    }

    /// No-op on stub platforms.
    pub fn close(&mut self) {}
}

/// Placeholder Audio Unit native editor on non-macOS platforms.
#[derive(Debug)]
pub struct AuEditor;

impl AuEditor {
    /// Returns an error because Audio Units are macOS-only.
    ///
    /// # Errors
    ///
    /// Always returns an error on non-macOS targets.
    pub fn open(
        _title: &str,
        _identifier: &str,
        _initial_state: &str,
        _sample_rate: f64,
    ) -> Result<Self, String> {
        Err(String::from("Audio Unit editors require macOS"))
    }

    /// Always false on non-macOS platforms.
    #[must_use]
    pub fn used_custom_view(&self) -> bool {
        false
    }

    /// Always false on non-macOS platforms.
    #[must_use]
    pub fn is_open(&self) -> bool {
        false
    }

    /// Empty state string on non-macOS platforms.
    #[must_use]
    pub fn save_parameter_state(&self) -> String {
        String::new()
    }

    /// No-op on non-macOS platforms.
    pub fn close(&mut self) {}
}
