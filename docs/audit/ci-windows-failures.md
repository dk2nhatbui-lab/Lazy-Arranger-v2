# Windows CI failures (open)

**Priority:** High  
**Status:** Needs investigation; root cause unconfirmed  
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
