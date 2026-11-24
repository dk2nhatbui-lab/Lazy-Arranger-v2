//! Desktop entry point. All behaviour lives in the library so tests can reach it.

use std::path::PathBuf;
use std::sync::Arc;

fn main() -> eframe::Result<()> {
    let project_path: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);
    let (project, project_path, startup_error) =
        lazy_arranger_desktop::load_startup_project(project_path.as_deref());
    let settings = lazy_arranger_desktop::settings::Settings::load(
        &lazy_arranger_desktop::settings::Settings::path(),
    );
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Lazy Arranger Midi DAW")
            .with_inner_size([settings.window_width, settings.window_height])
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
