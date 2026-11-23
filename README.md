# Lazy Arranger Rust v2

A second-generation, MIDI-first arranger DAW in Rust: a five-view egui desktop
app, a headless CLI, a versioned project format and a 93-tool MCP server.

The product plan is in [`LAZY_ARRANGER_RUST_V2_BLUEPRINT.md`](LAZY_ARRANGER_RUST_V2_BLUEPRINT.md).
The C++ v1 feature crosscheck and evidence live in
[`docs/audit/`](docs/audit/), shared contracts in
[`docs/architecture/contracts.md`](docs/architecture/contracts.md), and decisions
in [`docs/adr/`](docs/adr/). Parity is **partial**, not complete; remaining
blockers are listed at the end.

## Status

| Area | State | Evidence |
| --- | --- | --- |
| Source audit (C++ / Cadenza / Vibez) | verified | `docs/audit/` |
| Musical primitives, tempo map, 960 PPQ | verified | `crates/music-core` |
| Project schema v19, migrations, validation | verified | `crates/project` |
| Legacy C++ import | verified on fixtures | `project::legacy` |
| Typed commands, inverse edits, undo/redo | verified | `crates/app-commands` |
| Sample-clock scheduler | verified | `crates/sequencer` |
| Standard MIDI file read/write and keyboard export | verified | `crates/smf` |
| MusicXML export/import, slurs, chords, lyrics, drums | verified round-trip tests | `crates/musicxml` |
| Yamaha style parse/import/CASM rules | verified on 16,158 real styles | `crates/style-formats` |
| Style writer (SFF1/SFF2, 15 sections) | verified round-trip | `style-formats::write_style` |
| Arranger rules, performance capture, freeze | verified | `crates/arranger`, `crates/performance` |
| Grooves, forms, transitions, doctor, fit, theory | verified model + CLI/MCP | `crates/arranging`, UI |
| Score engraving: ties, beams, tuplets, rests, dots, slurs, percussion, lyrics, chord names, frames, spacing, drag/resize | verified tests; PDF/canvas checks | `crates/score`, `apps/desktop` |
| Vector score PDF and print via `lp` (printer picker) | PDF verified; print unverified | `score::pdf`, Score view |
| Offline audio render, SF2, cpal playback | rendered tests; audible cpal unverified | `crates/audio` |
| Mixer: buses, sends, pan, automation, master gain/limiter | verified render tests | `audio`, Mix view |
| Stems, freeze, 16/24/32-bit float WAV, gain/normalize | verified | `audio`, CLI |
| Audio references: trim, fades, resample-to-fit, waveform zoom, normalize | verified helpers | `audio`, Editor |
| Audio analysis: monophonic pitch, chroma chords, link-to-MIDI | monophonic verified | `audio::analysis`, `link` |
| MIDI profiles, maps, learn, channel check, panic | verified builders; hardware unverified | `crates/midi-devices` |
| L6max take scan/import; Korg recognition | synthetic and real-file scans | `audio::l6max`, `style-formats::korg` |
| Library: JSON/CSV/TSV/MFD, playlists, browser | verified real MFD (1,653 rows) | `crates/library` |
| MCP stdio/socket server, 93 tools | tool/reference tests and CLI runs | `crates/mcp` |
| Desktop shell: Perform, Arrange, Mix, Loops, Score | runs; captures in `docs/audit/` | `apps/desktop` |
| Light/dark palettes, six languages, Kids Mode, lessons | verified tests; light capture | `theme`, `strings`, `lesson` |
| Plugin discovery (VST3/AU) | scan only; no hosting | `crates/plugins` |
| Real MIDI keyboard, PSR-S970, cpal audibility, print, microphone | **unverified** | no hardware here |

## Quick start

```sh
cargo test --workspace

# Desktop app (five views; ARRANGER_VIEW=score opens a view directly).
cargo run -p lazy-arranger-desktop

# Headless demo project, render and inspect.
cargo run -p lazy-arranger-cli -- demo-project --out demo.json
cargo run -p lazy-arranger-cli -- inspect demo.json
cargo run -p lazy-arranger-cli -- export-audio demo.json --out demo.wav --bits 24

# Import/export.
cargo run -p lazy-arranger-cli -- midi-import song.mid --out song.json
cargo run -p lazy-arranger-cli -- export-music-xml song.json --out score.musicxml
cargo run -p lazy-arranger-cli -- score-pdf demo.json --out score.pdf --track 2

# MCP over stdio (also mcp-socket).
cargo run -p lazy-arranger-cli -- mcp-tools
cargo run -p lazy-arranger-cli -- mcp-serve
```

## Workspace

```text
crates/music-core      pitch, chord, tick, tempo, theory, lesson questions
crates/project         song schema v19, validation, legacy import, recent files
crates/app-commands    typed commands, inverses, undo/redo
crates/sequencer       prepared patterns, sample-clock scheduler
crates/smf             standard MIDI file encode/decode, keyboard export
crates/musicxml        MusicXML export/import and round-trip audit
crates/style-formats   Yamaha containers, CASM rules, Korg recognition
crates/arranger        chord rules, sections, style conversion
crates/arranging       grooves, forms, doctor, fit, clip grid, beat, demo songs
crates/performance     intent/rendered capture, provenance, freeze
crates/score           engraving, pages/systems, PDF
crates/audio           synth, SF2, WAV, mix, buses, automation, analysis, L6max
crates/library         Music Finder books, playlists
crates/midi-devices    profiles, ports, maps, SysEx, dispatch
crates/plugins         VST3/AU discovery (no hosting)
crates/mcp             stdio/socket MCP server, 93 tools, local assistant
apps/cli               headless binary
apps/desktop           egui 0.31 desktop app
```

## Platform notes

- Rust 2024 workspace, MSRV 1.88; `./scripts/rust.sh` sets a workspace-local
  `CARGO_HOME`.
- `cargo fmt`, `clippy -D warnings`, `cargo test --workspace` and rustdoc are
  expected green before a commit.
- Style files are never copied into the repository; validation runs read-only
  against `ARRANGER_STYLE_LIBRARY` when configured.

## Remaining parity gaps

- Plugin **hosting/processing/editors** (discovery only).
- Korg PCG program/combi decoding and an L6max project/mixer page (scans/imports
  only).
- SF3 and SoundFont modulators.
- Audio monitoring, take comping/selection and destructive waveform editing.
- A real AI model, speech input and chat-driven tool use (the assistant is a
  local rule engine over the MCP registry).
- Score clefs/key signatures, vertical rest/dot collision avoidance and a full
  print dialog.
- Channel-count-preserving WAV edits.
- Hardware acceptance: real MIDI keyboards, PSR-S970, cpal output, printers.

These are tracked with row-level evidence in
[`docs/audit/cpp-features-crosscheck.md`](docs/audit/cpp-features-crosscheck.md).
