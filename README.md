# Lazy Arranger — Rust

A Rust reimplementation of the Lazy Arranger MIDI arranger DAW. It imports and
exports Yamaha styles and MIDI, hosts VST3/AU plug-ins, renders audio, records
MIDI and audio, and provides a desktop editor, CLI and MCP server.

This is a work in progress. It targets the feature set of the C++ v1.0.0-beta.1
app, but **parity is not claimed**. The audited feature-by-feature status lives
in [`docs/audit/cpp-features-crosscheck.md`](docs/audit/cpp-features-crosscheck.md).

## Requirements

- Rust 1.89 or newer (the repository pins the toolchain through `rust-toolchain.toml`)
- macOS for Audio Units and the desktop bundle; Linux and Windows build the
  desktop shell and all core crates
- Optional: SoundFont 2 banks, VST3/AU plug-ins, `yt-dlp`, `ffmpeg`, `lp`

## Build and run

```sh
# All release gates: fmt, clippy, tests and rustdoc.
./scripts/check.sh

# Debug desktop app.
./scripts/rust.sh run -p lazy-arranger-desktop

# CLI.
./scripts/rust.sh run -p lazy-arranger-cli -- --help

# macOS application bundle (ad-hoc signed, not notarized).
./scripts/rust.sh build --release -p lazy-arranger-desktop
./scripts/bundle-macos.sh
open "dist/Lazy Arranger Midi DAW.app"
```

The first launch opens a built-in demo. Open or import a project with
`Cmd/Ctrl+O`; `.mid`, `.midi`, `.musicxml` and `.xml` import as untitled
projects.

## Workspace

| Path | Purpose |
| --- | --- |
| `apps/desktop` | egui/eframe shell, editors, mixer, score, controllers |
| `apps/cli` | Headless import/export, rendering, hardware checks |
| `crates/project` | Project schema, migrations, validation, recent files |
| `crates/music-core` | Pitch, chord, tempo, MIDI message and timing types |
| `crates/style-formats` | Yamaha SFF1/SFF2, CASM and PSR/PSR-S970 export |
| `crates/audio` | SoundFont/plug-in render, WAV/compressed decode, cpal playback |
| `crates/plugin-host` | The only crate allowed to contain `unsafe` code |
| `crates/mcp` | JSON-RPC tool server over loopback |
| `crates/score` | Vector score layout and PDF; Cecilia adapter |
| `crates/arranging` | Chords, grooves, forms, generated parts, styles from clips |
| `crates/performance` | Recorded performance sections and provenance |

## Verification

```sh
./scripts/rust.sh test --workspace
```

Latest local result: **898 tests pass**. GitHub Actions runs the same lint,
test, build, documentation and headless-demo steps on macOS, Linux and Windows.

Hardware checks and their limits are recorded in
[`docs/audit/hardware.md`](docs/audit/hardware.md). Current verified hardware:
MIDI sends and panic on five outputs, physical note input on Launchkey,
PSR-S970, XioSynth and X-TOUCH, CC capture on Launchkey/X-TOUCH, L6max CH1 mute,
cpal output on Mac mini Speakers and one printed score page on a Brother
printer. D-05 key input and L6max readback/transport remain unverified.

## Documentation

- [Feature crosscheck](docs/audit/cpp-features-crosscheck.md)
- [Architecture contracts](docs/architecture/contracts.md)
- [ADRs](docs/adr/README.md)
- [Hardware evidence](docs/audit/hardware.md)
- [Final status](docs/audit/final-status.md)
- [C++ feature reference](docs/reference/cpp-features.md)

## Licence

Apache-2.0. Third-party crate licences are listed in `Cargo.lock`; see
[`docs/audit/licensing.md`](docs/audit/licensing.md).
