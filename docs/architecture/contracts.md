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
  preview and approve the same values.
* `ReplaceTrackNotes` and `SetProvenance` are the two commands regeneration
  uses. Both store exact inverses, so a whole regeneration is undoable. Coalescing and approval are ADR-006.
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
* Section boundaries and notes come from the MIDI track
  (`style_formats::import_style`): each section starts at a marker meta event
  whose name matches a `Sdec` entry. Notes are rebased to the section, clipped
  to it, and grouped by source channel. A channel with no rule for that section
  gets the oracle's default rule (`ChordRule::default_for_channel`). The last
  section has no following marker, so its length is rounded up to a whole bar.
* Not imported: program changes and setup events, the `SInt` region before the
  first section, voices, and style writing.

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
| Styles imported end to end | 16,158 |
| Sections | 234,332 |
| Notes | 48,823,435 |
| Rule property checks (identity, limits) | 87,320,457 checks, 0 failures |

The property checks convert every pitch `12..=115` through every parsed rule at
that rule's own source chord. A note must not move when the selected range has
full MIDI limits and is not guitar fingering, and must land inside the limits
when the range can fold. Two rules in the library carry limits above 127; their
results are compared against the MIDI-clamped window.

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
| style import: sections, notes, defaults | `style_formats::import::tests` |
| rule identity and limits over a library | `style-formats/tests/local_library.rs` (opt-in) |
| dual capture of intent and rendered tracks | `performance::tests` |
| staleness and edit detection by digest | `project::provenance::tests` |
| regeneration policies (keep/replace/duplicate) | `performance::tests` |

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

## 16. Performance capture and provenance

Schema version 2 adds `Track.provenance: Option<Provenance>`, and
`ProjectFile::migrate` upgrades version 1 documents by defaulting it to `None`.

`Provenance` records the style id, the section name, the generated range
`start..end`, a digest of the intent in force over that range, and a digest of
the notes as generated (`project::provenance`). The intent digest covers the
meter, the tempo in force and every tempo change up to `end`, the chords that
start in the range, and arranger events in the range; changes outside the range
do not affect it. The notes digest is order- and id-independent, so a manual
edit is detected while renumbering is not.

`track_status` classifies a track as `Manual`, `Fresh`, `Stale`, `Edited` or
`StaleAndEdited`.

`performance::PerformanceRecorder` writes both representations: chord and
style-selection events in the intent lanes, and one track per destination
channel per section, each with provenance for that range.
`performance::regenerate` never touches manual tracks or tracks of another
style, and applies an explicit `Policy`:

| Status | `Keep` (default) | `Replace` | `Duplicate` |
| --- | --- | --- | --- |
| `Fresh` | untouched | untouched | untouched |
| `Stale` | kept, reported | recompiled, provenance refreshed | fresh copy added, old kept |
| `Edited` / `StaleAndEdited` | kept, reported | recompiled, edit discarded | fresh copy added, old kept |

`freeze` clears provenance, which makes a track manual and immune. A track whose
section is missing from the supplied style is reported, not modified.

## 17. Offline audio

`crates/audio` renders a song to interleaved stereo `f32` at a chosen sample
rate and encodes it as 16-bit or 24-bit PCM WAV. Rendering is a pure function of the song
and the tempo map: note start and end ticks become sample frames through
`TempoMap::seconds_at`, so timing follows tempo changes. Each note is an
additive sine voice with two quiet harmonics, a 5 ms attack, a release that
starts 80 ms before the note ends, and a velocity-scaled amplitude; the mix is
clamped into `-1.0..=1.0`.

`export-audio` takes a project, refuses sample rates outside 8000..=192000, and
writes the WAV. It does not open an audio device, so it runs in CI and on
headless machines. A realtime backend will reuse `audio::render`'s voice.

## 18. Audio analysis and smart features

`audio::analysis` is the in-process half of audio-to-MIDI: autocorrelation pitch
detection with a first-local-maximum rule and parabolic refinement, then frame
segmentation into monophonic notes (`DetectedNote`), with `as_ticks` and
`as_pitch` converting results for the project model.

The model-heavy halves stay outside the core, as in the C++ design: stem
separation, polyphonic transcription, instrument tagging and lyrics are produced
by external helpers as JSON and read back. They are not implemented here yet,
and the parity table records them as missing rather than implied.

### YouTube links

`link-to-midi <url> --out song.mid` cleans and validates the link, runs yt-dlp
with `--no-playlist -x --audio-format wav`, parses the `FILE <path>` line,
decodes 16-bit PCM (`audio::wav`), detects monophonic notes
(`audio::analysis`), and writes a 960 PPQ MIDI file at 120 bpm. The app does not
download by itself and does not bundle yt-dlp or ffmpeg; a missing program is
reported, never silently skipped. Downloaded WAVs are deleted unless `--keep`.

## 19. Style writing

`write_casm_sff1` and `write_casm_sff2` emit a `CASM` chunk with one `CSEG` per
group: an `Sdec` with the section names and `Ctab` (26-byte SFF1) or `Ctb2`
(40-byte SFF2) records. SFF2 carries all three ranges and the packed bass bit;
SFF1 keeps its single range with NTT 3 for bass. Names truncate at eight bytes
and the vendor tail is preserved.

`write_style` places the encoded MIDI song first and the CASM chunk after it,
which is the whole Yamaha container. `style-save` rewrote a real 15-section
style and re-read it as SFF2. Voices and setup events are not written yet, and
no Yamaha keyboard has accepted an output here.

### Track programs

`Track.program: Option<u8>` (schema v4) records the chosen General MIDI voice.
`SetTrackProgram` sets or clears it with an exact inverse; the MCP tool
`set_track_program` exposes it; keyboard export uses it first and falls back to
the profile name match. The shell shows `prog N` next to the note count when a
program is set.

## 20. Assistant tools (MCP core)

`crates/mcp` holds the tool registry, JSON schemas and dispatch onto
`EditSession`. Six tools exist: `get_song`, `add_track`, `add_notes`,
`set_chords`, `get_undo`, `undo`. `add_notes` writes a whole batch in one
`ReplaceTrackNotes`, so it is one undo step.

`handle_message` implements JSON-RPC for `initialize`, `ping`, `tools/list` and
`tools/call`; notifications are silent and unknown methods return -32601.
`serve` reads newline-delimited messages and reports parse errors as -32700
without stopping. The `mcp-serve` CLI wraps it and saves the project after each
successful call.

Transport is stdio only here. The C++ app also offers a local socket with a
secret regenerated at each start; that, and the remaining tools, are not built.

### Music Finder MFD

`library::Book::from_mfd` reads the Yamaha `MDB-100-100-3000` layout: 36-byte
header with a big-endian record count at 34..36, then 92-byte records with the
style number at 4..6, meter at 6..8, tempo at 8..10, title at 11..43, genre at
43..59 and keywords at 59..91. Empty titles are skipped; genre and keywords join
with `, `. The path tail after the counted records is ignored. Verified against
the owner's `Nhat.mfd`: 1,653 entries, the same count the C++ documentation
reports.

### MusicXML import

`musicxml::import` reads partwise documents with `roxmltree` (DTD allowed for
the MusicXML doctype). One part becomes one track; measures advance by the
meter found in the first attributes; durations convert from divisions to 960
PPQ ticks; `<rest/>` advances without a note; `<chord/>` notes stack at the
previous note's start; `<unpitched>` becomes a channel-10 snare. Tempo comes
from the first `<sound tempo>`. Key, lyrics, marks, dynamics and ties are not
imported, and drum pitches are not preserved through export because only the
display position is written, so a round trip restores note counts and pitched
material but not drum identities or velocities.

### Keyboard song export

`smf::export_keyboard` writes a format-1 MIDI file with a setup track: track
name, tempo, meter, then a program change (voice), volume (CC 7) and pan
(CC 10) for each part; chord names as `Chord: X` markers and lyrics as lyric
events. Music starts one bar after the setup, drum parts are forced to channel
10, and the report counts tracks, notes, markers, lyrics and the shift.

Voices come from the caller (`--voice CHANNEL:PROGRAM`); there is no lookup in
a device profile yet, so the nearest-voice rule the C++ app applies is not
implemented. No keyboard has been used to confirm playback, and Style Remote,
style-by-number and Beat-pattern export are missing.

### Mix flags

Schema v5 adds `muted`, `soloed` and `armed` to every track, all false and
omitted from JSON when false. `Song::is_audible` applies one rule: if any track
is soloed, only soloed tracks sound; otherwise muted tracks are silent. The
sequencer and the offline renderer filter by it, so a muted track exports as
silence in a WAV; MIDI file export still writes every track because mute is a
mix setting, not a file property. `SetTrackFlags` is one undoable command; the
shell shows M and S toggles in the track row.

### Audio chord recognition

`audio::chords` estimates a 12-bin chroma per segment with a Hann-windowed
Goertzel filter over octaves 2..6, keeps classes above 45% of the peak, and
feeds them to `arranger::recognize`. `chords-from-audio` reports segments and
can write a chord-only project at a given tempo. It works on clean triads and
sevenths; dense mixes, inversions other than the lowest class, and rhythmic
material need the model-based pipeline the C++ app runs outside its process.

### Audio references

Schema v12 adds `Track.audio: Option<AudioRef { path, start, length, offset }>`; `offset` is the in-point and the 11 to 12 migration leaves it zero. The file
stays outside the project, as in the C++ app; `add-audio` reads the WAV header
(`audio::wav::info`), converts seconds to ticks at the tempo at the start tick,
and inserts a track holding the reference. `audio::wav::decode_mono` accepts PCM at 16, 24 and 32 bits and IEEE float at 32
bits (the Zoom L6max records 32-bit float), any channel count, downmixing to
mono. `audio::mix::mix_audio_references` reads the referenced WAVs, resamples them
linearly to the render rate, places each at its tempo-derived start, mixes them
at 90% gain under the synth, and returns any file it could not read. Both the
export and cpal paths call it; the render buffer grows to cover audio that
ends after the last note. There is still no waveform, recording, trimming,
fades or stretching.

### Score PDF

`score::pdf::to_pdf` writes PDF 1.4 from the same `Engraved` layout the canvas
uses: A4 pages, Helvetica for title and page numbers, stroked staff lines,
barlines and stems, filled-circle noteheads, and accidentals drawn as vector
paths (base-14 fonts have no printable sharp or flat). It has no crate dependency and no print dialog; `score-pdf` writes the file.

Ties are detected per pitch when a note ends exactly where the next of the same
pitch starts; the continued note draws no head and both renderers draw the tie.
`engrave_with` places one lyric syllable per note under its staff. Beams,
collision avoidance, tuplets and dragged marks are not implemented.

### Automation

Schema v9 adds `Track.automation`: lanes for volume and pan, each a list of
`(tick, value 0..=127)` points. Values interpolate linearly between points and
hold outside them; a track without a lane uses its static fader and pan.
`Track::volume_at` and `Track::pan_at` answer for a tick, and the synth render
evaluates them at each note start (so automation is per note, not per sample).
`SetAutomation` replaces the lanes as one undoable edit. No lane UI, plugin
parameter automation or MIDI-port automation exists yet.

### Lyrics

Schema v10 adds `Song.lyrics`, a list of `(tick, text)` syllables matched to
notes by tick. MusicXML export writes a `<lyric>` inside the matching note and
import reads it back; the score layout places song lyrics without needing an
explicit list (`engrave_with` can still override them positionally). Dynamics,
articulations and guitar frames are not imported or exported yet.

### Score marks

Schema v11 adds `Song.marks`: a tick plus a dynamic (pp..ff) or an articulation
(staccato, accent, tenuto, marcato, fermata). The layout places dynamics below
the staff and articulations above it; the canvas and PDF draw them as text.
MusicXML export writes a `direction` per mark (a `dynamics` element or a
`words` element) and import reads those back at the measure start. Note-level
`<notations>` articulations, transcriptions of articulations into playback and
guitar frames are not implemented.

### MusicXML ties

Export detects ties per pitch across a track (a note ending exactly where the
next of the same pitch starts) and writes `<tie type="start"/>` on the first
note and `<tie type="stop"/>` on the second. Import merges a stop-tied note
into the matching earlier note, summing durations, so a tied pair survives as
one longer note. `notations`/`tied` elements, ties between different voices and
slurs are not handled.

### Live MIDI input

`midi_devices::devices::LiveInput` opens the port named in Settings and forwards
parsed messages over a channel; the MIDI callback touches no app state. The
shell polls it once per frame, resolves each message through `MidiMap`, and
runs the action: play/stop toggle, undo, scene launch, or a track fader set to
the control-change value. Hardware behaviour is not verified here; a machine
with the named keyboard is needed.

### New assistant tools

`get_automation` and `set_automation` read and replace a track's lanes
(points are sorted and values clamped to 0..127); `get_lyrics` returns lyrics
and score marks; `style_catalog` lists a numbered folder without parsing; and
`theory` answers scale, roman-label, cadence and voice-leading questions. The
tool list is 22 entries; `mcp-call` exercises them from the CLI.

### Synth timbres

`audio::Timbre` selects a voice family from the track: channel 10 or a drum-like
name gives noise percussion, then program ranges (0-7 piano, 16-23 organ,
32-39 bass, 40-55 strings, 88-95 pad), then name hints. Each family has its own
attack, decay and harmonic weights; the demo renders at RMS 0.129. There is
still no SoundFont loading, filter, or metronome click.

### Drum slicing

`audio::drums` finds attacks from a short-time energy envelope compared with
the recent average (narrower than a 50 ms gap is ignored), cuts the buffer at
those points, and maps slices to the first 16 GM drum notes. `drum-slice`
writes one WAV per pad. There is no pad UI, slice editing or time-stretch, and
a pad is not yet wired to the keyboard or the sequencer.

### Waveform and trimming

`audio::waveform::peaks` reduces samples to min/max columns; Arrange draws them
for the selected audio track and offers "Trim 1 beat", which moves start and
in-point together and shortens the reference through `SetAudioRef` (one undo
step). `audio_ref::set_length` changes the span. Recording, monitoring, fades,
takes and stretching remain missing.

### Buses and sends

Schema v13 adds `Song.buses` (volume and pan per bus) and track routing: `Track.bus`
is the destination bus (none means master), and `Track.send` with `send_level`
adds a copy of the track to another bus. The render accumulates each bus's own
buffer, then sums it through the bus fader and pan, so a zero bus fader silences
routed tracks and a bus pan moves them. The Mix view lists bus faders and a
routing picker per track. There is no reverb/chorus processing, no send-level
control in the UI and no master effects.

### Generated parts

`arranging::generate` holds rule-based assistants, not models. `humanize`
nudges starts and velocities by a seeded amount and clamps them; `drumify`
writes a basic kit for every bar containing a chord; `write_part` writes bass,
pad or arpeggio notes from the chord timeline. Each call is one
`ReplaceTrackNotes` edit. The `generate` CLI exposes them. `continue_song` repeats the last bars after the song end, and `melody` writes
seeded eighth notes from chord tones. The `generate` MCP tool exposes all
seven operations. There is no learned generation and no chat or UI for them.

### Synth patches

Schema v14 adds `Track.patch`: waveform (sine, saw, square, triangle), attack,
decay, sustain, release and brightness. The render uses the patch instead of the
timbre's fixed envelope when one is set, with an amplitude envelope computed from
the note length. `SetTrackPatch` sets or clears it as one undo step. There is no
editor UI, per-device panel (D-05, XioSynth, S970 XG, L6max) or sysex dump.

### L6max takes

The L6max records 32-bit float WAV takes. `audio::l6max::scan` reads a folder's
`.wav` files, reports duration, channels, rate, bit depth and whether each is
32-bit float, and lists unreadable files rather than failing. `l6max-import`
adds each take as an audio track at a start tick, with length computed at the
project tempo. Device project files, marker metadata and the L6max mixer page
are not parsed or implemented.

### Local assistant

`mcp::assistant` parses `help`, `song`, `undo` and `tool <name> {json}`, runs the
tool through the same registry the server uses, and returns text, JSON and a
`changed` flag. The shell's Assistant window logs requests and replies and marks
the project dirty only when a call changed it. There is no model, network,
subscription or key handling, and no proposal-card flow.

### Speech output

`apps/desktop/src/speech.rs` cleans reply text (no control characters, 240
characters) and builds `say` arguments, optionally with a voice. The Assistant
window has a "Speak replies" toggle stored as `speech` in settings; non-macOS
hosts no-op. Speech input and any model-based speech (Whisper, Kitten, Kokoro)
are not implemented, and audible output was not verified in this environment.

### Plugin slots

Schema v15 adds `Track.plugins`: slots with an id, name, format (VST3 or Audio
Unit), recorded path, bypass flag and an opaque state string kept verbatim.
`SetPlugins` replaces the list as one undo step, and validation rejects
duplicate ids and warns on empty names. No plugin is loaded, scanned, processed
or given an editor window; the render ignores slots, and host compatibility is
unverified.

### Drum pads

The P-key Drum pads window loads a WAV loop, slices it with `audio::drums`, and
shows a 4x4 grid of the first 16 GM pad notes with their lengths. Clicking a pad
plays its audition buffer through the cpal player. Pads are not triggered by an
external keyboard, slice bounds cannot be edited and there is no time-stretch.

### Score note entry

`score::pitch_at_y` inverts the staff placement and `score::tick_at_x` maps a
click inside a measure to a quarter-quantised tick. With Note entry enabled, a
click in the Score canvas inserts a note on the selected track through the
normal insert command, so it is one undo step. Rests, moving or resizing
existing notes and grand-staff entry are not implemented.

### MCP over loopback TCP

`mcp::socket` binds 127.0.0.1 (port 0 picks a free one), generates a 16-byte hex
token per start, and requires `token` on every JSON-RPC line; a mismatch gets
error -32001 and never reaches a tool. `mcp-socket` writes the token to a file
and saves the project after every successful `tools/call`. There is no TLS,
authentication beyond the token, or concurrent client handling.

### Bounce

`app_commands::bounce::bounce_to_track` renders the song, mixes audio
references, writes a 16-bit WAV and inserts it as an audio track whose span is
derived at the tempo. The insert is one undo step; re-bouncing without muting
the bounce track doubles the mix. There is no input recording, monitoring or
take comping.

### Library browser

The L-key Library window loads a Music Finder book (JSON/CSV/MFD) or a playlist
TSV, searches title and detail case-insensitively, and applies a Music Finder
entry's tempo with `SetTempo` at tick zero (one undo step). Playlist rows are
displayed but not opened. A row with a style can be imported: the window takes
the style library root from Settings or `ARRANGER_STYLE_LIBRARY`, resolves the
number or name, imports the style's decoded song, and inserts its tracks with
fresh ids and a `Style ` prefix (one undo step per track). `copy_tracks` and the missing-file path are tested, and an ignored test imports
the first style when ARRANGER_STYLE_LIBRARY is set; `library-import` did so
against the real library, adding 1 track and 1,926 notes.

### Style resolution

`catalog::resolve` prefers an explicit 1-based number, then an exact
case-insensitive name, then a substring match; `style-resolve` reports the
match. It matched "ABBA Fernando" as entry 1 in the real library. The library
window does not yet call it; importing the resolved style into the project is
still manual through the style commands.

### Keyboard export audit

`smf::keyboard::audit` re-parses an exported file byte by byte and counts track
chunks, program changes, channel volumes and pans, chord markers, lyric events
and note-ons (drum note-ons on channel 10). `export-keyboard` prints the summary;
the demo produced 5 tracks, 4 programs/volumes/pans, 16 chord markers and 128
drum notes. Acceptance by a real PSR-S970 remains unverified.

### MusicXML round-trip audit

`musicxml::audit` compares a song with its re-imported copy: track, note, chord,
lyric and mark counts, naming every loss and classifying note reductions that
tie merging explains. `music-xml-audit` prints it. The demo round trip keeps
chords (16), lyrics (2) and marks (2), with 16 of 22 expected tie merges. Drum notes keep their pitch through an
`<instrument id="P..-I.."/>` reference and their velocity through
`<sound dynamics>`, verified by a round-trip test.
