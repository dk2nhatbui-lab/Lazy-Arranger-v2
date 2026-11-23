# Release build verification (2026-11-23)

Environment: macOS arm64, rustc 1.100.0 (Homebrew), commit `a299cb9` (code
identical to `d321f73`). No behavior changed for this check.

## Build

```sh
./scripts/rust.sh build --release --workspace
# Finished `release` profile [optimized] target(s) in 32.47s
```

The only warning is Cargo's future-incompatibility notice for `block v0.1.6`, a
transitive dependency (not a project lint failure). `cargo clippy -D warnings`
stays green on the workspace.

## Tests

```sh
./scripts/rust.sh test --release --workspace
# 648 passed, 0 failed (14.04 s including the first test build)
```

## Binaries

| Binary | Size | Smoke |
| --- | --- | --- |
| `target/release/lazy-arranger` | 4.7 MB | `--help` exit 0; `demo` wrote 271-byte MIDI, 40 events, 2 tracks/20 notes |
| `target/release/lazy-arranger-desktop` | 14 MB | ran 6 s under `timeout`; exit 124 is the expected timeout, no panic |

The debug workspace gate (648 tests, clippy `-D warnings`, fmt, rustdoc) and the
Windows/Linux/macOS CI matrix remain the authoritative checks; this file records
that the optimized build and its tests also pass.
