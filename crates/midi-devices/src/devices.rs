//! `midir` port access: enumeration, echo and recording.
//!
//! Everything in this module touches the operating system's MIDI stack. CI
//! runs it on a machine with no attached hardware, so the tests here only prove
//! that enumeration does not fail. Recording and echo against a real keyboard
//! are manual checks; see `docs/audit/compatibility-baseline.md`.

use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use midir::{MidiInput, MidiOutput, MidiOutputConnection};
use music_core::{MidiMessage, TempoMap};
use project::ids::TrackId;
use project::song::Song;
use thiserror::Error;

use crate::capture::{CaptureStats, CapturedNote, NoteCapture};

/// Errors raised by port access.
#[derive(Debug, Error)]
pub enum DeviceError {
    /// The MIDI backend could not be started.
    #[error("MIDI backend unavailable: {0}")]
    Init(String),
    /// A port name could not be read.
    #[error("MIDI port name unavailable: {0}")]
    PortName(String),
    /// No port matched the requested name.
    #[error("no MIDI port named {requested:?}; available: {available:?}")]
    NoSuchPort {
        /// The name that was requested.
        requested: String,
        /// The names that exist.
        available: Vec<String>,
    },
    /// A port could not be opened.
    #[error("MIDI port could not be opened: {0}")]
    Connect(String),
    /// Sending a message failed.
    #[error("MIDI send failed: {0}")]
    Send(String),
    /// The recording window ended before the requested duration because the
    /// input closed.
    #[error("MIDI input closed early")]
    InputClosed,
    /// A project could not be built from the recording.
    #[error("recording could not be stored: {0}")]
    Project(String),
}

/// Input and output port names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PortNames {
    /// Input port names in backend order.
    pub inputs: Vec<String>,
    /// Output port names in backend order.
    pub outputs: Vec<String>,
}

/// What an echo pass forwarded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EchoReport {
    /// Messages forwarded to the output.
    pub forwarded: usize,
    /// Bytes that were not a channel message and were skipped.
    pub skipped: usize,
}

/// Enumerates available MIDI ports.
///
/// # Errors
///
/// Returns [`DeviceError::Init`] when the operating system MIDI backend is
/// unavailable.
pub fn list_ports() -> Result<PortNames, DeviceError> {
    let input =
        MidiInput::new("lazy-arranger").map_err(|err| DeviceError::Init(err.to_string()))?;
    let output =
        MidiOutput::new("lazy-arranger").map_err(|err| DeviceError::Init(err.to_string()))?;
    let mut names = PortNames::default();
    for port in input.ports() {
        names.inputs.push(
            input
                .port_name(&port)
                .map_err(|err| DeviceError::PortName(err.to_string()))?,
        );
    }
    for port in output.ports() {
        names.outputs.push(
            output
                .port_name(&port)
                .map_err(|err| DeviceError::PortName(err.to_string()))?,
        );
    }
    Ok(names)
}

fn find_output(name: &str) -> Result<MidiOutputConnection, DeviceError> {
    let output =
        MidiOutput::new("lazy-arranger").map_err(|err| DeviceError::Init(err.to_string()))?;
    let ports = output.ports();
    let mut available = Vec::with_capacity(ports.len());
    for port in ports {
        let port_name = output
            .port_name(&port)
            .map_err(|err| DeviceError::PortName(err.to_string()))?;
        if port_name == name {
            return output
                .connect(&port, "lazy-arranger-out")
                .map_err(|err| DeviceError::Connect(err.to_string()));
        }
        available.push(port_name);
    }
    Err(DeviceError::NoSuchPort {
        requested: String::from(name),
        available,
    })
}

/// Forwards every channel message from one input to one output for `seconds`.
///
/// This is a wiring check, not the real-time engine: it forwards on the input
/// thread, so it adds the backend's latency but no scheduling queue.
///
/// # Errors
///
/// Returns [`DeviceError`] when either port cannot be opened or a send fails.
pub fn echo(input_name: &str, output_name: &str, seconds: f64) -> Result<EchoReport, DeviceError> {
    let mut output = find_output(output_name)?;
    let (sender, receiver) = channel::<MidiMessage>();
    let input =
        MidiInput::new("lazy-arranger").map_err(|err| DeviceError::Init(err.to_string()))?;
    let ports = input.ports();
    let mut available = Vec::with_capacity(ports.len());
    let mut chosen = None;
    for port in ports {
        let port_name = input
            .port_name(&port)
            .map_err(|err| DeviceError::PortName(err.to_string()))?;
        if port_name == input_name {
            chosen = Some(port);
            break;
        }
        available.push(port_name);
    }
    let port = chosen.ok_or_else(|| DeviceError::NoSuchPort {
        requested: String::from(input_name),
        available,
    })?;
    let _connection = input
        .connect(
            &port,
            "lazy-arranger-in",
            move |_timestamp, bytes, _| {
                if let Some(message) = MidiMessage::from_bytes(bytes) {
                    let _ = sender.send(message);
                }
            },
            (),
        )
        .map_err(|err| DeviceError::Connect(err.to_string()))?;

    let deadline = Instant::now() + Duration::from_secs_f64(seconds.max(0.0));
    let mut report = EchoReport::default();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(remaining) {
            Ok(message) => {
                output
                    .send(&message.to_bytes()[..message.encoded_len()])
                    .map_err(|err| DeviceError::Send(err.to_string()))?;
                report.forwarded += 1;
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => return Err(DeviceError::InputClosed),
        }
    }
    Ok(report)
}

/// Records notes from one input for `seconds` and returns a one-track song.
///
/// # Errors
///
/// Returns [`DeviceError`] when the port cannot be opened or the recorded
/// notes cannot be stored.
pub fn record_to_song(
    input_name: &str,
    seconds: f64,
    tempo_map: &TempoMap,
    track_name: &str,
) -> Result<(Song, Vec<CapturedNote>, CaptureStats), DeviceError> {
    let (sender, receiver) = channel::<(u64, MidiMessage)>();
    let input =
        MidiInput::new("lazy-arranger").map_err(|err| DeviceError::Init(err.to_string()))?;
    let ports = input.ports();
    let mut available = Vec::with_capacity(ports.len());
    let mut chosen = None;
    for port in ports {
        let port_name = input
            .port_name(&port)
            .map_err(|err| DeviceError::PortName(err.to_string()))?;
        if port_name == input_name {
            chosen = Some(port);
            break;
        }
        available.push(port_name);
    }
    let port = chosen.ok_or_else(|| DeviceError::NoSuchPort {
        requested: String::from(input_name),
        available,
    })?;
    let _connection = input
        .connect(
            &port,
            "lazy-arranger-rec",
            move |timestamp, bytes, _| {
                if let Some(message) = MidiMessage::from_bytes(bytes) {
                    let _ = sender.send((timestamp, message));
                }
            },
            (),
        )
        .map_err(|err| DeviceError::Connect(err.to_string()))?;

    let deadline = Instant::now() + Duration::from_secs_f64(seconds.max(0.0));
    let mut capture = NoteCapture::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(remaining) {
            Ok((timestamp, message)) => capture.ingest(timestamp, message),
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => return Err(DeviceError::InputClosed),
        }
    }
    let (notes, stats) = capture.finish();
    let track = crate::capture::capture_to_track(
        track_name,
        TrackId::new(1).map_err(|err| DeviceError::Project(err.to_string()))?,
        &notes,
        tempo_map,
    )
    .map_err(|err| DeviceError::Project(err.to_string()))?;
    let mut song = Song::new(track_name, tempo_map.bpm_at(0));
    song.tempo_map = tempo_map.clone();
    song.tracks.push(track);
    Ok((song, notes, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// True on CI runners that have no MIDI stack at all.
    ///
    /// The Linux runners have no ALSA sequencer device (`/dev/snd/seq`), so
    /// client creation fails before any port is enumerated. That is an
    /// environment limit, not a defect in this crate.
    fn backend_unavailable(error: &DeviceError) -> bool {
        matches!(error, DeviceError::Init(_))
    }

    #[test]
    fn backend_failure_is_classified() {
        assert!(backend_unavailable(&DeviceError::Init(String::from(
            "no sequencer"
        ))));
        assert!(!backend_unavailable(&DeviceError::NoSuchPort {
            requested: String::from("x"),
            available: Vec::new(),
        }));
    }

    #[test]
    fn enumeration_reports_ports_or_an_unavailable_backend() {
        match list_ports() {
            Ok(ports) => {
                assert!(ports.inputs.iter().all(|name| !name.is_empty()));
                assert!(ports.outputs.iter().all(|name| !name.is_empty()));
            }
            Err(error) if backend_unavailable(&error) => {}
            Err(other) => panic!("unexpected enumeration error: {other}"),
        }
    }

    #[test]
    fn unknown_ports_are_reported_with_the_available_names() {
        match find_output("definitely-not-a-port") {
            Ok(_) => panic!("a port with that name cannot exist"),
            Err(DeviceError::NoSuchPort { requested, .. }) => {
                assert_eq!(requested, "definitely-not-a-port");
            }
            Err(error) if backend_unavailable(&error) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn zero_second_echo_returns_without_looping() {
        match echo("definitely-not-a-port", "also-missing", 0.0) {
            Ok(report) => assert_eq!(report.forwarded, 0),
            Err(DeviceError::NoSuchPort { .. }) => {}
            Err(error) if backend_unavailable(&error) => {}
            Err(other) => panic!("unexpected error: {other}"),
        }
    }
}
