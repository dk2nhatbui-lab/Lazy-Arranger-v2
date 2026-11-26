//! Headless command line for Lazy Arranger, as a library plus a thin binary.
//!
//! Every subcommand works without a window and without an audio device:
//!
//! * `ports` enumerates MIDI ports (needs the OS MIDI stack, not hardware).
//! * `demo` renders a four-bar phrase through the real scheduler and writes it
//!   as a standard MIDI file.
//! * `render` does the same for a project file.
//! * `inspect` prints what a project contains and every validation finding.
//! * `import` converts a legacy C++ `.arranger` file to the current schema.
//! * `edit` applies typed commands to a project and saves the result.
//! * `record` and `echo` are the hardware paths.

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use app_commands::{Command, EditSession};
use clap::{Parser, Subcommand};
use midi_devices::{DeviceError, list_ports};
use music_core::{Channel, Chord, PPQ, Pitch, TempoMap, Tick, Velocity};
use project::song::{Note, ProjectFile, Song, Track};
use project::{NoteId, StyleId, TrackId};
use sequencer::{EventBuffer, PreparedPattern, Scheduler};

/// Default audio sample rate for scheduling.
const DEFAULT_SAMPLE_RATE: u32 = 48_000;
/// Default audio block size for scheduling.
const DEFAULT_BLOCK: usize = 128;

#[derive(Debug, Parser)]
#[command(
    name = "lazy-arranger",
    about = "Headless Lazy Arranger: import, inspect, render, record, edit",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// List MIDI input and output ports.
    Ports,
    /// Render the built-in four-bar phrase to a standard MIDI file.
    Demo {
        /// Output `.mid` path.
        #[arg(long, default_value = "four-bars.mid")]
        out: PathBuf,
        /// Audio sample rate used by the scheduler.
        #[arg(long, default_value_t = DEFAULT_SAMPLE_RATE, value_parser = parse_sample_rate)]
        sample_rate: u32,
        /// Audio block size used by the scheduler.
        #[arg(long, default_value_t = DEFAULT_BLOCK, value_parser = parse_block)]
        block: usize,
        /// Print the first scheduled events with their sample positions.
        #[arg(long)]
        verbose: bool,
    },
    /// Render a project file to a standard MIDI file.
    Render {
        /// Project `.json` path.
        project: PathBuf,
        /// Output `.mid` path.
        #[arg(long)]
        out: PathBuf,
        /// Audio sample rate used by the scheduler.
        #[arg(long, default_value_t = DEFAULT_SAMPLE_RATE, value_parser = parse_sample_rate)]
        sample_rate: u32,
        /// Audio block size used by the scheduler.
        #[arg(long, default_value_t = DEFAULT_BLOCK, value_parser = parse_block)]
        block: usize,
        /// Print the first scheduled events with their sample positions.
        #[arg(long)]
        verbose: bool,
    },
    /// Summarise a project and report validation findings.
    Inspect {
        /// Project `.json` path.
        project: PathBuf,
    },
    /// Import a legacy C++ `.arranger` project.
    Import {
        /// Legacy `.arranger` path.
        input: PathBuf,
        /// Output project `.json` path.
        #[arg(long)]
        out: PathBuf,
        /// Print the conversion report even when there are no warnings.
        #[arg(long)]
        report: bool,
    },
    /// Apply typed edit commands to a project.
    Edit {
        /// Project `.json` path.
        project: PathBuf,
        /// Output project `.json` path.
        #[arg(long)]
        out: PathBuf,
        /// Set the tempo that starts at tick 0, for example `--tempo 96`.
        #[arg(long, allow_hyphen_values = true)]
        tempo: Option<f64>,
        /// Set a chord at a tick, for example `--chord 3840=Am7`. Repeatable.
        #[arg(long = "chord", value_name = "TICK=SYMBOL")]
        chords: Vec<String>,
        /// Add an empty named track. Repeatable.
        #[arg(long = "add-track", value_name = "NAME")]
        add_tracks: Vec<String>,
    },
    /// Inspect a Yamaha style, and optionally render one section.
    Style {
        /// Style file (`.sty`, `.prs`, and the other Yamaha style extensions).
        style: PathBuf,
        /// Section to render. Defaults to the first section.
        #[arg(long)]
        section: Option<String>,
        /// Write the compiled section to this `.mid` file.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Chord at a tick, for example `--chord 0=Am7`. Repeatable.
        #[arg(long = "chord", value_name = "TICK=SYMBOL")]
        chords: Vec<String>,
        /// Print every part rule of the chosen section.
        #[arg(long)]
        rules: bool,
    },
    /// Export a song for the Yamaha PSR-S970 Song player.
    ExportKeyboard {
        /// Project `.json` path.
        project: PathBuf,
        /// Output `.mid` path.
        #[arg(long)]
        out: PathBuf,
        /// Voice per track as CHANNEL:PROGRAM, repeatable.
        #[arg(long = "voice", value_name = "CHANNEL:PROGRAM")]
        voices: Vec<String>,
        /// Device profile YAML; enables profile voices and bank select.
        #[arg(long)]
        profile: Option<PathBuf>,
    },
    /// Apply a decibel gain to a WAV file.
    WavGain {
        /// Input `.wav` file.
        wav: PathBuf,
        /// Output `.wav` path.
        #[arg(long)]
        out: PathBuf,
        /// Gain in decibels.
        #[arg(long, default_value_t = -3.0, allow_hyphen_values = true)]
        db: f32,
    },
    /// Normalize a WAV file to a peak level.
    WavNormalize {
        /// Input `.wav` file.
        wav: PathBuf,
        /// Output `.wav` path.
        #[arg(long)]
        out: PathBuf,
        /// Target peak, 0..=1.
        #[arg(long, default_value_t = 0.9)]
        peak: f32,
    },
    /// Trim a WAV file to a time window.
    WavTrim {
        /// Input `.wav` file.
        wav: PathBuf,
        /// Output `.wav` path.
        #[arg(long)]
        out: PathBuf,
        /// Start seconds.
        #[arg(long, default_value_t = 0.0)]
        start: f32,
        /// End seconds; omitted keeps the tail.
        #[arg(long)]
        end: Option<f32>,
    },
    /// Apply linear fade-in and fade-out to a WAV file.
    WavFade {
        /// Input `.wav` file.
        wav: PathBuf,
        /// Output `.wav` path.
        #[arg(long)]
        out: PathBuf,
        /// Fade-in seconds.
        #[arg(long, default_value_t = 0.0)]
        fade_in: f32,
        /// Fade-out seconds.
        #[arg(long, default_value_t = 0.0)]
        fade_out: f32,
    },
    /// Reverse a WAV file.
    WavReverse {
        /// Input `.wav` file.
        wav: PathBuf,
        /// Output `.wav` path.
        #[arg(long)]
        out: PathBuf,
    },
    /// Send all-notes-off on every channel to a MIDI output.
    MidiPanic {
        /// Output port name exactly as listed by `midi-ports`.
        #[arg(long)]
        output: String,
    },
    /// Import a Standard MIDI File as a project.
    MidiImport {
        /// Input `.mid` file.
        midi: PathBuf,
        /// Output `.json` path.
        #[arg(long)]
        out: PathBuf,
    },
    /// Add an audio file to a project as an audio track reference.
    AddAudio {
        /// Project `.json` path.
        project: PathBuf,
        /// Audio `.wav` file.
        wav: PathBuf,
        /// Output project `.json` path.
        #[arg(long)]
        out: PathBuf,
        /// Start tick; defaults to zero.
        #[arg(long, default_value_t = 0)]
        start: Tick,
    },
    /// Find chords in a WAV file and optionally write a chord project.
    ChordsFromAudio {
        /// Source `.wav` file.
        wav: PathBuf,
        /// Tempo used for bar length and ticks.
        #[arg(long, default_value_t = 120.0)]
        bpm: f64,
        /// Output project `.json` path with the found chords.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Recognize chords in a local audio file and print them.
    AudioToChords {
        /// Source audio file.
        wav: PathBuf,
        /// Tempo used to place notes; defaults to 120.
        #[arg(long, default_value_t = 120.0)]
        bpm: f64,
        /// Segment length in seconds.
        #[arg(long, default_value_t = 0.5)]
        segment: f64,
    },
    /// Transcribe monophonic audio to a MIDI file.
    AudioToMidi {
        /// Source audio file.
        wav: PathBuf,
        /// Output `.mid` path.
        out: PathBuf,
        /// Tempo used to place notes; defaults to 120.
        #[arg(long, default_value_t = 120.0)]
        bpm: f64,
        /// Transcribe kick/snare/hi-hat instead of melody.
        #[arg(long)]
        drums: bool,
    },
    /// Write a track's score as a PDF.
    ScorePdf {
        /// Project `.json` path.
        project: PathBuf,
        /// Output `.pdf` path.
        #[arg(long)]
        out: PathBuf,
        /// Track index, from zero; defaults to the first track.
        #[arg(long, default_value_t = 0)]
        track: usize,
        /// Score title; defaults to the song name.
        #[arg(long)]
        title: Option<String>,
        /// Write every track as a review full score instead of one track.
        #[arg(long)]
        all: bool,
        /// Include guitar chord frames in the full score.
        #[arg(long)]
        frames: bool,
    },
    /// Lay out a track as score pages and report the geometry.
    ScoreLayout {
        /// Project `.json` path.
        project: PathBuf,
        /// Track index, from zero; defaults to the first track.
        #[arg(long, default_value_t = 0)]
        track: usize,
    },
    /// List styles under a folder with stable numbers.
    StyleCatalog {
        /// Folder to scan.
        root: PathBuf,
        /// Show only the first N entries.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Render one numbered style from a folder.
    StyleByNumber {
        /// Number from `style-catalog`.
        number: usize,
        /// Folder to scan.
        root: PathBuf,
        /// Section to render; defaults to the first.
        #[arg(long)]
        section: Option<String>,
        /// Write the compiled section to this `.mid` file.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Print a MIDI control map.
    MidiMapShow {
        /// Map file path.
        #[arg(long, default_value = "lazy-arranger-midi-map.txt")]
        file: PathBuf,
    },
    /// Bind one control in a MIDI control map.
    MidiMapBind {
        /// Map file path.
        #[arg(long, default_value = "lazy-arranger-midi-map.txt")]
        file: PathBuf,
        /// Control, e.g. `note 36` or `cc 7`.
        control: String,
        /// Action, e.g. `play`, `stop`, `scene 1`, `fader track-2`.
        action: String,
    },
    /// Export a project to MusicXML, re-import it and report losses.
    MusicXmlAudit {
        /// Project `.json` path.
        project: PathBuf,
    },
    /// Render the mix to a WAV and insert it as an audio track.
    Bounce {
        /// Project `.json` path.
        project: PathBuf,
        /// Output `.wav`.
        #[arg(long)]
        wav: PathBuf,
        /// Output project `.json`.
        #[arg(long)]
        out: PathBuf,
        /// Name for the new track.
        #[arg(long, default_value = "Bounce")]
        name: String,
        /// Sample rate.
        #[arg(long, default_value_t = 48_000, value_parser = parse_sample_rate)]
        sample_rate: u32,
    },
    /// Write an XG System On SysEx dump.
    SysexXgOn {
        /// Output `.syx` file.
        #[arg(long)]
        out: PathBuf,
        /// Device number, 0..=15.
        #[arg(long, default_value_t = 0)]
        device: u8,
    },
    /// Send a hex SysEx message and print any SysEx replies.
    MidiSysex {
        /// MIDI input port name.
        #[arg(long = "in")]
        input: String,
        /// MIDI output port name.
        #[arg(long = "out")]
        output: String,
        /// Hex bytes from F0 to F7 (spaces optional).
        #[arg(long)]
        hex: String,
        /// Reply timeout in milliseconds.
        #[arg(long, default_value_t = 1200)]
        timeout_ms: u64,
    },
    /// Send a Universal Identity Request (`F0 7E 7F 06 01 F7`) and print SysEx replies.
    MidiIdentity {
        /// MIDI input port name.
        #[arg(long = "in")]
        input: String,
        /// MIDI output port name.
        #[arg(long = "out")]
        output: String,
        /// Reply timeout in milliseconds.
        #[arg(long, default_value_t = 1200)]
        timeout_ms: u64,
    },
    /// Monitor raw MIDI bytes (including SysEx and realtime) on an input port.
    MidiMonitorRaw {
        /// MIDI input port name.
        #[arg(long = "in")]
        input: String,
        /// Listening window in seconds.
        #[arg(long, default_value_t = 5.0)]
        seconds: f64,
    },
    /// Read the active temporary patch from a Roland D-05 / D-50 over SysEx.
    D05Read {
        /// MIDI input port name.
        #[arg(long = "in", default_value = "D-05")]
        input: String,
        /// MIDI output port name.
        #[arg(long = "out", default_value = "D-05")]
        output: String,
        /// Roland device ID (0..=31; default 0 = channel 1).
        #[arg(long, default_value_t = 0)]
        device: u8,
        /// Optional path to write the raw received `.syx` dump.
        #[arg(long)]
        save: Option<PathBuf>,
    },
    /// Read patch names from a Roland D-05 bank over MIDI + SysEx (restores active patch).
    D05Names {
        /// MIDI input port name.
        #[arg(long = "in", default_value = "D-05")]
        input: String,
        /// MIDI output port name.
        #[arg(long = "out", default_value = "D-05")]
        output: String,
        /// Roland device ID (0..=31; default 0 = channel 1).
        #[arg(long, default_value_t = 0)]
        device: u8,
        /// MIDI channel (1..=16).
        #[arg(long, default_value_t = 1)]
        channel: u8,
        /// Bank LSB: 0..=7 Internal 1–8, 8..=13 Preset 1–6.
        #[arg(long, default_value_t = 0)]
        bank: u8,
        /// Number of patches to query (1..=64).
        #[arg(long, default_value_t = 8)]
        count: usize,
    },
    /// Send a MIDI realtime transport byte (start, continue, stop) to a port (e.g. L6max).
    L6maxTransport {
        /// MIDI output port name.
        #[arg(long = "out", default_value = "L6max MIDI I/O Port")]
        output: String,
        /// start, continue, or stop.
        action: String,
    },
    /// Repeat a marker-named section in place.
    JoinLoop {
        /// Project `.json` path.
        project: PathBuf,
        /// Section marker name.
        #[arg(long)]
        section: String,
        /// Total copies that should exist.
        #[arg(long, default_value_t = 2)]
        times: usize,
        /// Output project `.json`.
        #[arg(long)]
        out: PathBuf,
    },
    /// Plan or apply a song-form transition at a tick.
    FormTransition {
        /// Project `.json` path.
        project: PathBuf,
        /// Join tick.
        #[arg(long)]
        tick: i64,
        /// straight, fill, lead, automatic, hold or break.
        #[arg(long, default_value = "automatic")]
        kind: String,
        /// Apply the plan and write `--out`.
        #[arg(long)]
        apply: bool,
        /// Output project when applying.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Assign an SF2 preset to a track.
    Sf2Use {
        /// Project `.json` path.
        project: PathBuf,
        /// Track id.
        #[arg(long)]
        track: u32,
        /// SF2 file.
        #[arg(long)]
        file: PathBuf,
        /// Bank number.
        #[arg(long, default_value_t = 0)]
        bank: u16,
        /// Program number.
        #[arg(long, default_value_t = 0)]
        program: u16,
        /// Output project `.json`.
        #[arg(long)]
        out: PathBuf,
    },
    /// Render one note from an SF2 preset to a WAV file.
    Sf2Render {
        /// SF2 file.
        file: PathBuf,
        /// Note name (`C4`) or MIDI number.
        note: String,
        /// Bank number.
        #[arg(long, default_value_t = 0)]
        bank: u16,
        /// Program number.
        #[arg(long, default_value_t = 0)]
        program: u16,
        /// Length in seconds.
        #[arg(long, default_value_t = 1.0)]
        seconds: f32,
        /// Output `.wav` file.
        #[arg(long)]
        out: PathBuf,
    },
    /// Send a short note on one MIDI channel to locate its sound.
    MidiChannelCheck {
        /// Output port name, as `ports` reports it.
        #[arg(long)]
        output: String,
        /// MIDI channel, 0..=15.
        #[arg(long, default_value_t = 0)]
        channel: u8,
        /// Note number.
        #[arg(long, default_value_t = 60)]
        note: u8,
        /// Note length in seconds.
        #[arg(long, default_value_t = 1.0)]
        seconds: f64,
    },
    /// Record the default audio input to a WAV file.
    RecordAudio {
        /// Output `.wav` path.
        #[arg(long)]
        out: PathBuf,
        /// Seconds to record.
        #[arg(long, default_value_t = 3.0)]
        seconds: f64,
        /// Input device name; omitted uses the default.
        #[arg(long)]
        device: Option<String>,
        /// One input channel; omitted mixes all.
        #[arg(long)]
        channel: Option<usize>,
    },
    /// Play a project through cpal and wait for it to finish.
    PlayAudio {
        /// Project `.json` path.
        project: PathBuf,
        /// Output device; omitted uses the default.
        #[arg(long)]
        device: Option<String>,
        /// Stop after this many seconds; omitted plays the whole render.
        #[arg(long)]
        seconds: Option<f64>,
    },
    /// List audio input and output devices.
    AudioDevices,
    /// List installed VST3 and Audio Unit bundles, without hosting them.
    PluginScan {
        /// Extra folders to scan; omitted scans the standard plug-in folders.
        #[arg(long = "dir")]
        dirs: Vec<PathBuf>,
    },
    /// Parse an SF2 SoundFont and list its presets.
    Sf2Info {
        /// SF2 file.
        file: PathBuf,
    },
    /// Recognise a Korg PCG container; decoding is not implemented.
    KorgScan {
        /// File to scan.
        file: PathBuf,
    },
    /// Render each audible track to its own WAV file.
    ExportStems {
        /// Project `.json` path.
        project: PathBuf,
        /// Output folder.
        #[arg(long)]
        out_dir: PathBuf,
        /// Sample rate.
        #[arg(long, default_value_t = 48_000, value_parser = parse_sample_rate)]
        sample_rate: u32,
    },
    /// Add a tempo change and print the map.
    TempoSet {
        /// Project `.json` path.
        project: PathBuf,
        /// Tick for the change.
        #[arg(long)]
        tick: i64,
        /// Beats per minute.
        #[arg(long)]
        bpm: f64,
        /// Output project `.json`.
        #[arg(long)]
        out: PathBuf,
    },
    /// Save, list or apply groove files.
    GrooveLibrary {
        /// `save`, `list` or `apply`.
        action: String,
        /// Folder holding `.groove` files.
        #[arg(long, default_value = "grooves")]
        dir: PathBuf,
        /// Project for save/apply.
        #[arg(long)]
        project: Option<PathBuf>,
        /// Track id for save/apply.
        #[arg(long)]
        track: Option<u32>,
        /// Groove name for save/apply.
        #[arg(long)]
        name: Option<String>,
        /// Output project for apply.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Timing amount for apply, 0..=1.
        #[arg(long, default_value_t = 1.0)]
        timing: f32,
        /// Accent amount for apply, 0..=1.
        #[arg(long, default_value_t = 1.0)]
        accents: f32,
    },
    /// Fit tracks to a synth's range and polyphony, optionally applying.
    FitProject {
        /// Project `.json` path.
        project: PathBuf,
        /// Lowest playable note.
        #[arg(long, default_value_t = 21)]
        low: u8,
        /// Highest playable note.
        #[arg(long, default_value_t = 108)]
        high: u8,
        /// Notes that may sound at once.
        #[arg(long, default_value_t = 16)]
        polyphony: usize,
        /// Declare the synth has no drum kit.
        #[arg(long)]
        no_drum_kit: bool,
        /// Apply the proposals and write `--out`.
        #[arg(long)]
        apply: bool,
        /// Output project when applying.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Print chord inversions, or write an A/B pair of WAV files.
    Voicing {
        /// Chord symbol.
        chord: String,
        /// Octave for the root position.
        #[arg(long, default_value_t = 4)]
        octave: i8,
        /// Folder for `a.wav` and `b.wav`; omitted prints only.
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },
    /// Load a device profile and optionally resolve a track name.
    DeviceProfile {
        /// YAML profile file.
        file: PathBuf,
        /// Track name to resolve.
        #[arg(long)]
        track: Option<String>,
    },
    /// Convert a Music Finder CSV into a Yamaha playlist TSV.
    PlaylistFromCsv {
        /// Music Finder `.csv` file.
        csv: PathBuf,
        /// Folder the playlist entries point at.
        #[arg(long, default_value = "/styles")]
        folder: String,
        /// Output `.tsv` path.
        #[arg(long)]
        out: PathBuf,
    },
    /// Import a style's tracks into a project by number or name.
    LibraryImport {
        /// Project `.json` path.
        #[arg(long)]
        project: PathBuf,
        /// Style library folder.
        #[arg(long)]
        root: PathBuf,
        /// Catalog number; takes precedence.
        #[arg(long)]
        number: Option<usize>,
        /// Style name hint.
        #[arg(long, default_value = "")]
        name: String,
        /// Prefix for imported track names.
        #[arg(long, default_value = "Style")]
        prefix: String,
        /// Output project `.json` path.
        #[arg(long)]
        out: PathBuf,
    },
    /// Resolve a style by number or name in a folder.
    StyleResolve {
        /// Folder to scan.
        root: PathBuf,
        /// Catalog number; takes precedence over the name.
        #[arg(long)]
        number: Option<usize>,
        /// Style name hint.
        #[arg(long, default_value = "")]
        name: String,
    },
    /// Import Zoom L6max takes as audio tracks and report the plan.
    L6maxImport {
        /// Folder holding the recorded `.wav` takes.
        folder: PathBuf,
        /// Base project `.json` path.
        #[arg(long)]
        project: PathBuf,
        /// Output project `.json` path.
        #[arg(long)]
        out: PathBuf,
        /// Timeline tick where every take starts.
        #[arg(long, default_value_t = 0)]
        start: Tick,
    },
    /// Generate a part: humanise notes, drumify bars or write bass/pad/arp.
    Generate {
        /// Project `.json` path.
        project: PathBuf,
        /// Target track id.
        #[arg(long)]
        track: u32,
        /// Operation: humanize, drumify, bass, pad or arp.
        #[arg(long)]
        op: String,
        /// Output project `.json` path.
        #[arg(long)]
        out: PathBuf,
        /// Timing jitter in ticks for humanize.
        #[arg(long, default_value_t = 8)]
        timing: i64,
        /// Velocity jitter for humanize.
        #[arg(long, default_value_t = 4)]
        velocity: u8,
        /// Random seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Bars to repeat for continue.
        #[arg(long, default_value_t = 1)]
        bars: usize,
    },
    /// Slice a drum loop at its attacks and write one WAV per pad.
    DrumSlice {
        /// Source `.wav` file.
        wav: PathBuf,
        /// Output folder for `pad-<note>.wav` files.
        #[arg(long)]
        out: PathBuf,
    },
    /// Print a scale as pitch-class names.
    TheoryScale {
        /// Tonic, e.g. C, F# or Bb.
        tonic: String,
        /// Mode name.
        #[arg(long, default_value = "major")]
        mode: String,
    },
    /// Print roman labels and the final cadence for a progression.
    TheoryAnalyze {
        /// Chord symbols, in order.
        chords: Vec<String>,
        /// Tonic; defaults to the first chord's root.
        #[arg(long)]
        key: Option<String>,
    },
    /// Print the voice-leading cost between two chords.
    TheoryVoiceLeading {
        /// First chord symbol.
        from: String,
        /// Second chord symbol.
        to: String,
    },
    /// Write the built-in demo project as JSON.
    DemoProject {
        /// Output `.json` path.
        #[arg(long)]
        out: PathBuf,
    },
    /// Export a project as MusicXML.
    ExportMusicXml {
        /// Project `.json` path.
        project: PathBuf,
        /// Output `.musicxml` path.
        #[arg(long)]
        out: PathBuf,
        /// Score title; defaults to the song name.
        #[arg(long)]
        title: Option<String>,
    },
    /// Import a MusicXML score as a project.
    ImportMusicXml {
        /// Source `.musicxml` file.
        musicxml: PathBuf,
        /// Output project `.json` path.
        #[arg(long)]
        out: PathBuf,
    },
    /// Manage the recent-projects list.
    Recent {
        /// List file path.
        #[arg(long, default_value = "recent-projects.txt")]
        file: PathBuf,
        /// Add a project to the front of the list.
        #[arg(long)]
        record: Option<PathBuf>,
        /// Clear the list.
        #[arg(long)]
        clear: bool,
    },
    /// Read a Yamaha Music Finder `.mfd` file and write a book.
    MusicFinderImport {
        /// Source `.mfd` file.
        mfd: PathBuf,
        /// Output path: `.json` (default) or `.csv`.
        out: PathBuf,
    },
    /// Print the assistant tool list as JSON.
    McpTools {},
    /// Serve assistant tools on loopback TCP with a per-start token.
    McpSocket {
        /// Project `.json` path.
        project: PathBuf,
        /// Port; 0 picks a free one.
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// File that receives the token.
        #[arg(long, default_value = "mcp-token.txt")]
        token_file: PathBuf,
    },
    /// Serve assistant tools over newline-delimited JSON-RPC on stdin/stdout.
    McpServe {
        /// Project `.json` path; saved after every successful tool call.
        project: PathBuf,
    },
    /// Call one assistant tool on a project and print the JSON result.
    McpCall {
        /// Project `.json` path.
        project: PathBuf,
        /// Tool name.
        #[arg(long)]
        tool: String,
        /// JSON arguments object.
        #[arg(long, default_value = "{}")]
        args: String,
        /// Write the result back to the project.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Write a style container back to disk (MIDI song plus CASM).
    StyleSave {
        /// Style file to read.
        style: PathBuf,
        /// Output style path.
        #[arg(long)]
        out: PathBuf,
        /// sff1 or sff2 (default).
        #[arg(long, default_value = "sff2")]
        version: String,
    },
    /// Build a style section from a project's tracks.
    StyleFromClips {
        /// Project `.json` path.
        project: PathBuf,
        /// Output style path.
        #[arg(long)]
        out: PathBuf,
        /// Section name written to the style.
        #[arg(long, default_value = "Main A")]
        section: String,
        /// Source chord, for example C or Am7.
        #[arg(long, default_value = "C")]
        chord: String,
        /// Section length in ticks (960 per quarter note).
        #[arg(long, default_value_t = 3840)]
        length: u32,
        /// Split sections at the project's markers instead of one section.
        #[arg(long)]
        from_markers: bool,
        /// sff1 or sff2 (default).
        #[arg(long, default_value = "sff2")]
        version: String,
    },
    /// Build a one-part style from monophonic audio notes.
    StyleFromAudio {
        /// Audio file (WAV or compressed).
        audio: PathBuf,
        /// Output style path.
        #[arg(long)]
        out: PathBuf,
        /// Tempo used to place detected notes.
        #[arg(long, default_value_t = 120.0)]
        bpm: f64,
        /// Section name.
        #[arg(long, default_value = "Main A")]
        section: String,
        /// Source chord, or `auto`.
        #[arg(long, default_value = "C")]
        chord: String,
        /// Split notes below C3 into a bass part and the rest into melody.
        #[arg(long)]
        split_parts: bool,
        /// Split into sections of this many 4/4 bars (0 keeps one section).
        #[arg(long, default_value_t = 0)]
        section_bars: u32,
        /// Derive a Drums part from onsets.
        #[arg(long)]
        drums: bool,
        /// Detect BPM from onsets instead of using --bpm.
        #[arg(long)]
        detect_tempo: bool,
        /// sff1 or sff2 (default).
        #[arg(long, default_value = "sff2")]
        version: String,
    },
    /// Guess an instrument family from audio.
    AudioTags {
        /// Audio file (WAV or compressed).
        audio: PathBuf,
    },
    /// Extract a groove template from monophonic audio.
    GrooveFromAudio {
        /// Audio file (WAV or compressed).
        audio: PathBuf,
        /// Folder holding `.groove` files.
        #[arg(long, default_value = "grooves")]
        dir: PathBuf,
        /// Groove name.
        #[arg(long)]
        name: String,
        /// Tempo used to place notes.
        #[arg(long, default_value_t = 120.0)]
        bpm: f64,
        /// Detect BPM from onsets instead.
        #[arg(long)]
        detect_tempo: bool,
        /// Grid denominator (4 = quarter, 16 = sixteenth).
        #[arg(long, default_value_t = 16)]
        grid: i64,
    },
    /// Turn a YouTube (or other yt-dlp) link into a MIDI file.
    LinkToMidi {
        /// Link to a video or audio page.
        link: String,
        /// Output `.mid` path.
        #[arg(long)]
        out: PathBuf,
        /// Folder for the downloaded WAV.
        #[arg(long)]
        folder: Option<PathBuf>,
        /// Folder holding ffmpeg when it is not on PATH.
        #[arg(long)]
        ffmpeg: Option<PathBuf>,
        /// Keep the downloaded WAV.
        #[arg(long)]
        keep: bool,
    },
    /// Render a project to a WAV file (16/24-bit PCM or 32-bit float).
    ExportAudio {
        /// Project `.json` path.
        project: PathBuf,
        /// Output `.wav` path.
        #[arg(long)]
        out: PathBuf,
        /// Sample rate; defaults to 48000.
        #[arg(long, default_value_t = audio::SAMPLE_RATE, value_parser = parse_sample_rate)]
        sample_rate: u32,
        /// Bit depth: 16, 24 or 32 (float; default 16).
        #[arg(long, default_value_t = 16)]
        bits: u16,
        /// Mix a metronome click track.
        #[arg(long)]
        metronome: bool,
        /// Master gain in decibels.
        #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
        gain_db: f32,
        /// Turn the master limiter off.
        #[arg(long)]
        no_limiter: bool,
        /// Master reverb wet amount, 0..=1.
        #[arg(long, default_value_t = 0.0)]
        reverb: f32,
        /// Master chorus wet amount, 0..=1.
        #[arg(long, default_value_t = 0.0)]
        chorus: f32,
    },
    /// Record a style performance as linked intent and rendered tracks.
    Perform {
        /// Style file.
        style: PathBuf,
        /// Output project `.json` path.
        #[arg(long)]
        out: PathBuf,
        /// Section to perform. Defaults to the first section.
        #[arg(long)]
        section: Option<String>,
        /// Chord at a tick, for example `--chord 0=Am7`. Repeatable.
        #[arg(long = "chord", value_name = "TICK=SYMBOL")]
        chords: Vec<String>,
        /// Tempo; defaults to the style's own tempo.
        #[arg(long)]
        tempo: Option<f64>,
    },
    /// Regenerate generated tracks after an intent edit.
    Regenerate {
        /// Project `.json` path.
        project: PathBuf,
        /// Style file the generated tracks came from.
        #[arg(long)]
        style: PathBuf,
        /// Output project `.json` path.
        #[arg(long)]
        out: PathBuf,
        /// keep (default), replace or duplicate.
        #[arg(long, default_value = "keep")]
        policy: String,
    },
    /// Play a project to a MIDI output port in real time.
    Play {
        /// Project `.json` path.
        project: PathBuf,
        /// Output port name, exactly as `ports` prints it.
        #[arg(long = "out-port")]
        out_port: String,
        /// Audio sample rate used to convert sample positions to time.
        #[arg(long, default_value_t = DEFAULT_SAMPLE_RATE, value_parser = parse_sample_rate)]
        sample_rate: u32,
        /// Print the schedule without sending anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Record MIDI input to a standard MIDI file.
    Record {
        /// Input port name, exactly as `ports` prints it.
        #[arg(long = "in")]
        input: String,
        /// Output `.mid` path.
        #[arg(long)]
        out: PathBuf,
        /// Recording length in seconds.
        #[arg(long, default_value_t = 5.0)]
        seconds: f64,
        /// Tempo used to place recorded notes on the tick timeline.
        #[arg(long, default_value_t = 120.0)]
        tempo: f64,
        /// Wait up to one hour for the first note, then record.
        #[arg(long)]
        wait: bool,
    },
    /// Send one MIDI control change.
    MidiCc {
        /// Output port name, exactly as `ports` prints it.
        #[arg(long)]
        output: String,
        /// MIDI channel, 0..=15.
        #[arg(long, default_value_t = 0)]
        channel: u8,
        /// Controller number, 0..=127.
        #[arg(long)]
        controller: u8,
        /// Controller value, 0..=127.
        #[arg(long)]
        value: u8,
    },
    /// Print incoming MIDI messages for a few seconds.
    MidiMonitor {
        /// Input port name, exactly as `ports` prints it.
        #[arg(long = "in")]
        input: String,
        /// How long to monitor, in seconds.
        #[arg(long, default_value_t = 5.0)]
        seconds: f64,
    },
    /// Forward MIDI input to an output port to check wiring.
    Echo {
        /// Input port name, exactly as `ports` prints it.
        #[arg(long = "in")]
        input: String,
        /// Output port name, exactly as `ports` prints it.
        #[arg(long)]
        out: String,
        /// How long to forward, in seconds.
        #[arg(long, default_value_t = 5.0)]
        seconds: f64,
    },
}

/// Parses the process arguments and runs one subcommand.
pub fn cli_main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    match cli.command {
        Commands::Ports => ports(),
        Commands::Demo {
            out,
            sample_rate,
            block,
            verbose,
        } => demo(&out, sample_rate, block, verbose),
        Commands::Render {
            project,
            out,
            sample_rate,
            block,
            verbose,
        } => render(&project, &out, sample_rate, block, verbose),
        Commands::Inspect { project } => inspect(&project),
        Commands::Import { input, out, report } => import(&input, &out, report),
        Commands::Edit {
            project,
            out,
            tempo,
            chords,
            add_tracks,
        } => edit(&project, &out, tempo, &chords, &add_tracks),
        Commands::ExportKeyboard {
            project,
            out,
            voices,
            profile,
        } => {
            let project = read_project(&project)?;
            let built_in = &midi_devices::PSR_S970;
            let loaded = match &profile {
                Some(path) => Some(midi_devices::profiles::load_file(path)?),
                None => None,
            };
            let first_bank = loaded.as_ref().and_then(|loaded| {
                loaded
                    .banks
                    .iter()
                    .find_map(|bank| Some((bank.msb?, bank.lsb?)))
            });
            let kit = loaded.as_ref().and_then(|loaded| loaded.drum_kits.first());
            let mut chosen: Vec<smf::KeyboardVoice> = project
                .song
                .tracks
                .iter()
                .enumerate()
                .map(|(index, track)| {
                    let drums = track.name.to_lowercase().contains("drum")
                        || track
                            .notes
                            .iter()
                            .any(|note| note.channel.zero_based() == 9);
                    let (program, bank) = if drums {
                        (
                            kit.map_or(0, |kit| kit.program),
                            kit.and_then(|kit| Some((kit.msb?, kit.lsb?))),
                        )
                    } else {
                        let program = track.program.unwrap_or_else(|| match &loaded {
                            Some(loaded) => {
                                midi_devices::profiles::nearest_in_profile(loaded, &track.name)
                            }
                            None => midi_devices::nearest_voice(built_in, &track.name),
                        });
                        (program, first_bank)
                    };
                    smf::KeyboardVoice {
                        channel: if drums {
                            9
                        } else {
                            u8::try_from(index).unwrap_or(0) & 0x0f
                        },
                        program,
                        volume: track.volume.min(127),
                        pan: track.pan.min(127),
                        bank,
                    }
                })
                .collect();
            for (index, entry) in voices.iter().enumerate() {
                let (channel, program) = entry
                    .split_once(':')
                    .ok_or_else(|| format!("--voice expects CHANNEL:PROGRAM, found {entry:?}"))?;
                let voice = smf::KeyboardVoice {
                    channel: channel.trim().parse()?,
                    program: program.trim().parse()?,
                    ..smf::KeyboardVoice::default()
                };
                if index < chosen.len() {
                    chosen[index] = voice;
                } else {
                    chosen.push(voice);
                }
            }
            let parsed = chosen;
            for (index, voice) in parsed.iter().enumerate() {
                let name = project
                    .song
                    .tracks
                    .get(index)
                    .map_or("extra part", |track| track.name.as_str());
                eprintln!(
                    "  {name}: channel {} program {}",
                    voice.channel, voice.program
                );
            }
            let (bytes, report) = smf::export_keyboard(&project.song, &parsed, &[]);
            let audit = smf::keyboard::audit(&bytes)?;
            fs::write(&out, &bytes)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "wrote {}: {} track(s), {} note(s), {} chord marker(s), setup bar of {} ticks",
                out.display(),
                report.tracks,
                report.notes,
                report.chords,
                report.shift
            );
            println!(
                "audit: {} track(s), {} program(s), {} volume(s), {} pan(s), {} chord marker(s), {} lyric(s), {} drum note(s)",
                audit.tracks,
                audit.programs.len(),
                audit.volumes.len(),
                audit.pans.len(),
                audit.chord_markers,
                audit.lyrics,
                audit.drum_note_ons
            );
            println!(
                "audit banks: msb {:?}, lsb {:?}",
                audit.banks_msb, audit.banks_lsb
            );
            Ok(())
        }
        Commands::AddAudio {
            project,
            wav,
            out,
            start,
        } => {
            let project = read_project(&project)?;
            let bytes = fs::read(&wav)
                .map_err(|error| format!("could not read {}: {error}", wav.display()))?;
            let info = audio::wav::info(&bytes)?;
            let bpm = project.song.tempo_map.bpm_at(start);
            let length = audio::chords::to_ticks(info.seconds, bpm);
            let mut session = EditSession::new(project)?;
            let track_id = session.next_track_id()?;
            let mut track = Track::new(
                track_id,
                wav.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("Audio")
                    .to_string(),
            );
            track.audio = Some(project::song::AudioRef {
                path: wav.display().to_string(),
                start,
                length,
                offset: 0,

                fade_in: 0,
                fade_out: 0,

                stretch: false,
                gain_mb: 0,
            });
            session.apply(Command::InsertTrack { track })?;
            let json = session.project().to_json_pretty()?;
            fs::write(&out, json)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "added {}: {} channel(s), {} Hz, {:.2} s, {length} ticks at {bpm:.1} bpm -> {}",
                wav.display(),
                info.channels,
                info.sample_rate,
                info.seconds,
                out.display()
            );
            Ok(())
        }
        Commands::ChordsFromAudio { wav, bpm, out } => {
            let bytes = fs::read(&wav)
                .map_err(|error| format!("could not read {}: {error}", wav.display()))?;
            let decoded = audio::wav::decode_mono(&bytes)?;
            let bar_seconds = 4.0 * 60.0 / bpm.max(1.0);
            let chords =
                audio::chords::recognize(&decoded.samples, decoded.sample_rate, bar_seconds);
            for chord in &chords {
                println!(
                    "{:>7.2}s  {}  ({:.0}%)",
                    chord.start,
                    chord.chord.format(music_core::Spelling::Sharp),
                    chord.score * 100.0
                );
            }
            if let Some(path) = out {
                let mut song = Song::new(
                    wav.file_stem()
                        .and_then(|stem| stem.to_str())
                        .unwrap_or("Audio chords"),
                    bpm,
                );
                for chord in &chords {
                    song.chords.push(project::song::ChordEvent {
                        tick: audio::chords::to_ticks(chord.start, bpm),
                        chord: chord.chord,
                    });
                }
                song.chords.sort_by_key(|event| event.tick);
                let project = ProjectFile::new(song);
                let json = project.to_json_pretty()?;
                fs::write(&path, json)
                    .map_err(|error| format!("could not write {}: {error}", path.display()))?;
                println!("{} chord(s) -> {}", chords.len(), path.display());
            }
            Ok(())
        }
        Commands::AudioToChords { wav, bpm, segment } => {
            let events = audio_chords_json(&wav, bpm, segment)?;
            println!("{}", serde_json::to_string_pretty(&events)?);
            Ok(())
        }
        Commands::AudioToMidi {
            wav,
            out,
            bpm,
            drums,
        } => {
            let song = audio_midi_song(&wav, bpm, drums)?;
            let bytes = smf::encode(&song)?;
            fs::write(&out, &bytes)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            let notes = song.tracks.first().map_or(0, |track| track.notes.len());
            println!(
                "wrote {}: {} note(s), {} byte(s)",
                out.display(),
                notes,
                bytes.len()
            );
            Ok(())
        }
        Commands::ScorePdf {
            project,
            out,
            track,
            title,
            all,
            frames,
        } => {
            let project = read_project(&project)?;
            let settings = score::Settings::default();
            let score_title = title.unwrap_or_else(|| project.song.name.clone());
            if all {
                let tracks: Vec<project::TrackId> =
                    project.song.tracks.iter().map(|entry| entry.id).collect();
                let pdf = score::pdf::to_full_pdf_with_frames(
                    &project.song,
                    settings,
                    &score_title,
                    &tracks,
                    frames,
                );
                fs::write(&out, &pdf)
                    .map_err(|error| format!("could not write {}: {error}", out.display()))?;
                println!(
                    "wrote {}: full score, {} track(s), {} byte(s)",
                    out.display(),
                    tracks.len(),
                    pdf.len()
                );
                return Ok(());
            }
            let Some(entry) = project.song.tracks.get(track) else {
                return Err(format!("no track at index {track}").into());
            };
            let engraved = score::engrave(&project.song, entry.id, settings);
            let pdf = score::cecilia::track_pdf(&project.song, entry.id, &score_title);
            fs::write(&out, &pdf)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "wrote {}: {} page(s), {} system(s), {} note(s), {} byte(s)",
                out.display(),
                engraved.pages.len(),
                engraved.systems(),
                engraved.notes.len(),
                pdf.len()
            );
            Ok(())
        }
        Commands::StyleCatalog { root, limit } => {
            let entries = style_formats::catalog::catalog(&root)?;
            let shown = limit.unwrap_or(entries.len());
            for entry in entries.iter().take(shown) {
                println!("{:>6}  {}", entry.number, entry.name);
            }
            println!("{} style(s) under {}", entries.len(), root.display());
            Ok(())
        }
        Commands::StyleByNumber {
            number,
            root,
            section,
            out,
        } => {
            let entries = style_formats::catalog::catalog(&root)?;
            let entry = style_formats::catalog::by_number(&entries, number).ok_or_else(|| {
                format!(
                    "no style numbered {number}; the catalog has {}",
                    entries.len()
                )
            })?;
            println!(
                "{:>6}  {}  ({})",
                entry.number,
                entry.name,
                entry.path.display()
            );
            style_command(&entry.path, section.as_deref(), out.as_ref(), &[], false)
        }
        Commands::MusicXmlAudit { project } => {
            let project = read_project(&project)?;
            let xml = musicxml::export(&project.song, &project.song.name);
            let imported = musicxml::import::import(&xml)?;
            let delta = musicxml::audit::audit(&project.song, &imported);
            println!(
                "tracks {}->{}, notes {}->{}, chords {}->{}, lyrics {}->{}, marks {}->{}",
                delta.tracks.0,
                delta.tracks.1,
                delta.notes.0,
                delta.notes.1,
                delta.chords.0,
                delta.chords.1,
                delta.lyrics.0,
                delta.lyrics.1,
                delta.marks.0,
                delta.marks.1
            );
            for note in &delta.info {
                println!("  {note}");
            }
            println!(
                "pitched detail {}->{} untied note(s)",
                delta.detail.0.len(),
                delta.detail.1.len()
            );
            for change in &delta.changes {
                println!("  {change}");
            }
            if delta.is_clean() {
                println!("round trip preserves the audited fields");
            }
            Ok(())
        }
        Commands::Bounce {
            project,
            wav,
            out,
            name,
            sample_rate,
        } => {
            let project = read_project(&project)?;
            let mut session = EditSession::new(project)?;
            let bounce =
                app_commands::bounce::bounce_to_track(&mut session, &wav, &name, sample_rate)?;
            let json = session.project().to_json_pretty()?;
            fs::write(&out, json)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "bounced {} ticks to {} on track {} -> {}",
                bounce.length,
                bounce.path.display(),
                bounce.track,
                out.display()
            );
            Ok(())
        }
        Commands::JoinLoop {
            project,
            section,
            times,
            out,
        } => {
            let project = read_project(&project)?;
            let mut session = EditSession::new(project)?;
            let outcome = arranging::forms::join_loop(&mut session, &section, times)?;
            let json = session.project().to_json_pretty()?;
            fs::write(&out, json)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "{}: {} copy/copies of {} ticks, {} note(s), {} chord(s) added -> {}",
                outcome.section,
                outcome.copies,
                outcome.length,
                outcome.notes,
                outcome.chords,
                out.display()
            );
            Ok(())
        }
        Commands::FormTransition {
            project,
            tick,
            kind,
            apply,
            out,
        } => {
            use arranging::forms::{Transition, apply_break, break_range, transition};
            let project = read_project(&project)?;
            let kind = match kind.as_str() {
                "straight" => Transition::Straight,
                "fill" => Transition::DrumFill,
                "lead" => Transition::LeadIn,
                "automatic" => Transition::Automatic,
                "hold" => Transition::Hold,
                "break" => Transition::Break,
                other => return Err(format!("unknown transition {other}").into()),
            };
            let plan = transition(&project.song, tick, kind);
            println!("{}: {} added note(s)", plan.description, plan.notes.len());
            let mut session = EditSession::new(project)?;
            if kind == Transition::Break {
                let bar = session.song().time_signature.bar_ticks();
                let (from, to) = break_range(tick, bar);
                println!("break silences {from}..{to}");
                if apply {
                    let removed = apply_break(&mut session, from, to)?;
                    println!("removed {removed} note(s)");
                }
            } else if apply {
                let added = arranging::forms::apply_transition(&mut session, &plan)?;
                println!("added {added} note(s)");
            }
            if apply {
                let out = out.ok_or("--apply needs --out")?;
                let json = session.project().to_json_pretty()?;
                fs::write(&out, json)
                    .map_err(|error| format!("could not write {}: {error}", out.display()))?;
                println!("wrote {}", out.display());
            }
            Ok(())
        }
        Commands::Sf2Use {
            project,
            track,
            file,
            bank,
            program,
            out,
        } => {
            let bytes = fs::read(&file).map_err(|error| format!("{}: {error}", file.display()))?;
            let font = audio::soundfont::parse(&bytes)
                .map_err(|error| format!("{}: {error}", file.display()))?;
            println!("{}", audio::soundfont::describe(&font));
            let project = read_project(&project)?;
            let track = TrackId::new(track)?;
            let mut session = EditSession::new(project)?;
            session.apply(Command::SetSoundFont {
                track,
                soundfont: Some(project::song::SoundFontRef {
                    path: file.display().to_string(),
                    bank,
                    program,
                }),
            })?;
            let json = session.project().to_json_pretty()?;
            fs::write(&out, json)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!("track {track} -> SF2 {bank}:{program} {}", file.display());
            Ok(())
        }
        Commands::Sf2Render {
            file,
            note,
            bank,
            program,
            seconds,
            out,
        } => {
            let bytes = fs::read(&file).map_err(|error| format!("{}: {error}", file.display()))?;
            let font = audio::soundfont::parse(&bytes)
                .map_err(|error| format!("{}: {error}", file.display()))?;
            let key = note
                .parse::<u8>()
                .ok()
                .or_else(|| {
                    music_core::Pitch::from_name(&note)
                        .ok()
                        .map(|pitch| pitch.as_u8())
                })
                .ok_or_else(|| format!("unrecognised note {note:?}"))?;
            let velocity = 100_u8;
            let mono = audio::soundfont::render_note(
                &font,
                bank,
                program,
                key,
                velocity,
                seconds.clamp(0.05, 30.0),
                audio::SAMPLE_RATE,
            )
            .ok_or_else(|| format!("no zone in {bank}:{program} covers note {note}"))?;
            let mut samples = Vec::with_capacity(mono.len() * 2);
            for value in &mono {
                samples.push(*value);
                samples.push(*value);
            }
            let rendered = audio::RenderedAudio {
                samples,
                sample_rate: audio::SAMPLE_RATE,
            };
            fs::write(&out, rendered.to_wav())
                .map_err(|error| format!("{}: {error}", out.display()))?;
            println!(
                "rendered {} frame(s), {:.2} s from {} -> {}",
                mono.len(),
                rendered.duration_seconds(),
                audio::soundfont::describe(&font),
                out.display()
            );
            Ok(())
        }
        Commands::MidiChannelCheck {
            output,
            channel,
            note,
            seconds,
        } => {
            let messages = midi_devices::devices::channel_check(&output, channel, note, seconds)?;
            println!(
                "sent {:?} then {:?} on {}",
                messages[0], messages[1], output
            );
            Ok(())
        }
        Commands::RecordAudio {
            out,
            seconds,
            device,
            channel,
        } => {
            let rendered = audio::record::record_seconds_from_channel(
                device.as_deref(),
                channel,
                seconds.clamp(0.1, 3600.0),
            )?;
            fs::write(&out, rendered.to_wav())
                .map_err(|error| format!("{}: {error}", out.display()))?;
            println!(
                "recorded {:.2} s at {} Hz -> {}",
                rendered.duration_seconds(),
                rendered.sample_rate,
                out.display()
            );
            Ok(())
        }
        Commands::PlayAudio {
            project,
            device,
            seconds,
        } => play_audio(&project, device.as_deref(), seconds),
        Commands::AudioDevices => {
            println!("inputs:");
            for name in audio::record::input_devices() {
                println!("  {name}");
            }
            println!("outputs:");
            for name in audio::player::output_devices() {
                println!("  {name}");
            }
            Ok(())
        }
        Commands::PluginScan { dirs } => {
            let roots = if dirs.is_empty() {
                plugins::default_roots()
            } else {
                dirs.iter()
                    .flat_map(|dir| {
                        [
                            plugins::root(plugins::Format::Vst3, dir),
                            plugins::root(plugins::Format::AudioUnit, dir),
                        ]
                    })
                    .collect()
            };
            let found = plugins::scan_roots(&roots);
            for plugin in &found {
                println!(
                    "{:<4} {}  {}",
                    plugin.format.label(),
                    plugin.name,
                    plugin.path.display()
                );
            }
            println!("{} plug-in bundle(s)", found.len());
            Ok(())
        }
        Commands::Sf2Info { file } => {
            let bytes = fs::read(&file).map_err(|error| format!("{}: {error}", file.display()))?;
            let font = audio::soundfont::parse(&bytes)
                .map_err(|error| format!("{}: {error}", file.display()))?;
            println!("{}", audio::soundfont::describe(&font));
            for preset in font.presets.iter().take(20) {
                println!(
                    "  {:03}:{:03} {} ({} zone(s))",
                    preset.bank,
                    preset.program,
                    preset.name,
                    preset.zones.len()
                );
            }
            if font.presets.len() > 20 {
                println!("  ... {} more", font.presets.len() - 20);
            }
            Ok(())
        }
        Commands::KorgScan { file } => {
            let bytes = fs::read(&file).map_err(|error| format!("{}: {error}", file.display()))?;
            let scan = style_formats::korg::scan(&bytes);
            println!("{}", style_formats::korg::describe(&scan));
            Ok(())
        }
        Commands::ExportStems {
            project,
            out_dir,
            sample_rate,
        } => {
            let project = read_project(&project)?;
            let stems = audio::render_stems(&project.song, sample_rate);
            fs::create_dir_all(&out_dir)
                .map_err(|error| format!("{}: {error}", out_dir.display()))?;
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
                fs::write(out_dir.join(&file), stem.audio.to_wav())
                    .map_err(|error| format!("{}: {error}", out_dir.display()))?;
                manifest.push_str(&format!(
                    "{}\t{file}\t{:.2}s\n",
                    stem.name,
                    stem.audio.duration_seconds()
                ));
                println!("{} -> {file}", stem.name);
            }
            fs::write(out_dir.join("stems.txt"), manifest)
                .map_err(|error| format!("{}: {error}", out_dir.display()))?;
            println!("{} stem(s) -> {}", stems.len(), out_dir.display());
            Ok(())
        }
        Commands::TempoSet {
            project,
            tick,
            bpm,
            out,
        } => {
            let project = read_project(&project)?;
            let mut session = EditSession::new(project)?;
            session.apply(Command::SetTempo { tick, bpm })?;
            let json = session.project().to_json_pretty()?;
            fs::write(&out, json)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            for change in session.song().tempo_map.changes() {
                println!("{} bpm at tick {}", change.bpm, change.tick);
            }
            println!("wrote {}", out.display());
            Ok(())
        }
        Commands::AudioTags { audio } => {
            let decoded = audio::decode::decode_mono(&audio).or_else(|_| {
                let bytes =
                    fs::read(&audio).map_err(|error| format!("{}: {error}", audio.display()))?;
                audio::wav::decode_mono(&bytes)
            })?;
            match audio::analysis::tag_instrument(&decoded.samples, decoded.sample_rate) {
                Some(tag) => println!(
                    "{}: {} ({:.2})",
                    audio.display(),
                    tag.instrument.label(),
                    tag.confidence
                ),
                None => println!("{}: no pitched material found", audio.display()),
            }
            Ok(())
        }
        Commands::GrooveFromAudio {
            audio,
            dir,
            name,
            bpm,
            detect_tempo,
            grid,
        } => groove_from_audio(&audio, &dir, &name, bpm, detect_tempo, grid),
        Commands::GrooveLibrary {
            action,
            dir,
            project,
            track,
            name,
            out,
            timing,
            accents,
        } => match action.as_str() {
            "list" => {
                for (found, path) in arranging::groove_file::list(&dir)? {
                    println!("{found}  ({})", path.display());
                }
                Ok(())
            }
            "save" => {
                let project = read_project(project.as_ref().ok_or("save needs --project")?)?;
                let track = TrackId::new(track.ok_or("save needs --track")?)?;
                let name = name.ok_or("save needs --name")?;
                let entry = project
                    .song
                    .tracks
                    .iter()
                    .find(|entry| entry.id == track)
                    .ok_or("unknown track")?;
                let bar = project.song.time_signature.bar_ticks().max(1);
                let groove = arranging::groove::extract(&entry.notes, bar, music_core::PPQ / 4)
                    .ok_or("track has no notes to extract a groove from")?;
                let path = arranging::groove_file::save(&dir, &name, &groove)?;
                println!("saved {name} -> {}", path.display());
                Ok(())
            }
            "apply" => {
                let project = read_project(project.as_ref().ok_or("apply needs --project")?)?;
                let track = TrackId::new(track.ok_or("apply needs --track")?)?;
                let name = name.ok_or("apply needs --name")?;
                let out = out.ok_or("apply needs --out")?;
                let (_, path) = arranging::groove_file::list(&dir)?
                    .into_iter()
                    .find(|(found, _)| found == &name)
                    .ok_or_else(|| format!("no groove named {name:?} in {}", dir.display()))?;
                let (_, groove) = arranging::groove_file::load(&path)?;
                let mut session = EditSession::new(project)?;
                let notes = session
                    .song()
                    .tracks
                    .iter()
                    .find(|entry| entry.id == track)
                    .ok_or("unknown track")?
                    .notes
                    .clone();
                let applied = arranging::groove::apply(
                    &notes,
                    &groove,
                    arranging::groove::Amounts { timing, accents },
                );
                session.apply(Command::ReplaceTrackNotes {
                    track,
                    notes: applied,
                })?;
                let json = session.project().to_json_pretty()?;
                fs::write(&out, json)
                    .map_err(|error| format!("could not write {}: {error}", out.display()))?;
                println!("applied {name} to {}", out.display());
                Ok(())
            }
            other => Err(format!("unknown groove action {other}").into()),
        },
        Commands::FitProject {
            project,
            low,
            high,
            polyphony,
            no_drum_kit,
            apply,
            out,
        } => {
            let project = read_project(&project)?;
            let spec = arranging::SynthSpec {
                polyphony,
                low,
                high,
                has_drum_kit: !no_drum_kit,
            };
            let proposals = arranging::fit_to_synth(&project.song, spec);
            if proposals.is_empty() {
                println!(
                    "no fits needed for {}..{} with {polyphony} voice(s)",
                    spec.low, spec.high
                );
            }
            for proposal in &proposals {
                println!("{}: {}", proposal.title, proposal.detail);
            }
            if apply {
                let out = out.ok_or("--apply needs --out")?;
                let mut session = EditSession::new(project)?;
                let mut applied = 0;
                for proposal in &proposals {
                    if !proposal.is_empty() {
                        applied += arranging::apply(&mut session, proposal)?;
                    }
                }
                let json = session.project().to_json_pretty()?;
                fs::write(&out, json)
                    .map_err(|error| format!("could not write {}: {error}", out.display()))?;
                println!("applied {applied} edit(s) -> {}", out.display());
            }
            Ok(())
        }
        Commands::Voicing {
            chord,
            octave,
            out_dir,
        } => {
            let chord = Chord::parse(&chord)?;
            let voicings = music_core::voicing::inversions(chord, octave);
            for (index, voicing) in voicings.iter().enumerate() {
                println!(
                    "{}: {} -> {:?}",
                    if index == 0 { 'A' } else { 'B' },
                    voicing.label(),
                    voicing.midi()
                );
            }
            if let Some(directory) = out_dir {
                fs::create_dir_all(&directory)?;
                for (name, voicing) in [("a", 0_usize), ("b", 1)] {
                    let Some(voicing) = voicings.get(voicing) else {
                        continue;
                    };
                    let mut song = Song::new("Voicing", 120.0);
                    let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
                    for (index, pitch) in voicing.pitches.iter().enumerate() {
                        track.notes.push(project::song::Note {
                            id: project::NoteId::new(index as u32 + 1)?,
                            start: 0,
                            duration: 3840,
                            pitch: *pitch,
                            velocity: music_core::Velocity::new(90)?,
                            channel: music_core::Channel::first(),
                        });
                    }
                    song.tracks.push(track);
                    let rendered = audio::render(&song, audio::SAMPLE_RATE);
                    let path = directory.join(format!("{name}.wav"));
                    fs::write(&path, rendered.to_wav())?;
                    println!("wrote {} ({})", path.display(), voicing.label());
                }
            }
            Ok(())
        }
        Commands::SysexXgOn { out, device } => {
            let bytes = midi_devices::sysex::xg_on(device);
            fs::write(&out, &bytes)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "wrote {} byte(s) of XG System On for device {device} -> {}",
                bytes.len(),
                out.display()
            );
            Ok(())
        }
        Commands::MidiSysex {
            input,
            output,
            hex,
            timeout_ms,
        } => {
            let bytes = parse_hex_bytes(&hex)?;
            let replies = midi_devices::devices::sysex_exchange(
                &input,
                &output,
                &bytes,
                std::time::Duration::from_millis(timeout_ms.clamp(100, 10_000)),
                std::time::Duration::from_millis(250),
            )?;
            println!("{} SysEx reply packet(s)", replies.len());
            for (index, reply) in replies.iter().enumerate() {
                let text: Vec<String> = reply
                    .iter()
                    .take(32)
                    .map(|byte| format!("{byte:02X}"))
                    .collect();
                let suffix = if reply.len() > 32 { " ..." } else { "" };
                println!("  [{index}] ({} B) {}{suffix}", reply.len(), text.join(" "));
            }
            Ok(())
        }
        Commands::MidiIdentity {
            input,
            output,
            timeout_ms,
        } => {
            let replies = midi_devices::devices::sysex_exchange(
                &input,
                &output,
                &midi_devices::d05::IDENTITY_REQUEST,
                std::time::Duration::from_millis(timeout_ms.clamp(100, 10_000)),
                std::time::Duration::from_millis(200),
            )?;
            println!("{} SysEx reply packet(s)", replies.len());
            for (index, reply) in replies.iter().enumerate() {
                let hex: Vec<String> = reply.iter().map(|byte| format!("{byte:02X}")).collect();
                println!("  [{index}] ({} B) {}", reply.len(), hex.join(" "));
                if let Some(id) = midi_devices::d05::IdentityReply::parse(reply) {
                    let mfr: Vec<String> =
                        id.manufacturer.iter().map(|b| format!("{b:02X}")).collect();
                    println!(
                        "      Identity: dev=0x{:02X} mfr={} family={:02X}{:02X} member={:02X}{:02X} rev={:?}",
                        id.device,
                        mfr.join(" "),
                        id.family[0],
                        id.family[1],
                        id.member[0],
                        id.member[1],
                        id.revision_text()
                    );
                }
            }
            Ok(())
        }
        Commands::MidiMonitorRaw { input, seconds } => {
            let packets = midi_devices::devices::monitor_raw(&input, seconds)?;
            println!("{} raw packet(s) from {input}", packets.len());
            for (timestamp, bytes) in packets.iter().take(64) {
                let hex: Vec<String> = bytes
                    .iter()
                    .take(32)
                    .map(|byte| format!("{byte:02X}"))
                    .collect();
                let suffix = if bytes.len() > 32 { " ..." } else { "" };
                println!(
                    "  {timestamp:>10} us ({:>3} B): {}{suffix}",
                    bytes.len(),
                    hex.join(" ")
                );
            }
            Ok(())
        }
        Commands::D05Read {
            input,
            output,
            device,
            save,
        } => {
            let requests = midi_devices::d05::TempPatch::request_messages(device)?;
            let mut patch = midi_devices::d05::TempPatch {
                bytes: [0; midi_devices::d05::TEMP_PATCH_BYTES],
            };
            let mut raw = Vec::new();
            let mut written = 0_usize;
            for request in &requests {
                let replies = midi_devices::devices::sysex_exchange(
                    &input,
                    &output,
                    request,
                    std::time::Duration::from_millis(1500),
                    std::time::Duration::from_millis(250),
                )?;
                for reply in replies {
                    raw.extend_from_slice(&reply);
                    if let Some(dt1) = midi_devices::d05::parse_dt1(&reply) {
                        written += patch.apply_dt1(&dt1);
                    }
                }
            }
            if written == 0 {
                return Err(format!(
                    "no D-05 DT1 reply received on {input} (device {device}); raw bytes: {}",
                    raw.len()
                )
                .into());
            }
            if let Some(path) = save {
                fs::write(&path, &raw)
                    .map_err(|error| format!("could not write {}: {error}", path.display()))?;
                println!("saved {} raw byte(s) -> {}", raw.len(), path.display());
            }
            let key_mode = usize::from(patch.patch_common(0x12));
            let key_mode_name = midi_devices::d05::KEY_MODES
                .get(key_mode)
                .copied()
                .unwrap_or("Unknown");
            println!("D-05 temp patch ({written} B decoded):");
            println!("  Patch name : {:?}", patch.patch_name());
            println!("  Upper tone : {:?}", patch.upper_tone_name());
            println!("  Lower tone : {:?}", patch.lower_tone_name());
            println!(
                "  Key mode   : {key_mode_name} ({key_mode}), split {}",
                patch.patch_common(0x13)
            );
            println!(
                "  Reverb     : type {}, balance {}",
                patch.patch_common(0x1e) + 1,
                patch.patch_common(0x1f)
            );
            println!(
                "  Volume     : {}, tone balance {}",
                patch.patch_common(0x20),
                patch.patch_common(0x21)
            );
            Ok(())
        }
        Commands::D05Names {
            input,
            output,
            device,
            channel,
            bank,
            count,
        } => {
            let bank = bank.min(13);
            let ch = channel.clamp(1, 16) - 1;
            let names =
                midi_devices::d05::read_bank_names(&input, &output, device, ch, bank, count)?;
            let label = midi_devices::d05::BANK_NAMES
                .get(usize::from(bank))
                .copied()
                .unwrap_or("Bank");
            println!("{label} ({} patch name(s)):", names.len());
            for (index, name) in names.iter().enumerate() {
                let group = index / 8 + 1;
                let slot = index % 8 + 1;
                println!("  {group}-{slot}: {name}");
            }
            Ok(())
        }
        Commands::L6maxTransport { output, action } => {
            let byte = match action.to_ascii_lowercase().as_str() {
                "start" => 0xfa,
                "continue" => 0xfb,
                "stop" => 0xfc,
                other => {
                    return Err(format!(
                        "unknown transport action {other:?}; use start, continue, or stop"
                    )
                    .into());
                }
            };
            midi_devices::devices::send_raw(&output, &[byte])?;
            println!("sent 0x{byte:02X} ({action}) to {output}");
            Ok(())
        }
        Commands::DeviceProfile { file, track } => {
            let profile = midi_devices::profiles::load_file(&file)?;
            println!(
                "{} ({}): {} voice(s), {} bank(s), {} drum kit(s)",
                profile.name,
                profile.id,
                profile.voices.len(),
                profile.banks.len(),
                profile.drum_kits.len()
            );
            if let Some(track) = track {
                println!(
                    "{track} -> program {}",
                    midi_devices::profiles::nearest_in_profile(&profile, &track)
                );
            }
            Ok(())
        }
        Commands::PlaylistFromCsv { csv, folder, out } => {
            let text = fs::read_to_string(&csv)
                .map_err(|error| format!("could not read {}: {error}", csv.display()))?;
            let book = library::Book::from_csv(&text)?;
            let list = library::List::from_book(&book, &folder);
            fs::write(&out, list.to_tsv())
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "wrote {} playlist item(s) -> {}",
                list.items.len(),
                out.display()
            );
            Ok(())
        }
        Commands::LibraryImport {
            project,
            root,
            number,
            name,
            prefix,
            out,
        } => {
            let project = read_project(&project)?;
            let entries = style_formats::catalog::catalog(&root)?;
            let entry = style_formats::catalog::resolve(&entries, number, &name)
                .ok_or_else(|| format!("no style matched in {}", root.display()))?;
            let mut session = EditSession::new(project)?;
            let imported =
                app_commands::style_import::import_file(&mut session, &entry.path, &prefix)?;
            let json = session.project().to_json_pretty()?;
            fs::write(&out, json)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "imported {} track(s), {} note(s) from {} -> {}",
                imported.tracks,
                imported.notes,
                entry.path.display(),
                out.display()
            );
            Ok(())
        }
        Commands::StyleResolve { root, number, name } => {
            let entries = style_formats::catalog::catalog(&root)?;
            match style_formats::catalog::resolve(&entries, number, &name) {
                Some(entry) => {
                    println!(
                        "{:>6}  {}  ({})",
                        entry.number,
                        entry.name,
                        entry.path.display()
                    );
                    Ok(())
                }
                None => Err(format!(
                    "no style matched number {number:?} or name {name:?} in {} entries",
                    entries.len()
                )
                .into()),
            }
        }
        Commands::L6maxImport {
            folder,
            project,
            out,
            start,
        } => {
            let project = read_project(&project)?;
            let plan = audio::l6max::scan(&folder)?;
            if plan.takes.is_empty() {
                return Err(format!("no readable takes in {}", folder.display()).into());
            }
            let mut session = EditSession::new(project)?;
            let bpm = session.song().tempo_map.bpm_at(start);
            let mut imported = 0_usize;
            for take in &plan.takes {
                let track_id = session.next_track_id()?;
                let mut track = Track::new(track_id, take.name.clone());
                track.audio = Some(project::song::AudioRef {
                    path: take.path.display().to_string(),
                    start,
                    length: audio::chords::to_ticks(take.seconds, bpm),
                    offset: 0,

                    fade_in: 0,
                    fade_out: 0,

                    stretch: false,
                    gain_mb: 0,
                });
                session.apply(Command::InsertTrack { track })?;
                imported += 1;
            }
            let json = session.project().to_json_pretty()?;
            fs::write(&out, json)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "imported {imported} take(s), {:.2} s total, {} 32-bit float -> {}",
                plan.seconds(),
                plan.float32(),
                out.display()
            );
            for (path, reason) in &plan.problems {
                eprintln!("skipped {}: {reason}", path.display());
            }
            Ok(())
        }
        Commands::Generate {
            project,
            track,
            op,
            out,
            timing,
            velocity,
            seed,
            bars,
        } => {
            let project = read_project(&project)?;
            let mut session = EditSession::new(project)?;
            let track = TrackId::new(track)?;
            let song = session.song().clone();
            let count = match op.as_str() {
                "humanize" => {
                    arranging::generate::humanize(&mut session, track, timing, velocity, seed)?
                }
                "drumify" => arranging::generate::drumify(&mut session, track, &song)?,
                "bass" => arranging::generate::write_part(
                    &mut session,
                    track,
                    arranging::generate::Part::Bass,
                    &song,
                )?,
                "pad" => arranging::generate::write_part(
                    &mut session,
                    track,
                    arranging::generate::Part::Pad,
                    &song,
                )?,
                "continue" => arranging::generate::continue_song(&mut session, track, bars)?,
                "melody" => arranging::generate::melody(&mut session, track, &song, seed)?,
                "arp" => arranging::generate::write_part(
                    &mut session,
                    track,
                    arranging::generate::Part::Arpeggio,
                    &song,
                )?,
                other => return Err(format!("unknown op {other}").into()),
            };
            let json = session.project().to_json_pretty()?;
            fs::write(&out, json)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!("{op}: {count} note(s) -> {}", out.display());
            Ok(())
        }
        Commands::DrumSlice { wav, out } => {
            let bytes = fs::read(&wav)
                .map_err(|error| format!("could not read {}: {error}", wav.display()))?;
            let decoded = audio::wav::decode_mono(&bytes)?;
            let cuts = audio::drums::slices(&decoded.samples, decoded.sample_rate);
            let kit = audio::drums::kit(&cuts);
            if kit.pads.is_empty() {
                return Err(format!("no slices found in {}", wav.display()).into());
            }
            fs::create_dir_all(&out)
                .map_err(|error| format!("could not create {}: {error}", out.display()))?;
            for pad in &kit.pads {
                let rendered = audio::drums::audition(&decoded.samples, *pad, decoded.sample_rate);
                let path = out.join(format!("pad-{}.wav", pad.note));
                fs::write(&path, rendered.to_wav())
                    .map_err(|error| format!("could not write {}: {error}", path.display()))?;
            }
            println!(
                "{} slice(s) from {} -> {} pad file(s) in {}",
                cuts.len(),
                wav.display(),
                kit.pads.len(),
                out.display()
            );
            Ok(())
        }
        Commands::MidiMapShow { file } => {
            // A missing file is an empty map; any other read error is real.
            let text = match fs::read_to_string(&file) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(error) => return Err(format!("{}: {error}", file.display()).into()),
            };
            let map = midi_devices::map::MidiMap::from_text(&text)?;
            if map.bindings.is_empty() {
                println!("no bindings in {}", file.display());
            }
            for binding in &map.bindings {
                println!("{} -> {:?}", binding.control_label(), binding.action);
            }
            Ok(())
        }
        Commands::MidiMapBind {
            file,
            control,
            action,
        } => {
            // Only a missing file starts an empty map; any other read error
            // must not wipe the existing bindings.
            let text = match fs::read_to_string(&file) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(error) => {
                    return Err(format!("could not read {}: {error}", file.display()).into());
                }
            };
            let mut map = midi_devices::map::MidiMap::from_text(&text)?;
            map.bind_text(&control, &action)?;
            fs::write(&file, map.to_text())
                .map_err(|error| format!("could not write {}: {error}", file.display()))?;
            println!("bound {control} to {action} in {}", file.display());
            Ok(())
        }
        Commands::TheoryScale { tonic, mode } => {
            let tonic = parse_tonic(&tonic)?;
            let mode = music_core::theory::Mode::parse(&mode)?;
            let notes: Vec<&str> = music_core::theory::scale(tonic, mode)
                .iter()
                .map(|pitch| pitch.name_sharp())
                .collect();
            println!(
                "{} {}: {}",
                tonic.name_sharp(),
                mode.name(),
                notes.join(" ")
            );
            Ok(())
        }
        Commands::TheoryAnalyze { chords, key } => {
            let parsed: Vec<music_core::Chord> = chords
                .iter()
                .map(|symbol| music_core::Chord::parse(symbol))
                .collect::<Result<_, _>>()?;
            let tonic = match key {
                Some(name) => parse_tonic(&name)?,
                None => parsed
                    .first()
                    .map(|chord| chord.root)
                    .ok_or("at least one chord is required")?,
            };
            let labels: Vec<String> = parsed
                .iter()
                .map(|chord| music_core::theory::roman_label(*chord, tonic))
                .collect();
            println!("{} in {}", labels.join(" - "), tonic.name_sharp());
            println!("cadence: {:?}", music_core::theory::cadence(&parsed, tonic));
            Ok(())
        }
        Commands::TheoryVoiceLeading { from, to } => {
            let from = music_core::Chord::parse(&from)?;
            let to = music_core::Chord::parse(&to)?;
            println!(
                "{} -> {}: cost {}",
                from.format(music_core::Spelling::Sharp),
                to.format(music_core::Spelling::Sharp),
                music_core::theory::voice_leading_cost(from, to)
            );
            Ok(())
        }
        Commands::DemoProject { out } => demo_project(&out),
        Commands::MidiImport { midi, out } => midi_import(&midi, &out),
        Commands::MidiPanic { output } => {
            let sent = midi_devices::devices::panic(&output)?;
            println!("sent {sent} all-notes-off message(s) to {output}");
            Ok(())
        }
        Commands::WavGain { wav, out, db } => wav_edit(&wav, &out, Some(db), 0.0),
        Commands::WavNormalize { wav, out, peak } => wav_edit(&wav, &out, None, peak),
        Commands::WavTrim {
            wav,
            out,
            start,
            end,
        } => {
            let bytes = fs::read(&wav)?;
            let mut audio = audio::wav::decode_stereo(&bytes)?;
            audio::edit::trim(&mut audio, start, end);
            fs::write(&out, audio.to_wav32f())?;
            println!("wrote {}", out.display());
            Ok(())
        }
        Commands::WavFade {
            wav,
            out,
            fade_in,
            fade_out,
        } => {
            let bytes = fs::read(&wav)?;
            let mut audio = audio::wav::decode_stereo(&bytes)?;
            audio::edit::fade(&mut audio, fade_in, fade_out);
            fs::write(&out, audio.to_wav32f())?;
            println!("wrote {}", out.display());
            Ok(())
        }
        Commands::WavReverse { wav, out } => {
            let bytes = fs::read(&wav)?;
            let mut audio = audio::wav::decode_stereo(&bytes)?;
            audio::edit::reverse(&mut audio);
            fs::write(&out, audio.to_wav32f())?;
            println!("wrote {}", out.display());
            Ok(())
        }
        Commands::ScoreLayout { project, track } => {
            let project = read_project(&project)?;
            let Some(entry) = project.song.tracks.get(track) else {
                return Err(format!("no track at index {track}").into());
            };
            let engraved = score::engrave(&project.song, entry.id, score::Settings::default());
            println!(
                "{}: {} page(s), {} system(s), {} measure(s), {} note(s)",
                entry.name,
                engraved.pages.len(),
                engraved.systems(),
                engraved.measures,
                engraved.notes.len()
            );
            for (index, page) in engraved.pages.iter().enumerate() {
                println!(
                    "  page {}: {} system(s), {} measure(s)",
                    index + 1,
                    page.systems.len(),
                    page.systems
                        .iter()
                        .map(|system| system.measures.len())
                        .sum::<usize>()
                );
            }
            Ok(())
        }
        Commands::ExportMusicXml {
            project,
            out,
            title,
        } => {
            let project = read_project(&project)?;
            let title = title.unwrap_or_else(|| project.song.name.clone());
            let xml = musicxml::export(&project.song, &title);
            fs::write(&out, &xml)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "wrote {}: {} track(s), {} measure(s), {} byte(s)",
                out.display(),
                project.song.tracks.len(),
                project.song.tracks.first().map_or(0, |_track| {
                    let bar = project.song.time_signature.bar_ticks().max(1);
                    ((project.song.end_tick().max(bar) + bar - 1) / bar) as usize
                }),
                xml.len()
            );
            Ok(())
        }
        Commands::ImportMusicXml {
            musicxml: source,
            out,
        } => {
            let xml = fs::read_to_string(&source)
                .map_err(|error| format!("could not read {}: {error}", source.display()))?;
            let song = musicxml::import::import(&xml)?;
            let notes: usize = song.tracks.iter().map(|track| track.notes.len()).sum();
            let project = ProjectFile::new(song);
            let json = project.to_json_pretty()?;
            fs::write(&out, json)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!(
                "imported {}: {} track(s), {notes} note(s) -> {}",
                source.display(),
                project.song.tracks.len(),
                out.display()
            );
            Ok(())
        }
        Commands::Recent {
            file,
            record,
            clear,
        } => {
            if clear {
                project::recent::clear(&file)?;
            }
            if let Some(project) = record {
                project::recent::record(&file, &project)?;
            }
            let entries = project::recent::read(&file);
            if entries.is_empty() {
                println!("no recent projects");
            }
            for (index, entry) in entries.iter().enumerate() {
                println!("{:>2}. {}", index + 1, entry.display());
            }
            Ok(())
        }
        Commands::MusicFinderImport { mfd, out } => {
            let bytes = fs::read(&mfd)
                .map_err(|error| format!("could not read {}: {error}", mfd.display()))?;
            let book = library::Book::from_mfd(&bytes)?;
            let text = if out.extension().and_then(|ext| ext.to_str()) == Some("csv") {
                book.to_csv()
            } else {
                book.to_json()
            };
            fs::write(&out, text)
                .map_err(|error| format!("could not write {}: {error}", out.display()))?;
            println!("{} entries -> {}", book.entries.len(), out.display());
            Ok(())
        }
        Commands::McpTools {} => {
            println!("{}", serde_json::to_string_pretty(&mcp::tool_list_json())?);
            Ok(())
        }
        Commands::McpServe { project } => mcp_serve(&project),
        Commands::McpSocket {
            project,
            port,
            token_file,
        } => mcp_socket(&project, port, &token_file),
        Commands::McpCall {
            project,
            tool,
            args,
            out,
        } => mcp_call(&project, &tool, &args, out.as_ref()),
        Commands::StyleSave {
            style,
            out,
            version,
        } => style_save(&style, &out, &version),
        Commands::StyleFromAudio {
            audio,
            out,
            bpm,
            section,
            chord,
            split_parts,
            section_bars,
            drums,
            detect_tempo,
            version,
        } => style_from_audio(
            &audio,
            &out,
            bpm,
            &section,
            &chord,
            split_parts,
            section_bars,
            drums,
            detect_tempo,
            &version,
        ),
        Commands::StyleFromClips {
            project,
            out,
            section,
            chord,
            length,
            from_markers,
            version,
        } => style_from_clips(
            &project,
            &out,
            &section,
            &chord,
            length,
            from_markers,
            &version,
        ),
        Commands::LinkToMidi {
            link,
            out,
            folder,
            ffmpeg,
            keep,
        } => link_to_midi(&link, &out, folder.as_deref(), ffmpeg.as_deref(), keep),
        Commands::ExportAudio {
            project,
            out,
            sample_rate,
            bits,
            metronome,
            gain_db,
            no_limiter,
            reverb,
            chorus,
        } => export_audio(
            &project,
            &out,
            sample_rate,
            bits,
            metronome,
            audio::master::Master {
                gain_db,
                limiter: !no_limiter,
                reverb: reverb.clamp(0.0, 1.0),
                chorus: chorus.clamp(0.0, 1.0),
            },
        ),
        Commands::Perform {
            style,
            out,
            section,
            chords,
            tempo,
        } => perform(&style, &out, section.as_deref(), &chords, tempo),
        Commands::Regenerate {
            project,
            style,
            out,
            policy,
        } => regenerate_project(&project, &style, &out, &policy),
        Commands::Style {
            style: style_path,
            section,
            out,
            chords,
            rules,
        } => style_command(
            &style_path,
            section.as_deref(),
            out.as_ref(),
            &chords,
            rules,
        ),
        Commands::Play {
            project,
            out_port,
            sample_rate,
            dry_run,
        } => play(&project, &out_port, sample_rate, dry_run),
        Commands::Record {
            input,
            out,
            seconds,
            tempo,
            wait,
        } => record(&input, &out, seconds, tempo, wait),
        Commands::MidiCc {
            output,
            channel,
            controller,
            value,
        } => midi_cc(&output, channel, controller, value),
        Commands::MidiMonitor { input, seconds } => midi_monitor(&input, seconds),
        Commands::Echo {
            input,
            out,
            seconds,
        } => echo(&input, &out, seconds),
    }
}

fn ports() -> Result<(), Box<dyn Error>> {
    let names = list_ports()?;
    print_ports("inputs", &names.inputs);
    print_ports("outputs", &names.outputs);
    Ok(())
}

fn print_ports(kind: &str, names: &[String]) {
    if names.is_empty() {
        println!("{kind}: none");
        return;
    }
    println!("{kind}:");
    for name in names {
        println!("  {name}");
    }
}

fn read_project(path: &PathBuf) -> Result<ProjectFile, Box<dyn Error>> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    ProjectFile::from_json(&text).map_err(Into::into)
}

fn write_midi(path: &PathBuf, song: &Song) -> Result<(), Box<dyn Error>> {
    let bytes = smf::encode(song)?;
    fs::write(path, &bytes)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    println!(
        "wrote {} ({} bytes, {} track(s), {} note(s))",
        path.display(),
        bytes.len(),
        song.tracks.len(),
        song.tracks
            .iter()
            .map(|track| track.notes.len())
            .sum::<usize>()
    );
    Ok(())
}

fn demo(
    out: &PathBuf,
    sample_rate: u32,
    block: usize,
    verbose: bool,
) -> Result<(), Box<dyn Error>> {
    if block == 0 {
        return Err("block size must be at least 1".into());
    }
    let song = demo_song()?;
    let summary = run_transport(&song, sample_rate, block, verbose)?;
    println!(
        "transport: {} event(s) over {} block(s) at {} Hz, {} frames per block",
        summary.events, summary.blocks, sample_rate, block
    );
    if summary.overflowed > 0 {
        return Err(format!(
            "{} event(s) were dropped because the event buffer was full",
            summary.overflowed
        )
        .into());
    }
    write_midi(out, &song)?;
    Ok(())
}

fn render(
    project_path: &PathBuf,
    out: &PathBuf,
    sample_rate: u32,
    block: usize,
    verbose: bool,
) -> Result<(), Box<dyn Error>> {
    if block == 0 {
        return Err("block size must be at least 1".into());
    }
    let project = read_project(project_path)?;
    let summary = run_transport(&project.song, sample_rate, block, verbose)?;
    println!(
        "transport: {} event(s) over {} block(s) at {} Hz, {} frames per block",
        summary.events, summary.blocks, sample_rate, block
    );
    if summary.overflowed > 0 {
        return Err(format!(
            "{} event(s) were dropped because the event buffer was full",
            summary.overflowed
        )
        .into());
    }
    write_midi(out, &project.song)?;
    Ok(())
}

/// Aggregate of a transport run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TransportSummary {
    /// Events emitted by the scheduler.
    pub events: usize,
    /// Blocks processed.
    pub blocks: usize,
    /// Events dropped because the buffer was full.
    pub overflowed: usize,
}

/// Runs the scheduler to the end of the pattern and returns what it produced.
///
/// `verbose` prints the first few events with absolute samples and block
/// offsets, which is the fastest way to see the transport is sample-addressed.
pub fn run_transport(
    song: &Song,
    sample_rate: u32,
    block: usize,
    verbose: bool,
) -> Result<TransportSummary, Box<dyn Error>> {
    if block == 0 {
        return Err("block size must be at least 1".into());
    }
    let prepared = PreparedPattern::prepare(song, sample_rate)?;
    let end_sample = prepared.end_sample();
    let mut scheduler = Scheduler::new(prepared);
    scheduler.play();
    let mut buffer = EventBuffer::with_capacity(1024);
    let mut summary = TransportSummary::default();
    let max_blocks = usize::try_from(end_sample / block as i64 + 2).unwrap_or(usize::MAX);
    let mut printed = 0;
    for _ in 0..max_blocks {
        let report = scheduler.process_block(block, &mut buffer);
        summary.blocks += 1;
        summary.events += report.emitted;
        summary.overflowed += report.overflowed;
        if verbose {
            for event in buffer.events() {
                if printed >= 8 {
                    break;
                }
                printed += 1;
                println!(
                    "  sample {:>8} tick {:>7} offset {:>4} ch {} {}",
                    event.sample,
                    event.tick,
                    event.offset_in(report.block_start, block),
                    event.message.channel(),
                    message_label(event.message)
                );
            }
        }
        if report.finished && report.emitted == 0 {
            break;
        }
    }
    Ok(summary)
}

/// The built-in four-bar phrase: C - Am - F - G at 120 bpm in 4/4.
///
/// Piano holds each chord for a bar; bass plays root and fifth on beats 1 and 3.
pub fn demo_song() -> Result<Song, Box<dyn Error>> {
    let mut song = Song::new("Four bars", 120.0);
    let bar = song.time_signature.bar_ticks();
    let chords = ["C", "Am", "F", "G"];

    let mut piano = Track::new(TrackId::new(1)?, "Piano");
    let mut bass = Track::new(TrackId::new(2)?, "Bass");
    let mut next_id = 0u32;
    for (bar_index, symbol) in chords.iter().enumerate() {
        let chord = Chord::parse(symbol)?;
        let start = bar_index as Tick * bar;
        for pitch in chord.tones(4) {
            next_id += 1;
            piano.notes.push(Note {
                id: NoteId::new(next_id)?,
                start,
                duration: bar,
                pitch,
                velocity: Velocity::new(80)?,
                channel: Channel::first(),
            });
        }
        let root = chord
            .bass_tone(2)
            .ok_or("chord root below the MIDI range")?;
        for beat in [0, 2] {
            next_id += 1;
            bass.notes.push(Note {
                id: NoteId::new(next_id)?,
                start: start + Tick::from(beat) * PPQ,
                duration: PPQ,
                pitch: root,
                velocity: Velocity::new(100)?,
                channel: Channel::new(1)?,
            });
        }
    }
    song.tracks.push(piano);
    song.tracks.push(bass);
    song.chords = chords
        .iter()
        .enumerate()
        .map(|(index, symbol)| {
            Ok(project::ChordEvent {
                tick: index as Tick * bar,
                chord: Chord::parse(symbol)?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    song.markers.push(project::Marker {
        tick: 0,
        name: String::from("Four bars"),
    });
    Ok(song)
}

/// Writes the demo project as JSON.
fn demo_project(out: &PathBuf) -> Result<(), Box<dyn Error>> {
    let project = ProjectFile::new(arranging::demo::demo_song());
    fs::write(out, project.to_json_pretty()?)?;
    println!("wrote {} with the built-in sixteen-bar demo", out.display());
    Ok(())
}

fn wav_edit(
    input: &PathBuf,
    out: &PathBuf,
    gain_db: Option<f32>,
    peak: f32,
) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(input)?;
    let mut audio = audio::wav::decode_stereo(&bytes)?;
    let factor = match gain_db {
        Some(db) => audio::edit::gain(&mut audio, db),
        None => audio::edit::normalize_peak(&mut audio, peak),
    };
    fs::write(out, audio.to_wav32f())?;
    println!(
        "wrote {} (factor {factor:.4}, {:.2} s)",
        out.display(),
        audio.duration_seconds()
    );
    Ok(())
}

fn midi_import(midi: &PathBuf, out: &PathBuf) -> Result<(), Box<dyn Error>> {
    let bytes = fs::read(midi)?;
    let (song, report) = smf::decode(&bytes).map_err(|error| error.to_string())?;
    let project = ProjectFile::new(song);
    fs::write(out, project.to_json_pretty()?)?;
    println!(
        "imported {} track(s), {} note(s) from {} -> {}",
        report.source_tracks,
        report.notes,
        midi.display(),
        out.display()
    );
    for warning in &report.warnings {
        println!("warning: {warning}");
    }
    Ok(())
}

fn inspect(path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let project = read_project(path)?;
    let song = &project.song;
    println!(
        "{} (schema {}, {} track(s))",
        if song.name.is_empty() {
            "<unnamed>"
        } else {
            song.name.as_str()
        },
        project.version,
        song.tracks.len()
    );
    println!(
        "tempo: {} change(s), {} bpm at tick 0, {} bpm at the end",
        song.tempo_map.changes().len(),
        song.tempo_map.bpm_at(0),
        song.tempo_map.bpm_at(song.end_tick())
    );
    println!(
        "meter: {}/{}",
        song.time_signature.numerator, song.time_signature.denominator
    );
    println!(
        "length: {} ticks ({} quarters)",
        song.end_tick(),
        music_core::time::ticks_to_quarters(song.end_tick())
    );
    for track in &song.tracks {
        let channels: Vec<u8> = {
            let mut seen: Vec<u8> = track
                .notes
                .iter()
                .map(|note| note.channel.one_based())
                .collect();
            seen.sort_unstable();
            seen.dedup();
            seen
        };
        println!(
            "  track {:<10} {:>4} note(s) channel(s) {:?}",
            track.id.to_string(),
            track.notes.len(),
            channels
        );
        if !track.name.is_empty() {
            println!("    name: {}", track.name);
        }
    }
    if !song.chords.is_empty() {
        println!("chords:");
        for event in &song.chords {
            println!("  tick {:>7} {}", event.tick, event.chord);
        }
    }
    if !song.markers.is_empty() {
        println!("markers:");
        for marker in &song.markers {
            println!("  tick {:>7} {}", marker.tick, marker.name);
        }
    }
    let issues = project.validate();
    if issues.is_empty() {
        println!("validation: clean");
    } else {
        println!("validation: {} finding(s)", issues.len());
        for issue in issues {
            println!("  {issue}");
        }
    }
    Ok(())
}

fn import(input: &PathBuf, out: &PathBuf, print_report: bool) -> Result<(), Box<dyn Error>> {
    let text = fs::read_to_string(input)
        .map_err(|error| format!("could not read {}: {error}", input.display()))?;
    let mut imported = project::legacy::import_cpp_project(&text)?;
    if let Some(dir) = input.parent() {
        let dir = if dir.as_os_str().is_empty() {
            std::env::current_dir()?
        } else {
            dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf())
        };
        project::legacy::resolve_audio_paths(&mut imported.project.song, &dir);
    }
    let json = imported.project.to_json_pretty()?;
    fs::write(out, json).map_err(|error| format!("could not write {}: {error}", out.display()))?;
    println!("wrote {}", out.display());
    if print_report || !imported.report.warnings.is_empty() {
        print!("{}", imported.report.to_text());
    } else {
        println!(
            "imported {} track(s), {} note(s)",
            imported.report.tracks, imported.report.notes
        );
    }
    Ok(())
}

fn edit(
    project_path: &PathBuf,
    out: &PathBuf,
    tempo: Option<f64>,
    chords: &[String],
    add_tracks: &[String],
) -> Result<(), Box<dyn Error>> {
    let project = read_project(project_path)?;
    let mut session = EditSession::new(project)?;
    for name in add_tracks {
        session.add_track(name.clone())?;
    }
    if let Some(bpm) = tempo {
        session.apply(Command::SetTempo { tick: 0, bpm })?;
    }
    for entry in chords {
        let (tick, symbol) = entry
            .split_once('=')
            .ok_or_else(|| format!("--chord expects TICK=SYMBOL, found {entry:?}"))?;
        let tick: Tick = tick
            .trim()
            .parse()
            .map_err(|_| format!("--chord tick {tick:?} is not a whole number"))?;
        let chord = Chord::parse(symbol)?;
        session.apply(Command::SetChord { tick, chord })?;
    }
    let json = session.project().to_json_pretty()?;
    fs::write(out, json).map_err(|error| format!("could not write {}: {error}", out.display()))?;
    println!(
        "wrote {} ({} undo step(s) available)",
        out.display(),
        session.undo_depth()
    );
    Ok(())
}

/// Parses `--sample-rate`: 8 kHz to 384 kHz.
fn parse_sample_rate(text: &str) -> Result<u32, String> {
    let rate: u32 = text
        .parse()
        .map_err(|_| format!("{text} is not a whole number"))?;
    if (8_000..=384_000).contains(&rate) {
        Ok(rate)
    } else {
        Err(String::from("sample rate must be 8000..=384000"))
    }
}

/// Parses `--block`: 1 to 65536 frames.
fn parse_block(text: &str) -> Result<usize, String> {
    let block: usize = text
        .parse()
        .map_err(|_| format!("{text} is not a whole number"))?;
    if (1..=65_536).contains(&block) {
        Ok(block)
    } else {
        Err(String::from("block must be 1..=65536"))
    }
}

fn mcp_serve(project_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let project = read_project(project_path)?;
    let mut session = mcp::session(project)?;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        if std::io::BufRead::read_line(&mut stdin.lock(), &mut line)? == 0 {
            return Ok(());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let message: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(message) => message,
            Err(error) => {
                let reply = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": serde_json::Value::Null,
                    "error": {"code": -32700, "message": error.to_string()},
                });
                writeln!(stdout, "{reply}")?;
                stdout.flush()?;
                continue;
            }
        };
        let reply = mcp::handle_message_saving(&mut session, &message, |session| {
            let json = session
                .project()
                .to_json_pretty()
                .map_err(|error| error.to_string())?;
            write_atomically(project_path, &json)
                .map_err(|error| format!("{}: {error}", project_path.display()))
        });
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
    }
}

fn write_private_file(path: &PathBuf, contents: &str) -> Result<(), Box<dyn Error>> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(contents.as_bytes())?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, contents)?;
        Ok(())
    }
}

fn mcp_socket(
    project_path: &PathBuf,
    port: u16,
    token_file: &PathBuf,
) -> Result<(), Box<dyn Error>> {
    let project = read_project(project_path)?;
    let output_root = project_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(
            || std::path::PathBuf::from("."),
            std::path::Path::to_path_buf,
        );
    mcp::set_output_root(&output_root)?;
    let mut session = mcp::session(project)?;
    let listener = mcp::socket::bind(port)?;
    let address = listener.local_addr()?;
    let token = mcp::socket::new_token().map_err(|error| format!("token: {error}"))?;
    write_private_file(token_file, &token)?;
    println!("listening on {address}; token in {}", token_file.display());
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("connection failed: {error}");
                continue;
            }
        };
        // One bad client must not stop the server: log and keep accepting.
        let served = mcp::socket::serve_stream_with(&mut session, &token, stream, |session| {
            let json = session
                .project()
                .to_json_pretty()
                .map_err(std::io::Error::other)?;
            write_atomically(project_path, &json)
        });
        if let Err(error) = served {
            eprintln!("connection closed: {error}");
        }
    }
    Ok(())
}

/// Writes `contents` to a temporary file beside `path`, then renames it over
/// `path`, so a crash never leaves a half-written project.
fn write_atomically(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let temporary = path.with_file_name(name);
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

fn mcp_call(
    project: &PathBuf,
    tool: &str,
    args: &str,
    out: Option<&PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let project = read_project(project)?;
    let mut session = mcp::session(project)?;
    let arguments: serde_json::Value =
        serde_json::from_str(args).map_err(|error| format!("--args is not JSON: {error}"))?;
    let result = mcp::call(&mut session, tool, &arguments)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    if let Some(path) = out {
        let json = session.project().to_json_pretty()?;
        fs::write(path, json)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
        eprintln!("wrote {}", path.display());
    }
    Ok(())
}

fn audio_midi_song(wav: &std::path::Path, bpm: f64, drums: bool) -> Result<Song, String> {
    let decoded = audio::decode::decode_mono(wav).or_else(|_| {
        let bytes = fs::read(wav).map_err(|error| format!("{}: {error}", wav.display()))?;
        audio::wav::decode_mono(&bytes)
    })?;
    let detected = if drums {
        audio::analysis::to_drums(&decoded.samples, decoded.sample_rate)
    } else {
        audio::analysis::to_midi(&decoded.samples, decoded.sample_rate)
    };
    if detected.is_empty() {
        return Err(String::from("no notes detected"));
    }
    let mut song = Song::new(
        wav.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Audio transcription"),
        bpm,
    );
    let mut track = project::song::Track::new(
        project::TrackId::new(1).map_err(|error| error.to_string())?,
        if drums { "Drums" } else { "Melody" },
    );
    let channel = if drums { 9_u8 } else { 0_u8 };
    for (index, note) in detected.iter().enumerate() {
        let (start, duration) = audio::analysis::as_ticks(note, bpm);
        track.notes.push(project::song::Note {
            id: project::NoteId::new(index as u32 + 1).map_err(|error| error.to_string())?,
            start,
            duration: duration.max(1),
            pitch: music_core::Pitch::new(note.pitch).map_err(|error| error.to_string())?,
            velocity: music_core::Velocity::new((note.level * 127.0).clamp(1.0, 127.0) as u8)
                .map_err(|error| error.to_string())?,
            channel: music_core::Channel::new(channel).map_err(|error| error.to_string())?,
        });
    }
    song.tracks.push(track);
    Ok(song)
}

fn audio_chords_json(
    wav: &std::path::Path,
    bpm: f64,
    segment: f64,
) -> Result<serde_json::Value, String> {
    let decoded = audio::decode::decode_mono(wav).or_else(|_| {
        let bytes = fs::read(wav).map_err(|error| format!("{}: {error}", wav.display()))?;
        audio::wav::decode_mono(&bytes)
    })?;
    let found = audio::chords::recognize(&decoded.samples, decoded.sample_rate, segment);
    Ok(serde_json::Value::Array(
        found
            .iter()
            .map(|event| {
                serde_json::json!({
                    "tick": audio::chords::to_ticks(event.start, bpm),
                    "name": event.chord.format(music_core::Spelling::Sharp),
                    "score": event.score
                })
            })
            .collect(),
    ))
}

#[allow(clippy::too_many_arguments)]
fn groove_from_audio(
    audio_path: &PathBuf,
    dir: &std::path::Path,
    name: &str,
    bpm: f64,
    detect_tempo: bool,
    grid: i64,
) -> Result<(), Box<dyn Error>> {
    let decoded = audio::decode::decode_mono(audio_path).or_else(|_| {
        let bytes =
            fs::read(audio_path).map_err(|error| format!("{}: {error}", audio_path.display()))?;
        audio::wav::decode_mono(&bytes)
    })?;
    let bpm = if detect_tempo {
        audio::analysis::estimate_tempo(&decoded.samples, decoded.sample_rate).unwrap_or(bpm)
    } else {
        bpm
    };
    let detected = audio::analysis::to_midi(&decoded.samples, decoded.sample_rate);
    let mut notes = Vec::new();
    for (index, note) in detected.iter().enumerate() {
        let (start, duration) = audio::analysis::as_ticks(note, bpm);
        if duration <= 0 {
            continue;
        }
        let channel = music_core::Channel::from_one_based(1).map_err(|error| error.to_string())?;
        notes.push(project::song::Note {
            id: project::NoteId::new(index as u32 + 1).map_err(|error| error.to_string())?,
            start,
            duration,
            pitch: music_core::Pitch::new(note.pitch).map_err(|error| error.to_string())?,
            velocity: music_core::Velocity::new((note.level * 127.0).clamp(1.0, 127.0) as u8)
                .map_err(|error| error.to_string())?,
            channel,
        });
    }
    let bar = 4 * music_core::PPQ;
    let grid = (4 * music_core::PPQ / grid.max(1)).max(1);
    let groove = arranging::groove::extract(&notes, bar, grid)
        .ok_or("could not extract a groove from this audio")?;
    let path = arranging::groove_file::save(dir, name, &groove)?;
    println!(
        "wrote {}: {:.2} bpm, {} note(s), {} slot(s), offsets {:?}",
        path.display(),
        bpm,
        notes.len(),
        groove.offsets.len(),
        groove.offsets
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn style_from_audio(
    audio_path: &PathBuf,
    out: &PathBuf,
    bpm: f64,
    section: &str,
    chord: &str,
    split_parts: bool,
    section_bars: u32,
    drums: bool,
    detect_tempo: bool,
    version: &str,
) -> Result<(), Box<dyn Error>> {
    let decoded = audio::decode::decode_mono(audio_path).or_else(|_| {
        let bytes =
            fs::read(audio_path).map_err(|error| format!("{}: {error}", audio_path.display()))?;
        audio::wav::decode_mono(&bytes)
    })?;
    let source = if chord.eq_ignore_ascii_case("auto") {
        None
    } else {
        let parsed = Chord::parse(chord)?;
        Some(
            arranger::chord_type::ArrangerChord::try_from(&parsed)
                .map_err(|error| format!("unsupported style chord {chord:?}: {error:?}"))?,
        )
    };
    let built = style_formats::style_from_audio(
        &decoded,
        style_formats::AudioStyleOptions {
            bpm,
            section,
            source_chord: source,
            split_parts,
            section_bars,
            include_drums: drums,
            detect_tempo,
        },
    )?;
    let style = built.style;
    let version = match version {
        "sff1" => style_formats::casm::SffVersion::Sff1,
        "sff2" => style_formats::casm::SffVersion::Sff2,
        other => return Err(format!("--version must be sff1 or sff2, found {other:?}").into()),
    };
    let bytes = style_formats::write_native_style(&style, version)?;
    fs::write(out, &bytes)
        .map_err(|error| format!("could not write {}: {error}", out.display()))?;
    let check = style_formats::import_style(&bytes)?;
    println!(
        "wrote {}: source {}, {:.2} bpm, {} detected note(s), {} section(s), {} byte(s); re-read {:?}",
        out.display(),
        built.source_chord.name(),
        built.bpm,
        built.notes,
        style.sections.len(),
        bytes.len(),
        check.section_names()
    );
    Ok(())
}

fn style_from_clips(
    project: &PathBuf,
    out: &PathBuf,
    section: &str,
    chord: &str,
    length: u32,
    from_markers: bool,
    version: &str,
) -> Result<(), Box<dyn Error>> {
    let text = fs::read_to_string(project)
        .map_err(|error| format!("could not read {}: {error}", project.display()))?;
    let file = ProjectFile::from_json(&text)?;
    let source_text = Chord::parse(chord)?;
    let source = arranger::chord_type::ArrangerChord::try_from(&source_text)
        .map_err(|error| format!("unsupported style chord {chord:?}: {error:?}"))?;
    let mut style = if from_markers {
        arranger::style::from_song_sections(&file.song, &file.song.name, source)
    } else {
        arranger::style::from_song(
            &file.song,
            &file.song.name,
            source,
            Tick::from(length.max(1)),
        )
    };
    if !from_markers && let Some(first) = style.sections.first_mut() {
        first.name = section.to_string();
    }
    let version = match version {
        "sff1" => style_formats::casm::SffVersion::Sff1,
        "sff2" => style_formats::casm::SffVersion::Sff2,
        other => return Err(format!("--version must be sff1 or sff2, found {other:?}").into()),
    };
    let bytes = style_formats::write_native_style(&style, version)?;
    fs::write(out, &bytes)
        .map_err(|error| format!("could not write {}: {error}", out.display()))?;
    let check = style_formats::import_style(&bytes)?;
    println!(
        "wrote {}: {} part(s), {} section(s), {} byte(s); re-read sections {:?}",
        out.display(),
        style
            .sections
            .first()
            .map_or(0, |section| section.parts.len()),
        style.sections.len(),
        bytes.len(),
        check.section_names()
    );
    Ok(())
}

fn style_save(style: &PathBuf, out: &PathBuf, version: &str) -> Result<(), Box<dyn Error>> {
    let bytes =
        fs::read(style).map_err(|error| format!("could not read {}: {error}", style.display()))?;
    let imported = style_formats::import_style(&bytes)?;
    let rules = style_formats::casm::read_rules(&bytes);
    let version = match version {
        "sff1" => style_formats::casm::SffVersion::Sff1,
        "sff2" => style_formats::casm::SffVersion::Sff2,
        other => return Err(format!("--version must be sff1 or sff2, found {other:?}").into()),
    };
    let written = style_formats::write_style(&imported, &rules, version)?;
    fs::write(out, &written)
        .map_err(|error| format!("could not write {}: {error}", out.display()))?;
    let check = style_formats::import_style(&written)?;
    println!(
        "wrote {}: {} section(s), {} track(s), {} byte(s), re-read {} section(s) as {:?}",
        out.display(),
        imported.sections.len(),
        imported.song.tracks.len(),
        written.len(),
        check.sections.len(),
        version
    );
    Ok(())
}

fn link_to_midi(
    link: &str,
    out: &PathBuf,
    folder: Option<&std::path::Path>,
    ffmpeg: Option<&std::path::Path>,
    keep: bool,
) -> Result<(), Box<dyn Error>> {
    let cleaned = audio::link::clean(link);
    if cleaned.is_empty() {
        return Err("not an http(s) link".into());
    }
    let folder = folder
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir);
    let ffmpeg = ffmpeg.unwrap_or_else(|| std::path::Path::new(""));
    let path = audio::link::download(&cleaned, &folder, ffmpeg)?;
    let bytes = fs::read(&path)?;
    let decoded = audio::wav::decode_mono(&bytes)?;
    let notes = audio::analysis::to_midi(&decoded.samples, decoded.sample_rate);
    if notes.is_empty() {
        return Err("the audio produced no notes".into());
    }
    let mut song = project::song::Song::new(
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Audio")
            .to_string(),
        120.0,
    );
    let mut track = project::song::Track::new(project::TrackId::new(1)?, "Audio to MIDI");
    for (index, detected) in notes.iter().enumerate() {
        let (start, duration) = audio::analysis::as_ticks(detected, 120.0);
        track.notes.push(project::song::Note {
            id: project::NoteId::new(u32::try_from(index + 1)?)?,
            start,
            duration: duration.max(1),
            pitch: music_core::Pitch::new(detected.pitch)?,
            velocity: music_core::Velocity::new((detected.level * 127.0).clamp(1.0, 127.0) as u8)?,
            channel: music_core::Channel::first(),
        });
    }
    song.tracks.push(track);
    let encoded = smf::encode(&song)?;
    fs::write(out, encoded)
        .map_err(|error| format!("could not write {}: {error}", out.display()))?;
    if !keep {
        let _ = fs::remove_file(&path);
    }
    println!(
        "converted {} note(s) from {} to {}",
        notes.len(),
        cleaned,
        out.display()
    );
    Ok(())
}

fn parse_tonic(name: &str) -> Result<music_core::PitchClass, Box<dyn Error>> {
    let name = name.trim();
    let with_octave = if name
        .chars()
        .last()
        .is_some_and(|last| last.is_ascii_digit())
    {
        name.to_string()
    } else {
        format!("{name}4")
    };
    let pitch = music_core::Pitch::from_name(&with_octave)?;
    Ok(music_core::PitchClass::new(i32::from(pitch.as_u8() % 12)))
}

fn export_audio(
    project: &PathBuf,
    out: &PathBuf,
    sample_rate: u32,
    bits: u16,
    metronome: bool,
    master: audio::master::Master,
) -> Result<(), Box<dyn Error>> {
    if !matches!(bits, 16 | 24 | 32) {
        return Err(format!("--bits must be 16, 24 or 32, found {bits}").into());
    }
    if !(8_000..=192_000).contains(&sample_rate) {
        return Err(
            format!("--sample-rate must be between 8000 and 192000, found {sample_rate}").into(),
        );
    }
    let project = read_project(project)?;
    let rendered =
        audio::render_mastered(&project.song, sample_rate, master, metronome.then_some(1.0));
    for problem in audio::mix::check_audio_references(&project.song) {
        eprintln!(
            "audio {} on track {}: {}",
            problem.path, problem.track, problem.reason
        );
    }
    let levels = audio::master::meter(&rendered);
    let wav = match bits {
        24 => rendered.to_wav24(),
        32 => rendered.to_wav32f(),
        _ => rendered.to_wav(),
    };
    fs::write(out, &wav).map_err(|error| format!("could not write {}: {error}", out.display()))?;
    println!(
        "rendered {} track(s) to {:.2} s at {} Hz: {} byte(s) -> {}",
        project.song.tracks.len(),
        rendered.duration_seconds(),
        sample_rate,
        wav.len(),
        out.display()
    );
    println!("master: peak {:.3}, rms {:.3}", levels.peak, levels.rms);
    Ok(())
}

fn read_style(path: &PathBuf) -> Result<style_formats::ImportedStyle, Box<dyn Error>> {
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    Ok(style_formats::import_style(&bytes)?)
}

fn parse_chords(chords: &[String]) -> Result<Vec<(Tick, Chord)>, Box<dyn Error>> {
    let mut out = Vec::new();
    for entry in chords {
        let (tick, symbol) = entry
            .split_once('=')
            .ok_or_else(|| format!("--chord expects TICK=SYMBOL, found {entry:?}"))?;
        let tick: Tick = tick
            .trim()
            .parse()
            .map_err(|_| format!("--chord tick {tick:?} is not a whole number"))?;
        out.push((tick, Chord::parse(symbol)?));
    }
    Ok(out)
}

fn perform(
    style_path: &PathBuf,
    out: &PathBuf,
    section: Option<&str>,
    chords: &[String],
    tempo: Option<f64>,
) -> Result<(), Box<dyn Error>> {
    let imported = read_style(style_path)?;
    let chosen = section
        .map(String::from)
        .or_else(|| imported.sections.first().map(|entry| entry.name.clone()))
        .ok_or("style has no sections")?;
    let mut recorder = performance::PerformanceRecorder::new(
        imported.native.clone(),
        StyleId::new(1)?,
        format!("{} {chosen}", style_path.display()),
        tempo.unwrap_or(imported.tempo_bpm),
    );
    for (tick, chord) in parse_chords(chords)? {
        recorder.chord(tick, &chord)?;
    }
    let tracks = recorder.section(&chosen, 0)?;
    if tracks.is_empty() {
        return Err(format!("section {chosen:?} produced no notes").into());
    }
    let project = recorder.finish();
    let json = project.to_json_pretty()?;
    fs::write(out, json).map_err(|error| format!("could not write {}: {error}", out.display()))?;
    let notes = project
        .song
        .tracks
        .iter()
        .map(|track| track.notes.len())
        .sum::<usize>();
    println!(
        "recorded {chosen:?} from {}: {} track(s), {} note(s), provenance on {} track(s)",
        style_path.display(),
        project.song.tracks.len(),
        notes,
        project
            .song
            .tracks
            .iter()
            .filter(|track| track.provenance.is_some())
            .count()
    );
    println!("wrote {}", out.display());
    Ok(())
}

fn regenerate_project(
    project_path: &PathBuf,
    style_path: &PathBuf,
    out: &PathBuf,
    policy: &str,
) -> Result<(), Box<dyn Error>> {
    let imported = read_style(style_path)?;
    let policy = match policy {
        "keep" => performance::Policy::Keep,
        "replace" => performance::Policy::Replace,
        "duplicate" => performance::Policy::Duplicate,
        other => {
            return Err(
                format!("--policy must be keep, replace or duplicate, found {other:?}").into(),
            );
        }
    };
    let project = read_project(project_path)?;
    let mut session = EditSession::new(project)?;
    let report = performance::regenerate(&mut session, &imported.native, StyleId::new(1)?, policy)?;
    let json = session.project().to_json_pretty()?;
    fs::write(out, json).map_err(|error| format!("could not write {}: {error}", out.display()))?;
    println!(
        "regenerate: fresh {} kept {} replaced {} duplicated {} missing-section {} manual {} other-style {}",
        report.fresh.len(),
        report.kept.len(),
        report.replaced.len(),
        report.duplicated.len(),
        report.missing_sections.len(),
        report.manual,
        report.other_style
    );
    println!("wrote {}", out.display());
    Ok(())
}

fn style_command(
    path: &PathBuf,
    section: Option<&str>,
    out: Option<&PathBuf>,
    chords: &[String],
    show_rules: bool,
) -> Result<(), Box<dyn Error>> {
    let bytes =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let imported = style_formats::import_style(&bytes)?;
    println!(
        "{}: {} section(s), source {}, source ppq {}, {:.3} bpm, {}/{}",
        path.display(),
        imported.sections.len(),
        imported.native.source_chord.name(),
        imported.source_ppq,
        imported.tempo_bpm,
        imported.time_signature.numerator,
        imported.time_signature.denominator
    );
    let bar = imported.time_signature.bar_ticks();
    for entry in &imported.sections {
        println!(
            "  {:<12} {:>6} ticks ({:>4.1} bars) {:>5} note(s) {:>2} part(s)",
            entry.name,
            entry.length_ticks,
            entry.length_ticks as f64 / bar as f64,
            entry.notes,
            entry.parts
        );
    }
    println!(
        "  markers {} ({} used, {} skipped), default rules {}, unpaired note-ons {}",
        imported.stats.markers_total,
        imported.stats.markers_used,
        imported.stats.markers_skipped,
        imported.stats.channels_without_rule,
        imported.stats.unpaired_note_ons
    );
    let chosen = section.unwrap_or_else(|| {
        imported
            .sections
            .first()
            .map_or("", |entry| entry.name.as_str())
    });
    let Some(native_section) = imported.native.section(chosen) else {
        return Err(format!(
            "style has no section {chosen:?}; it has {:?}",
            imported.section_names()
        )
        .into());
    };
    if show_rules {
        println!("rules for {chosen:?}:");
        for part in &native_section.parts {
            println!(
                "  ch{:>2}->{:>2} {:<8} {} note(s) ntr={:?} ntt={:?} bass={} limits={}..{}",
                part.rule.source_channel,
                part.rule.dest_channel,
                part.name,
                part.notes.len(),
                part.rule.transpose.ntr,
                part.rule.transpose.ntt,
                part.rule.transpose.bass,
                part.rule.transpose.low_limit,
                part.rule.transpose.high_limit
            );
        }
    }
    let Some(out) = out else {
        return Ok(());
    };
    let mut chord_events = Vec::new();
    for entry in chords {
        let (tick, symbol) = entry
            .split_once('=')
            .ok_or_else(|| format!("--chord expects TICK=SYMBOL, found {entry:?}"))?;
        let tick: Tick = tick
            .trim()
            .parse()
            .map_err(|_| format!("--chord tick {tick:?} is not a whole number"))?;
        chord_events.push(arranger::ChordAt {
            tick,
            chord: arranger::ArrangerChord::try_from(&Chord::parse(symbol)?)?,
        });
        let _ = &chords;
    }
    let compiled = imported.native.compile(chosen, 0, &chord_events)?;
    let mut song = Song::new(format!("{} {chosen}", path.display()), imported.tempo_bpm);
    song.time_signature = imported.time_signature;
    let mut track = Track::new(TrackId::new(1)?, "Arranger");
    for (index, note) in compiled.iter().enumerate() {
        track.notes.push(Note {
            id: NoteId::new(u32::try_from(index + 1).unwrap_or(u32::MAX))?,
            start: note.tick,
            duration: note.duration,
            pitch: Pitch::new(note.pitch)?,
            velocity: Velocity::new(note.velocity)?,
            channel: note.channel,
        });
    }
    song.tracks.push(track);
    write_midi(out, &song)?;
    println!(
        "compiled {chosen:?}: {} note(s) from {} part(s)",
        compiled.len(),
        native_section.parts.len()
    );
    Ok(())
}

fn play(
    project_path: &PathBuf,
    out_port: &str,
    sample_rate: u32,
    dry_run: bool,
) -> Result<(), Box<dyn Error>> {
    let project = read_project(project_path)?;
    let prepared = PreparedPattern::prepare(&project.song, sample_rate)?;
    let messages: Vec<midi_devices::TimedMessage> = prepared
        .events()
        .iter()
        .map(|event| midi_devices::TimedMessage {
            at_micros: midi_devices::sample_to_micros(event.sample, sample_rate),
            message: event.message,
        })
        .collect();
    let last = messages.last().map_or(0, |message| message.at_micros);
    println!(
        "schedule: {} message(s) over {:.3} s at {} Hz",
        messages.len(),
        last as f64 / 1_000_000.0,
        sample_rate
    );
    if dry_run {
        for message in messages.iter().take(8) {
            println!(
                "  {:>8} us ch {} {}",
                message.at_micros,
                message.message.channel(),
                message_label(message.message)
            );
        }
        println!("dry run: nothing was sent to {out_port:?}");
        return Ok(());
    }
    let sent = midi_devices::play_to_port(out_port, &messages)?;
    println!("sent {sent} message(s) to {out_port:?}, then all-notes-off");
    Ok(())
}

fn record(
    input: &str,
    out: &PathBuf,
    seconds: f64,
    tempo: f64,
    wait: bool,
) -> Result<(), Box<dyn Error>> {
    let tempo_map = TempoMap::constant(tempo);
    let (song, notes, stats) =
        midi_devices::record_to_song_after(input, seconds, &tempo_map, "Recorded", wait)
            .map_err(map_device_error)?;
    println!(
        "recorded {} note(s) in {seconds} s from {input:?}",
        notes.len()
    );
    if stats.unmatched_note_offs > 0 {
        eprintln!(
            "warning: {} unmatched note-off(s)",
            stats.unmatched_note_offs
        );
    }
    if stats.held_at_stop > 0 {
        eprintln!(
            "warning: {} note(s) were still held when recording stopped and were dropped",
            stats.held_at_stop
        );
    }
    write_midi(out, &song)?;
    Ok(())
}

fn play_audio(
    project: &PathBuf,
    device: Option<&str>,
    seconds: Option<f64>,
) -> Result<(), Box<dyn Error>> {
    let text = fs::read_to_string(project)?;
    let project = project::song::ProjectFile::from_json(&text)?;
    let rendered = audio::render(&project.song, audio::SAMPLE_RATE);
    let duration = rendered.duration_seconds();
    let player = audio::player::Player::new_on(rendered, device, None)?;
    player.play();
    let limit = seconds.map_or(duration + 0.2, |value| value.max(0.1));
    let started = std::time::Instant::now();
    while player.is_playing() && started.elapsed().as_secs_f64() < limit {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    player.release();
    println!("played {:.2} s with cpal", limit.min(duration));
    Ok(())
}

fn midi_cc(output: &str, channel: u8, controller: u8, value: u8) -> Result<(), Box<dyn Error>> {
    if channel > 15 || controller > 127 || value > 127 {
        return Err("channel must be 0..=15 and controller/value 0..=127".into());
    }
    let mut connection = midi_devices::devices::open_output(output).map_err(map_device_error)?;
    let bytes = [0xB0 | channel, controller, value];
    connection
        .send(&bytes)
        .map_err(|error| format!("{output}: {error}"))?;
    println!(
        "sent CC {controller} = {value} on channel {} to {output}",
        channel + 1
    );
    Ok(())
}

fn midi_monitor(input: &str, seconds: f64) -> Result<(), Box<dyn Error>> {
    if !seconds.is_finite() {
        return Err("--seconds must be a finite number".into());
    }
    let live = midi_devices::LiveInput::open(input).map_err(map_device_error)?;
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs_f64(seconds.clamp(0.1, 86_400.0));
    let mut count = 0_usize;
    let mut controllers = 0_usize;
    while std::time::Instant::now() < deadline {
        while let Some(message) = live.next() {
            count += 1;
            if matches!(message, music_core::MidiMessage::ControlChange { .. }) {
                controllers += 1;
            }
            println!("{message:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    println!("{count} message(s), {controllers} control change(s)");
    Ok(())
}

fn echo(input: &str, output: &str, seconds: f64) -> Result<(), Box<dyn Error>> {
    let report = midi_devices::echo(input, output, seconds).map_err(map_device_error)?;
    println!(
        "forwarded {} message(s) from {input:?} to {output:?} in {seconds} s",
        report.forwarded
    );
    Ok(())
}

fn map_device_error(error: DeviceError) -> Box<dyn Error> {
    if let DeviceError::NoSuchPort { available, .. } = &error {
        eprintln!("available ports: {available:?}");
    }
    Box::new(error)
}

fn message_label(message: music_core::MidiMessage) -> &'static str {
    use music_core::MidiMessage as M;
    match message {
        M::NoteOn { .. } => "note-on",
        M::NoteOff { .. } => "note-off",
        M::ControlChange { .. } => "control-change",
        M::ProgramChange { .. } => "program-change",
        M::PitchBend { .. } => "pitch-bend",
    }
}

fn parse_hex_bytes(text: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let cleaned: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    if cleaned.is_empty() || !cleaned.len().is_multiple_of(2) {
        return Err("hex string must contain an even number of hex digits".into());
    }
    let mut bytes = Vec::with_capacity(cleaned.len() / 2);
    for pair in cleaned.as_bytes().as_chunks::<2>().0 {
        let s = std::str::from_utf8(pair)?;
        bytes.push(u8::from_str_radix(s, 16)?);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn on_a_large_stack(run: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(run)
            .expect("spawn")
            .join()
            .expect("join");
    }

    #[test]
    fn the_command_tree_is_valid() {
        on_a_large_stack(|| Cli::command().debug_assert());
    }

    #[test]
    fn audio_midi_song_transcribes_a_tone() {
        let rate = 48_000_u32;
        let mut samples = vec![0.0_f32; rate as usize];
        for (index, sample) in samples.iter_mut().enumerate() {
            let time = index as f32 / rate as f32;
            *sample = (std::f32::consts::TAU * 440.0 * time).sin() * 0.5;
        }
        let path =
            std::env::temp_dir().join(format!("lazy-arranger-cli-midi-{}.wav", std::process::id()));
        std::fs::write(
            &path,
            audio::RenderedAudio {
                samples,
                sample_rate: rate,
            }
            .to_wav32f(),
        )
        .expect("wav");
        let song = audio_midi_song(&path, 120.0, false).expect("midi");
        let _ = std::fs::remove_file(&path);
        assert!(!song.tracks.is_empty());
        assert!(!song.tracks[0].notes.is_empty());
        assert_eq!(song.tracks[0].notes[0].channel.zero_based(), 0);
    }

    #[test]
    fn score_pdf_accepts_full_score_frames() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                use clap::Parser;
                let cli = Cli::try_parse_from([
                    "lazy-arranger",
                    "score-pdf",
                    "song.arranger",
                    "--out",
                    "score.pdf",
                    "--all",
                    "--frames",
                ])
                .expect("parse");
                match cli.command {
                    Commands::ScorePdf { all, frames, .. } => {
                        assert!(all);
                        assert!(frames);
                    }
                    _ => panic!("score-pdf"),
                }
            })
            .expect("thread")
            .join()
            .expect("join");
    }

    #[test]
    fn wav_reverse_accepts_a_file_and_output() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                use clap::Parser;
                let cli = Cli::try_parse_from([
                    "lazy-arranger",
                    "wav-reverse",
                    "in.wav",
                    "--out",
                    "out.wav",
                ])
                .expect("parse");
                match cli.command {
                    Commands::WavReverse { wav, out } => {
                        assert_eq!(wav, std::path::PathBuf::from("in.wav"));
                        assert_eq!(out, std::path::PathBuf::from("out.wav"));
                    }
                    _ => panic!("wav-reverse"),
                }
            })
            .expect("thread")
            .join()
            .expect("join");
    }

    #[test]
    fn wav_trim_accepts_a_window() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                use clap::Parser;
                let cli = Cli::try_parse_from([
                    "lazy-arranger",
                    "wav-trim",
                    "in.wav",
                    "--out",
                    "out.wav",
                    "--start",
                    "0.25",
                    "--end",
                    "2.5",
                ])
                .expect("parse");
                match cli.command {
                    Commands::WavTrim { start, end, .. } => {
                        assert!((start - 0.25).abs() < 1.0e-6);
                        assert!((end.expect("end") - 2.5).abs() < 1.0e-6);
                    }
                    _ => panic!("wav-trim"),
                }
            })
            .expect("thread")
            .join()
            .expect("join");
    }

    #[test]
    fn wav_fade_accepts_ramp_seconds() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                use clap::Parser;
                let cli = Cli::try_parse_from([
                    "lazy-arranger",
                    "wav-fade",
                    "in.wav",
                    "--out",
                    "out.wav",
                    "--fade-in",
                    "0.5",
                    "--fade-out",
                    "1.5",
                ])
                .expect("parse");
                match cli.command {
                    Commands::WavFade {
                        fade_in, fade_out, ..
                    } => {
                        assert!((fade_in - 0.5).abs() < 1.0e-6);
                        assert!((fade_out - 1.5).abs() < 1.0e-6);
                    }
                    _ => panic!("wav-fade"),
                }
            })
            .expect("thread")
            .join()
            .expect("join");
    }

    #[test]
    fn audio_midi_song_can_transcribe_drums() {
        let rate = 48_000_u32;
        let mut samples = vec![0.0_f32; rate as usize];
        let burst = (0.08 * f64::from(rate)) as usize;
        let start = (0.2 * f64::from(rate)) as usize;
        for index in 0..burst {
            let time = index as f64 / f64::from(rate);
            samples[start + index] = (std::f64::consts::TAU * 60.0 * time).sin() as f32 * 0.8;
        }
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-cli-drums-{}.wav",
            std::process::id()
        ));
        std::fs::write(
            &path,
            audio::RenderedAudio {
                samples,
                sample_rate: rate,
            }
            .to_wav32f(),
        )
        .expect("wav");
        let song = audio_midi_song(&path, 120.0, true).expect("drums");
        let _ = std::fs::remove_file(&path);
        assert!(
            song.tracks[0]
                .notes
                .iter()
                .all(|note| note.channel.zero_based() == 9)
        );
    }

    #[test]
    fn audio_chords_json_recognizes_a_triad() {
        let rate = 48_000_u32;
        let mut samples = vec![0.0_f32; rate as usize];
        for (index, sample) in samples.iter_mut().enumerate() {
            let time = index as f32 / rate as f32;
            *sample = [261.63_f32, 329.63, 392.0]
                .iter()
                .map(|hz| (std::f32::consts::TAU * hz * time).sin())
                .sum::<f32>()
                / 3.0;
        }
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-cli-chords-{}.wav",
            std::process::id()
        ));
        std::fs::write(
            &path,
            audio::RenderedAudio {
                samples,
                sample_rate: rate,
            }
            .to_wav32f(),
        )
        .expect("wav");
        let value = audio_chords_json(&path, 120.0, 0.5).expect("chords");
        let _ = std::fs::remove_file(&path);
        assert!(value.as_array().is_some_and(|chords| !chords.is_empty()));
    }

    #[cfg(unix)]
    #[test]
    fn write_private_file_tightens_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-cli-private-{}.token",
            std::process::id()
        ));
        std::fs::write(&path, "old").expect("seed");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        write_private_file(&path, "fresh-token").expect("write");
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        let _ = std::fs::remove_file(&path);
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn every_subcommand_accepts_help() {
        on_a_large_stack(|| {
            for subcommand in Cli::command().get_subcommands() {
                let name = subcommand.get_name().to_string();
                let command = subcommand.clone();
                let error = command
                    .try_get_matches_from(["lazy-arranger", "--help"])
                    .expect_err("--help is not a match");
                assert_eq!(
                    error.kind(),
                    clap::error::ErrorKind::DisplayHelp,
                    "subcommand {name}"
                );
            }
        });
    }
}
