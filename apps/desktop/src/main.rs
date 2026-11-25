//! Desktop entry point. All behaviour lives in the library so tests can reach it.

// Release builds on Windows are GUI apps: no console window opens behind them.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;

fn main() -> eframe::Result<()> {
    let project_path: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);
    let (project, project_path, startup_error) =
        lazy_arranger_desktop::load_startup_project(project_path.as_deref());
    let settings = lazy_arranger_desktop::settings::Settings::load(
        &lazy_arranger_desktop::settings::Settings::path(),
    );
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Lazy Arranger Midi DAW")
        .with_inner_size([settings.window_width, settings.window_height]);
    if let (Some(x), Some(y)) = (settings.window_x, settings.window_y) {
        viewport = viewport.with_position([x, y]);
    }
    let options = eframe::NativeOptions {
        viewport: viewport
            .with_min_inner_size([960.0, 600.0])
            // ADR-011: start hidden; the shell shows it after the first frame.
            .with_visible(false)
            .with_icon(Arc::new(lazy_arranger_desktop::icon_data())),
        ..Default::default()
    };
    eframe::run_native(
        "Lazy Arranger Midi DAW",
        options,
        Box::new(move |cc| {
            let mut app = lazy_arranger_desktop::ShellApp::new(&cc.egui_ctx, project, project_path);
            if let Some(error) = startup_error {
                app.report_startup_error(error);
            }
            Ok(Box::new(app))
        }),
    )
}
