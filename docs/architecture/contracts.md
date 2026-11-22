# Architecture contracts

Authoritative shared types, ownership and invariants for the Rust workspace.
The blueprint (`LAZY_ARRANGER_RUST_V2_BLUEPRINT.md`, §15) requires one such
file so parallel work cannot invent conflicting domain APIs.

**Change rule.** A change that alters a type, invariant, rounding rule or
serialized field in this document needs an ADR or an explicit review note in
the pull request. Adding a test does not need one.

Every claim below names the file that implements it.

---

## 1. Crate boundaries and dependency direction

```text
music-core          no I/O, no platform, no audio, no UI
  ↑
project             depends on music-core only
  ↑
app-commands        depends on project + music-core
sequencer           depends on project + music-core
smf                 depends on project + music-core
midi-devices        depends on project + music-core (+ midir)
style-formats       depends on nothing but thiserror
  ↑
apps/cli            depends on all of the above
```

Rules:

* Nothing below `apps/` may depend on a UI, audio-device or plugin crate.
* `music-core` must stay usable in a `no_std`-style context; it currently uses
  only `serde` and `thiserror`.
* The real-time path is `sequencer::Scheduler::process_block`. It must not call
  into `project`, `app-commands`, `smf` or `midi-devices`.

Enforced by the Cargo manifests; `cargo clippy --workspace --all-targets` is
the gate.

## 2. Time

| Type | Meaning | Implementation |
| --- | --- | --- |
| `music_core::Tick` = `i64` | edit timeline | `crates/music-core/src/time.rs` |
| `music_core::SamplePosition` = `i64` | playback timeline | same file |
| `music_core::PPQ` = 960 | ticks per quarter note | same file |

Invariants:

* Every project uses 960 PPQ. PPQ is a compile-time constant, not a file field.
  The C++ project file also does not store it (`docs/audit/notes/cpp-song-model.md`).
* Ticks are non-negative in valid documents. Helper functions still accept
  negative ticks and use Euclidean division so boundary math is total.
* `TimeSignature::new` accepts numerator `1..=32` and a power-of-two
  denominator `1..=32`. Schema v1 holds one signature per song.
* Boundary helpers: `beat_boundary_at_or_after`, `bar_boundary_at_or_after`,
  `beat_boundary_after`, `bar_boundary_after`. Whether a transition needs the
  strict or non-strict boundary is decided by the arranger, not here.

### Tick to sample conversion

`TempoMap::samples_at(tick, sample_rate)` computes
`ceil(seconds_at(tick) * sample_rate - 1e-6)`.

This reproduces the C++ oracle exactly
(`src/engine/src/Transport.cpp`, `ProcessContext::sampleForTick`). The epsilon
absorbs floating-point noise so an event that lands exactly on a sample does
not slip one sample later. Rounding up means a note never sounds before its
tick. See ADR-002.

`tick_at_seconds` rounds down so a timestamp never lands after the event it
describes. The two conversions are inverse within one tick; the tests pin that.

Tempo is piecewise constant, not ramped. `TempoMap` rejects an empty map, a
map that does not start at tick 0, and non-ascending changes.

### Deliberate divergences from the C++ oracle

These are decisions, not gaps. Each one is visible to a caller.

| Concern | C++ behaviour | This workspace | Why |
| --- | --- | --- | --- |
| Invalid tempo | clamped to 20..400 bpm, NaN becomes 120 (`src/core/src/Time.cpp`) | rejected with `MusicError::Tempo` | a silent clamp hides a wrong edit; the command layer reports it |
| Tick from seconds | returns a fractional tick (`tickAtSeconds`) | returns a floored integer tick | project ticks are integers; flooring never places an event after its timestamp |
| Unknown project keys | dropped on re-save | preserved in `extra` | silent data loss is the failure mode ADR-004 exists to prevent |
| Invalid edit | silently becomes a no-op | rolled back with a typed error | the user must learn that an edit failed |
| Tempo map storage | point list with precomputed segment starts | sorted change list, seconds computed from absolute positions | same numbers, no incremental drift |

## 3. Pitch and chord

| Type | Invariant | Serde |
| --- | --- | --- |
| `PitchClass` | `0..=11`, C = 0; carries no spelling | integer, validated on load |
| `Pitch` | `0..=127`, 60 = C4 | integer, validated on load |
| `Chord` | root + quality + optional slash bass | `{root, quality, bass?}` |
| `ChordQuality` | one of 15 fixed qualities | kebab-case string |

* Display spelling is chosen at format time (`Spelling::Sharp` or
  `Spelling::Flat`), never stored in the pitch. See
  `Chord::format(Spelling)`.
* `ChordQuality` parsing, display and intervals come from one table
  (`Qualities::TABLE` in `crates/music-core/src/chord.rs`). Adding a quality
  means adding one variant and one row; a unit test walks the table and fails
  if a row cannot round-trip.
* Chord symbols parse root, quality suffix (longest match wins) and slash bass.
  Invalid symbols are errors, not defaults.
* `Chord::tones(octave)` drops tones above MIDI 127; it never panics.

## 4. MIDI messages

`music_core::MidiMessage` covers channel messages only: note on, note off,
control change, program change and pitch bend. SysEx is deliberately excluded
(ADR-009).

* `normalized()` maps note-on velocity 0 to note-off, per the MIDI spec.
* `sort_rank()` is the one ordering policy: note-off `0`, controllers/program/
  bend `1`, note-on `2`.
* `to_bytes()` and `from_bytes()` round-trip every channel message; tests pin
  each status byte.

## 5. Project schema (version 1)

Envelope:

```json
{ "format": "lazy-arranger-project", "version": 1, "song": { ... } }
```

`ProjectFile::from_json` order of checks: JSON shape → `format` tag →
`version` not newer than `SCHEMA_VERSION` → migration → validation.
`version: 0` is invalid; a version below 1 has no migration path and is
rejected with `ProjectError::Migration`.

Song fields: `name`, `tempo_map`, `time_signature`, `tracks`, `chords`,
`arranger`, `markers`, plus unknown keys.

Track fields: `id`, `name`, `notes`, plus unknown keys.
Note fields: `id`, `start`, `duration`, `pitch`, `velocity`, `channel`.
Chord event: `tick`, `chord`. Marker: `tick`, `name`.
Arranger event: tagged enum `select-style`, `select-variation`, `trigger-fill`,
`start`, `stop`; every variant carries `tick`.

Decisions that must not drift:

* **Channel lives on the note**, not on the track. An imported MIDI file can
  put several channels on one track; a drum track is a track whose notes use
  channel 10.
* **Identifiers start at 1** and serialize as numbers. Zero is rejected.
  Track identifiers are unique in a song; note identifiers are unique in a
  track.
* **Unknown keys are preserved** at envelope, song and track level via
  `#[serde(flatten)] BTreeMap<String, Value>`. A future writer never deletes
  data it does not understand.
* Schema version 1 has no clips, audio lanes, automation, provenance or
  plugin state. Those arrive as version 2 with an explicit migration step in
  `ProjectFile::migrate`.

### Validation

`ProjectFile::validate` returns every finding with a JSON-ish path and a
severity (`validate.rs`). Errors block load and edit; warnings do not.

Errors: invalid tempo map, invalid time signature, duplicate track id,
duplicate note id, negative note start, non-positive duration, note end
overflow, duplicate chord tick, unsorted or negative chord/arranger/marker
ticks, and any command result that fails these checks.

Warnings: empty song, unnamed track, unsorted notes, overlapping same-pitch
notes, unsorted markers.

Note that deserialization validates newtypes (`Pitch`, `Velocity`, `Channel`,
`PitchClass`, ids) but not composite validity; `validate` covers the rest.

## 6. Legacy C++ import

`project::legacy::import_cpp_project` is **one-way**. It accepts C++ schema
1..=4, reads `title`, `tempo` (`[[tick, bpm], ...]`), `timeSignature`, and
`tracks[].clips[].notes` tuples `[offset, length, pitch, velocity]`, with the
channel taken from `output.channel` (one-based) and clip offsets turned into
absolute ticks.

Every key it does not interpret is copied into `extra` and listed in the
import report as deferred. The report also counts imported tracks and notes and
records skipped zero-length notes. The importer never claims lossless
conversion, and this repository does not write `.arranger` files (ADR-004).

Verified against `tests/fixtures/legacy-cpp-v3.arranger` (265 notes, 6 tracks)
and `tests/fixtures/legacy-cpp-v4.arranger` (schema 4, 10 named tracks, no
notes). The C++ model stores chords as free text in `ScoreMark.text`; chord
import is therefore deferred and documented as unverified.

## 7. Edit commands

`app_commands::EditSession` is the only mutation path for a project.

* `apply(command)` computes the inverse, mutates, validates, and rolls back by
  restoring the pre-command project if validation fails. Nothing invalid is
  ever observable.
* One command is one undo step. `undo` and `redo` store inverse commands, not
  full snapshots. A `SetTempo` inverse restores the exact `TempoMap`.
* Applying a command clears the redo stack.
* `CommandError` distinguishes unknown targets, duplicates, bad tempo,
  validation failure and empty history.
* The command set is data (`Command` enum), so a future MCP server can log,
  preview and approve the same values. Coalescing and approval are ADR-006.
* The real-time scheduler never executes a command; it consumes a prepared
  snapshot (section 8).

## 8. Sequencing and the real-time boundary

`PreparedPattern::prepare(song, sample_rate)` is the only place that does
tempo-map arithmetic for playback. It produces absolute sample positions for
every note on and note off, sorted by
`(sample, message.sort_rank(), track_id)` with a stable sort.

`Scheduler::process_block(frames, out)` contract:

* Block is `[position, position + frames)`; each prepared event is emitted at
  most once, because the cursor only moves forward.
* Events are never dropped for being late; `ScheduledEvent::offset_in` clamps to
  `0..frames-1`, matching `Engine::offsetFor` in the C++ oracle.
* `EventBuffer` is fixed capacity. `push` returns false and increments
  `overflowed` when full; it never reallocates. `process_block` allocates
  nothing.
* `publish(prepared)` swaps snapshots at a block boundary and repositions the
  cursor by sample. Tempo edits require a republish; they do not retime events
  already compiled.
* `play`, `stop`, `seek`, `seek_tick` change transport state only.
  `finished()` is true once the position passes the last event sample.
* Block size is caller-chosen and provably irrelevant: the integration test
  runs block sizes 1..4096 and compares the full event list.

## 9. Standard MIDI files

`smf::encode` writes format 1 at 960 PPQ: a conductor track (time signature,
tempo changes, markers) plus one track per project track, each starting with a
track-name meta event and ending with end-of-track.

* Within one tick, note-offs are encoded before note-ons (`sort_rank`), so a
  repeated pitch releases before it strikes again.
* `smf::decode` accepts any metrical division and rescales ticks to 960 with
  integer half-up rounding, matching `MidiFile.cpp`. SMPTE-timed files are
  rejected.
* Lossy by design and reported in `DecodeReport`: SysEx and escape events
  skipped, unpaired note-ons dropped, unpaired note-offs ignored, one tempo map
  and one time signature, tracks with no notes not imported.

Round-trip tests pin notes, tempo changes, time signature, markers and track
names.

## 10. MIDI devices and capture

* `list_ports` returns names in backend order. Enumeration is exercised by a
  unit test; physical recording is not.
* `NoteCapture` pairs by `(channel, pitch)` with FIFO order for repeated
  strikes, drops zero-length notes, and counts unmatched events.
* Capture timestamps are microseconds as reported by the backend, not audio
  samples. `to_note` converts through `TempoMap::tick_at_seconds` and forces a
  minimum length of one tick. The result is capture-accurate, not
  sample-accurate; quantisation is a later stage.
* `echo` forwards on the input thread. It is a wiring check, not the real-time
  engine.

## 11. Style formats

`style-formats` proves the Yamaha container layout only: four-byte chunk id
plus big-endian 32-bit length, starting with `MThd`. It reports whether `CASM`
and `CSEG` are present and warns when they are missing.

No CASM parsing, chord conversion, section timing or playback is implemented.
`NativeStyle` exists so the eventual importer has a manufacturer-neutral shape
that keeps the source chunks for lossy-conversion reports. See ADR-003 and
`docs/audit/notes/cpp-style-arranger.md` for the conversion specification.

## 12. Test contract

A feature may be called `verified` in the README only when a named test
exercises it headlessly.

| Claim | Test |
| --- | --- |
| chord parsing, spelling, intervals | `music-core` unit tests |
| tempo map rounding and drift | `tempo::tests` |
| boundary helpers | `time::tests` |
| schema round-trip, unknown keys | `song::tests` |
| validation errors and warnings | `validate::tests` |
| legacy v3/v4 import | `legacy::tests` |
| block-size independence, ordering | `sequencer::tests`, `apps/cli/tests/sprint_exit.rs` |
| SMF round-trip and rescaling | `smf::tests` |
| schedule equals exported file | `sprint_exit::scheduled_events_match_the_exported_midi_file` |
| command undo/redo and rollback | `app-commands` unit tests |
| capture pairing | `capture::tests` |
| container scanning | `style-formats` unit tests |

Hardware record and echo have no automated test. They keep the status
`manual only` until a device-and-loopback run is recorded in
`docs/audit/compatibility-baseline.md`.
