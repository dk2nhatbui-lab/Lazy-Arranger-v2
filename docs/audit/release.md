# Release build verification (2026-11-24)

Environment: macOS arm64, rustc 1.100.0 (Homebrew), commit `043bbfe`.

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
./scripts/rust.sh test --release --workspace
# 897 passed, 0 failed
```

## Binaries

| Binary | Size | Smoke |
| --- | --- | --- |
| `target/release/lazy-arranger` | 9.1 MB | CLI builds and runs |
| `target/release/lazy-arranger-desktop` | 23 MB | app bundle launches |
| bundle `lazy-arranger-plugin-scan` | — | returned R0Verb 1.0.4 FX metadata from `/Library/Audio/Plug-Ins/VST3/R0Verb.vst3` |

The debug workspace gate (897 tests, clippy `-D warnings`, fmt, rustdoc) and the
Windows/Linux/macOS CI matrix remain the authoritative checks; this file records
that the optimized build and its tests also pass. Hardware remains powered down,
so audio/MIDI/print hardware checks are not part of this snapshot.

Runtime smoke at this commit: the bundled app opened its restored 1546x978 window, showed the Arrange timeline with the Add clip button and live meters, and closed cleanly (local screenshot, not committed).

