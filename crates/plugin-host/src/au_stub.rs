//! Non-macOS Audio Unit placeholders.
//!
//! Audio Units exist only on macOS. These types keep the desktop and audio
//! crates platform-neutral: construction fails cleanly and no native code runs.

/// Placeholder for a macOS Audio Unit effect.
#[derive(Debug, Default)]
pub struct AuEffect;

impl AuEffect {
    /// Audio Units are unavailable on this platform.
    ///
    /// # Errors
    ///
    /// Always returns an error outside macOS.
    pub fn new(_identifier: &str, _sample_rate: f64) -> Result<Self, String> {
        Err(String::from("Audio Units are only available on macOS"))
    }

    /// No parameters can be enumerated.
    #[must_use]
    pub fn parameters(&self) -> Vec<(u32, String, f32, f32, f32, f32)> {
        Vec::new()
    }

    /// Reject parameter edits on a placeholder.
    ///
    /// # Errors
    ///
    /// Always returns an error outside macOS.
    pub fn set_parameter(&self, _id: u32, _value: f32) -> Result<(), String> {
        Err(String::from("Audio Units are only available on macOS"))
    }

    /// Ignore saved state on a placeholder.
    pub fn apply_state(&self, _state: &str) {}

    /// A placeholder has no latency.
    #[must_use]
    pub fn latency_samples(&self) -> usize {
        0
    }
}

/// Placeholder for a macOS Audio Unit instrument.
#[derive(Debug, Default)]
pub struct AuInstrument;

impl AuInstrument {
    /// Audio Units are unavailable on this platform.
    ///
    /// # Errors
    ///
    /// Always returns an error outside macOS.
    pub fn new(_identifier: &str, _sample_rate: f64) -> Result<Self, String> {
        Err(String::from("Audio Units are only available on macOS"))
    }

    /// No parameters can be enumerated.
    #[must_use]
    pub fn parameters(&self) -> Vec<(u32, String, f32, f32, f32, f32)> {
        Vec::new()
    }

    /// Reject parameter edits on a placeholder.
    ///
    /// # Errors
    ///
    /// Always returns an error outside macOS.
    pub fn set_parameter(&self, _id: u32, _value: f32) -> Result<(), String> {
        Err(String::from("Audio Units are only available on macOS"))
    }

    /// Ignore saved state on a placeholder.
    pub fn apply_state(&self, _state: &str) {}
}
