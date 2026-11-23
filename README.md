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
| Project schema v20, migrations, validation | verified | `crates/project` |
| Legacy C++ import | verified on fixtures | `project::legacy` |
| Typed commands, inverse edits, undo/redo | verified | `crates/app-commands` |
| Sample-clock scheduler | verified | `crates/sequencer` |
| Standard MIDI file read/write and keyboard export | verified | `crates/smf` |
| MusicXML export/import, slurs, chords, lyrics, drums | verified round-trip tests | `crates/musicxml` |
| Yamaha style parse/import/CASM rules | verified on 16,158 real styles | `crates/style-formats` |
| Style writer (SFF1/SFF2, 15 sections, programs) | verified round-trip | `style-formats::write_style` |
| Arranger rules, performance capture, freeze | verified | `crates/arranger`, `crates/performance` |
| Grooves, forms, transitions, doctor, fit, theory | verified model + CLI/MCP | `crates/arranging`, UI |
| Score engraving: Cecilia 0.51 (clefs/key/grand staff/PDF) plus legacy vector editor | verified adapter/PDF/PNG tests; captures | `crates/score`, `apps/desktop` |
| Vector score PDF and print via `lp` (printer picker) | PDF verified; print unverified | `score::pdf`, Score view |
| Offline audio render, SF2, cpal playback | rendered tests; audible cpal unverified | `crates/audio` |
| Mixer: buses, sends, pan, automation, master gain/limiter/reverb/chorus, peak/RMS and per-track meters | verified render tests | `audio`, Mix view |
| Stems, freeze, 16/24/32-bit float WAV, gain/normalize | verified | `audio`, CLI |
| Audio references: trim, fades, resample-to-fit, waveform zoom, normalize | verified helpers | `audio`, Editor |
| Drum pads: onset slicing, pad audition, ±10 ms boundary nudge | verified model; player unverified | `audio::drums`, Drum pads window |
| Audio analysis: monophonic pitch, chroma chords, link-to-MIDI | monophonic verified | `audio::analysis`, `link` |
| MIDI profiles, maps, learn, channel check, panic | verified builders; hardware unverified | `crates/midi-devices` |
| L6max take scan/import; Korg recognition | synthetic and real-file scans | `audio::l6max`, `style-formats::korg` |
| Library: JSON/CSV/TSV/MFD, playlists, browser | verified real MFD (1,653 rows) | `crates/library` |
| MCP stdio/socket server, 93 tools | tool/reference tests and CLI runs | `crates/mcp` |
| Desktop shell: Perform, Arrange, Mix, Loops, Score; persistent right inspector and bottom mixer | runs; C++ comparison capture `docs/audit/ui-cpp.png` | `apps/desktop` |
| Light/dark palettes, eight languages with plural rules, Kids Mode, lessons | verified tests; light capture | `theme`, `strings`, `lesson` |
| Plugin manager (VST3/AU metadata + catalog cache) | scan/list/filter/add slots; no binary loading/hosting yet | `crates/plugins`, My Plug-Ins window |
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
crates/project         song schema v20, validation, legacy import, recent files
crates/app-commands    typed commands, inverses, undo/redo
crates/sequencer       prepared patterns, sample-clock scheduler
crates/smf             standard MIDI file encode/decode, keyboard export
crates/musicxml        MusicXML export/import and round-trip audit
crates/style-formats   Yamaha containers, CASM rules, Korg recognition
crates/arranger        chord rules, sections, style conversion
crates/arranging       grooves, forms, doctor, fit, clip grid, beat, demo songs
crates/performance     intent/rendered capture, provenance, freeze
crates/score           engraving, pages/systems, PDF
crates/audio           synth, SF2 (loop crossfade), WAV/FLAC/MP3/M4A/AIFF/OGG decode, mix, buses, automation, analysis, L6max
crates/library         Music Finder books, playlists
crates/midi-devices    profiles, ports, maps, SysEx, dispatch
crates/plugins         VST3/AU metadata, catalog cache and browser (no hosting)
crates/plugin-host     isolated unsafe VST3/AU FFI: class scan + stereo effect processing (slots applied in track render; VST3 state; params/editors pending)
crates/mcp             stdio/socket MCP server, 93 tools, local assistant
apps/cli               headless binary
apps/desktop           egui 0.31 desktop app
```

## Platform notes

- Rust 2024 workspace, MSRV 1.89; `./scripts/rust.sh` sets a workspace-local
  `CARGO_HOME`.
- `cargo fmt`, `clippy -D warnings`, `cargo test --workspace` and rustdoc are
  expected green before a commit.
- Style files are never copied into the repository; validation runs read-only
  against `ARRANGER_STYLE_LIBRARY` when configured.
- The optimized workspace builds and its 648 tests pass; see
  [`docs/audit/release.md`](docs/audit/release.md).

## Remaining parity gaps

- Plugin **hosting/processing/editors** (discovery only).
- Korg PCG program/combi decoding and an L6max project/mixer page (scans/imports
  only).
- SF3 and SoundFont modulators.
- Take comping/selection and destructive waveform editing (input monitoring exists but is hardware-unverified).
- A real AI model, speech input and chat-driven tool use (the assistant is a
  local rule engine over the MCP registry).
- Score key signatures and broader vertical rest collision avoidance (G/F clefs and space-aware dots are implemented).
- Channel-count-preserving WAV edits.
- Hardware acceptance: real MIDI keyboards, PSR-S970, cpal output, printers.

These are tracked with row-level evidence in
[`docs/audit/cpp-features-crosscheck.md`](docs/audit/cpp-features-crosscheck.md).
