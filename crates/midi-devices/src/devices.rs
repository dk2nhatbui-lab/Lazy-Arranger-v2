//! `midir` port access: enumeration, echo and recording.
//!
//! Everything in this module touches the operating system's MIDI stack. CI
//! runs it on a machine with no attached hardware, so the tests here only prove
//! that enumeration does not fail. Recording and echo against a real keyboard
//! are manual checks; see `docs/audit/compatibility-baseline.md`.

use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use music_core::{MidiMessage, TempoMap};
use project::ids::TrackId;
use project::song::Song;
use thiserror::Error;

use crate::capture::{CaptureStats, CapturedNote, NoteCapture};

/// Errors raised by port access.
#[derive(Debug, Error, PartialEq, Eq)]
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
    /// A dispatch schedule was not sorted by time.
    #[error("dispatch schedule goes back in time: {at} us after {previous} us")]
    ScheduleOrder {
        /// The offending offset.
        at: u64,
        /// The offset before it.
        previous: u64,
    },
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

/// A live input connection that the UI polls each frame.
///
/// The callback only parses and forwards messages, so no app state is touched
/// on the MIDI thread.
pub struct LiveInput {
    /// Open connection; dropping it closes the port.
    pub connection: MidiInputConnection<()>,
    receiver: Receiver<MidiMessage>,
}

impl std::fmt::Debug for LiveInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("LiveInput")
    }
}

impl LiveInput {
    /// Opens a named input port.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError`] when the backend or the port is unavailable.
    pub fn open(name: &str) -> Result<Self, DeviceError> {
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
            if port_name == name {
                chosen = Some(port);
                break;
            }
            available.push(port_name);
        }
        let port = chosen.ok_or_else(|| DeviceError::NoSuchPort {
            requested: String::from(name),
            available,
        })?;
        let connection = input
            .connect(
                &port,
                "lazy-arranger-live",
                move |_timestamp, bytes, _| {
                    if let Some(message) = MidiMessage::from_bytes(bytes) {
                        let _ = sender.send(message);
                    }
                },
                (),
            )
            .map_err(|err| DeviceError::Connect(err.to_string()))?;
        Ok(Self {
            connection,
            receiver,
        })
    }

    /// The next pending message, if any.
    #[must_use]
    pub fn next(&self) -> Option<MidiMessage> {
        self.receiver.try_recv().ok()
    }
}

/// Several live input connections polled together.
pub struct LiveInputs {
    inputs: Vec<LiveInput>,
}

impl std::fmt::Debug for LiveInputs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LiveInputs")
            .field("count", &self.inputs.len())
            .finish()
    }
}

impl LiveInputs {
    /// Wraps one named input.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError`] when the input cannot be opened.
    pub fn open_named(name: &str) -> Result<Self, DeviceError> {
        Ok(Self {
            inputs: vec![LiveInput::open(name)?],
        })
    }

    /// Opens every input port that can be opened.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError`] when the backend or enumeration fails.
    pub fn open_all() -> Result<Self, DeviceError> {
        let names = list_ports()?.inputs;
        let mut inputs = Vec::new();
        for name in names {
            if let Ok(input) = LiveInput::open(&name) {
                inputs.push(input);
            }
        }
        Ok(Self { inputs })
    }

    /// Number of open inputs.
    #[must_use]
    pub fn count(&self) -> usize {
        self.inputs.len()
    }

    /// The next pending message from any input.
    #[must_use]
    pub fn next(&self) -> Option<MidiMessage> {
        self.inputs.iter().find_map(LiveInput::next)
    }
}

/// Builds the note-on and note-off bytes for a channel check.
///
/// # Errors
///
/// Returns an error when the channel is above 15 or the note is above 127.
pub fn channel_check_messages(channel: u8, note: u8) -> Result<[Vec<u8>; 2], DeviceError> {
    if channel > 15 {
        return Err(DeviceError::Init(String::from("channel must be 0..=15")));
    }
    if note > 127 {
        return Err(DeviceError::Init(String::from("note must be 0..=127")));
    }
    Ok([
        vec![0x90 | channel, note, 100],
        vec![0x80 | channel, note, 0],
    ])
}

/// Sends a short note on one channel so the player can find where it sounds.
///
/// # Errors
///
/// Returns [`DeviceError`] when the output port is missing or the send fails.
pub fn channel_check(
    output_name: &str,
    channel: u8,
    note: u8,
    seconds: f64,
) -> Result<[Vec<u8>; 2], DeviceError> {
    let messages = channel_check_messages(channel, note)?;
    let mut output = open_output(output_name)?;
    output
        .send(&messages[0])
        .map_err(|err| DeviceError::Connect(err.to_string()))?;
    std::thread::sleep(std::time::Duration::from_secs_f64(
        seconds.clamp(0.05, 10.0),
    ));
    output
        .send(&messages[1])
        .map_err(|err| DeviceError::Connect(err.to_string()))?;
    Ok(messages)
}

/// Control-change 123 on every channel, in channel order.
#[must_use]
pub fn panic_messages() -> [[u8; 3]; 16] {
    let mut messages = [[0_u8; 3]; 16];
    for (channel, message) in messages.iter_mut().enumerate() {
        *message = [0xb0 | u8::try_from(channel).unwrap_or(0), 123, 0];
    }
    messages
}

/// Sends all-notes-off on every channel to one output.
///
/// # Errors
///
/// Returns [`DeviceError`] when the output port is missing or a send fails.
pub fn panic(output_name: &str) -> Result<usize, DeviceError> {
    let mut output = open_output(output_name)?;
    let messages = panic_messages();
    for message in messages {
        output
            .send(&message)
            .map_err(|error| DeviceError::Connect(error.to_string()))?;
    }
    Ok(messages.len())
}

/// Opens a named output port.
///
/// # Errors
///
/// Returns [`DeviceError`] when the MIDI backend or the port is unavailable.
pub fn open_output(name: &str) -> Result<MidiOutputConnection, DeviceError> {
    find_output(name)
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
    record_to_song_after(input_name, seconds, tempo_map, track_name, false)
}

/// Records notes, optionally starting the window on the first note-on.
///
/// With `wait_for_note`, the function waits up to one hour for a note-on and
/// then records for `seconds`.
///
/// # Errors
///
/// Returns [`DeviceError`] when the port cannot be opened or the recorded
/// notes cannot be stored.
pub fn record_to_song_after(
    input_name: &str,
    seconds: f64,
    tempo_map: &TempoMap,
    track_name: &str,
    wait_for_note: bool,
) -> Result<(Song, Vec<CapturedNote>, CaptureStats), DeviceError> {
    if !seconds.is_finite() {
        return Err(DeviceError::Project(String::from(
            "recording length must be finite",
        )));
    }
    let seconds = seconds.clamp(0.0, 86_400.0);
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

    let wait_deadline = Instant::now() + Duration::from_secs(3600);
    let mut deadline = if wait_for_note {
        wait_deadline
    } else {
        Instant::now() + Duration::from_secs_f64(seconds.max(0.0))
    };
    let mut capture = NoteCapture::new();
    let mut waiting = wait_for_note;
    let mut origin = None;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match receiver.recv_timeout(remaining) {
            Ok((timestamp, message)) => {
                if waiting {
                    if matches!(message, MidiMessage::NoteOn { velocity, .. } if velocity.as_u8() > 0)
                    {
                        waiting = false;
                        origin = Some(timestamp);
                        deadline = Instant::now() + Duration::from_secs_f64(seconds);
                    } else {
                        continue;
                    }
                }
                let origin = *origin.get_or_insert(timestamp);
                capture.ingest(timestamp.saturating_sub(origin), message);
            }
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

    #[test]
    fn opening_all_inputs_does_not_panic() {
        let _ = LiveInputs::open_all();
    }

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

#[cfg(test)]
mod channel_check_tests {
    use super::*;

    #[test]
    fn channel_messages_encode_channel_and_note() {
        let messages = channel_check_messages(3, 60).expect("messages");
        assert_eq!(messages[0], vec![0x93, 60, 100]);
        assert_eq!(messages[1], vec![0x83, 60, 0]);
        assert!(channel_check_messages(16, 60).is_err());
        assert!(channel_check_messages(0, 200).is_err());
    }
}

#[cfg(test)]
mod panic_tests {
    use super::*;

    #[test]
    fn panic_covers_every_channel_with_cc_123() {
        let messages = panic_messages();
        assert_eq!(messages.len(), 16);
        for (channel, message) in messages.iter().enumerate() {
            assert_eq!(message[0] & 0xf0, 0xb0);
            assert_eq!(usize::from(message[0] & 0x0f), channel);
            assert_eq!(message[1], 123);
            assert_eq!(message[2], 0);
        }
    }
}
