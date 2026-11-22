# Lazy Arranger Rust v2

A second-generation, MIDI-first arranger DAW in Rust. This repository is the
headless foundation: musical primitives, a versioned project schema, a
deterministic sample-clock scheduler, standard MIDI file I/O, MIDI port access
and typed edit commands with undo.

The full product plan, including the UI, arranger styles and device profiles,
is in [`LAZY_ARRANGER_RUST_V2_BLUEPRINT.md`](LAZY_ARRANGER_RUST_V2_BLUEPRINT.md).
Stage 0 findings are in [`docs/audit/`](docs/audit/), the shared type contracts
in [`docs/architecture/contracts.md`](docs/architecture/contracts.md), and the
decisions in [`docs/adr/`](docs/adr/).

## Status

| Area | State | Evidence |
| --- | --- | --- |
| Source audit (C++ / Cadenza / Vibez) | verified | `docs/audit/` |
| Musical primitives, tempo map, 960 PPQ | verified | `crates/music-core` |
| Project schema v1 + legacy C++ import | verified | `crates/project` |
| Sample-clock scheduling | verified | `crates/sequencer` |
| Standard MIDI file read/write | verified | `crates/smf` |
| Typed commands, validation, undo | verified | `crates/app-commands` |
| MIDI port enumeration | verified | `crates/midi-devices` |
| MIDI capture and pairing | verified | `crates/midi-devices` |
| Real-time MIDI dispatch | verified headless, manual on hardware | `crates/midi-devices/src/dispatch.rs` |
| Physical keyboard record/echo/play | manual only | needs a device |
| Chord conversion for arranger styles | verified against the pinned oracle cases | `crates/arranger` |
| Style sections, chord following, transitions | prototype | `crates/arranger/src/style.rs` |
| Yamaha CASM rule import (SFF1/SFF2) | verified on 16,189 real styles | `crates/style-formats/src/casm.rs` |
| Style import: section markers, notes, default rules | verified on 16,158 real styles | `crates/style-formats/src/import.rs` |
| Style writer, setup events and voices | not started | Stage 4 remainder |
| Dual capture: intent + rendered tracks | verified | `crates/performance` |
| Provenance-aware regeneration (keep/replace/duplicate) | verified | `crates/performance` |
| Freeze / detach generated tracks | verified | `performance::freeze` |
| Golden parity, arranger playback in the app, UI, audio engine | not started / blocked | Stage 3 exit needs a C++ build and a CASM style |

## Quick start

```sh
cargo test --workspace

# Render a four-bar phrase through the sample-clock scheduler.
cargo run -q -p lazy-arranger-cli -- demo --out four-bars.mid --verbose

# Show the real-time MIDI schedule for a project without sending anything.
cargo run -q -p lazy-arranger-cli -- play song.json --out-port "My Keyboard" --dry-run

# List MIDI ports.
cargo run -q -p lazy-arranger-cli -- ports

# Record a style section as intent plus rendered tracks, then regenerate.
cargo run -q -p lazy-arranger-cli -- perform "MyStyle.prs" --section "Main A" \
    --chord 0=C --chord 1920=Am --out take.json
cargo run -q -p lazy-arranger-cli -- regenerate take.json --style "MyStyle.prs" \
    --policy replace --out take2.json

# Inspect a Yamaha style, then render a section with chords.
cargo run -q -p lazy-arranger-cli -- style "MyStyle.prs" --section "Main A" --rules
cargo run -q -p lazy-arranger-cli -- style "MyStyle.prs" --section "Main A" \
    --chord 0=C --chord 1920=Am --out main-a.mid

# Import a C++ .arranger project, then render it.
cargo run -q -p lazy-arranger-cli -- import tests/fixtures/legacy-cpp-v3.arranger --out song.json
cargo run -q -p lazy-arranger-cli -- render song.json --out song.mid

# Edit with the typed command path.
cargo run -q -p lazy-arranger-cli -- edit song.json --out edited.json --tempo 96 --chord 0=Cmaj7
```

## Workspace

```text
crates/music-core      pitch, chord, tick, tempo, MIDI messages (no I/O)
crates/project         song schema, validation, legacy C++ import
crates/app-commands    typed commands, validation, undo/redo
crates/sequencer       prepared patterns, sample-clock scheduler
crates/smf             standard MIDI file encode/decode
crates/midi-devices    ports, capture, record, echo, real-time dispatch
crates/style-formats   Yamaha containers, CASM rules, native style model
crates/arranger        chord vocabulary, chord rules, conversion, sections
apps/cli               headless binary
```

Rules the code keeps:

* `music-core` and `project` never depend on a platform, audio or UI crate.
* The scheduler never allocates in `process_block`; the event buffer is fixed
  capacity and overflow is counted.
* Every edit goes through `app-commands`, and an edit that would make the
  project invalid is rolled back.
* Unknown keys in a project file survive a load/save round trip.

## Development environment

The checked-in `scripts/rust.sh` wrapper points `CARGO_HOME` at
`.cargo-home/` inside the repository. That keeps the build self-contained in
sandboxes that forbid writes to `~/.cargo`. Use plain `cargo` when your
environment allows it; CI does.

## Licence

Apache-2.0, as stated in [`LICENSE`](LICENSE). The audit found that the C++
application ships no licence file and that Cadenza and Vibez are
GPL-3.0-or-later; none of their code is copied here. See
[`docs/audit/licensing.md`](docs/audit/licensing.md) for the evidence and the
consequences.
