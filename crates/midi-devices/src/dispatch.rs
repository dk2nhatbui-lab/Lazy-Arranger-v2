//! Real-time MIDI dispatch: send a prepared schedule through a port on time.
//!
//! The blueprint separates the sample clock from the MIDI dispatch worker
//! because physical ports add OS latency that the internal schedule cannot
//! model. This worker is the MIDI side of that split: it takes messages with
//! offsets in microseconds and sends them through a [`MidiSink`], waiting with
//! an injectable [`Clock`].
//!
//! Injecting the clock is what makes timing testable without hardware: tests
//! advance a logical clock and record the waits, so the exact delay between
//! messages is asserted rather than slept through.
//!
//! Accuracy: the wait is a plain `thread::sleep` against an absolute deadline,
//! so jitter is the OS timer plus port latency. Do not claim sample accuracy
//! for a physical port on the strength of this module.

use std::time::{Duration, Instant};

use midir::MidiOutputConnection;
use music_core::MidiMessage;

use crate::DeviceError;

/// A destination that accepts one channel message at a time.
pub trait MidiSink {
    /// Sends one message now.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::Send`] when the port refuses the message.
    fn send(&mut self, message: MidiMessage) -> Result<(), DeviceError>;
}

/// A message with an offset from the start of playback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimedMessage {
    /// Offset in microseconds from playback start.
    pub at_micros: u64,
    /// The message to send.
    pub message: MidiMessage,
}

/// A clock the dispatcher waits on.
pub trait Clock {
    /// Time since the clock's origin.
    fn now(&mut self) -> Duration;
    /// Blocks for at least `duration`.
    fn sleep(&mut self, duration: Duration);
}

/// A clock backed by the monotonic system clock.
#[derive(Clone, Copy, Debug)]
pub struct RealClock {
    origin: Instant,
}

impl Default for RealClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Clock for RealClock {
    fn now(&mut self) -> Duration {
        self.origin.elapsed()
    }

    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// A MIDI output port used as a sink.
pub struct PortSink {
    connection: MidiOutputConnection,
}

impl PortSink {
    /// Opens a named output port.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError`] when the backend or the port is unavailable.
    pub fn open(name: &str) -> Result<Self, DeviceError> {
        Ok(Self {
            connection: crate::devices::open_output(name)?,
        })
    }
}

impl std::fmt::Debug for PortSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PortSink { connection: MidiOutputConnection }")
    }
}

impl MidiSink for PortSink {
    fn send(&mut self, message: MidiMessage) -> Result<(), DeviceError> {
        let bytes = message.to_bytes();
        self.connection
            .send(&bytes[..message.encoded_len()])
            .map_err(|error| DeviceError::Send(error.to_string()))
    }
}

/// Converts a sample position to microseconds, rounding to the nearest
/// microsecond.
#[must_use]
pub fn sample_to_micros(sample: i64, sample_rate: u32) -> u64 {
    if sample_rate == 0 || sample <= 0 {
        return 0;
    }
    let numerator = u128::try_from(sample).unwrap_or(0) * 1_000_000;
    let denominator = u128::from(sample_rate);
    let rounded = (numerator + denominator / 2) / denominator;
    u64::try_from(rounded).unwrap_or(u64::MAX)
}

/// Sends every message at its deadline.
///
/// The messages must be sorted by `at_micros`; equal timestamps are allowed and
/// are sent in list order.
///
/// # Errors
///
/// Returns [`DeviceError::ScheduleOrder`] for an unsorted list and propagates
/// [`MidiSink`] errors, which stop playback.
pub fn play_timed(
    messages: &[TimedMessage],
    sink: &mut impl MidiSink,
    clock: &mut impl Clock,
) -> Result<usize, DeviceError> {
    // Check the whole schedule first, so a bad one sends nothing at all.
    if let Some(pair) = messages
        .windows(2)
        .find(|pair| pair[1].at_micros < pair[0].at_micros)
    {
        return Err(DeviceError::ScheduleOrder {
            at: pair[1].at_micros,
            previous: pair[0].at_micros,
        });
    }
    let start = clock.now();
    let mut sent = 0usize;
    for message in messages {
        let target = start + Duration::from_micros(message.at_micros);
        let now = clock.now();
        if target > now {
            clock.sleep(target - now);
        }
        sink.send(message.message)?;
        sent += 1;
    }
    Ok(sent)
}

/// Builds all-notes-off messages for the channels a schedule uses.
///
/// The panic messages are queued one millisecond after the last scheduled
/// message so a stopped or finished playback never leaves a note held.
#[must_use]
pub fn all_notes_off_messages(messages: &[TimedMessage], last_micros: u64) -> Vec<TimedMessage> {
    let mut channels: Vec<music_core::Channel> = messages
        .iter()
        .map(|message| message.message.channel())
        .collect();
    channels.sort_unstable();
    channels.dedup();
    channels
        .into_iter()
        .map(|channel| TimedMessage {
            at_micros: last_micros.saturating_add(1_000),
            message: MidiMessage::ControlChange {
                channel,
                controller: music_core::ControlNumber::all_notes_off(),
                value: 0,
            },
        })
        .collect()
}

/// Plays a schedule through a named port with the real clock, then panics
/// every channel it used.
///
/// # Errors
///
/// Returns [`DeviceError`] when the port cannot be opened, the schedule is not
/// sorted or a send fails.
pub fn play_to_port(port_name: &str, messages: &[TimedMessage]) -> Result<usize, DeviceError> {
    let mut sink = PortSink::open(port_name)?;
    let mut clock = RealClock::default();
    play_with(messages, &mut sink, &mut clock)
}

fn play_with(
    messages: &[TimedMessage],
    sink: &mut impl MidiSink,
    clock: &mut impl Clock,
) -> Result<usize, DeviceError> {
    let played = play_timed(messages, sink, clock);
    // The second schedule starts when the first one ends, so its timestamps
    // are relative to that instant. One millisecond after the last note is
    // enough to release every channel without replaying the song duration.
    // It runs even when playback failed part way, so no note is left held.
    let panic = all_notes_off_messages(messages, 0);
    let released = play_timed(&panic, sink, clock);
    let sent = played?;
    released?;
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, Pitch, Velocity};

    /// Clock that advances only when the dispatcher sleeps.
    #[derive(Debug, Default)]
    struct LogicalClock {
        now: Duration,
        sleeps: Vec<Duration>,
    }

    impl Clock for LogicalClock {
        fn now(&mut self) -> Duration {
            self.now
        }

        fn sleep(&mut self, duration: Duration) {
            self.sleeps.push(duration);
            self.now += duration;
        }
    }

    /// Sink that records what it was asked to send.
    #[derive(Debug, Default)]
    struct RecordingSink {
        sent: Vec<MidiMessage>,
        fail_after: Option<usize>,
    }

    impl MidiSink for RecordingSink {
        fn send(&mut self, message: MidiMessage) -> Result<(), DeviceError> {
            if let Some(limit) = self.fail_after
                && self.sent.len() >= limit
            {
                return Err(DeviceError::Send(String::from("sink refused")));
            }
            self.sent.push(message);
            Ok(())
        }
    }

    fn note_on(pitch: &str) -> MidiMessage {
        MidiMessage::note_on(
            Channel::first(),
            Pitch::from_name(pitch).expect("valid pitch"),
            Velocity::new(100).expect("valid velocity"),
        )
    }

    #[test]
    fn sends_messages_at_their_offsets() {
        let messages = [
            TimedMessage {
                at_micros: 0,
                message: note_on("C4"),
            },
            TimedMessage {
                at_micros: 500_000,
                message: note_on("E4"),
            },
            TimedMessage {
                at_micros: 1_000_000,
                message: note_on("G4"),
            },
        ];
        let mut sink = RecordingSink::default();
        let mut clock = LogicalClock::default();
        let sent = play_timed(&messages, &mut sink, &mut clock).expect("plays");
        assert_eq!(sent, 3);
        assert_eq!(
            clock.sleeps,
            vec![Duration::from_millis(500), Duration::from_millis(500)]
        );
        assert_eq!(sink.sent.len(), 3);
        assert_eq!(clock.now, Duration::from_millis(1000));
    }

    #[test]
    fn all_notes_off_comes_one_millisecond_after_the_schedule() {
        let messages = [TimedMessage {
            at_micros: 1_000_000,
            message: note_on("C4"),
        }];
        let mut sink = RecordingSink::default();
        let mut clock = LogicalClock::default();
        play_with(&messages, &mut sink, &mut clock).expect("plays");
        assert_eq!(clock.now, Duration::from_micros(1_001_000));
        assert_eq!(sink.sent.len(), 2);
        assert!(matches!(sink.sent[1], MidiMessage::ControlChange { .. }));
    }

    #[test]
    fn equal_timestamps_do_not_wait_between_messages() {
        let messages = [
            TimedMessage {
                at_micros: 250,
                message: note_on("C4"),
            },
            TimedMessage {
                at_micros: 250,
                message: note_on("E4"),
            },
        ];
        let mut sink = RecordingSink::default();
        let mut clock = LogicalClock::default();
        play_timed(&messages, &mut sink, &mut clock).expect("plays");
        assert_eq!(clock.sleeps, vec![Duration::from_micros(250)]);
        assert_eq!(sink.sent.len(), 2);
    }

    #[test]
    fn unsorted_messages_are_rejected_before_playback() {
        let messages = [
            TimedMessage {
                at_micros: 1000,
                message: note_on("C4"),
            },
            TimedMessage {
                at_micros: 500,
                message: note_on("E4"),
            },
        ];
        let mut sink = RecordingSink::default();
        let mut clock = LogicalClock::default();
        let error = play_timed(&messages, &mut sink, &mut clock).expect_err("unsorted");
        assert_eq!(
            error,
            DeviceError::ScheduleOrder {
                at: 500,
                previous: 1000
            }
        );
    }

    #[test]
    fn a_sink_error_stops_playback() {
        let messages = [
            TimedMessage {
                at_micros: 0,
                message: note_on("C4"),
            },
            TimedMessage {
                at_micros: 500_000,
                message: note_on("E4"),
            },
        ];
        let mut sink = RecordingSink {
            fail_after: Some(1),
            ..RecordingSink::default()
        };
        let mut clock = LogicalClock::default();
        let error = play_timed(&messages, &mut sink, &mut clock).expect_err("sink fails");
        assert!(matches!(error, DeviceError::Send(_)));
        assert_eq!(sink.sent.len(), 1);
        assert_eq!(clock.now, Duration::from_millis(500));
    }

    #[test]
    fn sample_conversion_rounds_to_the_nearest_microsecond() {
        assert_eq!(sample_to_micros(0, 48_000), 0);
        assert_eq!(sample_to_micros(24_000, 48_000), 500_000);
        assert_eq!(sample_to_micros(1, 48_000), 21);
        assert_eq!(sample_to_micros(2, 44_100), 45);
        assert_eq!(
            sample_to_micros(-5, 48_000),
            0,
            "a position before the start is clamped"
        );
        assert_eq!(sample_to_micros(10, 0), 0);
    }

    #[test]
    fn panic_messages_cover_each_channel_once() {
        let first = Channel::first();
        let drums = Channel::drums();
        let messages = [
            TimedMessage {
                at_micros: 0,
                message: MidiMessage::note_on(
                    first,
                    Pitch::from_name("C4").expect("valid"),
                    Velocity::new(100).expect("valid"),
                ),
            },
            TimedMessage {
                at_micros: 0,
                message: MidiMessage::note_on(
                    drums,
                    Pitch::from_name("C2").expect("valid"),
                    Velocity::new(100).expect("valid"),
                ),
            },
            TimedMessage {
                at_micros: 500_000,
                message: MidiMessage::note_off(
                    first,
                    Pitch::from_name("C4").expect("valid"),
                    Velocity::new(0).expect("valid"),
                ),
            },
        ];
        let panic = all_notes_off_messages(&messages, 500_000);
        assert_eq!(panic.len(), 2, "one message per channel, not per event");
        assert_eq!(panic[0].at_micros, 501_000);
        assert!(panic.iter().all(|timed| matches!(
            timed.message,
            MidiMessage::ControlChange { controller, .. }
                if controller == music_core::ControlNumber::all_notes_off()
        )));
        let channels: Vec<Channel> = panic.iter().map(|timed| timed.message.channel()).collect();
        assert_eq!(channels, vec![first, drums]);
    }

    #[test]
    fn an_empty_schedule_sends_nothing() {
        let mut sink = RecordingSink::default();
        let mut clock = LogicalClock::default();
        assert_eq!(play_timed(&[], &mut sink, &mut clock).expect("plays"), 0);
        assert!(clock.sleeps.is_empty());
    }
}
