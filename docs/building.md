# Building Lazy Arranger from source

This page is for developers. For how to use the app, see the
[user manual](manual/user-manual.md).

## Making a release

1. Set `version` in the root `Cargo.toml` (for example `0.1.0-beta.2`) and add
   a `## [0.1.0-beta.2]` section to `CHANGELOG.md`. Merge to `main`.
2. Push the tag `v0.1.0-beta.2`, or run the **Release** workflow from the
   Actions tab with that tag.
3. The workflow builds all three downloads and publishes the GitHub release.
   It is marked as the latest release. Tick **prerelease** in Run workflow
   for a test build that should not be marked latest.

## Requirements (building from source)

- Rust 1.89 or newer (MSRV declared in `Cargo.toml` via `rust-version = "1.89"`)
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
# UNIVERSAL=1 builds for Apple Silicon and Intel.
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

Latest local result: **923 tests pass**. GitHub Actions runs the same lint,
test, build, documentation and headless-demo steps on macOS, Linux and Windows.

Hardware checks and their limits are recorded in
[`audit/hardware.md`](audit/hardware.md). Current verified hardware:
MIDI sends and panic on five outputs, physical note input on Launchkey,
PSR-S970, XioSynth and X-TOUCH, CC capture on Launchkey/X-TOUCH, L6max CH1 mute,
L6max Editor Port SysEx identity (`v0110`) and 48 kHz USB audio capture,
Roland D-05 SysEx temporary-patch read/write (448 B / `.syx`) and Internal/Preset
bank name scans, cpal output on Mac mini Speakers and one printed score page on
a Brother printer. D-05 key/audio output, XioSynth sound page, and L6max
project/mixer readback and physical transport reaction remain unverified.

