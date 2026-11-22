# ADR-002: Audio clock ownership, tick/sample conversion and MIDI dispatch

**Status:** Accepted. Implemented for the internal scheduler.

## Context

The C++ oracle runs a JUCE device callback that owns the sample clock. The
transport keeps an anchor `(tick, sample, seconds)` and converts ticks with

```text
sampleForTick(t) = anchorSample + ceil((secondsAt(t) - anchorSeconds) * sampleRate - 1e-6)
```

(`src/engine/src/Transport.cpp`). Scheduling has no look-ahead, and events are
ordered by `(tick, note-off=0, controls=1, note-on=2)` then by track index
(`src/engine/src/Playback.cpp`, `src/engine/src/Engine.cpp`). An event that
computes behind the block start is clamped into the block, never dropped
(`Engine::offsetFor`).

The blueprint requires one deterministic ordering policy and a testable,
platform-free core.

## Decision

1. The **sample count is the master playback clock**. Ticks are the edit
   timeline. The scheduler compiles a song into absolute sample positions on a
   control thread and never recomputes tempo math per block.
2. **Rounding matches the oracle**: ceiling with a `1e-6`-sample epsilon
   (`music_core::tempo::seconds_to_samples`). An event never sounds before its
   tick, and floating-point noise cannot push it a sample late.
3. **Ordering is a type property**, not a caller convention:
   `MidiMessage::sort_rank` returns 0 for note-off, 1 for controllers/program/
   bend, 2 for note-on. Ties break on track id, then stable insertion order.
4. **No look-ahead is not required.** `Scheduler::process_block` emits every
   event in `[position, position + frames)`; a caller may use any block size and
   gets the same event list. The integration test runs sizes 1..4096.
5. **Late events clamp** to `0..frames-1` (`ScheduledEvent::offset_in`), which
   keeps the C++ "late is not dead" behaviour.
6. **Tempo edits republish.** A tempo change compiles a new
   `PreparedPattern` and swaps it at a block boundary; events already played are
   not retimed and not replayed.
7. **External MIDI jitter is a separate problem.** Internal timing is
   sample-exact; physical ports add OS and device latency that this ADR does not
   model. `midi-devices::echo` measures wiring only.
8. **External sync (MTC, Ableton Link, MIDI clock) is out of scope** until
   Stage 7. When added it becomes a clock source that feeds the same anchor,
   not a second scheduler.

## Update: MIDI dispatch worker

Stage 2 added `crates/midi-devices/src/dispatch.rs`: a `MidiSink`, an injected
`Clock`, and `play_timed`, which sends a prepared schedule at absolute
deadlines and finishes with all-notes-off on every channel used. The clock is
injected so tests assert delays without sleeping. Production uses a monotonic
clock and `thread::sleep`, so the accuracy statement in point 7 stands: internal
timing is sample-exact, a physical port is not.

## Consequences

* Tempo ramps need a new curve type in `TempoMap`; the current map is
  piecewise constant.
* A republish costs one allocation-sized copy of the event list on the control
  thread. That is acceptable because it happens at edit rate, not audio rate.
* Device profiles and external instruments cannot be promised sample accuracy.
  The blueprint already forbids that claim.

## Evidence

* `crates/music-core/src/tempo.rs` (`samples_at`, `seconds_to_samples`).
* `crates/sequencer/src/lib.rs` (`PreparedPattern`, `Scheduler`).
* `apps/cli/tests/sprint_exit.rs` (`block_size_never_changes_the_rendered_phrase`,
  `scheduled_events_match_the_exported_midi_file`).
* `docs/audit/notes/cpp-rt-midi.md` §1-§9.
