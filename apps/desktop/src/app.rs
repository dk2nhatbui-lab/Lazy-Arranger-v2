//! The Stage 6 shell: Perform / Arrange / Mix over the existing models.

use std::path::PathBuf;
use std::time::Duration;

use egui::{Context, Rect, ViewportCommand};
use music_core::{PPQ, Tick};
use project::song::{ProjectFile, Song};
use project::{NoteId, ProjectError, TrackId};

use crate::editor::{Editor, snap};
use crate::persistence::{self, Autosave};

use crate::piano_roll::{self, RollState};
use crate::theme;
use crate::widgets;

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

/// One row of the track list, snapshotted before drawing so the borrow ends.
#[derive(Clone, Debug, Default)]
struct L6Scene {
    channels: Vec<(u8, u8, bool, u8)>,
}

#[derive(Clone, Debug)]
struct AssistantProposal {
    command: String,
    label: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PsrRegistration {
    channel: u8,
    bank_msb: u8,
    bank_lsb: u8,
    program: u8,
    volume: u8,
    pan: u8,
    reverb: u8,
    chorus: u8,
}

impl PsrRegistration {
    fn encode(self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.channel,
            self.bank_msb,
            self.bank_lsb,
            self.program,
            self.volume,
            self.pan,
            self.reverb,
            self.chorus
        )
    }

    fn decode(line: &str) -> Option<Self> {
        let mut parts = line.trim().split(',');
        let mut next = || parts.next()?.trim().parse::<u8>().ok();
        let registration = Self {
            channel: next()?,
            bank_msb: next()?,
            bank_lsb: next()?,
            program: next()?,
            volume: next()?,
            pan: next()?,
            reverb: next()?,
            chorus: next()?,
        };
        (parts.next().is_none()).then_some(registration)
    }
}

fn psr_regs_path() -> std::path::PathBuf {
    std::path::PathBuf::from("lazy-arranger-psr.txt")
}

fn load_psr_regs() -> [PsrRegistration; 8] {
    let mut regs = [PsrRegistration::default(); 8];
    let Ok(text) = std::fs::read_to_string(psr_regs_path()) else {
        return regs;
    };
    for (index, line) in text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(8)
        .enumerate()
    {
        if let Some(reg) = PsrRegistration::decode(line) {
            regs[index] = reg;
        }
    }
    regs
}

fn save_psr_regs(regs: &[PsrRegistration; 8]) {
    let text: String = regs
        .iter()
        .map(|reg| reg.encode())
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(psr_regs_path(), format!("{text}\n"));
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
fn count_in_audio(bpm: f64, bars: u8) -> audio::RenderedAudio {
    let sample_rate = 48_000_u32;
    let beats = usize::from(bars) * 4;
    let seconds = beats as f64 * 60.0 / bpm.max(1.0);
    let frames = (seconds * f64::from(sample_rate)).round() as usize;
    let mut samples = vec![0.0_f32; frames * 2];
    let click_frames = (0.035 * f64::from(sample_rate)) as usize;
    for beat in 0..beats {
        let start = (beat as f64 * 60.0 / bpm.max(1.0) * f64::from(sample_rate)).round() as usize;
        let amplitude = if beat.is_multiple_of(4) { 0.5 } else { 0.3 };
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

/// Formats bytes as space-separated uppercase hex.
#[must_use]
pub fn sysex_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_sysex(text: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for token in text.split_whitespace() {
        let token = token.trim_start_matches("0x").trim_start_matches("0X");
        let byte =
            u8::from_str_radix(token, 16).map_err(|_| format!("not a hex byte: {token:?}"))?;
        bytes.push(byte);
    }
    if bytes.is_empty() {
        return Err(String::from("enter at least one byte"));
    }
    if bytes.first() != Some(&0xF0) {
        bytes.insert(0, 0xF0);
    }
    if bytes.last() != Some(&0xF7) {
        bytes.push(0xF7);
    }
    Ok(bytes)
}

fn profile_for_port(
    port: &str,
    profiles: &[midi_devices::profiles::LoadedProfile],
) -> Option<String> {
    let normalize = |text: &str| -> String {
        text.chars()
            .filter(char::is_ascii_alphanumeric)
            .flat_map(char::to_lowercase)
            .collect()
    };
    let port = normalize(port);
    profiles
        .iter()
        .find(|profile| {
            let id = normalize(&profile.id);
            let name = normalize(&profile.name);
            (!id.is_empty() && port.contains(&id))
                || (!name.is_empty() && (port.contains(&name) || name.contains(&port)))
        })
        .map(|profile| profile.id.clone())
}

fn l6_scene_path() -> std::path::PathBuf {
    std::path::PathBuf::from("lazy-arranger-l6-scenes.txt")
}

fn encode_l6_scenes(scenes: &[Option<L6Scene>; 4]) -> String {
    let mut text = String::new();
    for (index, scene) in scenes.iter().enumerate() {
        let Some(scene) = scene else {
            continue;
        };
        let values: Vec<String> = scene
            .channels
            .iter()
            .map(|(volume, pan, muted, send)| format!("{volume},{pan},{},{send}", u8::from(*muted)))
            .collect();
        text.push_str(&format!("{index}:{}\n", values.join("|")));
    }
    text
}

fn decode_l6_scenes(text: &str) -> [Option<L6Scene>; 4] {
    let mut scenes: [Option<L6Scene>; 4] = [None, None, None, None];
    for line in text.lines() {
        let Some((index, values)) = line.split_once(':') else {
            continue;
        };
        let Ok(index) = index.parse::<usize>() else {
            continue;
        };
        if index >= 4 {
            continue;
        }
        let mut channels = Vec::new();
        for value in values.split('|').filter(|value| !value.is_empty()) {
            let mut parts = value.split(',');
            let (Some(volume), Some(pan), Some(muted), Some(send)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if let (Ok(volume), Ok(pan), Ok(send)) =
                (volume.parse::<u8>(), pan.parse::<u8>(), send.parse::<u8>())
            {
                channels.push((volume.min(127), pan.min(127), muted == "1", send.min(127)));
            }
        }
        scenes[index] = Some(L6Scene { channels });
    }
    scenes
}

fn load_l6_scenes() -> [Option<L6Scene>; 4] {
    std::fs::read_to_string(l6_scene_path())
        .map(|text| decode_l6_scenes(&text))
        .unwrap_or([None, None, None, None])
}

fn save_l6_scenes(scenes: &[Option<L6Scene>; 4]) {
    let _ = std::fs::write(l6_scene_path(), encode_l6_scenes(scenes));
}

fn seed_default_device_profiles(folder: &std::path::Path) {
    if std::fs::create_dir_all(folder).is_err() {
        return;
    }
    for (name, body) in [
        (
            "xiosynth.yaml",
            include_str!("../assets/profiles/xiosynth.yaml"),
        ),
        ("d05.yaml", include_str!("../assets/profiles/d05.yaml")),
        (
            "generic-gs.yaml",
            include_str!("../assets/profiles/generic-gs.yaml"),
        ),
        (
            "generic-xg.yaml",
            include_str!("../assets/profiles/generic-xg.yaml"),
        ),
        (
            "korg-pa.yaml",
            include_str!("../assets/profiles/korg-pa.yaml"),
        ),
        (
            "psr-s970.yaml",
            include_str!("../assets/profiles/psr-s970.yaml"),
        ),
    ] {
        let path = folder.join(name);
        if !path.exists() {
            let _ = std::fs::write(path, body);
        }
    }
}

fn load_device_profiles() -> Vec<midi_devices::profiles::LoadedProfile> {
    let folder = crate::settings::Settings::path()
        .parent()
        .map(|path| path.join("profiles"))
        .unwrap_or_else(|| std::path::PathBuf::from("profiles"));
    seed_default_device_profiles(&folder);
    midi_devices::profiles::load_directory(&folder)
        .map(|(profiles, _)| profiles)
        .unwrap_or_default()
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

pub struct ShellApp {
    editor: Editor,
    selected_note: Option<(TrackId, NoteId)>,
    selected_notes: Vec<(TrackId, NoteId)>,
    selected_chord: Option<Tick>,
    preview_player: Option<audio::player::Player>,
    selected_clip: Option<(TrackId, project::ClipId)>,
    selected_clips: Vec<(TrackId, project::ClipId)>,
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
    songs_open: bool,
    lessons_open: bool,
    lesson_kind: music_core::lesson::LessonKind,
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
    launcher: arranging::grid::Launcher,
    recent: Vec<std::path::PathBuf>,
    recent_file: std::path::PathBuf,
    midi_input: Option<midi_devices::devices::LiveInputs>,
    midi_map: midi_devices::map::MidiMap,
    loop_enabled: bool,
    loop_region: Option<(Tick, Tick)>,
    loop_buffer: Option<audio::RenderedAudio>,
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
    assistant_open: bool,
    assistant_input: String,
    assistant_log: Vec<String>,
    ai_pending: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    assistant_proposal: Option<AssistantProposal>,
    available_update: Option<crate::update::Release>,
    update_download: Option<std::sync::mpsc::Receiver<Result<std::path::PathBuf, String>>>,
    dictation_pending: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    waveform: Option<(TrackId, String, Vec<audio::waveform::Peak>)>,
    timeline_waveforms: std::collections::HashMap<TrackId, (String, Vec<audio::waveform::Peak>)>,
    midi_ports: Option<(Vec<String>, Vec<String>)>,
    audio_inputs: Option<Vec<String>>,
    audio_outputs: Option<Vec<String>>,
    settings: crate::settings::Settings,
    settings_open: bool,
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
    device_sysex: String,
    device_dump_path: String,
    groove_open: bool,
    groove_entries: Vec<(String, std::path::PathBuf)>,
    groove_selected: usize,
    groove_loaded: Option<arranging::groove::Groove>,
    groove_timing: f32,
    groove_accents: f32,
    help_open: bool,
    score_entry: bool,
    score_octave: i32,
    score_note_length: music_core::Tick,
    score_dotted: bool,
    score_cursor: music_core::Tick,
    score_page: usize,
    score_zoom: f32,
    score_grand: bool,
    score_preview: bool,
    patch_open: bool,
    patch_track: Option<TrackId>,
    patch_draft: Option<project::song::Patch>,
    style_clips_open: bool,
    style_name: String,
    style_chord: String,
    style_length: u32,
    style_from_markers: bool,
    style_path: String,
    style_sff1: bool,
    style_draft: Option<arranger::style::Style>,
    style_rules_open: bool,
    style_preview: Option<audio::player::Player>,
    generate_open: bool,
    generate_seed: u64,
    audio_chords_open: bool,
    audio_chords_path: String,
    audio_chords: Vec<audio::chords::AudioChord>,
    audio_chords_pending:
        Option<std::sync::mpsc::Receiver<Result<Vec<audio::chords::AudioChord>, String>>>,
    audio_chords_auto: bool,
    style_link_open: bool,
    style_link_url: String,
    style_link_pending: Option<std::sync::mpsc::Receiver<Result<std::path::PathBuf, String>>>,
    style_file_open: bool,
    style_file_path: String,
    style_import: Option<style_formats::ImportedStyle>,
    style_section: usize,
    style_file_loop: bool,
    style_auto_load: bool,
    style_queued_section: Option<usize>,
    style_queued_chord: Option<String>,
    style_fill_return: Option<usize>,
    style_ending: bool,
    style_held: [bool; 128],
    style_last_chord: String,
    midi_chord: Option<String>,
    style_queue_at: f64,
    audio_style_open: bool,
    audio_style_path: String,
    audio_style_bpm: f64,
    audio_style_chord: String,
    audio_style_section: String,
    audio_style_split: bool,
    audio_style_section_bars: u32,
    audio_style_drums: bool,
    audio_style_detect_tempo: bool,
    audio_style_out: String,
    audio_style_sff1: bool,
    master_fx_open: bool,
    master_fx_choice: usize,
    bus_fx_open: bool,
    bus_fx_choice: usize,
    bus_fx_plugin_choice: usize,
    midi_out_open: bool,
    midi_out_port: Option<String>,
    midi_out_channel: u8,
    midi_out_program: u8,
    midi_out_note: u8,
    midi_out_velocity: u8,
    midi_out_cc: u8,
    xg_open: bool,
    xg_parts: [[u8; 4]; 16],
    psr_open: bool,
    psr_channel: u8,
    psr_bank_msb: u8,
    psr_bank_lsb: u8,
    psr_program: u8,
    psr_volume: u8,
    psr_pan: u8,
    psr_reverb: u8,
    psr_chorus: u8,
    psr_regs: [PsrRegistration; 8],
    midi_out_cc_value: u8,
    midi_out_bank_msb: u8,
    midi_out_bank_lsb: u8,
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
    score_pdf_path: String,
    score_track: Option<TrackId>,
    score_state: crate::score_view::ScoreState,
    lyrics_open: bool,
    lyric_text: String,
    lyric_selected: Option<Tick>,
    tempo_input: f64,
    automation_target: project::song::AutomationTarget,
    automation_plugin_slot: u32,
    automation_plugin_param: u32,
    l6max_folder: String,
    l6max_plan: Option<audio::l6max::Plan>,
    l6max_error: Option<String>,
    l6max_selected: Vec<bool>,
    l6max_mixer_open: bool,
    l6max_compact: bool,
    l6max_scenes: [Option<L6Scene>; 4],
    sf2_path: String,
    sf2_bank: u16,
    sf2_program: u16,
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
    last_bar: i64,
    synth: arranging::SynthSpec,
    project_path: Option<PathBuf>,
    view: View,
    roll: RollState,
    playing: bool,
    position: Tick,
    icon: egui::TextureHandle,
    player: Option<audio::player::Player>,
    autosave: Autosave,
    recovery: Option<std::path::PathBuf>,
    shown: bool,
    load_error: Option<String>,
}

impl ShellApp {
    #[must_use]
    pub fn new(context: &Context, project: ProjectFile, project_path: Option<PathBuf>) -> Self {
        let settings = crate::settings::Settings::load(&crate::settings::Settings::path());
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
            chord_dialog: false,
            chord_dialog_target: 0,
            reharmonize_dialog: false,
            chord_input: String::from("Am F C G"),
            key: music_core::PitchClass::new(0),
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
            songs_open: initial_window == "songs",
            lessons_open: initial_window == "lessons",
            lesson_kind: music_core::lesson::LessonKind::ChordQuality,
            lesson_seed: 1,
            lesson_question: None,
            lesson_score: (0, 0),
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
            beat_dialog: initial_window == "beat",
            beat_track: None,
            beat_start: 0,
            pending_beat_start: None,
            beat_bar: 3840,
            beat_pattern: arranging::beat::BeatPattern::default(),
            beat_clipboard: None,
            beat_bars: 1,
            waveform_zoom: 1.0,
            waveform_position: 0.0,
            launcher: arranging::grid::Launcher::default(),
            recent_file: std::path::PathBuf::from("recent-projects.txt"),
            recent: project::recent::read(std::path::Path::new("recent-projects.txt")),
            midi_input: None,
            loop_enabled: false,
            loop_region: None,
            loop_buffer: None,
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
            assistant_open: initial_window == "assistant",
            assistant_input: String::new(),
            assistant_log: Vec::new(),
            ai_pending: None,
            assistant_proposal: None,
            available_update: None,
            update_download: None,
            dictation_pending: None,
            waveform: None,
            timeline_waveforms: std::collections::HashMap::new(),
            midi_ports: None,
            audio_inputs: None,
            audio_outputs: None,
            midi_map: std::fs::read_to_string("lazy-arranger-midi-map.txt")
                .ok()
                .and_then(|text| midi_devices::map::MidiMap::from_text(&text).ok())
                .unwrap_or_default(),
            settings,
            settings_open: initial_window == "settings",
            shortcuts_open: initial_window == "shortcuts",
            about_open: initial_window == "about",
            mixer_visible: true,
            kids_last_activity: None,
            timeline_mode: true,
            timeline_zoom: 1.0,
            pending_clip_edit: None,
            inspector_name_track: None,
            inspector_name: String::new(),
            device_profiles: load_device_profiles(),
            device_editor_open: initial_window == "device",
            device_editor_profile: 0,
            device_editor_values: Vec::new(),
            device_sysex: String::new(),
            device_dump_path: String::from("dump.syx"),
            groove_open: initial_window == "groove",
            groove_entries: Vec::new(),
            groove_selected: 0,
            groove_loaded: None,
            groove_timing: 1.0,
            groove_accents: 1.0,
            help_open: initial_window == "help",
            score_entry: false,
            score_octave: 4,
            score_note_length: music_core::PPQ,
            score_dotted: false,
            score_cursor: 0,
            score_page: 0,
            score_zoom: 1.0,
            score_grand: std::env::var("ARRANGER_GRAND").is_ok_and(|value| value == "1"),
            score_preview: std::env::var("ARRANGER_SCORE_EDIT").is_err(),
            patch_open: std::env::var("ARRANGER_PATCH").is_ok(),
            patch_track: None,
            patch_draft: None,
            style_clips_open: std::env::var("ARRANGER_STYLE").is_ok(),
            style_name: String::from("Main A"),
            style_chord: String::from("C"),
            style_length: 3840,
            style_from_markers: true,
            style_path: String::from("clips.sty"),
            style_sff1: false,
            style_draft: None,
            style_rules_open: false,
            style_preview: None,
            generate_open: std::env::var("ARRANGER_GENERATE").is_ok(),
            generate_seed: 1,
            audio_chords_open: std::env::var("ARRANGER_CHORDS").is_ok(),
            audio_chords_path: std::env::var("ARRANGER_CHORDS_FILE").unwrap_or_default(),
            audio_chords: Vec::new(),
            audio_chords_pending: None,
            audio_chords_auto: std::env::var("ARRANGER_CHORDS_FILE").is_ok(),
            style_link_open: std::env::var("ARRANGER_STYLE_LINK").is_ok(),
            style_link_url: String::new(),
            style_link_pending: None,
            style_file_open: std::env::var("ARRANGER_STYLE").is_ok(),
            style_file_path: std::env::var("ARRANGER_STYLE_FILE").unwrap_or_default(),
            style_import: None,
            style_section: 0,
            style_file_loop: true,
            style_auto_load: std::env::var("ARRANGER_STYLE_FILE").is_ok(),
            style_queued_section: None,
            style_queued_chord: None,
            style_fill_return: None,
            style_ending: false,
            style_held: [false; 128],
            style_last_chord: String::new(),
            midi_chord: None,
            style_queue_at: 0.0,
            audio_style_open: std::env::var("ARRANGER_AUDIO_STYLE").is_ok(),
            audio_style_path: String::new(),
            audio_style_bpm: 120.0,
            audio_style_chord: String::from("auto"),
            audio_style_section: String::from("Main A"),
            audio_style_split: false,
            audio_style_section_bars: 0,
            audio_style_drums: false,
            audio_style_detect_tempo: false,
            audio_style_out: String::from("audio-style.sty"),
            audio_style_sff1: false,
            master_fx_open: std::env::var("ARRANGER_MASTER_FX").is_ok(),
            master_fx_choice: 0,
            bus_fx_open: std::env::var("ARRANGER_BUS_FX").is_ok(),
            bus_fx_choice: 0,
            bus_fx_plugin_choice: 0,
            midi_out_open: std::env::var("ARRANGER_MIDI_OUT").is_ok(),
            midi_out_port: midi_output_pref,
            midi_out_channel: 1,
            midi_out_program: 0,
            midi_out_note: 60,
            midi_out_velocity: 100,
            midi_out_cc: 7,
            xg_open: std::env::var("ARRANGER_XG").is_ok(),
            xg_parts: [[100, 64, 40, 0]; 16],
            psr_open: std::env::var("ARRANGER_PSR").is_ok(),
            psr_channel: 1,
            psr_bank_msb: 0,
            psr_bank_lsb: 0,
            psr_program: 0,
            psr_volume: 100,
            psr_pan: 64,
            psr_reverb: 40,
            psr_chorus: 0,
            psr_regs: load_psr_regs(),
            midi_out_cc_value: 100,
            midi_out_bank_msb: 0,
            midi_out_bank_lsb: 0,
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
            score_pdf_path: String::from("score.pdf"),
            score_track: None,
            score_state: crate::score_view::ScoreState::default(),
            lyrics_open: initial_window == "lyrics",
            lyric_text: String::new(),
            lyric_selected: None,
            tempo_input: 120.0,
            automation_target: project::song::AutomationTarget::Volume,
            automation_plugin_slot: 1,
            automation_plugin_param: 0,
            l6max_folder: String::new(),
            l6max_plan: None,
            l6max_error: None,
            l6max_selected: Vec::new(),
            l6max_mixer_open: std::env::var("ARRANGER_L6MIX").is_ok(),
            l6max_compact: false,
            l6max_scenes: load_l6_scenes(),
            sf2_path: String::new(),
            sf2_bank: 0,
            sf2_program: 0,
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
            last_bar: 0,
            synth: arranging::SynthSpec::default(),
            project_path,
            view: initial_view,
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
            .score_track
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
            self.settings.kids_mode = !self.settings.kids_mode;
            self.view = if self.settings.kids_mode {
                View::Perform
            } else {
                View::Arrange
            };
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
            score_typing(self.view, self.score_entry, context.wants_keyboard_input());
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
        if open_keys {
            self.file_path_input = self
                .project_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            self.file_mode = Some(FileMode::Open);
        }
        if save_as {
            self.begin_save_as();
        }
        if save {
            match self.project_path.clone() {
                Some(path) => self.save_to(path),
                None => self.begin_save_as(),
            }
        }
        if self.view == View::Score
            && self.score_entry
            && !context.wants_keyboard_input()
            && !context.input(|input| input.modifiers.command || input.modifiers.alt)
        {
            let track = self
                .score_track
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
                        && let Some(pitch) = score_key_pitch(key, self.score_octave)
                        && let Ok(pitch) = music_core::Pitch::new(pitch)
                    {
                        match self.editor.insert_length(
                            track,
                            self.position.max(0),
                            pitch,
                            self.score_note_length.max(1),
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

    fn begin_save_as(&mut self) {
        self.file_path_input = self
            .project_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| String::from("project.arranger"));
        self.file_mode = Some(FileMode::SaveAs);
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
                if ui.button("Cancel").clicked() {
                    self.file_mode = None;
                }
            });
        });
        if action {
            let path = std::path::PathBuf::from(self.file_path_input.trim());
            if path.as_os_str().is_empty() {
                self.load_error = Some(String::from("enter a file path"));
            } else if mode == FileMode::Open {
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
                        self.status =
                            Some(format!("exported {stems} stem(s) to {}", path.display()));
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
        if !open {
            self.file_mode = None;
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
            let seconds = player.frame() as f64 / f64::from(audio::SAMPLE_RATE);
            self.position = self.song().tempo_map.tick_at_seconds(seconds);
            self.playing = player.is_playing();
            if self.loop_enabled
                && self.playing
                && let Some(region) = self.loop_region
                && self.position >= region.1
            {
                self.position = region.0;
                if let Some(buffer) = self.loop_buffer.clone()
                    && let Ok(restart) = audio::player::Player::new_on(
                        buffer,
                        self.settings.audio_output.as_deref(),
                        (self.settings.audio_buffer_frames > 0)
                            .then_some(self.settings.audio_buffer_frames),
                    )
                {
                    restart.play();
                    self.player = Some(restart);
                }
                self.advance_launcher();
                context.request_repaint_after(Duration::from_millis(16));
                return;
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

    /// Commits a queued scene when the playhead crosses a bar line.
    fn advance_launcher(&mut self) {
        let bar = self.song().time_signature.bar_ticks().max(1);
        let bar_index = self.position.div_euclid(bar);
        if self.launcher.playing && bar_index != self.last_bar {
            self.last_bar = bar_index;
            if let Some(scene) = self.launcher.advance(bar_index * bar) {
                self.status = Some(format!("scene {}", scene + 1));
            }
        }
    }

    /// Loads and caches the selected audio track's waveform peaks.
    fn refresh_waveform(&mut self) {
        if self.view != View::Arrange {
            return;
        }
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
        for (id, path) in audio_tracks {
            if self
                .timeline_waveforms
                .get(&id)
                .is_some_and(|(cached, _)| *cached == path)
            {
                continue;
            }
            let source = std::path::Path::new(&path);
            let peaks = std::fs::read(source)
                .ok()
                .and_then(|bytes| {
                    if bytes.starts_with(b"RIFF") {
                        audio::wav::decode_mono(&bytes).ok()
                    } else {
                        audio::decode::decode_mono(source).ok()
                    }
                })
                .map(|decoded| audio::waveform::peaks(&decoded.samples, 240))
                .unwrap_or_default();
            self.timeline_waveforms.insert(id, (path, peaks));
        }
        let Some(track) = self
            .roll
            .selected
            .or_else(|| {
                self.song()
                    .tracks
                    .iter()
                    .find(|track| track.audio.is_some())
                    .map(|track| track.id)
            })
            .and_then(|id| self.song().tracks.iter().find(|track| track.id == id))
        else {
            self.waveform = None;
            return;
        };
        let Some(reference) = &track.audio else {
            self.waveform = None;
            return;
        };
        if self
            .waveform
            .as_ref()
            .is_some_and(|(id, path, _)| *id == track.id && *path == reference.path)
        {
            return;
        }
        let path = std::path::Path::new(&reference.path);
        let peaks = std::fs::read(path)
            .ok()
            .and_then(|bytes| {
                if bytes.starts_with(b"RIFF") {
                    audio::wav::decode_mono(&bytes).ok()
                } else {
                    audio::decode::decode_mono(path).ok()
                }
            })
            .map(|decoded| audio::waveform::peaks(&decoded.samples, 240))
            .unwrap_or_default();
        self.waveform = Some((track.id, reference.path.clone(), peaks));
    }

    fn draw_waveform(ui: &mut egui::Ui, peaks: &[audio::waveform::Peak], zoom: f32, position: f32) {
        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(ui.available_width(), 44.0),
            egui::Sense::hover(),
        );
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, theme::lane());
        if peaks.is_empty() {
            return;
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
    }

    /// Opens the configured keyboard once and applies mapped controls.
    fn poll_midi(&mut self) {
        if self.settings.device_profile.is_none()
            && let Some(output) = self.settings.midi_output.clone()
            && let Some(profile) = profile_for_port(&output, &self.device_profiles)
        {
            self.status = Some(format!("device profile {profile} from {output}"));
            self.settings.device_profile = Some(profile);
            let _ = self.settings.save(&crate::settings::Settings::path());
        }
        if self.midi_input.is_none() {
            if self.settings.auto_inputs {
                match midi_devices::devices::LiveInputs::open_all() {
                    Ok(inputs) => {
                        self.status =
                            Some(format!("listening to {} MIDI input(s)", inputs.count()));
                        self.midi_input = Some(inputs);
                    }
                    Err(error) => self.status = Some(format!("MIDI inputs: {error}")),
                }
            } else if let Some(name) = self.settings.keyboard.clone() {
                match midi_devices::devices::LiveInputs::open_named(&name) {
                    Ok(input) => {
                        self.midi_input = Some(input);
                        if self.settings.device_profile.is_none()
                            && let Some(profile) = profile_for_port(&name, &self.device_profiles)
                        {
                            self.status = Some(format!("device profile {profile} from {name}"));
                            self.settings.device_profile = Some(profile);
                            let _ = self.settings.save(&crate::settings::Settings::path());
                        }
                    }
                    Err(error) => {
                        self.status = Some(format!("MIDI input {name}: {error}"));
                        self.settings.keyboard = None;
                    }
                }
            }
        }
        let mut actions = Vec::new();
        let mut learned = None;
        let mut midi_activity = false;
        let mut detected_chord = None;
        let mut preview_keys = Vec::new();
        let mut score_note_ons = Vec::new();
        let mut score_chord_done = false;
        if let Some(input) = &self.midi_input {
            while let Some(message) = input.next() {
                midi_activity = true;
                match &message {
                    music_core::MidiMessage::NoteOn { key, velocity, .. }
                        if velocity.as_u8() > 0 =>
                    {
                        self.style_held[usize::from(key.as_u8())] = true;
                        if self.settings.kids_mode && self.settings.kids_keys_play {
                            preview_keys.push(key.as_u8());
                        }
                        if self.view == View::Score && self.score_entry {
                            score_note_ons.push(key.as_u8());
                        }
                    }
                    music_core::MidiMessage::NoteOff { key, .. }
                    | music_core::MidiMessage::NoteOn { key, .. } => {
                        self.style_held[usize::from(key.as_u8())] = false;
                        if self.view == View::Score
                            && self.score_entry
                            && !self.style_held.iter().any(|held| *held)
                        {
                            score_chord_done = true;
                        }
                    }
                    _ => {}
                }
                self.midi_chord = held_chord_name(&self.style_held);
                if self.style_file_open
                    && let Some(name) = &self.midi_chord
                    && *name != self.style_last_chord
                {
                    detected_chord = Some(name.clone());
                }
                let label = midi_label(&message);
                self.midi_recent.push_front(label.clone());
                self.midi_recent.truncate(16);
                self.midi_log.push_front((std::time::Instant::now(), label));
                while self
                    .midi_log
                    .back()
                    .is_some_and(|(at, _)| at.elapsed().as_secs() > 120)
                {
                    self.midi_log.pop_back();
                }
                if let music_core::MidiMessage::ControlChange {
                    controller, value, ..
                } = &message
                {
                    self.midi_cc_values[usize::from(controller.as_u8())] = *value;
                }
                if let Some(learn) = self.midi_learn {
                    if let Some((control, action)) = learn.capture(&message) {
                        learned = Some((control, action));
                    }
                    continue;
                }
                if let Some(action) = self.midi_map.action_for(&message) {
                    let value = midi_devices::map::value_of(&message);
                    actions.push((action, value));
                }
            }
        }
        if !score_note_ons.is_empty() {
            let track = self
                .score_track
                .or(self.roll.selected)
                .or_else(|| self.song().tracks.first().map(|track| track.id));
            if let Some(track) = track {
                let length = entry_length(self.score_note_length.max(1), self.score_dotted);
                for pitch in score_note_ons {
                    if let Ok(pitch) = music_core::Pitch::new(pitch) {
                        let _ = self.editor.insert_length(
                            track,
                            self.score_cursor.max(0),
                            pitch,
                            length,
                        );
                    }
                }
            }
        }
        if score_chord_done {
            let length = entry_length(self.score_note_length.max(1), self.score_dotted);
            self.score_cursor += length;
        }
        for pitch in preview_keys {
            self.preview_pitch(pitch);
        }
        if let Some(name) = detected_chord {
            self.style_last_chord = name.clone();
            self.queue_style_chord(&name);
        }
        if midi_activity && self.settings.kids_mode && self.view == View::Perform {
            self.kids_last_activity = Some(std::time::Instant::now());
            if !self.playing {
                self.toggle_playback();
            }
        }
        if let Some((control, action)) = learned {
            self.midi_map.bind(control, action);
            let text = self.midi_map.to_text();
            if let Err(error) = std::fs::write("lazy-arranger-midi-map.txt", text) {
                self.status = Some(format!("midi map: {error}"));
            } else {
                self.status = Some(format!("bound {} -> {}", control.label(), action.label()));
            }
            self.midi_learn = None;
        }
        for (action, value) in actions {
            self.apply_midi_action(action, value);
        }
    }

    /// Runs one mapped action.
    fn apply_midi_action(&mut self, action: midi_devices::map::Action, value: Option<u8>) {
        use midi_devices::map::Action;
        match action {
            Action::Play => {
                if !self.playing {
                    self.toggle_playback();
                }
            }
            Action::Stop => {
                if self.playing {
                    self.stop_playback();
                }
            }
            Action::Undo => {
                if !self.editor.undo() {
                    self.status = Some(String::from("nothing to undo"));
                }
            }
            Action::Scene(index) => {
                self.launcher.launch(index);
                self.status = Some(format!("scene {}", index + 1));
            }
            Action::Pad(note) => {
                let Some(pads) = &self.pad_state else {
                    self.status = Some(String::from("load a loop into Drum pads first"));
                    return;
                };
                let Some(pad) = pads.kit.pads.iter().find(|pad| pad.note == note).copied() else {
                    self.status = Some(format!("pad {note} is empty"));
                    return;
                };
                match audio::player::Player::new(pads.audio(pad)) {
                    Ok(player) => {
                        self.pad_player = Some(player);
                        self.status = Some(format!("pad {note}"));
                    }
                    Err(error) => self.status = Some(error),
                }
            }
            Action::TrackFader(track) => {
                let Some(value) = value else {
                    return;
                };
                let pan = self
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == track)
                    .map(|entry| entry.pan)
                    .unwrap_or(64);
                if let Err(error) = self.editor.set_track_gain(track, value.min(127), pan) {
                    self.status = Some(error);
                }
            }
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
                            | crate::library_view::Action::Import(index) => index,
                        };
                        let row = &browser.rows[index];
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

    fn assistant_history(&self) -> Vec<(String, String)> {
        self.assistant_log
            .iter()
            .filter_map(|line| {
                line.strip_prefix("> ").map_or_else(
                    || (!line.is_empty()).then(|| (String::from("assistant"), line.clone())),
                    |user| Some((String::from("user"), user.to_string())),
                )
            })
            .collect()
    }

    fn request_assistant(&mut self, prompt: String) {
        let config = crate::ai::Config {
            url: self.settings.ai_url.clone(),
            model: self.settings.ai_model.clone(),
            api_key: self.settings.ai_api_key.clone(),
        };
        let command = self.settings.assistant_command.clone();
        if !config.is_ready() && command.trim().is_empty() {
            return;
        }
        let history = self.assistant_history();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.ai_pending = Some(receiver);
        std::thread::spawn(move || {
            let result = if config.is_ready() {
                crate::ai::chat_with_history(&config, &history, &prompt)
            } else {
                crate::ai::local_chat(&command, &prompt)
            };
            let _ = sender.send(result);
        });
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

    fn poll_ai(&mut self) {
        let Some(receiver) = &self.ai_pending else {
            return;
        };
        match receiver.try_recv() {
            Ok(Ok(reply)) => {
                self.ai_pending = None;
                self.finish_ai_reply(reply);
            }
            Ok(Err(error)) => {
                self.ai_pending = None;
                self.assistant_log.push(format!("AI: {error}"));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.ai_pending = None;
            }
        }
    }

    fn finish_ai_reply(&mut self, reply: String) {
        let mut text = reply.clone();
        if let Some(command) = crate::ai::tool_command(&reply) {
            let label = crate::ai::tool_name(command).unwrap_or("tool").to_string();
            if crate::ai::changes_project(command) {
                self.assistant_proposal = Some(AssistantProposal {
                    command: command.to_string(),
                    label: label.clone(),
                });
                text = format!("Proposed: {label}. Apply or cancel below.");
            } else {
                self.selected_note = None;
                self.selected_clip = None;
                self.selected_clips.clear();
                let result = self.editor.assistant(command);
                text.push('\n');
                text.push_str(&result.text);
            }
        }
        if self.settings.speech
            && let Err(error) = crate::speech::speak_with(&text, &self.settings.speak_command)
        {
            self.status = Some(error);
        }
        self.assistant_log.push(text);
    }

    /// The local assistant panel.
    fn assistant_window(&mut self, context: &egui::Context) {
        if !self.assistant_open {
            return;
        }
        let mut open = self.assistant_open;
        let mut apply_proposal = false;
        let mut cancel_proposal = false;
        egui::Window::new("Assistant")
            .open(&mut open)
            .default_width(420.0)
            .show(context, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.assistant_log.is_empty() {
                            ui.label(
                                egui::RichText::new(
                                    "Ask for help, a song summary, undo, or run any tool:\n                                     tool add_track {\"name\":\"Bass\"}",
                                )
                                .size(10.0)
                                .color(theme::text_dim()),
                            );
                        }
                        for line in &self.assistant_log {
                            ui.label(egui::RichText::new(line).monospace().size(10.0));
                        }
                    });
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.assistant_input)
                            .hint_text("help")
                            .desired_width(320.0),
                    );
                    let enter = response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if ui
                        .button("Dictate")
                        .on_hover_text("Record 5 s and run the Whisper command from Settings")
                        .clicked()
                    {
                        if self.settings.speech_command.trim().is_empty() {
                            self.status = Some(String::from(
                                "set a Whisper command in Settings first",
                            ));
                        } else {
                            let command = self.settings.speech_command.clone();
                            let (sender, receiver) = std::sync::mpsc::channel();
                            self.dictation_pending = Some(receiver);
                            self.status = Some(String::from("listening for 5 s…"));
                            std::thread::spawn(move || {
                                let result = crate::dictate::record_to_wav(5.0)
                                    .and_then(|path| {
                                        let text =
                                            crate::dictate::transcribe(&path, &command);
                                        let _ = std::fs::remove_file(path);
                                        text
                                    });
                                let _ = sender.send(result);
                            });
                        }
                    }
                    if (ui.button("Send").clicked() || enter) && !self.assistant_input.is_empty() {
                        let input = self.assistant_input.trim().to_string();
                        self.assistant_log.push(format!("> {input}"));
                        self.assistant_input.clear();
                        let config = crate::ai::Config {
                            url: self.settings.ai_url.clone(),
                            model: self.settings.ai_model.clone(),
                            api_key: self.settings.ai_api_key.clone(),
                        };
                        if config.is_ready()
                            || !self.settings.assistant_command.trim().is_empty()
                        {
                            self.request_assistant(input);
                        } else {
                            if input == "undo" || input.starts_with("tool ") {
                                self.selected_note = None;
                                self.selected_clip = None;
                                self.selected_clips.clear();
                            }
                            let reply = self.editor.assistant(&input);
                            if self.settings.speech
                                && let Err(error) = crate::speech::speak_with(&reply.text, &self.settings.speak_command)
                            {
                                self.status = Some(error);
                            }
                            self.assistant_log.push(reply.text);
                        }
                    }
                });
                if let Some(proposal) = self.assistant_proposal.clone() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(format!("Proposed change: {}", proposal.label))
                                .size(11.0)
                                .strong(),
                        );
                        ui.label(
                            egui::RichText::new(&proposal.command)
                                .monospace()
                                .size(9.0)
                                .color(theme::text_dim()),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() {
                                apply_proposal = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_proposal = true;
                            }
                        });
                    });
                }
                if self.ai_pending.is_some() {
                    ui.label(
                        egui::RichText::new("Waiting for the AI endpoint…")
                            .size(10.0)
                            .color(theme::accent()),
                    );
                }
                if self.dictation_pending.is_some() {
                    ui.label(
                        egui::RichText::new("Listening and transcribing…")
                            .size(10.0)
                            .color(theme::accent()),
                    );
                }
                let mut speech = self.settings.speech;
                if ui.checkbox(&mut speech, "Speak replies").changed() {
                    self.settings.speech = speech;
                    let _ = self.settings.save(&crate::settings::Settings::path());
                }
                ui.label(
                    egui::RichText::new(
                        "With an AI endpoint or local assistant command set in Settings, replies come from that model or CLI; \
                         a `tool ...` reply runs one app tool. Otherwise local commands over \
                         the same tools are used. Speech out uses macOS say; speech input is not implemented.",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
            });
        if apply_proposal && let Some(proposal) = self.assistant_proposal.take() {
            self.selected_note = None;
            self.selected_clip = None;
            self.selected_clips.clear();
            let result = self.editor.assistant(&proposal.command);
            self.assistant_log.push(result.text);
        }
        if cancel_proposal && self.assistant_proposal.take().is_some() {
            self.assistant_log
                .push(String::from("proposal cancelled; no change"));
        }
        if !open {
            self.assistant_open = false;
        }
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
        let Some(track) = self
            .score_track
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
        let grand_staff = self.score_grand
            || self
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
        let pdf = if self.score_preview {
            match self
                .song()
                .tracks
                .iter()
                .position(|entry| entry.id == track)
            {
                Some(index) => {
                    match score::cecilia::render_pdf(self.song(), index, &title, self.score_grand) {
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
        let path =
            std::env::temp_dir().join(format!("lazy-arranger-score-{}.pdf", std::process::id()));
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

    fn start_plugin_scan(&mut self) {
        if self.plugin_scanning {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let blocklist_path = std::path::Path::new(plugins::BLOCKLIST_FILE);
            let mut blocked = plugins::load_blocklist(blocklist_path);
            let before = blocked.len();
            let report = plugins::scan_installed_report(&blocked);
            for path in &report.failed {
                if !blocked.contains(path) {
                    blocked.push(path.clone());
                }
            }
            if blocked.len() > before {
                let _ = plugins::save_blocklist(blocklist_path, &blocked);
            }
            let _ = sender.send((report.plugins, blocked.len().saturating_sub(before)));
        });
        self.plugin_scan_rx = Some(receiver);
        self.plugin_scanning = true;
        self.plugin_error = None;
    }

    fn plugins_window(&mut self, context: &egui::Context) {
        if !self.plugins_open {
            return;
        }
        if self.plugin_scan.is_none() {
            self.plugin_scan = plugins::load_catalog(std::path::Path::new(plugins::CATALOG_FILE));
            if self.plugin_scan.is_none() {
                self.start_plugin_scan();
            }
        }
        let mut open = self.plugins_open;
        let mut add = None;
        egui::Window::new("My Plug-Ins")
            .open(&mut open)
            .default_width(520.0)
            .default_height(420.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("My Plug-Ins")
                            .size(20.0)
                            .strong(),
                    );
                    ui.add_space(10.0);
                    if ui
                        .add_enabled(!self.plugin_scanning, egui::Button::new("Rescan"))
                        .clicked()
                    {
                        self.start_plugin_scan();
                    }
                    if self.plugin_scanning {
                        ui.label(
                            egui::RichText::new("Scanning…")
                                .size(10.0)
                                .color(theme::text_muted()),
                        );
                    }
                    let blocked = plugins::load_blocklist(std::path::Path::new(
                        plugins::BLOCKLIST_FILE,
                    ));
                    if !blocked.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("{} blocked", blocked.len()))
                                .size(10.0)
                                .color(theme::text_dim()),
                        );
                        if ui.small_button("Clear blocklist").clicked() {
                            let _ = std::fs::remove_file(plugins::BLOCKLIST_FILE);
                            self.plugin_error = None;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.plugin_search)
                            .hint_text("Search plug-ins")
                            .desired_width(240.0),
                    );
                    ui.selectable_value(&mut self.plugin_filter, None, "All");
                    ui.selectable_value(
                        &mut self.plugin_filter,
                        Some(plugins::Format::AudioUnit),
                        "AU",
                    );
                    ui.selectable_value(
                        &mut self.plugin_filter,
                        Some(plugins::Format::Vst3),
                        "VST3",
                    );
                });
                ui.label(
                    egui::RichText::new(
                        "Installed bundles are listed here. Hosting and editors are not implemented.",
                    )
                    .size(10.0)
                    .color(theme::text_dim()),
                );
                ui.separator();
                let scan = self.plugin_scan.clone().unwrap_or_default();
                ui.label(
                    egui::RichText::new(format!(
                        "{} plug-in(s) · cache {}",
                        scan.len(),
                        plugins::CATALOG_FILE
                    ))
                    .size(10.0)
                    .color(theme::text_dim()),
                );
                if scan.is_empty() {
                    ui.label("no plug-in bundles found");
                }
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for plugin in scan
                            .iter()
                            .filter(|plugin| {
                                plugin_matches(
                                    plugin,
                                    &self.plugin_search,
                                    self.plugin_filter,
                                )
                            })
                        {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_sized(
                                        [260.0, 28.0],
                                        egui::Button::new(egui::RichText::new(&plugin.name).size(13.0)),
                                    )
                                    .on_hover_text(plugin.path.display().to_string())
                                    .clicked()
                                {
                                    add = Some(plugin.clone());
                                }
                                ui.label(plugin.format.label());
                                ui.label(
                                    egui::RichText::new(match plugin.role {
                                        plugins::Role::Instrument => "instrument",
                                        plugins::Role::Effect => "effect",
                                        plugins::Role::Unknown => "",
                                    })
                                    .size(10.0)
                                    .color(theme::text_muted()),
                                );
                                if !plugin.vendor.is_empty() {
                                    ui.label(
                                        egui::RichText::new(&plugin.vendor)
                                            .size(10.0)
                                            .color(theme::text_dim()),
                                    );
                                }
                            });
                        }
                    });
            });
        self.plugins_open = open;
        if let Some(plugin) = add {
            let track = self
                .roll
                .selected
                .or_else(|| self.song().tracks.first().map(|track| track.id));
            if let Some(track) = track {
                let format = match plugin.format {
                    plugins::Format::Vst3 => project::song::PluginFormat::Vst3,
                    plugins::Format::AudioUnit => project::song::PluginFormat::AudioUnit,
                };
                match self.editor.add_plugin(
                    track,
                    &plugin.name,
                    format,
                    &plugin.path.display().to_string(),
                    &plugin.identifier,
                ) {
                    Ok(id) => {
                        self.status = Some(format!(
                            "added {} to track {track} (slot {id})",
                            plugin.name
                        ));
                    }
                    Err(error) => self.status = Some(error),
                }
            } else {
                self.status = Some(String::from("select a track first"));
            }
        }
    }

    fn bottom_mixer(&mut self, context: &Context) {
        if self.settings.kids_mode || !self.mixer_visible {
            return;
        }
        let tracks = self.song().tracks.clone();
        let buses = self.song().buses.clone();
        let mut edits: Vec<(TrackId, u8, u8, bool, bool, u8, bool)> = Vec::new();
        let mut bus_edits: Vec<(u8, u8, u8, bool, bool)> = Vec::new();
        let bus_levels: Vec<f32> = buses
            .iter()
            .map(|bus| {
                if !self.playing {
                    return 0.0;
                }
                tracks
                    .iter()
                    .filter(|track| track.bus == Some(bus.id) || track.send == Some(bus.id))
                    .map(|track| {
                        let level = track_level(track, self.position);
                        if track.bus == Some(bus.id) {
                            level
                        } else {
                            level * f32::from(track.send_level) / 127.0
                        }
                    })
                    .sum::<f32>()
                    .clamp(0.0, 1.0)
            })
            .collect();
        let mut master_gain = self.settings.master_gain_db;
        egui::TopBottomPanel::bottom("bottom-mixer")
            .resizable(true)
            .default_height(236.0)
            .height_range(120.0..=420.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(context, |ui| {
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        for (index, track) in tracks.iter().enumerate() {
                            let color = crate::timeline::lane_color(index);
                            let mut volume = f32::from(track.volume);
                            let mut pan = f32::from(track.pan);
                            let mut send = f32::from(track.send_level);
                            let mut muted = track.muted;
                            let mut soloed = track.soloed;
                            let mut armed = track.armed;
                            ui.allocate_ui(egui::vec2(108.0, 220.0), |ui| {
                                ui.vertical(|ui| {
                                    painter_strip(ui, color);
                                    ui.label(
                                        egui::RichText::new(if track.name.is_empty() {
                                            "untitled"
                                        } else {
                                            &track.name
                                        })
                                        .size(11.0)
                                        .strong(),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            if track.name.to_ascii_lowercase().contains("drum") {
                                                "Electronic Kit"
                                            } else {
                                                "Built-in SoundFont"
                                            },
                                        )
                                        .size(9.0)
                                        .color(theme::text_dim()),
                                    );
                                    ui.horizontal(|ui| {
                                        let rev =
                                            mixer_knob(ui, &mut send, 0.0, 127.0, "Rev", true);
                                        ui.add_space(28.0);
                                        if rev {
                                            edits.push((
                                                track.id,
                                                track.volume,
                                                track.pan,
                                                muted,
                                                soloed,
                                                u8::try_from(send.round() as i32)
                                                    .unwrap_or(track.send_level),
                                                armed,
                                            ));
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        let label = pan_label(pan);
                                        let changed =
                                            mixer_knob(ui, &mut pan, 0.0, 127.0, &label, true);
                                        if changed {
                                            edits.push((
                                                track.id,
                                                track.volume,
                                                u8::try_from(pan.round() as i32)
                                                    .unwrap_or(track.pan),
                                                muted,
                                                soloed,
                                                track.send_level,
                                                armed,
                                            ));
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.small_button("M").clicked() {
                                            muted = !muted;
                                            edits.push((
                                                track.id,
                                                track.volume,
                                                track.pan,
                                                muted,
                                                soloed,
                                                track.send_level,
                                                armed,
                                            ));
                                        }
                                        if ui.small_button("S").clicked() {
                                            soloed = !soloed;
                                            edits.push((
                                                track.id,
                                                track.volume,
                                                track.pan,
                                                muted,
                                                soloed,
                                                track.send_level,
                                                armed,
                                            ));
                                        }
                                        if ui.small_button("R").clicked() {
                                            armed = !armed;
                                            edits.push((
                                                track.id,
                                                track.volume,
                                                track.pan,
                                                muted,
                                                soloed,
                                                track.send_level,
                                                armed,
                                            ));
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui(egui::vec2(42.0, 88.0), |ui| {
                                            if mixer_fader(
                                                ui,
                                                &mut volume,
                                                0.0,
                                                127.0,
                                                88.0,
                                                color,
                                                "Volume",
                                                100.0,
                                            ) {
                                                edits.push((
                                                    track.id,
                                                    u8::try_from(volume.round() as i32)
                                                        .unwrap_or(track.volume),
                                                    track.pan,
                                                    muted,
                                                    soloed,
                                                    track.send_level,
                                                    armed,
                                                ));
                                            }
                                        });
                                        let (meter, _) = ui.allocate_exact_size(
                                            egui::vec2(5.0, 82.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(meter, 1.0, theme::raised());
                                        let level = if self.playing {
                                            track_level(track, self.position)
                                        } else {
                                            0.0
                                        };
                                        let fill = egui::Rect::from_min_max(
                                            egui::pos2(
                                                meter.left(),
                                                meter.bottom() - meter.height() * level,
                                            ),
                                            meter.max,
                                        );
                                        ui.painter().rect_filled(fill, 1.0, color);
                                    });
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{:.1} dB",
                                            f32::from(track.volume) - 100.0
                                        ))
                                        .size(9.0)
                                        .color(theme::text_muted()),
                                    );
                                });
                            });
                        }
                        for (bus_index, bus) in buses.iter().enumerate() {
                            let mut volume = f32::from(bus.volume);
                            let mut pan = f32::from(bus.pan);
                            let mut bus_volume = bus.volume;
                            let mut bus_pan = bus.pan;
                            let mut bus_muted = bus.muted;
                            let mut bus_soloed = bus.soloed;
                            ui.allocate_ui(egui::vec2(96.0, 220.0), |ui| {
                                ui.vertical(|ui| {
                                    painter_strip(ui, theme::accent());
                                    ui.label(egui::RichText::new(&bus.name).size(11.0).strong());
                                    ui.label(
                                        egui::RichText::new("Bus")
                                            .size(9.0)
                                            .color(theme::text_dim()),
                                    );
                                    ui.horizontal(|ui| {
                                        let pan_text = pan_label(pan);
                                        if mixer_knob(ui, &mut pan, 0.0, 127.0, &pan_text, true) {
                                            bus_pan =
                                                u8::try_from(pan.round() as i32).unwrap_or(bus.pan);
                                            bus_edits.push((
                                                bus.id, bus_volume, bus_pan, bus_muted, bus_soloed,
                                            ));
                                        }
                                    });
                                    ui.add_space(18.0);
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui(egui::vec2(42.0, 88.0), |ui| {
                                            if mixer_fader(
                                                ui,
                                                &mut volume,
                                                0.0,
                                                127.0,
                                                88.0,
                                                theme::accent(),
                                                "Bus volume",
                                                100.0,
                                            ) {
                                                bus_volume = u8::try_from(volume.round() as i32)
                                                    .unwrap_or(bus.volume);
                                                bus_edits.push((
                                                    bus.id, bus_volume, bus_pan, bus_muted,
                                                    bus_soloed,
                                                ));
                                            }
                                        });
                                        let (meter, _) = ui.allocate_exact_size(
                                            egui::vec2(5.0, 82.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(meter, 1.0, theme::raised());
                                        let level =
                                            bus_levels.get(bus_index).copied().unwrap_or_default();
                                        let fill = egui::Rect::from_min_max(
                                            egui::pos2(
                                                meter.left(),
                                                meter.bottom() - meter.height() * level,
                                            ),
                                            meter.max,
                                        );
                                        ui.painter().rect_filled(fill, 1.0, theme::accent());
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.small_button("M").clicked() {
                                            bus_muted = !bus_muted;
                                            bus_edits.push((
                                                bus.id, bus_volume, bus_pan, bus_muted, bus_soloed,
                                            ));
                                        }
                                        if ui.small_button("S").clicked() {
                                            bus_soloed = !bus_soloed;
                                            bus_edits.push((
                                                bus.id, bus_volume, bus_pan, bus_muted, bus_soloed,
                                            ));
                                        }
                                    });
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{:.1} dB",
                                            f32::from(bus.volume) - 100.0
                                        ))
                                        .size(9.0)
                                        .color(theme::text_muted()),
                                    );
                                });
                            });
                        }
                        let master_level = self.player.as_ref().map_or_else(
                            || {
                                if self.playing {
                                    self.song()
                                        .tracks
                                        .iter()
                                        .map(|track| track_level(track, self.position))
                                        .fold(0.0_f32, f32::max)
                                } else {
                                    0.0
                                }
                            },
                            |player| player.meter().peak.clamp(0.0, 1.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.allocate_ui(egui::vec2(110.0, 220.0), |ui| {
                                ui.vertical(|ui| {
                                    painter_strip(ui, theme::text_muted());
                                    ui.label(egui::RichText::new("Master").size(11.0).strong());
                                    ui.label(
                                        egui::RichText::new(if self.player.is_some() {
                                            "cpal"
                                        } else {
                                            "offline"
                                        })
                                        .size(9.0)
                                        .color(theme::text_dim()),
                                    );
                                    ui.add_space(62.0);
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui(egui::vec2(42.0, 88.0), |ui| {
                                            mixer_fader(
                                                ui,
                                                &mut master_gain,
                                                -24.0,
                                                12.0,
                                                88.0,
                                                theme::accent(),
                                                "Master",
                                                0.0,
                                            );
                                        });
                                        let (meter, _) = ui.allocate_exact_size(
                                            egui::vec2(5.0, 82.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(meter, 1.0, theme::raised());
                                        let fill = egui::Rect::from_min_max(
                                            egui::pos2(
                                                meter.left(),
                                                meter.bottom() - meter.height() * master_level,
                                            ),
                                            meter.max,
                                        );
                                        ui.painter().rect_filled(fill, 1.0, theme::accent());
                                    });
                                    ui.label(
                                        egui::RichText::new(format!("{master_gain:.1} dB"))
                                            .size(9.0)
                                            .color(theme::text_muted()),
                                    );
                                });
                            });
                        });
                    });
                });
            });
        self.settings.master_gain_db = master_gain;
        if !bus_edits.is_empty() {
            let mut updated = self.song().buses.clone();
            for (id, volume, pan, muted, soloed) in &bus_edits {
                if let Some(bus) = updated.iter_mut().find(|bus| bus.id == *id) {
                    bus.volume = *volume;
                    bus.pan = *pan;
                    bus.muted = *muted;
                    bus.soloed = *soloed;
                }
            }
            match self.editor.set_buses(updated) {
                Ok(()) => self.status = Some(String::from("bus levels updated")),
                Err(error) => self.status = Some(error),
            }
        }
        for (id, volume, pan, muted, soloed, send_level, armed) in edits {
            let _ = self.editor.set_track_gain(id, volume, pan);
            let _ = self.editor.set_track_flags(id, muted, soloed, armed);
            if let Some(track) = self.song().tracks.iter().find(|track| track.id == id) {
                let _ = self
                    .editor
                    .set_track_routing(id, track.bus, track.send, send_level);
            }
        }
    }

    fn send_chord_notes(
        &mut self,
        chord: &music_core::Chord,
        channel: music_core::Channel,
        velocity: music_core::Velocity,
        hold_ms: u64,
    ) {
        let root = 60 + i32::from(chord.root.as_u8());
        let mut pitches = Vec::new();
        for interval in chord.quality.intervals() {
            if let Ok(pitch) =
                music_core::Pitch::new(u8::try_from(root + i32::from(*interval)).unwrap_or(60))
            {
                self.send_midi(music_core::MidiMessage::note_on(channel, pitch, velocity));
                pitches.push(pitch);
            }
        }
        let port = self.midi_out_port.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(hold_ms));
            if let Some(port) = port
                && let Ok(mut connection) = midi_devices::devices::open_output(&port)
                && let Ok(release) = music_core::Velocity::new(0)
            {
                for pitch in pitches {
                    let bytes = music_core::MidiMessage::note_off(channel, pitch, release);
                    let _ = connection.send(&bytes.to_bytes()[..3]);
                }
            }
        });
    }

    fn psr_send_state(&mut self) {
        let Ok(channel) = music_core::Channel::from_one_based(self.psr_channel) else {
            return;
        };
        if let (Ok(msb), Ok(lsb)) = (
            music_core::ControlNumber::new(0),
            music_core::ControlNumber::new(32),
        ) {
            self.send_midi(music_core::MidiMessage::ControlChange {
                channel,
                controller: msb,
                value: self.psr_bank_msb,
            });
            self.send_midi(music_core::MidiMessage::ControlChange {
                channel,
                controller: lsb,
                value: self.psr_bank_lsb,
            });
        }
        self.send_midi(music_core::MidiMessage::ProgramChange {
            channel,
            program: self.psr_program,
        });
        for (value, controller) in [
            (self.psr_volume, 7_u8),
            (self.psr_pan, 10),
            (self.psr_reverb, 91),
            (self.psr_chorus, 93),
        ] {
            if let Ok(control) = music_core::ControlNumber::new(controller) {
                self.send_midi(music_core::MidiMessage::ControlChange {
                    channel,
                    controller: control,
                    value,
                });
            }
        }
    }

    fn psr_store(&mut self, index: usize) {
        if index >= self.psr_regs.len() {
            return;
        }
        self.psr_regs[index] = PsrRegistration {
            channel: self.psr_channel,
            bank_msb: self.psr_bank_msb,
            bank_lsb: self.psr_bank_lsb,
            program: self.psr_program,
            volume: self.psr_volume,
            pan: self.psr_pan,
            reverb: self.psr_reverb,
            chorus: self.psr_chorus,
        };
        save_psr_regs(&self.psr_regs);
        self.status = Some(format!("PSR registration {}", index + 1));
    }

    fn psr_recall(&mut self, index: usize) {
        let Some(reg) = self.psr_regs.get(index).copied() else {
            return;
        };
        self.psr_channel = reg.channel.max(1);
        self.psr_bank_msb = reg.bank_msb;
        self.psr_bank_lsb = reg.bank_lsb;
        self.psr_program = reg.program;
        self.psr_volume = reg.volume;
        self.psr_pan = reg.pan;
        self.psr_reverb = reg.reverb;
        self.psr_chorus = reg.chorus;
        self.psr_send_state();
    }

    fn xg_window(&mut self, context: &Context) {
        if !self.xg_open {
            return;
        }
        let mut open = self.xg_open;
        let mut send_all = false;
        let mut reset = false;
        let mut sends: Vec<(usize, u8, u8)> = Vec::new();
        egui::Window::new("XG part mixer")
            .open(&mut open)
            .resizable(true)
            .default_width(520.0)
            .show(context, |ui| {
                ui.label(
                    egui::RichText::new(
                        "16 MIDI parts · volume / pan / reverb / chorus · uses the selected MIDI output",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
                ui.horizontal(|ui| {
                    if ui.button("Send all").clicked() {
                        send_all = true;
                    }
                    if ui.button("Reset").clicked() {
                        reset = true;
                    }
                });
                for (index, values) in self.xg_parts.iter_mut().enumerate() {
                    let mut current = *values;
                    ui.horizontal(|ui| {
                        ui.label(format!("Part {:>2}", index + 1));
                        for (label, slot, cc) in [
                            ("vol", 0, 7_u8),
                            ("pan", 1, 10),
                            ("rev", 2, 91),
                            ("cho", 3, 93),
                        ] {
                            let mut value = current[slot];
                            if ui
                                .add(egui::Slider::new(&mut value, 0..=127).text(label))
                                .changed()
                            {
                                current[slot] = value;
                                sends.push((index, cc, value));
                            }
                        }
                    });
                    *values = current;
                }
            });
        for (part, cc, value) in sends {
            if let (Ok(channel), Ok(controller)) = (
                music_core::Channel::from_one_based(u8::try_from(part + 1).unwrap_or(1)),
                music_core::ControlNumber::new(cc),
            ) {
                self.send_midi(music_core::MidiMessage::ControlChange {
                    channel,
                    controller,
                    value,
                });
            }
        }
        if reset {
            self.xg_parts = [[100, 64, 40, 0]; 16];
        }
        if send_all {
            let parts = self.xg_parts;
            for (index, values) in parts.iter().enumerate() {
                let Ok(channel) =
                    music_core::Channel::from_one_based(u8::try_from(index + 1).unwrap_or(1))
                else {
                    continue;
                };
                for (value, cc) in values.iter().zip([7_u8, 10, 91, 93]) {
                    if let Ok(controller) = music_core::ControlNumber::new(cc) {
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller,
                            value: *value,
                        });
                    }
                }
            }
            self.status = Some(String::from("XG part mixer sent"));
        }
        self.xg_open = open;
    }

    fn psr_window(&mut self, context: &Context) {
        if !self.psr_open {
            return;
        }
        let mut open = self.psr_open;
        egui::Window::new("PSR-S970 remote")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                ui.label(
                    egui::RichText::new("Yamaha PSR-S970 · MIDI channel 1")
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                ui.horizontal(|ui| {
                    ui.label("bank");
                    ui.add(egui::DragValue::new(&mut self.psr_bank_msb).range(0..=127));
                    ui.label("/");
                    ui.add(egui::DragValue::new(&mut self.psr_bank_lsb).range(0..=127));
                    ui.label("program");
                    ui.add(egui::DragValue::new(&mut self.psr_program).range(0..=127));
                    if ui.button("Send voice").clicked()
                        && let Ok(channel) = music_core::Channel::from_one_based(self.psr_channel)
                        && let Ok(msb) = music_core::ControlNumber::new(0)
                        && let Ok(lsb) = music_core::ControlNumber::new(32)
                    {
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller: msb,
                            value: self.psr_bank_msb,
                        });
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller: lsb,
                            value: self.psr_bank_lsb,
                        });
                        self.send_midi(music_core::MidiMessage::ProgramChange {
                            channel,
                            program: self.psr_program,
                        });
                    }
                });
                for (label, controller, current) in [
                    ("Volume", 7_u8, self.psr_volume),
                    ("Pan", 10, self.psr_pan),
                    ("Reverb", 91, self.psr_reverb),
                    ("Chorus", 93, self.psr_chorus),
                ] {
                    let mut value = current;
                    let changed = ui
                        .horizontal(|ui| {
                            ui.label(label);
                            ui.add(egui::Slider::new(&mut value, 0..=127).show_value(true))
                                .changed()
                        })
                        .inner;
                    if changed {
                        match controller {
                            7 => self.psr_volume = value,
                            10 => self.psr_pan = value,
                            91 => self.psr_reverb = value,
                            _ => self.psr_chorus = value,
                        }
                        if let (Ok(channel), Ok(control)) = (
                            music_core::Channel::from_one_based(self.psr_channel),
                            music_core::ControlNumber::new(controller),
                        ) {
                            self.send_midi(music_core::MidiMessage::ControlChange {
                                channel,
                                controller: control,
                                value,
                            });
                        }
                    }
                }
                ui.separator();
                ui.label(egui::RichText::new("Registrations").strong());
                ui.horizontal(|ui| {
                    for index in 0..8 {
                        let response =
                            ui.add_sized([34.0, 30.0], egui::Button::new((index + 1).to_string()));
                        if response.clicked() {
                            self.psr_recall(index);
                        }
                        if response.secondary_clicked() {
                            self.psr_store(index);
                        }
                        response.on_hover_text("Click to recall; right-click to store");
                    }
                });
                if ui
                    .button("Store current in slot 1")
                    .on_hover_text("Right-click a number above to store in that slot")
                    .clicked()
                {
                    self.psr_store(0);
                }
                ui.separator();
                ui.label(egui::RichText::new("Chord audition").strong());
                for row in [["C", "Dm", "Em", "F"], ["G", "Am", "G7", "D7"]] {
                    ui.horizontal(|ui| {
                        for name in row {
                            if ui
                                .add_sized([70.0, 34.0], egui::Button::new(name))
                                .clicked()
                                && let Ok(chord) = music_core::Chord::parse(name)
                                && let Ok(channel) =
                                    music_core::Channel::from_one_based(self.psr_channel)
                                && let Ok(velocity) = music_core::Velocity::new(100)
                            {
                                self.send_chord_notes(&chord, channel, velocity, 500);
                            }
                        }
                    });
                }
                ui.label(
                    egui::RichText::new("Choose an output in Windows > MIDI out.")
                        .size(9.0)
                        .color(theme::text_dim()),
                );
            });
        self.psr_open = open;
    }

    fn send_raw(&mut self, bytes: &[u8]) {
        let Some(port) = self.midi_out_port.clone() else {
            self.status = Some(String::from("choose a MIDI output first"));
            return;
        };
        match midi_devices::devices::open_output(&port) {
            Ok(mut connection) => match connection.send(bytes) {
                Ok(()) => self.status = Some(format!("sent {} byte(s) to {port}", bytes.len())),
                Err(error) => self.status = Some(format!("{port}: {error}")),
            },
            Err(error) => self.status = Some(error.to_string()),
        }
    }

    fn send_midi(&mut self, message: music_core::MidiMessage) {
        let Some(port) = self.midi_out_port.clone() else {
            self.status = Some(String::from("choose a MIDI output first"));
            return;
        };
        match midi_devices::devices::open_output(&port) {
            Ok(mut connection) => {
                let bytes = message.to_bytes();
                match connection.send(&bytes[..message.encoded_len()]) {
                    Ok(()) => self.status = Some(format!("sent to {port}")),
                    Err(error) => self.status = Some(format!("{port}: {error}")),
                }
            }
            Err(error) => self.status = Some(error.to_string()),
        }
    }

    fn midi_monitor_window(&mut self, context: &Context) {
        if !self.midi_monitor_open {
            return;
        }
        let mut open = self.midi_monitor_open;
        egui::Window::new("MIDI monitor")
            .open(&mut open)
            .resizable(true)
            .default_width(320.0)
            .show(context, |ui| {
                if ui.button("Clear").clicked() {
                    self.midi_log.clear();
                    self.midi_recent.clear();
                }
                ui.label(
                    egui::RichText::new("Last two minutes of input messages")
                        .size(9.0)
                        .color(theme::text_dim()),
                );
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        if self.midi_log.is_empty() {
                            ui.label("No MIDI messages yet.");
                        }
                        for (at, label) in &self.midi_log {
                            ui.label(format!("{:>6.1}s  {label}", at.elapsed().as_secs_f64()));
                        }
                    });
            });
        self.midi_monitor_open = open;
    }

    fn midi_out_window(&mut self, context: &Context) {
        if !self.midi_out_open {
            return;
        }
        let mut open = self.midi_out_open;
        egui::Window::new("MIDI out")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                if self.midi_ports.is_none()
                    && let Ok(ports) = midi_devices::devices::list_ports()
                {
                    self.midi_ports = Some((ports.inputs, ports.outputs));
                }
                let outputs = self
                    .midi_ports
                    .as_ref()
                    .map(|(_, outputs)| outputs.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("midi-out-port")
                    .selected_text(self.midi_out_port.as_deref().unwrap_or("Choose output"))
                    .show_ui(ui, |ui| {
                        for name in &outputs {
                            ui.selectable_value(&mut self.midi_out_port, Some(name.clone()), name);
                        }
                    });
                if ui
                    .button("Channel check")
                    .on_hover_text("Play a short note on each of the 16 channels")
                    .clicked()
                {
                    if let Some(port) = self.midi_out_port.clone() {
                        self.status = Some(format!("channel check on {port}"));
                        std::thread::spawn(move || {
                            if let Ok(mut connection) = midi_devices::devices::open_output(&port) {
                                for channel in 0..16 {
                                    if let Ok(messages) =
                                        midi_devices::devices::channel_check_messages(channel, 60)
                                    {
                                        for bytes in messages {
                                            let _ = connection.send(&bytes);
                                        }
                                    }
                                }
                            }
                        });
                    } else {
                        self.status = Some(String::from("choose a MIDI output first"));
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("channel");
                    ui.add(egui::DragValue::new(&mut self.midi_out_channel).range(1..=16));
                    ui.label("program");
                    ui.add(egui::DragValue::new(&mut self.midi_out_program).range(0..=127));
                    if ui.button("Send program").clicked()
                        && let Ok(channel) =
                            music_core::Channel::from_one_based(self.midi_out_channel)
                    {
                        self.send_midi(music_core::MidiMessage::ProgramChange {
                            channel,
                            program: self.midi_out_program,
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("bank");
                    ui.add(egui::DragValue::new(&mut self.midi_out_bank_msb).range(0..=127));
                    ui.label("/");
                    ui.add(egui::DragValue::new(&mut self.midi_out_bank_lsb).range(0..=127));
                    if ui.button("Send bank").clicked()
                        && let (Ok(channel), Ok(msb), Ok(lsb)) = (
                            music_core::Channel::from_one_based(self.midi_out_channel),
                            music_core::ControlNumber::new(0),
                            music_core::ControlNumber::new(32),
                        )
                    {
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller: msb,
                            value: self.midi_out_bank_msb,
                        });
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller: lsb,
                            value: self.midi_out_bank_lsb,
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("CC");
                    ui.add(egui::DragValue::new(&mut self.midi_out_cc).range(0..=127));
                    ui.add(egui::DragValue::new(&mut self.midi_out_cc_value).range(0..=127));
                    if ui.button("Send CC").clicked()
                        && let (Ok(channel), Ok(controller)) = (
                            music_core::Channel::from_one_based(self.midi_out_channel),
                            music_core::ControlNumber::new(self.midi_out_cc),
                        )
                    {
                        self.send_midi(music_core::MidiMessage::ControlChange {
                            channel,
                            controller,
                            value: self.midi_out_cc_value,
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("note");
                    ui.add(egui::DragValue::new(&mut self.midi_out_note).range(0..=127));
                    ui.label("velocity");
                    ui.add(egui::DragValue::new(&mut self.midi_out_velocity).range(1..=127));
                    if ui.button("Send note").clicked()
                        && let (Ok(channel), Ok(pitch), Ok(velocity)) = (
                            music_core::Channel::from_one_based(self.midi_out_channel),
                            music_core::Pitch::new(self.midi_out_note),
                            music_core::Velocity::new(self.midi_out_velocity),
                        )
                    {
                        self.send_midi(music_core::MidiMessage::note_on(channel, pitch, velocity));
                        let port = self.midi_out_port.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(350));
                            if let Some(port) = port
                                && let Ok(mut connection) =
                                    midi_devices::devices::open_output(&port)
                            {
                                let bytes = music_core::MidiMessage::note_off(
                                    channel,
                                    pitch,
                                    music_core::Velocity::new(0).unwrap_or(velocity),
                                );
                                let _ = connection.send(&bytes.to_bytes()[..3]);
                            }
                        });
                    }
                    if ui.button("Send chord").clicked()
                        && let Some(chord) = self.song().chord_at(self.position)
                        && let Ok(channel) =
                            music_core::Channel::from_one_based(self.midi_out_channel)
                        && let Some(port) = self.midi_out_port.clone()
                    {
                        let root = 60 + i32::from(chord.root.as_u8());
                        let mut pitches = Vec::new();
                        for interval in chord.quality.intervals() {
                            if let Ok(pitch) = music_core::Pitch::new(
                                u8::try_from(root + i32::from(*interval)).unwrap_or(60),
                            ) && let Ok(velocity) =
                                music_core::Velocity::new(self.midi_out_velocity)
                            {
                                self.send_midi(music_core::MidiMessage::note_on(
                                    channel, pitch, velocity,
                                ));
                                pitches.push(pitch);
                            }
                        }
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(500));
                            if let Ok(mut connection) = midi_devices::devices::open_output(&port)
                                && let Ok(release) = music_core::Velocity::new(0)
                            {
                                for pitch in pitches {
                                    let bytes =
                                        music_core::MidiMessage::note_off(channel, pitch, release);
                                    let _ = connection.send(&bytes.to_bytes()[..3]);
                                }
                            }
                        });
                    }
                    if ui.button("Panic").clicked()
                        && let Some(port) = self.midi_out_port.clone()
                    {
                        match midi_devices::devices::panic(&port) {
                            Ok(sent) => self.status = Some(format!("panic sent {sent}")),
                            Err(error) => self.status = Some(error.to_string()),
                        }
                    }
                    if ui.button("Start").clicked() {
                        self.send_raw(&[0xFA]);
                    }
                    if ui.button("Continue").clicked() {
                        self.send_raw(&[0xFB]);
                    }
                    if ui.button("Stop").clicked() {
                        self.send_raw(&[0xFC]);
                    }
                });
                if self.midi_out_port.is_none() {
                    ui.label(
                        egui::RichText::new("No output selected. Use Settings to list ports.")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
            });
        self.midi_out_open = open;
    }

    fn bus_fx_window(&mut self, context: &Context) {
        if !self.bus_fx_open {
            return;
        }
        let mut open = self.bus_fx_open;
        let mut buses = self.song().buses.clone();
        let catalog = self
            .plugin_scan
            .clone()
            .or_else(|| plugins::load_catalog(std::path::Path::new(plugins::CATALOG_FILE)))
            .unwrap_or_default();
        let mut changed = false;
        egui::Window::new("Bus effects")
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(context, |ui| {
                if buses.is_empty() {
                    ui.label("No buses. Add one in the Mixer.");
                    return;
                }
                self.bus_fx_choice = self.bus_fx_choice.min(buses.len() - 1);
                egui::ComboBox::from_id_salt("bus-fx-bus")
                    .selected_text(&buses[self.bus_fx_choice].name)
                    .show_ui(ui, |ui| {
                        for (index, bus) in buses.iter().enumerate() {
                            ui.selectable_value(&mut self.bus_fx_choice, index, &bus.name);
                        }
                    });
                let plugins = &mut buses[self.bus_fx_choice].plugins;
                if plugins.is_empty() {
                    ui.label("No effects on this bus.");
                }
                let mut remove = None;
                let mut move_up = None;
                for (index, slot) in plugins.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut slot.bypassed, "").changed() {
                            changed = true;
                        }
                        ui.label(&slot.name);
                        if index > 0 && ui.small_button("↑").clicked() {
                            move_up = Some(index);
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                        if slot.format == project::song::PluginFormat::Vst3
                            && !slot.identifier.is_empty()
                        {
                            if ui
                                .small_button("Capture")
                                .on_hover_text("Save the plug-in's current state")
                                .clicked()
                            {
                                match plugin_host::Vst3Effect::new(
                                    std::path::Path::new(&slot.path),
                                    &slot.identifier,
                                    48_000.0,
                                    512,
                                )
                                .and_then(|effect| effect.save_state_base64())
                                {
                                    Ok(state) => {
                                        slot.state = state;
                                        changed = true;
                                        self.status =
                                            Some(String::from("master plug-in state captured"));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                            if !slot.state.is_empty() && ui.small_button("Clear").clicked() {
                                slot.state.clear();
                                changed = true;
                            }
                        }
                    });
                }
                if let Some(index) = move_up {
                    plugins.swap(index, index - 1);
                    changed = true;
                }
                if let Some(index) = remove {
                    plugins.remove(index);
                    changed = true;
                }
                egui::ComboBox::from_id_salt("bus-fx-plugin")
                    .selected_text(
                        catalog
                            .get(self.bus_fx_plugin_choice)
                            .map_or("No cached plug-ins", |plugin| plugin.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for (index, plugin) in catalog.iter().enumerate().take(20) {
                            ui.selectable_value(
                                &mut self.bus_fx_plugin_choice,
                                index,
                                &plugin.name,
                            );
                        }
                    });
                if ui.button("Add plug-in").clicked()
                    && let Some(plugin) = catalog.get(self.bus_fx_plugin_choice)
                {
                    let id = plugins.iter().map(|slot| slot.id).max().unwrap_or(0) + 1;
                    plugins.push(project::song::PluginSlot {
                        id,
                        name: plugin.name.clone(),
                        format: match plugin.format {
                            plugins::Format::Vst3 => project::song::PluginFormat::Vst3,
                            plugins::Format::AudioUnit => project::song::PluginFormat::AudioUnit,
                        },
                        path: plugin.path.display().to_string(),
                        identifier: plugin.identifier.clone(),
                        bypassed: false,
                        state: String::new(),
                    });
                    changed = true;
                }
            });
        if changed {
            match self.editor.set_buses(buses) {
                Ok(()) => self.status = Some(String::from("bus effects updated")),
                Err(error) => self.status = Some(error),
            }
        }
        self.bus_fx_open = open;
    }

    fn master_fx_window(&mut self, context: &Context) {
        if !self.master_fx_open {
            return;
        }
        let mut open = self.master_fx_open;
        let mut changed = false;
        let mut plugins = self.song().master_plugins.clone();
        let catalog = self
            .plugin_scan
            .clone()
            .or_else(|| plugins::load_catalog(std::path::Path::new(plugins::CATALOG_FILE)))
            .unwrap_or_default();
        egui::Window::new("Master effects")
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(context, |ui| {
                if plugins.is_empty() {
                    ui.label("No master effects.");
                }
                let mut remove = None;
                let mut move_up = None;
                for (index, slot) in plugins.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut slot.bypassed, "").changed() {
                            changed = true;
                        }
                        ui.label(&slot.name);
                        if index > 0 && ui.small_button("↑").clicked() {
                            move_up = Some(index);
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                        if slot.format == project::song::PluginFormat::Vst3
                            && !slot.identifier.is_empty()
                        {
                            if ui
                                .small_button("Capture")
                                .on_hover_text("Save the plug-in's current state")
                                .clicked()
                            {
                                match plugin_host::Vst3Effect::new(
                                    std::path::Path::new(&slot.path),
                                    &slot.identifier,
                                    48_000.0,
                                    512,
                                )
                                .and_then(|effect| effect.save_state_base64())
                                {
                                    Ok(state) => {
                                        slot.state = state;
                                        changed = true;
                                        self.status =
                                            Some(String::from("bus plug-in state captured"));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                            if !slot.state.is_empty() && ui.small_button("Clear").clicked() {
                                slot.state.clear();
                                changed = true;
                            }
                        }
                    });
                }
                if let Some(index) = move_up {
                    plugins.swap(index, index - 1);
                    changed = true;
                }
                if let Some(index) = remove {
                    plugins.remove(index);
                    changed = true;
                }
                egui::ComboBox::from_id_salt("master-fx-plugin")
                    .selected_text(
                        catalog
                            .get(self.master_fx_choice)
                            .map_or("No cached plug-ins", |plugin| plugin.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for (index, plugin) in catalog.iter().enumerate().take(20) {
                            ui.selectable_value(&mut self.master_fx_choice, index, &plugin.name);
                        }
                    });
                if ui.button("Add plug-in").clicked()
                    && let Some(plugin) = catalog.get(self.master_fx_choice)
                {
                    let id = plugins.iter().map(|slot| slot.id).max().unwrap_or(0) + 1;
                    plugins.push(project::song::PluginSlot {
                        id,
                        name: plugin.name.clone(),
                        format: match plugin.format {
                            plugins::Format::Vst3 => project::song::PluginFormat::Vst3,
                            plugins::Format::AudioUnit => project::song::PluginFormat::AudioUnit,
                        },
                        path: plugin.path.display().to_string(),
                        identifier: plugin.identifier.clone(),
                        bypassed: false,
                        state: String::new(),
                    });
                    changed = true;
                }
                ui.label(
                    egui::RichText::new(
                        "Master effects run after the bus sum and before the master limiter.",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
            });
        if changed {
            match self.editor.set_master_plugins(plugins) {
                Ok(()) => self.status = Some(String::from("master effects updated")),
                Err(error) => self.status = Some(error),
            }
        }
        self.master_fx_open = open;
    }

    fn style_rules_window(&mut self, context: &Context) {
        if !self.style_rules_open {
            return;
        }
        let mut open = self.style_rules_open;
        egui::Window::new("Style rules")
            .open(&mut open)
            .resizable(true)
            .default_width(620.0)
            .show(context, |ui| {
                let Some(style) = self.style_draft.as_mut() else {
                    ui.label("No style draft.");
                    return;
                };
                ui.label(
                    egui::RichText::new(
                        "Rules apply to the exported style; notes stay as recorded.",
                    )
                    .size(10.0)
                    .color(theme::text_dim()),
                );
                let roots = [
                    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
                ];
                for section in &mut style.sections {
                    ui.separator();
                    ui.label(egui::RichText::new(&section.name).strong());
                    for part in &mut section.parts {
                        ui.group(|ui| {
                            ui.label(
                                egui::RichText::new(&part.name)
                                    .size(11.0)
                                    .color(theme::text_muted()),
                            );
                            ui.horizontal(|ui| {
                                ui.label("dest ch");
                                ui.add(
                                    egui::DragValue::new(&mut part.rule.dest_channel).range(9..=16),
                                );
                                ui.label("written root");
                                let root = usize::from(part.rule.source_root.min(11));
                                egui::ComboBox::from_id_salt(format!(
                                    "rule-root-{}-{}",
                                    section.name, part.name
                                ))
                                .selected_text(roots[root])
                                .show_ui(ui, |ui| {
                                    for (index, name) in roots.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut part.rule.source_root,
                                            index as u8,
                                            *name,
                                        );
                                    }
                                });
                                egui::ComboBox::from_id_salt(format!(
                                    "rule-type-{}-{}",
                                    section.name, part.name
                                ))
                                .selected_text(part.rule.source_chord_type.suffix())
                                .show_ui(ui, |ui| {
                                    for chord_type in arranger::chord_type::ALL_TYPES {
                                        ui.selectable_value(
                                            &mut part.rule.source_chord_type,
                                            chord_type,
                                            if chord_type.suffix().is_empty() {
                                                "major"
                                            } else {
                                                chord_type.suffix()
                                            },
                                        );
                                    }
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label("NTR");
                                egui::ComboBox::from_id_salt(format!(
                                    "rule-ntr-{}-{}",
                                    section.name, part.name
                                ))
                                .selected_text(format!("{:?}", part.rule.transpose.ntr))
                                .show_ui(ui, |ui| {
                                    for value in [
                                        arranger::rules::Ntr::RootTransposition,
                                        arranger::rules::Ntr::RootFixed,
                                        arranger::rules::Ntr::Guitar,
                                    ] {
                                        ui.selectable_value(
                                            &mut part.rule.transpose.ntr,
                                            value,
                                            format!("{value:?}"),
                                        );
                                    }
                                });
                                ui.label("NTT");
                                egui::ComboBox::from_id_salt(format!(
                                    "rule-ntt-{}-{}",
                                    section.name, part.name
                                ))
                                .selected_text(format!("{:?}", part.rule.transpose.ntt))
                                .show_ui(ui, |ui| {
                                    for value in [
                                        arranger::rules::Ntt::Bypass,
                                        arranger::rules::Ntt::Melody,
                                        arranger::rules::Ntt::Chord,
                                    ] {
                                        ui.selectable_value(
                                            &mut part.rule.transpose.ntt,
                                            value,
                                            format!("{value:?}"),
                                        );
                                    }
                                });
                                ui.checkbox(&mut part.rule.transpose.bass, "follow bass");
                            });
                            ui.horizontal(|ui| {
                                ui.label("held notes");
                                egui::ComboBox::from_id_salt(format!(
                                    "rule-rtr-{}-{}",
                                    section.name, part.name
                                ))
                                .selected_text(format!("{:?}", part.rule.transpose.rtr))
                                .show_ui(ui, |ui| {
                                    for value in [
                                        arranger::rules::Rtr::Stop,
                                        arranger::rules::Rtr::PitchShift,
                                        arranger::rules::Rtr::PitchShiftToRoot,
                                        arranger::rules::Rtr::Retrigger,
                                        arranger::rules::Rtr::RetriggerToRoot,
                                        arranger::rules::Rtr::NoteGenerator,
                                    ] {
                                        ui.selectable_value(
                                            &mut part.rule.transpose.rtr,
                                            value,
                                            format!("{value:?}"),
                                        );
                                    }
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label("high key");
                                ui.add(
                                    egui::DragValue::new(&mut part.rule.transpose.high_key)
                                        .range(0..=11),
                                );
                                ui.label("low");
                                ui.add(
                                    egui::DragValue::new(&mut part.rule.transpose.low_limit)
                                        .range(0..=127),
                                );
                                ui.label("high");
                                ui.add(
                                    egui::DragValue::new(&mut part.rule.transpose.high_limit)
                                        .range(0..=127),
                                );
                            });
                        });
                    }
                }
            });
        self.style_rules_open = open;
    }

    fn build_audio_style(&self) -> Result<style_formats::BuiltAudioStyle, String> {
        let path = std::path::Path::new(&self.audio_style_path);
        let decoded = audio::decode::decode_mono(path).or_else(|_| {
            let bytes =
                std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
            audio::wav::decode_mono(&bytes)
        })?;
        let source = if self.audio_style_chord.eq_ignore_ascii_case("auto") {
            None
        } else {
            let chord = music_core::Chord::parse(&self.audio_style_chord)
                .map_err(|error| error.to_string())?;
            Some(
                arranger::chord_type::ArrangerChord::try_from(&chord)
                    .map_err(|_| format!("unsupported chord {:?}", self.audio_style_chord))?,
            )
        };
        style_formats::style_from_audio(
            &decoded,
            style_formats::AudioStyleOptions {
                bpm: self.audio_style_bpm,
                section: &self.audio_style_section,
                source_chord: source,
                split_parts: self.audio_style_split,
                section_bars: self.audio_style_section_bars,
                include_drums: self.audio_style_drums,
                detect_tempo: self.audio_style_detect_tempo,
            },
        )
    }

    fn audio_style_window(&mut self, context: &Context) {
        if !self.audio_style_open {
            return;
        }
        let mut open = self.audio_style_open;
        egui::Window::new("Style from audio")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Audio file");
                ui.add(
                    egui::TextEdit::singleline(&mut self.audio_style_path)
                        .hint_text("WAV, FLAC, MP3, M4A")
                        .desired_width(320.0),
                );
                ui.horizontal(|ui| {
                    ui.label("BPM");
                    ui.add(egui::DragValue::new(&mut self.audio_style_bpm).range(20.0..=300.0));
                    ui.label("chord");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.audio_style_chord).desired_width(60.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("section");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.audio_style_section)
                            .desired_width(100.0),
                    );
                    ui.checkbox(&mut self.audio_style_split, "split bass/melody");
                });
                ui.horizontal(|ui| {
                    ui.label("section bars");
                    ui.add(egui::DragValue::new(&mut self.audio_style_section_bars).range(0..=16));
                    ui.label("output");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.audio_style_out).desired_width(150.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.audio_style_drums, "add drums from onsets");
                    ui.checkbox(&mut self.audio_style_detect_tempo, "detect BPM");
                    ui.checkbox(&mut self.audio_style_sff1, "SFF1 instead of SFF2");
                });
                ui.horizontal(|ui| {
                    if ui.button("Export style").clicked() {
                        match self.build_audio_style() {
                            Ok(built) => {
                                let version = if self.audio_style_sff1 {
                                    style_formats::casm::SffVersion::Sff1
                                } else {
                                    style_formats::casm::SffVersion::Sff2
                                };
                                match style_formats::write_native_style(&built.style, version)
                                    .map_err(|error| error.to_string())
                                    .and_then(|bytes| {
                                        std::fs::write(&self.audio_style_out, &bytes)
                                            .map(|()| bytes.len())
                                            .map_err(|error| error.to_string())
                                    }) {
                                    Ok(bytes) => {
                                        self.status = Some(format!(
                                            "wrote {}: source {}, {} note(s), {} bytes",
                                            self.audio_style_out,
                                            built.source_chord.name(),
                                            built.notes,
                                            bytes
                                        ));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Audition").clicked() {
                        match self
                            .build_audio_style()
                            .and_then(|built| style_preview_song(&built.style))
                            .and_then(|song| {
                                audio::player::Player::new(audio::render(&song, audio::SAMPLE_RATE))
                            }) {
                            Ok(player) => {
                                player.play();
                                self.style_preview = Some(player);
                                self.status = Some(String::from("auditioning audio style"));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Stop").clicked() {
                        self.style_preview = None;
                    }
                });
            });
        self.audio_style_open = open;
    }

    fn audition_style_section(&mut self) {
        match self.style_file_preview_song().and_then(|song| {
            audio::player::Player::new(audio::render(&song, audio::SAMPLE_RATE))
                .map_err(|error| error.to_string())
        }) {
            Ok(player) => {
                player.play();
                self.style_preview = Some(player);
                self.status = Some(String::from("auditioning style section"));
            }
            Err(error) => self.status = Some(error),
        }
    }

    fn queue_style_section(&mut self, index: usize) {
        let Some(imported) = self.style_import.as_ref() else {
            return;
        };
        let current_role = imported
            .section_names()
            .get(self.style_section)
            .map_or(StyleSectionRole::Main, |name| style_section_role(name));
        let role = imported
            .section_names()
            .get(index)
            .map_or(StyleSectionRole::Main, |name| style_section_role(name));
        match role {
            StyleSectionRole::Fill => {
                if current_role == StyleSectionRole::Main {
                    self.style_fill_return = Some(self.style_section);
                }
                self.style_ending = false;
            }
            StyleSectionRole::Ending => {
                self.style_fill_return = None;
                self.style_ending = true;
            }
            StyleSectionRole::Main => {
                self.style_fill_return = None;
                self.style_ending = false;
            }
        }
        let Some(player) = self.style_preview.as_ref() else {
            self.style_section = index;
            self.audition_style_section();
            return;
        };
        self.style_section = index;
        if !player.is_playing() {
            self.audition_style_section();
            return;
        }
        let bar_ticks = imported.time_signature.bar_ticks().max(1);
        let seconds = 60.0 / (imported.tempo_bpm.max(30.0) * music_core::PPQ as f64);
        let bar_seconds = bar_ticks as f64 * seconds;
        let now = player.frame() as f64 / f64::from(audio::SAMPLE_RATE);
        self.style_queued_section = Some(index);
        self.style_queue_at = next_bar_seconds(now, bar_seconds);
        self.status = Some(String::from("section queued at the next bar"));
    }

    fn queue_style_chord(&mut self, chord: &str) {
        let Some(imported) = self.style_import.as_ref() else {
            return;
        };
        let Some(player) = self.style_preview.as_ref() else {
            self.style_chord = chord.to_string();
            self.audition_style_section();
            return;
        };
        if !player.is_playing() {
            self.style_chord = chord.to_string();
            self.audition_style_section();
            return;
        }
        let bar_ticks = imported.time_signature.bar_ticks().max(1);
        let seconds = 60.0 / (imported.tempo_bpm.max(30.0) * music_core::PPQ as f64);
        let bar_seconds = bar_ticks as f64 * seconds;
        let now = player.frame() as f64 / f64::from(audio::SAMPLE_RATE);
        self.style_chord = chord.to_string();
        self.style_queued_chord = Some(chord.to_string());
        self.style_queue_at = next_bar_seconds(now, bar_seconds);
        self.status = Some(format!("chord {chord} queued at the next bar"));
    }

    fn load_style_file(&mut self) {
        match std::fs::read(&self.style_file_path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                style_formats::import_style(&bytes).map_err(|error| error.to_string())
            }) {
            Ok(imported) => {
                self.style_section = 0;
                self.style_fill_return = None;
                self.style_ending = false;
                self.status = Some(format!(
                    "style loaded: {} section(s)",
                    imported.sections.len()
                ));
                self.style_import = Some(imported);
            }
            Err(error) => self.status = Some(error),
        }
    }

    fn analyze_audio_chords(&mut self) {
        let path = std::path::PathBuf::from(&self.audio_chords_path);
        let bpm = self.song().tempo_map.bpm_at(0);
        let bar_seconds = self.song().time_signature.bar_ticks() as f64 * 60.0
            / (bpm.max(1.0) * music_core::PPQ as f64);
        let (sender, receiver) = std::sync::mpsc::channel();
        self.audio_chords_pending = Some(receiver);
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

    fn l6max_mixer_window(&mut self, context: &Context) {
        if !self.l6max_mixer_open {
            return;
        }
        let mut open = self.l6max_mixer_open;
        let compact = self.l6max_compact;
        let mut edits = Vec::new();
        let mut scene_save = None;
        let mut scene_recall = None;
        egui::Window::new("L6max mixer")
            .open(&mut open)
            .resizable(true)
            .default_width(520.0)
            .show(context, |ui| {
                let tracks: Vec<(TrackId, String, u8, u8, bool, u8)> = self
                    .song()
                    .tracks
                    .iter()
                    .filter(|track| track.audio.is_some())
                    .take(8)
                    .map(|track| {
                        (
                            track.id,
                            track.name.clone(),
                            track.volume,
                            track.pan.clamp(0, 127),
                            track.muted,
                            track.send_level,
                        )
                    })
                    .collect();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.l6max_compact, "Compact layout");
                    for (index, label) in ["A", "B", "C", "D"].iter().enumerate() {
                        if ui.small_button(format!("Save {label}")).clicked() {
                            scene_save = Some(index);
                        }
                        if ui.small_button(*label).clicked() {
                            scene_recall = Some(index);
                        }
                    }
                    if ui.button("Reset").clicked() {
                        for (id, _, _, _, _, _) in &tracks {
                            edits.push((*id, 100, 64, false, 0));
                        }
                    }
                });
                if tracks.is_empty() {
                    ui.label("No imported L6max audio tracks yet.");
                    return;
                }
                for (id, name, volume, pan, muted, send) in &tracks {
                    ui.horizontal(|ui| {
                        ui.label(name);
                        let mut level = *volume;
                        if ui
                            .add(egui::Slider::new(&mut level, 0..=127).text("level"))
                            .changed()
                        {
                            edits.push((*id, level, *pan, *muted, *send));
                        }
                        let mut pan = *pan;
                        if ui
                            .add(egui::Slider::new(&mut pan, 0..=127).text("pan"))
                            .changed()
                        {
                            edits.push((*id, *volume, pan, *muted, *send));
                        }
                        let mut send_value = *send;
                        if ui
                            .add(egui::Slider::new(&mut send_value, 0..=127).text("send"))
                            .changed()
                        {
                            edits.push((*id, *volume, pan, *muted, send_value));
                        }
                        let mut mute = *muted;
                        if ui.checkbox(&mut mute, "Mute").changed() {
                            edits.push((*id, *volume, pan, mute, *send));
                        }
                    });
                }
                ui.label(
                    egui::RichText::new(
                        "L6max EQ, MONO, USB routing, effects and compressor need device project data.",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
                if compact {
                    ui.label(
                        egui::RichText::new("Compact rows use the same controls.")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                }
            });
        if let Some(index) = scene_save {
            let scene = L6Scene {
                channels: self
                    .song()
                    .tracks
                    .iter()
                    .filter(|track| track.audio.is_some())
                    .take(8)
                    .map(|track| {
                        (
                            track.volume,
                            track.pan.clamp(0, 127),
                            track.muted,
                            track.send_level,
                        )
                    })
                    .collect(),
            };
            self.l6max_scenes[index] = Some(scene);
            save_l6_scenes(&self.l6max_scenes);
            self.status = Some(format!("saved L6max scene {}", ["A", "B", "C", "D"][index]));
        }
        if let Some(index) = scene_recall
            && let Some(scene) = self.l6max_scenes[index].clone()
        {
            let audio_tracks: Vec<TrackId> = self
                .song()
                .tracks
                .iter()
                .filter(|track| track.audio.is_some())
                .take(8)
                .map(|track| track.id)
                .collect();
            for (track, (volume, pan, muted, send)) in audio_tracks.iter().zip(&scene.channels) {
                edits.push((*track, *volume, *pan, *muted, *send));
            }
            self.status = Some(format!(
                "recalled L6max scene {}",
                ["A", "B", "C", "D"][index]
            ));
        }
        for (id, volume, pan, muted, send) in edits {
            let _ = self.editor.set_track_gain(id, volume, pan);
            let info = self
                .song()
                .tracks
                .iter()
                .find(|track| track.id == id)
                .map(|track| (track.soloed, track.armed, track.bus, track.send));
            if let Some((soloed, armed, bus, has_send)) = info {
                let _ = self.editor.set_track_flags(id, muted, soloed, armed);
                let _ = self.editor.set_track_routing(id, bus, has_send, send);
            }
        }
        self.l6max_mixer_open = open;
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
        if !self.audio_chords_open {
            return;
        }
        if self.audio_chords_auto {
            self.audio_chords_auto = false;
            self.analyze_audio_chords();
        }
        let mut open = self.audio_chords_open;
        egui::Window::new("Chords from audio")
            .open(&mut open)
            .resizable(false)
            .default_width(360.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Audio");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.audio_chords_path)
                            .desired_width(220.0),
                    );
                    if ui.button("Analyze").clicked() {
                        self.analyze_audio_chords();
                    }
                });
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for chord in &self.audio_chords {
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
                        self.audio_chords.clear();
                    }
                });
            });
        self.audio_chords_open = open;
    }

    fn style_link_window(&mut self, context: &Context) {
        if !self.style_link_open {
            return;
        }
        let mut open = self.style_link_open;
        egui::Window::new("Style from link")
            .open(&mut open)
            .resizable(false)
            .default_width(380.0)
            .show(context, |ui| {
                ui.label("Link");
                ui.add(
                    egui::TextEdit::singleline(&mut self.style_link_url)
                        .hint_text("https://…")
                        .desired_width(300.0),
                );
                ui.label(
                    egui::RichText::new(
                        "Downloads the best audio with yt-dlp and opens Style from audio. yt-dlp and FFmpeg must be installed.",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
                if ui.button("Download audio").clicked() {
                    if self.style_link_url.trim().is_empty() {
                        self.status = Some(String::from("enter a link first"));
                    } else {
                        let url = self.style_link_url.clone();
                        let (sender, receiver) = std::sync::mpsc::channel();
                        self.style_link_pending = Some(receiver);
                        self.status = Some(String::from("downloading link audio…"));
                        std::thread::spawn(move || {
                            let _ = sender.send(crate::link::download(&url));
                        });
                    }
                }
                if self.style_link_pending.is_some() {
                    ui.label(
                        egui::RichText::new("Downloading…")
                            .size(10.0)
                            .color(theme::accent()),
                    );
                }
            });
        self.style_link_open = open;
    }

    fn style_file_window(&mut self, context: &Context) {
        if !self.style_file_open {
            return;
        }
        if self.style_auto_load {
            self.style_auto_load = false;
            self.load_style_file();
        }
        let mut open = self.style_file_open;
        let mut queue_choice = None;
        let mut chord_choice = None;
        egui::Window::new("Style file player")
            .open(&mut open)
            .resizable(false)
            .default_width(360.0)
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label("File");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.style_file_path)
                            .desired_width(220.0),
                    );
                    if ui.button("Load").clicked() {
                        self.load_style_file();
                    }
                });
                let Some(imported) = &self.style_import else {
                    ui.label(
                        egui::RichText::new("Load a Yamaha style to audition sections.")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                    return;
                };
                let names = imported.section_names();
                if names.is_empty() {
                    ui.label("No sections.");
                    return;
                }
                self.style_section = self.style_section.min(names.len() - 1);
                egui::ComboBox::from_id_salt("style-file-section")
                    .selected_text(names[self.style_section])
                    .show_ui(ui, |ui| {
                        for (index, name) in names.iter().enumerate() {
                            if ui
                                .selectable_label(self.style_section == index, *name)
                                .clicked()
                                && self.style_section != index
                            {
                                queue_choice = Some(index);
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    ui.label("Chord");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.style_chord).desired_width(70.0),
                    );
                    for chord in ["C", "Dm", "Em", "F", "G", "Am", "G7"] {
                        if ui.small_button(chord).clicked() {
                            chord_choice = Some(chord);
                        }
                    }
                });
                ui.label(format!(
                    "style written in {} · {:.1} BPM · {} ticks",
                    imported.native.source_chord.name(),
                    imported.tempo_bpm,
                    imported.native.sections[self.style_section].length_ticks
                ));
                ui.horizontal(|ui| {
                    if ui.button("Audition").clicked() {
                        self.style_queued_section = None;
                        self.style_queued_chord = None;
                        self.style_fill_return = None;
                        self.style_ending = false;
                        self.audition_style_section();
                    }
                    if ui.button("Stop").clicked() {
                        self.style_preview = None;
                    }
                    ui.checkbox(&mut self.style_file_loop, "Loop");
                });
                ui.label(
                    egui::RichText::new(
                        "Section and chord changes queue at the next bar; held MIDI notes are recognised; fills return to the main section and endings stop looping.",
                    )
                    .size(9.0)
                    .color(theme::text_dim()),
                );
            });
        self.style_file_open = open;
        if let Some(chord) = chord_choice {
            self.queue_style_chord(chord);
        }
        if let Some(index) = queue_choice {
            self.queue_style_section(index);
        }
    }

    fn style_file_preview_song(&self) -> Result<project::Song, String> {
        let imported = self
            .style_import
            .as_ref()
            .ok_or_else(|| String::from("load a style first"))?;
        style_section_song(imported, self.style_section, &self.style_chord)
    }

    fn style_clips_window(&mut self, context: &Context) {
        if !self.style_clips_open {
            return;
        }
        let mut open = self.style_clips_open;
        egui::Window::new("Style from clips")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                ui.label("Section name");
                ui.text_edit_singleline(&mut self.style_name);
                ui.horizontal(|ui| {
                    ui.label("Chord");
                    ui.add(egui::TextEdit::singleline(&mut self.style_chord).desired_width(80.0));
                });
                ui.checkbox(&mut self.style_from_markers, "split at markers");
                if !self.style_from_markers {
                    ui.add(
                        egui::DragValue::new(&mut self.style_length)
                            .range(1..=960 * 128)
                            .suffix(" ticks"),
                    );
                }
                ui.horizontal(|ui| {
                    ui.label("Output");
                    ui.add(egui::TextEdit::singleline(&mut self.style_path).desired_width(220.0));
                });
                ui.checkbox(&mut self.style_sff1, "SFF1 instead of SFF2");
                ui.horizontal(|ui| {
                    if ui.button("Export style").clicked() {
                        match self.export_style_from_clips() {
                            Ok(message) => self.status = Some(message),
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Audition").clicked() {
                        match self.build_style_from_clips() {
                            Ok(style) => match style_preview_song(&style).and_then(|song| {
                                audio::player::Player::new(audio::render(&song, audio::SAMPLE_RATE))
                            }) {
                                Ok(player) => {
                                    player.play();
                                    self.style_preview = Some(player);
                                    self.status = Some(String::from("auditioning style"));
                                }
                                Err(error) => self.status = Some(error),
                            },
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Stop").clicked() {
                        self.style_preview = None;
                    }
                    if ui.button("Edit rules").clicked() {
                        match self.build_style_from_clips() {
                            Ok(style) => {
                                self.style_draft = Some(style);
                                self.style_rules_open = true;
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
            });
        self.style_clips_open = open;
    }

    fn build_style_from_clips(&self) -> Result<arranger::style::Style, String> {
        if let Some(draft) = &self.style_draft {
            return Ok(draft.clone());
        }
        let chord =
            music_core::Chord::parse(&self.style_chord).map_err(|error| error.to_string())?;
        let source = arranger::chord_type::ArrangerChord::try_from(&chord)
            .map_err(|_| format!("unsupported style chord {:?}", self.style_chord))?;
        let mut style = if self.style_from_markers {
            arranger::style::from_song_sections(self.song(), &self.song().name, source)
        } else {
            arranger::style::from_song(
                self.song(),
                &self.song().name,
                source,
                music_core::Tick::from(self.style_length.max(1)),
            )
        };
        if !self.style_from_markers
            && let Some(first) = style.sections.first_mut()
        {
            first.name = self.style_name.clone();
        }
        Ok(style)
    }

    fn export_style_from_clips(&self) -> Result<String, String> {
        let style = self.build_style_from_clips()?;
        let version = if self.style_sff1 {
            style_formats::casm::SffVersion::Sff1
        } else {
            style_formats::casm::SffVersion::Sff2
        };
        let bytes = style_formats::write_native_style(&style, version)
            .map_err(|error| error.to_string())?;
        std::fs::write(&self.style_path, &bytes)
            .map_err(|error| format!("{}: {error}", self.style_path))?;
        Ok(format!(
            "wrote {}: {} section(s), {} byte(s)",
            self.style_path,
            style.sections.len(),
            bytes.len()
        ))
    }

    fn patch_window(&mut self, context: &Context) {
        let mut open = self.patch_open;
        egui::Window::new("Synth patch")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                let track = self
                    .roll
                    .selected
                    .or_else(|| self.song().tracks.first().map(|track| track.id));
                let Some(track) = track else {
                    ui.label("Add a track first.");
                    return;
                };
                if self.patch_track != Some(track) || self.patch_draft.is_none() {
                    self.patch_track = Some(track);
                    self.patch_draft = Some(
                        self.song()
                            .tracks
                            .iter()
                            .find(|entry| entry.id == track)
                            .and_then(|entry| entry.patch)
                            .unwrap_or_default(),
                    );
                }
                let Some(mut patch) = self.patch_draft else {
                    return;
                };
                let waveform_name = |waveform: project::song::Waveform| match waveform {
                    project::song::Waveform::Sine => "Sine",
                    project::song::Waveform::Saw => "Saw",
                    project::song::Waveform::Square => "Square",
                    project::song::Waveform::Triangle => "Triangle",
                };
                egui::ComboBox::from_id_salt("patch-waveform")
                    .selected_text(waveform_name(patch.waveform))
                    .show_ui(ui, |ui| {
                        for waveform in [
                            project::song::Waveform::Sine,
                            project::song::Waveform::Saw,
                            project::song::Waveform::Square,
                            project::song::Waveform::Triangle,
                        ] {
                            ui.selectable_value(
                                &mut patch.waveform,
                                waveform,
                                waveform_name(waveform),
                            );
                        }
                    });
                ui.add(
                    egui::Slider::new(&mut patch.attack, 0.001..=2.0)
                        .text("Attack")
                        .logarithmic(true),
                );
                ui.add(egui::Slider::new(&mut patch.decay, 0.01..=3.0).text("Decay"));
                ui.add(egui::Slider::new(&mut patch.sustain, 0.0..=1.0).text("Sustain"));
                ui.add(egui::Slider::new(&mut patch.release, 0.0..=3.0).text("Release"));
                ui.add(egui::Slider::new(&mut patch.brightness, 0.0..=1.0).text("Brightness"));
                self.patch_draft = Some(patch);
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        match self.editor.set_track_patch(track, Some(patch)) {
                            Ok(()) => self.status = Some(String::from("synth patch applied")),
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Default").clicked() {
                        self.patch_draft = Some(project::song::Patch::default());
                    }
                    if ui.button("Clear").clicked() {
                        match self.editor.set_track_patch(track, None) {
                            Ok(()) => {
                                self.patch_draft = Some(project::song::Patch::default());
                                self.status = Some(String::from("synth patch cleared"));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
            });
        self.patch_open = open;
    }

    fn capture_vst3_state(&mut self, track: TrackId, slot: u32) {
        let Some(plugin) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.plugins.iter().find(|entry| entry.id == slot))
            .cloned()
        else {
            self.status = Some(String::from("plug-in slot not found"));
            return;
        };
        let effect = match plugin_host::Vst3Effect::new(
            std::path::Path::new(&plugin.path),
            &plugin.identifier,
            48_000.0,
            512,
        ) {
            Ok(effect) => effect,
            Err(error) => {
                self.status = Some(format!("VST3: {error}"));
                return;
            }
        };
        match effect.save_state_base64() {
            Ok(state) => match self.editor.set_plugin_state(track, slot, &state) {
                Ok(()) => self.status = Some(String::from("VST3 state captured")),
                Err(error) => self.status = Some(error),
            },
            Err(error) => self.status = Some(format!("VST3: {error}")),
        }
    }

    fn open_vst3_parameters(&mut self, track: TrackId, slot: u32) {
        let Some(plugin) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.plugins.iter().find(|entry| entry.id == slot))
            .cloned()
        else {
            self.status = Some(String::from("plug-in slot not found"));
            return;
        };
        let effect = match plugin_host::Vst3Effect::new(
            std::path::Path::new(&plugin.path),
            &plugin.identifier,
            48_000.0,
            512,
        ) {
            Ok(effect) => effect,
            Err(error) => {
                self.status = Some(format!("VST3: {error}"));
                return;
            }
        };
        let mut effect = effect;
        if !plugin.state.is_empty() {
            let _ = effect.load_state_base64(&plugin.state);
        }
        let params = effect.parameters();
        if params.is_empty() {
            self.status = Some(format!("{} exposes no VST3 parameters", plugin.name));
            return;
        }
        self.vst3_params = params;
        self.vst3_effect = Some(effect);
        self.vst3_params_track = Some(track);
        self.vst3_params_slot = Some(slot);
        self.vst3_params_open = true;
    }

    fn vst3_parameters_window(&mut self, context: &Context) {
        if !self.vst3_params_open {
            return;
        }
        let mut open = self.vst3_params_open;
        egui::Window::new("VST3 parameters")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                if let (Some(track), Some(slot)) = (self.vst3_params_track, self.vst3_params_slot) {
                    ui.label(
                        egui::RichText::new(format!("track {track} · slot {slot}"))
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                let slot = self.vst3_params_slot;
                let mut automate = None;
                if let Some(effect) = self.vst3_effect.as_mut() {
                    for (id, name, value) in &mut self.vst3_params {
                        let label = if name.is_empty() {
                            format!("Param {id}")
                        } else {
                            name.clone()
                        };
                        ui.horizontal(|ui| {
                            let changed = ui
                                .add(egui::Slider::new(value, 0.0..=1.0).text(label))
                                .changed();
                            if changed {
                                effect.set_parameter(*id, *value);
                            }
                            if ui
                                .small_button("A")
                                .on_hover_text("Use this parameter for automation")
                                .clicked()
                                && let Some(slot) = slot
                            {
                                automate = Some((slot, *id));
                            }
                        });
                    }
                }
                if let Some((slot, parameter)) = automate {
                    self.automation_target =
                        project::song::AutomationTarget::PluginParam { slot, parameter };
                    self.status = Some(format!(
                        "automation target: slot {slot} parameter {parameter}"
                    ));
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply state").clicked()
                        && let (Some(effect), Some(track), Some(slot)) = (
                            self.vst3_effect.as_ref(),
                            self.vst3_params_track,
                            self.vst3_params_slot,
                        )
                    {
                        match effect.save_state_base64() {
                            Ok(state) => match self.editor.set_plugin_state(track, slot, &state) {
                                Ok(()) => self.status = Some(String::from("VST3 state saved")),
                                Err(error) => self.status = Some(error),
                            },
                            Err(error) => self.status = Some(format!("VST3: {error}")),
                        }
                    }
                });
            });
        self.vst3_params_open = open;
        if !open {
            self.vst3_effect = None;
            self.vst3_params.clear();
        }
    }

    fn open_au_parameters(&mut self, track: TrackId, slot: u32) {
        let Some(plugin) = self
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .and_then(|entry| entry.plugins.iter().find(|entry| entry.id == slot))
            .cloned()
        else {
            self.status = Some(String::from("plug-in slot not found"));
            return;
        };
        if plugin.format != project::song::PluginFormat::AudioUnit {
            self.status = Some(String::from("only AU parameters are supported"));
            return;
        }
        let unit = if let Ok(effect) = plugin_host::AuEffect::new(&plugin.identifier, 48_000.0) {
            effect.apply_state(&plugin.state);
            AuEditorUnit::Effect(effect)
        } else if let Ok(instrument) = plugin_host::AuInstrument::new(&plugin.identifier, 48_000.0)
        {
            instrument.apply_state(&plugin.state);
            AuEditorUnit::Instrument(instrument)
        } else {
            self.status = Some(format!(
                "AU: {} could not be opened as an effect or instrument",
                plugin.name
            ));
            return;
        };
        self.au_params = unit.parameters();
        self.au_effect = Some(unit);
        self.au_editor_track = Some(track);
        self.au_editor_slot = Some(slot);
        self.au_editor_open = true;
    }

    fn au_parameters_window(&mut self, context: &Context) {
        let mut open = self.au_editor_open;
        egui::Window::new("AU parameters")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                if let (Some(track), Some(slot)) = (self.au_editor_track, self.au_editor_slot) {
                    ui.label(
                        egui::RichText::new(format!("track {track} · slot {slot}"))
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                let slot = self.au_editor_slot;
                let mut automate = None;
                if let Some(effect) = self.au_effect.as_ref() {
                    for parameter in &mut self.au_params {
                        let (id, name, value, min, max, _) = parameter;
                        if *min >= *max {
                            continue;
                        }
                        let label = if name.is_empty() {
                            format!("Param {id}")
                        } else {
                            name.clone()
                        };
                        ui.horizontal(|ui| {
                            let changed = ui
                                .add(egui::Slider::new(value, *min..=*max).text(label))
                                .changed();
                            if changed {
                                effect.set_parameter(*id, *value);
                            }
                            if ui
                                .small_button("A")
                                .on_hover_text("Use this parameter for automation")
                                .clicked()
                                && let Some(slot) = slot
                            {
                                automate = Some((slot, *id));
                            }
                        });
                    }
                } else {
                    ui.label("No AU instance.");
                }
                if let Some((slot, parameter)) = automate {
                    self.automation_target =
                        project::song::AutomationTarget::PluginParam { slot, parameter };
                    self.status = Some(format!(
                        "automation target: slot {slot} parameter {parameter}"
                    ));
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked()
                        && let (Some(track), Some(slot)) =
                            (self.au_editor_track, self.au_editor_slot)
                    {
                        let state: String = self
                            .au_params
                            .iter()
                            .map(|(id, _, value, _, _, _)| format!("{id}={value};"))
                            .collect();
                        match self.editor.set_plugin_state(track, slot, &state) {
                            Ok(()) => self.status = Some(String::from("AU parameter state saved")),
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Defaults").clicked()
                        && let Some(effect) = self.au_effect.as_ref()
                    {
                        for (id, _, value, _, _, default) in &mut self.au_params {
                            *value = *default;
                            effect.set_parameter(*id, *default);
                        }
                    }
                });
            });
        self.au_editor_open = open;
        if !open {
            self.au_effect = None;
            self.au_params.clear();
        }
    }

    fn start_midi_recording(&mut self) {
        if self.recording.is_some() {
            return;
        }
        let Some(keyboard) = self.settings.keyboard.clone() else {
            self.status = Some(String::from("choose a MIDI keyboard in Settings first"));
            return;
        };
        let tempo = self.song().tempo_map.clone();
        let seconds = self.record_seconds;
        let count_in = self.settings.record_count_in;
        let bpm = tempo.bpm_at(0).max(1.0);
        let delay = f64::from(count_in) * 4.0 * 60.0 / bpm;
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            if delay > 0.0 {
                if let Ok(player) = audio::player::Player::new(count_in_audio(bpm, count_in)) {
                    player.play();
                    std::thread::sleep(std::time::Duration::from_secs_f64(delay));
                    player.release();
                } else {
                    std::thread::sleep(std::time::Duration::from_secs_f64(delay));
                }
            }
            let result =
                midi_devices::devices::record_to_song(&keyboard, seconds, &tempo, "Recorded")
                    .map(|(song, _, _)| song)
                    .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.record_target = self
            .song()
            .tracks
            .iter()
            .find(|track| track.armed)
            .map(|track| track.id);
        self.recording = Some(receiver);
        self.status = Some(if count_in == 0 {
            format!("recording {seconds} s")
        } else {
            format!("count-in {count_in} bar(s), then {seconds} s")
        });
    }

    fn right_inspector(&mut self, context: &egui::Context) {
        if self.settings.kids_mode {
            return;
        }
        let track_id = self
            .score_track
            .or(self.roll.selected)
            .or_else(|| self.song().tracks.first().map(|track| track.id));
        if self.midi_ports.is_none()
            && let Ok(ports) = midi_devices::devices::list_ports()
        {
            self.midi_ports = Some((ports.inputs, ports.outputs));
        }
        let track = track_id.and_then(|id| {
            self.song()
                .tracks
                .iter()
                .find(|track| track.id == id)
                .cloned()
        });
        let mut meter_change = None;
        let held_chord = self.midi_chord.clone();
        egui::SidePanel::right("right-inspector")
            .resizable(true)
            .default_width(300.0)
            .width_range(240.0..=480.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(context, |ui| {
                let full = ui.max_rect();
                let chat_height = 190.0_f32.min((full.height() * 0.42).max(110.0));
                let top_rect = egui::Rect::from_min_max(
                    full.min,
                    egui::pos2(full.right(), full.bottom() - chat_height - 6.0),
                );
                let mut top_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(top_rect)
                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                );
                egui::ScrollArea::vertical().show(&mut top_ui, |ui| {
                    ui.label(
                        egui::RichText::new("TRACK")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    let Some(track) = &track else {
                        ui.label("No track");
                        return;
                    };
                    if self.inspector_name_track != Some(track.id) {
                        self.inspector_name_track = Some(track.id);
                        self.inspector_name = track.name.clone();
                    }
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.inspector_name)
                                .desired_width(150.0)
                                .font(egui::TextStyle::Heading),
                        );
                        if response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            let name = self.inspector_name.trim().to_string();
                            match self.editor.rename_track(track.id, &name) {
                                Ok(()) => self.status = Some(format!("track renamed to {name:?}")),
                                Err(error) => self.status = Some(error),
                            }
                        }
                        let channel = track
                            .notes
                            .first()
                            .map_or(1, |note| note.channel.one_based());
                        egui::ComboBox::from_id_salt("track-channel")
                            .selected_text(format!("Ch {channel}"))
                            .width(64.0)
                            .show_ui(ui, |ui| {
                                for value in 1..=16 {
                                    if ui
                                        .selectable_label(channel == value, format!("Ch {value}"))
                                        .clicked()
                                        && let Ok(channel) =
                                            music_core::Channel::from_one_based(value)
                                        && let Err(error) =
                                            self.editor.set_track_channel(track.id, channel)
                                    {
                                        self.status = Some(error);
                                    }
                                }
                            });
                    });
                    let instrument = track.soundfont.as_ref().map_or_else(
                        || {
                            track.program.map_or_else(
                                || String::from("Built-in SoundFont"),
                                |program| format!("Program {program}"),
                            )
                        },
                        |font| format!("{}:{} {}", font.bank, font.program, font.path),
                    );
                    ui.label(
                        egui::RichText::new(instrument)
                            .size(10.0)
                            .color(theme::text_muted()),
                    );
                    if track.soundfont.is_none() {
                        let selected = track
                            .program
                            .map_or("Built-in SoundFont", midi_devices::programs::name);
                        egui::ComboBox::from_id_salt("track-program")
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(track.program.is_none(), "Built-in SoundFont")
                                    .clicked()
                                {
                                    let _ = self.editor.set_track_program(track.id, None);
                                }
                                for (value, name) in
                                    midi_devices::programs::GM_NAMES.iter().enumerate()
                                {
                                    let program = u8::try_from(value).unwrap_or(0);
                                    if ui
                                        .selectable_label(track.program == Some(program), *name)
                                        .clicked()
                                    {
                                        let _ =
                                            self.editor.set_track_program(track.id, Some(program));
                                    }
                                }
                            });
                    }
                    if track.soundfont.is_none() {
                        ui.label(
                            egui::RichText::new("General MIDI · 128 sounds")
                                .size(9.0)
                                .color(theme::text_dim()),
                        );
                    }
                    ui.horizontal(|ui| {
                        let mut muted = track.muted;
                        let mut soloed = track.soloed;
                        let mut armed = track.armed;
                        let changed = ui.checkbox(&mut muted, "M").changed()
                            | ui.checkbox(&mut soloed, "S").changed()
                            | ui.checkbox(&mut armed, "R").changed();
                        if changed
                            && let Err(error) =
                                self.editor.set_track_flags(track.id, muted, soloed, armed)
                        {
                            self.status = Some(error);
                        }
                    });
                    if ui.button("Effects…").clicked() {
                        self.view = View::Mix;
                    }
                    if ui.button("SoundFont…").clicked() {
                        self.view = View::Mix;
                        self.status =
                            Some(String::from("SoundFont controls are in the Mixer view"));
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new("SONG")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.song_name_input)
                                .desired_width(160.0),
                        )
                        .lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        let name = self.song_name_input.trim().to_string();
                        match self.editor.rename_song(&name) {
                            Ok(()) => self.status = Some(format!("song renamed to {name:?}")),
                            Err(error) => self.status = Some(error),
                        }
                    }
                    let key = score::spelling::detect_key(self.song());
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} {}",
                            if key.minor {
                                key.tonic.name_flat()
                            } else {
                                key.tonic.name_sharp()
                            },
                            if key.minor { "minor" } else { "major" }
                        ));
                        if ui.small_button("Find").on_hover_text("Find songs").clicked() {
                            self.songs_open = true;
                        }
                    });
                    let meter = self.song().time_signature;
                    ui.horizontal(|ui| {
                        ui.label("Meter");
                        egui::ComboBox::from_id_salt("song-meter")
                            .selected_text(format!(
                                "{}/{}",
                                meter.numerator, meter.denominator
                            ))
                            .show_ui(ui, |ui| {
                                for (numerator, denominator) in
                                    [(4_u8, 4_u8), (3, 4), (2, 4), (6, 8), (5, 4), (7, 8)]
                                {
                                    if ui
                                        .selectable_label(
                                            meter.numerator == numerator
                                                && meter.denominator == denominator,
                                            format!("{numerator}/{denominator}"),
                                        )
                                        .clicked()
                                    {
                                        meter_change = Some((numerator, denominator));
                                    }
                                }
                            });
                    });
                    ui.separator();
                    ui.label(
                        egui::RichText::new("METRONOME")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.settings.metronome, "Click on");
                        let beats = usize::from(self.song().time_signature.numerator.clamp(1, 12));
                        let (dots, _) = ui.allocate_exact_size(
                            egui::vec2(6.0 + beats as f32 * 13.0, 14.0),
                            egui::Sense::hover(),
                        );
                        for beat in 0..beats {
                            let centre = egui::pos2(
                                dots.left() + 6.0 + beat as f32 * 13.0,
                                dots.center().y,
                            );
                            ui.painter().circle_filled(
                                centre,
                                4.0,
                                if beat == 0 {
                                    theme::accent()
                                } else {
                                    theme::line_strong()
                                },
                            );
                        }
                    });
                    ui.add(
                        egui::Slider::new(&mut self.settings.metronome_accent, 0.0..=1.0)
                            .text("Accent"),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new("AUDIO OUTPUT")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    if let Some(player) = &self.player {
                        let buffer = player.buffer_frames().map_or_else(
                            || String::from("device buffer"),
                            |frames| {
                                format!(
                                    "{frames} frames · {:.1} ms",
                                    player.buffer_latency_ms().unwrap_or_default()
                                )
                            },
                        );
                        ui.label(format!(
                            "{} · {} Hz · {buffer}",
                            self.settings
                                .audio_output
                                .as_deref()
                                .unwrap_or("Default output"),
                            player.sample_rate()
                        ));
                    } else {
                        ui.label(
                            self.settings
                                .audio_output
                                .clone()
                                .unwrap_or_else(audio_engine_label),
                        );
                    }
                    if ui.button("Audio and MIDI settings").clicked() {
                        self.settings_open = true;
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new("MIDI INPUTS")
                            .size(9.0)
                            .color(theme::text_dim()),
                    );
                    let inputs = self
                        .midi_ports
                        .as_ref()
                        .map(|(inputs, _)| inputs.clone())
                        .unwrap_or_default();
                    if inputs.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "No MIDI inputs found. Plug in a keyboard, then open Audio and MIDI settings.",
                            )
                            .size(10.0)
                            .color(theme::text_dim()),
                        );
                    }
                    for input in inputs.iter().take(4) {
                        ui.label(
                            egui::RichText::new(input)
                                .size(10.0)
                                .color(theme::text_muted()),
                        );
                    }
                    if let Some(chord) = &held_chord {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Held chord: {chord}"))
                                    .size(11.0)
                                    .color(theme::accent()),
                            );
                            if ui
                                .small_button("Write at playhead")
                                .on_hover_text("Add this chord to the song at the playhead")
                                .clicked()
                            {
                                match self.editor.set_chord(self.position.max(0), chord) {
                                    Ok(()) => {
                                        self.status = Some(format!(
                                            "wrote chord {chord} at {}",
                                            self.position
                                        ));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                        });
                    }
                });
                let chat_rect = egui::Rect::from_min_max(
                    egui::pos2(full.left(), full.bottom() - chat_height),
                    full.max,
                );
                let mut chat_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(chat_rect)
                        .layout(egui::Layout::top_down(egui::Align::LEFT)),
                );
                self.assistant_inline(&mut chat_ui, chat_height);
            });
        if let Some((numerator, denominator)) = meter_change {
            match self.editor.set_time_signature(numerator, denominator) {
                Ok(()) => {
                    self.status = Some(format!("meter {numerator}/{denominator}"));
                }
                Err(error) => self.status = Some(error),
            }
        }
    }

    fn assistant_inline(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("ASSISTANT")
                    .size(9.0)
                    .color(theme::text_dim()),
            );
            if ui
                .small_button("Open")
                .on_hover_text("Open the larger chat window")
                .clicked()
            {
                self.assistant_open = true;
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("inspector-chat")
            .max_height((height - 62.0).max(40.0))
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.assistant_log.is_empty() {
                    ui.label(
                        egui::RichText::new("Ask for help or run a tool.")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                for line in &self.assistant_log {
                    ui.label(egui::RichText::new(line).monospace().size(9.0));
                }
            });
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.assistant_input)
                    .hint_text("help")
                    .desired_width(130.0),
            );
            let enter =
                response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (ui.small_button("Send").clicked() || enter) && !self.assistant_input.is_empty() {
                let input = self.assistant_input.trim().to_string();
                if input == "undo" || input.starts_with("tool ") {
                    self.selected_note = None;
                    self.selected_clip = None;
                    self.selected_clips.clear();
                }
                let reply = self.editor.assistant(&input);
                self.assistant_log.push(format!("> {input}"));
                self.assistant_log.push(reply.text);
                self.assistant_input.clear();
            }
        });
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
        let mut answer = None;
        egui::Window::new("Lessons")
            .open(&mut open)
            .default_width(400.0)
            .show(context, |ui| {
                egui::ComboBox::from_id_salt("lesson-kind")
                    .selected_text(self.lesson_kind.name())
                    .show_ui(ui, |ui| {
                        for kind in music_core::lesson::LessonKind::ALL {
                            if ui
                                .selectable_label(self.lesson_kind == kind, kind.name())
                                .clicked()
                            {
                                self.lesson_kind = kind;
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
            self.lesson_feedback = Some(correct);
        }
        if new_question {
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
                ui.label(
                    egui::RichText::new(
                        "Read-only; scales, roman labels, cadence and voice-leading cost.",
                    )
                    .size(10.0)
                    .color(theme::text_dim()),
                );
            });
        self.theory_open = open;
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

    fn controller_window(&mut self, context: &egui::Context) {
        if !self.controller_open {
            return;
        }
        let tracks: Vec<(TrackId, String)> = self
            .song()
            .tracks
            .iter()
            .map(|track| (track.id, track.name.clone()))
            .collect();
        let actions: Vec<midi_devices::map::Action> = vec![
            midi_devices::map::Action::Play,
            midi_devices::map::Action::Stop,
            midi_devices::map::Action::Undo,
            midi_devices::map::Action::Scene(0),
            midi_devices::map::Action::TrackFader(
                tracks
                    .first()
                    .map_or_else(|| TrackId::new(1).expect("valid"), |(id, _)| *id),
            ),
            midi_devices::map::Action::Pad(38),
        ];
        let mut open = self.controller_open;
        egui::Window::new("Controllers")
            .open(&mut open)
            .default_width(340.0)
            .show(context, |ui| {
                let choice = self.controller_action.min(actions.len() - 1);
                let mut chosen = choice;
                egui::ComboBox::from_id_salt("controller-action")
                    .selected_text(actions[choice].label())
                    .show_ui(ui, |ui| {
                        for (index, action) in actions.iter().enumerate() {
                            ui.selectable_value(&mut chosen, index, action.label());
                        }
                    });
                self.controller_action = chosen;
                if ui.button("Learn a controller").clicked() {
                    self.midi_learn = Some(midi_devices::map::Learn::begin(actions[chosen]));
                    self.status = Some(String::from("waiting for a note or CC"));
                }
                if ui
                    .button("Load Launchkey mixer preset")
                    .on_hover_text("CC 21-28 set faders for tracks 1-8")
                    .clicked()
                {
                    self.midi_map = midi_devices::map::launchkey_mixer_map();
                    let text = self.midi_map.to_text();
                    match std::fs::write("lazy-arranger-midi-map.txt", text) {
                        Ok(()) => {
                            self.status = Some(String::from("loaded Launchkey mixer preset"));
                        }
                        Err(error) => self.status = Some(format!("midi map: {error}")),
                    }
                }
                if self.midi_learn.is_some() {
                    ui.label(
                        egui::RichText::new("Waiting for the next note or control change…")
                            .color(theme::accent()),
                    );
                }
                ui.separator();
                ui.label(egui::RichText::new("Recent MIDI").strong());
                if self.midi_recent.is_empty() {
                    ui.label(
                        egui::RichText::new("no messages yet")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                }
                for line in self.midi_recent.iter().take(8) {
                    ui.label(
                        egui::RichText::new(line)
                            .monospace()
                            .size(9.0)
                            .color(theme::text_muted()),
                    );
                }
                ui.separator();
                for (number, action) in self.midi_map.controller_bindings() {
                    let value = self.midi_cc_values[usize::from(number)];
                    ui.horizontal(|ui| {
                        ui.label(format!("CC {number}"));
                        ui.add(
                            egui::ProgressBar::new(f32::from(value) / 127.0)
                                .desired_width(120.0)
                                .text(format!("{value}")),
                        );
                        ui.label(action.label());
                    });
                }
                if self.midi_map.controller_bindings().is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "no controller bindings — choose an action and press Learn",
                        )
                        .size(10.0)
                        .color(theme::text_dim()),
                    );
                }
            });
        self.controller_open = open;
    }

    fn midi_map_window(&mut self, context: &egui::Context) {
        if !self.midi_map_open {
            return;
        }
        let tracks: Vec<(TrackId, String)> = self
            .song()
            .tracks
            .iter()
            .map(|track| (track.id, track.name.clone()))
            .collect();
        let actions: Vec<midi_devices::map::Action> = vec![
            midi_devices::map::Action::Play,
            midi_devices::map::Action::Stop,
            midi_devices::map::Action::Undo,
            midi_devices::map::Action::Scene(0),
            midi_devices::map::Action::TrackFader(
                tracks
                    .first()
                    .map_or_else(|| TrackId::new(1).expect("valid"), |(id, _)| *id),
            ),
            midi_devices::map::Action::Pad(38),
        ];
        let mut open = self.midi_map_open;
        egui::Window::new("MIDI map")
            .open(&mut open)
            .default_width(340.0)
            .show(context, |ui| {
                let choice = self.midi_learn_choice.min(actions.len() - 1);
                let mut chosen = choice;
                egui::ComboBox::from_id_salt("midi-map-action")
                    .selected_text(actions[choice].label())
                    .show_ui(ui, |ui| {
                        for (index, action) in actions.iter().enumerate() {
                            ui.selectable_value(&mut chosen, index, action.label());
                        }
                    });
                self.midi_learn_choice = chosen;
                ui.horizontal(|ui| {
                    if ui.button("Learn").clicked() {
                        self.midi_learn = Some(midi_devices::map::Learn::begin(actions[chosen]));
                        self.status = Some(String::from("waiting for a note or CC"));
                    }
                    if self.midi_learn.is_some() && ui.button("Cancel").clicked() {
                        self.midi_learn = None;
                        self.status = Some(String::from("learn cancelled"));
                    }
                    if ui.button("Clear all").clicked() {
                        self.midi_map = midi_devices::map::MidiMap::default();
                        let _ =
                            std::fs::write("lazy-arranger-midi-map.txt", self.midi_map.to_text());
                        self.status = Some(String::from("MIDI map cleared"));
                    }
                    if ui
                        .button("Launchkey pads")
                        .on_hover_text("Notes 36-51 audition the matching drum pad")
                        .clicked()
                    {
                        self.midi_map = midi_devices::map::launchkey_pad_map();
                        let _ =
                            std::fs::write("lazy-arranger-midi-map.txt", self.midi_map.to_text());
                        self.status = Some(String::from("Launchkey pad map loaded"));
                    }
                });
                if self.midi_learn.is_some() {
                    ui.label(
                        egui::RichText::new("Waiting for the next note or control change…")
                            .color(theme::accent()),
                    );
                }
                ui.separator();
                if self.midi_map.bindings.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "no bindings — choose an action, press Learn, then move a control",
                        )
                        .color(theme::text_dim()),
                    );
                }
                for binding in &self.midi_map.bindings {
                    ui.label(format!(
                        "{} = {}",
                        binding.control_label(),
                        binding.action.label()
                    ));
                }
                ui.label(
                    egui::RichText::new("Learn saves the map automatically.")
                        .size(10.0)
                        .color(theme::text_dim()),
                );
            });
        self.midi_map_open = open;
    }

    fn inspector_window(&mut self, context: &egui::Context) {
        if !self.inspector_open {
            return;
        }
        let track_id = self
            .roll
            .selected
            .or_else(|| self.song().tracks.first().map(|track| track.id));
        let Some(track_id) = track_id else {
            self.inspector_open = false;
            return;
        };
        let info = self
            .song()
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .map(|track| {
                (
                    track.name.clone(),
                    track.volume,
                    track.pan,
                    track.muted,
                    track.soloed,
                    track.armed,
                    track.program,
                    track.bank,
                    track.notes.len(),
                    track.clips.len(),
                    track.audio.clone(),
                    track.soundfont.clone(),
                    track.plugins.len(),
                    track.automation.len(),
                    track.bus,
                    track.send,
                    track.send_level,
                )
            });
        let Some((
            name,
            mut volume,
            mut pan,
            mut muted,
            mut soloed,
            mut armed,
            program,
            bank,
            notes,
            clips,
            audio,
            soundfont,
            plugins,
            automation,
            mut bus,
            mut send,
            mut send_level,
        )) = info
        else {
            self.inspector_open = false;
            return;
        };
        if self.inspector_track != Some(track_id) {
            self.inspector_track = Some(track_id);
            self.inspector_name = name.clone();
        }
        let mut open = self.inspector_open;
        let mut rename = false;
        egui::Window::new("Inspector")
            .open(&mut open)
            .default_width(320.0)
            .show(context, |ui| {
                let tracks: Vec<(TrackId, String)> = self
                    .song()
                    .tracks
                    .iter()
                    .map(|track| (track.id, track.name.clone()))
                    .collect();
                let mut chosen = track_id;
                egui::ComboBox::from_id_salt("inspector-track")
                    .selected_text(&self.inspector_name)
                    .show_ui(ui, |ui| {
                        for (id, label) in &tracks {
                            ui.selectable_value(&mut chosen, *id, label);
                        }
                    });
                if chosen != track_id {
                    self.roll.selected = Some(chosen);
                    self.inspector_track = None;
                }
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.inspector_name);
                    if ui.button("Rename").clicked() {
                        rename = true;
                    }
                });
                if rename {
                    match self.editor.rename_track(track_id, &self.inspector_name) {
                        Ok(()) => self.status = Some(String::from("track renamed")),
                        Err(error) => self.status = Some(error),
                    }
                }
                let language = crate::strings::Language::parse(&self.settings.language);
                let nouns = crate::strings::nouns(language);
                let counts = format!(
                    "{} · {} · {}",
                    crate::strings::count(language, notes, nouns.note.0, nouns.note.1),
                    crate::strings::count(language, clips, nouns.clip.0, nouns.clip.1),
                    crate::strings::count(language, automation, nouns.lane.0, nouns.lane.1)
                );
                ui.label(
                    egui::RichText::new(counts)
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                let volume_changed = ui
                    .add(
                        egui::Slider::new(&mut volume, 0..=127)
                            .text("volume")
                            .fixed_decimals(0),
                    )
                    .changed();
                let pan_changed = ui
                    .add(
                        egui::Slider::new(&mut pan, 0..=127)
                            .text("pan")
                            .fixed_decimals(0),
                    )
                    .changed();
                if (volume_changed || pan_changed)
                    && let Err(error) = self.editor.set_track_gain(track_id, volume, pan)
                {
                    self.status = Some(error);
                }
                ui.horizontal(|ui| {
                    let changed = ui.checkbox(&mut muted, "Mute").changed()
                        | ui.checkbox(&mut soloed, "Solo").changed()
                        | ui.checkbox(&mut armed, "Arm").changed();
                    if changed
                        && let Err(error) =
                            self.editor.set_track_flags(track_id, muted, soloed, armed)
                    {
                        self.status = Some(error);
                    }
                });
                ui.horizontal(|ui| {
                    let mut enabled = program.is_some();
                    let mut number = i32::from(program.unwrap_or(0));
                    let toggled = ui.checkbox(&mut enabled, "Program").changed();
                    let changed = ui
                        .add(egui::DragValue::new(&mut number).range(0..=127))
                        .changed();
                    if toggled || changed {
                        let value = enabled.then_some(number as u8);
                        if let Err(error) = self.editor.set_track_program(track_id, value) {
                            self.status = Some(error);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let mut enabled = bank.is_some();
                    let mut msb = i32::from(bank.map_or(0, |bank| bank.msb));
                    let mut lsb = i32::from(bank.map_or(0, |bank| bank.lsb));
                    let toggled = ui.checkbox(&mut enabled, "Bank").changed();
                    let changed = ui
                        .add(egui::DragValue::new(&mut msb).range(0..=127))
                        .changed()
                        | ui.add(egui::DragValue::new(&mut lsb).range(0..=127))
                            .changed();
                    if toggled || changed {
                        let value = enabled.then_some(project::song::BankSelect {
                            msb: u8::try_from(msb).unwrap_or(0),
                            lsb: u8::try_from(lsb).unwrap_or(0),
                        });
                        if let Err(error) = self.editor.set_track_bank(track_id, value) {
                            self.status = Some(error);
                        }
                    }
                });
                ui.separator();
                ui.label(
                    egui::RichText::new("Tempo changes")
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                for change in self.song().tempo_map.changes().iter().take(5) {
                    ui.label(
                        egui::RichText::new(format!("{} bpm at tick {}", change.bpm, change.tick))
                            .size(10.0)
                            .color(theme::text_muted()),
                    );
                }
                let meter = self.song().time_signature;
                let mut numerator = meter.numerator as u32;
                let mut denominator = meter.denominator as u32;
                ui.horizontal(|ui| {
                    ui.label("meter");
                    ui.add(egui::DragValue::new(&mut numerator).range(1..=32));
                    ui.label("/");
                    egui::ComboBox::from_id_salt("meter-denominator")
                        .selected_text(denominator.to_string())
                        .show_ui(ui, |ui| {
                            for choice in [1_u32, 2, 4, 8, 16, 32] {
                                ui.selectable_value(&mut denominator, choice, choice.to_string());
                            }
                        });
                    if ui.button("Set meter").clicked() {
                        match self.editor.set_time_signature(
                            u8::try_from(numerator).unwrap_or(4),
                            u8::try_from(denominator).unwrap_or(4),
                        ) {
                            Ok(()) => {
                                self.status = Some(format!("meter {numerator}/{denominator}"));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.tempo_input)
                            .range(20.0..=300.0)
                            .suffix(" bpm"),
                    );
                    if ui.small_button("Current").clicked() {
                        self.tempo_input = self.song().tempo_map.bpm_at(self.position);
                    }
                    if ui.button("Set at playhead").clicked() {
                        match self
                            .editor
                            .set_tempo_at(self.position.max(0), self.tempo_input)
                        {
                            Ok(()) => {
                                self.status = Some(format!(
                                    "tempo {:.1} bpm at {}",
                                    self.tempo_input, self.position
                                ));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
                let bus_options: Vec<(u8, String)> = self
                    .song()
                    .buses
                    .iter()
                    .map(|bus| (bus.id, bus.name.clone()))
                    .collect();
                let mut routing_changed = false;
                ui.horizontal(|ui| {
                    ui.label("Bus");
                    egui::ComboBox::from_id_salt("track-bus")
                        .selected_text(
                            bus.and_then(|id| {
                                bus_options
                                    .iter()
                                    .find(|(option, _)| *option == id)
                                    .map(|(_, name)| name.as_str())
                            })
                            .unwrap_or("None"),
                        )
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(bus.is_none(), "None").clicked() {
                                bus = None;
                                routing_changed = true;
                            }
                            for (id, name) in &bus_options {
                                if ui.selectable_label(bus == Some(*id), name).clicked() {
                                    bus = Some(*id);
                                    routing_changed = true;
                                }
                            }
                        });
                    ui.label("Send");
                    egui::ComboBox::from_id_salt("track-send")
                        .selected_text(
                            send.and_then(|id| {
                                bus_options
                                    .iter()
                                    .find(|(option, _)| *option == id)
                                    .map(|(_, name)| name.as_str())
                            })
                            .unwrap_or("None"),
                        )
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(send.is_none(), "None").clicked() {
                                send = None;
                                routing_changed = true;
                            }
                            for (id, name) in &bus_options {
                                if ui.selectable_label(send == Some(*id), name).clicked() {
                                    send = Some(*id);
                                    routing_changed = true;
                                }
                            }
                        });
                    if ui
                        .add(egui::Slider::new(&mut send_level, 0..=127).text("send level"))
                        .changed()
                    {
                        routing_changed = true;
                    }
                });
                if routing_changed {
                    match self
                        .editor
                        .set_track_routing(track_id, bus, send, send_level)
                    {
                        Ok(()) => self.status = Some(String::from("routing updated")),
                        Err(error) => self.status = Some(error),
                    }
                }
                if let Some(reference) = &audio {
                    ui.label(
                        egui::RichText::new(format!(
                            "audio {} (fades {} / {})",
                            reference.path, reference.fade_in, reference.fade_out
                        ))
                        .size(10.0)
                        .color(theme::text_muted()),
                    );
                }
                if let Some(font) = &soundfont {
                    ui.label(
                        egui::RichText::new(format!(
                            "soundfont {}:{} {}",
                            font.bank, font.program, font.path
                        ))
                        .size(10.0)
                        .color(theme::text_muted()),
                    );
                }
                ui.label(
                    egui::RichText::new(plural(plugins, "plug-in slot"))
                        .size(10.0)
                        .color(theme::text_muted()),
                );
            });
        self.inspector_open = open;
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

    fn device_editor_window(&mut self, context: &egui::Context) {
        if !self.device_editor_open {
            return;
        }
        let mut open = self.device_editor_open;
        egui::Window::new("Device editor")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                if self.device_profiles.is_empty() {
                    ui.label("No device profiles found. Add YAML files to the profiles folder.");
                    return;
                }
                let selected = self
                    .device_profiles
                    .get(self.device_editor_profile)
                    .map(|profile| profile.name.clone())
                    .unwrap_or_default();
                egui::ComboBox::from_id_salt("device-editor-profile")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (index, profile) in self.device_profiles.iter().enumerate() {
                            if ui
                                .selectable_value(
                                    &mut self.device_editor_profile,
                                    index,
                                    &profile.name,
                                )
                                .clicked()
                            {
                                self.device_editor_values.clear();
                            }
                        }
                    });
                let controls = self
                    .device_profiles
                    .get(self.device_editor_profile)
                    .map(|profile| profile.controls.clone())
                    .unwrap_or_default();
                if self.device_editor_values.len() != controls.len() {
                    self.device_editor_values =
                        controls.iter().map(|control| control.min).collect();
                }
                for (index, control) in controls.iter().enumerate() {
                    let name = control.name.clone();
                    let cc = control.cc;
                    let channel_number = control.channel;
                    let min = control.min;
                    let max = control.max;
                    let Some(value) = self.device_editor_values.get(index).copied() else {
                        continue;
                    };
                    let mut current = value;
                    if ui
                        .add(egui::Slider::new(&mut current, min..=max).text(name))
                        .changed()
                    {
                        if let Some(slot) = self.device_editor_values.get_mut(index) {
                            *slot = current;
                        }
                        if let (Ok(channel), Ok(controller)) = (
                            music_core::Channel::from_one_based(channel_number),
                            music_core::ControlNumber::new(cc),
                        ) {
                            self.send_midi(music_core::MidiMessage::ControlChange {
                                channel,
                                controller,
                                value: current,
                            });
                        }
                    }
                }
                if controls.is_empty() {
                    ui.label("This profile has no controls.");
                }
                if ui
                    .button("Send all")
                    .on_hover_text("Send every control's current value")
                    .clicked()
                {
                    let sends: Vec<(u8, u8, u8)> = controls
                        .iter()
                        .zip(&self.device_editor_values)
                        .map(|(control, value)| (control.channel, control.cc, *value))
                        .collect();
                    for (channel_number, cc, value) in sends {
                        if let (Ok(channel), Ok(controller)) = (
                            music_core::Channel::from_one_based(channel_number),
                            music_core::ControlNumber::new(cc),
                        ) {
                            self.send_midi(music_core::MidiMessage::ControlChange {
                                channel,
                                controller,
                                value,
                            });
                        }
                    }
                }
                ui.separator();
                ui.label("System exclusive (hex)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.device_sysex)
                        .hint_text("F0 7E ... F7")
                        .desired_width(280.0),
                );
                if ui.button("Send SysEx").clicked() {
                    match parse_sysex(&self.device_sysex) {
                        Ok(bytes) => self.send_raw(&bytes),
                        Err(error) => self.status = Some(error),
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Dump file");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.device_dump_path).desired_width(200.0),
                    );
                    if ui.button("Save dump").clicked() {
                        match parse_sysex(&self.device_sysex) {
                            Ok(bytes) => match std::fs::write(&self.device_dump_path, &bytes) {
                                Ok(()) => {
                                    self.status = Some(format!(
                                        "wrote {} byte(s) to {}",
                                        bytes.len(),
                                        self.device_dump_path
                                    ));
                                }
                                Err(error) => self.status = Some(error.to_string()),
                            },
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Load dump").clicked() {
                        match std::fs::read(&self.device_dump_path) {
                            Ok(bytes) => {
                                self.device_sysex = sysex_hex(&bytes);
                                self.send_raw(&bytes);
                                self.status =
                                    Some(format!("loaded and sent {} byte(s)", bytes.len()));
                            }
                            Err(error) => self.status = Some(error.to_string()),
                        }
                    }
                    if ui.button("Send file").clicked() {
                        match std::fs::read(&self.device_dump_path) {
                            Ok(bytes) => {
                                self.send_raw(&bytes);
                                self.status = Some(format!("sent {} byte(s)", bytes.len()));
                            }
                            Err(error) => self.status = Some(error.to_string()),
                        }
                    }
                });
            });
        self.device_editor_open = open;
    }

    fn groove_window(&mut self, context: &egui::Context) {
        if !self.groove_open {
            return;
        }
        let mut open = self.groove_open;
        egui::Window::new("Groove library")
            .open(&mut open)
            .resizable(false)
            .show(context, |ui| {
                if ui.button("Refresh").clicked() || self.groove_entries.is_empty() {
                    self.groove_entries =
                        arranging::groove_file::list(std::path::Path::new("grooves"))
                            .unwrap_or_default();
                }
                let selected = self
                    .groove_entries
                    .get(self.groove_selected)
                    .map(|entry| entry.0.clone())
                    .unwrap_or_else(|| String::from("No groove files"));
                egui::ComboBox::from_id_salt("groove-file")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (index, (name, _)) in self.groove_entries.iter().enumerate() {
                            ui.selectable_value(&mut self.groove_selected, index, name);
                        }
                    });
                if ui.button("Load").clicked()
                    && let Some((_, path)) = self.groove_entries.get(self.groove_selected)
                {
                    match arranging::groove_file::load(path) {
                        Ok((_, groove)) => {
                            self.groove_loaded = Some(groove);
                            self.status = Some(String::from("groove loaded"));
                        }
                        Err(error) => self.status = Some(error),
                    }
                }
                ui.add(egui::Slider::new(&mut self.groove_timing, 0.0..=1.0).text("timing"));
                ui.add(egui::Slider::new(&mut self.groove_accents, 0.0..=1.0).text("accents"));
                if let Some(groove) = &self.groove_loaded {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} slots · grid {}",
                            groove.offsets.len(),
                            groove.grid
                        ))
                        .size(10.0)
                        .color(theme::text_dim()),
                    );
                }
                if ui.button("Apply to selected track").clicked()
                    && let Some(groove) = &self.groove_loaded
                    && let Some(track) = self.roll.selected
                {
                    let amounts = arranging::groove::Amounts {
                        timing: self.groove_timing,
                        accents: self.groove_accents,
                    };
                    match self.editor.apply_stored_groove(track, groove, amounts) {
                        Ok(count) => {
                            self.status = Some(format!("groove applied to {count} note(s)"))
                        }
                        Err(error) => self.status = Some(error),
                    }
                }
            });
        self.groove_open = open;
    }

    fn windows(&mut self, context: &egui::Context) {
        self.about_window(context);
        self.groove_window(context);
        self.device_editor_window(context);
        if self.settings_open {
            let mut open = self.settings_open;
            egui::Window::new("Settings")
                .open(&mut open)
                .collapsible(false)
                .show(context, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Language");
                        egui::ComboBox::from_id_salt("settings-language")
                            .selected_text(if self.settings.language == "system" {
                                "System default"
                            } else {
                                &self.settings.language
                            })
                            .show_ui(ui, |ui| {
                                for tag in [
                                    "system", "en", "da", "fr", "vi", "de", "es", "it", "pt",
                                ] {
                                    ui.selectable_value(
                                        &mut self.settings.language,
                                        tag.to_string(),
                                        if tag == "system" { "System default" } else { tag },
                                    );
                                }
                            });
                        ui.label(
                            egui::RichText::new(
                                "English, Danish, French, Vietnamese, German, Spanish, Italian, Portuguese or System default",
                            )
                            .size(9.0)
                            .color(theme::text_dim()),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Device profile");
                        let selected = self
                            .settings
                            .device_profile
                            .as_ref()
                            .and_then(|id| {
                                self.device_profiles
                                    .iter()
                                    .find(|profile| &profile.id == id)
                            })
                            .map(|profile| profile.name.clone())
                            .unwrap_or_else(|| String::from("Built-in PSR-S970"));
                        egui::ComboBox::from_id_salt("device-profile")
                            .selected_text(selected)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        self.settings.device_profile.is_none(),
                                        "Built-in PSR-S970",
                                    )
                                    .clicked()
                                {
                                    self.settings.device_profile = None;
                                }
                                for profile in &self.device_profiles {
                                    if ui
                                        .selectable_label(
                                            self.settings.device_profile.as_ref()
                                                == Some(&profile.id),
                                            &profile.name,
                                        )
                                        .clicked()
                                    {
                                        self.settings.device_profile =
                                            Some(profile.id.clone());
                                    }
                                }
                            });
                        if ui.button("Reload").clicked() {
                            self.device_profiles = load_device_profiles();
                        }
                    });
                    ui.add(egui::Slider::new(&mut self.settings.grade, 1..=9).text("chord grade"));
                    let mut keyboard = self.settings.keyboard.clone().unwrap_or_default();
                    if ui
                        .add(egui::TextEdit::singleline(&mut keyboard).hint_text("MIDI keyboard"))
                        .changed()
                    {
                        self.settings.keyboard = if keyboard.trim().is_empty() {
                            None
                        } else {
                            Some(keyboard)
                        };
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Find MIDI inputs").clicked() {
                            match midi_devices::devices::list_ports() {
                                Ok(names) => {
                                    self.status = Some(crate::settings::ports_summary(
                                        &names.inputs,
                                        &names.outputs,
                                    ));
                                    self.midi_ports = Some((names.inputs, names.outputs));
                                }
                                Err(error) => self.status = Some(error.to_string()),
                            }
                        }
                        if let Some((inputs, _)) = &self.midi_ports {
                            for name in inputs {
                                if ui
                                    .selectable_label(
                                        self.settings.keyboard.as_deref() == Some(name.as_str()),
                                        name,
                                    )
                                    .clicked()
                                {
                                    self.settings.keyboard = Some(name.clone());
                                    self.midi_input = None;
                                    let _ = self.settings.save(&crate::settings::Settings::path());
                                    self.status = Some(format!("MIDI input: {name}"));
                                }
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Find audio inputs").clicked() {
                            let inputs = audio::record::input_devices();
                            self.status = Some(format!("{} audio input(s)", inputs.len()));
                            self.audio_inputs = Some(inputs);
                        }
                        if ui
                            .selectable_label(
                                self.settings.audio_input.is_none(),
                                "Default input",
                            )
                            .clicked()
                        {
                            self.settings.audio_input = None;
                            let _ = self.settings.save(&crate::settings::Settings::path());
                        }
                        if let Some(inputs) = &self.audio_inputs {
                            for name in inputs {
                                if ui
                                    .selectable_label(
                                        self.settings.audio_input.as_deref() == Some(name.as_str()),
                                        name,
                                    )
                                    .clicked()
                                {
                                    self.settings.audio_input = Some(name.clone());
                                    let _ = self.settings.save(&crate::settings::Settings::path());
                                    self.status = Some(format!("audio input: {name}"));
                                }
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Find audio outputs").clicked() {
                            let outputs = audio::player::output_devices();
                            self.status = Some(format!("{} audio output(s)", outputs.len()));
                            self.audio_outputs = Some(outputs);
                        }
                        if ui
                            .selectable_label(
                                self.settings.audio_output.is_none(),
                                "Default output",
                            )
                            .clicked()
                        {
                            self.settings.audio_output = None;
                            let _ = self.settings.save(&crate::settings::Settings::path());
                        }
                        if let Some(outputs) = &self.audio_outputs {
                            egui::ComboBox::from_id_salt("audio-output")
                                .selected_text(
                                    self.settings
                                        .audio_output
                                        .as_deref()
                                        .unwrap_or("Choose output"),
                                )
                                .show_ui(ui, |ui| {
                                    for name in outputs {
                                        if ui
                                            .selectable_label(
                                                self.settings.audio_output.as_deref()
                                                    == Some(name.as_str()),
                                                name,
                                            )
                                            .clicked()
                                        {
                                            self.settings.audio_output = Some(name.clone());
                                            let _ = self.settings.save(
                                                &crate::settings::Settings::path(),
                                            );
                                            self.status =
                                                Some(format!("audio output: {name}"));
                                        }
                                    }
                                });
                        }
                        ui.label("buffer");
                        let mut frames = self.settings.audio_buffer_frames;
                        egui::ComboBox::from_id_salt("audio-buffer")
                            .selected_text(if frames == 0 {
                                String::from("Default")
                            } else {
                                format!("{frames}")
                            })
                            .show_ui(ui, |ui| {
                                for option in [0_u32, 128, 256, 512, 1024] {
                                    let label = if option == 0 {
                                        String::from("Default")
                                    } else {
                                        format!("{option}")
                                    };
                                    if ui
                                        .selectable_label(frames == option, label)
                                        .clicked()
                                    {
                                        frames = option;
                                    }
                                }
                            });
                        if frames != self.settings.audio_buffer_frames {
                            self.settings.audio_buffer_frames = frames;
                            let _ = self.settings.save(&crate::settings::Settings::path());
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Input channel");
                        egui::ComboBox::from_id_salt("audio-input-channel")
                            .selected_text(if self.settings.audio_input_channel == 0 {
                                String::from("Mix")
                            } else {
                                format!("CH{}", self.settings.audio_input_channel)
                            })
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        self.settings.audio_input_channel == 0,
                                        "Mix",
                                    )
                                    .clicked()
                                {
                                    self.settings.audio_input_channel = 0;
                                }
                                for channel in 1..=8 {
                                    if ui
                                        .selectable_label(
                                            self.settings.audio_input_channel == channel,
                                            format!("CH{channel}"),
                                        )
                                        .clicked()
                                    {
                                        self.settings.audio_input_channel = channel;
                                    }
                                }
                            });
                    });
                    if ui.button("Check for updates").clicked() {
                        match crate::update::fetch_release() {
                            Ok(release) => {
                                let current = env!("CARGO_PKG_VERSION");
                                if crate::update::should_offer(
                                    &release.tag,
                                    current,
                                    &self.settings.skipped_update,
                                ) {
                                    self.status = Some(crate::update::describe_release(
                                        current,
                                        &release.tag,
                                        &release.url,
                                    ));
                                    self.available_update = Some(release);
                                } else if release.tag == self.settings.skipped_update {
                                    self.status = Some(format!(
                                        "Update {} skipped in Settings",
                                        release.tag
                                    ));
                                } else {
                                    self.status = Some(crate::update::describe_release(
                                        current,
                                        &release.tag,
                                        &release.url,
                                    ));
                                }
                            }
                            Err(error) => {
                                self.status = Some(format!("Update check failed: {error}"));
                            }
                        }
                    }
                    if let Some(release) = self.available_update.clone() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Release {}", release.tag));
                            if ui.button("Open release page").clicked() {
                                crate::update::open_url(&release.url);
                            }
                            if let Some(asset) = crate::update::preferred_asset(&release)
                                && ui.button(format!("Download {}", asset.name)).clicked()
                            {
                                let asset = asset.clone();
                                let directory = std::env::var_os("HOME")
                                    .map(std::path::PathBuf::from)
                                    .unwrap_or_else(std::env::temp_dir)
                                    .join("Downloads");
                                let (sender, receiver) = std::sync::mpsc::channel();
                                self.update_download = Some(receiver);
                                self.status = Some(format!("downloading {}", asset.name));
                                std::thread::spawn(move || {
                                    let _ = sender.send(crate::update::download_asset(
                                        &asset, &directory,
                                    ));
                                });
                            }
                            if ui.button("Skip this version").clicked() {
                                self.settings.skipped_update = release.tag.clone();
                                let _ = self.settings.save(&crate::settings::Settings::path());
                                self.available_update = None;
                                self.status = Some(format!("Update {} skipped", release.tag));
                            }
                        });
                    }
                    if ui.button("Show device profiles folder").clicked() {
                        let folder = crate::settings::Settings::path()
                            .parent()
                            .map(|path| path.join("profiles"))
                            .unwrap_or_else(|| std::path::PathBuf::from("profiles"));
                        if let Err(error) = std::fs::create_dir_all(&folder) {
                            self.status = Some(format!("{}: {error}", folder.display()));
                        } else {
                            self.status = Some(format!("device profiles: {}", folder.display()));
                            #[cfg(target_os = "macos")]
                            {
                                let _ = std::process::Command::new("open")
                                    .arg(&folder)
                                    .spawn();
                            }
                        }
                    }
                    ui.checkbox(&mut self.settings.metronome, "metronome with playback");
                    ui.checkbox(
                        &mut self.settings.record_chords,
                        "detect chords after MIDI recording",
                    );
                    ui.checkbox(
                        &mut self.settings.monitor,
                        "monitor input through output (feedback risk)",
                    );
                    ui.checkbox(
                        &mut self.settings.record_quantise,
                        "quantise recorded notes to 1/16",
                    );
                    ui.checkbox(&mut self.settings.reduced_motion, "reduced motion");
                    if ui
                        .checkbox(&mut self.settings.kids_mode, "Kids Mode")
                        .changed()
                    {
                        self.view = if self.settings.kids_mode {
                            View::Perform
                        } else {
                            View::Arrange
                        };
                    }
                    if self.settings.kids_mode {
                        ui.add(
                            egui::Slider::new(&mut self.settings.kids_pause_seconds, 0.5..=5.0)
                                .text("Kids pause (s)"),
                        );
                        ui.checkbox(
                            &mut self.settings.kids_keys_play,
                            "Keys also play their own notes",
                        );
                    }
                    let mut light_theme = self.settings.light_theme;
                    if ui.checkbox(&mut light_theme, "Light theme").changed() {
                        self.settings.light_theme = light_theme;
                        theme::set_light(light_theme);
                        theme::refresh(ui.ctx());
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new("AI endpoint (OpenAI-compatible)")
                            .size(11.0)
                            .color(theme::text_muted()),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.ai_url)
                            .hint_text("http://localhost:11434/v1/chat/completions")
                            .desired_width(340.0),
                    );
                    ui.horizontal(|ui| {
                        ui.label("model");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings.ai_model)
                                .desired_width(180.0),
                        );
                        ui.label("key");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings.ai_api_key)
                                .password(true)
                                .desired_width(140.0),
                        );
                    });
                    ui.label(
                        egui::RichText::new(
                            "The key is stored in plain text in the settings file. Leave the URL empty to stay local.",
                        )
                        .size(9.0)
                        .color(theme::text_dim()),
                    );
                    ui.label(
                        egui::RichText::new("Local assistant command (use {prompt})")
                            .size(11.0)
                            .color(theme::text_muted()),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.assistant_command)
                            .hint_text("codex exec {prompt}")
                            .desired_width(340.0),
                    );
                    ui.label(
                        egui::RichText::new("Speak command (use {text})")
                            .size(11.0)
                            .color(theme::text_muted()),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.speak_command)
                            .hint_text("kokoro-tts {text}")
                            .desired_width(340.0),
                    );
                    ui.label(
                        egui::RichText::new("Whisper command (use {file})")
                            .size(11.0)
                            .color(theme::text_muted()),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.speech_command)
                            .hint_text("whisper-cli -f {file} -nt")
                            .desired_width(340.0),
                    );
                    if ui.button("Save settings").clicked() {
                        match self.settings.save(&crate::settings::Settings::path()) {
                            Ok(()) => {
                                self.status = Some(String::from("settings saved"));
                                self.settings_open = false;
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
            if !open {
                self.settings_open = false;
            }
        }
        if self.shortcuts_open {
            let mut open = self.shortcuts_open;
            let mut undo_keys = self.settings.undo_keys.clone();
            let mut redo_keys = self.settings.redo_keys.clone();
            let mut save_keys = self.settings.save_keys.clone();
            let mut save_as_keys = self.settings.save_as_keys.clone();
            let mut open_keys = self.settings.open_keys.clone();
            let mut play_keys = self.settings.play_keys.clone();
            let mut save_bindings = false;
            egui::Window::new("Shortcuts")
                .open(&mut open)
                .collapsible(false)
                .default_width(420.0)
                .show(context, |ui| {
                    ui.label(egui::RichText::new("Editable bindings").strong());
                    for (label, value) in [
                        ("Undo", &mut undo_keys),
                        ("Redo", &mut redo_keys),
                        ("Save", &mut save_keys),
                        ("Save as", &mut save_as_keys),
                        ("Open", &mut open_keys),
                        ("Play/pause", &mut play_keys),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            ui.text_edit_singleline(value);
                        });
                    }
                    let conflicts = crate::settings::binding_conflicts(&[
                        ("Undo", &undo_keys),
                        ("Redo", &redo_keys),
                        ("Save", &save_keys),
                        ("Save as", &save_as_keys),
                        ("Open", &open_keys),
                        ("Play/pause", &play_keys),
                    ]);
                    if conflicts.is_empty() {
                        ui.label(
                            egui::RichText::new("No duplicate bindings.")
                                .size(10.0)
                                .color(theme::text_dim()),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(format!("Conflict: {}", conflicts.join(", ")))
                                .size(10.0)
                                .color(theme::recording()),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Save bindings").clicked() {
                            save_bindings = true;
                        }
                        if ui.button("Reset defaults").clicked() {
                            let defaults = crate::settings::Settings::default();
                            undo_keys = defaults.undo_keys;
                            redo_keys = defaults.redo_keys;
                            save_keys = defaults.save_keys;
                            save_as_keys = defaults.save_as_keys;
                            open_keys = defaults.open_keys;
                            play_keys = defaults.play_keys;
                        }
                    });
                    ui.label(
                        egui::RichText::new(
                            "Use cmd/ctrl/shift/alt plus a key, for example cmd+shift+z.",
                        )
                        .size(10.0)
                        .color(theme::text_dim()),
                    );
                    ui.separator();
                    ui.label(egui::RichText::new("Other shortcuts").strong());
                    for shortcut in crate::settings::SHORTCUTS {
                        if matches!(
                            shortcut.action,
                            "toggle playback" | "save project" | "undo" | "redo"
                        ) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(shortcut.keys).monospace().size(10.0));
                            ui.label(shortcut.action);
                            ui.label(
                                egui::RichText::new(shortcut.context)
                                    .size(9.0)
                                    .color(theme::text_dim()),
                            );
                        });
                    }
                });
            if save_bindings {
                let values = [
                    &undo_keys,
                    &redo_keys,
                    &save_keys,
                    &save_as_keys,
                    &open_keys,
                    &play_keys,
                ];
                let labels = [
                    ("Undo", undo_keys.as_str()),
                    ("Redo", redo_keys.as_str()),
                    ("Save", save_keys.as_str()),
                    ("Save as", save_as_keys.as_str()),
                    ("Open", open_keys.as_str()),
                    ("Play/pause", play_keys.as_str()),
                ];
                if values
                    .iter()
                    .any(|value| crate::settings::parse_shortcut(value).is_none())
                {
                    self.status = Some(String::from("invalid shortcut binding"));
                } else if !crate::settings::binding_conflicts(&labels).is_empty() {
                    self.status = Some(String::from("shortcut conflict; change one binding"));
                } else {
                    self.settings.undo_keys = undo_keys;
                    self.settings.redo_keys = redo_keys;
                    self.settings.save_keys = save_keys;
                    self.settings.save_as_keys = save_as_keys;
                    self.settings.open_keys = open_keys;
                    self.settings.play_keys = play_keys;
                    match self.settings.save(&crate::settings::Settings::path()) {
                        Ok(()) => self.status = Some(String::from("shortcuts saved")),
                        Err(error) => self.status = Some(error),
                    }
                }
            }
            if !open {
                self.shortcuts_open = false;
            }
        }
    }

    /// Mixer strips: one per track with a fader, pan and mute/solo.
    fn mixer_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mixer");
        if self.stem_levels.is_empty()
            && self
                .player
                .as_ref()
                .is_some_and(audio::player::Player::is_playing)
        {
            self.stem_levels = audio::render_stems(self.song(), audio::SAMPLE_RATE)
                .into_iter()
                .map(|stem| (stem.track, stem.audio.samples))
                .collect();
        }
        ui.horizontal(|ui| {
            ui.label("Master");
            ui.add(
                egui::Slider::new(&mut self.settings.master_gain_db, -24.0..=12.0)
                    .suffix(" dB")
                    .fixed_decimals(1),
            );
            ui.checkbox(&mut self.settings.master_limiter, "Limiter");
            ui.add(
                egui::Slider::new(&mut self.settings.master_reverb, 0.0..=1.0)
                    .text("reverb")
                    .fixed_decimals(2),
            );
            ui.add(
                egui::Slider::new(&mut self.settings.master_chorus, 0.0..=1.0)
                    .text("chorus")
                    .fixed_decimals(2),
            );
        });
        let meter = self
            .player
            .as_ref()
            .filter(|player| player.is_playing())
            .map_or(
                audio::master::Meter::default(),
                audio::player::Player::meter,
            );
        ui.horizontal(|ui| {
            ui.label("Peak");
            ui.add(
                egui::ProgressBar::new(meter.peak.clamp(0.0, 1.0))
                    .desired_width(140.0)
                    .text(format!("{:.0}%", meter.peak * 100.0)),
            );
            ui.label("RMS");
            ui.add(
                egui::ProgressBar::new(meter.rms.clamp(0.0, 1.0))
                    .desired_width(140.0)
                    .text(format!("{:.0}%", meter.rms * 100.0)),
            );
        });
        ui.separator();
        let mut buses = self.song().buses.clone();
        let mut buses_edited = false;
        let mut remove_bus = None;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Buses").strong());
            if ui.small_button("Add bus").clicked() {
                let id = buses
                    .iter()
                    .map(|bus| bus.id)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1);
                buses.push(project::song::Bus {
                    id,
                    name: format!("Bus {id}"),
                    volume: 100,
                    pan: 64,
                    muted: false,
                    soloed: false,
                    plugins: Vec::new(),
                });
                buses_edited = true;
            }
        });
        for bus in &mut buses {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{} {}", bus.id, bus.name))
                        .size(10.0)
                        .color(theme::text_muted()),
                );
                if ui
                    .add(egui::Slider::new(&mut bus.volume, 0..=127).text("vol"))
                    .changed()
                {
                    buses_edited = true;
                }
                if ui
                    .add(egui::Slider::new(&mut bus.pan, 0..=127).text("pan"))
                    .changed()
                {
                    buses_edited = true;
                }
                if ui.small_button("Remove").clicked() {
                    remove_bus = Some(bus.id);
                }
            });
        }
        if let Some(id) = remove_bus {
            buses.retain(|bus| bus.id != id);
            buses_edited = true;
        }
        if buses_edited && let Err(error) = self.editor.set_buses(buses) {
            self.status = Some(error);
        }
        ui.collapsing("L6max takes", |ui| {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.l6max_folder);
                if ui.button("Scan").clicked() {
                    match audio::l6max::scan(std::path::Path::new(&self.l6max_folder)) {
                        Ok(plan) => {
                            self.l6max_error = None;
                            self.l6max_selected = vec![true; plan.takes.len()];
                            self.l6max_plan = Some(plan);
                        }
                        Err(error) => {
                            self.l6max_plan = None;
                            self.l6max_error = Some(error);
                        }
                    }
                }
            });
            if let Some(error) = &self.l6max_error {
                ui.label(egui::RichText::new(error).color(theme::recording()));
            }
            if let Some(plan) = &self.l6max_plan {
                ui.label(format!(
                    "{} take(s), {} at 32-bit float, {:.1} s total",
                    plan.takes.len(),
                    plan.float32(),
                    plan.seconds()
                ));
                if self.l6max_selected.len() != plan.takes.len() {
                    self.l6max_selected = vec![true; plan.takes.len()];
                }
                for (index, take) in plan.takes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let selected = self
                            .l6max_selected
                            .get_mut(index)
                            .is_some_and(|selected| *selected);
                        let mut chosen = selected;
                        ui.checkbox(&mut chosen, "");
                        if let Some(selected) = self.l6max_selected.get_mut(index) {
                            *selected = chosen;
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {:.1} s · {} ch",
                                take.name, take.seconds, take.channels
                            ))
                            .size(10.0)
                            .color(theme::text_dim()),
                        );
                    });
                }
                if ui.button("Import selected at playhead").clicked() {
                    let takes: Vec<audio::l6max::Take> = plan
                        .takes
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| {
                            self.l6max_selected.get(*index).copied().unwrap_or(true)
                        })
                        .map(|(_, take)| take.clone())
                        .collect();
                    if takes.is_empty() {
                        self.status = Some(String::from("select at least one L6max take"));
                    } else {
                        let selected = audio::l6max::Plan {
                            takes,
                            problems: Vec::new(),
                        };
                        match self.editor.import_l6max(&selected, self.position.max(0)) {
                            Ok(count) => {
                                self.status = Some(format!("imported {count} L6max take(s)"));
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                }
            }
        });
        if let Some(track_id) = self
            .roll
            .selected
            .or_else(|| self.song().tracks.first().map(|track| track.id))
        {
            ui.collapsing("Plugins", |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("Track {track_id}"))
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                    if ui
                        .add_enabled(!self.plugin_scanning, egui::Button::new("Scan installed"))
                        .clicked()
                    {
                        self.start_plugin_scan();
                    }
                });
                if let Some(error) = &self.plugin_error {
                    ui.label(egui::RichText::new(error).color(theme::recording()));
                }
                let slots = self
                    .song()
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .map(|track| track.plugins.clone())
                    .unwrap_or_default();
                for slot in slots {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} · {}",
                            slot.name,
                            match slot.format {
                                project::song::PluginFormat::Vst3 => "VST3",
                                project::song::PluginFormat::AudioUnit => "AU",
                            }
                        ));
                        if ui.small_button("↑").on_hover_text("Move earlier").clicked()
                            && let Err(error) = self.editor.move_plugin(track_id, slot.id, true)
                        {
                            self.status = Some(error);
                        }
                        if ui.small_button("↓").on_hover_text("Move later").clicked()
                            && let Err(error) = self.editor.move_plugin(track_id, slot.id, false)
                        {
                            self.status = Some(error);
                        }
                        if slot.format == project::song::PluginFormat::AudioUnit
                            && ui.small_button("Params").clicked()
                        {
                            self.open_au_parameters(track_id, slot.id);
                        }
                        if slot.format == project::song::PluginFormat::Vst3 {
                            if ui.small_button("Params").clicked() {
                                self.open_vst3_parameters(track_id, slot.id);
                            }
                            if ui.small_button("Capture").clicked() {
                                self.capture_vst3_state(track_id, slot.id);
                            }
                        }
                        if ui
                            .small_button(if slot.bypassed { "Enable" } else { "Bypass" })
                            .clicked()
                            && let Err(error) = self.editor.toggle_plugin(track_id, slot.id)
                        {
                            self.status = Some(error);
                        }
                        if ui.small_button("Remove").clicked()
                            && let Err(error) = self.editor.remove_plugin(track_id, slot.id)
                        {
                            self.status = Some(error);
                        }
                    });
                }
                if let Some(found) = self.plugin_scan.clone() {
                    ui.separator();
                    for plugin in found.iter().take(10) {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} · {}",
                                    plugin.name,
                                    plugin.format.label()
                                ))
                                .size(10.0)
                                .color(theme::text_muted()),
                            );
                            if ui.small_button("Add").clicked() {
                                let format = match plugin.format {
                                    plugins::Format::Vst3 => project::song::PluginFormat::Vst3,
                                    plugins::Format::AudioUnit => {
                                        project::song::PluginFormat::AudioUnit
                                    }
                                };
                                let path = plugin.path.display().to_string();
                                match self.editor.add_plugin(
                                    track_id,
                                    &plugin.name,
                                    format,
                                    &path,
                                    &plugin.identifier,
                                ) {
                                    Ok(id) => {
                                        self.status =
                                            Some(format!("added {} (slot {id})", plugin.name));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                        });
                    }
                    if found.len() > 10 {
                        ui.label(
                            egui::RichText::new(format!("... {} more", found.len() - 10))
                                .size(10.0)
                                .color(theme::text_dim()),
                        );
                    }
                }
            });
        }
        if let Some(track_id) = self
            .roll
            .selected
            .or_else(|| self.song().tracks.first().map(|track| track.id))
        {
            ui.collapsing("SoundFont", |ui| {
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.sf2_path);
                    ui.label("bank");
                    ui.add(egui::DragValue::new(&mut self.sf2_bank).range(0..=16383));
                    ui.label("program");
                    ui.add(egui::DragValue::new(&mut self.sf2_program).range(0..=127));
                });
                let current = self
                    .song()
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .and_then(|track| track.soundfont.as_ref())
                    .map(|font| format!("{}:{} {}", font.bank, font.program, font.path))
                    .unwrap_or_else(|| String::from("built-in timbre"));
                ui.label(
                    egui::RichText::new(format!("Track {track_id}: {current}"))
                        .size(10.0)
                        .color(theme::text_dim()),
                );
                ui.horizontal(|ui| {
                    if ui.button("Use preset").clicked() {
                        let path = self.sf2_path.trim().to_string();
                        let result = if path.is_empty() {
                            Err(String::from("enter an SF2 path first"))
                        } else {
                            std::fs::read(&path)
                                .map_err(|error| format!("{path}: {error}"))
                                .and_then(|bytes| {
                                    audio::soundfont::parse(&bytes)
                                        .map_err(|error| format!("{path}: {error}"))
                                })
                                .and_then(|font| {
                                    let preset = font
                                        .preset(self.sf2_bank, self.sf2_program)
                                        .map(|preset| preset.name.clone())
                                        .unwrap_or_else(|| String::from("unknown preset"));
                                    self.editor
                                        .set_soundfont(
                                            track_id,
                                            Some(project::song::SoundFontRef {
                                                path: path.clone(),
                                                bank: self.sf2_bank,
                                                program: self.sf2_program,
                                            }),
                                        )
                                        .map(|()| {
                                            format!(
                                                "{} -> {}:{} ({preset})",
                                                font.name, self.sf2_bank, self.sf2_program
                                            )
                                        })
                                })
                        };
                        match result {
                            Ok(message) => self.status = Some(message),
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Load current").clicked()
                        && let Some(font) = self
                            .song()
                            .tracks
                            .iter()
                            .find(|track| track.id == track_id)
                            .and_then(|track| track.soundfont.clone())
                    {
                        self.sf2_path = font.path;
                        self.sf2_bank = font.bank;
                        self.sf2_program = font.program;
                    }
                    if ui.button("Clear").clicked() {
                        match self.editor.set_soundfont(track_id, None) {
                            Ok(()) => self.status = Some(String::from("cleared soundfont")),
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
            });
        }
        let strips: Vec<(TrackId, String, u8, u8, bool, bool)> = self
            .song()
            .tracks
            .iter()
            .map(|track| {
                (
                    track.id,
                    track.name.clone(),
                    track.volume,
                    track.pan,
                    track.muted,
                    track.soloed,
                )
            })
            .collect();
        if strips.is_empty() {
            ui.label(egui::RichText::new("No tracks to mix.").color(theme::text_muted()));
            return;
        }
        let buses = self.song().buses.clone();
        for bus in &buses {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("BUS {}", bus.name))
                        .size(11.0)
                        .color(theme::accent()),
                );
                let mut level = i32::from(bus.volume);
                if ui
                    .add(egui::Slider::new(&mut level, 0..=127).text("vol"))
                    .changed()
                {
                    let mut updated = buses.clone();
                    if let Some(target) = updated.iter_mut().find(|entry| entry.id == bus.id) {
                        target.volume = u8::try_from(level).unwrap_or(bus.volume);
                    }
                    let _ = self.editor.set_buses(updated);
                }
                let mut position = i32::from(bus.pan);
                if ui
                    .add(egui::Slider::new(&mut position, 0..=127).text("pan"))
                    .changed()
                {
                    let mut updated = buses.clone();
                    if let Some(target) = updated.iter_mut().find(|entry| entry.id == bus.id) {
                        target.pan = u8::try_from(position).unwrap_or(bus.pan);
                    }
                    let _ = self.editor.set_buses(updated);
                }
            });
        }
        for (id, name, volume, pan, muted, soloed) in strips {
            let meter = if self
                .player
                .as_ref()
                .is_some_and(audio::player::Player::is_playing)
            {
                let position = self.player.as_ref().map_or(0, |player| player.frame() * 2);
                self.stem_levels
                    .iter()
                    .find(|(track, _)| *track == id)
                    .map_or_else(audio::master::Meter::default, |(_, samples)| {
                        audio::master::meter_at(samples, position, audio::SAMPLE_RATE as usize / 20)
                    })
            } else {
                audio::master::Meter::default()
            };
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(if name.is_empty() { "untitled" } else { &name })
                        .size(11.0),
                );
                ui.add(
                    egui::ProgressBar::new(meter.peak.clamp(0.0, 1.0))
                        .desired_width(54.0)
                        .text(format!("{:.0}", meter.peak * 100.0)),
                );
                let mut level = i32::from(volume);
                let response = ui.add(
                    egui::Slider::new(&mut level, 0..=127)
                        .text("vol")
                        .show_value(true),
                );
                if response.changed() {
                    let _ =
                        self.editor
                            .set_track_gain(id, u8::try_from(level).unwrap_or(volume), pan);
                }
                let mut position = i32::from(pan);
                let pan_response = ui.add(
                    egui::Slider::new(&mut position, 0..=127)
                        .text("pan")
                        .show_value(true),
                );
                if pan_response.changed() {
                    let _ = self.editor.set_track_gain(
                        id,
                        volume,
                        u8::try_from(position).unwrap_or(pan),
                    );
                }
                let routing = self
                    .song()
                    .tracks
                    .iter()
                    .find(|track| track.id == id)
                    .map(|track| (track.bus, track.send, track.send_level))
                    .unwrap_or((None, None, 100));
                let mut bus_choice = routing.0;
                egui::ComboBox::from_id_salt(format!("bus-{id}"))
                    .selected_text(match bus_choice {
                        Some(bus) => format!("Bus {bus}"),
                        None => String::from("Master"),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut bus_choice, None, "Master");
                        for bus in &buses {
                            ui.selectable_value(
                                &mut bus_choice,
                                Some(bus.id),
                                format!("{} ({})", bus.name, bus.id),
                            );
                        }
                    });
                if bus_choice != routing.0 {
                    let _ = self
                        .editor
                        .set_track_routing(id, bus_choice, routing.1, routing.2);
                }
                let mut send_choice = routing.1;
                egui::ComboBox::from_id_salt(format!("send-{id}"))
                    .selected_text(match send_choice {
                        Some(bus) => format!("Send {bus}"),
                        None => String::from("No send"),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut send_choice, None, "No send");
                        for bus in &buses {
                            ui.selectable_value(
                                &mut send_choice,
                                Some(bus.id),
                                format!("Send {}", bus.id),
                            );
                        }
                    });
                if send_choice != routing.1 {
                    let _ = self
                        .editor
                        .set_track_routing(id, routing.0, send_choice, routing.2);
                }
                if let Some(_send) = send_choice {
                    let mut level = i32::from(routing.2);
                    if ui
                        .add(egui::Slider::new(&mut level, 0..=127).text("send"))
                        .changed()
                    {
                        let _ = self.editor.set_track_routing(
                            id,
                            routing.0,
                            send_choice,
                            u8::try_from(level).unwrap_or(routing.2),
                        );
                    }
                }
                if ui
                    .small_button("Freeze")
                    .on_hover_text("Render this track to audio and clear its notes")
                    .clicked()
                {
                    let path = std::path::PathBuf::from(format!("freeze-{id}.wav"));
                    match self.editor.freeze_track(id, &path) {
                        Ok(()) => {
                            self.status = Some(format!("froze {id} -> {}", path.display()));
                        }
                        Err(error) => self.status = Some(error),
                    }
                }
                if ui.selectable_label(muted, "M").clicked() {
                    let _ = self.editor.set_track_flags(id, !muted, soloed, false);
                }
                if ui.selectable_label(soloed, "S").clicked() {
                    let _ = self.editor.set_track_flags(id, muted, !soloed, false);
                }
            });
        }
        ui.label(
            egui::RichText::new("Fader and pan write through one undoable command each.")
                .size(10.0)
                .color(theme::text_dim()),
        );
    }

    /// The clip grid: scenes as rows, tracks as columns, launch and record.
    fn grid_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading(format!(
            "Clip grid · {} scene(s) of two bars",
            self.editor.grid(2).rows.len()
        ));
        let grid = self.editor.grid(2);
        let bar = self.song().time_signature.bar_ticks().max(1);
        let track_ids = grid.tracks.clone();
        let track_names: Vec<String> = track_ids
            .iter()
            .map(|track| {
                self.song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == *track)
                    .map_or_else(|| format!("track {track}"), |entry| entry.name.clone())
            })
            .collect();
        enum SlotAction {
            Clear,
            Length(Tick),
            Copy(Tick),
        }
        let mut slot_action: Option<(usize, TrackId, SlotAction)> = None;
        egui::Grid::new("clip-grid")
            .num_columns(2 + track_ids.len())
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                ui.label("");
                ui.label(egui::RichText::new("Scene").strong());
                for name in &track_names {
                    ui.label(egui::RichText::new(name).strong());
                }
                ui.end_row();
                for (scene_index, row) in grid.rows.iter().enumerate() {
                    if ui
                        .small_button("Rename")
                        .on_hover_text("Rename scene")
                        .clicked()
                    {
                        self.rename_scene = Some(scene_index);
                        self.rename_scene_input = row.scene.name.clone();
                    }
                    let playing = self.launcher.current == Some(scene_index);
                    let waiting = self.launcher.waiting == Some(scene_index);
                    let label = if playing {
                        format!("▶ {}", row.scene.name)
                    } else if waiting {
                        format!("… {}", row.scene.name)
                    } else {
                        row.scene.name.clone()
                    };
                    if ui.selectable_label(playing || waiting, label).clicked() {
                        if self.launcher.playing && self.launcher.current == Some(scene_index) {
                            self.launcher.stop();
                            self.status = Some(String::from("stopped"));
                        } else {
                            self.launcher.launch(scene_index);
                            self.status = Some(format!("scene {} queued", scene_index + 1));
                        }
                    }
                    for (column, slot) in row.slots.iter().enumerate() {
                        let Some(track) = track_ids.get(column).copied() else {
                            ui.label("");
                            continue;
                        };
                        let name = slot.clip.map_or_else(
                            || String::from("—"),
                            |clip| {
                                self.song()
                                    .tracks
                                    .iter()
                                    .find(|entry| entry.id == track)
                                    .and_then(|entry| {
                                        entry.clips.iter().find(|entry| entry.id == clip)
                                    })
                                    .map_or_else(
                                        || format!("clip {clip}"),
                                        |entry| entry.name.clone(),
                                    )
                            },
                        );
                        ui.button(name).context_menu(|ui| {
                            if ui.button("Clear").clicked() {
                                slot_action = Some((scene_index, track, SlotAction::Clear));
                                ui.close_menu();
                            }
                            if ui.button("Length: 1 bar").clicked() {
                                slot_action = Some((scene_index, track, SlotAction::Length(bar)));
                                ui.close_menu();
                            }
                            if ui.button("Length: 2 bars").clicked() {
                                slot_action =
                                    Some((scene_index, track, SlotAction::Length(bar * 2)));
                                ui.close_menu();
                            }
                            if ui.button("Copy to playhead").clicked() {
                                slot_action = Some((
                                    scene_index,
                                    track,
                                    SlotAction::Copy(self.position.max(0)),
                                ));
                                ui.close_menu();
                            }
                        });
                    }
                    ui.end_row();
                }
            });
        if let Some((scene, track, action)) = slot_action {
            let result = match action {
                SlotAction::Clear => self.editor.clear_slot(scene, track),
                SlotAction::Length(length) => self.editor.set_slot_length(scene, track, length),
                SlotAction::Copy(target) => self.editor.copy_slot(scene, track, target).map(|_| ()),
            };
            if let Err(error) = result {
                self.status = Some(error);
            }
        }
        ui.horizontal(|ui| {
            if ui
                .button(format!(
                    "Record take ({} scene(s))",
                    self.launcher.log.len()
                ))
                .clicked()
            {
                let log = self.launcher.log.clone();
                match self.editor.record_grid(&log) {
                    Ok(copied) => {
                        self.status = Some(format!("take: {copied} note(s)"));
                        self.launcher.log.clear();
                    }
                    Err(error) => self.status = Some(error),
                }
            }
            if ui.button("Stop").clicked() {
                self.launcher.stop();
            }
        });
        ui.label(
            egui::RichText::new(
                "Click a scene to queue it; right-click a slot to clear, resize or copy it.",
            )
            .size(10.0)
            .color(theme::text_dim()),
        );
        if let Some(index) = self.rename_scene {
            let mut open = true;
            egui::Window::new("Rename scene")
                .collapsible(false)
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rename_scene_input)
                            .desired_width(180.0),
                    );
                    if ui.button("Apply").clicked() {
                        let name = self.rename_scene_input.clone();
                        match self.editor.rename_scene(index, &name) {
                            Ok(()) => self.rename_scene = None,
                            Err(error) => self.status = Some(error),
                        }
                    }
                });
            if !open {
                self.rename_scene = None;
            }
        }
    }

    /// The Beat view: four drum rows of sixteen steps for one bar.
    fn load_beat(&mut self) {
        let bar = self.song().time_signature.bar_ticks().max(1);
        self.load_beat_from(snap(self.position, bar));
    }

    fn load_beat_from(&mut self, start: Tick) {
        let Some(track) = self
            .roll
            .selected
            .or_else(|| self.song().tracks.first().map(|track| track.id))
        else {
            return;
        };
        if let Some((bar, pattern)) = self.editor.beat_pattern(track, start) {
            self.beat_track = Some(track);
            self.beat_start = start;
            self.beat_bar = bar;
            self.beat_pattern = pattern;
        }
    }

    fn beat_dialog(&mut self, context: &Context) {
        if !self.beat_dialog {
            return;
        }
        let mut open = self.beat_dialog;
        let bar = self.song().time_signature.bar_ticks().max(1);
        let mut nav = 0_i32;
        egui::Window::new("Beat")
            .collapsible(false)
            .open(&mut open)
            .show(context, |ui| {
                ui.label(format!(
                    "bar {} · {}, {} bars · click to cycle off/soft/normal/loud",
                    self.beat_start / self.beat_bar.max(1) + 1,
                    self.beat_pattern.bars(),
                    self.beat_bars
                ));
                let mut reload_bars = None;
                ui.horizontal(|ui| {
                    if ui.small_button("◀").on_hover_text("Previous bar").clicked() {
                        nav = -1;
                    }
                    if ui.small_button("▶").on_hover_text("Next bar").clicked() {
                        nav = 1;
                    }
                    for bars in [1_usize, 2, 4] {
                        if ui
                            .selectable_label(self.beat_bars == bars, format!("{bars} bar"))
                            .clicked()
                        {
                            reload_bars = Some(bars);
                        }
                    }
                });
                egui::ScrollArea::horizontal()
                    .id_salt("beat-grid")
                    .show(ui, |ui| {
                        for (row, (_, label)) in arranging::beat::ROWS.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(*label).size(10.0));
                                let steps = self.beat_pattern.grid[row].len();
                                for slot in 0..steps {
                                    let level = self.beat_pattern.grid[row][slot];
                                    let text = if level == 0 {
                                        String::from("·")
                                    } else {
                                        level.to_string()
                                    };
                                    if ui.small_button(text).clicked() {
                                        self.beat_pattern.cycle(row, slot);
                                    }
                                }
                                let copied_row = self.beat_pattern.row(row).map(<[u8]>::to_vec);
                                if ui.small_button("C").on_hover_text("Copy row").clicked() {
                                    self.beat_clipboard = copied_row;
                                }
                                let can_paste = self.beat_clipboard.is_some();
                                if ui
                                    .add_enabled(can_paste, egui::Button::new("P"))
                                    .on_hover_text("Paste row")
                                    .clicked()
                                    && let Some(values) = self.beat_clipboard.clone()
                                {
                                    self.beat_pattern.set_row(row, &values);
                                }
                                if ui.small_button("X").on_hover_text("Clear row").clicked() {
                                    self.beat_pattern.clear_row(row);
                                }
                            });
                        }
                    });
                if let Some(bars) = reload_bars
                    && let (Some(track), Some(_)) = (self.beat_track, Some(self.beat_start))
                    && let Some((bar, pattern)) =
                        self.editor.beat_pattern_bars(track, self.beat_start, bars)
                {
                    self.beat_bar = bar;
                    self.beat_pattern = pattern;
                    self.beat_bars = bars;
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked()
                        && let Some(track) = self.beat_track
                    {
                        let bar = self.beat_bar;
                        let start = self.beat_start;
                        match self
                            .editor
                            .apply_beat(track, start, bar, &self.beat_pattern)
                        {
                            Ok(added) => {
                                self.status = Some(format!("beat: {added} hit(s)"));
                                self.beat_dialog = false;
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui
                        .button("Put in 4 bars")
                        .on_hover_text("Repeat the pattern across four bars as one edit")
                        .clicked()
                        && let Some(track) = self.beat_track
                    {
                        let bar = self.beat_bar;
                        let start = self.beat_start;
                        match self
                            .editor
                            .put_in_bars(track, start, bar, &self.beat_pattern, 4)
                        {
                            Ok(added) => {
                                self.status = Some(format!("beat: {added} hit(s) in 4 bars"));
                                self.beat_dialog = false;
                            }
                            Err(error) => self.status = Some(error),
                        }
                    }
                    if ui.button("Close").clicked() {
                        self.beat_dialog = false;
                    }
                });
            });
        self.beat_dialog = open && self.beat_dialog;
        if nav != 0 {
            self.load_beat_from((self.beat_start + Tick::from(nav) * bar).max(0));
        }
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

    /// The Arrangement Doctor's proposal cards.
    fn audition_proposal(&mut self, proposal: &arranging::Proposal, after: bool) {
        let mut project = self.editor.project().clone();
        if after
            && let Ok(mut session) = app_commands::EditSession::new(project.clone())
            && arranging::apply(&mut session, proposal).is_ok()
        {
            project = session.project().clone();
        }
        let rendered = audio::render(&project.song, audio::SAMPLE_RATE);
        let _ = audio::mix::check_audio_references(&project.song);
        self.audition_player = audio::player::Player::new(rendered).ok();
        self.status = Some(String::from(if after {
            "audition: after"
        } else {
            "audition: before"
        }));
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
            } else {
                player.play();
                self.playing = true;
            }
            return;
        }
        let mut audio = audio::render(self.song(), audio::SAMPLE_RATE);
        let problems = audio::mix::check_audio_references(self.song());
        if self.settings.metronome {
            audio::metronome::mix_accent(
                &mut audio,
                self.song(),
                1.0,
                self.settings.metronome_accent,
            );
        }
        audio::master::apply(
            &mut audio,
            audio::master::Master {
                gain_db: self.settings.master_gain_db,
                limiter: self.settings.master_limiter,
                reverb: self.settings.master_reverb,
                chorus: self.settings.master_chorus,
            },
        );
        if let Some(problem) = problems.first() {
            self.status = Some(format!("audio {}: {}", problem.path, problem.reason));
        }
        if self.loop_enabled
            && let Some((start, end)) = self.loop_region
        {
            let from = self.song().tempo_map.seconds_at(start);
            let to = self.song().tempo_map.seconds_at(end);
            let sliced = audio.slice(from, to);
            self.loop_buffer = Some(sliced.clone());
            match audio::player::Player::new_on(
                sliced,
                self.settings.audio_output.as_deref(),
                (self.settings.audio_buffer_frames > 0)
                    .then_some(self.settings.audio_buffer_frames),
            ) {
                Ok(player) => {
                    self.position = start;
                    self.stem_levels.clear();
                    self.player = Some(player);
                    self.playing = true;
                    if let Some(player) = &self.player {
                        player.play();
                    }
                    return;
                }
                Err(error) => {
                    self.status = Some(error);
                    return;
                }
            }
        }
        let stems = if self.view == View::Mix {
            audio::render_stems(self.song(), audio::SAMPLE_RATE)
                .into_iter()
                .map(|stem| (stem.track, stem.audio.samples))
                .collect()
        } else {
            Vec::new()
        };
        match audio::player::Player::new_on(
            audio,
            self.settings.audio_output.as_deref(),
            (self.settings.audio_buffer_frames > 0).then_some(self.settings.audio_buffer_frames),
        ) {
            Ok(player) => {
                player.play();
                self.stem_levels = stems;
                self.player = Some(player);
                self.playing = true;
            }
            Err(error) => self.load_error = Some(format!("audio: {error}")),
        }
    }

    /// Stops playback and releases the device.
    fn stop_playback(&mut self) {
        if let Some(player) = self.player.take() {
            player.stop();
            player.release();
        }
        self.playing = false;
        self.position = 0;
    }

    fn top_bar(&mut self, context: &Context) {
        egui::TopBottomPanel::top("header")
            .exact_height(theme::HEADER_HEIGHT)
            .frame(egui::Frame::NONE.fill(theme::panel()))
            .show(context, |ui| {
                if self.settings.kids_mode {
                    ui.spacing_mut().button_padding = egui::Vec2::new(10.0, 7.0);
                }
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    egui::Stroke::new(1.0_f32, theme::line()),
                );
                let mut area = rect.shrink2(egui::vec2(16.0, 0.0));
                // Title area: badge, title, subtitle (HeaderBar::paint).
                let title_area = Rect::from_min_size(area.min, egui::vec2(200.0, area.height()));
                area = Rect::from_min_max(egui::pos2(title_area.right(), area.top()), area.max);
                let badge = Rect::from_center_size(
                    egui::pos2(title_area.left() + 18.0, title_area.center().y),
                    egui::vec2(36.0, 36.0),
                );
                ui.painter().image(
                    self.icon.id(),
                    badge,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                let title_rect = Rect::from_min_max(
                    egui::pos2(title_area.left() + 36.0 + 14.0, title_area.top() + 6.0),
                    egui::pos2(title_area.right(), title_area.center().y),
                );
                let name = if self.song().name.is_empty() {
                    "Untitled"
                } else {
                    self.song().name.as_str()
                };
                ui.painter().text(
                    title_rect.left_bottom(),
                    egui::Align2::LEFT_BOTTOM,
                    name,
                    egui::FontId::proportional(14.0),
                    theme::text(),
                );
                let subtitle = format!(
                    "{} BPM \u{b7} {}/{}",
                    self.song().tempo_map.bpm_at(self.position).round() as i64,
                    self.song().time_signature.numerator,
                    self.song().time_signature.denominator
                );
                ui.painter().text(
                    egui::pos2(title_rect.left(), title_rect.bottom() + 2.0),
                    egui::Align2::LEFT_TOP,
                    subtitle,
                    egui::FontId::proportional(12.5),
                    theme::text_muted(),
                );

                // Transport buttons: stop 44, play 54, 8 px gaps.
                let buttons_rect = Rect::from_min_size(area.min, egui::vec2(214.0, area.height()));
                area = Rect::from_min_max(egui::pos2(buttons_rect.right(), area.top()), area.max);
                let mut x = buttons_rect.left();
                let stop = Rect::from_center_size(
                    egui::pos2(x + 19.0, buttons_rect.center().y),
                    egui::vec2(38.0, 38.0),
                );
                x += 44.0;
                let play = Rect::from_center_size(
                    egui::pos2(x + 22.0, buttons_rect.center().y),
                    egui::vec2(44.0, 44.0),
                );
                if transport_button(ui, stop, "\u{25A0}", false) {
                    self.stop_playback();
                }
                if transport_button(
                    ui,
                    play,
                    if self.playing { "\u{2016}" } else { "\u{25B6}" },
                    self.playing,
                ) {
                    self.toggle_playback();
                    context.request_repaint_after(std::time::Duration::from_millis(16));
                }
                x += 50.0;
                let record = Rect::from_center_size(
                    egui::pos2(x + 19.0, buttons_rect.center().y),
                    egui::vec2(38.0, 38.0),
                );
                if transport_button(ui, record, "\u{25CF}", self.recording.is_some()) {
                    self.start_midi_recording();
                }
                x += 44.0;
                let undo = Rect::from_center_size(
                    egui::pos2(x + 16.0, buttons_rect.center().y),
                    egui::vec2(32.0, 32.0),
                );
                if transport_button(ui, undo, "\u{21B6}", false) && !self.editor.undo() {
                    self.status = Some(String::from("nothing to undo"));
                }
                x += 38.0;
                let metronome = Rect::from_center_size(
                    egui::pos2(x + 16.0, buttons_rect.center().y),
                    egui::vec2(32.0, 32.0),
                );
                if metronome_button(ui, metronome, self.settings.metronome) {
                    self.settings.metronome = !self.settings.metronome;
                }
                // LCD, 320 x 52, then the view tabs on the right.
                let lcd_rect = Rect::from_center_size(
                    egui::pos2(area.left() + 16.0 + 175.0, rect.center().y),
                    widgets::LCD_SIZE,
                );
                let mut lcd_ui = ui.new_child(egui::UiBuilder::new().max_rect(lcd_rect));
                widgets::lcd(
                    &mut lcd_ui,
                    self.position,
                    self.song().tempo_map.bpm_at(self.position),
                    self.song().time_signature,
                );
                if self.settings.kids_mode {
                    let settings_rect = Rect::from_center_size(
                        egui::pos2(rect.right() - 16.0 - 40.0, rect.center().y),
                        egui::vec2(80.0, 34.0),
                    );
                    let response = ui.interact(
                        settings_rect,
                        ui.id().with("kids-settings"),
                        egui::Sense::click(),
                    );
                    ui.painter()
                        .rect_filled(settings_rect, 4.0, theme::raised());
                    ui.painter().rect_stroke(
                        settings_rect,
                        4.0,
                        egui::Stroke::new(1.0_f32, theme::line()),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        settings_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Settings",
                        egui::FontId::proportional(11.0),
                        theme::text(),
                    );
                    if response.on_hover_text("Adult settings").clicked() {
                        self.settings_open = true;
                    }
                    return;
                }
                let wide = rect.width() >= 1380.0;
                let ai_rect = Rect::from_center_size(
                    egui::pos2(rect.right() - 16.0 - 38.0, rect.center().y),
                    egui::vec2(76.0, 34.0),
                );
                let (mixer_rect, audio_rect, minus_rect, plus_rect) = if wide {
                    let plus_rect = Rect::from_center_size(
                        egui::pos2(ai_rect.left() - 6.0 - 12.0, rect.center().y),
                        egui::vec2(24.0, 28.0),
                    );
                    let minus_rect = Rect::from_center_size(
                        egui::pos2(plus_rect.left() - 4.0 - 12.0, rect.center().y),
                        egui::vec2(24.0, 28.0),
                    );
                    let audio_rect = Rect::from_center_size(
                        egui::pos2(minus_rect.left() - 8.0 - 40.0, rect.center().y),
                        egui::vec2(80.0, 32.0),
                    );
                    let mixer_rect = Rect::from_center_size(
                        egui::pos2(audio_rect.left() - 8.0 - 27.0, rect.center().y),
                        egui::vec2(54.0, 34.0),
                    );
                    (
                        mixer_rect,
                        Some(audio_rect),
                        Some(minus_rect),
                        Some(plus_rect),
                    )
                } else {
                    let mixer_rect = Rect::from_center_size(
                        egui::pos2(ai_rect.left() - 8.0 - 27.0, rect.center().y),
                        egui::vec2(54.0, 34.0),
                    );
                    (mixer_rect, None, None, None)
                };
                let tabs_rect = Rect::from_center_size(
                    egui::pos2(
                        mixer_rect.left() - 8.0 - widgets::TABS_SIZE.x * 0.5,
                        rect.center().y,
                    ),
                    widgets::TABS_SIZE,
                );
                let mut tabs_ui = ui.new_child(egui::UiBuilder::new().max_rect(tabs_rect));
                let views: Vec<View> = View::allowed(self.settings.kids_mode)
                    .into_iter()
                    .filter(|view| self.settings.kids_mode || *view != View::Mix)
                    .collect();
                let names: Vec<&str> = views
                    .iter()
                    .map(|view| match view {
                        View::Arrange => "Timeline",
                        View::Mix => "Mixer",
                        View::Drums => "Drums",
                        View::Map => "Map",
                        other => crate::strings::text(
                            crate::strings::Language::parse(&self.settings.language),
                            other.key(),
                        ),
                    })
                    .collect();
                let selected = views
                    .iter()
                    .position(|view| *view == self.view)
                    .unwrap_or(0);
                if let Some(index) = widgets::view_tabs(&mut tabs_ui, selected, &names)
                    && let Some(view) = views.get(index)
                {
                    self.view = *view;
                    if *view == View::Drums {
                        self.load_beat();
                    }
                }
                if let (Some(audio_rect), Some(minus_rect), Some(plus_rect)) =
                    (audio_rect, minus_rect, plus_rect)
                {
                    let audio_response = ui.interact(
                        audio_rect,
                        ui.id().with("audio-settings"),
                        egui::Sense::click(),
                    );
                    ui.painter().rect_filled(audio_rect, 4.0, theme::raised());
                    ui.painter().rect_stroke(
                        audio_rect,
                        4.0,
                        egui::Stroke::new(1.0_f32, theme::line()),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().circle_filled(
                        egui::pos2(audio_rect.left() + 10.0, audio_rect.center().y),
                        3.5,
                        egui::Color32::from_rgb(0x4c, 0xaf, 0x50),
                    );
                    ui.painter().text(
                        egui::pos2(audio_rect.left() + 20.0, audio_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "CoreAudio",
                        egui::FontId::proportional(11.0),
                        theme::text(),
                    );
                    if audio_response
                        .on_hover_text("Audio and MIDI settings")
                        .clicked()
                    {
                        self.settings_open = true;
                    }
                    for (rect, glyph, direction) in
                        [(minus_rect, "-", 1.25_f32), (plus_rect, "+", 0.8_f32)]
                    {
                        let response = ui.interact(rect, ui.id().with(glyph), egui::Sense::click());
                        ui.painter().rect_filled(
                            rect,
                            4.0,
                            if response.hovered() {
                                theme::line()
                            } else {
                                theme::raised()
                            },
                        );
                        ui.painter().rect_stroke(
                            rect,
                            4.0,
                            egui::Stroke::new(1.0_f32, theme::line()),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            glyph,
                            egui::FontId::proportional(14.0),
                            theme::text(),
                        );
                        if response
                            .on_hover_text(if direction < 1.0 {
                                "Zoom in"
                            } else {
                                "Zoom out"
                            })
                            .clicked()
                        {
                            if self.view == View::Arrange && self.timeline_mode {
                                self.timeline_zoom =
                                    (self.timeline_zoom * direction).clamp(0.5, 3.0);
                            } else {
                                self.roll.ticks_per_point =
                                    (self.roll.ticks_per_point * direction).clamp(2.0, 64.0);
                            }
                        }
                    }
                }
                let mixer_response = ui.interact(
                    mixer_rect,
                    ui.id().with("mixer-toggle"),
                    egui::Sense::click(),
                );
                ui.painter().rect_filled(
                    mixer_rect,
                    4.0,
                    if self.mixer_visible {
                        theme::accent().gamma_multiply(0.18)
                    } else {
                        theme::raised()
                    },
                );
                ui.painter().rect_stroke(
                    mixer_rect,
                    4.0,
                    egui::Stroke::new(1.0_f32, theme::line()),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    mixer_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Mixer",
                    egui::FontId::proportional(11.0),
                    theme::text(),
                );
                if mixer_response
                    .on_hover_text("Show or hide the mixer")
                    .clicked()
                {
                    self.mixer_visible = !self.mixer_visible;
                }
                let ai_response =
                    ui.interact(ai_rect, ui.id().with("ask-ai"), egui::Sense::click());
                ui.painter().rect_filled(
                    ai_rect,
                    16.0,
                    theme::accent().gamma_multiply(if ai_response.hovered() { 1.0 } else { 0.9 }),
                );
                ui.painter().text(
                    ai_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Ask AI",
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
                if ai_response.on_hover_text("Open the assistant").clicked() {
                    self.assistant_open = true;
                }
                let title = if self.song().name.is_empty() {
                    String::from("Lazy Arranger Midi DAW")
                } else {
                    format!("{} — Lazy Arranger Midi DAW", self.song().name)
                };
                context.send_viewport_cmd(egui::ViewportCommand::Title(title));
            });
    }

    fn sidebar(&mut self, context: &Context) {
        if self.settings.kids_mode && self.view == View::Perform {
            return;
        }
        egui::SidePanel::left("tracks")
            .resizable(true)
            .default_width(theme::SIDEBAR_WIDTH)
            .width_range(180.0..=420.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Tracks").size(13.0).strong());
                    ui.label(
                        egui::RichText::new(self.song().tracks.len().to_string())
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("+").on_hover_text("Add a track").clicked() {
                            match self.editor.add_empty_track() {
                                Ok(_) => self.status = Some(String::from("track added")),
                                Err(error) => self.status = Some(error),
                            }
                        }
                    });
                });
                let chord_track = self
                    .roll
                    .selected
                    .and_then(|id| self.song().tracks.iter().find(|track| track.id == id))
                    .or_else(|| {
                        self.song()
                            .tracks
                            .iter()
                            .max_by_key(|track| track.notes.len())
                    })
                    .map(|track| track.name.clone())
                    .unwrap_or_else(|| String::from("no track"));
                ui.label(
                    egui::RichText::new(format!("Chords on {chord_track}"))
                        .size(9.0)
                        .color(theme::text_dim()),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.loop_enabled, "Loop")
                        .on_hover_text("Repeat the loop region")
                        .clicked()
                    {
                        self.loop_enabled = !self.loop_enabled;
                        if self.loop_enabled && self.loop_region.is_none() {
                            let bar = self.song().time_signature.bar_ticks().max(1);
                            self.loop_region =
                                Some(crate::transport::bar_region(self.position, bar));
                        }
                    }
                    if ui
                        .small_button("From bar")
                        .on_hover_text("Set the loop to the current bar")
                        .clicked()
                    {
                        let bar = self.song().time_signature.bar_ticks().max(1);
                        self.loop_region = Some(crate::transport::bar_region(self.position, bar));
                        self.loop_enabled = true;
                        self.loop_buffer = None;
                    }
                });
                ui.add_space(4.0);
                let rows: Vec<Row> = self
                    .song()
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(index, track)| Row {
                        index,
                        id: track.id,
                        name: track.name.clone(),
                        notes: track.notes.len(),
                        program: track.program,
                        audio: track.audio.is_some(),
                        muted: track.muted,
                        soloed: track.soloed,
                        armed: track.armed,
                    })
                    .collect();
                egui::ScrollArea::vertical()
                    .id_salt("track-list")
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - 110.0).max(120.0))
                    .show(ui, |ui| {
                        for row in rows {
                            self.track_row(ui, &row);
                        }
                        if self.song().tracks.is_empty() {
                            ui.label(egui::RichText::new("no tracks").color(theme::text_dim()));
                        }
                    });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if self.settings.kids_mode {
                        ui.horizontal(|ui| {
                            let size = egui::Vec2::new(74.0, 30.0);
                            if ui.add_sized(size, egui::Button::new("Undo")).clicked() {
                                let _ = self.editor.undo();
                            }
                            if ui.add_sized(size, egui::Button::new("Redo")).clicked() {
                                let _ = self.editor.redo();
                            }
                            let play = if self.playing { "Pause" } else { "Play" };
                            if ui.add_sized(size, egui::Button::new(play)).clicked() {
                                self.toggle_playback();
                            }
                            if ui.add_sized(size, egui::Button::new("Stop")).clicked() {
                                self.stop_playback();
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Settings").clicked() {
                                self.settings_open = true;
                            }
                            if ui.button("Lessons").clicked() {
                                self.lessons_open = true;
                            }
                            if ui.button("Theory").clicked() {
                                self.theory_open = true;
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            let label = if self.recording.is_some() {
                                "Recording…"
                            } else {
                                "Record"
                            };
                            if ui.small_button(label).clicked() && self.recording.is_none() {
                                self.start_midi_recording();
                            }
                            ui.add(
                                egui::DragValue::new(&mut self.record_seconds)
                                    .range(1.0..=60.0)
                                    .suffix(" s"),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.settings.record_count_in)
                                    .range(0..=2)
                                    .suffix(" bar"),
                            )
                            .on_hover_text("Count-in before MIDI or audio recording");
                            let audio_label = if self.audio_recording.is_some() {
                                "Recording audio…"
                            } else {
                                "Rec audio"
                            };
                            if ui
                                .add_enabled(
                                    self.audio_recording.is_none(),
                                    egui::Button::new(audio_label),
                                )
                                .clicked()
                            {
                                let path = std::env::temp_dir().join(format!(
                                    "lazy-arranger-input-{}.wav",
                                    std::process::id()
                                ));
                                let seconds = self.audio_record_seconds;
                                let start = self.position.max(0);
                                let input = self.settings.audio_input.clone();
                                let bpm = self.song().tempo_map.bpm_at(0).max(1.0);
                                let count_in = self.settings.record_count_in;
                                let delay = f64::from(count_in) * 4.0 * 60.0 / bpm;
                                let (sender, receiver) = std::sync::mpsc::channel();
                                std::thread::spawn(move || {
                                    if delay > 0.0 {
                                        if let Ok(player) = audio::player::Player::new(
                                            count_in_audio(bpm, count_in),
                                        ) {
                                            player.play();
                                            std::thread::sleep(std::time::Duration::from_secs_f64(
                                                delay,
                                            ));
                                            player.release();
                                        } else {
                                            std::thread::sleep(std::time::Duration::from_secs_f64(
                                                delay,
                                            ));
                                        }
                                    }
                                    let result = audio::record::record_seconds_from(
                                        input.as_deref(),
                                        seconds,
                                    )
                                    .and_then(|rendered| {
                                        std::fs::write(&path, rendered.to_wav())
                                            .map_err(|error| format!("{}: {error}", path.display()))
                                    })
                                    .map(|()| path);
                                    let _ = sender.send(result);
                                });
                                self.audio_recording = Some(receiver);
                                self.audio_record_start = start;
                                self.audio_record_target = self
                                    .song()
                                    .tracks
                                    .iter()
                                    .find(|track| track.armed)
                                    .map(|track| track.id);
                                self.status = Some(if count_in == 0 {
                                    format!("recording {:.1} s of audio", seconds)
                                } else {
                                    format!("count-in {count_in} bar(s), then {seconds:.1} s audio")
                                });
                            }
                            ui.add(
                                egui::DragValue::new(&mut self.audio_record_seconds)
                                    .range(1.0..=60.0)
                                    .suffix(" s"),
                            );
                        });
                        ui.menu_button("Windows", |ui| {
                            if ui.button("Inspector").clicked() {
                                self.inspector_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Library").clicked() {
                                self.library_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Pads").clicked() {
                                self.pads_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Assistant").clicked() {
                                self.assistant_open = true;
                                ui.close_menu();
                            }
                            if ui.button("MIDI map").clicked() {
                                self.midi_map_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Controllers").clicked() {
                                self.controller_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Lyrics").clicked() {
                                self.lyrics_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Markers").clicked() {
                                self.markers_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Theory").clicked() {
                                self.theory_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Lessons").clicked() {
                                self.lessons_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Songs & tutorial").clicked() {
                                self.songs_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Help").clicked() {
                                self.help_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Plugins").clicked() {
                                self.plugins_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Synth patch").clicked() {
                                self.patch_open = true;
                                self.patch_draft = None;
                                ui.close_menu();
                            }
                            if ui.button("Style from clips").clicked() {
                                self.style_clips_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Style from audio").clicked() {
                                self.audio_style_open = true;
                                ui.close_menu();
                            }
                            if ui.button("MIDI monitor").clicked() {
                                self.midi_monitor_open = true;
                                ui.close_menu();
                            }
                            if ui.button("XG part mixer").clicked() {
                                self.xg_open = true;
                                ui.close_menu();
                            }
                            if ui.button("L6max mixer").clicked() {
                                self.l6max_mixer_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Generate part").clicked() {
                                self.generate_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Style from link").clicked() {
                                self.style_link_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Chords from audio").clicked() {
                                self.audio_chords_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Style file player").clicked() {
                                self.style_file_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Master effects").clicked() {
                                self.master_fx_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Bus effects").clicked() {
                                self.bus_fx_open = true;
                                ui.close_menu();
                            }
                            if ui.button("MIDI out").clicked() {
                                self.midi_out_open = true;
                                ui.close_menu();
                            }
                            if ui.button("PSR-S970 remote").clicked() {
                                self.psr_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Shortcuts").clicked() {
                                self.shortcuts_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Device editor").clicked() {
                                self.device_editor_open = true;
                                ui.close_menu();
                            }
                            if ui.button("Groove library").clicked() {
                                self.groove_open = true;
                                ui.close_menu();
                            }
                            if ui.button("About Lazy Arranger Midi DAW").clicked() {
                                self.about_open = true;
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Import L6max").clicked() {
                                self.file_path_input.clear();
                                self.file_mode = Some(FileMode::ImportL6max);
                                ui.close_menu();
                            }
                            if ui.button("Import audio").clicked() {
                                self.file_path_input.clear();
                                self.file_mode = Some(FileMode::ImportAudio);
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Export MIDI").clicked() {
                                self.file_path_input = self
                                    .project_path
                                    .as_ref()
                                    .map(|path| path.with_extension("mid").display().to_string())
                                    .unwrap_or_else(|| String::from("project.mid"));
                                self.file_mode = Some(FileMode::ExportMidi);
                                ui.close_menu();
                            }
                            if ui.button("Export stems").clicked() {
                                self.file_path_input = self
                                    .project_path
                                    .as_ref()
                                    .and_then(|path| path.parent())
                                    .map(|parent| parent.join("stems").display().to_string())
                                    .unwrap_or_else(|| String::from("stems"));
                                self.file_mode = Some(FileMode::ExportStems);
                                ui.close_menu();
                            }
                            if ui.button("Export WAV").clicked() {
                                self.file_path_input = self
                                    .project_path
                                    .as_ref()
                                    .map(|path| path.with_extension("wav").display().to_string())
                                    .unwrap_or_else(|| String::from("project.wav"));
                                self.file_mode = Some(FileMode::ExportWav);
                                ui.close_menu();
                            }
                            if ui.button("Export XML").clicked() {
                                self.file_path_input = self
                                    .project_path
                                    .as_ref()
                                    .map(|path| {
                                        path.with_extension("musicxml").display().to_string()
                                    })
                                    .unwrap_or_else(|| String::from("project.musicxml"));
                                self.file_mode = Some(FileMode::ExportMusicXml);
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Settings").clicked() {
                                self.settings_open = true;
                                ui.close_menu();
                            }
                        });
                    }
                    if !self.recent.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("RECENT")
                                    .size(9.0)
                                    .color(theme::text_dim()),
                            );
                            if ui.small_button("Clear").clicked() {
                                match project::recent::clear(&self.recent_file) {
                                    Ok(()) => {
                                        self.recent.clear();
                                        self.status = Some(String::from("recent list cleared"));
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                        });
                        let entries = self.recent.clone();
                        for entry in entries {
                            let label = entry
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("project")
                                .to_string();
                            if ui
                                .button(label)
                                .on_hover_text(entry.display().to_string())
                                .clicked()
                            {
                                self.open(&entry);
                            }
                        }
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button("New")
                            .on_hover_text("Start an empty project")
                            .clicked()
                        {
                            self.replace_project(
                                ProjectFile::new(project::song::Song::new("Untitled", 120.0)),
                                "new project",
                            );
                        }
                        if ui.button("Open project…").clicked()
                            && let Some(path) = rfd_pick()
                        {
                            self.open(&path);
                        }
                        if ui.button("Save").clicked() {
                            if let Some(path) = self.project_path.clone() {
                                self.save_to(path);
                            } else {
                                self.begin_save_as();
                            }
                        }
                        if ui.button("Save as…").clicked() {
                            self.begin_save_as();
                        }
                    });
                });
            });
        if let Some(error) = self.load_error.clone() {
            egui::Window::new("Could not open project")
                .collapsible(false)
                .show(context, |ui| {
                    ui.label(error);
                    if ui.button("Dismiss").clicked() {
                        self.load_error = None;
                    }
                });
        }
    }

    fn track_row(&mut self, ui: &mut egui::Ui, row: &Row) {
        let selected = self.roll.selected == Some(row.id);
        let colour = theme::track_colour(row.index);
        let mut flags_changed = false;
        let response = egui::Frame::new()
            .fill(if selected {
                theme::raised()
            } else {
                egui::Color32::TRANSPARENT
            })
            .inner_margin(egui::Margin::symmetric(2, 2))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(6.0, 60.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 0.0, colour);
                    let (icon_rect, _) =
                        ui.allocate_exact_size(egui::vec2(28.0, 34.0), egui::Sense::hover());
                    draw_track_icon(ui, icon_rect, &row.name, row.program, row.audio, colour);
                    ui.allocate_ui_with_layout(
                        egui::vec2((ui.available_width() - 62.0).max(40.0), 60.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            let name = if row.name.is_empty() {
                                "untitled"
                            } else {
                                row.name.as_str()
                            };
                            ui.label(egui::RichText::new(name).size(12.0).color(if selected {
                                theme::text()
                            } else {
                                theme::text_muted()
                            }));
                            let detail = if row.name.to_ascii_lowercase().contains("drum") {
                                String::from("Electronic Kit")
                            } else if let Some(program) = row.program {
                                format!("SoundFont · {}", midi_devices::programs::name(program))
                            } else {
                                format!("SoundFont · {} notes", row.notes)
                            };
                            ui.label(
                                egui::RichText::new(detail)
                                    .size(9.0)
                                    .color(theme::text_dim()),
                            );
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .selectable_label(row.armed, "R")
                            .on_hover_text("Record-arm this track")
                            .clicked()
                        {
                            flags_changed = true;
                            let _ = self
                                .editor
                                .set_track_flags(row.id, row.muted, row.soloed, !row.armed);
                        }
                        if ui
                            .selectable_label(row.soloed, "S")
                            .on_hover_text("Solo")
                            .clicked()
                        {
                            flags_changed = true;
                            let _ =
                                self.editor
                                    .set_track_flags(row.id, row.muted, !row.soloed, false);
                        }
                        if ui
                            .selectable_label(row.muted, "M")
                            .on_hover_text("Mute")
                            .clicked()
                        {
                            flags_changed = true;
                            let _ = self
                                .editor
                                .set_track_flags(row.id, !row.muted, row.soloed, false);
                        }
                    });
                });
            })
            .response;
        if !flags_changed && response.interact(egui::Sense::click()).clicked() {
            self.roll.selected = if selected { None } else { Some(row.id) };
        }
    }

    fn status_bar(&mut self, context: &Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(theme::STATUS_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(theme::panel())
                    .inner_margin(egui::Margin::symmetric(12, 4)),
            )
            .show(context, |ui| {
                ui.horizontal(|ui| {
                    let notes: usize = self
                        .song()
                        .tracks
                        .iter()
                        .map(|track| track.notes.len())
                        .sum();
                    let issues = self.editor.project().validate();
                    let errors = issues
                        .iter()
                        .filter(|issue| issue.severity == project::Severity::Error)
                        .count();
                    let label = if errors == 0 {
                        let midi = self.midi_input.as_ref().map_or_else(
                            || String::from("No MIDI inputs"),
                            |inputs| format!("{} MIDI input(s)", inputs.count()),
                        );
                        format!(
                            "schema v{}  ·  {} track(s)  ·  {notes} note(s)  ·  {}  ·  {midi}",
                            self.editor.project().version,
                            self.song().tracks.len(),
                            audio_engine_label()
                        )
                    } else {
                        format!("{errors} validation error(s)")
                    };
                    ui.label(egui::RichText::new(label).size(10.0).color(if errors == 0 {
                        theme::text_muted()
                    } else {
                        theme::recording()
                    }));
                    if let Some(status) = &self.status {
                        ui.label(
                            egui::RichText::new(status)
                                .size(10.0)
                                .color(theme::text_muted()),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(path) = &self.project_path {
                            ui.label(
                                egui::RichText::new(path.display().to_string())
                                    .size(10.0)
                                    .color(theme::text_dim()),
                            );
                        }
                        if self.view == View::Arrange
                            && self.timeline_mode
                            && ui.small_button("Piano roll").clicked()
                        {
                            self.timeline_mode = false;
                        }
                        ui.label(
                            egui::RichText::new("Space Play · C Metronome · Home Start")
                                .size(9.0)
                                .color(theme::text_dim()),
                        );
                    });
                });
            });
    }

    fn open(&mut self, path: &PathBuf) {
        let kind = project_kind(path);
        let imported = kind != ProjectKind::Json;
        match load_project(path) {
            Ok(project) => {
                match Editor::new(project) {
                    Ok(editor) => self.editor = editor,
                    Err(error) => {
                        self.load_error = Some(error.to_string());
                        return;
                    }
                }
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
        self.sync_monitor();
        let _ = frame;
        // ADR-011: paint the first themed frame before the window becomes visible.
        let mut style = (*context.style()).clone();
        let delay_changed = (style.interaction.tooltip_delay - 0.6).abs() > 0.01;
        style.interaction.tooltip_delay = 0.6;
        if style.visuals.panel_fill != theme::panel() || delay_changed {
            style.visuals = theme::visuals();
            context.set_style(style);
        }
        self.handle_keys(context);
        if (self.style_queued_section.is_some() || self.style_queued_chord.is_some())
            && let Some(player) = &self.style_preview
            && (!player.is_playing()
                || player.frame() as f64 / f64::from(audio::SAMPLE_RATE) >= self.style_queue_at)
        {
            if let Some(index) = self.style_queued_section.take() {
                self.style_section = index;
            }
            if let Some(chord) = self.style_queued_chord.take() {
                self.style_chord = chord;
            }
            self.audition_style_section();
        }
        if let Some(return_index) = self.style_fill_return
            && self.style_queued_section.is_none()
            && let Some(player) = &self.style_preview
            && !player.is_playing()
        {
            self.style_fill_return = None;
            self.style_section = return_index;
            self.audition_style_section();
        }
        if self.style_file_loop
            && !self.style_ending
            && self.style_fill_return.is_none()
            && self.style_queued_section.is_none()
            && self.style_queued_chord.is_none()
            && let Some(player) = &self.style_preview
            && !player.is_playing()
        {
            player.play();
        }
        self.poll_midi();
        self.poll_ai();
        if let Some(receiver) = &self.style_link_pending {
            match receiver.try_recv() {
                Ok(Ok(path)) => {
                    self.audio_style_path = path.display().to_string();
                    self.audio_style_open = true;
                    self.status = Some(String::from("audio downloaded; style build is ready"));
                    self.style_link_pending = None;
                }
                Ok(Err(error)) => {
                    self.status = Some(format!("link: {error}"));
                    self.style_link_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.style_link_pending = None;
                }
            }
        }
        if let Some(receiver) = &self.audio_chords_pending {
            match receiver.try_recv() {
                Ok(Ok(chords)) => {
                    self.status = Some(format!("recognised {} chord(s)", chords.len()));
                    self.audio_chords = chords;
                    self.audio_chords_pending = None;
                }
                Ok(Err(error)) => {
                    self.status = Some(error);
                    self.audio_chords_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.audio_chords_pending = None;
                }
            }
        }
        if let Some(receiver) = &self.dictation_pending {
            match receiver.try_recv() {
                Ok(Ok(text)) => {
                    self.assistant_input = text.clone();
                    self.status = Some(String::from("dictation ready"));
                    self.dictation_pending = None;
                    self.assistant_open = true;
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
        self.refresh_waveform();
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
            }
        }
        if self.view == View::Drums
            && (self.beat_track.is_none()
                || (self.roll.selected.is_some() && self.roll.selected != self.beat_track))
        {
            self.load_beat();
        }
        self.run_autosave();
        self.recovery_dialog(context);
        self.chord_dialog(context);
        self.reharmonize_dialog(context);
        self.doctor_dialog(context);
        self.form_dialog(context);
        self.beat_dialog(context);
        self.advance(context);
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
        self.audio_style_window(context);
        self.style_rules_window(context);
        self.master_fx_window(context);
        self.bus_fx_window(context);
        self.psr_window(context);
        self.xg_window(context);
        self.midi_out_window(context);
        self.midi_monitor_window(context);
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
        let kids = self.settings.kids_mode;
        if kids {
            self.lesson
                .sync(self.song().chords.iter().map(|event| event.chord).collect());
        }
        let lesson = self.lesson.clone();
        let pads = music_core::roman::pads(self.key);
        let waveform = self.waveform.clone();
        let panel = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::timeline()))
            .show(context, |ui| {
                let view = self.view;
                let position = self.position;
                let mut clicked = None;
                let mut automation_edit: Option<(TrackId, project::song::Automation)> = None;
                let mut audio_trim: Option<(TrackId, Tick)> = None;
                let mut audio_length: Option<(TrackId, Tick)> = None;
                let mut audio_fades: Option<(TrackId, Tick, Tick)> = None;
                let mut audio_stretch: Option<(TrackId, bool)> = None;
                let mut audio_normalize: Option<TrackId> = None;
                let mut score_status: Option<String> = None;
                let entry_enabled_flag = self.score_entry;
                let mut perform_chord: Option<music_core::Chord> = None;
                let mut lesson_next = false;
                let mut lesson_reset = false;
                let mut kids_load = false;
                let mut scroll_edit: Option<crate::piano_roll::RollEdit> = None;
                let mut velocity_edit: Option<(TrackId, project::NoteId, u8)> = None;
                let mut preview_note: Option<u8> = None;
                let mut chord_click: Option<Tick> = None;
                let mut lyric_click: Option<Tick> = None;
                let mut seek: Option<Tick> = None;
                let timeline_mode = self.timeline_mode;
                let timeline_zoom = self.timeline_zoom;
                let Self { editor, roll, .. } = self;
                let project = editor.project();
                match view {
                    View::Arrange => {
                        if !timeline_mode {
                            ui.horizontal(|ui| {
                                if ui
                                    .button("Timeline")
                                    .on_hover_text("Back to the coloured track lanes")
                                    .clicked()
                                {
                                    self.timeline_mode = true;
                                }
                                ui.label(
                                    egui::RichText::new("Piano roll")
                                        .size(11.0)
                                        .color(theme::text_muted()),
                                );
                                ui.checkbox(&mut roll.snap, "Snap 1/16").on_hover_text(
                                    "Snap note drags and insertions to a sixteenth",
                                );
                            });
                        }
                        if let Some((track_id, path, peaks)) = &waveform
                            && roll.selected == Some(*track_id)
                        {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(
                                        std::path::Path::new(path)
                                            .file_name()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or("audio"),
                                    )
                                    .size(10.0)
                                    .color(theme::text_muted()),
                                );
                                if ui.button("Trim 1 beat").clicked() {
                                    let bar = project.song.time_signature.bar_ticks().max(1);
                                    audio_trim = Some((*track_id, bar / 4));
                                }
                                if ui
                                    .button("Start at playhead")
                                    .on_hover_text("Trim the front to the playhead")
                                    .clicked()
                                    && let Some(reference) = project
                                        .song
                                        .tracks
                                        .iter()
                                        .find(|track| track.id == *track_id)
                                        .and_then(|track| track.audio.as_ref())
                                {
                                    audio_trim = Some((
                                        *track_id,
                                        (position - reference.start).max(1),
                                    ));
                                }
                                if ui
                                    .button("End at playhead")
                                    .on_hover_text("Trim the audio end to the playhead")
                                    .clicked()
                                    && let Some(reference) = project
                                        .song
                                        .tracks
                                        .iter()
                                        .find(|track| track.id == *track_id)
                                        .and_then(|track| track.audio.as_ref())
                                {
                                    audio_length =
                                        Some((*track_id, (position - reference.start).max(1)));
                                }
                                let bar = project.song.time_signature.bar_ticks().max(1);
                                let current = project
                                    .song
                                    .tracks
                                    .iter()
                                    .find(|track| track.id == *track_id)
                                    .and_then(|track| track.audio.as_ref())
                                    .map_or((0, 0), |reference| {
                                        (reference.fade_in, reference.fade_out)
                                    });
                                if ui.small_button("Fade in bar").clicked() {
                                    audio_fades = Some((*track_id, bar, current.1));
                                }
                                if ui.small_button("Fade out bar").clicked() {
                                    audio_fades = Some((*track_id, current.0, bar));
                                }
                                if ui.small_button("No fades").clicked() {
                                    audio_fades = Some((*track_id, 0, 0));
                                }
                                let mut stretch = project
                                    .song
                                    .tracks
                                    .iter()
                                    .find(|track| track.id == *track_id)
                                    .and_then(|track| track.audio.as_ref())
                                    .is_some_and(|reference| reference.stretch);
                                if ui
                                    .checkbox(&mut stretch, "Stretch")
                                    .on_hover_text(
                                        "Resample the source to the clip length (pitch shifts)",
                                    )
                                    .changed()
                                {
                                    audio_stretch = Some((*track_id, stretch));
                                }
                                if ui
                                    .small_button("Normalize")
                                    .on_hover_text("Write a normalized copy and switch to it")
                                    .clicked()
                                {
                                    audio_normalize = Some(*track_id);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("waveform")
                                        .size(10.0)
                                        .color(theme::text_dim()),
                                );
                                ui.add(
                                    egui::Slider::new(&mut self.waveform_zoom, 1.0..=8.0)
                                        .text("zoom")
                                        .fixed_decimals(1),
                                );
                                ui.add(
                                    egui::Slider::new(&mut self.waveform_position, 0.0..=1.0)
                                        .text("position")
                                        .fixed_decimals(2),
                                );
                            });
                            Self::draw_waveform(
                                ui,
                                peaks,
                                self.waveform_zoom,
                                self.waveform_position,
                            );
                        }
                        let selected_clips = self.selected_clips.clone();
                        let (outcome, timeline_track, clip_edit) = if timeline_mode {
                            crate::timeline::show(
                                ui,
                                &project.song,
                                position,
                                roll.selected,
                                timeline_zoom,
                                self.playing,
                                &self.timeline_waveforms,
                            )
                        } else {
                            (
                                piano_roll::show(
                                    ui,
                                    &project.song,
                                    roll,
                                    position,
                                    &selected_clips,
                                ),
                                None,
                                None,
                            )
                        };
                        if let Some(track) = timeline_track {
                            roll.selected = Some(track);
                        }
                        if clip_edit.is_some() {
                            self.pending_clip_edit = clip_edit;
                        }
                        if let Some((first, last)) = roll.band.take()
                            && let Some(track_id) = roll.selected
                            && let Some(track) = project
                                .song
                                .tracks
                                .iter()
                                .find(|track| track.id == track_id)
                        {
                            let clips = piano_roll::clips_in_band(track, first, last);
                            self.selected_clips =
                                clips.iter().map(|clip| (track_id, *clip)).collect();
                            self.selected_clip = clips.last().map(|clip| (track_id, *clip));
                        }
                        clicked = outcome.click;
                        if let Some(edit) = outcome.edit {
                            scroll_edit = Some(edit);
                        }
                        if let Some(velocity) = outcome.velocity {
                            velocity_edit = Some(velocity);
                        }
                        if let Some(preview) = outcome.preview {
                            preview_note = Some(preview);
                        }
                        if let Some(tick) = outcome.chord {
                            chord_click = Some(tick);
                        }
                        if let Some(tick) = outcome.lyric {
                            lyric_click = Some(tick);
                        }
                        if let Some(tick) = outcome.seek {
                            seek = Some(tick);
                        }
                        if let Some(track_id) = roll.selected
                            && let Some(track) = project
                                .song
                                .tracks
                                .iter()
                                .find(|track| track.id == track_id)
                        {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Automation")
                                        .size(10.0)
                                        .color(theme::text_dim()),
                                );
                                let mut target = self.automation_target;
                                egui::ComboBox::from_id_salt("automation-target")
                                    .selected_text(format!("{target:?}"))
                                    .show_ui(ui, |ui| {
                                        use project::song::AutomationTarget;
                                        for option in [
                                            AutomationTarget::Volume,
                                            AutomationTarget::Pan,
                                            AutomationTarget::Send,
                                            AutomationTarget::Brightness,
                                        ] {
                                            ui.selectable_value(
                                                &mut target,
                                                option,
                                                format!("{option:?}"),
                                            );
                                        }
                                    });
                                self.automation_target = target;
                                ui.label("slot");
                                ui.add(
                                    egui::DragValue::new(&mut self.automation_plugin_slot)
                                        .range(0..=9999),
                                );
                                ui.label("param");
                                ui.add(
                                    egui::DragValue::new(&mut self.automation_plugin_param)
                                        .range(0..=9999),
                                );
                                if ui.button("Plugin").clicked() {
                                    self.automation_target =
                                        project::song::AutomationTarget::PluginParam {
                                            slot: self.automation_plugin_slot,
                                            parameter: self.automation_plugin_param,
                                        };
                                }
                            });
                            let target = self.automation_target;
                            let lane = track
                                .automation
                                .iter()
                                .find(|lane| lane.target == target)
                                .cloned()
                                .unwrap_or(project::song::Automation {
                                    target,
                                    points: Vec::new(),
                                });
                            if let Some(edited) = crate::automation_view::show(
                                ui,
                                &lane,
                                project.song.end_tick().max(1),
                            ) {
                                automation_edit = Some((track_id, edited));
                            }
                        }
                    }
                    View::Drums => {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Beat").size(16.0).strong());
                            if ui
                                .button("Open floating editor")
                                .on_hover_text("Open the same grid in a window")
                                .clicked()
                            {
                                self.beat_dialog = true;
                            }
                            if ui.small_button("◀").on_hover_text("Previous bar").clicked() {
                                self.pending_beat_start =
                                    Some((self.beat_start - self.beat_bar).max(0));
                            }
                            if ui.small_button("▶").on_hover_text("Next bar").clicked() {
                                self.pending_beat_start =
                                    Some((self.beat_start + self.beat_bar).max(0));
                            }
                            if let Some(track) = self.beat_track {
                                let name = project
                                    .song
                                    .tracks
                                    .iter()
                                    .find(|entry| entry.id == track)
                                    .map_or_else(
                                        || format!("track {track}"),
                                        |entry| entry.name.clone(),
                                    );
                                ui.label(
                                    egui::RichText::new(name)
                                        .size(10.0)
                                        .color(theme::text_dim()),
                                );
                            }
                        });
                        if self.beat_track.is_none() {
                            ui.label("Select a track to edit its beat.");
                        } else {
                            let mut reload = None;
                            ui.horizontal(|ui| {
                                for bars in [1_usize, 2, 4] {
                                    if ui
                                        .selectable_label(
                                            self.beat_bars == bars,
                                            format!("{bars} bar"),
                                        )
                                        .clicked()
                                    {
                                        reload = Some(bars);
                                    }
                                }
                            });
                            for (row, (_, label)) in arranging::beat::ROWS.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(*label)
                                            .size(11.0)
                                            .color(theme::text_dim()),
                                    );
                                    let steps = self.beat_pattern.grid[row].len();
                                    for slot in 0..steps {
                                        let level = self.beat_pattern.grid[row][slot];
                                        let text = if level == 0 {
                                            String::from("·")
                                        } else {
                                            level.to_string()
                                        };
                                        if ui
                                            .add_sized([24.0, 24.0], egui::Button::new(text))
                                            .clicked()
                                        {
                                            self.beat_pattern.cycle(row, slot);
                                        }
                                    }
                                });
                            }
                            if let Some(bars) = reload
                                && let Some(track) = self.beat_track
                                && let Some((bar, pattern)) = self.editor.beat_pattern_bars(
                                    track,
                                    self.beat_start,
                                    bars,
                                )
                            {
                                self.beat_bar = bar;
                                self.beat_pattern = pattern;
                                self.beat_bars = bars;
                            }
                            if ui.button("Apply beat").clicked()
                                && let Some(track) = self.beat_track
                            {
                                match self.editor.apply_beat(
                                    track,
                                    self.beat_start,
                                    self.beat_bar,
                                    &self.beat_pattern,
                                ) {
                                    Ok(added) => {
                                        self.status = Some(format!("beat: {added} hit(s)"))
                                    }
                                    Err(error) => self.status = Some(error),
                                }
                            }
                        }
                    }
                    View::Map => {
                        ui.heading("MIDI Map");
                        ui.label(
                            egui::RichText::new(
                                "Controller bindings and track controls. Learn bindings in Windows > Controllers.",
                            )
                            .size(10.0)
                            .color(theme::text_dim()),
                        );
                        if ui.button("Open Controllers").clicked() {
                            self.controller_open = true;
                        }
                        ui.separator();
                        egui::Grid::new("midi-map-grid")
                            .striped(true)
                            .num_columns(4)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Track").strong());
                                ui.label(egui::RichText::new("Volume").strong());
                                ui.label(egui::RichText::new("Pan").strong());
                                ui.label(egui::RichText::new("Send").strong());
                                ui.end_row();
                                for track in &project.song.tracks {
                                    ui.label(if track.name.is_empty() {
                                        "untitled"
                                    } else {
                                        &track.name
                                    });
                                    ui.label(track.volume.to_string());
                                    ui.label(track.pan.to_string());
                                    ui.label(track.send_level.to_string());
                                    ui.end_row();
                                }
                            });
                        ui.separator();
                        ui.label(egui::RichText::new("Controller bindings").strong());
                        if self.midi_map.controller_bindings().is_empty() {
                            ui.label(
                                egui::RichText::new("no controller bindings yet")
                                    .size(10.0)
                                    .color(theme::text_dim()),
                            );
                        }
                        for (number, action) in self.midi_map.controller_bindings() {
                            ui.label(format!("CC {number}  →  {}", action.label()));
                        }
                    }
                    View::Perform => {
                        if kids {
                            let time = ui.input(|input| input.time) as f32;
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 190.0),
                                egui::Sense::hover(),
                            );
                            let pulse = if self.playing && !self.settings.reduced_motion {
                                (time * 3.0).sin() * 6.0
                            } else {
                                0.0
                            };
                            let center = rect.center();
                            ui.painter().circle_filled(
                                center,
                                72.0 + pulse,
                                theme::accent().gamma_multiply(if self.playing { 0.30 } else { 0.14 }),
                            );
                            ui.painter().circle_stroke(
                                center,
                                72.0 + pulse,
                                egui::Stroke::new(2.0_f32, theme::accent()),
                            );
                            ui.painter().text(
                                center,
                                egui::Align2::CENTER_CENTER,
                                if self.playing { "Playing" } else { "Press any key" },
                                egui::FontId::proportional(20.0),
                                theme::text(),
                            );
                        }
                        if !kids {
                            let current = project
                                .song
                                .chords
                                .iter()
                                .filter(|event| event.tick <= position)
                                .max_by_key(|event| event.tick)
                                .map(|event| event.chord.format(music_core::Spelling::Sharp));
                            let next = project
                                .song
                                .chords
                                .iter()
                                .filter(|event| event.tick > position)
                                .min_by_key(|event| event.tick)
                                .map(|event| event.chord.format(music_core::Spelling::Sharp));
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Now")
                                        .size(11.0)
                                        .color(theme::text_dim()),
                                );
                                ui.label(
                                    egui::RichText::new(
                                        current.unwrap_or_else(|| String::from("—")),
                                    )
                                    .size(28.0),
                                );
                                ui.add_space(18.0);
                                ui.label(
                                    egui::RichText::new("Next")
                                        .size(11.0)
                                        .color(theme::text_dim()),
                                );
                                ui.label(
                                    egui::RichText::new(next.unwrap_or_else(|| String::from("—")))
                                        .size(18.0),
                                );
                            });
                            ui.separator();
                        }
                        if let Some(chord) = &self.midi_chord {
                            ui.label(
                                egui::RichText::new(format!("Keys: {chord}"))
                                    .size(13.0)
                                    .color(theme::accent()),
                            );
                        }
                        if kids && !lesson.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("Play the next chord")
                                        .size(14.0)
                                        .color(theme::text_muted()),
                                );
                                let current = lesson
                                    .current()
                                    .map(|chord| chord.format(music_core::Spelling::Sharp))
                                    .unwrap_or_else(|| String::from("done"));
                                ui.label(egui::RichText::new(current).size(34.0));
                                let (at, total) = lesson.progress();
                                ui.label(format!("chord {at} of {total}"));
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_sized([140.0, 40.0], egui::Button::new("Next chord"))
                                        .clicked()
                                    {
                                        lesson_next = true;
                                    }
                                    if ui
                                        .add_sized([140.0, 40.0], egui::Button::new("Start over"))
                                        .clicked()
                                    {
                                        lesson_reset = true;
                                    }
                                    if ui
                                        .add_sized(
                                            [150.0, 40.0],
                                            egui::Button::new("Load kids demo"),
                                        )
                                        .clicked()
                                    {
                                        kids_load = true;
                                    }
                                });
                            });
                            ui.separator();
                        }
                        ui.label(
                            egui::RichText::new("Chord pads")
                                .size(12.0)
                                .color(theme::text_dim()),
                        );
                        let pad_width = ((ui.available_width() - 24.0) / 4.0).clamp(96.0, 190.0);
                        for row in 0..4 {
                            ui.horizontal(|ui| {
                                for column in 0..4 {
                                    let index = row * 4 + column;
                                    let Some(chord) = pads.get(index) else {
                                        continue;
                                    };
                                    let text = chord.format(music_core::Spelling::Sharp);
                                    if ui
                                        .add_sized(
                                            [pad_width, 52.0],
                                            egui::Button::new(
                                                egui::RichText::new(&text).size(14.0),
                                            ),
                                        )
                                        .clicked()
                                    {
                                        perform_chord = Some(*chord);
                                    }
                                }
                            });
                        }
                    }
                    View::Mix => {
                        self.mixer_panel(ui);
                    }
                    View::Loops => {
                        self.grid_panel(ui);
                    }
                    View::Score => {
                        let mut save_pdf = false;
                        if !self.printers_loaded {
                            self.printers = available_printers();
                            self.printers_loaded = true;
                        }
                        if self.score_track.is_none() {
                            self.score_track = project
                                .song
                                .tracks
                                .iter()
                                .find(|track| needs_grand_staff(track))
                                .or_else(|| {
                                    project.song.tracks.iter().find(|track| {
                                        !track
                                            .notes
                                            .iter()
                                            .any(|note| note.channel.zero_based() == 9)
                                    })
                                })
                                .or_else(|| project.song.tracks.first())
                                .map(|track| track.id);
                        }
                        let score_tracks: Vec<(TrackId, String)> = project
                            .song
                            .tracks
                            .iter()
                            .map(|track| (track.id, track.name.clone()))
                            .collect();
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new("Score").strong());
                            let mut write = self.score_entry;
                            if ui.checkbox(&mut write, "Write notes").changed() {
                                self.score_entry = write;
                                self.score_preview = !write;
                            }
                            let selected = self
                                .score_track
                                .or_else(|| score_tracks.first().map(|(id, _)| *id));
                            egui::ComboBox::from_id_salt("score-track")
                                .selected_text(
                                    selected
                                        .and_then(|id| {
                                            score_tracks
                                                .iter()
                                                .find(|(track, _)| *track == id)
                                                .map(|(_, name)| name.as_str())
                                        })
                                        .unwrap_or("Selected track"),
                                )
                                .show_ui(ui, |ui| {
                                    for (id, name) in &score_tracks {
                                        ui.selectable_value(&mut roll.selected, Some(*id), name);
                                    }
                                });
                            ui.label("Octave");
                            ui.add(
                                egui::DragValue::new(&mut self.score_octave).range(0..=8),
                            );
                            ui.label("Length");
                            egui::ComboBox::from_id_salt("score-length")
                                .selected_text(match self.score_note_length {
                                    value if value >= music_core::PPQ * 4 => "Whole",
                                    value if value >= music_core::PPQ * 2 => "Half",
                                    value if value >= music_core::PPQ => "Quarter",
                                    value if value >= music_core::PPQ / 2 => "Eighth",
                                    _ => "16th",
                                })
                                .show_ui(ui, |ui| {
                                    for (value, label) in [
                                        (music_core::PPQ * 4, "Whole"),
                                        (music_core::PPQ * 2, "Half"),
                                        (music_core::PPQ, "Quarter"),
                                        (music_core::PPQ / 2, "Eighth"),
                                        (music_core::PPQ / 4, "16th"),
                                    ] {
                                        ui.selectable_value(
                                            &mut self.score_note_length,
                                            value,
                                            label,
                                        );
                                    }
                                });
                            ui.checkbox(&mut self.score_dotted, "Dot");
                            if ui.button("Rest").on_hover_text("Advance the entry cursor").clicked() {
                                self.score_cursor += entry_length(
                                    self.score_note_length.max(1),
                                    self.score_dotted,
                                );
                            }
                            ui.label(format!("cursor {}", self.score_cursor));
                            if ui.small_button("Reset").clicked() {
                                self.score_cursor = 0;
                            }
                            ui.label("Staves: automatic");
                            ui.checkbox(&mut self.score_grand, "Grand staff");
                            if ui.button("−").clicked() {
                                self.score_zoom = (self.score_zoom - 0.1).max(0.5);
                            }
                            ui.label(format!("{:.1}", self.score_zoom));
                            if ui.button("+").clicked() {
                                self.score_zoom = (self.score_zoom + 0.1).min(2.5);
                            }
                            if ui.button("◀").clicked() {
                                self.score_page = self.score_page.saturating_sub(1);
                            }
                            if ui.button("▶").clicked() {
                                self.score_page += 1;
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}/{}",
                                    self.score_page + 1,
                                    self.score_state.cecilia_page_count.max(1)
                                ))
                                .size(10.0)
                                .color(theme::text_dim()),
                            );
                            if ui.button("Export PDF…").clicked() {
                                save_pdf = true;
                            }
                            if ui.button("Print…").clicked() {
                                self.print_dialog = true;
                            }
                        });
                        if let Some(track) = self
                            .score_track
                            .or(roll.selected)
                            .or_else(|| project.song.tracks.first().map(|track| track.id))
                        {
                            let grand_staff = self.score_grand
                                || needs_grand_staff(
                                    project
                                        .song
                                        .tracks
                                        .iter()
                                        .find(|entry| entry.id == track)
                                        .unwrap_or(&project.song.tracks[0]),
                                );
                            let score_settings = score::Settings {
                                grand_staff,
                                ..score::Settings::default()
                            };
                            let engraved = score::engrave(&project.song, track, score_settings);
                            if !self.score_preview {
                                if self.playing
                                    && let Some(page) =
                                        crate::score_view::page_at_tick(&engraved, position)
                                {
                                    self.score_page = page;
                                }
                                self.score_page =
                                    crate::score_view::clamp_page(&engraved, self.score_page);
                            }
                            let title = if project.song.name.is_empty() {
                                "Lazy Arranger score"
                            } else {
                                &project.song.name
                            };
                            let track_index = project
                                .song
                                .tracks
                                .iter()
                                .position(|entry| entry.id == track)
                                .unwrap_or(0);
                            if save_pdf {
                                let pdf = if self.score_preview {
                                    score::cecilia::render_pdf(
                                        &project.song,
                                        track_index,
                                        title,
                                        grand_staff,
                                    )
                                } else {
                                    Ok(score::pdf::to_pdf(&engraved, score_settings, title))
                                };
                                match pdf {
                                    Ok(pdf) => match std::fs::write(&self.score_pdf_path, pdf) {
                                        Ok(()) => {
                                            score_status =
                                                Some(format!("wrote {}", self.score_pdf_path));
                                        }
                                        Err(error) => {
                                            score_status =
                                                Some(format!("{}: {error}", self.score_pdf_path));
                                        }
                                    },
                                    Err(error) => {
                                        score_status = Some(format!("Cecilia: {error}"));
                                    }
                                }
                            }
                            let zoom = self.score_zoom;
                            let page = self.score_page;
                            let outcome = if self.score_preview {
                                self.score_state.ensure_cecilia(
                                    ui.ctx(),
                                    &project.song,
                                    track_index,
                                    title,
                                    grand_staff,
                                );
                                if let Some(error) = &self.score_state.cecilia_error {
                                    score_status = Some(format!("Cecilia: {error}"));
                                }
                                let pages = self.score_state.cecilia_page_count.max(1);
                                self.score_page = self.score_page.min(pages - 1);
                                crate::score_view::show_preview(
                                    ui,
                                    &mut self.score_state,
                                    self.score_page,
                                    zoom,
                                );
                                crate::score_view::ScoreOutcome::default()
                            } else {
                                crate::score_view::show(
                                    ui,
                                    &engraved,
                                    page,
                                    zoom,
                                    title,
                                    &mut self.score_state,
                                    position,
                                )
                            };
                            if let Some(drag) = outcome.drag {
                                if let Some(found) = project
                                    .song
                                    .tracks
                                    .iter()
                                    .find(|entry| entry.id == track)
                                    .and_then(|entry| {
                                        entry.notes.iter().find(|note| {
                                            note.start == drag.from_tick
                                                && note.pitch.as_u8() == drag.from_pitch
                                        })
                                    })
                                    .map(|note| note.id)
                                {
                                    if drag.resize {
                                        match self.editor.set_note_duration(
                                            track,
                                            found,
                                            drag.duration,
                                        ) {
                                            Ok(()) => {
                                                score_status = Some(format!(
                                                    "duration set to {} ticks",
                                                    drag.duration
                                                ));
                                            }
                                            Err(error) => score_status = Some(error),
                                        }
                                    } else {
                                        match music_core::Pitch::new(drag.to_pitch) {
                                            Ok(pitch) => {
                                                match editor.move_note(
                                                    track,
                                                    found,
                                                    drag.to_tick,
                                                    pitch,
                                                ) {
                                                    Ok(()) => {
                                                        score_status = Some(format!(
                                                            "moved to tick {} pitch {}",
                                                            drag.to_tick, drag.to_pitch
                                                        ));
                                                    }
                                                    Err(error) => {
                                                        score_status = Some(error.to_string());
                                                    }
                                                }
                                            }
                                            Err(error) => {
                                                score_status = Some(error.to_string());
                                            }
                                        }
                                    }
                                }
                            } else if let Some((tick, pitch)) = outcome.note {
                                if let Some(entry) =
                                    project.song.tracks.iter().find(|entry| entry.id == track)
                                    && let Some(found) = entry.notes.iter().find(|note| {
                                        note.start == tick && note.pitch.as_u8() == pitch
                                    })
                                {
                                    self.selected_note = Some((track, found.id));
                                    self.selected_notes.clear();
                                    score_status = Some(format!("selected note {}", found.id));
                                }
                            } else if let Some(hit) = outcome.insert
                                && entry_enabled_flag
                            {
                                match music_core::Pitch::new(hit.pitch) {
                                    Ok(pitch) => match editor.insert(track, hit.tick, pitch) {
                                        Ok(_) => {
                                            score_status = Some(format!(
                                                "inserted {} at tick {}",
                                                pitch.as_u8(),
                                                hit.tick
                                            ));
                                        }
                                        Err(error) => score_status = Some(error.to_string()),
                                    },
                                    Err(error) => score_status = Some(error.to_string()),
                                }
                            }
                        } else {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    egui::RichText::new("Add a track to see a score.")
                                        .color(theme::text_muted()),
                                );
                            });
                        }
                    }
                }
                (
                    clicked,
                    automation_edit,
                    audio_trim,
                    audio_length,
                    audio_fades,
                    audio_stretch,
                    audio_normalize,
                    score_status,
                    perform_chord,
                    lesson_next,
                    lesson_reset,
                    kids_load,
                    scroll_edit,
                    velocity_edit,
                    preview_note,
                    chord_click,
                    seek,
                    lyric_click,
                )
            });
        let (
            clicked,
            automation_edit,
            audio_trim,
            audio_length,
            audio_fades,
            audio_stretch,
            audio_normalize,
            score_status,
            perform_chord,
            lesson_next,
            lesson_reset,
            kids_load,
            scroll_edit,
            velocity_edit,
            preview_note,
            chord_click,
            seek,
            lyric_click,
        ) = panel.inner;
        if let Some(tick) = seek {
            self.seek_to(tick);
        }
        if let Some(tick) = lyric_click {
            self.lyrics_open = true;
            self.lyric_selected = Some(tick);
            self.lyric_text = self
                .song()
                .lyrics
                .iter()
                .find(|event| event.tick == tick)
                .map(|event| event.text.clone())
                .unwrap_or_default();
        }
        if let Some(tick) = chord_click {
            self.selected_chord = Some(tick);
            self.status = Some(format!("selected chord at tick {tick}"));
        }
        if let Some(click) = clicked {
            self.edit_click(click);
        }
        if let Some(pitch) = preview_note {
            self.preview_pitch(pitch);
        }
        if let Some((track, note, velocity)) = velocity_edit
            && let Err(error) = self.editor.set_velocity(track, note, velocity)
        {
            self.status = Some(error);
        }
        if let Some(edit) = scroll_edit {
            let result = match edit.mode {
                crate::piano_roll::DragMode::Resize => {
                    self.editor
                        .resize_note(edit.track, edit.note, edit.duration)
                }
                crate::piano_roll::DragMode::Move => match music_core::Pitch::new(edit.pitch) {
                    Ok(pitch) => self
                        .editor
                        .move_note(edit.track, edit.note, edit.start, pitch)
                        .map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                },
            };
            if let Err(error) = result {
                self.status = Some(error);
            }
        }
        if lesson_next {
            self.lesson.advance();
        }
        if lesson_reset {
            self.lesson.reset();
        }
        if kids_load {
            match self.editor.load_kids_demo() {
                Ok(()) => {
                    self.status = Some(String::from("Kids demo loaded"));
                    self.lesson.reset();
                }
                Err(error) => self.status = Some(error),
            }
        }
        if let Some(chord) = perform_chord {
            let mut song = Song::new("Pads", 120.0);
            let mut track = project::song::Track::new(TrackId::new(1).expect("valid"), "Pad");
            for (index, pitch) in chord.tones(4).iter().enumerate() {
                if let Ok(id) = NoteId::new(index as u32 + 1) {
                    track.notes.push(project::song::Note {
                        id,
                        start: 0,
                        duration: music_core::PPQ * 4,
                        pitch: *pitch,
                        velocity: music_core::Velocity::new(90).expect("valid"),
                        channel: music_core::Channel::first(),
                    });
                }
            }
            song.tracks.push(track);
            let rendered = audio::render(&song, audio::SAMPLE_RATE);
            match audio::player::Player::new(rendered) {
                Ok(player) => {
                    self.perform_player = Some(player);
                    self.status = Some(chord.format(music_core::Spelling::Sharp));
                }
                Err(error) => self.status = Some(error),
            }
        }
        if let Some(status) = score_status {
            self.status = Some(status);
        }
        if let Some((track, length)) = audio_length
            && let Err(error) = self.editor.trim_audio_length(track, length)
        {
            self.status = Some(error);
        }
        if let Some((track, delta)) = audio_trim {
            if let Err(error) = self.editor.trim_audio_start(track, delta) {
                self.status = Some(error);
            } else {
                self.status = Some(format!("trimmed {delta} tick(s)"));
            }
        }
        if let Some((track, fade_in, fade_out)) = audio_fades
            && let Err(error) = self.editor.set_audio_fades(track, fade_in, fade_out)
        {
            self.status = Some(error);
        }
        if let Some((track, stretch)) = audio_stretch
            && let Err(error) = self.editor.set_audio_stretch(track, stretch)
        {
            self.status = Some(error);
        }
        if let Some(track) = audio_normalize {
            match self.editor.normalize_audio(track, 0.9) {
                Ok(path) => self.status = Some(format!("normalized to {}", path.display())),
                Err(error) => self.status = Some(error),
            }
        }
        if let Some((track, lane)) = automation_edit
            && let Err(error) = self.editor.merge_automation(track, lane)
        {
            self.status = Some(error);
        }
        if !self.shown {
            self.shown = true;
            context.send_viewport_cmd(ViewportCommand::Visible(true));
        }
    }
}

/// A round transport button drawn like v1's `RoundButton`.
fn draw_track_icon(
    ui: &egui::Ui,
    rect: Rect,
    name: &str,
    program: Option<u8>,
    audio: bool,
    color: egui::Color32,
) {
    let lower = name.to_ascii_lowercase();
    let has = |word: &str| lower.contains(word);
    let icon = if audio {
        "waveform"
    } else if has("drum") || has("kit") || has("perc") {
        "drums"
    } else if has("guitar") {
        "guitar"
    } else if has("bass") {
        "bass"
    } else if has("brass") || has("trumpet") || has("trombone") || has("sax") || has("horn") {
        "brass"
    } else if has("piano") || has("keys") {
        "piano"
    } else if has("lead") || has("synth") || has("pad") || has("arp") {
        "synth"
    } else {
        match program.unwrap_or(0) {
            24..=31 => "guitar",
            32..=39 => "bass",
            56..=63 => "brass",
            80..=87 => "synth",
            0..=7 => "piano",
            _ => "instrument",
        }
    };
    let painter = ui.painter();
    let c = rect.center();
    let ink = theme::text_muted();
    let stroke = egui::Stroke::new(1.4_f32, ink);
    let line = |a: egui::Pos2, b: egui::Pos2| painter.line_segment([a, b], stroke);
    match icon {
        "drums" => {
            painter.circle_stroke(egui::pos2(c.x - 3.0, c.y + 3.0), 4.5, stroke);
            painter.circle_stroke(egui::pos2(c.x - 3.0, c.y + 3.0), 1.8, stroke);
            let snare =
                Rect::from_center_size(egui::pos2(c.x + 5.0, c.y + 2.0), egui::vec2(5.0, 3.5));
            painter.rect_stroke(snare, 1.0, stroke, egui::StrokeKind::Inside);
            line(
                egui::pos2(snare.left(), snare.bottom()),
                egui::pos2(snare.left(), c.y + 8.0),
            );
            line(
                egui::pos2(snare.right(), snare.bottom()),
                egui::pos2(snare.right(), c.y + 8.0),
            );
            line(
                egui::pos2(c.x - 7.0, c.y - 8.0),
                egui::pos2(c.x + 2.0, c.y - 8.0),
            );
            line(egui::pos2(c.x - 5.0, c.y - 8.0), egui::pos2(c.x, c.y + 1.0));
        }
        "guitar" => {
            painter.circle_stroke(egui::pos2(c.x - 4.0, c.y + 4.0), 4.5, stroke);
            painter.circle_stroke(egui::pos2(c.x + 0.5, c.y - 0.5), 3.5, stroke);
            line(
                egui::pos2(c.x + 1.0, c.y - 1.0),
                egui::pos2(c.x + 9.0, c.y - 9.0),
            );
            painter.circle_filled(egui::pos2(c.x - 4.0, c.y + 4.0), 1.5, ink);
        }
        "bass" => {
            painter.circle_stroke(egui::pos2(c.x - 4.0, c.y + 4.0), 5.0, stroke);
            line(
                egui::pos2(c.x - 1.0, c.y + 1.0),
                egui::pos2(c.x + 8.0, c.y - 8.0),
            );
            painter.rect_filled(
                Rect::from_center_size(egui::pos2(c.x + 8.5, c.y - 8.5), egui::vec2(3.0, 4.0)),
                1.0,
                ink,
            );
        }
        "brass" => {
            let points = vec![
                egui::pos2(c.x - 9.0, c.y),
                egui::pos2(c.x - 2.0, c.y + 1.0),
                egui::pos2(c.x + 2.0, c.y + 5.0),
                egui::pos2(c.x + 8.0, c.y + 6.0),
                egui::pos2(c.x + 8.0, c.y - 5.0),
                egui::pos2(c.x + 2.0, c.y - 3.0),
                egui::pos2(c.x - 2.0, c.y + 1.0),
            ];
            painter.add(egui::Shape::line(points, stroke));
            line(
                egui::pos2(c.x - 1.0, c.y - 3.0),
                egui::pos2(c.x - 1.0, c.y + 3.0),
            );
        }
        "piano" => {
            let keys = Rect::from_center_size(egui::pos2(c.x, c.y - 1.0), egui::vec2(15.0, 7.0));
            painter.rect_stroke(keys, 1.0, stroke, egui::StrokeKind::Inside);
            for index in 1..5 {
                let x = keys.left() + keys.width() * index as f32 / 5.0;
                line(egui::pos2(x, keys.top()), egui::pos2(x, keys.bottom()));
            }
            line(
                egui::pos2(keys.left() + 2.0, keys.bottom()),
                egui::pos2(keys.left() + 1.0, c.y + 9.0),
            );
            line(
                egui::pos2(keys.right() - 2.0, keys.bottom()),
                egui::pos2(keys.right() - 1.0, c.y + 9.0),
            );
        }
        "synth" => {
            let panel = Rect::from_center_size(c, egui::vec2(15.0, 10.0));
            painter.rect_stroke(panel, 1.0, stroke, egui::StrokeKind::Inside);
            for index in 0..3 {
                painter.circle_stroke(
                    egui::pos2(panel.left() + 3.0 + index as f32 * 4.0, panel.top() + 3.0),
                    1.4,
                    stroke,
                );
            }
            line(
                egui::pos2(panel.left() + 2.0, panel.bottom() - 2.0),
                egui::pos2(panel.right() - 2.0, panel.bottom() - 2.0),
            );
        }
        "waveform" => {
            let heights = [4.0_f32, 8.0, 11.0, 6.0, 9.0];
            for (index, height) in heights.iter().enumerate() {
                let x = c.x - 8.0 + index as f32 * 4.0;
                painter.rect_filled(
                    Rect::from_min_max(
                        egui::pos2(x, c.y - height / 2.0),
                        egui::pos2(x + 2.5, c.y + height / 2.0),
                    ),
                    1.0,
                    color,
                );
            }
        }
        _ => {
            let keys = Rect::from_center_size(c, egui::vec2(14.0, 10.0));
            painter.rect_stroke(keys, 1.0, stroke, egui::StrokeKind::Inside);
            for index in 1..4 {
                let x = keys.left() + keys.width() * index as f32 / 4.0;
                line(egui::pos2(x, keys.top()), egui::pos2(x, keys.bottom()));
            }
            for index in 0..3 {
                let x = keys.left() + 2.0 + index as f32 * 3.5;
                painter.rect_filled(
                    Rect::from_min_size(egui::pos2(x, keys.top()), egui::vec2(2.0, 5.0)),
                    0.5,
                    ink,
                );
            }
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

#[allow(clippy::too_many_arguments)]
fn mixer_fader(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    height: f32,
    color: egui::Color32,
    label: &str,
    default_value: f32,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(32.0, height), egui::Sense::click_and_drag());
    let response =
        response.on_hover_text(format!("{label}: drag (Shift fine), double-click to reset"));
    let double_clicked = response.double_clicked();
    let delta = ui.input(|input| input.pointer.delta().y);
    let mut changed = false;
    if double_clicked {
        *value = default_value.clamp(min, max);
        changed = true;
    } else if response.dragged() && delta != 0.0 {
        let scale = if ui.input(|input| input.modifiers.shift) {
            0.25
        } else {
            1.0
        };
        *value = (*value - delta / height * (max - min) * scale).clamp(min, max);
        changed = true;
    }
    let rect = egui::Rect::from_min_size(rect.min, egui::vec2(32.0, height));
    let center_x = rect.center().x;
    let track = egui::Rect::from_min_max(
        egui::pos2(center_x - 2.0, rect.top() + 6.0),
        egui::pos2(center_x + 2.0, rect.bottom() - 6.0),
    );
    ui.painter().rect_filled(track, 2.0, theme::raised());
    let ratio = ((*value - min) / (max - min).max(0.0001)).clamp(0.0, 1.0);
    let thumb_y = track.bottom() - ratio * track.height();
    let filled = egui::Rect::from_min_max(
        egui::pos2(center_x - 2.0, thumb_y),
        egui::pos2(center_x + 2.0, track.bottom()),
    );
    ui.painter().rect_filled(filled, 2.0, color);
    ui.painter()
        .circle_filled(egui::pos2(center_x, thumb_y), 6.0, theme::raised());
    ui.painter().circle_stroke(
        egui::pos2(center_x, thumb_y),
        6.0,
        egui::Stroke::new(1.0_f32, theme::line()),
    );
    response.on_hover_text(label).clicked() || changed
}

fn painter_strip(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 3.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 1.0, color);
}

fn pan_label(pan: f32) -> String {
    let value = pan.round() as i32;
    if value == 64 {
        String::from("C")
    } else if value < 64 {
        format!("L{}", 64 - value)
    } else {
        format!("R{}", value - 64)
    }
}

fn mixer_knob(
    ui: &mut egui::Ui,
    value: &mut f32,
    min: f32,
    max: f32,
    label: &str,
    enabled: bool,
) -> bool {
    let size = egui::vec2(44.0, 46.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    if enabled && response.dragged() {
        let delta = ui.input(|input| input.pointer.delta().y);
        *value = (*value - delta / 90.0 * (max - min)).clamp(min, max);
    }
    let changed = enabled && response.dragged() && ui.input(|input| input.pointer.delta().y != 0.0);
    let center = egui::pos2(rect.center().x, rect.top() + 15.0);
    let radius = 13.0;
    let fill = if enabled {
        theme::raised()
    } else {
        theme::panel()
    };
    ui.painter().circle_filled(center, radius, fill);
    ui.painter()
        .circle_stroke(center, radius, egui::Stroke::new(1.0_f32, theme::line()));
    let ratio = ((*value - min) / (max - min).max(0.0001)).clamp(0.0, 1.0);
    let angle = (-std::f32::consts::FRAC_PI_2 - 2.356) + ratio * 4.712;
    let end = egui::pos2(
        center.x + angle.cos() * (radius - 3.0),
        center.y + angle.sin() * (radius - 3.0),
    );
    ui.painter().line_segment(
        [center, end],
        egui::Stroke::new(
            2.0_f32,
            if enabled {
                theme::accent()
            } else {
                theme::text_dim()
            },
        ),
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 6.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(9.0),
        theme::text_muted(),
    );
    let response = if enabled {
        response.on_hover_text("Drag up or down")
    } else {
        response.on_hover_text("Chorus send is not routed")
    };
    let _ = response;
    changed
}

fn draw_metronome_icon(painter: &egui::Painter, rect: Rect, color: egui::Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let stroke = egui::Stroke::new((s * 0.07).max(1.0), color);
    let body = vec![
        egui::pos2(c.x - s * 0.08, c.y - s * 0.38),
        egui::pos2(c.x + s * 0.08, c.y - s * 0.38),
        egui::pos2(c.x + s * 0.30, c.y + s * 0.36),
        egui::pos2(c.x - s * 0.30, c.y + s * 0.36),
    ];
    painter.add(egui::Shape::closed_line(body, stroke));
    painter.line_segment(
        [
            egui::pos2(c.x, c.y + s * 0.20),
            egui::pos2(c.x + s * 0.22, c.y - s * 0.30),
        ],
        stroke,
    );
}

fn metronome_button(ui: &mut egui::Ui, rect: Rect, active: bool) -> bool {
    let response = ui.interact(rect, ui.id().with("metronome-button"), egui::Sense::click());
    let fill = if active {
        theme::accent().gamma_multiply(0.25)
    } else if response.hovered() {
        theme::line()
    } else {
        theme::raised()
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(
            1.0_f32,
            if active {
                theme::accent()
            } else {
                theme::line()
            },
        ),
        egui::StrokeKind::Inside,
    );
    draw_metronome_icon(
        ui.painter(),
        rect.shrink(6.0),
        if active {
            theme::accent()
        } else {
            theme::text()
        },
    );
    response.on_hover_text("Metronome").clicked()
}

fn draw_transport_icon(painter: &egui::Painter, rect: Rect, glyph: &str, color: egui::Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    match glyph {
        "\u{25A0}" => {
            painter.rect_filled(
                Rect::from_center_size(c, egui::vec2(s * 0.42, s * 0.42)),
                2.0,
                color,
            );
        }
        "\u{25B6}" => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - s * 0.22, c.y - s * 0.30),
                    egui::pos2(c.x - s * 0.22, c.y + s * 0.30),
                    egui::pos2(c.x + s * 0.30, c.y),
                ],
                color,
                egui::Stroke::NONE,
            ));
        }
        "\u{2016}" => {
            for offset in [-1.0_f32, 1.0] {
                painter.rect_filled(
                    Rect::from_center_size(
                        egui::pos2(c.x + offset * s * 0.13, c.y),
                        egui::vec2(s * 0.12, s * 0.44),
                    ),
                    1.0,
                    color,
                );
            }
        }
        "\u{25CF}" => {
            painter.circle_filled(c, s * 0.28, color);
        }
        "\u{21B6}" => {
            let stroke = egui::Stroke::new((s * 0.09).max(1.2), color);
            let tip = egui::pos2(c.x - s * 0.30, c.y + s * 0.28);
            let base = egui::pos2(tip.x + s * 0.14, tip.y - s * 0.14);
            let barb = s * 0.085;
            painter.line_segment([tip, egui::pos2(base.x + barb, base.y + barb)], stroke);
            painter.line_segment([tip, egui::pos2(base.x - barb, base.y - barb)], stroke);
            let control = egui::pos2(c.x + s * 0.22, c.y - s * 0.16);
            let end = egui::pos2(c.x + s * 0.12, c.y - s * 0.36);
            let points: Vec<egui::Pos2> = (0..=16)
                .map(|index| {
                    let t = index as f32 / 16.0;
                    let one = 1.0 - t;
                    egui::pos2(
                        one * one * base.x + 2.0 * one * t * control.x + t * t * end.x,
                        one * one * base.y + 2.0 * one * t * control.y + t * t * end.y,
                    )
                })
                .collect();
            painter.add(egui::Shape::line(points, stroke));
        }
        _ => {
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                glyph,
                egui::FontId::proportional(s * 0.32),
                color,
            );
        }
    }
}

fn transport_button(ui: &mut egui::Ui, rect: Rect, glyph: &str, active: bool) -> bool {
    let response = ui.interact(rect, ui.id().with(glyph), egui::Sense::click());
    let fill = if active {
        theme::accent().gamma_multiply(0.25)
    } else if response.hovered() {
        theme::line()
    } else {
        theme::raised()
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(
            1.0_f32,
            if active {
                theme::accent()
            } else {
                theme::line()
            },
        ),
        egui::StrokeKind::Inside,
    );
    let icon_colour = match glyph {
        "\u{25CF}" => theme::recording(),
        "\u{25B6}" => theme::playing(),
        "\u{2016}" => theme::accent(),
        _ => {
            if active {
                theme::accent()
            } else {
                theme::text()
            }
        }
    };
    draw_transport_icon(ui.painter(), rect, glyph, icon_colour);
    let hint = match glyph {
        "\u{25A0}" => "Stop",
        "\u{25B6}" => "Play",
        "\u{2016}" => "Pause",
        "\u{25CF}" => "Record",
        "\u{21B6}" => "Undo",
        _ => "Transport",
    };
    response.on_hover_text(hint).clicked()
}

/// Minimal file dialog: the shell must not pull a dialog crate in prematurely.
fn rfd_pick() -> Option<PathBuf> {
    // Without a dialog dependency, opening from the command line is the path.
    None
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
}

impl ProjectKind {
    /// Short label for status messages.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Json => "project",
            Self::Midi => "MIDI",
            Self::MusicXml => "MusicXML",
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
        ProjectKind::Json => {}
    }
    let text = String::from_utf8(bytes)
        .map_err(|error| ProjectError::Legacy(format!("{}: {error}", path.display())))?;
    ProjectFile::from_json(&text)
}

/// The built-in demo used when no project path is given.
#[must_use]
pub fn demo_project() -> ProjectFile {
    ProjectFile::new(arranging::demo::demo_song())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l6max_folder_import_adds_tracks() {
        let folder = std::env::temp_dir().join("lazy-arranger-shell-l6max");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("dir");
        for name in ["TAKE0.WAV", "TAKE1.WAV", "MASTER.WAV"] {
            let audio = audio::RenderedAudio {
                samples: vec![0.1; 4 * 2],
                sample_rate: 48_000,
            };
            std::fs::write(folder.join(name), audio.to_wav()).expect("take");
        }
        let mut editor = Editor::new(demo_project()).expect("editor");
        let before = editor.song().tracks.len();
        let (imported, problems) = import_l6max_folder(&mut editor, &folder, 0).expect("import");
        assert_eq!(imported, 3);
        assert_eq!(problems, 0);
        assert_eq!(editor.song().tracks.len(), before + 3);
        assert!(
            editor
                .song()
                .tracks
                .iter()
                .any(|track| track.name == "MASTER" && track.muted)
        );
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn stem_export_writes_files_and_a_manifest() {
        let directory = std::env::temp_dir().join("lazy-arranger-shell-stems");
        let _ = std::fs::remove_dir_all(&directory);
        let song = arranging::demo::demo_song();
        let stems = export_stems_folder(&song, &directory).expect("stems");
        assert!(stems > 0);
        let manifest = std::fs::read_to_string(directory.join("stems.txt")).expect("manifest");
        assert_eq!(manifest.lines().count(), stems);
        let wavs = std::fs::read_dir(&directory)
            .expect("dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "wav"))
            .count();
        assert_eq!(wavs, stems);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn wav_export_writes_a_24_bit_file() {
        let song = arranging::demo::demo_song();
        let path = std::env::temp_dir().join("lazy-arranger-export-shell.wav");
        let (seconds, problems) = export_wav_file(&song, &path).expect("wav");
        assert!(seconds > 0.0);
        assert_eq!(problems, 0);
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 1);
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 24);
        let info = audio::wav::info(&bytes).expect("info");
        assert!(info.frames > 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn l6_scenes_round_trip_as_lines() {
        let mut scenes: [Option<L6Scene>; 4] = [None, None, None, None];
        scenes[1] = Some(L6Scene {
            channels: vec![(100, 64, false, 0), (90, 40, true, 30)],
        });
        let back = decode_l6_scenes(&encode_l6_scenes(&scenes));
        assert_eq!(back[1].as_ref().expect("scene").channels.len(), 2);
        assert_eq!(
            back[1].as_ref().expect("scene").channels[1],
            (90, 40, true, 30)
        );
    }

    #[test]
    fn psr_registrations_round_trip_as_lines() {
        let registration = PsrRegistration {
            channel: 3,
            bank_msb: 63,
            bank_lsb: 1,
            program: 42,
            volume: 99,
            pan: 64,
            reverb: 55,
            chorus: 12,
        };
        assert_eq!(
            PsrRegistration::decode(&registration.encode()),
            Some(registration)
        );
        assert!(PsrRegistration::decode("1,2,3").is_none());
    }

    #[test]
    fn a_port_name_selects_a_profile() {
        let folder = std::env::temp_dir().join("lazy-arranger-port-profiles");
        let _ = std::fs::remove_dir_all(&folder);
        seed_default_device_profiles(&folder);
        let (profiles, _) = midi_devices::profiles::load_directory(&folder).expect("load");
        assert_eq!(
            profile_for_port("Yamaha PSR-S970", &profiles).as_deref(),
            Some("psr-s970")
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn default_device_profiles_are_seeded() {
        let folder = std::env::temp_dir().join("lazy-arranger-seed-profiles");
        let _ = std::fs::remove_dir_all(&folder);
        seed_default_device_profiles(&folder);
        let (profiles, problems) = midi_devices::profiles::load_directory(&folder).expect("load");
        assert_eq!(profiles.len(), 6);
        assert!(problems.is_empty());
        assert!(profiles.iter().any(|profile| profile.id == "xiosynth"));
        assert!(profiles.iter().any(|profile| profile.id == "d05"));
        assert!(profiles.iter().any(|profile| profile.id == "generic-gs"));
        assert!(profiles.iter().any(|profile| profile.id == "generic-xg"));
        assert!(profiles.iter().any(|profile| profile.id == "korg-pa"));
        assert!(profiles.iter().any(|profile| profile.id == "psr-s970"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn track_meters_follow_active_notes() {
        let mut song = project::Song::new("Meter", 120.0);
        let mut track = project::song::Track::new(project::TrackId::new(1).expect("track"), "Bass");
        track.notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("note"),
            start: 0,
            duration: music_core::PPQ,
            pitch: music_core::Pitch::from_name("C2").expect("pitch"),
            velocity: music_core::Velocity::new(100).expect("velocity"),
            channel: music_core::Channel::first(),
        });
        song.tracks.push(track);
        assert!(track_level(&song.tracks[0], 0) > 0.5);
        assert_eq!(track_level(&song.tracks[0], music_core::PPQ), 0.0);
    }

    #[test]
    fn sysex_text_is_wrapped_and_parsed() {
        assert_eq!(
            parse_sysex("7E 7F 09 01").expect("sysex"),
            vec![0xF0, 0x7E, 0x7F, 0x09, 0x01, 0xF7]
        );
        assert!(parse_sysex("F0 F7").is_ok());
        assert!(parse_sysex("F0 nope").is_err());
        assert!(parse_sysex("   ").is_err());
    }

    #[test]
    fn a_loaded_profile_supplies_the_keyboard_program() {
        let mut song = project::Song::new("Profile", 120.0);
        let mut track =
            project::song::Track::new(project::TrackId::new(1).expect("track"), "Grand Piano");
        track.notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("note"),
            start: 0,
            duration: music_core::PPQ,
            pitch: music_core::Pitch::from_name("C4").expect("pitch"),
            velocity: music_core::Velocity::new(100).expect("velocity"),
            channel: music_core::Channel::first(),
        });
        song.tracks.push(track);
        let profile = midi_devices::profiles::LoadedProfile {
            id: String::from("test"),
            name: String::from("Test"),
            voices: vec![(String::from("Grand Piano"), 42)],
            banks: Vec::new(),
            drum_kits: Vec::new(),
            controls: Vec::new(),
        };
        let path = std::env::temp_dir().join("lazy-arranger-profile.mid");
        export_midi_file_with_profile(&song, &path, Some(&profile)).expect("midi");
        let bytes = std::fs::read(&path).expect("read");
        let audit = smf::keyboard::audit(&bytes).expect("audit");
        assert!(audit.programs.contains(&42), "{:?}", audit.programs);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn midi_and_musicxml_exports_write_reloadable_files() {
        let song = arranging::demo::demo_song();
        let midi = std::env::temp_dir().join("lazy-arranger-export-shell.mid");
        let tracks = export_midi_file(&song, &midi).expect("midi");
        assert!(tracks > 0);
        let bytes = std::fs::read(&midi).expect("read midi");
        assert_eq!(&bytes[0..4], b"MThd");
        let audit = smf::keyboard::audit(&bytes).expect("audit");
        assert!(audit.tracks > 0);
        let xml = std::env::temp_dir().join("lazy-arranger-export-shell.musicxml");
        export_musicxml_file(&song, &xml, "Shell").expect("xml");
        let text = std::fs::read_to_string(&xml).expect("read xml");
        let imported = musicxml::import::import(&text).expect("import xml");
        assert_eq!(imported.tracks.len(), song.tracks.len());
        let _ = std::fs::remove_file(midi);
        let _ = std::fs::remove_file(xml);
    }

    #[test]
    fn a_musicxml_file_loads_as_a_project() {
        let song = arranging::demo::demo_song();
        let xml = musicxml::export(&song, "Round trip");
        let path = std::env::temp_dir().join("lazy-arranger-desktop-import.musicxml");
        std::fs::write(&path, xml).expect("write");
        assert_eq!(project_kind(&path), ProjectKind::MusicXml);
        let loaded = load_project(&path).expect("load");
        assert_eq!(loaded.song.tracks.len(), song.tracks.len());
        assert!(
            loaded
                .song
                .tracks
                .iter()
                .any(|track| !track.notes.is_empty())
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_midi_file_loads_as_an_untitled_project() {
        let song = arranging::demo::demo_song();
        let bytes = smf::encode(&song).expect("encode");
        let path = std::env::temp_dir().join("lazy-arranger-desktop-import.mid");
        std::fs::write(&path, bytes).expect("write");
        assert!(is_midi_path(&path));
        let loaded = load_project(&path).expect("load");
        assert_eq!(loaded.song.tracks.len(), song.tracks.len());
        assert!(
            loaded
                .song
                .tracks
                .iter()
                .any(|track| !track.notes.is_empty())
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn theory_summary_reports_key_and_cadence() {
        let song = arranging::demo::demo_song();
        let lines = theory_summary(&song);
        assert!(lines[0].starts_with("Key: "), "{:?}", lines[0]);
        assert!(lines.iter().any(|line| line.starts_with("Chords: ")));
        assert!(lines.iter().any(|line| line.starts_with("Cadence:")));
        let empty = project::song::Song::new("Empty", 120.0);
        assert_eq!(theory_summary(&empty)[2], "No chords yet");
    }

    #[test]
    fn recorded_starts_snap_to_the_grid() {
        use music_core::{Channel, Pitch, Velocity};
        use project::NoteId;
        let mut notes = vec![
            project::song::Note {
                id: NoteId::new(1).expect("valid"),
                start: 100,
                duration: 120,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            },
            project::song::Note {
                id: NoteId::new(2).expect("valid"),
                start: 940,
                duration: 120,
                pitch: Pitch::from_name("E4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            },
        ];
        quantise_recorded(&mut notes, music_core::PPQ / 4);
        assert_eq!(notes[0].start, 0);
        assert_eq!(notes[1].start, 960);
    }

    #[test]
    fn style_section_names_choose_fill_and_ending_roles() {
        assert_eq!(style_section_role("Main A"), StyleSectionRole::Main);
        assert_eq!(style_section_role("Fill In AA"), StyleSectionRole::Fill);
        assert_eq!(style_section_role("Ending A"), StyleSectionRole::Ending);
        assert_eq!(style_section_role("Intro"), StyleSectionRole::Main);
    }

    #[test]
    fn held_midi_notes_name_a_chord() {
        let mut held = [false; 128];
        held[60] = true;
        held[64] = true;
        held[67] = true;
        assert_eq!(held_chord_name(&held).as_deref(), Some("C"));
        held[60] = false;
        held[57] = true;
        held[64] = true;
        held[67] = false;
        held[60] = true;
        assert_eq!(held_chord_name(&held).as_deref(), Some("Am"));
        held[60] = false;
        held[57] = false;
        assert!(held_chord_name(&held).is_none());
    }

    #[test]
    fn style_section_queue_lands_on_the_next_bar() {
        assert_eq!(next_bar_seconds(0.0, 2.0), 2.0);
        assert_eq!(next_bar_seconds(1.9, 2.0), 2.0);
        assert_eq!(next_bar_seconds(2.0, 2.0), 4.0);
        assert_eq!(next_bar_seconds(5.2, 2.0), 6.0);
    }

    #[test]
    fn a_style_section_song_contains_compiled_notes() {
        let song = arranging::demo::demo_song();
        let native = arranger::style::from_song(
            &song,
            "Demo",
            arranger::ArrangerChord::new(
                music_core::PitchClass::new(0),
                arranger::ChordType::Major,
            ),
            music_core::PPQ * 4,
        );
        let imported = style_formats::ImportedStyle {
            native,
            source_ppq: 960,
            tempo_bpm: 120.0,
            time_signature: song.time_signature,
            song,
            sections: Vec::new(),
            stats: style_formats::ImportStats::default(),
        };
        let preview = style_section_song(&imported, 0, "C").expect("section");
        assert_eq!(preview.tracks.len(), 1);
        assert!(!preview.tracks[0].notes.is_empty());
    }

    #[test]
    fn a_style_audition_song_contains_compiled_notes() {
        let mut song = project::Song::new("Source", 120.0);
        let mut track = project::song::Track::new(project::TrackId::new(1).expect("track"), "Bass");
        track.notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("note"),
            start: 0,
            duration: music_core::PPQ,
            pitch: music_core::Pitch::from_name("C2").expect("pitch"),
            velocity: music_core::Velocity::new(100).expect("velocity"),
            channel: music_core::Channel::from_one_based(11).expect("channel"),
        });
        song.tracks.push(track);
        let style = arranger::style::from_song(
            &song,
            "Source",
            arranger::chord_type::ArrangerChord::new(
                music_core::PitchClass::new(0),
                arranger::chord_type::ChordType::Major,
            ),
            music_core::PPQ,
        );
        let preview = style_preview_song(&style).expect("preview");
        assert_eq!(preview.tracks.len(), 1);
        assert_eq!(preview.tracks[0].notes.len(), 1);
    }

    #[test]
    fn count_in_audio_has_one_accented_click_per_bar() {
        let audio = count_in_audio(120.0, 1);
        assert_eq!(audio.sample_rate, 48_000);
        assert_eq!(audio.samples.len(), 96_000 * 2);
        let frames: Vec<f32> = audio.samples.chunks(2).map(|frame| frame[0]).collect();
        let peaks: Vec<usize> = frames
            .windows(480)
            .enumerate()
            .filter(|(_, window)| window.iter().any(|sample| sample.abs() > 0.25))
            .map(|(index, _)| index)
            .take(8)
            .collect();
        assert!(!peaks.is_empty());
        assert!(peaks[0] < 480, "the first click starts the count-in");
    }

    #[test]
    fn plugin_filter_checks_name_path_and_format() {
        use plugins::{Format, FoundPlugin};
        let plugin = FoundPlugin {
            name: String::from("R0Verb"),
            vendor: String::from("Klevgrand"),
            version: String::from("1.0.4"),
            identifier: String::from("Klev:rvrb"),
            role: plugins::Role::Effect,
            path: std::path::PathBuf::from("/Library/Audio/Plug-Ins/Components/R0Verb.component"),
            format: Format::AudioUnit,
        };
        assert!(plugin_matches(&plugin, "", None));
        assert!(plugin_matches(&plugin, "r0v", None));
        assert!(plugin_matches(&plugin, "components", None));
        assert!(!plugin_matches(&plugin, "LABS", None));
        assert!(plugin_matches(&plugin, "", Some(Format::AudioUnit)));
        assert!(!plugin_matches(&plugin, "", Some(Format::Vst3)));
    }

    #[test]
    fn sysex_hex_round_trips() {
        let bytes = parse_sysex("F0 7E 7F 09 01 F7").expect("bytes");
        assert_eq!(sysex_hex(&bytes), "F0 7E 7F 09 01 F7");
        assert_eq!(parse_sysex(&sysex_hex(&bytes)).expect("back"), bytes);
    }

    #[test]
    fn midi_messages_have_compact_labels() {
        use music_core::{Channel, ControlNumber, MidiMessage, Pitch, Velocity};
        assert_eq!(
            midi_label(&MidiMessage::NoteOn {
                channel: Channel::first(),
                key: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
            }),
            "ch1 note-on C4 vel 100"
        );
        assert_eq!(
            midi_label(&MidiMessage::ControlChange {
                channel: Channel::first(),
                controller: ControlNumber::new(7).expect("valid"),
                value: 64,
            }),
            "ch1 CC 7 = 64"
        );
    }

    #[test]
    fn print_arguments_include_printer_copies_and_pages() {
        let path = std::path::Path::new("/tmp/score.pdf");
        assert_eq!(
            print_arguments(Some("Office"), 2, "1-4, 7", path),
            vec![
                "-d",
                "Office",
                "-n",
                "2",
                "-o",
                "page-ranges=1-4, 7",
                "/tmp/score.pdf"
            ]
        );
        assert_eq!(print_arguments(None, 1, "", path), vec!["/tmp/score.pdf"]);
    }

    #[test]
    fn a_save_path_without_extension_gets_json() {
        assert_eq!(
            normalize_save_path(std::path::PathBuf::from("song")),
            std::path::PathBuf::from("song.json")
        );
        assert_eq!(
            normalize_save_path(std::path::PathBuf::from("song.arranger")),
            std::path::PathBuf::from("song.arranger")
        );
        assert_eq!(
            normalize_save_path(std::path::PathBuf::from("")),
            std::path::PathBuf::from("")
        );
    }

    #[test]
    fn clip_clicks_toggle_a_multi_selection() {
        use project::{ClipId, TrackId};
        let first = (
            TrackId::new(1).expect("valid"),
            ClipId::new(1).expect("valid"),
        );
        let second = (
            TrackId::new(1).expect("valid"),
            ClipId::new(2).expect("valid"),
        );
        let mut selection = Vec::new();
        assert_eq!(apply_clip_click(&mut selection, first, false), Some(first));
        assert_eq!(selection.len(), 1);
        assert_eq!(apply_clip_click(&mut selection, second, true), Some(second));
        assert_eq!(selection.len(), 2);
        assert_eq!(apply_clip_click(&mut selection, first, true), Some(second));
        assert_eq!(selection, vec![second]);
        assert_eq!(apply_clip_click(&mut selection, first, false), Some(first));
        assert_eq!(selection, vec![first]);
    }

    #[test]
    fn plurals_read_naturally() {
        assert_eq!(plural(0, "note"), "0 notes");
        assert_eq!(plural(1, "note"), "1 note");
        assert_eq!(plural(2, "clip"), "2 clips");
        assert_eq!(plural(1, "automation lane"), "1 automation lane");
    }

    #[test]
    fn named_songs_validate() {
        for song in [
            arranging::demo::demo_song(),
            arranging::demo::kids_song(),
            project::song::Song::new("Untitled", 120.0),
        ] {
            let project = ProjectFile::new(song);
            assert!(
                project
                    .validate()
                    .iter()
                    .all(|issue| issue.severity != project::Severity::Error)
            );
        }
    }

    #[test]
    fn the_audio_engine_label_is_accurate() {
        let label = audio_engine_label();
        assert!(label.contains("48 kHz"), "{label}");
        assert!(label.contains("cpal"), "{label}");
        assert!(!label.contains("no audio engine"));
    }

    #[test]
    fn lpstat_output_parses_to_printer_names() {
        assert_eq!(
            printers_from_lpstat(
                "printer Alpha is idle.  enabled since today\nprinter Beta_2 is idle.\n"
            ),
            vec!["Alpha", "Beta_2"]
        );
        assert!(printers_from_lpstat("no printers").is_empty());
    }

    #[test]
    fn dotted_entry_lengths_are_three_halves() {
        assert_eq!(entry_length(music_core::PPQ, false), music_core::PPQ);
        assert_eq!(entry_length(music_core::PPQ, true), music_core::PPQ * 3 / 2);
    }

    #[test]
    fn score_typing_needs_the_score_view_and_focus() {
        assert!(score_typing(View::Score, true, false));
        assert!(!score_typing(View::Score, true, true));
        assert!(!score_typing(View::Score, false, false));
        assert!(!score_typing(View::Arrange, true, false));
    }

    #[test]
    fn score_keys_map_to_pitches() {
        assert_eq!(score_key_pitch(egui::Key::A, 4), Some(60));
        assert_eq!(score_key_pitch(egui::Key::W, 4), Some(61));
        assert_eq!(score_key_pitch(egui::Key::J, 4), Some(71));
        assert_eq!(score_key_pitch(egui::Key::A, 5), Some(72));
        assert_eq!(score_key_pitch(egui::Key::Z, 4), None);
    }

    #[test]
    fn view_names_parse_for_the_developer_override() {
        assert_eq!(View::parse("score"), Some(View::Score));
        assert_eq!(View::parse("MIX"), Some(View::Mix));
        assert_eq!(View::parse("arrange"), Some(View::Arrange));
        assert_eq!(View::parse("nope"), None);
    }

    #[test]
    fn the_tool_windows_draw_without_panicking() {
        let context = egui::Context::default();
        let mut app = ShellApp::new(&context, demo_project(), None);
        app.roll.selected = app.song().tracks.first().map(|track| track.id);
        app.file_mode = Some(FileMode::Open);
        app.file_path_input = String::from("");
        app.print_dialog = true;
        app.about_open = true;
        app.assistant_open = true;
        app.audio_chords_open = true;
        app.audio_style_open = true;
        app.bus_fx_open = true;
        app.controller_open = true;
        app.device_editor_open = true;
        app.generate_open = true;
        app.groove_open = true;
        app.help_open = true;
        app.inspector_open = true;
        app.l6max_mixer_open = true;
        app.library_open = true;
        app.lyrics_open = true;
        app.markers_open = true;
        app.master_fx_open = true;
        app.midi_map_open = true;
        app.midi_monitor_open = true;
        app.midi_out_open = true;
        app.pads_open = true;
        app.plugins_open = true;
        app.psr_open = true;
        app.songs_open = true;
        app.lessons_open = true;
        app.theory_open = true;
        app.style_clips_open = true;
        app.style_file_open = true;
        app.style_link_open = true;
        app.xg_open = true;
        let _ = context.run(egui::RawInput::default(), |context| {
            app.about_window(context);
            app.assistant_window(context);
            app.audio_chords_window(context);
            app.audio_style_window(context);
            app.bus_fx_window(context);
            app.controller_window(context);
            app.device_editor_window(context);
            app.file_window(context);
            app.generate_window(context);
            app.groove_window(context);
            app.help_window(context);
            app.inspector_window(context);
            app.l6max_mixer_window(context);
            app.library_window(context);
            app.lyrics_window(context);
            app.markers_window(context);
            app.master_fx_window(context);
            app.midi_map_window(context);
            app.midi_monitor_window(context);
            app.midi_out_window(context);
            app.pads_window(context);
            app.plugins_window(context);
            app.print_window(context);
            app.psr_window(context);
            app.songs_window(context);
            app.lessons_window(context);
            app.theory_window(context);
            app.style_clips_window(context);
            app.style_file_window(context);
            app.style_link_window(context);
            app.xg_window(context);
        });
    }

    #[test]
    fn kids_mode_offers_two_views_and_languages_change_labels() {
        assert_eq!(View::allowed(true), vec![View::Perform, View::Arrange]);
        assert_eq!(View::allowed(false).len(), 5);
        let danish = crate::strings::Language::parse("da");
        assert_eq!(crate::strings::text(danish, View::Score.key()), "Partitur");
        assert_eq!(crate::strings::text(danish, View::Perform.key()), "Spil");
    }
}
