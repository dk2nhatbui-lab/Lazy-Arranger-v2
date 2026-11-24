# Release build verification (2026-11-24)

Environment: macOS arm64, rustc 1.100.0 (Homebrew), updated after Roland D-05 SysEx editor, Zoom L6max live probe, macOS native VST3/AU editor windows, Unicode/CJK PDF font embedding, and local offline speech STT/TTS.

## Build

```sh
./scripts/bundle-macos.sh
# Finished `release` profile [optimized]
# built dist/Lazy Arranger Midi DAW.app
```

The only warning is Cargo's future-incompatibility notice for `block v0.1.6`, a
transitive dependency (not a project lint failure). `cargo clippy -D warnings`
stays green on the workspace.

## Tests

```sh
./scripts/check.sh
# 923 passed, 0 failed (fmt, clippy -D warnings, tests, rustdoc -D warnings)
```

## Binaries

| Binary | Size | Smoke |
| --- | --- | --- |
| `dist/Lazy Arranger Midi DAW.app/Contents/MacOS/lazy-arranger-desktop` | 24 MB | release bundle builds and signs cleanly |
| `dist/Lazy Arranger Midi DAW.app/Contents/MacOS/lazy-arranger-plugin-scan` | 467 KB | returned R0Verb 1.0.4 Fx metadata from `/Library/Audio/Plug-Ins/VST3/R0Verb.vst3` |

The debug workspace gate (923 tests, clippy `-D warnings`, fmt, rustdoc) and the
Windows/Linux/macOS CI matrix remain the authoritative checks. Live hardware
verification on powered Roland D-05 and Zoom L6max plus main-thread native
VST3/AU editor smoke (`R0Verb.vst3`, `appl:dcmp`, `appl:dls`) are recorded in
`docs/audit/hardware.md` and `docs/audit/final-status.md`.

