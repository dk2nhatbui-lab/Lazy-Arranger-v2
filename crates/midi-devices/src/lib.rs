//! MIDI ports, timestamped capture and recording.
//!
//! Two layers:
//!
//! * [`capture`] is pure: it pairs note-on and note-off messages using their
//!   arrival times and turns them into project notes. It has no device and no
//!   clock, so it is tested exactly.
//! * [`devices`] wraps `midir` to enumerate ports, forward messages and record
//!   from hardware. Hardware behaviour is not covered by CI; see
//!   `docs/audit/compatibility-baseline.md` for what still needs a device.
//!
//! Capture timestamps come from the backend. `midir` reports microseconds since
//! the input was connected, which is the only timestamp the platform gives us;
//! the recorder therefore treats time zero as the moment recording starts, not
//! as a sample-accurate audio clock.

pub mod capture;
pub mod devices;
pub mod dispatch;
pub mod map;
pub mod profiles;
pub mod sysex;

pub use capture::{CaptureStats, CapturedNote, NoteCapture};
pub use devices::{DeviceError, EchoReport, PortNames, echo, list_ports, record_to_song};
pub use dispatch::{
    Clock, MidiSink, PortSink, RealClock, TimedMessage, all_notes_off_messages, play_timed,
    play_to_port, sample_to_micros,
};
pub use profiles::{DeviceProfile, PSR_S970, VoiceSlot, nearest_voice, program_for_track};
