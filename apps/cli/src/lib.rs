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
use std::path::PathBuf;
use std::process::ExitCode;

use app_commands::{Command, EditSession};
use clap::{Parser, Subcommand};
use midi_devices::{DeviceError, list_ports};
use music_core::{Channel, Chord, PPQ, TempoMap, Tick, Velocity};
use project::song::{Note, ProjectFile, Song, Track};
use project::{NoteId, TrackId};
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
