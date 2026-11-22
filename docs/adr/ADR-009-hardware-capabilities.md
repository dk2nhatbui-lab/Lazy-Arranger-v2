# ADR-009: External hardware capability model and SysEx

**Status:** Accepted for principles. Implementation deferred past Stage 2.

## Context

The C++ app describes devices with YAML profiles: identity, MIDI dialect
(GM/GS/XG banks, drum kits, reset, SysEx), sound lists, controls and
parameters, plus a `capabilities` map where 0 means unknown
(`docs/audit/notes/cpp-surface.md`). Per-byte bank suppression (`-1`),
one-based `programBase`, and section reuse are behaviours a Rust port must keep.

Using a keyboard as a tone generator and remotely controlling its internal
arranger are different capabilities and must not be conflated.

## Decision

1. **Profiles are declarative data, re-authored for this project.** The C++
   YAML files are not copied (no licence permits it); the schema is re-derived
   from behaviour and the public MIDI implementation charts.
2. **The capability map is explicit.** Each entry is a tri-state: supported,
   unsupported, or unknown (0). The UI and the arranger must treat unknown as
   "do not send it and do not promise it".
3. **SysEx never enters the audio callback.** It is owned by the device adapter,
   sent at setup or from the control thread, and kept out of
   `MidiMessage` (contracts §4). A profile that needs SysEx declares it.
4. **Panic paths are mandatory.** All-notes-off and all-sound-off are emitted
   on stop, on port loss and on reconnect; the C++ behaviour
   (`Engine.cpp` CC 64/120/123) is the reference.
5. **Recording works without profiles.** Capture needs only a port and a
   channel; profiles affect sending and labelling, not note recording.
6. **Unverified hardware claims stay unverified.** Yamaha mute-mask polarity,
   reconnect behaviour after unplug, and SysEx acknowledgement are marked
   unverified until a device test records them.

## Consequences

* Stage 3/4 keeps hardware work to enumeration, capture and echo; profiles
  arrive with the mixer milestone.
* `midi-devices` stays small: port listing, capture, record and echo. Sending
  device commands belongs to a future `device-profiles` adapter crate.
* The audit's profile inventory is a specification source, not importable
  content.

## Evidence

* `docs/audit/notes/cpp-surface.md` Scope A, and `cpp-rt-midi.md` §6-§7.
* `crates/midi-devices/src/{devices,capture}.rs`.
* `LAZY_ARRANGER_RUST_V2_BLUEPRINT.md` §7.5.
