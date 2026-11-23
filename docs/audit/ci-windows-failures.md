# Windows CI failures (fixed)

**Priority:** High  
**Status:** Fixed and confirmed by CI  
**Scope:** GitHub Actions CI, not a feature implementation

### Evidence
- GitHub Actions CI runs #92 through #114 are shown as failed on `main` (runs #115 and #116 were still in progress when inspected).
- Run #114 (`0725e6a`, Music Finder and playlist browser) reports `windows-latest: Process completed with exit code 1`.
- The macOS arm64 capacity annotation is informational, not evidence of the failure.
- Public GitHub view requires authentication for detailed job logs; the exact failing command and error are unknown.
- Run: https://github.com/nhth-mibu/Lazy-Arranger-v2/actions/runs/45095307329

### Suggested actions for the implementing AI
1. Open the latest failed Windows job's full logs and identify the **first actual error**, not just the final exit code.
2. Compare the earliest failed run with the preceding successful run to isolate the introducing commit or workflow change.
3. Reproduce on Windows with the exact CI command(s), toolchain and feature flags from `.github/workflows/ci.yml`.
4. Check Windows-specific differences only as hypotheses: shell syntax/path separators, native dependencies and linkers, filesystem case handling, audio/MIDI device assumptions, and test fixtures.
5. Fix the underlying source/test/workflow issue; do not remove Windows from the CI matrix or mask failures with `continue-on-error`.
6. Confirm Linux, Windows and macOS jobs pass on the same commit. Add a regression test if applicable.
7. Record the exact failure, fix, and successful run link in this cross-check document.

## Root cause and fix (2026-11-23)

The Windows job failed in the headless demo with
`thread 'main' has overflowed its stack`. The clap command tree is large in
debug builds and exceeds the 1 MiB default main-thread stack on Windows; even
`--help` overflowed.

Reproduced on macOS by lowering the main stack:

```sh
bash -c 'ulimit -s 1024; ./target/debug/lazy-arranger --help'   # overflowed
bash -c 'ulimit -s 1024; ./target/debug/lazy-arranger demo --out /tmp/stack.mid --verbose'
```

`apps/cli/src/main.rs` now runs `cli_main` on a named thread with a 32 MiB
stack. Under the same 1 MiB limit, `--help` and `demo` both exit 0 and the demo
writes a 271-byte MIDI file. The workspace gate (643 tests, clippy, fmt,
rustdoc) is green; the Windows job should confirm the fix.

## Confirmation

CI run [45158544677](https://github.com/nhth-mibu/Lazy-Arranger-v2/actions/runs/45158544677)
on `5c98cc4` passed all three jobs: ubuntu-latest 1m19s, macos-latest 1m49s,
windows-latest 2m17s. The Windows headless demo now passes.
