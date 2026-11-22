# ADR-003: Canonical style model and preservation of manufacturer semantics

**Status:** Accepted for the model shape. Implementation starts at Stage 4.

## Context

Yamaha styles encode chord behaviour in CASM: source chord, NTR, NTT, note
limits, retrigger and controller rules. The C++ audit found a working
transposition implementation and several ambiguities: NTT flavours collapse to
melody, RTR is unused, drum passthrough depends on NTR, and mute-mask polarity
is assumed but never verified against a real Yamaha file
(`docs/audit/notes/cpp-style-arranger.md`).

Korg Pa styles use a different model. The blueprint forbids treating generic
MIDI import as Korg compatibility.

## Decision

1. **Native canonical model with lossy adapters.** A style is stored in a
   versioned native schema; Yamaha and Korg data are attached as typed metadata,
   not flattened into bare notes.
2. **Preserve raw source chunks.** Any importer keeps the original chunk bytes
   (or their digest plus the bytes when legally distributable) so a lossy
   conversion can be reported and re-attempted.
3. **Report, do not hide.** Every import produces a report with status
   `complete`, `partial` or `unsupported`, plus warnings for unknown chunks and
   unsupported conversion rules.
4. **One conversion implementation, specified before coded.** The eight-step
   transposition spec in the audit note is the reference. The first Rust
   implementation lands with golden tests captured from the C++ build.
5. **Verify mute-mask polarity first.** A real PSR style with CASM must confirm
   whether 1 means "plays"; every downstream test depends on it.
6. **Korg is a separate adapter and a separate milestone.** No shared
   "manufacturer magic" layer.

## Consequences

* Stage 4 needs real CASM fixtures. The only style fixture in this repository
  (`tests/fixtures/demo-style.sty`) has `MThd` + `MTrk` and no CASM, so it
  proves container scanning only.
* `crates/style-formats` is deliberately thin today: `scan_chunks` plus
  `NativeStyle` with source chunks and warnings. No claim of chord rules.
* Round-trip writing is deferred until the parity tests exist; the C++ writer
  re-serialises rather than preserving bytes, and copying that behaviour
  without tests would hide loss.

## Evidence

* `docs/audit/notes/cpp-style-arranger.md` §2-§6.
* `crates/style-formats/src/lib.rs`.
* `docs/architecture/contracts.md` §11.
