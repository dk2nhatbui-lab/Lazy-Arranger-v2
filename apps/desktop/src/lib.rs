//! Stage 6 desktop shell over the existing crates.
//!
//! The shell is a client of `project`, `music-core` and `app-commands`; no
//! domain type knows about egui. The v1 "Studio Console" colours live in
//! [`theme`]; the piano roll is a read-only canvas with tested geometry.

pub mod app;
pub mod automation_view;
pub mod editor;
pub mod persistence;
pub mod piano_roll;
pub mod score_view;
pub mod settings;
pub mod theme;
pub mod widgets;

pub use app::{ShellApp, demo_project, load_project};

/// The window and header icon, as raw RGBA for egui.
#[must_use]
pub fn icon_data() -> egui::IconData {
    const SIZE: u32 = 256;
    egui::IconData {
        rgba: include_bytes!("../assets/app-icon-256.rgba").to_vec(),
        width: SIZE,
        height: SIZE,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_icon_is_well_formed() {
        let icon = super::icon_data();
        assert_eq!(icon.width, 256);
        assert_eq!(icon.height, 256);
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // Corners are transparent outside the gear; the centre is opaque.
        assert_eq!(icon.rgba[3], 0);
        let centre = (128 * 256 + 128) * 4;
        assert_eq!(icon.rgba[centre + 3], 255);
    }
}
