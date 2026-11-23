# Final status (2026-11-23)

Commit basis: `7482ea4` (docs-only after `752b1ca`). Goal: C++ v1.0.0-beta.1
feature parity. **Parity is not claimed.** All 39 crosscheck rows are `partial`;
none is `verified` in full, because several require material this environment
does not have.

## Gate and CI

- `cargo test --workspace`: **656 passed, 0 failed**.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all`
  and rustdoc (warnings denied): clean.
- Release build: 32.47 s; 648 release tests at the time; CLI demo and desktop
  smoke recorded in `docs/audit/release.md`.
- GitHub Actions, runs on `fbead10`, `9e3bfe0`, `d3b0199` and `752b1ca`:
  ubuntu, macos and windows all green. The long Windows stack overflow is fixed
  (`5c98cc4`); evidence in `docs/audit/ci-windows-failures.md`.

## Verified areas (representative)

Project schema v20 (program + bank select), migrations, undo/validation; style
parse/import/write with SFF1/SFF2, programs and bank select; score engraving
(ties, beams, tuplets, rests, dots, slurs, percussion, lyrics, chords, frames,
spacing, meter, measure numbers, PDF/canvas/print dialog); audio render, SF2,
mixer with buses/sends/master reverb/chorus/meters, stems/freeze, float WAV;
MIDI maps/learn/Launchkey preset/panic builders; arrangements, grooves, forms,
theory/lessons; MCP server with 93 tools; five-view egui shell with eight
languages, Kids Mode, named songs and tutorial.

## Concrete blockers to full parity

These are not ordinary bugs; each needs an asset, format, model or product
decision that is absent here.

1. **Plugin hosting/processing/editors** — 21 AU/VST3 bundles are installed and
   listed in the My Plug-Ins browser, but no host integration exists. The UAD
   `.lunacomponent` files used by Luna are proprietary and are not AU/VST3, so
   they cannot be loaded without UA's host. Hosting/editors remain a project.
2. **Korg PCG program/combi decoding and L6max project/mixer page** — the Pa5X
   user manual describes imports but does not specify the PCG binary layout, and
   no PCG/L6max project fixture was supplied; only recognition/scans exist.
3. **SoundFont modulators** — SF3 is out of scope by request; SF2 is supported.
   Modulator semantics remain unverified.
4. **AI/speech input (optional)** — the assistant is a local rule engine and
   speech output uses macOS `say`. Live speech input needs a speech model and
   microphone permission; it is optional, not required for the current scope.
5. **Hardware acceptance (partial)** — MIDI ports/sends and a PSR-S970 send were
   exercised, and printers are visible; key capture, audible output, monitoring
   latency and actual print jobs remain unverified. See `docs/audit/hardware.md`.
6. **Key-signature layout** — G/F clefs are now drawn on canvas and PDF without
   changing the four-measure system. A key-signature gutter still needs a
   product decision: it would force three measures per system on the current
   595 pt page (four measures use 496 of 499 pt).

## Scoped limitations that remain implementable

Take comping (scoped model/UI), destructive waveform editing and broader vertical rest collision avoidance are implementable with more rounds, but they do not remove the blockers above and
were not forced speculatively.

## How to resume

Provide one or more of: a VST3/AU host harness and test plugins; Korg PCG and
L6max project fixtures/specs; an SF3 fixture or decoder budget; an AI/speech
endpoint with credentials and permitted models; or the hardware listed above.
For the clef layout, choose either four measures per system without clefs
(current) or three measures per system with a clef/key gutter.
