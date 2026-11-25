//! The Stage 6 shell: Perform / Arrange / Mix over the existing models.

use std::path::PathBuf;
use std::time::Duration;

use egui::{Context, Rect, ViewportCommand};
use music_core::{PPQ, Tick};
use project::song::{ProjectFile, Song};
use project::{NoteId, TrackId};

use crate::editor::{Editor, snap};
use crate::persistence::{self, Autosave};

use crate::piano_roll::{self, RollState};
use crate::theme;
use crate::transport::{metronome_button, transport_button};
use crate::widgets;
use crate::widgets::{draw_track_icon, mixer_fader, mixer_knob, pan_label};

mod assistant;
mod devices;
mod drums;
mod files;
mod jam;
mod layout;
mod map;
mod menu;
mod mixer;
mod native_menu;
mod style;

use assistant::AssistantState;
use devices::AudioChordsState;
use devices::MidiOutState;
use devices::{
    D05State, L6MaxState, PsrState, XgState, load_device_profiles, load_l6_scenes, load_psr_regs,
};
pub use devices::{parse_sysex, sysex_hex};
pub use files::{
    ProjectKind, demo_project, export_midi_file, export_midi_file_with_profile,
    export_musicxml_file, export_stems_folder, export_wav_file, file_kind, import_l6max_folder,
    is_midi_path, load_project, load_startup_project, project_kind,
};
use layout::ScoreUi;
use menu::MenuState;
use style::AudioStyleState;
use style::StyleState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Perform,
    Arrange,
    Drums,
    Loops,
    Score,
    Map,
    Mix,
}

impl View {
    /// Parses a view name for the `ARRANGER_VIEW` developer override.
    #[must_use]
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "perform" => Some(Self::Perform),
            "arrange" => Some(Self::Arrange),
            "mix" => Some(Self::Mix),
            "drums" => Some(Self::Drums),
            "loops" => Some(Self::Loops),
            "score" => Some(Self::Score),
            "map" => Some(Self::Map),
            _ => None,
        }
    }

    /// Stable name used by settings.
    #[must_use]
    fn as_str(self) -> &'static str {
        match self {
            Self::Perform => "perform",
            Self::Arrange => "arrange",
            Self::Drums => "drums",
            Self::Loops => "loops",
            Self::Score => "score",
            Self::Map => "map",
            Self::Mix => "mix",
        }
    }

    /// Every view, in tab order.
    const ALL: [Self; 5] = [
        Self::Arrange,
        Self::Score,
        Self::Drums,
        Self::Loops,
        Self::Map,
    ];

    /// The views Kids Mode offers.
    #[must_use]
    fn allowed(kids_mode: bool) -> Vec<Self> {
        if kids_mode {
            vec![Self::Perform, Self::Arrange]
        } else {
            Self::ALL.to_vec()
        }
    }

    /// The translatable label key.
    fn key(self) -> crate::strings::Key {
        match self {
            Self::Perform => crate::strings::Key::Perform,
            Self::Arrange => crate::strings::Key::Arrange,
            Self::Drums => crate::strings::Key::Arrange,
            Self::Mix => crate::strings::Key::Mix,
            Self::Loops => crate::strings::Key::Loops,
            Self::Score => crate::strings::Key::Score,
            Self::Map => crate::strings::Key::Arrange,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Perform => 0,
            Self::Arrange => 1,
            Self::Score => 2,
            Self::Drums => 3,
            Self::Loops => 4,
            Self::Map => 5,
            Self::Mix => 6,
        }
    }
}

/// One AU slot open in the parameter window: effect or instrument.
enum AuEditorUnit {
    Effect(plugin_host::AuEffect),
    Instrument(plugin_host::AuInstrument),
}

impl AuEditorUnit {
    fn parameters(&self) -> Vec<(u32, String, f32, f32, f32, f32)> {
        match self {
            Self::Effect(effect) => effect.parameters(),
            Self::Instrument(instrument) => instrument.parameters(),
        }
    }

    fn set_parameter(&self, id: u32, value: f32) {
        match self {
            Self::Effect(effect) => {
                let _ = effect.set_parameter(id, value);
            }
            Self::Instrument(instrument) => {
                let _ = instrument.set_parameter(id, value);
            }
        }
    }
}

#[derive(Clone, Debug)]
struct AssistantProposal {
    command: String,
    label: String,
}

struct Row {
    index: usize,
    id: TrackId,
    name: String,
    notes: usize,
    program: Option<u8>,
    audio: bool,
    muted: bool,
    soloed: bool,
    armed: bool,
}

/// Which file action the path window is serving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileMode {
    /// Open an existing project or MIDI file.
    Open,
    /// Save the project to a new path.
    SaveAs,
    /// Import a WAV as an audio track.
    ImportAudio,
    /// Export the song as a MIDI file.
    ExportMidi,
    /// Export the song as MusicXML.
    ExportMusicXml,
    /// Export the mixed song as a WAV.
    ExportWav,
    /// Export one WAV per audible track into a folder.
    ExportStems,
    /// Import every WAV in a folder as an L6max take.
    ImportL6max,
}

/// Theory facts for the selected song: key, scale, roman chords and cadence.
#[must_use]
pub fn theory_summary(song: &project::song::Song) -> Vec<String> {
    use music_core::theory::{self, Mode};
    let key = score::spelling::detect_key(song);
    let mode = if key.minor {
        Mode::NaturalMinor
    } else {
        Mode::Major
    };
    let scale: Vec<&str> = theory::scale(key.tonic, mode)
        .iter()
        .map(|pitch| pitch.name_sharp())
        .collect();
    let mut lines = vec![
        format!("Key: {} {}", key.tonic.name_sharp(), mode.name()),
        format!("Scale: {}", scale.join(" ")),
    ];
    let chords: Vec<music_core::Chord> = song
        .chords
        .iter()
        .take(8)
        .map(|event| event.chord)
        .collect();
    if chords.is_empty() {
        lines.push(String::from("No chords yet"));
        return lines;
    }
    let labels: Vec<String> = chords
        .iter()
        .map(|chord| theory::roman_label(*chord, key.tonic))
        .collect();
    lines.push(format!("Chords: {}", labels.join(" - ")));
    lines.push(format!(
        "Cadence: {:?}",
        theory::cadence(&chords, key.tonic)
    ));
    if let Some(pair) = chords.windows(2).next() {
        lines.push(format!(
            "Voice leading: {} -> {} cost {}",
            pair[0].format(music_core::Spelling::Sharp),
            pair[1].format(music_core::Spelling::Sharp),
            theory::voice_leading_cost(pair[0], pair[1])
        ));
    }
    lines
}

fn consume_binding(context: &Context, text: &str, fallback: &str) -> bool {
    let binding =
        crate::settings::parse_shortcut(text).or_else(|| crate::settings::parse_shortcut(fallback));
    binding.is_some_and(|binding| {
        context.input_mut(|input| input.consume_key(binding.modifiers, binding.key))
    })
}

/// Rounds recorded note starts to the nearest grid step and re-sorts them.
fn resolve_playlist_path(base: &std::path::Path, path: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.parent()
            .map_or_else(|| path.to_path_buf(), |parent| parent.join(path))
    }
}

fn detected_to_notes(
    detected: &[audio::analysis::DetectedNote],
    bpm: f64,
    channel: music_core::Channel,
) -> Result<Vec<project::song::Note>, String> {
    let mut notes = Vec::with_capacity(detected.len());
    for (index, detected) in detected.iter().enumerate() {
        let (start, duration) = audio::analysis::as_ticks(detected, bpm);
        let Some(pitch) = audio::analysis::as_pitch(detected.pitch) else {
            continue;
        };
        notes.push(project::song::Note {
            id: project::NoteId::new(index as u32 + 1).map_err(|error| error.to_string())?,
            start,
            duration: duration.max(1),
            pitch,
            velocity: music_core::Velocity::new((detected.level * 127.0).clamp(1.0, 127.0) as u8)
                .map_err(|error| error.to_string())?,
            channel,
        });
    }
    Ok(notes)
}

pub fn quantise_recorded(notes: &mut [project::song::Note], grid: Tick) {
    if grid <= 0 {
        return;
    }
    for note in notes.iter_mut() {
        note.start = ((note.start.max(0) + grid / 2) / grid) * grid;
    }
    notes.sort_by(|left, right| {
        (left.start, left.pitch.as_u8()).cmp(&(right.start, right.pitch.as_u8()))
    });
}

/// True when a track's range or role calls for a piano-style grand staff.
#[must_use]
pub fn needs_grand_staff(track: &project::song::Track) -> bool {
    let low = track.notes.iter().any(|note| note.pitch.as_u8() < 60);
    let high = track.notes.iter().any(|note| note.pitch.as_u8() >= 60);
    (low && high) || track.name.to_ascii_lowercase().contains("piano")
}

/// Builds a short stereo metronome count-in at 48 kHz.
fn count_in_seconds(bpm: f64, bars: u8, meter: music_core::TimeSignature) -> f64 {
    let bar_seconds = meter.bar_ticks() as f64 / music_core::PPQ as f64 * 60.0 / bpm.max(1.0);
    f64::from(bars) * bar_seconds
}

fn count_in_audio(bpm: f64, bars: u8, meter: music_core::TimeSignature) -> audio::RenderedAudio {
    let sample_rate = 48_000_u32;
    let beats_per_bar = usize::from(meter.numerator.max(1));
    let bar_seconds = meter.bar_ticks() as f64 / music_core::PPQ as f64 * 60.0 / bpm.max(1.0);
    let beats = usize::from(bars) * beats_per_bar;
    let seconds = count_in_seconds(bpm, bars, meter);
    let frames = (seconds * f64::from(sample_rate)).round() as usize;
    let mut samples = vec![0.0_f32; frames * 2];
    let click_frames = (0.035 * f64::from(sample_rate)) as usize;
    for beat in 0..beats {
        let start = ((beat as f64 / beats_per_bar as f64 * bar_seconds) * f64::from(sample_rate))
            .round() as usize;
        let amplitude = if beat.is_multiple_of(beats_per_bar) {
            0.5
        } else {
            0.3
        };
        for offset in 0..click_frames {
            let frame = start + offset;
            if frame >= frames {
                break;
            }
            let time = offset as f32 / sample_rate as f32;
            let envelope = 1.0 - offset as f32 / click_frames as f32;
            let value = (std::f32::consts::TAU * 1_000.0 * time).sin() * envelope * amplitude;
            samples[frame * 2] += value;
            samples[frame * 2 + 1] += value;
        }
    }
    audio::RenderedAudio {
        samples,
        sample_rate,
    }
}

fn style_preview_song(style: &arranger::style::Style) -> Result<project::Song, String> {
    let mut song = project::Song::new("Style audition", 120.0);
    let mut track = project::song::Track::new(
        project::TrackId::new(1).map_err(|error| error.to_string())?,
        "Style",
    );
    let mut offset = 0;
    let mut next_note = 1_u32;
    for section in &style.sections {
        let compiled = style
            .compile(&section.name, offset, &[])
            .map_err(|error| error.to_string())?;
        for note in compiled {
            track.notes.push(project::song::Note {
                id: project::NoteId::new(next_note).map_err(|error| error.to_string())?,
                start: note.tick,
                duration: note.duration,
                pitch: music_core::Pitch::new(note.pitch).map_err(|error| error.to_string())?,
                velocity: music_core::Velocity::new(note.velocity)
                    .map_err(|error| error.to_string())?,
                channel: note.channel,
            });
            next_note += 1;
        }
        offset = offset.saturating_add(section.length_ticks);
    }
    song.tracks.push(track);
    Ok(song)
}

fn entry_length(base: music_core::Tick, dotted: bool) -> music_core::Tick {
    if dotted {
        base.saturating_mul(3) / 2
    } else {
        base
    }
}

fn score_typing(view: View, entry: bool, wants_keyboard: bool) -> bool {
    view == View::Score && entry && !wants_keyboard
}

fn score_key_pitch(key: egui::Key, octave: i32) -> Option<u8> {
    let semitone = match key {
        egui::Key::A => 0,
        egui::Key::W => 1,
        egui::Key::S => 2,
        egui::Key::E => 3,
        egui::Key::D => 4,
        egui::Key::F => 5,
        egui::Key::T => 6,
        egui::Key::G => 7,
        egui::Key::Y => 8,
        egui::Key::H => 9,
        egui::Key::U => 10,
        egui::Key::J => 11,
        _ => return None,
    };
    Some((((octave.clamp(0, 8) + 1) * 12 + semitone) as u8).min(127))
}

fn held_chord_name(held: &[bool; 128]) -> Option<String> {
    let pitches: Vec<u8> = held
        .iter()
        .enumerate()
        .filter_map(|(pitch, down)| down.then_some(u8::try_from(pitch).ok()?))
        .collect();
    if pitches.len() < 3 {
        return None;
    }
    arranger::recognize_symbol(&pitches, pitches.first().copied())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StyleSectionRole {
    Main,
    Fill,
    Ending,
}

fn style_section_role(name: &str) -> StyleSectionRole {
    let lower = name.to_ascii_lowercase();
    if lower.contains("fill") {
        StyleSectionRole::Fill
    } else if lower.contains("ending") {
        StyleSectionRole::Ending
    } else {
        StyleSectionRole::Main
    }
}

fn next_bar_seconds(now: f64, bar_seconds: f64) -> f64 {
    (now / bar_seconds.max(0.001))
        .floor()
        .mul_add(bar_seconds, bar_seconds)
}

fn style_section_song(
    imported: &style_formats::ImportedStyle,
    section: usize,
    chord_name: &str,
) -> Result<project::Song, String> {
    let names = imported.section_names();
    let name = names
        .get(section)
        .ok_or_else(|| String::from("no section selected"))?;
    let chord = music_core::Chord::parse(chord_name).map_err(|error| error.to_string())?;
    let arranger_chord =
        arranger::ArrangerChord::try_from(&chord).map_err(|error| error.to_string())?;
    let compiled = imported
        .native
        .compile(
            name,
            0,
            &[arranger::ChordAt {
                tick: 0,
                chord: arranger_chord,
            }],
        )
        .map_err(|error| error.to_string())?;
    let mut song = project::Song::new("Style audition", imported.tempo_bpm.max(30.0));
    let mut track = project::song::Track::new(
        project::TrackId::new(1).map_err(|error| error.to_string())?,
        "Style",
    );
    for (index, note) in compiled.into_iter().enumerate() {
        track.notes.push(project::song::Note {
            id: project::NoteId::new(u32::try_from(index + 1).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?,
            start: note.tick,
            duration: note.duration,
            pitch: music_core::Pitch::new(note.pitch).map_err(|error| error.to_string())?,
            velocity: music_core::Velocity::new(note.velocity)
                .map_err(|error| error.to_string())?,
            channel: note.channel,
        });
    }
    song.tracks.push(track);
    Ok(song)
}

fn plugin_matches(
    plugin: &plugins::FoundPlugin,
    search: &str,
    format: Option<plugins::Format>,
) -> bool {
    if format.is_some_and(|wanted| wanted != plugin.format) {
        return false;
    }
    let needle = search.trim().to_ascii_lowercase();
    needle.is_empty()
        || plugin.name.to_ascii_lowercase().contains(&needle)
        || plugin.vendor.to_ascii_lowercase().contains(&needle)
        || plugin.identifier.to_ascii_lowercase().contains(&needle)
        || plugin
            .path
            .display()
            .to_string()
            .to_ascii_lowercase()
            .contains(&needle)
}

/// Compact text for the recent-MIDI list.
#[must_use]
pub fn midi_label(message: &music_core::MidiMessage) -> String {
    use music_core::MidiMessage;
    match message {
        MidiMessage::NoteOn {
            channel,
            key,
            velocity,
        } => format!(
            "ch{} note-on {} vel {}",
            channel.one_based(),
            key,
            velocity.as_u8()
        ),
        MidiMessage::NoteOff {
            channel,
            key,
            velocity,
        } => format!(
            "ch{} note-off {} vel {}",
            channel.one_based(),
            key,
            velocity.as_u8()
        ),
        MidiMessage::ControlChange {
            channel,
            controller,
            value,
        } => format!(
            "ch{} CC {} = {}",
            channel.one_based(),
            controller.as_u8(),
            value
        ),
        MidiMessage::ProgramChange { channel, program } => {
            format!("ch{} program {}", channel.one_based(), program)
        }
        MidiMessage::PitchBend { channel, value } => {
            format!("ch{} bend {}", channel.one_based(), value)
        }
    }
}

/// Builds `lp` arguments for the print dialog.
#[must_use]
pub fn print_arguments(
    printer: Option<&str>,
    copies: u32,
    pages: &str,
    path: &std::path::Path,
) -> Vec<String> {
    let mut arguments = Vec::new();
    if let Some(printer) = printer {
        arguments.push(String::from("-d"));
        arguments.push(printer.to_string());
    }
    if copies > 1 {
        arguments.push(String::from("-n"));
        arguments.push(copies.to_string());
    }
    let pages = pages.trim();
    if !pages.is_empty() {
        arguments.push(String::from("-o"));
        arguments.push(format!("page-ranges={pages}"));
    }
    arguments.push(path.display().to_string());
    arguments
}

/// Appends `.json` when a Save-as path has no extension.
#[must_use]
pub fn normalize_save_path(path: std::path::PathBuf) -> std::path::PathBuf {
    if path.as_os_str().is_empty() || path.extension().is_some() {
        path
    } else {
        path.with_extension("json")
    }
}

/// Applies a clip click to the multi-selection and returns the primary clip.
#[must_use]
pub fn apply_clip_click(
    selection: &mut Vec<(TrackId, project::ClipId)>,
    entry: (TrackId, project::ClipId),
    shift: bool,
) -> Option<(TrackId, project::ClipId)> {
    if shift {
        if let Some(index) = selection.iter().position(|item| *item == entry) {
            selection.remove(index);
        } else {
            selection.push(entry);
        }
        selection.last().copied()
    } else {
        selection.clear();
        selection.push(entry);
        Some(entry)
    }
}

/// Formats a count with a regular plural.
#[must_use]
pub fn plural(count: usize, singular: &str) -> String {
    format!("{count} {singular}{}", if count == 1 { "" } else { "s" })
}

/// Label for the built-in audio engine.
#[must_use]
pub fn audio_engine_label() -> String {
    format!("audio {} kHz (cpal)", audio::SAMPLE_RATE / 1000)
}

/// Parses `lpstat -p` lines into printer names.
#[must_use]
pub fn printers_from_lpstat(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("printer "))
        .filter_map(|rest| rest.split_whitespace().next())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// Queries the local CUPS printer list; empty when `lpstat` is unavailable.
#[must_use]
pub fn available_printers() -> Vec<String> {
    let Ok(output) = std::process::Command::new("lpstat").arg("-p").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    printers_from_lpstat(&String::from_utf8_lossy(&output.stdout))
}

struct RenderedSong {
    audio: audio::RenderedAudio,
    stems: Vec<(TrackId, Vec<f32>)>,
    problems: Vec<audio::mix::AudioProblem>,
}

enum NativeEditorKind {
    Vst3(plugin_host::Vst3Editor),
    Au(plugin_host::AuEditor),
}

struct OpenNativeEditor {
    track: TrackId,
    slot_id: u32,
    name: String,
    kind: NativeEditorKind,
}

impl OpenNativeEditor {
    fn is_open(&self) -> bool {
        match &self.kind {
            NativeEditorKind::Vst3(editor) => editor.is_open(),
            NativeEditorKind::Au(editor) => editor.is_open(),
        }
    }

    fn capture_state(&self) -> Result<String, String> {
        match &self.kind {
            NativeEditorKind::Vst3(editor) => editor.save_state_base64(),
            NativeEditorKind::Au(editor) => Ok(editor.save_parameter_state()),
        }
    }

    fn close(&mut self) {
        match &mut self.kind {
            NativeEditorKind::Vst3(editor) => editor.close(),
            NativeEditorKind::Au(editor) => editor.close(),
        }
    }
}

enum D05TaskResult {
    TempPatch(Result<(Box<midi_devices::d05::TempPatch>, usize), String>),
    BankNames(u8, Result<Vec<String>, String>),
}

pub struct ShellApp {
    editor: Editor,
    selected_note: Option<(TrackId, NoteId)>,
    selected_notes: Vec<(TrackId, NoteId)>,
    selected_chord: Option<Tick>,
    preview_player: Option<audio::player::Player>,
    selected_clip: Option<(TrackId, project::ClipId)>,
    selected_clips: Vec<(TrackId, project::ClipId)>,
    clip_clipboard: Vec<(TrackId, project::song::Clip)>,
    chord_dialog: bool,
    chord_dialog_target: Tick,
    reharmonize_dialog: bool,
    chord_input: String,
    key: music_core::PitchClass,
    status: Option<String>,
    doctor_dialog: bool,
    doctor_proposals: Vec<arranging::Proposal>,
    fit_proposals: Vec<arranging::Proposal>,
    fit_low: u8,
    fit_high: u8,
    fit_polyphony: usize,
    fit_drum_kit: bool,
    audition_player: Option<audio::player::Player>,
    file_mode: Option<FileMode>,
    file_path_input: String,
    audio_recording: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
    audio_record_seconds: f64,
    audio_record_start: Tick,
    audio_record_target: Option<TrackId>,
    song_name_input: String,
    markers_open: bool,
    theory_open: bool,
    theory_minor: bool,
    songs_open: bool,
    lessons_open: bool,
    lesson_kind: music_core::lesson::LessonKind,
    lesson_pack: Option<usize>,
    lesson_seed: u64,
    lesson_question: Option<music_core::lesson::Question>,
    lesson_score: (u32, u32),
    lesson_feedback: Option<bool>,
    printers: Vec<String>,
    printer_choice: Option<String>,
    printers_loaded: bool,
    print_dialog: bool,
    print_copies: u32,
    print_pages: String,
    monitor: Option<audio::monitor::Monitor>,
    monitor_input: Option<Option<String>>,
    monitor_channel: u8,
    stem_levels: Vec<(TrackId, Vec<f32>)>,
    marker_text: String,
    marker_selected: Option<Tick>,
    form_dialog: bool,
    join_section: String,
    join_total: usize,
    beat_dialog: bool,
    beat_track: Option<TrackId>,
    beat_start: Tick,
    pending_beat_start: Option<Tick>,
    beat_bar: Tick,
    beat_pattern: arranging::beat::BeatPattern,
    beat_clipboard: Option<Vec<u8>>,
    beat_bars: usize,
    waveform_zoom: f32,
    waveform_position: f32,
    launcher: arranging::loops::Launcher,
    jam_next: Option<jam::JamNext>,
    recent: Vec<std::path::PathBuf>,
    recent_file: std::path::PathBuf,
    midi_input: Option<midi_devices::devices::LiveInputs>,
    midi_map: midi_devices::map::MidiMap,
    loop_enabled: bool,
    loop_region: Option<(Tick, Tick)>,
    last_loop_wraps: u64,
    stem_sample_rate: u32,
    lesson: crate::kids::Lesson,
    perform_player: Option<audio::player::Player>,
    library_open: bool,
    library_path: String,
    library_query: String,
    library: Option<crate::library_view::Browser>,
    pads_open: bool,
    pad_state: Option<crate::pads::Pads>,
    pad_path: String,
    pad_player: Option<audio::player::Player>,
    pad_selected: Option<u8>,
    assistant: AssistantState,
    ai_pending: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    window_size: egui::Vec2,
    window_pos: Option<egui::Pos2>,
    available_update: Option<crate::update::Release>,
    update_download: Option<std::sync::mpsc::Receiver<Result<std::path::PathBuf, String>>>,
    downloaded_update: Option<std::path::PathBuf>,
    dictation_pending: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    waveform: Option<(TrackId, String, Vec<audio::waveform::Peak>)>,
    timeline_waveforms: std::collections::HashMap<TrackId, (String, Vec<audio::waveform::Peak>)>,
    waveform_pending: std::collections::HashMap<
        TrackId,
        (
            String,
            std::sync::mpsc::Receiver<Vec<audio::waveform::Peak>>,
        ),
    >,
    midi_ports: Option<(Vec<String>, Vec<String>)>,
    audio_inputs: Option<Vec<String>>,
    audio_outputs: Option<Vec<String>>,
    /// Audio system name and whether it has an output, for the header pill.
    audio_host: Option<(String, bool)>,
    settings: crate::settings::Settings,
    settings_open: bool,
    /// The menu bar: native on macOS, drawn in the window elsewhere.
    menu: MenuState,
    shortcuts_open: bool,
    about_open: bool,
    mixer_visible: bool,
    kids_last_activity: Option<std::time::Instant>,
    timeline_mode: bool,
    timeline_zoom: f32,
    pending_clip_edit: Option<crate::timeline::ClipEdit>,
    inspector_name_track: Option<TrackId>,
    inspector_name: String,
    device_profiles: Vec<midi_devices::profiles::LoadedProfile>,
    device_editor_open: bool,
    device_editor_profile: usize,
    device_editor_values: Vec<u8>,
    device_editor_voice: usize,
    device_editor_kit: usize,
    device_sysex: String,
    device_dump_path: String,
    groove_open: bool,
    groove_entries: Vec<(String, std::path::PathBuf)>,
    groove_selected: usize,
    groove_loaded: Option<arranging::groove::Groove>,
    groove_timing: f32,
    groove_accents: f32,
    help_open: bool,
    score: ScoreUi,
    pending_clip_add: Option<(TrackId, Tick, Tick)>,
    pending_reverse: Option<TrackId>,
    patch_open: bool,
    patch_track: Option<TrackId>,
    patch_draft: Option<project::song::Patch>,
    style: StyleState,
    generate_open: bool,
    generate_seed: u64,
    audio_chords: AudioChordsState,
    midi_chord: Option<String>,
    audio_style: AudioStyleState,
    master_fx_open: bool,
    master_fx_choice: usize,
    bus_fx_open: bool,
    bus_fx_choice: usize,
    bus_fx_plugin_choice: usize,
    midi_out: MidiOutState,
    marks_open: bool,
    mark_choice: usize,
    l6max: L6MaxState,
    d05: D05State,
    native_plugin_editor: Option<OpenNativeEditor>,
    takes_open: bool,
    xg: XgState,
    psr: PsrState,
    au_editor_open: bool,
    au_editor_track: Option<TrackId>,
    au_editor_slot: Option<u32>,
    au_effect: Option<AuEditorUnit>,
    au_params: Vec<(u32, String, f32, f32, f32, f32)>,
    vst3_params_open: bool,
    vst3_params_track: Option<TrackId>,
    vst3_params_slot: Option<u32>,
    vst3_effect: Option<plugin_host::Vst3Effect>,
    vst3_params: Vec<(u32, String, f64)>,
    lyrics_open: bool,
    lyric_text: String,
    lyric_selected: Option<Tick>,
    tempo_input: f64,
    automation_target: project::song::AutomationTarget,
    automation_plugin_slot: u32,
    automation_plugin_param: u32,
    sf2_path: String,
    sf2_bank: u16,
    sf2_program: u16,
    sf2_presets: Vec<(u16, u16, String)>,
    plugins_open: bool,
    plugin_search: String,
    plugin_filter: Option<plugins::Format>,
    plugin_scan: Option<Vec<plugins::FoundPlugin>>,
    plugin_scan_rx: Option<std::sync::mpsc::Receiver<(Vec<plugins::FoundPlugin>, usize)>>,
    plugin_scanning: bool,
    plugin_error: Option<String>,
    inspector_open: bool,
    inspector_track: Option<TrackId>,
    midi_map_open: bool,
    midi_learn: Option<midi_devices::map::Learn>,
    midi_learn_choice: usize,
    controller_open: bool,
    midi_cc_values: [u8; 128],
    midi_recent: std::collections::VecDeque<String>,
    midi_log: std::collections::VecDeque<(std::time::Instant, String)>,
    midi_monitor_open: bool,
    controller_action: usize,
    recording: Option<std::sync::mpsc::Receiver<Result<Song, String>>>,
    record_target: Option<TrackId>,
    record_seconds: f64,
    rename_scene: Option<usize>,
    rename_scene_input: String,
    slot_clipboard: Option<project::song::SlotClip>,
    rename_slot: Option<(usize, TrackId)>,
    loops_fill: Option<u32>,
    chosen_slot: Option<(usize, TrackId)>,
    slot_edit: Option<(usize, TrackId)>,
    drums: drums::DrumsUi,
    map_bank: usize,
    slot_roll: piano_roll::RollState,
    slot_note: Option<project::NoteId>,
    loops_quantise_bars: u32,
    jam_recording: bool,
    synth: arranging::SynthSpec,
    project_path: Option<PathBuf>,
    view: View,
    last_view: View,
    roll: RollState,
    playing: bool,
    position: Tick,
    icon: egui::TextureHandle,
    player: Option<audio::player::Player>,
    render_pending: Option<(
        std::sync::mpsc::Receiver<Result<RenderedSong, String>>,
        music_core::Tick,
    )>,
    autosave: Autosave,
    recovery: Option<std::path::PathBuf>,
    shown: bool,
    load_error: Option<String>,
}

/// The shortcut presses `handle_keys` reads once per frame, in the order it
/// consumes them.
#[derive(Clone, Copy, Debug)]
struct KeyPresses {
    undo: bool,
    redo: bool,
    save_as: bool,
    save: bool,
    open_keys: bool,
    play: bool,
    copy: bool,
    cut: bool,
    paste: bool,
    select_all: bool,
    delete: bool,
    score_typing: bool,
    copy_clips: bool,
    paste_clips: bool,
}

impl ShellApp {
    /// Shows a startup failure in the file window.
    pub fn report_startup_error(&mut self, error: impl Into<String>) {
        self.load_error = Some(error.into());
    }

    #[must_use]
    pub fn new(context: &Context, project: ProjectFile, project_path: Option<PathBuf>) -> Self {
        let settings = crate::settings::Settings::load(&crate::settings::Settings::path());
        let initial_lesson_kind = music_core::lesson::LessonKind::ALL
            .get(settings.lesson_kind)
            .copied()
            .unwrap_or(music_core::lesson::LessonKind::ChordQuality);
        let initial_score_follow = settings.score_follow;
        let initial_score_frames = settings.score_frames;
        let initial_timeline_zoom = settings.timeline_zoom;
        let initial_waveform_zoom = settings.waveform_zoom;
        let initial_score_zoom = settings.score_zoom;
        let recent_file = crate::settings::Settings::path().with_file_name("recent-projects.txt");
        let recent = project::recent::read(&recent_file);
        let midi_output_pref = settings.midi_output.clone();
        let initial_window = std::env::var("ARRANGER_WINDOW")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let initial_view = if settings.kids_mode {
            View::Perform
        } else {
            std::env::var("ARRANGER_VIEW")
                .ok()
                .and_then(|value| View::parse(&value))
                .or_else(|| View::parse(&settings.view))
                .unwrap_or(View::Arrange)
        };
        theme::set_light(settings.light_theme);
        theme::install(context);
        let icon = context.load_texture(
            "app-icon",
            egui::ColorImage::from_rgba_unmultiplied(
                [256, 256],
                include_bytes!("../assets/app-icon-256.rgba"),
            ),
            egui::TextureOptions::LINEAR,
        );
        let initial_key = project.song.key.map_or_else(
            || score::spelling::detect_key(&project.song).tonic,
            |key| key.tonic,
        );
        let initial_minor = project.song.key.is_some_and(|key| key.minor);
        let editor = match Editor::new(project) {
            Ok(editor) => editor,
            Err(_) => Editor::new(demo_project()).expect("the demo project is valid"),
        };
        let song_name_input = editor.song().name.clone();
        let initial_track = std::env::var("ARRANGER_TRACK")
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
            .and_then(|value| TrackId::new(value).ok())
            .filter(|id| editor.song().tracks.iter().any(|track| track.id == *id));
        let mut roll = RollState::default();
        roll.selected =
            initial_track.or_else(|| editor.song().tracks.first().map(|track| track.id));
        Self {
            editor,
            icon,
            player: None,
            render_pending: None,
            autosave: Autosave::new(std::time::Duration::from_secs(10)),
            recovery: project_path
                .as_ref()
                .filter(|path| persistence::recovery_available(path))
                .map(|path| persistence::autosave_path(path)),
            selected_note: None,
            selected_notes: Vec::new(),
            selected_chord: None,
            preview_player: None,
            selected_clip: None,
            selected_clips: Vec::new(),
            clip_clipboard: Vec::new(),
            chord_dialog: false,
            chord_dialog_target: 0,
            reharmonize_dialog: false,
            chord_input: String::from("Am F C G"),
            key: initial_key,
            status: None,
            doctor_dialog: false,
            doctor_proposals: Vec::new(),
            fit_proposals: Vec::new(),
            fit_low: 21,
            fit_high: 108,
            fit_polyphony: 16,
            fit_drum_kit: true,
            audition_player: None,
            file_mode: None,
            file_path_input: String::new(),
            audio_recording: None,
            audio_record_seconds: 3.0,
            audio_record_start: 0,
            audio_record_target: None,
            song_name_input,
            markers_open: initial_window == "markers",
            theory_open: initial_window == "theory",
            theory_minor: initial_minor,
            songs_open: initial_window == "songs",
            lessons_open: initial_window == "lessons",
            lesson_kind: initial_lesson_kind,
            lesson_pack: settings
                .lesson_pack
                .filter(|index| *index < music_core::lesson::PACKS.len()),
            lesson_seed: 1,
            lesson_question: None,
            lesson_score: (settings.lesson_correct, settings.lesson_wrong),
            lesson_feedback: None,
            printers: Vec::new(),
            printer_choice: None,
            printers_loaded: false,
            print_dialog: initial_window == "print",
            print_copies: 1,
            print_pages: String::new(),
            monitor: None,
            monitor_input: None,
            monitor_channel: 0,
            stem_levels: Vec::new(),
            marker_text: String::new(),
            marker_selected: None,
            form_dialog: false,
            join_section: String::new(),
            join_total: 2,
            beat_dialog: initial_window == "beat",
            beat_track: None,
            beat_start: 0,
            pending_beat_start: None,
            beat_bar: 3840,
            beat_pattern: arranging::beat::BeatPattern::default(),
            beat_clipboard: None,
            beat_bars: 1,
            waveform_zoom: initial_waveform_zoom,
            waveform_position: 0.0,
            launcher: arranging::loops::Launcher::default(),
            jam_next: None,
            recent_file,
            recent,
            midi_input: None,
            loop_enabled: false,
            loop_region: None,
            last_loop_wraps: 0,
            stem_sample_rate: audio::SAMPLE_RATE,
            lesson: crate::kids::Lesson::default(),
            perform_player: None,
            library_open: initial_window == "library",
            library_path: String::new(),
            library_query: String::new(),
            library: None,
            pads_open: initial_window == "pads",
            pad_state: None,
            pad_path: String::new(),
            pad_player: None,
            pad_selected: None,
            assistant: AssistantState {
                open: initial_window == "assistant",
                input: String::new(),
                log: Vec::new(),
                tool_rounds: 0,
                proposal: None,
            },
            ai_pending: None,
            window_size: egui::vec2(settings.window_width, settings.window_height),
            window_pos: match (settings.window_x, settings.window_y) {
                (Some(x), Some(y)) => Some(egui::pos2(x, y)),
                _ => None,
            },
            available_update: None,
            update_download: None,
            downloaded_update: None,
            dictation_pending: None,
            waveform: None,
            timeline_waveforms: std::collections::HashMap::new(),
            waveform_pending: std::collections::HashMap::new(),
            midi_ports: None,
            audio_inputs: None,
            audio_outputs: None,
            audio_host: None,
            midi_map: std::fs::read_to_string("lazy-arranger-midi-map.txt")
                .ok()
                .and_then(|text| midi_devices::map::MidiMap::from_text(&text).ok())
                .unwrap_or_default(),
            settings,
            settings_open: initial_window == "settings",
            menu: MenuState::default(),
            shortcuts_open: initial_window == "shortcuts",
            about_open: initial_window == "about",
            mixer_visible: true,
            kids_last_activity: None,
            // `ARRANGER_WINDOW=piano-roll` opens the docked editor, like v1 `--editor`.
            timeline_mode: initial_window != "piano-roll",
            timeline_zoom: initial_timeline_zoom,
            pending_clip_edit: None,
            inspector_name_track: None,
            inspector_name: String::new(),
            device_profiles: load_device_profiles(),
            device_editor_open: initial_window == "device",
            device_editor_profile: 0,
            device_editor_values: Vec::new(),
            device_editor_voice: 0,
            device_editor_kit: 0,
            device_sysex: String::new(),
            device_dump_path: String::from("dump.syx"),
            groove_open: initial_window == "groove",
            groove_entries: Vec::new(),
            groove_selected: 0,
            groove_loaded: None,
            groove_timing: 1.0,
            groove_accents: 1.0,
            help_open: initial_window == "help",
            score: ScoreUi {
                entry: std::env::var("ARRANGER_SCORE_ENTRY").is_ok(),
                octave: 4,
                note_length: music_core::PPQ,
                dotted: false,
                cursor: std::env::var("ARRANGER_SCORE_CURSOR")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                page: 0,
                zoom: initial_score_zoom,
                grand: std::env::var("ARRANGER_GRAND").is_ok_and(|value| value == "1"),
                one_staff: false,
                preview: std::env::var("ARRANGER_SCORE_EDIT").is_err()
                    && std::env::var("ARRANGER_SCORE_ENTRY").is_err(),
                // v1 opens the score on all tracks.
                all_tracks: std::env::var("ARRANGER_SCORE_ALL").map_or(true, |value| value != "0"),
                follow: initial_score_follow,
                full_frames: std::env::var("ARRANGER_FULL_FRAMES").is_ok() || initial_score_frames,
                full_hidden: std::collections::HashSet::new(),
                full_cache: None,
                pdf_path: crate::settings::Settings::data_dir()
                    .join("score.pdf")
                    .display()
                    .to_string(),
                track: None,
                state: crate::score_view::ScoreState::default(),
            },
            pending_clip_add: None,
            pending_reverse: None,
            patch_open: std::env::var("ARRANGER_PATCH").is_ok(),
            patch_track: None,
            patch_draft: None,
            style: StyleState {
                clips_open: std::env::var("ARRANGER_STYLE").is_ok(),
                name: String::from("Main A"),
                chord: String::from("C"),
                length: 3840,
                from_markers: true,
                path: crate::settings::Settings::data_dir()
                    .join("styles")
                    .join("clips.sty")
                    .display()
                    .to_string(),
                sff1: false,
                draft: None,
                rules_open: false,
                preview: None,
                link_open: std::env::var("ARRANGER_STYLE_LINK").is_ok(),
                link_url: String::new(),
                link_pending: None,
                file_open: std::env::var("ARRANGER_STYLE").is_ok(),
                file_path: std::env::var("ARRANGER_STYLE_FILE").unwrap_or_default(),
                import: None,
                section: 0,
                file_loop: true,
                auto_load: std::env::var("ARRANGER_STYLE_FILE").is_ok(),
                queued_section: None,
                queued_chord: None,
                fill_return: None,
                ending: false,
                held: [false; 128],
                last_chord: String::new(),
                queue_at: 0.0,
            },
            generate_open: std::env::var("ARRANGER_GENERATE").is_ok(),
            generate_seed: 1,
            audio_chords: AudioChordsState {
                open: std::env::var("ARRANGER_CHORDS").is_ok(),
                path: std::env::var("ARRANGER_CHORDS_FILE").unwrap_or_default(),
                chords: Vec::new(),
                pending: None,
                auto: std::env::var("ARRANGER_CHORDS_FILE").is_ok(),
            },
            midi_chord: None,
            audio_style: AudioStyleState {
                open: std::env::var("ARRANGER_AUDIO_STYLE").is_ok(),
                path: String::new(),
                bpm: 120.0,
                chord: String::from("auto"),
                section: String::from("Main A"),
                split: false,
                section_bars: 0,
                drums: false,
                detect_tempo: false,
                out: String::from("audio-style.sty"),
                sff1: false,
            },
            master_fx_open: std::env::var("ARRANGER_MASTER_FX").is_ok(),
            master_fx_choice: 0,
            bus_fx_open: std::env::var("ARRANGER_BUS_FX").is_ok(),
            bus_fx_choice: 0,
            bus_fx_plugin_choice: 0,
            midi_out: MidiOutState {
                open: std::env::var("ARRANGER_MIDI_OUT").is_ok(),
                port: midi_output_pref,
                conn: None,
                channel: 1,
                program: 0,
                note: 60,
                velocity: 100,
                cc: 7,
                cc_value: 100,
                bank_msb: 0,
                bank_lsb: 0,
            },
            marks_open: std::env::var("ARRANGER_MARKS").is_ok(),
            mark_choice: 0,
            l6max: L6MaxState {
                device_open: std::env::var("ARRANGER_L6DEVICE").is_ok(),
                device: crate::l6max_device::load(
                    &crate::settings::Settings::data_dir().join(crate::l6max_device::STATE_PATH),
                ),
                identity: String::new(),
                id_reply: None,
                folder: String::new(),
                plan: None,
                error: None,
                selected: Vec::new(),
                mixer_open: std::env::var("ARRANGER_L6MIX").is_ok(),
                compact: false,
                scenes: load_l6_scenes(),
            },
            d05: D05State {
                open: std::env::var("ARRANGER_D05").is_ok(),
                editor: crate::d05_editor::D05EditorState::default(),
                reply: None,
            },
            native_plugin_editor: None,
            takes_open: std::env::var("ARRANGER_TAKES").is_ok(),
            xg: XgState {
                open: std::env::var("ARRANGER_XG").is_ok(),
                parts: [[100, 64, 40, 0]; 16],
            },
            psr: PsrState {
                open: std::env::var("ARRANGER_PSR").is_ok(),
                channel: 1,
                bank_msb: 0,
                bank_lsb: 0,
                program: 0,
                volume: 100,
                pan: 64,
                reverb: 40,
                chorus: 0,
                regs: load_psr_regs(),
            },
            au_editor_open: false,
            au_editor_track: None,
            au_editor_slot: None,
            au_effect: None,
            au_params: Vec::new(),
            vst3_params_open: false,
            vst3_params_track: None,
            vst3_params_slot: None,
            vst3_effect: None,
            vst3_params: Vec::new(),
            lyrics_open: initial_window == "lyrics",
            lyric_text: String::new(),
            lyric_selected: None,
            tempo_input: 120.0,
            automation_target: project::song::AutomationTarget::Volume,
            automation_plugin_slot: 1,
            automation_plugin_param: 0,
            sf2_path: String::new(),
            sf2_bank: 0,
            sf2_program: 0,
            sf2_presets: Vec::new(),
            plugins_open: initial_window == "plugins",
            plugin_search: String::new(),
            plugin_filter: None,
            plugin_scan: None,
            plugin_scan_rx: None,
            plugin_scanning: false,
            plugin_error: None,
            inspector_open: initial_window == "inspector",
            inspector_track: None,
            midi_map_open: initial_window == "midi-map" || initial_window == "midi_map",
            midi_learn: None,
            midi_learn_choice: 0,
            controller_open: initial_window == "controllers",
            midi_cc_values: [0; 128],
            midi_recent: std::collections::VecDeque::new(),
            midi_log: std::collections::VecDeque::new(),
            midi_monitor_open: std::env::var("ARRANGER_MIDIMON").is_ok(),
            controller_action: 0,
            recording: None,
            record_target: None,
            record_seconds: 4.0,
            rename_scene: None,
            rename_scene_input: String::new(),
            slot_clipboard: None,
            rename_slot: None,
            loops_fill: None,
            chosen_slot: None,
            slot_edit: None,
            drums: drums::DrumsUi::default(),
            map_bank: 0,
            slot_roll: piano_roll::RollState::default(),
            slot_note: None,
            loops_quantise_bars: 1,
            jam_recording: false,
            synth: arranging::SynthSpec::default(),
            project_path,
            view: initial_view,
            last_view: initial_view,
            roll,
            playing: false,
            position: 0,
            shown: false,
            load_error: None,
        }
    }

    fn song(&self) -> &Song {
        self.editor.song()
    }

    /// Applies a click from the piano roll: select a note, or insert one.
    fn edit_click(&mut self, click: piano_roll::RollClick) {
        if let Some((track, clip)) = click.clip {
            self.roll.selected = Some(track);
            self.selected_clip =
                apply_clip_click(&mut self.selected_clips, (track, clip), click.shift);
            self.selected_note = None;
            return;
        }
        if let Some((track, note)) = click.note {
            self.roll.selected = Some(track);
            self.selected_note = Some((track, note));
            self.selected_clip = None;
            self.selected_clips.clear();
            return;
        }
        let Some(track) = self
            .score
            .track
            .or(self.roll.selected)
            .or_else(|| self.song().tracks.first().map(|track| track.id))
        else {
            return;
        };
        let Ok(pitch) = music_core::Pitch::new(click.pitch) else {
            return;
        };
        let start = if self.roll.snap {
            snap(click.tick, PPQ / 4)
        } else {
            click.tick.max(0)
        };
        let _ = self.editor.insert(track, start, pitch);
        self.selected_note = None;
        self.selected_clip = None;
        self.selected_clips.clear();
    }

    /// Undo, redo, delete and save, from the keyboard.
    fn handle_keys(&mut self, context: &Context) {
        if context.input_mut(|input| {
            input.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::ALT,
                egui::Key::K,
            )
        }) {
            self.toggle_kids_mode();
        }
        let undo = consume_binding(context, &self.settings.undo_keys, "cmd+z");
        let redo = consume_binding(context, &self.settings.redo_keys, "cmd+shift+z");
        let save_as = consume_binding(context, &self.settings.save_as_keys, "cmd+shift+s");
        let save = !save_as && consume_binding(context, &self.settings.save_keys, "cmd+s");
        let open_keys = consume_binding(context, &self.settings.open_keys, "cmd+o");
        let play = consume_binding(context, &self.settings.play_keys, "cmd+p");
        let copy =
            context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::C));
        let cut =
            context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::X));
        let paste =
            context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::V));
        let select_all =
            context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::A));
        let delete = context.input(|input| {
            input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace)
        });
        let score_typing =
            score_typing(self.view, self.score.entry, context.wants_keyboard_input());
        let copy_clips = !score_typing
            && context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::C));
        let paste_clips = !score_typing
            && context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::V));
        let keys = KeyPresses {
            undo,
            redo,
            save_as,
            save,
            open_keys,
            play,
            copy,
            cut,
            paste,
            select_all,
            delete,
            score_typing,
            copy_clips,
            paste_clips,
        };
        self.handle_edit_keys(context, keys);
        self.handle_chord_clipboard_keys(context, keys);
        self.handle_clipboard_keys(keys);
        self.handle_arrange_keys(context, keys);
        self.handle_file_keys(keys);
        self.handle_score_entry_keys(context);
    }

    /// Help, play, undo, redo and select-all shortcuts.
    fn handle_edit_keys(&mut self, context: &Context, keys: KeyPresses) {
        let KeyPresses {
            undo,
            redo,
            play,
            select_all,
            ..
        } = keys;
        if context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F1)) {
            self.shortcuts_open = true;
        }
        if play {
            self.toggle_playback();
        }
        if undo {
            self.editor.undo();
            self.selected_note = None;
            self.selected_clip = None;
            self.selected_clips.clear();
        }
        if redo {
            self.editor.redo();
            self.selected_note = None;
            self.selected_clip = None;
            self.selected_clips.clear();
        }
        if select_all
            && let Some(track) = self
                .roll
                .selected
                .or_else(|| self.song().tracks.first().map(|track| track.id))
        {
            self.selected_notes = self
                .song()
                .tracks
                .iter()
                .find(|entry| entry.id == track)
                .map(|entry| entry.notes.iter().map(|note| (track, note.id)).collect())
                .unwrap_or_default();
            self.status = Some(format!("selected {} note(s)", self.selected_notes.len()));
        }
    }

    /// Copy, cut and paste of the selected chord.
    fn handle_chord_clipboard_keys(&mut self, context: &Context, keys: KeyPresses) {
        let KeyPresses {
            copy, cut, paste, ..
        } = keys;
        if let Some(tick) = self.selected_chord {
            if copy || cut {
                let bar = self.song().time_signature.bar_ticks().max(1);
                let end = self
                    .song()
                    .chords
                    .iter()
                    .map(|event| event.tick)
                    .find(|other| *other > tick)
                    .unwrap_or(tick + bar);
                self.editor.copy_chords(tick, end);
                if cut {
                    match self.editor.remove_chords(tick, end) {
                        Ok(removed) => {
                            self.status = Some(format!("cut {removed} chord(s)"));
                            self.selected_chord = None;
                        }
                        Err(error) => self.status = Some(error),
                    }
                } else {
                    self.status = Some(format!("copied chord(s) from {tick}"));
                }
            }
            if paste {
                let at = self.roll.hover.unwrap_or(self.position).max(0);
                let bar = self.song().time_signature.bar_ticks().max(1);
                let spacing = self
                    .song()
                    .chords
                    .iter()
                    .map(|event| event.tick)
                    .find(|other| *other > tick)
                    .map(|end| end - tick)
                    .unwrap_or(bar);
                let repeat = context
                    .input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, egui::Key::V));
                let count = if repeat { 4 } else { 1 };
                let result = if repeat {
                    self.editor.paste_chords_repeated(at, count, spacing)
                } else {
                    self.editor.paste_chords(at)
                };
                match result {
                    Ok(0) => self.status = Some(String::from("clipboard has no chords")),
                    Ok(copied) => {
                        self.status = Some(format!(
                            "pasted {copied} chord(s) at {at}{}",
                            if repeat { " (x4)" } else { "" }
                        ))
                    }
                    Err(error) => self.status = Some(error),
                }
            }
        }
    }

    /// Copy, cut, paste and delete of notes and clips.
    fn handle_clipboard_keys(&mut self, keys: KeyPresses) {
        let KeyPresses {
            copy,
            cut,
            paste,
            delete,
            copy_clips,
            paste_clips,
            ..
        } = keys;
        if !self.selected_notes.is_empty() && (copy || cut) {
            let track = self.selected_notes[0].0;
            let ids: Vec<project::NoteId> = self
                .selected_notes
                .iter()
                .filter(|(entry, _)| *entry == track)
                .map(|(_, note)| *note)
                .collect();
            if copy {
                self.editor.copy_notes(track, &ids);
                self.status = Some(format!("copied {} note(s)", ids.len()));
            }
            if cut {
                match self.editor.cut_notes(track, &ids) {
                    Ok(removed) => {
                        self.status = Some(format!("cut {removed} note(s)"));
                        self.selected_notes.clear();
                    }
                    Err(error) => self.status = Some(error),
                }
            }
        } else if let Some((track, note)) = self.selected_note {
            if copy {
                self.editor.copy_note(track, note);
                self.status = Some(String::from("note copied"));
            }
            if cut && self.editor.cut_note(track, note) {
                self.selected_note = None;
                self.status = Some(String::from("note cut"));
            }
        }
        if paste {
            let track = self
                .roll
                .selected
                .or_else(|| self.selected_note.map(|(track, _)| track))
                .or_else(|| self.song().tracks.first().map(|track| track.id));
            if let Some(track) = track {
                let (at, from_mouse) = match self.roll.hover {
                    Some(tick) => (snap(tick, PPQ), true),
                    None => (snap(self.position, PPQ), false),
                };
                match self.editor.paste_notes(track, at) {
                    Ok(0) => self.status = Some(String::from("clipboard is empty")),
                    Ok(count) => {
                        self.status = Some(format!(
                            "pasted {count} note(s) at tick {at}{}",
                            if from_mouse { " (mouse)" } else { "" }
                        ));
                    }
                    Err(error) => self.status = Some(error),
                }
            }
        }
        if copy_clips {
            let mut selected = self.selected_clips.clone();
            if let Some(clip) = self.selected_clip {
                selected.push(clip);
            }
            selected.sort_unstable();
            selected.dedup();
            self.clip_clipboard = selected
                .into_iter()
                .filter_map(|(track, clip)| {
                    self.song()
                        .tracks
                        .iter()
                        .find(|entry| entry.id == track)
                        .and_then(|entry| entry.clips.iter().find(|entry| entry.id == clip))
                        .cloned()
                        .map(|clip| (track, clip))
                })
                .collect();
            if !self.clip_clipboard.is_empty() {
                self.status = Some(format!("copied {} clip(s)", self.clip_clipboard.len()));
            }
        }
        if paste_clips && !self.clip_clipboard.is_empty() {
            let earliest = self
                .clip_clipboard
                .iter()
                .map(|(_, clip)| clip.start)
                .min()
                .unwrap_or(0);
            let mut pasted = 0;
            for (track, clip) in self.clip_clipboard.clone() {
                let start = self.position + clip.start.saturating_sub(earliest);
                if self.editor.add_clip(track, start, clip.length).is_ok() {
                    pasted += 1;
                }
            }
            self.status = Some(format!("pasted {pasted} clip(s)"));
        }
        if delete {
            if !self.selected_notes.is_empty() {
                let mut tracks: Vec<TrackId> = self
                    .selected_notes
                    .iter()
                    .map(|(track, _)| *track)
                    .collect();
                tracks.sort_by_key(|track| track.get());
                tracks.dedup();
                for track in tracks {
                    let ids: Vec<project::NoteId> = self
                        .selected_notes
                        .iter()
                        .filter(|(entry, _)| *entry == track)
                        .map(|(_, note)| *note)
                        .collect();
                    let _ = self.editor.delete_notes(track, &ids);
                }
                self.status = Some(format!("deleted {} note(s)", self.selected_notes.len()));
                self.selected_notes.clear();
            } else if !self.selected_clips.is_empty() {
                for (track, clip) in std::mem::take(&mut self.selected_clips) {
                    let _ = self.editor.remove_clip(track, clip);
                }
                self.selected_clip = None;
            } else if let Some((track, clip)) = self.selected_clip.take() {
                let _ = self.editor.remove_clip(track, clip);
            } else if let Some((track, note)) = self.selected_note.take() {
                let _ = self.editor.delete(track, note);
            }
        }
    }

    /// Clip nudging and the single-letter arrange shortcuts.
    fn handle_arrange_keys(&mut self, context: &Context, keys: KeyPresses) {
        let KeyPresses { score_typing, .. } = keys;
        if !score_typing
            && context.input(|input| input.key_pressed(egui::Key::N))
            && self.selected_clip.is_none()
            && self.selected_clips.is_empty()
            && let Some(track) = self
                .roll
                .selected
                .or_else(|| self.song().tracks.first().map(|track| track.id))
        {
            let bar = self.song().time_signature.bar_ticks().max(1);
            let start = snap(self.position, bar);
            let next = self
                .song()
                .tracks
                .iter()
                .find(|candidate| candidate.id == track)
                .and_then(|candidate| candidate.clips.iter().map(|clip| clip.id.get()).max())
                .unwrap_or(0)
                .saturating_add(1);
            if let Ok(id) = project::ClipId::new(next) {
                let clip = project::song::Clip {
                    id,
                    name: format!("Clip {next}"),
                    start,
                    length: bar,
                };
                let _ = self.editor.insert_clip(track, clip);
                self.selected_clip = Some((track, id));
                self.selected_clips = vec![(track, id)];
            }
        }
        let targets: Vec<(TrackId, project::ClipId)> = if self.selected_clips.is_empty() {
            self.selected_clip.into_iter().collect()
        } else {
            self.selected_clips.clone()
        };
        let bar = self.song().time_signature.bar_ticks().max(1);
        let right = context.input(|input| input.key_pressed(egui::Key::ArrowRight));
        let left = context.input(|input| input.key_pressed(egui::Key::ArrowLeft));
        let shift = context.input(|input| input.modifiers.shift);
        if right || left {
            for (track, clip) in &targets {
                let Some(found) = self
                    .song()
                    .tracks
                    .iter()
                    .find(|candidate| candidate.id == *track)
                    .and_then(|candidate| candidate.clips.iter().find(|cl| cl.id == *clip))
                    .cloned()
                else {
                    continue;
                };
                if shift {
                    let length = if right {
                        found.length + bar
                    } else {
                        (found.length - bar).max(1)
                    };
                    let _ = self.editor.resize_clip(*track, *clip, length);
                } else {
                    let target = if right {
                        found.start + bar
                    } else {
                        (found.start - bar).max(0)
                    };
                    let _ = self.editor.move_clip(*track, *clip, target);
                }
            }
        }
        if let Some((track, clip)) = self.selected_clip
            && !score_typing
            && context.input(|input| input.key_pressed(egui::Key::J))
            && let Some(next) = self
                .song()
                .tracks
                .iter()
                .find(|candidate| candidate.id == track)
                .and_then(|candidate| {
                    candidate
                        .clips
                        .iter()
                        .find(|cl| {
                            cl.start
                                > candidate
                                    .clips
                                    .iter()
                                    .find(|sel| sel.id == clip)
                                    .map_or(i64::MAX, |sel| sel.start)
                        })
                        .map(|cl| cl.id)
                })
        {
            let _ = self.editor.join_clips(track, clip, next);
            self.selected_clip = Some((track, clip));
        }
        if !score_typing
            && context.input(|input| input.key_pressed(egui::Key::G))
            && let Some(source) = self.roll.selected
        {
            match self
                .editor
                .apply_groove(source, arranging::groove::Amounts::default())
            {
                Ok(0) => self.status = Some(String::from("no other track to groove")),
                Ok(count) => self.status = Some(format!("groove applied to {count} track(s)")),
                Err(error) => self.status = Some(error),
            }
        }
        if !score_typing && context.input(|input| input.key_pressed(egui::Key::T)) {
            let bar = self.song().time_signature.bar_ticks().max(1);
            let join = snap(self.position, bar).max(bar);
            match self
                .editor
                .apply_transition(join, arranging::forms::Transition::Automatic)
            {
                Ok(0) => self.status = Some(String::from("nothing to add at that join")),
                Ok(added) => self.status = Some(format!("transition: {added} note(s)")),
                Err(error) => self.status = Some(error),
            }
        }
        if context.input(|input| input.key_pressed(egui::Key::B))
            && let Some(track) = self
                .roll
                .selected
                .or_else(|| self.song().tracks.first().map(|track| track.id))
        {
            let bar = self.song().time_signature.bar_ticks().max(1);
            let start = snap(self.position, bar);
            if let Some((bar, pattern)) = self.editor.beat_pattern(track, start) {
                self.beat_track = Some(track);
                self.beat_start = start;
                self.beat_bar = bar;
                self.beat_pattern = pattern;
                self.beat_dialog = true;
            }
        }
        if !score_typing && context.input(|input| input.key_pressed(egui::Key::Y)) {
            self.form_dialog = true;
        }
        if !score_typing && context.input(|input| input.key_pressed(egui::Key::D)) {
            self.doctor_proposals = self.editor.proposals();
            self.doctor_proposals
                .extend(self.editor.fit_proposals(self.synth));
            self.doctor_dialog = true;
        }
        if !score_typing && context.input(|input| input.key_pressed(egui::Key::F)) {
            match self.editor.find_chords() {
                Ok(0) => self.status = Some(String::from("no chords found")),
                Ok(count) => self.status = Some(format!("found {count} chord(s)")),
                Err(error) => self.status = Some(error),
            }
        }
        if !score_typing && context.input(|input| input.key_pressed(egui::Key::K)) {
            let bar = self.song().time_signature.bar_ticks().max(1);
            self.chord_dialog_target = snap(self.position, bar);
            self.chord_dialog = true;
        }
        if !score_typing
            && context.input(|input| input.key_pressed(egui::Key::H))
            && let Some(found) = self
                .song()
                .chords
                .iter()
                .filter(|event| event.tick <= self.position)
                .max_by_key(|event| event.tick)
        {
            self.chord_dialog_target = found.tick;
            self.reharmonize_dialog = true;
        }
    }

    /// Open, save and save-as shortcuts.
    fn handle_file_keys(&mut self, keys: KeyPresses) {
        let KeyPresses {
            save_as,
            save,
            open_keys,
            ..
        } = keys;
        if open_keys {
            self.choose_file(FileMode::Open);
        }
        if save_as {
            self.choose_file(FileMode::SaveAs);
        }
        if save {
            match self.project_path.clone() {
                Some(path) => self.save_to(path),
                None => self.choose_file(FileMode::SaveAs),
            }
        }
    }

    /// Note entry from the computer keyboard in the score view.
    fn handle_score_entry_keys(&mut self, context: &Context) {
        if self.view == View::Score
            && self.score.entry
            && !context.wants_keyboard_input()
            && !context.input(|input| input.modifiers.command || input.modifiers.alt)
        {
            let track = self
                .score
                .track
                .or(self.roll.selected)
                .or_else(|| self.song().tracks.first().map(|track| track.id));
            if let Some(track) = track {
                for key in [
                    egui::Key::A,
                    egui::Key::W,
                    egui::Key::S,
                    egui::Key::E,
                    egui::Key::D,
                    egui::Key::F,
                    egui::Key::T,
                    egui::Key::G,
                    egui::Key::Y,
                    egui::Key::H,
                    egui::Key::U,
                    egui::Key::J,
                ] {
                    if context.input(|input| input.key_pressed(key))
                        && let Some(pitch) = score_key_pitch(key, self.score.octave)
                        && let Ok(pitch) = music_core::Pitch::new(pitch)
                    {
                        match self.editor.insert_length(
                            track,
                            self.position.max(0),
                            pitch,
                            self.score.note_length.max(1),
                        ) {
                            Ok(_) => {
                                self.status =
                                    Some(format!("inserted {pitch} at {}", self.position));
                            }
                            Err(error) => self.status = Some(error.to_string()),
                        }
                    }
                }
            }
        }
    }

    /// Per-frame upkeep before drawing: style queue, background jobs and pending edits.
    fn poll_background(&mut self, context: &Context) {
        if (self.style.queued_section.is_some() || self.style.queued_chord.is_some())
            && let Some(player) = &self.style.preview
            && (!player.is_playing() || player.seconds() >= self.style.queue_at)
        {
            if let Some(index) = self.style.queued_section.take() {
                self.style.section = index;
            }
            if let Some(chord) = self.style.queued_chord.take() {
                self.style.chord = chord;
            }
            self.audition_style_section();
        }
        if let Some(return_index) = self.style.fill_return
            && self.style.queued_section.is_none()
            && let Some(player) = &self.style.preview
            && !player.is_playing()
        {
            self.style.fill_return = None;
            self.style.section = return_index;
            self.audition_style_section();
        }
        if self.style.file_loop
            && !self.style.ending
            && self.style.fill_return.is_none()
            && self.style.queued_section.is_none()
            && self.style.queued_chord.is_none()
            && let Some(player) = &self.style.preview
            && !player.is_playing()
        {
            player.play();
        }
        self.poll_midi();
        self.poll_ai();
        if let Some(receiver) = &self.style.link_pending {
            match receiver.try_recv() {
                Ok(Ok(path)) => {
                    self.audio_style.path = path.display().to_string();
                    self.audio_style.open = true;
                    self.status = Some(String::from("audio downloaded; style build is ready"));
                    self.style.link_pending = None;
                }
                Ok(Err(error)) => {
                    self.status = Some(format!("link: {error}"));
                    self.style.link_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.style.link_pending = None;
                }
            }
        }
        if let Some(receiver) = &self.audio_chords.pending {
            match receiver.try_recv() {
                Ok(Ok(chords)) => {
                    self.status = Some(format!("recognised {} chord(s)", chords.len()));
                    self.audio_chords.chords = chords;
                    self.audio_chords.pending = None;
                }
                Ok(Err(error)) => {
                    self.status = Some(error);
                    self.audio_chords.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.audio_chords.pending = None;
                }
            }
        }
        if let Some(receiver) = &self.dictation_pending {
            match receiver.try_recv() {
                Ok(Ok(text)) => {
                    self.assistant.input = text.clone();
                    self.status = Some(String::from("dictation ready"));
                    self.dictation_pending = None;
                    self.assistant.open = true;
                }
                Ok(Err(error)) => {
                    self.status = Some(format!("dictation: {error}"));
                    self.dictation_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.dictation_pending = None;
                }
            }
        }
        if let Some(receiver) = &self.update_download {
            match receiver.try_recv() {
                Ok(Ok(path)) => {
                    self.status = Some(format!("downloaded {}", path.display()));
                    self.downloaded_update = Some(path);
                    self.update_download = None;
                }
                Ok(Err(error)) => {
                    self.status = Some(format!("download failed: {error}"));
                    self.update_download = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.update_download = None;
                }
            }
        }
        if let Some(start) = self.pending_beat_start.take() {
            self.load_beat_from(start);
        }
        if let Some(edit) = self.pending_clip_edit.take() {
            let result = match edit.kind {
                crate::timeline::ClipEditKind::Move => {
                    self.editor.move_clip(edit.track, edit.clip, edit.start)
                }
                crate::timeline::ClipEditKind::Resize => {
                    self.editor.resize_clip(edit.track, edit.clip, edit.length)
                }
            };
            match result {
                Ok(()) => {
                    self.selected_clip = Some((edit.track, edit.clip));
                    self.status = Some(String::from("clip moved"));
                }
                Err(error) => self.status = Some(error.to_string()),
            }
        }
        self.refresh_waveform(context);
        if self.settings.kids_mode && self.view.index() > 1 {
            self.view = View::Perform;
        }
        if self.settings.kids_mode && self.view == View::Perform {
            let active =
                context.input(|input| !input.keys_down.is_empty() || input.pointer.any_down());
            if active {
                self.kids_last_activity = Some(std::time::Instant::now());
                if !self.playing {
                    self.toggle_playback();
                }
            } else if self.playing
                && self.kids_last_activity.is_some_and(|last| {
                    last.elapsed().as_secs_f32() >= self.settings.kids_pause_seconds
                })
            {
                self.toggle_playback();
                self.kids_last_activity = None;
            }
            if !self.loop_enabled {
                let end = self.song().end_tick().max(1);
                self.loop_region = Some((0, end));
                self.loop_enabled = true;
                self.sync_player_loop();
            }
        }
        if self.view == View::Drums
            && (self.beat_track.is_none()
                || (self.roll.selected.is_some() && self.roll.selected != self.beat_track))
        {
            self.load_beat();
        }
    }

    /// Dialogs, floating windows and the side and bottom panels.
    fn draw_windows(&mut self, context: &Context) {
        self.run_autosave();
        self.recovery_dialog(context);
        self.chord_dialog(context);
        self.reharmonize_dialog(context);
        self.doctor_dialog(context);
        self.form_dialog(context);
        self.beat_dialog(context);
        self.advance(context);
        self.menu_bar(context);
        self.top_bar(context);
        self.windows(context);
        self.inspector_window(context);
        self.controller_window(context);
        self.lyrics_window(context);
        self.markers_window(context);
        self.theory_window(context);
        self.lessons_window(context);
        self.songs_window(context);
        self.help_window(context);
        self.plugins_window(context);
        self.patch_window(context);
        self.style_clips_window(context);
        self.style_file_window(context);
        self.style_link_window(context);
        self.audio_chords_window(context);
        self.generate_window(context);
        self.l6max_mixer_window(context);
        self.l6max_device_window(context);
        self.d05_editor_window(context);
        self.marks_window(context);
        self.audio_style_window(context);
        self.style_rules_window(context);
        self.master_fx_window(context);
        self.bus_fx_window(context);
        self.psr_window(context);
        self.xg_window(context);
        self.takes_window(context);
        self.midi_out_window(context);
        self.midi_monitor_window(context);
        self.poll_native_plugin_editor();
        self.au_parameters_window(context);
        self.vst3_parameters_window(context);
        self.print_window(context);
        self.file_window(context);
        self.midi_map_window(context);
        self.assistant_window(context);
        self.pads_window(context);
        self.library_window(context);
        self.status_bar(context);
        self.right_inspector(context);
        self.bottom_mixer(context);
        self.sidebar(context);
    }

    fn sync_monitor(&mut self) {
        let wanted = self.settings.audio_input.clone();
        if self.settings.monitor
            && (self.monitor_input.as_ref() != Some(&wanted)
                || self.monitor_channel != self.settings.audio_input_channel)
        {
            self.monitor = None;
            self.monitor_input = None;
            let channel = (self.settings.audio_input_channel > 0)
                .then_some(usize::from(self.settings.audio_input_channel - 1));
            match audio::monitor::start_on_channel(wanted.as_deref(), channel, 0.7) {
                Ok(monitor) => {
                    self.monitor = Some(monitor);
                    self.monitor_input = Some(wanted);
                    self.monitor_channel = self.settings.audio_input_channel;
                }
                Err(error) => {
                    self.settings.monitor = false;
                    self.status = Some(format!("monitor: {error}"));
                }
            }
        } else if !self.settings.monitor {
            self.monitor = None;
            self.monitor_input = None;
        }
    }

    fn seek_to(&mut self, tick: Tick) {
        let tick = tick.max(0);
        if self.playing
            && let Some(player) = &self.player
        {
            let _ = player.seek_seconds(self.song().tempo_map.seconds_at(tick));
        }
        self.position = tick;
    }

    fn save_to(&mut self, path: std::path::PathBuf) {
        if let Err(error) = self.editor.save(&path) {
            self.load_error = Some(error);
            return;
        }
        let _ = std::fs::remove_file(persistence::autosave_path(&path));
        self.recent = project::recent::record(&self.recent_file, &path)
            .unwrap_or_else(|_| self.recent.clone());
        self.project_path = Some(path.clone());
        self.settings.last_project = Some(path.display().to_string());
        let _ = self.settings.save(&crate::settings::Settings::path());
        self.recovery = None;
        self.load_error = None;
        self.status = Some(format!("saved {}", path.display()));
    }

    fn file_window(&mut self, context: &egui::Context) {
        let Some(mode) = self.file_mode else {
            return;
        };
        let mut open = true;
        let mut action = false;
        let mut browse = false;
        egui::Window::new(match mode {
            FileMode::Open => "Open project",
            FileMode::SaveAs => "Save as",
            FileMode::ImportAudio => "Import audio",
            FileMode::ExportMidi => "Export MIDI",
            FileMode::ExportMusicXml => "Export MusicXML",
            FileMode::ExportWav => "Export WAV",
            FileMode::ExportStems => "Export stems",
            FileMode::ImportL6max => "Import L6max takes",
        })
        .collapsible(false)
        .open(&mut open)
        .default_width(420.0)
        .show(context, |ui| {
            ui.label(match mode {
                FileMode::ImportAudio => "Path (WAV, FLAC, MP3, M4A, AIFF, OGG or video)",
                FileMode::ExportMidi => "Path (.mid)",
                FileMode::ExportMusicXml => "Path (.musicxml)",
                FileMode::ExportWav => "Path (.wav)",
                FileMode::ExportStems => "Folder",
                FileMode::ImportL6max => "Take folder",
                _ => "Path (.arranger JSON, .mid, .midi or .musicxml)",
            });
            ui.add(egui::TextEdit::singleline(&mut self.file_path_input).desired_width(400.0));
            ui.horizontal(|ui| {
                let label = match mode {
                    FileMode::Open => "Open",
                    FileMode::SaveAs => "Save",
                    FileMode::ImportAudio | FileMode::ImportL6max => "Import",
                    FileMode::ExportMidi
                    | FileMode::ExportMusicXml
                    | FileMode::ExportWav
                    | FileMode::ExportStems => "Export",
                };
                if ui.button(label).clicked() {
                    action = true;
                }
                if ui.button("Browse…").clicked() {
                    browse = true;
                }
                if ui.button("Cancel").clicked() {
                    self.file_mode = None;
                }
            });
        });
        if browse && let Some(path) = files::dialog(mode, &self.file_path_input) {
            self.file_path_input = path.display().to_string();
        }
        if action {
            let path = std::path::PathBuf::from(self.file_path_input.trim());
            if path.as_os_str().is_empty() {
                self.load_error = Some(String::from("enter a file path"));
            } else {
                self.run_file_action(mode, path);
            }
        }
        if !open {
            self.file_mode = None;
        }
    }

    /// Opens, saves, imports or exports `path`, as `mode` says.
    pub(super) fn run_file_action(&mut self, mode: FileMode, path: std::path::PathBuf) {
        if mode == FileMode::Open {
            self.open(&path);
            if self.load_error.is_none() {
                self.file_mode = None;
            }
        } else if mode == FileMode::ImportAudio {
            let audio_path = if crate::video::is_video(&path) {
                match crate::video::extract_audio(&path) {
                    Ok(extracted) => extracted,
                    Err(error) => {
                        self.load_error = Some(error);
                        return;
                    }
                }
            } else {
                path
            };
            match self.editor.import_audio(&audio_path, self.position.max(0)) {
                Ok(track) => {
                    self.status = Some(format!("imported audio as track {}", track.get()));
                    self.file_mode = None;
                }
                Err(error) => self.load_error = Some(error),
            }
        } else if mode == FileMode::ExportMidi {
            let song = self.song().clone();
            let profile = self.settings.device_profile.as_ref().and_then(|id| {
                self.device_profiles
                    .iter()
                    .find(|profile| &profile.id == id)
            });
            match export_midi_file_with_profile(&song, &path, profile) {
                Ok(tracks) => {
                    self.status = Some(format!(
                        "exported {tracks} MIDI track(s) to {}",
                        path.display()
                    ));
                    self.file_mode = None;
                }
                Err(error) => self.load_error = Some(error),
            }
        } else if mode == FileMode::ImportL6max {
            match import_l6max_folder(&mut self.editor, &path, self.position.max(0)) {
                Ok((imported, problems)) => {
                    self.status = Some(format!(
                        "imported {imported} L6max take(s), {problems} problem(s)"
                    ));
                    self.file_mode = None;
                }
                Err(error) => self.load_error = Some(error),
            }
        } else if mode == FileMode::ExportStems {
            let song = self.song().clone();
            match export_stems_folder(&song, &path) {
                Ok(stems) => {
                    self.status = Some(format!("exported {stems} stem(s) to {}", path.display()));
                    self.file_mode = None;
                }
                Err(error) => self.load_error = Some(error),
            }
        } else if mode == FileMode::ExportWav {
            let song = self.song().clone();
            match export_wav_file(&song, &path) {
                Ok((seconds, problems)) => {
                    self.status = Some(format!(
                        "exported {seconds:.2} s WAV to {} ({problems} audio problem(s))",
                        path.display()
                    ));
                    self.file_mode = None;
                }
                Err(error) => self.load_error = Some(error),
            }
        } else if mode == FileMode::ExportMusicXml {
            let song = self.song().clone();
            let title = if song.name.is_empty() {
                "Lazy Arranger score"
            } else {
                &song.name
            };
            match export_musicxml_file(&song, &path, title) {
                Ok(()) => {
                    self.status = Some(format!("exported MusicXML to {}", path.display()));
                    self.file_mode = None;
                }
                Err(error) => self.load_error = Some(error),
            }
        } else {
            self.save_to(normalize_save_path(path));
            if self.load_error.is_none() {
                self.file_mode = None;
            }
        }
    }

    fn advance(&mut self, context: &Context) {
        if let Some(receiver) = self.plugin_scan_rx.take() {
            match receiver.try_recv() {
                Ok((found, failed)) => {
                    let _ =
                        plugins::save_catalog(std::path::Path::new(plugins::CATALOG_FILE), &found);
                    self.plugin_error = if found.is_empty() {
                        Some(String::from("no bundles found"))
                    } else if failed > 0 {
                        Some(format!(
                            "{failed} plug-in bundle(s) blocked after scan failure"
                        ))
                    } else {
                        None
                    };
                    self.plugin_scan = Some(found);
                    self.plugin_scanning = false;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.plugin_scan_rx = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.plugin_scanning = false;
                    self.plugin_error = Some(String::from("plug-in scan thread stopped"));
                }
            }
        }
        if let Some(receiver) = self.audio_recording.take() {
            match receiver.try_recv() {
                Ok(Ok(path)) => {
                    let target = self.audio_record_target.take();
                    let result = match target {
                        Some(track) => self
                            .editor
                            .record_audio_into(track, &path, self.audio_record_start)
                            .map(|()| track),
                        None => self.editor.import_audio(&path, self.audio_record_start),
                    };
                    match result {
                        Ok(track) => {
                            self.status =
                                Some(format!("recorded audio into track {}", track.get()));
                        }
                        Err(error) => self.status = Some(error),
                    }
                }
                Ok(Err(error)) => self.status = Some(error),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.audio_recording = Some(receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status = Some(String::from("audio recording thread stopped"));
                }
            }
        }
        let finished = self
            .recording
            .as_ref()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some(Err(String::from("recording thread ended")))
                }
            });
        if let Some(result) = finished {
            self.recording = None;
            match result {
                Ok(song) => {
                    let mut notes = song
                        .tracks
                        .first()
                        .map(|track| track.notes.clone())
                        .unwrap_or_default();
                    if self.settings.record_quantise {
                        quantise_recorded(&mut notes, music_core::PPQ / 4);
                    }
                    let target = self.record_target.take();
                    let inserted = match target {
                        Some(track) => self.editor.append_recorded(track, &notes).map(|()| track),
                        None => self.editor.insert_recorded("Recorded", &notes),
                    };
                    match inserted {
                        Ok(track) => {
                            self.roll.selected = Some(track);
                            self.status = Some(format!("recorded {} notes", notes.len()));
                            if self.settings.record_chords {
                                match self.editor.find_chords() {
                                    Ok(found) => {
                                        self.status = Some(format!(
                                            "recorded {} notes; detected {found} chord(s)",
                                            notes.len()
                                        ));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                        }
                        Err(error) => self.status = Some(error),
                    }
                }
                Err(error) => self.status = Some(format!("recording: {error}")),
            }
        }
        if let Some(player) = &self.player {
            let seconds = player.seconds();
            self.position = self.song().tempo_map.tick_at_seconds(seconds);
            self.playing = player.is_playing();
            let wraps = player.loop_wraps();
            if wraps != self.last_loop_wraps {
                self.last_loop_wraps = wraps;
            }
            self.advance_launcher();
            if self.playing {
                context.request_repaint_after(Duration::from_millis(16));
            }
            return;
        }
        if !self.playing {
            return;
        }
        let dt = context.input(|input| input.stable_dt).min(0.1);
        let bpm = self.song().tempo_map.bpm_at(self.position);
        let ticks = (f64::from(dt) * bpm / 60.0 * PPQ as f64).round() as Tick;
        self.position += ticks.max(1);
        let end = self.song().end_tick().max(1);
        if self.position > end {
            self.position = 0;
            self.playing = false;
        }
        context.request_repaint_after(Duration::from_millis(16));
    }

    /// Appends a chord symbol to the chord text field.
    fn append_chord(&mut self, text: &str) {
        if self.chord_input.trim().is_empty() {
            self.chord_input = text.to_string();
        } else {
            self.chord_input.push(' ');
            self.chord_input.push_str(text);
        }
    }

    /// The chord lane entry dialog: type a progression, apply or cancel.
    fn chord_dialog(&mut self, context: &Context) {
        if !self.chord_dialog {
            return;
        }
        let mut open = self.chord_dialog;
        egui::Window::new("Chords")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label("Am F C G · vi IV I V · C Am | F G   (| bar, % repeat, / hold)");
                ui.horizontal(|ui| {
                    ui.label("Key:");
                    egui::ComboBox::from_id_salt("chord-key")
                        .selected_text(self.key.name_sharp())
                        .show_ui(ui, |ui| {
                            for semitone in 0..12_u8 {
                                let candidate = music_core::PitchClass::new(i32::from(semitone));
                                ui.selectable_value(
                                    &mut self.key,
                                    candidate,
                                    candidate.name_sharp(),
                                );
                            }
                        });
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.chord_input)
                        .desired_width(320.0)
                        .hint_text("Am F C G"),
                );
                let pad_chords = music_core::roman::pads(self.key);
                egui::Grid::new("chord-pads")
                    .num_columns(4)
                    .spacing([4.0, 4.0])
                    .show(ui, |ui| {
                        for (index, chord) in pad_chords.iter().enumerate() {
                            let label = chord.format(music_core::Spelling::Sharp);
                            if ui.button(label.clone()).clicked() {
                                self.append_chord(&label);
                            }
                            if index % 4 == 3 {
                                ui.end_row();
                            }
                        }
                    });
                let suggestions = music_core::progression::parse_in_key(
                    &self.chord_input,
                    0,
                    self.song().time_signature,
                    self.key,
                )
                .ok()
                .and_then(|placed| placed.last().map(|entry| entry.chord))
                .map(music_core::roman::suggest_next);
                if let Some(list) = suggestions {
                    ui.horizontal(|ui| {
                        ui.label("Next:");
                        for chord in list {
                            let label = chord.format(music_core::Spelling::Sharp);
                            if ui.button(label.clone()).clicked() {
                                self.append_chord(&label);
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        let bar = self.song().time_signature.bar_ticks().max(1);
                        let start = snap(self.position, bar);
                        match self.editor.apply_progression_in_key(
                            &self.chord_input,
                            start,
                            self.key,
                        ) {
                            Ok(count) => {
                                self.load_error = None;
                                let _ = count;
                                self.chord_dialog = false;
                            }
                            Err(error) => self.load_error = Some(error),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.chord_dialog = false;
                    }
                });
            });
        self.chord_dialog = open && self.chord_dialog;
    }

    /// Loads and caches audio waveform peaks for the meters and timeline.
    fn refresh_waveform(&mut self, context: &Context) {
        let audio_tracks: Vec<(TrackId, String)> = self
            .song()
            .tracks
            .iter()
            .filter_map(|track| {
                track
                    .audio
                    .as_ref()
                    .map(|reference| (track.id, reference.path.clone()))
            })
            .collect();
        self.waveform_pending
            .retain(|track, (path, _)| audio_tracks.iter().any(|(id, p)| id == track && p == path));
        let mut finished = Vec::new();
        for (track, (path, receiver)) in &self.waveform_pending {
            match receiver.try_recv() {
                Ok(peaks) => finished.push((*track, path.clone(), peaks)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    finished.push((*track, path.clone(), Vec::new()))
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        for (track, path, peaks) in finished {
            self.waveform_pending.remove(&track);
            self.timeline_waveforms.insert(track, (path, peaks));
        }
        for (id, path) in &audio_tracks {
            if self
                .timeline_waveforms
                .get(id)
                .is_some_and(|(cached, _)| cached == path)
                || self.waveform_pending.contains_key(id)
            {
                continue;
            }
            let path = path.clone();
            let source = std::path::PathBuf::from(&path);
            let (sender, receiver) = std::sync::mpsc::channel();
            self.waveform_pending.insert(*id, (path.clone(), receiver));
            context.request_repaint_after(std::time::Duration::from_millis(100));
            std::thread::spawn(move || {
                let peaks = std::fs::read(&source)
                    .ok()
                    .and_then(|bytes| {
                        if bytes.starts_with(b"RIFF") {
                            audio::wav::decode_mono(&bytes).ok()
                        } else {
                            audio::decode::decode_mono(&source).ok()
                        }
                    })
                    .map(|decoded| audio::waveform::peaks(&decoded.samples, 240))
                    .unwrap_or_default();
                let _ = sender.send(peaks);
            });
        }
        if !self.waveform_pending.is_empty() {
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let selected = self.roll.selected.or_else(|| {
            self.song()
                .tracks
                .iter()
                .find(|track| track.audio.is_some())
                .map(|track| track.id)
        });
        self.waveform = selected.and_then(|id| {
            self.timeline_waveforms
                .get(&id)
                .map(|(path, peaks)| (id, path.clone(), peaks.clone()))
        });
    }

    fn window_title(name: &str, dirty: bool) -> String {
        let dirty = if dirty { "*" } else { "" };
        if name.is_empty() {
            format!("Lazy Arranger Midi DAW{dirty}")
        } else {
            format!("{name}{dirty} — Lazy Arranger Midi DAW")
        }
    }

    fn waveform_fraction(rect: Rect, x: f32, start: usize, end: usize, peaks: usize) -> f32 {
        if peaks == 0 {
            return 0.0;
        }
        let fraction = ((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
        let index = start as f32 + fraction * end.saturating_sub(start) as f32;
        (index / peaks as f32).clamp(0.0, 1.0)
    }

    fn draw_waveform(
        ui: &mut egui::Ui,
        peaks: &[audio::waveform::Peak],
        zoom: f32,
        position: f32,
    ) -> Option<f32> {
        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(ui.available_width(), 44.0),
            egui::Sense::click_and_drag(),
        );
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, theme::lane());
        if peaks.is_empty() {
            return None;
        }
        let centre = rect.center().y;
        let height = rect.height() * 0.45;
        let (start, end) = audio::waveform::visible(peaks.len(), zoom, position);
        let visible = &peaks[start..end];
        for (index, peak) in visible.iter().enumerate() {
            let x = rect.left() + rect.width() * index as f32 / visible.len() as f32;
            painter.line_segment(
                [
                    egui::Pos2::new(x, centre - peak.max * height),
                    egui::Pos2::new(x, centre - peak.min * height),
                ],
                egui::Stroke::new(1.0_f32, theme::accent()),
            );
        }
        painter.line_segment(
            [
                egui::Pos2::new(rect.left(), centre),
                egui::Pos2::new(rect.right(), centre),
            ],
            egui::Stroke::new(0.5_f32, theme::line()),
        );
        let click = response
            .interact_pointer_pos()
            .filter(|_| response.clicked() || response.dragged())?;
        Some(Self::waveform_fraction(
            rect,
            click.x,
            start,
            end,
            peaks.len(),
        ))
    }

    fn open_library_path(&mut self, path: &std::path::Path) -> Result<(), String> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension == "mid" || extension == "midi" {
            let bytes =
                std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
            let (song, _) = smf::decode(&bytes).map_err(|error| error.to_string())?;
            self.replace_project(ProjectFile::new(song), &path.display().to_string());
            self.project_path = Some(path.to_path_buf());
            Ok(())
        } else {
            let imported = self.editor.import_style_tracks(path, "Playlist")?;
            self.status = Some(format!(
                "opened {} track(s), {} note(s) from {}",
                imported.tracks,
                imported.notes,
                path.display()
            ));
            Ok(())
        }
    }

    /// The Music Finder and playlist browser.
    fn library_window(&mut self, context: &egui::Context) {
        if !self.library_open {
            return;
        }
        let mut open = self.library_open;
        egui::Window::new("Library")
            .open(&mut open)
            .default_width(520.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.library_path)
                            .hint_text("Music Finder .json/.csv/.mfd or playlist .tsv")
                            .desired_width(320.0),
                    );
                    if ui.button("Load").clicked() {
                        match crate::library_view::load(std::path::Path::new(&self.library_path)) {
                            Ok(browser) => {
                                self.status = Some(format!("{} row(s) loaded", browser.rows.len()));
                                self.library = Some(browser);
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
                if let Some(browser) = &self.library {
                    if let Some(action) =
                        crate::library_view::show(ui, browser, &mut self.library_query)
                    {
                        let index = match action {
                            crate::library_view::Action::Apply(index)
                            | crate::library_view::Action::Import(index)
                            | crate::library_view::Action::Open(index) => index,
                        };
                        let row = browser.rows[index].clone();
                        match action {
                            crate::library_view::Action::Apply(_) => match row.tempo {
                                Some(bpm) => match self.editor.set_tempo(bpm) {
                                    Ok(()) => {
                                        self.status =
                                            Some(format!("{}: tempo {bpm} bpm", row.title))
                                    }
                                    Err(error) => self.status = Some(error),
                                },
                                None => {
                                    self.status = Some(format!("{}: playlist entry", row.title))
                                }
                            },
                            crate::library_view::Action::Import(_) => {
                                let root = if self.settings.style_library.is_empty() {
                                    std::env::var("ARRANGER_STYLE_LIBRARY").unwrap_or_default()
                                } else {
                                    self.settings.style_library.clone()
                                };
                                let hint = row
                                    .style
                                    .as_deref()
                                    .unwrap_or(&row.title)
                                    .rsplit_once('.')
                                    .map_or_else(
                                        || row.style.as_deref().unwrap_or(&row.title).to_string(),
                                        |(stem, _)| stem.to_string(),
                                    );
                                let chosen = if root.is_empty() {
                                    Err(String::from(
                                        "set a style library folder in Settings first",
                                    ))
                                } else {
                                    style_formats::catalog::catalog(std::path::Path::new(&root))
                                        .map_err(|error| error.to_string())
                                        .and_then(|entries| {
                                            style_formats::catalog::resolve(
                                                &entries,
                                                row.style_number,
                                                &hint,
                                            )
                                            .map(|entry| entry.path.clone())
                                            .ok_or_else(|| format!("no style matched {hint:?}"))
                                        })
                                };
                                match chosen {
                                    Ok(path) => {
                                        match self.editor.import_style_tracks(&path, "Style") {
                                            Ok(imported) => {
                                                self.status = Some(format!(
                                                    "imported {} track(s), {} note(s) from {}",
                                                    imported.tracks,
                                                    imported.notes,
                                                    path.display()
                                                ));
                                            }
                                            Err(error) => self.status = Some(error),
                                        }
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                            crate::library_view::Action::Open(_) => {
                                let path = row.path.clone().unwrap_or_default();
                                if path.is_empty() {
                                    self.status = Some(String::from("row has no file"));
                                } else {
                                    let path = resolve_playlist_path(
                                        std::path::Path::new(&self.library_path),
                                        &path,
                                    );
                                    match self.open_library_path(&path) {
                                        Ok(()) => {}
                                        Err(error) => self.status = Some(error),
                                    }
                                }
                            }
                        }
                    }
                } else {
                    ui.label(
                        egui::RichText::new("Load a book to browse it.")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
            });
        if !open {
            self.library_open = false;
        }
    }

    /// The drum pad window.
    fn pads_window(&mut self, context: &egui::Context) {
        if !self.pads_open {
            return;
        }
        let mut open = self.pads_open;
        egui::Window::new("Drum pads")
            .open(&mut open)
            .default_width(380.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pad_path)
                            .hint_text("loop .wav path")
                            .desired_width(240.0),
                    );
                    if ui.button("Load").clicked() {
                        match crate::pads::Pads::load(std::path::Path::new(&self.pad_path)) {
                            Ok(pads) => {
                                self.status =
                                    Some(format!("{} pad(s) loaded", pads.kit.pads.len()));
                                self.pad_state = Some(pads);
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
                if let Some(pads) = &self.pad_state {
                    if let Some(pad) = crate::pads::show(ui, pads) {
                        self.pad_selected = Some(pad.note);
                        match audio::player::Player::new(pads.audio(pad)) {
                            Ok(player) => self.pad_player = Some(player),
                            Err(error) => self.status = Some(error),
                        }
                    }
                } else {
                    ui.label(
                        egui::RichText::new("Load a loop to slice it into pads.")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                if let Some(note) = self.pad_selected
                    && let Some(pads) = &mut self.pad_state
                    && let Some(pad) = pads.kit.pads.iter().find(|pad| pad.note == note).copied()
                {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!(
                            "Pad {note} · start {} ms · end {} ms · {} ms",
                            pads.start_ms(pad),
                            pads.end_ms(pad),
                            pads.millis(pad)
                        ))
                        .size(10.0)
                        .color(theme::text_dim()),
                    );
                    let mut change = None;
                    ui.horizontal(|ui| {
                        if ui.small_button("Start -10").clicked() {
                            change = Some((-10, 0));
                        }
                        if ui.small_button("Start +10").clicked() {
                            change = Some((10, 0));
                        }
                        if ui.small_button("End -10").clicked() {
                            change = Some((0, -10));
                        }
                        if ui.small_button("End +10").clicked() {
                            change = Some((0, 10));
                        }
                    });
                    if let Some((start, end)) = change
                        && pads.adjust(note, start, end)
                        && let Some(pad) =
                            pads.kit.pads.iter().find(|pad| pad.note == note).copied()
                    {
                        match audio::player::Player::new(pads.audio(pad)) {
                            Ok(player) => self.pad_player = Some(player),
                            Err(error) => self.status = Some(error),
                        }
                    }
                }
            });
        if !open {
            self.pads_open = false;
        }
    }

    fn preview_pitch(&mut self, pitch: u8) {
        let mut song = Song::new("Preview", 120.0);
        let mut track = project::song::Track::new(TrackId::new(1).expect("valid"), "Preview");
        if let (Ok(pitch), Ok(id)) = (music_core::Pitch::new(pitch), NoteId::new(1)) {
            track.notes.push(project::song::Note {
                id,
                start: 0,
                duration: music_core::PPQ / 2,
                pitch,
                velocity: music_core::Velocity::new(100).expect("valid"),
                channel: music_core::Channel::first(),
            });
        }
        song.tracks.push(track);
        let rendered = audio::render(&song, audio::SAMPLE_RATE);
        self.preview_player = audio::player::Player::new(rendered).ok();
    }

    /// Settings and shortcut windows.
    fn replace_project(&mut self, project: ProjectFile, label: &str) {
        match Editor::new(project) {
            Ok(editor) => {
                self.stop_playback();
                self.player = None;
                self.editor = editor;
                self.project_path = None;
                self.selected_note = None;
                self.selected_clip = None;
                self.selected_clips.clear();
                self.selected_chord = None;
                self.stem_levels.clear();
                self.position = 0;
                self.key = self.song().key.map_or_else(
                    || score::spelling::detect_key(self.song()).tonic,
                    |key| key.tonic,
                );
                self.theory_minor = self.song().key.is_some_and(|key| key.minor);
                self.score.full_hidden.clear();
                self.score.full_cache = None;
                self.score.track = None;
                self.score.page = 0;
                self.song_name_input = self.song().name.clone();
                self.status = Some(format!("loaded {label}"));
                self.load_error = None;
            }
            Err(error) => self.load_error = Some(error.to_string()),
        }
    }

    fn print_window(&mut self, context: &egui::Context) {
        if !self.print_dialog {
            return;
        }
        let mut open = self.print_dialog;
        let mut print = false;
        egui::Window::new("Print score")
            .open(&mut open)
            .default_width(320.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Copies");
                    ui.add(egui::DragValue::new(&mut self.print_copies).range(1..=99));
                });
                ui.horizontal(|ui| {
                    ui.label("Pages");
                    ui.text_edit_singleline(&mut self.print_pages);
                });
                if !self.printers.is_empty() {
                    egui::ComboBox::from_id_salt("print-dialog-printer")
                        .selected_text(self.printer_choice.as_deref().unwrap_or("Default printer"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.printer_choice, None, "Default printer");
                            for name in &self.printers {
                                ui.selectable_value(
                                    &mut self.printer_choice,
                                    Some(name.clone()),
                                    name,
                                );
                            }
                        });
                }
                ui.label(
                    egui::RichText::new("Example: 1-4, 7. Leave empty for all pages.")
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                ui.horizontal(|ui| {
                    if ui.button("Print").clicked() {
                        print = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.print_dialog = false;
                    }
                });
            });
        self.print_dialog = open;
        if print {
            self.print_score_pdf();
        }
    }

    fn print_score_pdf(&mut self) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        if self.score.all_tracks {
            let selected: Vec<TrackId> = self
                .song()
                .tracks
                .iter()
                .filter(|track| !self.score.full_hidden.contains(&track.id))
                .map(|track| track.id)
                .collect();
            let selected = if selected.is_empty() {
                self.song().tracks.iter().map(|track| track.id).collect()
            } else {
                selected
            };
            let title = if self.song().name.is_empty() {
                String::from("Lazy Arranger full score")
            } else {
                self.song().name.clone()
            };
            let pdf = score::pdf::to_full_pdf_with_frames(
                self.song(),
                score::Settings::default(),
                &title,
                &selected,
                self.score.full_frames,
            );
            let path = std::env::temp_dir().join(format!(
                "lazy-arranger-full-score-{}-{stamp}.pdf",
                std::process::id()
            ));
            if let Err(error) = std::fs::write(&path, pdf) {
                self.status = Some(format!("{}: {error}", path.display()));
                return;
            }
            let pages = self.print_pages.trim();
            if !pages.is_empty()
                && !pages.chars().all(|character| {
                    character.is_ascii_digit() || character == '-' || character == ','
                })
            {
                self.status = Some(String::from("pages must look like 1-4, 7"));
                return;
            }
            let arguments = print_arguments(
                self.printer_choice.as_deref(),
                self.print_copies,
                pages,
                &path,
            );
            match std::process::Command::new("lp").args(&arguments).output() {
                Ok(output) if output.status.success() => {
                    self.status = Some(format!(
                        "sent {} to lp: {}",
                        path.display(),
                        String::from_utf8_lossy(&output.stdout).trim()
                    ));
                    self.print_dialog = false;
                }
                Ok(output) => {
                    self.status = Some(format!(
                        "lp failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                }
                Err(error) => self.status = Some(format!("lp: {error}")),
            }
            return;
        }
        let Some(track) = self
            .score
            .track
            .or(self.roll.selected)
            .or_else(|| self.song().tracks.first().map(|track| track.id))
        else {
            self.status = Some(String::from("no track to print"));
            return;
        };
        let pages = self.print_pages.trim();
        if !pages.is_empty()
            && !pages
                .chars()
                .all(|character| character.is_ascii_digit() || character == '-' || character == ',')
        {
            self.status = Some(String::from("pages must look like 1-4, 7"));
            return;
        }
        let grand_staff = self.score.grand
            || !self.score.one_staff
                && self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == track)
                    .is_some_and(needs_grand_staff);
        let settings = score::Settings {
            grand_staff,
            ..score::Settings::default()
        };
        let title = if self.song().name.is_empty() {
            String::from("Lazy Arranger score")
        } else {
            self.song().name.clone()
        };
        let pdf = if self.score.preview {
            match self
                .song()
                .tracks
                .iter()
                .position(|entry| entry.id == track)
            {
                Some(index) => {
                    match score::cecilia::render_pdf(self.song(), index, &title, self.score.grand) {
                        Ok(pdf) => pdf,
                        Err(error) => {
                            self.status = Some(format!("Cecilia: {error}"));
                            return;
                        }
                    }
                }
                None => {
                    self.status = Some(String::from("track not found for print"));
                    return;
                }
            }
        } else {
            let engraved = score::engrave(self.song(), track, settings);
            score::pdf::to_pdf(&engraved, settings, &title)
        };
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-score-{}-{stamp}.pdf",
            std::process::id()
        ));
        if let Err(error) = std::fs::write(&path, pdf) {
            self.status = Some(format!("{}: {error}", path.display()));
            return;
        }
        let arguments = print_arguments(
            self.printer_choice.as_deref(),
            self.print_copies,
            pages,
            &path,
        );
        match std::process::Command::new("lp").args(&arguments).output() {
            Ok(output) if output.status.success() => {
                self.status = Some(format!(
                    "sent {} to lp: {}",
                    path.display(),
                    String::from_utf8_lossy(&output.stdout).trim()
                ));
                self.print_dialog = false;
            }
            Ok(output) => {
                self.status = Some(format!(
                    "lp failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            Err(error) => self.status = Some(format!("could not run lp: {error}")),
        }
    }

    fn transcribe_audio_track(&mut self, track_id: TrackId) -> Result<(), String> {
        let reference = self
            .song()
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.audio.clone())
            .ok_or_else(|| String::from("this track has no active audio reference"))?;
        let decoded = audio::decode::decode_mono(std::path::Path::new(&reference.path))
            .map_err(|error| format!("{}: {error}", reference.path))?;
        let detected = audio::analysis::to_midi(&decoded.samples, decoded.sample_rate);
        if detected.is_empty() {
            return Err(String::from("no stable monophonic pitch found"));
        }
        let bpm = self.song().tempo_map.bpm_at(0).max(1.0);
        let notes = detected_to_notes(&detected, bpm, music_core::Channel::first())?;
        if notes.is_empty() {
            return Err(String::from("no notes after pitch conversion"));
        }
        let name = format!(
            "{} MIDI",
            self.song()
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .map_or("Audio", |track| track.name.as_str())
        );
        let new_track = self.editor.insert_recorded(&name, &notes)?;
        self.status = Some(format!(
            "transcribed {} note(s) into track {}",
            notes.len(),
            new_track.get()
        ));
        Ok(())
    }

    fn transcribe_audio_chords(&mut self, track_id: TrackId) -> Result<usize, String> {
        let reference = self
            .song()
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.audio.clone())
            .ok_or_else(|| String::from("this track has no active audio reference"))?;
        let decoded = audio::decode::decode_mono(std::path::Path::new(&reference.path))
            .map_err(|error| format!("{}: {error}", reference.path))?;
        let found = audio::chords::recognize(&decoded.samples, decoded.sample_rate, 0.5);
        if found.is_empty() {
            return Err(String::from("no chords found"));
        }
        let bpm = self.song().tempo_map.bpm_at(0).max(1.0);
        let mut chords = Vec::with_capacity(found.len());
        let mut previous = None;
        for event in &found {
            if previous == Some(event.chord) {
                continue;
            }
            previous = Some(event.chord);
            chords.push(project::song::ChordEvent {
                tick: audio::chords::to_ticks(event.start, bpm),
                chord: event.chord,
            });
        }
        self.editor.set_chords(chords)?;
        self.status = Some(format!("found {} chord(s)", found.len()));
        Ok(found.len())
    }

    fn transcribe_audio_drums(&mut self, track_id: TrackId) -> Result<(), String> {
        let reference = self
            .song()
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .and_then(|track| track.audio.clone())
            .ok_or_else(|| String::from("this track has no active audio reference"))?;
        let decoded = audio::decode::decode_mono(std::path::Path::new(&reference.path))
            .map_err(|error| format!("{}: {error}", reference.path))?;
        let detected = audio::analysis::to_drums(&decoded.samples, decoded.sample_rate);
        if detected.is_empty() {
            return Err(String::from("no drum onsets found"));
        }
        let bpm = self.song().tempo_map.bpm_at(0).max(1.0);
        let notes = detected_to_notes(
            &detected,
            bpm,
            music_core::Channel::new(9).map_err(|error| error.to_string())?,
        )?;
        let name = format!(
            "{} Drums",
            self.song()
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .map_or("Audio", |track| track.name.as_str())
        );
        let new_track = self.editor.insert_recorded(&name, &notes)?;
        self.status = Some(format!(
            "transcribed {} drum hit(s) into track {}",
            notes.len(),
            new_track.get()
        ));
        Ok(())
    }

    fn takes_window(&mut self, context: &Context) {
        if !self.takes_open {
            return;
        }
        let track_id = self.roll.selected.or_else(|| {
            self.song()
                .tracks
                .iter()
                .find(|track| track.audio.is_some())
                .map(|track| track.id)
        });
        let Some(track_id) = track_id else {
            self.takes_open = false;
            return;
        };
        let (active, takes, comps) = self
            .song()
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .map(|track| {
                (
                    track.audio.clone(),
                    track.takes.clone(),
                    track.comps.clone(),
                )
            })
            .unwrap_or_default();
        let mut open = self.takes_open;
        let mut action = None;
        let mut comp_edit: Option<(usize, usize, Tick, Tick)> = None;
        let mut play_take: Option<String> = None;
        let mut to_midi = false;
        let mut to_drums = false;
        let mut to_chords = false;
        let mut duplicate = false;
        egui::Window::new("Audio takes")
            .open(&mut open)
            .default_width(400.0)
            .show(context, |ui| {
                match &active {
                    Some(reference) => {
                        ui.horizontal(|ui| {
                            ui.label(format!("Active: {}", reference.path));
                            if ui.small_button("To MIDI").clicked() {
                                to_midi = true;
                            }
                            if ui.small_button("To drums").clicked() {
                                to_drums = true;
                            }
                            if ui.small_button("To chords").clicked() {
                                to_chords = true;
                            }
                            if ui.small_button("Duplicate track").clicked() {
                                duplicate = true;
                            }
                        });
                    }
                    None => {
                        ui.label("This track has no active audio reference.");
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Store active as take").clicked() {
                        action = Some((0_usize, 0_u8));
                    }
                    if self.audition_player.is_some() && ui.button("Stop audition").clicked() {
                        self.audition_player = None;
                    }
                });
                ui.separator();
                if takes.is_empty() {
                    ui.label(
                        egui::RichText::new("No stored takes.")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                for (index, take) in takes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let name = std::path::Path::new(&take.path)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or(&take.path);
                        let is_active = active.as_ref().is_some_and(|reference| {
                            reference.path == take.path && reference.start == take.start
                        });
                        ui.label(format!(
                            "{index}: {name} · {} tick(s){}",
                            take.length,
                            if is_active { " · active" } else { "" }
                        ));
                        if ui.small_button("Play").clicked() {
                            play_take = Some(take.path.clone());
                        }
                        if ui.small_button("Use").clicked() {
                            action = Some((index, 1));
                        }
                        if ui.small_button("Comp here").clicked() {
                            action = Some((index, 3));
                        }
                        if ui.small_button("Remove").clicked() {
                            action = Some((index, 2));
                        }
                    });
                }
                ui.separator();
                ui.label(egui::RichText::new("Comps").strong());
                if comps.is_empty() {
                    ui.label(
                        egui::RichText::new("No comp spans; the active reference plays.")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                for (index, comp) in comps.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{index}"));
                        let mut take = comp.take;
                        egui::ComboBox::from_id_salt(format!("comp-take-{index}"))
                            .width(80.0)
                            .selected_text(format!("take {take}"))
                            .show_ui(ui, |ui| {
                                for (choice, _) in takes.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut take,
                                        choice,
                                        format!("take {choice}"),
                                    );
                                }
                            });
                        let mut start = comp.start;
                        let mut length = comp.length;
                        let start_changed = ui
                            .add(
                                egui::DragValue::new(&mut start)
                                    .range(0..=1_000_000)
                                    .speed(10.0),
                            )
                            .changed();
                        let length_changed = ui
                            .add(
                                egui::DragValue::new(&mut length)
                                    .range(1..=1_000_000)
                                    .speed(10.0),
                            )
                            .changed();
                        if take != comp.take || start_changed || length_changed {
                            comp_edit = Some((index, take, start, length.max(1)));
                        }
                        if ui.small_button("Remove").clicked() {
                            action = Some((index, 4));
                        }
                    });
                }
            });
        if let Some((index, kind)) = action {
            let result = match kind {
                0 => self.editor.store_audio_take(track_id).map(|_| ()),
                1 => self.editor.use_audio_take(track_id, index),
                2 => self.editor.remove_audio_take(track_id, index),
                3 => {
                    let length = takes.get(index).map_or(0, |take| take.length.max(1));
                    self.editor
                        .add_audio_comp(track_id, index, self.position, length)
                }
                _ => self.editor.remove_audio_comp(track_id, index),
            };
            match result {
                Ok(()) => self.status = Some(String::from("audio takes updated")),
                Err(error) => self.status = Some(error),
            }
        }
        if let Some((index, take, start, length)) = comp_edit
            && let Err(error) = self
                .editor
                .set_audio_comp(track_id, index, take, start, length)
        {
            self.status = Some(error);
        }
        if let Some(path) = play_take {
            let source = std::path::Path::new(&path);
            let decoded = std::fs::read(source).ok().and_then(|bytes| {
                if bytes.starts_with(b"RIFF") {
                    audio::wav::decode_stereo(&bytes).ok()
                } else {
                    audio::decode::decode_stereo(source).ok()
                }
            });
            match decoded.and_then(|audio| audio::player::Player::new(audio).ok()) {
                Some(player) => {
                    self.audition_player = Some(player);
                    self.status = Some(format!("audition {path}"));
                }
                None => self.status = Some(format!("could not audition {path}")),
            }
        }
        if to_midi && let Err(error) = self.transcribe_audio_track(track_id) {
            self.status = Some(error);
        }
        if to_drums && let Err(error) = self.transcribe_audio_drums(track_id) {
            self.status = Some(error);
        }
        if to_chords && let Err(error) = self.transcribe_audio_chords(track_id) {
            self.status = Some(error);
        }
        if duplicate {
            match self.editor.duplicate_audio_track(track_id) {
                Ok(id) => self.status = Some(format!("duplicated audio to track {}", id.get())),
                Err(error) => self.status = Some(error),
            }
        }
        self.takes_open = open;
    }

    fn analyze_audio_chords(&mut self) {
        let path = std::path::PathBuf::from(&self.audio_chords.path);
        let bpm = self.song().tempo_map.bpm_at(0);
        let bar_seconds = self.song().time_signature.bar_ticks() as f64 * 60.0
            / (bpm.max(1.0) * music_core::PPQ as f64);
        let (sender, receiver) = std::sync::mpsc::channel();
        self.audio_chords.pending = Some(receiver);
        self.status = Some(String::from("analysing audio…"));
        std::thread::spawn(move || {
            let result = std::fs::read(&path)
                .map_err(|error| error.to_string())
                .and_then(|bytes| {
                    if bytes.starts_with(b"RIFF") {
                        audio::wav::decode_mono(&bytes).map_err(|error| error.to_string())
                    } else {
                        audio::decode::decode_mono(&path).map_err(|error| error.to_string())
                    }
                })
                .map(|decoded| {
                    audio::chords::recognize(&decoded.samples, decoded.sample_rate, bar_seconds)
                });
            let _ = sender.send(result);
        });
    }

    fn generate_window(&mut self, context: &Context) {
        if !self.generate_open {
            return;
        }
        let mut open = self.generate_open;
        egui::Window::new("Generate part")
            .open(&mut open)
            .resizable(false)
            .default_width(360.0)
            .show(context, |ui| {
                let tracks: Vec<(TrackId, String)> = self
                    .song()
                    .tracks
                    .iter()
                    .map(|track| (track.id, track.name.clone()))
                    .collect();
                let selected = self
                    .roll
                    .selected
                    .filter(|id| tracks.iter().any(|(track, _)| track == id))
                    .or_else(|| tracks.first().map(|(id, _)| *id));
                let Some(track) = selected else {
                    ui.label("Add a track first.");
                    return;
                };
                ui.label(format!(
                    "Track: {}",
                    tracks
                        .iter()
                        .find(|(id, _)| *id == track)
                        .map_or("selected", |(_, name)| name.as_str())
                ));
                ui.horizontal(|ui| {
                    ui.label("Seed");
                    ui.add(egui::DragValue::new(&mut self.generate_seed));
                });
                ui.label(
                    egui::RichText::new(
                        "Each button writes one undoable edit from the song's own chords and notes.",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
                for (label, op) in [
                    ("Humanise", "humanize"),
                    ("Drumify", "drumify"),
                    ("Bass", "bass"),
                    ("Pad", "pad"),
                    ("Arpeggio", "arp"),
                    ("Continue 4 bars", "continue"),
                    ("Melody", "melody"),
                ] {
                    if ui.button(label).clicked() {
                        match self.editor.generate_part(track, op, self.generate_seed) {
                            Ok(_) => self.status = Some(format!("generated {op}")),
                            Err(error) => self.status = Some(error),
                        }
                    }
                }
            });
        self.generate_open = open;
    }

    fn audio_chords_window(&mut self, context: &Context) {
        if !self.audio_chords.open {
            return;
        }
        if self.audio_chords.auto {
            self.audio_chords.auto = false;
            self.analyze_audio_chords();
        }
        let mut open = self.audio_chords.open;
        egui::Window::new("Chords from audio")
            .open(&mut open)
            .resizable(false)
            .default_width(360.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Audio");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.audio_chords.path)
                            .desired_width(220.0),
                    );
                    if ui.button("Analyze").clicked() {
                        self.analyze_audio_chords();
                    }
                });
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for chord in &self.audio_chords.chords {
                            ui.label(format!(
                                "{:>7.2}s  {}  ({:.0}%)",
                                chord.start,
                                chord.chord.format(music_core::Spelling::Sharp),
                                chord.score * 100.0
                            ));
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("Write to song").clicked() {
                        let bpm = self.song().tempo_map.bpm_at(0);
                        let events: Vec<project::song::ChordEvent> = self
                            .audio_chords
                            .chords
                            .iter()
                            .map(|chord| project::song::ChordEvent {
                                tick: audio::chords::to_ticks(chord.start, bpm),
                                chord: chord.chord,
                            })
                            .collect();
                        match self.editor.set_chords(events) {
                            Ok(()) => {
                                self.status = Some(String::from("wrote audio chords to the song"));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Clear").clicked() {
                        self.audio_chords.chords.clear();
                    }
                });
            });
        self.audio_chords.open = open;
    }

    fn help_window(&mut self, context: &egui::Context) {
        if !self.help_open {
            return;
        }
        let mut open = self.help_open;
        let language = crate::strings::Language::parse(&self.settings.language);
        let sections = crate::help::sections(language);
        egui::Window::new("Help")
            .open(&mut open)
            .default_width(460.0)
            .show(context, |ui| {
                for section in sections {
                    ui.heading(section.title);
                    for line in section.lines {
                        ui.label(format!("• {line}"));
                    }
                    ui.add_space(6.0);
                }
            });
        self.help_open = open;
    }

    fn songs_window(&mut self, context: &egui::Context) {
        if !self.songs_open {
            return;
        }
        enum Choice {
            Arranger,
            Kids,
            Empty,
        }
        let mut open = self.songs_open;
        let mut choice = None;
        egui::Window::new("Songs & tutorial")
            .open(&mut open)
            .default_width(380.0)
            .show(context, |ui| {
                ui.label(egui::RichText::new("Named songs").strong());
                if ui
                    .add_sized([220.0, 30.0], egui::Button::new("Lazy Arranger demo"))
                    .on_hover_text("Sixteen bars with drums, bass, piano and pad")
                    .clicked()
                {
                    choice = Some(Choice::Arranger);
                }
                if ui
                    .add_sized([220.0, 30.0], egui::Button::new("Kids demo"))
                    .on_hover_text("Four slow chords for the chord lesson")
                    .clicked()
                {
                    choice = Some(Choice::Kids);
                }
                if ui
                    .add_sized([220.0, 30.0], egui::Button::new("Empty song"))
                    .clicked()
                {
                    choice = Some(Choice::Empty);
                }
                ui.separator();
                ui.label(egui::RichText::new("Tutorial").strong());
                for step in [
                    "1. Pick a named song or open a project with Cmd+O.",
                    "2. Press Play; click the bar ruler to seek while playing.",
                    "3. In Arrange, drag a note to move it or its right edge to resize; Snap 1/16 toggles snapping.",
                    "4. Windows opens the Mixer, Score, Lessons, Theory and the other tools.",
                ] {
                    ui.label(step);
                }
                ui.label(
                    egui::RichText::new("Loading a song replaces the current project.")
                        .size(10.0)
                        .color(theme::text_dim()),
                );
            });
        self.songs_open = open;
        match choice {
            Some(Choice::Arranger) => {
                self.replace_project(demo_project(), "the arranger demo");
            }
            Some(Choice::Kids) => {
                self.replace_project(
                    ProjectFile::new(arranging::demo::kids_song()),
                    "the kids demo",
                );
            }
            Some(Choice::Empty) => {
                self.replace_project(
                    ProjectFile::new(project::song::Song::new("Untitled", 120.0)),
                    "an empty song",
                );
            }
            None => {}
        }
    }

    fn lessons_window(&mut self, context: &egui::Context) {
        if !self.lessons_open {
            return;
        }
        let mut open = self.lessons_open;
        let mut new_question = false;
        let mut reset_score = false;
        let mut answer = None;
        egui::Window::new("Lessons")
            .open(&mut open)
            .default_width(400.0)
            .show(context, |ui| {
                egui::ComboBox::from_id_salt("lesson-pack")
                    .selected_text(
                        self.lesson_pack
                            .and_then(|index| music_core::lesson::PACKS.get(index))
                            .map_or("Custom", |pack| pack.name),
                    )
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.lesson_pack.is_none(), "Custom")
                            .clicked()
                        {
                            self.lesson_pack = None;
                            self.settings.lesson_pack = None;
                            let _ = self.settings.save(&crate::settings::Settings::path());
                            self.lesson_question = None;
                            self.lesson_feedback = None;
                        }
                        for (index, pack) in music_core::lesson::PACKS.iter().enumerate() {
                            if ui
                                .selectable_label(self.lesson_pack == Some(index), pack.name)
                                .clicked()
                            {
                                self.lesson_pack = Some(index);
                                self.settings.lesson_pack = Some(index);
                                let _ = self.settings.save(&crate::settings::Settings::path());
                                self.lesson_kind = pack.kind(0);
                                self.lesson_question = None;
                                self.lesson_feedback = None;
                            }
                        }
                    });
                if let Some(pack) = self
                    .lesson_pack
                    .and_then(|index| music_core::lesson::PACKS.get(index))
                {
                    ui.label(
                        egui::RichText::new(pack.description)
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                }
                egui::ComboBox::from_id_salt("lesson-kind")
                    .selected_text(self.lesson_kind.name())
                    .show_ui(ui, |ui| {
                        for kind in music_core::lesson::LessonKind::ALL {
                            if ui
                                .selectable_label(self.lesson_kind == kind, kind.name())
                                .clicked()
                            {
                                self.lesson_kind = kind;
                                self.lesson_pack = None;
                                self.settings.lesson_pack = None;
                                self.settings.lesson_kind = music_core::lesson::LessonKind::ALL
                                    .iter()
                                    .position(|entry| *entry == kind)
                                    .unwrap_or(0);
                                let _ = self.settings.save(&crate::settings::Settings::path());
                                self.lesson_question = None;
                                self.lesson_feedback = None;
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    if ui.button("New question").clicked() {
                        new_question = true;
                    }
                    ui.label(format!(
                        "score {} / {}",
                        self.lesson_score.0,
                        self.lesson_score.0 + self.lesson_score.1
                    ));
                    if ui.small_button("Reset").clicked() {
                        reset_score = true;
                    }
                });
                if let Some(question) = &self.lesson_question {
                    ui.separator();
                    ui.label(egui::RichText::new(&question.prompt).strong());
                    for (index, choice) in question.choices.iter().enumerate() {
                        if ui
                            .add_enabled(self.lesson_feedback.is_none(), egui::Button::new(choice))
                            .clicked()
                        {
                            answer = Some(index);
                        }
                    }
                    if let Some(correct) = self.lesson_feedback {
                        ui.label(if correct {
                            egui::RichText::new("Correct").color(theme::playing())
                        } else {
                            egui::RichText::new(format!("Wrong — {}", question.explanation()))
                                .color(theme::recording())
                        });
                        if ui.button("Next").clicked() {
                            new_question = true;
                        }
                    }
                } else {
                    ui.add(
                        egui::Label::new("Press New question to start a graded exercise.").wrap(),
                    );
                }
            });
        self.lessons_open = open;
        if let Some(index) = answer
            && let Some(question) = &self.lesson_question
            && self.lesson_feedback.is_none()
        {
            let correct = question.is_correct(index);
            if correct {
                self.lesson_score.0 += 1;
            } else {
                self.lesson_score.1 += 1;
            }
            self.settings.lesson_correct = self.lesson_score.0;
            self.settings.lesson_wrong = self.lesson_score.1;
            let _ = self.settings.save(&crate::settings::Settings::path());
            self.lesson_feedback = Some(correct);
        }
        if reset_score {
            self.lesson_score = (0, 0);
            self.settings.lesson_correct = 0;
            self.settings.lesson_wrong = 0;
            let _ = self.settings.save(&crate::settings::Settings::path());
        }
        if new_question {
            if let Some(pack) = self
                .lesson_pack
                .and_then(|index| music_core::lesson::PACKS.get(index))
            {
                self.lesson_kind = pack.kind(self.lesson_seed);
            }
            self.lesson_question = Some(music_core::lesson::question(
                self.lesson_kind,
                self.lesson_seed,
            ));
            self.lesson_seed = self.lesson_seed.wrapping_add(1);
            self.lesson_feedback = None;
        }
    }

    fn theory_window(&mut self, context: &egui::Context) {
        if !self.theory_open {
            return;
        }
        let lines = theory_summary(self.song());
        let mut open = self.theory_open;
        egui::Window::new("Theory")
            .open(&mut open)
            .default_width(420.0)
            .show(context, |ui| {
                for line in lines {
                    ui.label(line);
                }
                ui.separator();
                let mut tonic = self.key;
                ui.horizontal(|ui| {
                    ui.label("Song key");
                    egui::ComboBox::from_id_salt("theory-key")
                        .selected_text(tonic.name_sharp())
                        .show_ui(ui, |ui| {
                            for semitone in 0..12 {
                                let candidate = music_core::PitchClass::new(semitone);
                                ui.selectable_value(&mut tonic, candidate, candidate.name_sharp());
                            }
                        });
                    ui.checkbox(&mut self.theory_minor, "minor");
                });
                self.key = tonic;
                let mut key_change = None;
                ui.horizontal(|ui| {
                    if ui.button("Set song key").clicked() {
                        key_change = Some(Some(project::song::KeySignature {
                            tonic: self.key,
                            minor: self.theory_minor,
                        }));
                    }
                    if ui.button("Auto-detect").clicked() {
                        key_change = Some(None);
                    }
                    ui.label(
                        egui::RichText::new(
                            "Read-only analysis above; setting the key is one undo step.",
                        )
                        .size(10.0)
                        .color(theme::text_dim()),
                    );
                });
                if let Some(key) = key_change
                    && let Err(error) = self.editor.set_song_key(key)
                {
                    self.status = Some(error);
                }
            });
        self.theory_open = open;
    }

    fn marks_window(&mut self, context: &egui::Context) {
        if !self.marks_open {
            return;
        }
        let marks = self.song().marks.clone();
        let position = self.position.max(0);
        let mut open = self.marks_open;
        let mut add = false;
        let mut remove = None;
        egui::Window::new("Score marks")
            .open(&mut open)
            .default_width(340.0)
            .show(context, |ui| {
                ui.label(
                    egui::RichText::new(format!("playhead at tick {position}"))
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                let options = [
                    "pp", "p", "mp", "mf", "f", "ff", "staccato", "accent", "tenuto", "marcato",
                    "fermata",
                ];
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("mark-kind")
                        .selected_text(options[self.mark_choice.min(options.len() - 1)])
                        .show_ui(ui, |ui| {
                            for (index, name) in options.iter().enumerate() {
                                ui.selectable_value(&mut self.mark_choice, index, *name);
                            }
                        });
                    if ui.button("Add at playhead").clicked() {
                        add = true;
                    }
                });
                ui.separator();
                for (index, mark) in marks.iter().enumerate() {
                    let label = match mark.kind {
                        project::song::MarkKind::Dynamic(dynamic) => dynamic.text().to_string(),
                        project::song::MarkKind::Articulation(articulation) => {
                            format!("{articulation:?}").to_lowercase()
                        }
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{}: {label}", mark.tick));
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                }
            });
        self.marks_open = open;
        if add {
            let kind = match self.mark_choice.min(10) {
                0 => project::song::MarkKind::Dynamic(project::song::Dynamic::Pp),
                1 => project::song::MarkKind::Dynamic(project::song::Dynamic::P),
                2 => project::song::MarkKind::Dynamic(project::song::Dynamic::Mp),
                3 => project::song::MarkKind::Dynamic(project::song::Dynamic::Mf),
                4 => project::song::MarkKind::Dynamic(project::song::Dynamic::F),
                5 => project::song::MarkKind::Dynamic(project::song::Dynamic::Ff),
                6 => project::song::MarkKind::Articulation(project::song::Articulation::Staccato),
                7 => project::song::MarkKind::Articulation(project::song::Articulation::Accent),
                8 => project::song::MarkKind::Articulation(project::song::Articulation::Tenuto),
                9 => project::song::MarkKind::Articulation(project::song::Articulation::Marcato),
                _ => project::song::MarkKind::Articulation(project::song::Articulation::Fermata),
            };
            match self.editor.add_mark(position, kind) {
                Ok(()) => self.status = Some(String::from("score mark added")),
                Err(error) => self.status = Some(error),
            }
        }
        if let Some(index) = remove {
            match self.editor.remove_mark(index) {
                Ok(()) => self.status = Some(String::from("score mark removed")),
                Err(error) => self.status = Some(error),
            }
        }
    }

    fn markers_window(&mut self, context: &egui::Context) {
        if !self.markers_open {
            return;
        }
        let rows: Vec<(Tick, String)> = self
            .song()
            .markers
            .iter()
            .map(|marker| (marker.tick, marker.name.clone()))
            .collect();
        let position = self.position.max(0);
        let mut open = self.markers_open;
        let mut add = false;
        let mut remove = None;
        let mut save = false;
        let mut goto = None;
        egui::Window::new("Markers")
            .open(&mut open)
            .default_width(300.0)
            .show(context, |ui| {
                ui.label(
                    egui::RichText::new(format!("playhead at tick {position}"))
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.marker_text);
                    if ui.button("Add at playhead").clicked() {
                        add = true;
                    }
                });
                ui.separator();
                for (tick, name) in &rows {
                    ui.horizontal(|ui| {
                        ui.label(format!("{tick}: {name}"));
                        if ui.small_button("Edit").clicked() {
                            self.marker_selected = Some(*tick);
                            self.marker_text = name.clone();
                        }
                        if ui.small_button("Go").clicked() {
                            goto = Some(*tick);
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(*tick);
                        }
                    });
                }
                if let Some(tick) = self.marker_selected {
                    ui.separator();
                    ui.label(format!("editing tick {tick}"));
                    if ui.button("Save marker").clicked() {
                        save = true;
                    }
                }
            });
        self.markers_open = open;
        if let Some(tick) = goto {
            self.seek_to(tick);
        }
        if add {
            match self.editor.add_marker(position, self.marker_text.trim()) {
                Ok(()) => self.marker_selected = Some(position),
                Err(error) => self.status = Some(error),
            }
        }
        if let Some(tick) = remove {
            if let Err(error) = self.editor.remove_marker(tick) {
                self.status = Some(error);
            }
            if self.marker_selected == Some(tick) {
                self.marker_selected = None;
            }
        }
        if save
            && let Some(tick) = self.marker_selected
            && let Err(error) = self.editor.add_marker(tick, self.marker_text.trim())
        {
            self.status = Some(error);
        }
    }

    fn lyrics_window(&mut self, context: &egui::Context) {
        if !self.lyrics_open {
            return;
        }
        let rows: Vec<(Tick, String)> = self
            .song()
            .lyrics
            .iter()
            .map(|event| (event.tick, event.text.clone()))
            .collect();
        let position = self.position.max(0);
        let mut open = self.lyrics_open;
        let mut add = false;
        let mut remove = None;
        let mut save = false;
        let mut goto = None;
        egui::Window::new("Lyrics")
            .open(&mut open)
            .default_width(320.0)
            .show(context, |ui| {
                ui.label(
                    egui::RichText::new(format!("playhead at tick {position}"))
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.lyric_text);
                    if ui.button("Add at playhead").clicked() {
                        add = true;
                    }
                });
                ui.separator();
                for (tick, text) in &rows {
                    ui.horizontal(|ui| {
                        ui.label(format!("{tick}: {text}"));
                        if ui.small_button("Edit").clicked() {
                            self.lyric_selected = Some(*tick);
                            self.lyric_text = text.clone();
                        }
                        if ui.small_button("Go").clicked() {
                            goto = Some(*tick);
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(*tick);
                        }
                    });
                }
                if let Some(tick) = self.lyric_selected {
                    ui.separator();
                    ui.label(format!("editing tick {tick}"));
                    if ui.button("Save lyric").clicked() {
                        save = true;
                    }
                }
            });
        self.lyrics_open = open;
        if let Some(tick) = goto {
            self.seek_to(tick);
        }
        if add {
            match self.editor.add_lyric(position, self.lyric_text.trim()) {
                Ok(()) => self.lyric_selected = Some(position),
                Err(error) => self.status = Some(error),
            }
        }
        if let Some(tick) = remove {
            if let Err(error) = self.editor.remove_lyric(tick) {
                self.status = Some(error);
            }
            if self.lyric_selected == Some(tick) {
                self.lyric_selected = None;
            }
        }
        if save
            && let Some(tick) = self.lyric_selected
            && let Err(error) = self.editor.add_lyric(tick, self.lyric_text.trim())
        {
            self.status = Some(error);
        }
    }

    fn about_window(&mut self, context: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut open = self.about_open;
        egui::Window::new("About Lazy Arranger Midi DAW")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.heading("Lazy Arranger Midi DAW");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("Project schema v20 · 960 PPQ");
                ui.label("Rust · egui/eframe · Cecilia 0.51 engraving");
                ui.separator();
                if let Some(path) = &self.project_path {
                    ui.label(format!("Project: {}", path.display()));
                } else {
                    ui.label("Project: untitled");
                }
                ui.label(format!("Audio: {} Hz", audio::SAMPLE_RATE));
                ui.label(format!("{} MCP tools", mcp::tools().len()));
                ui.separator();
                ui.label(
                    egui::RichText::new(
                        "A local arranger for Yamaha styles, MIDI and audio. No song data leaves this computer.",
                    )
                    .size(10.0)
                    .color(theme::text_dim()),
                );
            });
        self.about_open = open;
    }

    /// Song Forms: four preset forms built from the song's markers.
    fn form_dialog(&mut self, context: &Context) {
        if !self.form_dialog {
            return;
        }
        let mut open = self.form_dialog;
        egui::Window::new("Song Forms")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                let sections = self.editor.sections();
                if sections.is_empty() {
                    ui.label("No sections: add markers to the song first.");
                }
                ui.separator();
                ui.label(
                    egui::RichText::new("Transition at playhead")
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                ui.horizontal(|ui| {
                    for (label, kind) in [
                        ("Automatic", arranging::forms::Transition::Automatic),
                        ("Fill", arranging::forms::Transition::DrumFill),
                        ("Lead-in", arranging::forms::Transition::LeadIn),
                        ("Hold", arranging::forms::Transition::Hold),
                        ("Break", arranging::forms::Transition::Break),
                    ] {
                        if ui.button(label).clicked() {
                            match self.editor.apply_transition(self.position, kind) {
                                Ok(added) => {
                                    self.status = Some(format!(
                                        "{label}: {added} note(s) at tick {}",
                                        self.position
                                    ));
                                }
                                Err(error) => self.status = Some(error),
                            }
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Join loop");
                    egui::ComboBox::from_id_salt("join-section")
                        .selected_text(if self.join_section.is_empty() {
                            "section"
                        } else {
                            &self.join_section
                        })
                        .show_ui(ui, |ui| {
                            for section in &sections {
                                ui.selectable_value(
                                    &mut self.join_section,
                                    section.name.clone(),
                                    &section.name,
                                );
                            }
                        });
                    ui.add(egui::DragValue::new(&mut self.join_total).range(2..=16));
                    if ui.button("Join").clicked() && !self.join_section.is_empty() {
                        match self.editor.join_loop(&self.join_section, self.join_total) {
                            Ok(outcome) => {
                                self.status = Some(format!(
                                    "{}: {} copy(ies), {} note(s)",
                                    outcome.section, outcome.copies, outcome.notes
                                ));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
                ui.separator();
                for (label, form) in [
                    ("Short", arranging::forms::short(&sections)),
                    ("Gig", arranging::forms::gig(&sections)),
                    ("Long intro", arranging::forms::long_intro(&sections)),
                    ("Instrumental", arranging::forms::instrumental(&sections)),
                ] {
                    if ui.button(label).clicked() {
                        match self.editor.apply_form(&form) {
                            Ok(copied) => {
                                self.status = Some(format!("{label}: {copied} note(s) placed"));
                                self.form_dialog = false;
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                }
            });
        self.form_dialog = open && self.form_dialog;
    }

    fn doctor_dialog(&mut self, context: &Context) {
        if !self.doctor_dialog {
            return;
        }
        let mut open = self.doctor_dialog;
        egui::Window::new("Arrangement Doctor")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                if self.doctor_proposals.is_empty() {
                    ui.label("Nothing to suggest: no crowded or repeated bars.");
                }
                let mut used = None;
                let mut audition_doctor: Option<(usize, bool)> = None;
                for (index, proposal) in self.doctor_proposals.iter().enumerate() {
                    ui.group(|ui| {
                        ui.label(egui::RichText::new(&proposal.title).strong());
                        ui.label(&proposal.detail);
                        ui.horizontal(|ui| {
                            if ui.button("Use").clicked() {
                                used = Some(index);
                            }
                            if ui.small_button("A").on_hover_text("Hear before").clicked() {
                                audition_doctor = Some((index, false));
                            }
                            if ui.small_button("B").on_hover_text("Hear after").clicked() {
                                audition_doctor = Some((index, true));
                            }
                        });
                    });
                }
                if let Some((index, after)) = audition_doctor {
                    let proposal = self.doctor_proposals[index].clone();
                    self.audition_proposal(&proposal, after);
                }
                if let Some(index) = used {
                    let proposal = self.doctor_proposals[index].clone();
                    match self.editor.apply_proposal(&proposal) {
                        Ok(changed) => {
                            self.status = Some(format!("applied: {changed} note(s) changed"));
                            self.doctor_proposals.remove(index);
                        }
                        Err(error) => self.load_error = Some(error),
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new("Fit to My Synths").strong());
                ui.horizontal(|ui| {
                    ui.label("low");
                    ui.add(egui::DragValue::new(&mut self.fit_low).range(0..=127));
                    ui.label("high");
                    ui.add(egui::DragValue::new(&mut self.fit_high).range(0..=127));
                    ui.label("voices");
                    ui.add(egui::DragValue::new(&mut self.fit_polyphony).range(1..=64));
                    ui.checkbox(&mut self.fit_drum_kit, "drum kit");
                });
                if ui.button("Compute fit").clicked() {
                    let spec = arranging::SynthSpec {
                        low: self.fit_low,
                        high: self.fit_high,
                        polyphony: self.fit_polyphony,
                        has_drum_kit: self.fit_drum_kit,
                    };
                    self.fit_proposals = arranging::fit_to_synth(self.song(), spec);
                }
                let mut use_fit = None;
                let mut audition_fit: Option<(usize, bool)> = None;
                for (index, proposal) in self.fit_proposals.iter().enumerate() {
                    ui.group(|ui| {
                        ui.label(egui::RichText::new(&proposal.title).strong());
                        ui.label(&proposal.detail);
                        ui.horizontal(|ui| {
                            if ui.button("Use").clicked() {
                                use_fit = Some(index);
                            }
                            if ui.small_button("A").on_hover_text("Hear before").clicked() {
                                audition_fit = Some((index, false));
                            }
                            if ui.small_button("B").on_hover_text("Hear after").clicked() {
                                audition_fit = Some((index, true));
                            }
                        });
                    });
                }
                if let Some((index, after)) = audition_fit {
                    let proposal = self.fit_proposals[index].clone();
                    self.audition_proposal(&proposal, after);
                }
                if let Some(index) = use_fit {
                    let proposal = self.fit_proposals[index].clone();
                    match self.editor.apply_proposal(&proposal) {
                        Ok(changed) => {
                            self.status = Some(format!("fit: {changed} note(s) changed"));
                            self.fit_proposals.remove(index);
                        }
                        Err(error) => self.load_error = Some(error),
                    }
                }
            });
        self.doctor_dialog = open && self.doctor_dialog;
    }

    /// The reharmonize dialog: four rule-based proposals for the chord at the
    /// playhead, each applied over one bar.
    fn reharmonize_dialog(&mut self, context: &Context) {
        if !self.reharmonize_dialog {
            return;
        }
        let target = self.chord_dialog_target;
        let Some(original) = self
            .song()
            .chords
            .iter()
            .find(|event| event.tick == target)
            .map(|event| event.chord)
        else {
            self.reharmonize_dialog = false;
            return;
        };
        let mut open = self.reharmonize_dialog;
        egui::Window::new("Reharmonize")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(format!(
                    "{} at bar {}",
                    original.format(music_core::Spelling::Sharp),
                    target / 3840 + 1
                ));
                for (kind, chords) in music_core::reharmonize::candidates(original) {
                    let label = chords
                        .iter()
                        .map(|chord| chord.format(music_core::Spelling::Sharp))
                        .collect::<Vec<_>>()
                        .join(" ");
                    if ui.button(format!("{}: {label}", kind.label())).clicked() {
                        match self.editor.reharmonize(target, &chords) {
                            Ok(()) => self.reharmonize_dialog = false,
                            Err(error) => self.load_error = Some(error),
                        }
                    }
                }
            });
        self.reharmonize_dialog = open && self.reharmonize_dialog;
    }

    /// Writes an autosave when the project is dirty and the interval elapsed.
    fn run_autosave(&mut self) {
        if !self.editor.dirty() {
            return;
        }
        let Some(path) = self.project_path.clone() else {
            return;
        };
        let Ok(json) = self.editor.project().to_json_pretty() else {
            return;
        };
        let _ = self.autosave.maybe_save(Some(&path), true, &json);
    }

    /// Offers to restore an autosave that is newer than the project.
    fn recovery_dialog(&mut self, context: &Context) {
        let Some(autosave) = self.recovery.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new("Recovered edits found")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(format!("{} is newer than the project.", autosave.display()));
                ui.horizontal(|ui| {
                    if ui.button("Restore").clicked() {
                        match load_project(&autosave) {
                            Ok(project) => match Editor::new(project) {
                                Ok(editor) => {
                                    self.editor = editor;
                                    let original = self
                                        .project_path
                                        .clone()
                                        .unwrap_or_else(|| autosave.clone());
                                    self.project_path = Some(original);
                                    let _ = std::fs::remove_file(&autosave);
                                    self.recovery = None;
                                }
                                Err(error) => self.load_error = Some(error.to_string()),
                            },
                            Err(error) => self.load_error = Some(error.to_string()),
                        }
                    }
                    if ui.button("Discard").clicked() {
                        let _ = std::fs::remove_file(&autosave);
                        self.recovery = None;
                    }
                });
            });
        if !open {
            self.recovery = None;
        }
    }

    /// Starts, pauses or resumes the audio stream.
    fn toggle_playback(&mut self) {
        if let Some(player) = &self.player {
            if player.is_playing() {
                player.pause();
                self.playing = false;
                return;
            }
            // A paused player holds a snapshot from before later edits; rebuild
            // it and resume at the same musical position.
            self.player = None;
        }
        if self.render_pending.is_some() {
            self.status = Some(String::from("still rendering audio"));
            return;
        }
        let resume = if self.position >= self.song().end_tick() {
            0
        } else {
            self.position.max(0)
        };
        let song = if self.launcher.active() {
            self.jam_song(&self.launcher.now_playing(), resume)
        } else {
            self.song().clone()
        };
        let receiver = self.spawn_render(song, self.view == View::Mix);
        self.render_pending = Some((receiver, resume));
        self.status = Some(String::from("rendering audio"));
    }

    /// Renders a song in the background with the metronome and master bus.
    fn spawn_render(
        &self,
        song: project::song::Song,
        stems_wanted: bool,
    ) -> std::sync::mpsc::Receiver<Result<RenderedSong, String>> {
        let metronome = self.settings.metronome;
        let accent = self.settings.metronome_accent;
        let master = audio::master::Master {
            gain_db: self.settings.master_gain_db,
            limiter: self.settings.master_limiter,
            reverb: self.settings.master_reverb,
            chorus: self.settings.master_chorus,
        };
        let output_name = self.settings.audio_output.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rate =
                audio::player::preferred_sample_rate(output_name.as_deref(), audio::SAMPLE_RATE);
            let mut audio = audio::render(&song, rate);
            let problems = audio::mix::check_audio_references(&song);
            if metronome {
                audio::metronome::mix_accent(&mut audio, &song, 1.0, accent);
            }
            audio::master::apply(&mut audio, master);
            let stems = if stems_wanted {
                audio::render_stems(&song, rate)
                    .into_iter()
                    .map(|stem| (stem.track, stem.audio.samples))
                    .collect()
            } else {
                Vec::new()
            };
            let _ = sender.send(Ok(RenderedSong {
                audio,
                stems,
                problems,
            }));
        });
        receiver
    }

    /// Turns looping on or off; with no loop region yet, loops the current bar.
    fn toggle_loop(&mut self) {
        self.loop_enabled = !self.loop_enabled;
        if self.loop_enabled && self.loop_region.is_none() {
            let bar = self.song().time_signature.bar_ticks().max(1);
            self.loop_region = Some(crate::transport::bar_region(self.position, bar));
        }
        self.sync_player_loop();
    }

    fn sync_player_loop(&self) {
        let Some(player) = &self.player else {
            return;
        };
        if self.loop_enabled
            && let Some((start, end)) = self.loop_region
            && end > start
        {
            let from = self.song().tempo_map.seconds_at(start);
            let to = self.song().tempo_map.seconds_at(end);
            player.set_loop(true, from, to);
        } else {
            player.set_loop(false, 0.0, 0.0);
        }
    }

    fn poll_render(&mut self, context: &Context) {
        let Some((receiver, resume)) = &self.render_pending else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(std::time::Duration::from_millis(100));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(String::from("audio renderer stopped"))
            }
        };
        let resume = *resume;
        self.render_pending = None;
        match result {
            Ok(rendered) => {
                if let Some(problem) = rendered.problems.first() {
                    self.status = Some(format!("audio {}: {}", problem.path, problem.reason));
                }
                let stem_rate = rendered.audio.sample_rate;
                match audio::player::Player::new_on(
                    rendered.audio,
                    self.settings.audio_output.as_deref(),
                    (self.settings.audio_buffer_frames > 0)
                        .then_some(self.settings.audio_buffer_frames),
                ) {
                    Ok(player) => {
                        if self.loop_enabled
                            && let Some((start, end)) = self.loop_region
                            && end > start
                        {
                            let from = self.song().tempo_map.seconds_at(start);
                            let to = self.song().tempo_map.seconds_at(end);
                            player.set_loop(true, from, to);
                            let _ = player.seek_seconds(from);
                            self.position = start;
                        } else if resume > 0 {
                            let _ = player.seek_seconds(self.song().tempo_map.seconds_at(resume));
                        }
                        player.play();
                        self.last_loop_wraps = 0;
                        self.stem_sample_rate = stem_rate;
                        self.stem_levels = rendered.stems;
                        self.player = Some(player);
                        self.playing = true;
                        self.status = Some(String::from("playing"));
                    }
                    Err(error) => self.load_error = Some(format!("audio: {error}")),
                }
            }
            Err(error) => self.load_error = Some(format!("audio: {error}")),
        }
    }

    fn stop_playback(&mut self) {
        self.finish_jam();
        self.render_pending = None;
        self.reset_loops();
        if let Some(player) = self.player.take() {
            player.stop();
            player.release();
        }
        self.playing = false;
        self.position = 0;
    }

    fn open(&mut self, path: &PathBuf) {
        let kind = file_kind(path);
        let imported = kind != ProjectKind::Json;
        match load_project(path) {
            Ok(project) => {
                self.render_pending = None;
                if let Some(player) = self.player.take() {
                    player.pause();
                    player.release();
                }
                self.playing = false;
                self.last_loop_wraps = 0;
                match Editor::new(project) {
                    Ok(editor) => self.editor = editor,
                    Err(error) => {
                        self.load_error = Some(error.to_string());
                        return;
                    }
                }
                self.key = self.song().key.map_or_else(
                    || score::spelling::detect_key(self.song()).tonic,
                    |key| key.tonic,
                );
                self.theory_minor = self.song().key.is_some_and(|key| key.minor);
                self.score.full_hidden.clear();
                self.score.full_cache = None;
                self.selected_note = None;
                self.selected_clip = None;
                self.selected_clips.clear();
                self.project_path = if imported { None } else { Some(path.clone()) };
                self.status = if imported {
                    Some(format!(
                        "imported {}; saving writes an untitled project",
                        kind.label()
                    ))
                } else {
                    None
                };
                self.recent = project::recent::record(&self.recent_file, path)
                    .unwrap_or_else(|_| self.recent.clone());
                self.settings.last_project = Some(path.display().to_string());
                let _ = self.settings.save(&crate::settings::Settings::path());
                self.song_name_input = self.song().name.clone();
                self.position = 0;
                self.playing = false;
                if self
                    .roll
                    .selected
                    .is_none_or(|id| !self.song().tracks.iter().any(|track| track.id == id))
                {
                    self.roll.selected = self.song().tracks.first().map(|track| track.id);
                }
                self.load_error = None;
            }
            Err(error) => self.load_error = Some(error.to_string()),
        }
    }
}

impl std::fmt::Debug for ShellApp {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShellApp")
            .field("tracks", &self.song().tracks.len())
            .field("view", &self.view)
            .field("playing", &self.playing)
            .field("position", &self.position)
            .field("dirty", &self.editor.dirty())
            .finish()
    }
}

impl eframe::App for ShellApp {
    fn update(&mut self, context: &Context, frame: &mut eframe::Frame) {
        if let Some(rect) = context.input(|input| input.viewport().inner_rect) {
            self.window_size = rect.size();
        }
        if let Some(rect) = context.input(|input| input.viewport().outer_rect) {
            self.window_pos = Some(rect.min);
        }
        if self.view != self.last_view {
            self.last_view = self.view;
            self.settings.view = self.view.as_str().to_string();
            let _ = self.settings.save(&crate::settings::Settings::path());
        }
        self.sync_monitor();
        self.poll_render(context);
        let _ = frame;
        // ADR-011: paint the first themed frame before the window becomes visible.
        let mut style = (*context.style()).clone();
        let delay_changed = (style.interaction.tooltip_delay - 0.6).abs() > 0.01;
        style.interaction.tooltip_delay = 0.6;
        if style.visuals.panel_fill != theme::panel() || delay_changed {
            style.visuals = theme::visuals();
            context.set_style(style);
        }
        self.sync_native_menu(context);
        self.handle_menu_keys(context);
        self.handle_keys(context);
        self.poll_background(context);
        self.draw_windows(context);
        self.central_panel(context);
        if !self.shown {
            self.shown = true;
            context.send_viewport_cmd(ViewportCommand::Visible(true));
        }
    }

    fn raw_input_hook(&mut self, _context: &Context, raw_input: &mut egui::RawInput) {
        self.feed_menu_input(raw_input);
    }

    fn on_exit(&mut self) {
        if self.window_size.x >= 960.0 && self.window_size.y >= 600.0 {
            self.settings.window_width = self.window_size.x;
            self.settings.window_height = self.window_size.y;
            self.settings.timeline_zoom = self.timeline_zoom;
            self.settings.waveform_zoom = self.waveform_zoom;
            self.settings.score_zoom = self.score.zoom;
            if let Some(position) = self.window_pos {
                self.settings.window_x = Some(position.x);
                self.settings.window_y = Some(position.y);
            }
            let _ = self.settings.save(&crate::settings::Settings::path());
        }
    }
}

fn track_level(track: &project::song::Track, tick: Tick) -> f32 {
    let level: f32 = track
        .notes
        .iter()
        .filter(|note| note.start <= tick && tick < note.start.saturating_add(note.duration))
        .map(|note| f32::from(note.velocity.as_u8()) / 127.0)
        .sum();
    level.clamp(0.0, 1.0)
}

type WaveformCache =
    std::collections::HashMap<project::TrackId, (String, Vec<audio::waveform::Peak>)>;

fn track_meter_level(track: &project::song::Track, tick: Tick, waveforms: &WaveformCache) -> f32 {
    let mut level = track_level(track, tick);
    if let Some(reference) = &track.audio
        && let Some((_, peaks)) = waveforms.get(&track.id)
        && !peaks.is_empty()
        && reference.start <= tick
        && tick < reference.start.saturating_add(reference.length)
    {
        let fraction = (tick - reference.start) as f32 / reference.length.max(1) as f32;
        let index = (fraction * (peaks.len() - 1) as f32).round() as usize;
        let peak = peaks[index.min(peaks.len() - 1)];
        let audio_level = peak.min.abs().max(peak.max.abs()) * track.volume_at(tick) / 100.0;
        level = level.max(audio_level);
    }
    level.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;
