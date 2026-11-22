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
arranger            depends on music-core only
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
* Arranger chord conversion is pure: `arranger::transpose` takes a pitch, a
  rule and a chord and returns a pitch or nothing. It never mutates state. The
  MIDI dispatch worker is the only crate that sleeps on audio time.

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

`style-formats` proves the Yamaha container layout and now reads its chord
rules:

* `scan_chunks` walks four-byte chunk ids with big-endian 32-bit lengths. It
  skips up to three zero padding bytes before an identifier, because some
  writers pad an odd-length chunk; 113 files in the owner's library need this.
* `read_rules` decodes `CASM` → `CSEG` → `Sdec`/`Ctab`/`Ctb2` into
  `arranger::ChordRule`.
  - SFF1 `Ctab` records are 26 bytes; NTT value 3 means "melody, follow slash
    bass".
  - SFF2 `Ctb2` records are 40 bytes with three 6-byte ranges; the NTT byte's
    high bit carries `bass`.
  - Bytes after a record are kept verbatim in `ChordRule::special`.
  - Out-of-range values are normalised the way the oracle behaves
    (`ntr`→root fixed, `ntt`→melody, `rtr`→note generator, source chord type→
    major) and every normalisation is counted in `CasmStats`.
* `read_rules` never fails. Overlong chunk lengths are clamped (the oracle
  clamps too), a trailing fragment is counted in `unparsed_tail_bytes`, and a
  short rule is skipped and counted. Callers see what was recovered, never a
  silent success.
* Section boundaries are still missing: `Sdec` gives names, but the tick
  positions of each section live in the style's MIDI track and are Stage 4 work.

Validation against the owner's library, 2026-11-22 (read-only, no style file
copied into this repository, run with
`ARRANGER_STYLE_LIBRARY="…" cargo test -p style-formats --test local_library`):

| Measure | Value |
| --- | --- |
| Style containers scanned | 16,219 |
| With CASM and parsed rules | 16,189 |
| Without a CASM chunk | 30 |
| CASM but no rules parsed | 0 |
| Rule records read | 856,370 |
| Section groups | 101,433 |
| Files needing recovery | 88 (80 clamped lengths, 26 short rules, 2,954 tail bytes) |
| File versions | SFF1 10,360, SFF2 5,829 |
| Field normalisations | `ntr` 8, `ntt` 8, `rtr` 16, source chord type 3 |

The spot check on a real SFF2 file reproduces an independent byte dump exactly:
`ch 11 Aco.Bass root=0 type=M7 ntr=RootTransposition ntt=Melody bass=true
hk=3 limits=28..54 rtr=PitchShiftToRoot`, and drum rules at 9→10 / 10→9 with
`ntt=Bypass`.

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
| container scanning, padding | `style_formats::tests` |
| CASM rule decoding (SFF1/SFF2, bass bit, tails) | `style_formats::casm::tests` |
| real library coverage | `style-formats/tests/local_library.rs` (opt-in) |

Hardware record and echo have no automated test. They keep the status
`manual only` until a device-and-loopback run is recorded in
`docs/audit/compatibility-baseline.md`.

## 13. Arranger: chord rules, conversion and sections

Implemented in `crates/arranger`. The algorithm is a transcription of the C++
oracle's `StyleChord.cpp`, specified in
`docs/audit/notes/cpp-style-arranger.md` §3.

Vocabulary: `ChordType` has exactly the 35 Yamaha types in file order
(`ALL_TYPES`), each with a suffix, third/fifth/seventh intervals and a
seven-note scale (`chord_type::TABLE`). `ChordType::from_index` rejects numbers
above 34 instead of clamping them, because the oracle's clamp hides corrupt
input.

Rules: `ChordRule` mirrors `StyleChannelRule` with three note ranges
(`low_range`, `transpose`, `high_range`) selected by strict comparisons against
`middle_low`/`middle_high`. Defaults match the oracle: `ntr =
RootTransposition`, `ntt = Chord`, `high_key = 6`, limits `0..127`,
`note_mute = 0x0fff`, `chord_mute = 0xff_ffff_ffff` (all 40 bits set).

`arranger::transpose(pitch, rule, chord) -> Option<u8>` runs the eight steps in
order: range selection, bypass pass, no-chord test, target root (slash bass
only when the rule sets `bass`), guitar mode, source geometry, interval
mapping (chord tones before scale degrees; melody rejects an interval of 12),
root placement (transposing or nearest-note with ties down), then octave
folding inside the limits and a clamp to `0..=127`.

`arranger::plays(rule, chord)` is the separate part gate: a cancel chord plays
only when bit 34 of `chord_mute` is set; otherwise the root bit of `note_mute`
and the chord-type bit of `chord_mute` both have to be set. A false gate clears
the whole part.

`Style::compile(section, start_tick, chords)` splits a section at chord
changes, gates each part per segment, converts each note, and cuts a note at a
segment boundary or at the section end. Output is sorted by
`(tick, channel, pitch)`. The cut matches the oracle's behaviour of stopping
notes when the chord changes. Notes are not retriggered across a change.

`TransitionState` owns only *when* a section request takes effect:
`Immediate` at the request tick, `NextBeat`/`NextBar` at the meter's next
boundary at or after it, `EndOfSection` at the caller-supplied section end.
When several requests are due at one tick the latest wins; a fill is an
ordinary request, so returning from one is explicit.

Status: the conversion is verified against the cases the C++ test suite pins
(`StyleFileTests.cpp:173-238`). Section compilation has synthetic-style tests
only. Golden parity needs a C++ build and a real CASM style (ADR-008), so the
arranger is `prototype`, never `parity complete`.

## 14. MIDI dispatch

Implemented in `crates/midi-devices/src/dispatch.rs`.

* A `MidiSink` accepts one `MidiMessage` at a time; `PortSink` wraps a `midir`
  output connection.
* A `TimedMessage` is a message plus an offset in microseconds from playback
  start. `sample_to_micros` converts sample positions with nearest-microsecond
  rounding.
* `play_timed(messages, sink, clock)` waits on an injected `Clock` until each
  message's deadline, then sends. Production uses `RealClock`; tests use a
  logical clock that records waits, so ordering and delays are asserted without
  sleeping.
* The schedule must be sorted by offset. An unsorted list is rejected before
  any message is sent.
* `play_to_port` finishes by sending all-notes-off on every channel the
  schedule used, one millisecond after the last message. A stopped or finished
  playback must never leave a note held.
* Accuracy is OS timer plus port latency. This module does not make a physical
  port sample-accurate and must not be described as if it did.

## 15. What verification currently covers

| Claim | Test |
| --- | --- |
| 35 chord types, tones, names | `arranger::chord_type::tests` |
| transposition pinned to the C++ cases | `arranger::transpose::tests` |
| part gating and mute masks | `arranger::transpose::tests` |
| section splitting, cutting, gating | `arranger::style::tests` |
| transition boundaries and policy | `arranger::transition::tests` |
| dispatch ordering, delays, panic | `midi_devices::dispatch::tests` |

Hardware playback (`play` against a real port) stays manual. The command exists
and prints its schedule with `--dry-run`; no automated test sends MIDI to a
device.
