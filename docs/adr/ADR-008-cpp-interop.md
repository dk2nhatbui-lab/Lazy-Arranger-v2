# ADR-008: C++/Rust interoperability during migration

**Status:** Accepted.

## Context

The blueprint asks whether to bridge to the C++ implementation (`cxx`) or to
use file-based golden fixtures. The C++ app has no licence file and states that
the source is private, and it links JUCE under AGPL-3.0-or-later or a commercial
licence (`docs/audit/licensing.md`).

Linking or loading C++ code in-process would mix the licences of the two
binaries and create a build that needs JUCE, CMake and a C++ toolchain on every
developer machine and CI runner.

## Decision

1. **No `cxx`, no FFI, no shared library.** The Rust build has no C++ toolchain
   dependency.
2. **Parity is proven with files.** The C++ application is used offline, by its
   owner, to produce fixtures and golden outputs:
   * project files (`.arranger`) for import tests,
   * standard MIDI files for scheduling and export comparison,
   * style files for future chord-conversion tests.
3. **Golden data is data, not code.** It lives under `tests/fixtures/` with a
   provenance note and is only committed when its licence allows it.
4. **Tolerances are explicit** and recorded in
   `docs/audit/compatibility-baseline.md`: exact sample equality where the
   formula matches, at most one sample elsewhere, exact ordering and velocity.
5. **No C++ build in CI.** If a golden needs recapturing, that is a documented
   manual step with the exact commit pinned.

## Consequences

* Comparing behaviour requires the owner to run the C++ build; CI cannot.
* The port can proceed without waiting for a C++ toolchain, and the Rust
  workspace stays buildable on three operating systems with `cargo test`.
* If a future feature needs live comparison (for example interactive chord
  following), it must be expressed as recorded input/output files instead of a
  bridge.

## Evidence

* `docs/audit/licensing.md` (C++ licence status, JUCE).
* `apps/cli/tests/sprint_exit.rs`, `tests/fixtures/`.
