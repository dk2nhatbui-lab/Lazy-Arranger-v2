//! Project files: loading, classifying and exporting.

use std::path::PathBuf;

use music_core::Tick;
use project::ProjectError;
use project::song::ProjectFile;

use crate::editor::Editor;

use super::{FileMode, ShellApp};

/// Audio and video files Import Audio takes.
const AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "flac", "mp3", "m4a", "aiff", "aif", "ogg", "mp4", "mov", "mkv", "webm", "avi", "wmv",
    "mpeg", "mpg",
];

/// Shows the native file dialog for `mode`, starting at `suggested`.
/// `None` when the user cancels.
pub(super) fn dialog(mode: FileMode, suggested: &str) -> Option<PathBuf> {
    let suggested = std::path::Path::new(suggested);
    let mut dialog = rfd::FileDialog::new();
    if let Some(folder) = suggested.parent().filter(|folder| folder.is_dir()) {
        dialog = dialog.set_directory(folder);
    }
    if let Some(name) = suggested.file_name().and_then(|name| name.to_str()) {
        dialog = dialog.set_file_name(name);
    }
    match mode {
        FileMode::Open => dialog
            .set_title("Open project")
            .add_filter(
                "Projects, MIDI and MusicXML",
                &["arranger", "json", "mid", "midi", "musicxml", "xml"],
            )
            .pick_file(),
        FileMode::SaveAs => dialog
            .set_title("Save project")
            .add_filter("Lazy Arranger project", &["arranger"])
            .save_file(),
        FileMode::ImportAudio => dialog
            .set_title("Import audio file")
            .add_filter("Audio and video", AUDIO_EXTENSIONS)
            .pick_file(),
        FileMode::ImportL6max => dialog.set_title("Import L6max recording").pick_folder(),
        FileMode::ExportMidi => dialog
            .set_title("Export MIDI file")
            .add_filter("MIDI file", &["mid"])
            .save_file(),
        FileMode::ExportMusicXml => dialog
            .set_title("Export MusicXML file")
            .add_filter("MusicXML", &["musicxml"])
            .save_file(),
        FileMode::ExportWav => dialog
            .set_title("Export audio file")
            .add_filter("WAV audio", &["wav"])
            .save_file(),
        FileMode::ExportStems => dialog.set_title("Export stems to a folder").pick_folder(),
    }
}

impl ShellApp {
    /// Asks where to open, save, import or export.
    ///
    /// macOS and Windows show the native dialog at once. Linux shows the
    /// path window, with a Browse button for the dialog, because the Linux
    /// dialog needs a desktop portal that may not be running.
    pub(super) fn choose_file(&mut self, mode: FileMode) {
        self.file_path_input = self.suggested_path(mode);
        if cfg!(target_os = "linux") {
            self.file_mode = Some(mode);
            return;
        }
        if let Some(path) = dialog(mode, &self.file_path_input) {
            self.run_file_action(mode, path);
        }
    }

    /// The path the dialog starts at.
    fn suggested_path(&self, mode: FileMode) -> String {
        let project = self.project_path.as_ref();
        let with_extension = |extension: &str, fallback: &str| {
            project.map_or_else(
                || String::from(fallback),
                |path| path.with_extension(extension).display().to_string(),
            )
        };
        match mode {
            FileMode::Open => project
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            FileMode::SaveAs => project.map_or_else(
                || String::from("project.arranger"),
                |path| path.display().to_string(),
            ),
            FileMode::ExportMidi => with_extension("mid", "project.mid"),
            FileMode::ExportWav => with_extension("wav", "project.wav"),
            FileMode::ExportMusicXml => with_extension("musicxml", "project.musicxml"),
            FileMode::ExportStems => project.and_then(|path| path.parent()).map_or_else(
                || String::from("stems"),
                |parent| parent.join("stems").display().to_string(),
            ),
            FileMode::ImportAudio | FileMode::ImportL6max => String::new(),
        }
    }
}

/// The format of a project path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectKind {
    /// The app's JSON project.
    Json,
    /// A Standard MIDI File.
    Midi,
    /// A MusicXML score.
    MusicXml,
    /// A project saved by the C++ app (also `.arranger`, older schema).
    CppProject,
}

impl ProjectKind {
    /// Short label for status messages.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Json => "project",
            Self::Midi => "MIDI",
            Self::MusicXml => "MusicXML",
            Self::CppProject => "C++ app project",
        }
    }
}

/// Classifies a path by extension; unknown extensions are JSON projects.
#[must_use]
pub fn project_kind(path: &std::path::Path) -> ProjectKind {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mid" | "midi") => ProjectKind::Midi,
        Some("musicxml" | "xml") => ProjectKind::MusicXml,
        _ => ProjectKind::Json,
    }
}

/// Like [`project_kind`], but also reads a JSON file to tell a C++ app
/// project from one of this app's projects. Both use `.arranger`.
#[must_use]
pub fn file_kind(path: &std::path::Path) -> ProjectKind {
    match project_kind(path) {
        ProjectKind::Json => std::fs::read_to_string(path)
            .ok()
            .filter(|text| is_cpp_project(text))
            .map_or(ProjectKind::Json, |_| ProjectKind::CppProject),
        other => other,
    }
}

/// True when the JSON text is a project saved by the C++ app.
fn is_cpp_project(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|root| {
            root.get("format")
                .and_then(serde_json::Value::as_str)
                .map(|format| format == project::legacy::CPP_FORMAT)
        })
        .unwrap_or(false)
}

/// True for `.mid` or `.midi`, case-insensitive.
#[must_use]
pub fn is_midi_path(path: &std::path::Path) -> bool {
    project_kind(path) == ProjectKind::Midi
}

/// Writes a keyboard MIDI export and returns its track count.
///
/// # Errors
///
/// Returns a message when the file cannot be written.
pub fn export_midi_file(
    song: &project::song::Song,
    path: &std::path::Path,
) -> Result<usize, String> {
    export_midi_file_with_profile(song, path, None)
}

/// Writes a keyboard MIDI export using a loaded device profile.
///
/// # Errors
///
/// Returns a message when the file cannot be written.
pub fn export_midi_file_with_profile(
    song: &project::song::Song,
    path: &std::path::Path,
    profile: Option<&midi_devices::profiles::LoadedProfile>,
) -> Result<usize, String> {
    let bank = profile.and_then(|profile| {
        profile
            .banks
            .iter()
            .find_map(|bank| Some((bank.msb?, bank.lsb?)))
    });
    let voices: Vec<smf::KeyboardVoice> = song
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let drums = track
                .notes
                .iter()
                .any(|note| note.channel.zero_based() == 9);
            smf::KeyboardVoice {
                channel: if drums {
                    9
                } else {
                    u8::try_from(index).unwrap_or(0) & 0x0f
                },
                program: track.program.unwrap_or_else(|| match profile {
                    Some(profile) => {
                        midi_devices::profiles::nearest_in_profile(profile, &track.name)
                    }
                    None => midi_devices::nearest_voice(&midi_devices::PSR_S970, &track.name),
                }),
                volume: track.volume.min(127),
                pan: track.pan.min(127),
                bank,
            }
        })
        .collect();
    let (bytes, report) = smf::keyboard::export(song, &voices, &[]);
    std::fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(report.tracks)
}

/// Scans a take folder and imports each WAV at `start`.
///
/// # Errors
///
/// Returns a message when the folder cannot be scanned or a track cannot be
/// inserted.
pub fn import_l6max_folder(
    editor: &mut Editor,
    folder: &std::path::Path,
    start: Tick,
) -> Result<(usize, usize), String> {
    let plan = audio::l6max::scan(folder)?;
    let imported = editor.import_l6max(&plan, start)?;
    Ok((imported, plan.problems.len()))
}

/// Writes one 24-bit WAV per audible track plus `stems.txt`.
///
/// # Errors
///
/// Returns a message when the folder or a file cannot be written.
pub fn export_stems_folder(
    song: &project::song::Song,
    directory: &std::path::Path,
) -> Result<usize, String> {
    let stems = audio::render_stems(song, audio::SAMPLE_RATE);
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    let mut manifest = String::new();
    for stem in &stems {
        let slug: String = stem
            .name
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let file = format!("{}-{slug}.wav", stem.track.get());
        std::fs::write(directory.join(&file), stem.audio.to_wav24())
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        manifest.push_str(&format!(
            "{}\t{file}\t{:.2}s\n",
            stem.name,
            stem.audio.duration_seconds()
        ));
    }
    std::fs::write(directory.join("stems.txt"), manifest)
        .map_err(|error| format!("{}: {error}", directory.display()))?;
    Ok(stems.len())
}

/// Renders and writes a 24-bit WAV, returning duration and audio-reference problems.
///
/// # Errors
///
/// Returns a message when the file cannot be written.
pub fn export_wav_file(
    song: &project::song::Song,
    path: &std::path::Path,
) -> Result<(f64, usize), String> {
    let rendered = audio::render(song, audio::SAMPLE_RATE);
    let problems = audio::mix::check_audio_references(song).len();
    std::fs::write(path, rendered.to_wav24())
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok((rendered.duration_seconds(), problems))
}

/// Writes a MusicXML export.
///
/// # Errors
///
/// Returns a message when the file cannot be written.
pub fn export_musicxml_file(
    song: &project::song::Song,
    path: &std::path::Path,
    title: &str,
) -> Result<(), String> {
    let xml = musicxml::export(song, title);
    std::fs::write(path, xml).map_err(|error| format!("{}: {error}", path.display()))
}

/// Loads an optional startup project, falling back to the demo on failure.
///
/// A failed open clears the path so a later save cannot overwrite the damaged
/// file with the demo.
#[must_use]
pub fn load_startup_project(
    path: Option<&std::path::Path>,
) -> (ProjectFile, Option<PathBuf>, Option<String>) {
    let Some(path) = path else {
        return (demo_project(), None, None);
    };
    match load_project(&path.to_path_buf()) {
        // Only this app's own projects keep their path, so Save never
        // writes over a MIDI, MusicXML or C++ app file.
        Ok(project) => {
            let own = file_kind(path) == ProjectKind::Json;
            (project, own.then(|| path.to_path_buf()), None)
        }
        Err(error) => (
            demo_project(),
            None,
            Some(format!(
                "could not open {}: {error}. The demo is loaded and the file path was cleared.",
                path.display()
            )),
        ),
    }
}

pub fn load_project(path: &PathBuf) -> Result<ProjectFile, ProjectError> {
    let bytes = std::fs::read(path)
        .map_err(|error| ProjectError::Legacy(format!("{}: {error}", path.display())))?;
    match project_kind(path) {
        ProjectKind::Midi => {
            let (song, _) = smf::decode(&bytes)
                .map_err(|error| ProjectError::Legacy(format!("{}: {error}", path.display())))?;
            return Ok(ProjectFile::new(song));
        }
        ProjectKind::MusicXml => {
            let text = String::from_utf8(bytes)
                .map_err(|error| ProjectError::Legacy(format!("{}: {error}", path.display())))?;
            let song = musicxml::import::import(&text)
                .map_err(|error| ProjectError::Legacy(format!("{}: {error}", path.display())))?;
            return Ok(ProjectFile::new(song));
        }
        ProjectKind::Json | ProjectKind::CppProject => {}
    }
    let text = String::from_utf8(bytes)
        .map_err(|error| ProjectError::Legacy(format!("{}: {error}", path.display())))?;
    if is_cpp_project(&text) {
        let mut project = project::legacy::import_cpp_project(&text)?.project;
        if let Some(dir) = path.parent() {
            let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
            project::legacy::resolve_audio_paths(&mut project.song, &dir);
        }
        return Ok(project);
    }
    ProjectFile::from_json(&text)
}

/// The built-in demo used when no project path is given.
#[must_use]
pub fn demo_project() -> ProjectFile {
    ProjectFile::new(arranging::demo::demo_song())
}
