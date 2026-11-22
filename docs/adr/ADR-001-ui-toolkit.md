# ADR-001: Desktop UI toolkit

**Status:** Deferred. Revisit at Stage 6.

## Context

The blueprint proposes Slint as the starting hypothesis and asks for prototypes
of a dense piano roll, a timeline, a score view and plugin-window handling
before choosing. Slint, iced and egui have different rendering models,
accessibility stories and licences.

The licence question is a gating criterion, not a footnote. The licensing audit
(`docs/audit/licensing.md`) records that the C++ application links JUCE, which
is AGPL-3.0-or-later or commercial. Choosing a GPL-family UI toolkit would put
the same copyleft obligation on the binary. This Rust workspace is Apache-2.0, so the toolkit's licence must be verified
before the choice, not after.

## Decision

1. Do not choose a toolkit now. Keep building headless.
2. Keep every domain type and every edit reachable through `app-commands`, so a
   UI is a client of the model rather than a layer inside it. The workspace
   already enforces this: no UI crate can appear below `apps/`.
3. Before choosing, record in this ADR: licence and cost, piano-roll scroll and
   redraw performance with 50k notes, text/score rendering quality, IME and
   accessibility support, and plugin-window embedding.
4. Treat a GPL-family toolkit as a product-level decision that needs explicit
   owner approval, the same way ADR-007 treats plugin hosting.

## Consequences

* Stage 6 pays the evaluation cost, but no earlier work is blocked.
* If the chosen toolkit is copyleft, the whole product licence changes. That
  decision is recorded here or in a superseding ADR.
* The engine and model cannot assume a UI event loop exists, which is already a
  test requirement.

## Evidence

* `LAZY_ARRANGER_RUST_V2_BLUEPRINT.md` §6 and §14.
* `docs/audit/licensing.md` (JUCE AGPL, dependency inventory).
* `docs/architecture/contracts.md` §1 and §7.
