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
| Physical keyboard record/echo | manual only | needs a device |
| Yamaha style import (CASM, chord rules) | not started | `crates/style-formats` scans containers |
| Arranger playback, UI, audio engine | not started | Stage 3+ |

## Quick start

```sh
cargo test --workspace

# Render a four-bar phrase through the sample-clock scheduler.
cargo run -q -p lazy-arranger-cli -- demo --out four-bars.mid --verbose

# List MIDI ports.
cargo run -q -p lazy-arranger-cli -- ports

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
crates/midi-devices    ports, capture, record, echo
crates/style-formats   Yamaha container scanning, native style model
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

MIT OR Apache-2.0. The audit found that the C++ application ships no licence
file and that Cadenza and Vibez are GPL-3.0-or-later; none of their code is
copied here. See [`docs/audit/licensing.md`](docs/audit/licensing.md) for the
evidence and the consequences.
