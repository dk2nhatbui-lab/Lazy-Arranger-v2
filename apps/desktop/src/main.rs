//! Desktop entry point. All behaviour lives in the library so tests can reach it.

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let project_path: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);
    let project = match &project_path {
        Some(path) => lazy_arranger_desktop::load_project(path)
            .unwrap_or_else(|_| lazy_arranger_desktop::demo_project()),
        None => lazy_arranger_desktop::demo_project(),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Lazy Arranger")
            .with_inner_size([1360.0, 840.0])
            .with_min_inner_size([960.0, 600.0])
            // ADR-011: start hidden; the shell shows it after the first frame.
            .with_visible(false),
        ..Default::default()
    };
    eframe::run_native(
        "Lazy Arranger",
        options,
        Box::new(move |cc| {
            Ok(Box::new(lazy_arranger_desktop::ShellApp::new(
                &cc.egui_ctx,
                project,
                project_path,
            )))
        }),
    )
}
