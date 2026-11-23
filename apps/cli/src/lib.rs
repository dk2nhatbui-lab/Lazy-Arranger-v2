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
        #[arg(long, default_value_t = DEFAULT_SAMPLE_RATE)]
        sample_rate: u32,
        /// Audio block size used by the scheduler.
        #[arg(long, default_value_t = DEFAULT_BLOCK)]
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
        #[arg(long, default_value_t = DEFAULT_SAMPLE_RATE)]
        sample_rate: u32,
        /// Audio block size used by the scheduler.
        #[arg(long, default_value_t = DEFAULT_BLOCK)]
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
        #[arg(long, default_value_t = 48_000)]
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
    /// Render a project to a 16-bit stereo WAV file.
    ExportAudio {
        /// Project `.json` path.
        project: PathBuf,
        /// Output `.wav` path.
        #[arg(long)]
        out: PathBuf,
        /// Sample rate; defaults to 48000.
        #[arg(long, default_value_t = audio::SAMPLE_RATE)]
        sample_rate: u32,
        /// Bit depth: 16 or 24 (default 16).
        #[arg(long, default_value_t = 16)]
        bits: u16,
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
        #[arg(long, default_value_t = DEFAULT_SAMPLE_RATE)]
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
        Commands::ScorePdf {
            project,
            out,
            track,
            title,
        } => {
            let project = read_project(&project)?;
            let Some(entry) = project.song.tracks.get(track) else {
                return Err(format!("no track at index {track}").into());
            };
            let settings = score::Settings::default();
            let engraved = score::engrave(&project.song, entry.id, settings);
            let score_title = title.unwrap_or_else(|| project.song.name.clone());
            let pdf = score::pdf::to_pdf(&engraved, settings, &score_title);
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
            let text = fs::read_to_string(&file).unwrap_or_default();
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
            let text = fs::read_to_string(&file).unwrap_or_default();
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
        } => export_audio(&project, &out, sample_rate, bits),
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
        } => record(&input, &out, seconds, tempo),
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
    let imported = project::legacy::import_cpp_project(&text)?;
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
        let is_call =
            message.get("method").and_then(|method| method.as_str()) == Some("tools/call");
        if let Some(reply) = mcp::handle_message(&mut session, &message) {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
            if is_call && reply["result"]["isError"] == false {
                let json = session.project().to_json_pretty()?;
                fs::write(project_path, json).map_err(|error| {
                    format!("could not save {}: {error}", project_path.display())
                })?;
            }
        }
    }
}

fn mcp_socket(
    project_path: &PathBuf,
    port: u16,
    token_file: &PathBuf,
) -> Result<(), Box<dyn Error>> {
    let project = read_project(project_path)?;
    let mut session = mcp::session(project)?;
    let listener = mcp::socket::bind(port)?;
    let address = listener.local_addr()?;
    let token = mcp::socket::new_token();
    fs::write(token_file, &token)
        .map_err(|error| format!("could not write {}: {error}", token_file.display()))?;
    println!("listening on {address}; token in {}", token_file.display());
    for stream in listener.incoming() {
        let stream = stream?;
        let reader = std::io::BufReader::new(stream.try_clone()?);
        let mut writer = stream;
        for line in std::io::BufRead::lines(reader) {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let is_call = serde_json::from_str::<serde_json::Value>(&line)
                .ok()
                .and_then(|message| message.get("method").cloned())
                .and_then(|method| method.as_str().map(str::to_string))
                .as_deref()
                == Some("tools/call");
            if let Some(reply) = mcp::socket::handle_line(&mut session, &token, &line) {
                writeln!(writer, "{reply}")?;
                writer.flush()?;
                if is_call && reply["result"]["isError"] == false {
                    let json = session.project().to_json_pretty()?;
                    fs::write(project_path, json).map_err(|error| {
                        format!("could not save {}: {error}", project_path.display())
                    })?;
                }
            }
        }
    }
    Ok(())
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
) -> Result<(), Box<dyn Error>> {
    if !matches!(bits, 16 | 24) {
        return Err(format!("--bits must be 16 or 24, found {bits}").into());
    }
    if !(8_000..=192_000).contains(&sample_rate) {
        return Err(
            format!("--sample-rate must be between 8000 and 192000, found {sample_rate}").into(),
        );
    }
    let project = read_project(project)?;
    let mut rendered = audio::render(&project.song, sample_rate);
    for problem in audio::mix::mix_audio_references(&mut rendered, &project.song, 0.9) {
        eprintln!(
            "audio {} on track {}: {}",
            problem.path, problem.track, problem.reason
        );
    }
    let wav = if bits == 24 {
        rendered.to_wav24()
    } else {
        rendered.to_wav()
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
        "{}: {} section(s), source ppq {}, {:.3} bpm, {}/{}",
        path.display(),
        imported.sections.len(),
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

fn record(input: &str, out: &PathBuf, seconds: f64, tempo: f64) -> Result<(), Box<dyn Error>> {
    let tempo_map = TempoMap::constant(tempo);
    let (song, notes, stats) = midi_devices::record_to_song(input, seconds, &tempo_map, "Recorded")
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
