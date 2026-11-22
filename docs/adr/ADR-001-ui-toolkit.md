# ADR-001: Desktop UI toolkit

**Status:** Deferred. Revisit at Stage 6.

## Context

The blueprint proposes Slint as the starting hypothesis and asks for prototypes
of a dense piano roll, a timeline, a score view and plugin-window handling
before choosing. Slint, iced and egui have different rendering models,
accessibility stories and licences.

The licence question is a gating criterion, not a footnote. The licensing audit
(`docs/audit/licensing.md`) records that the C++ application links JUCE, which
is AGPL-3.0-or-later or commercial. This Rust workspace is Apache-2.0, so the
toolkit's licence decides what can be shipped, and it must be verified before
the choice, not after.

## What the reference projects actually use

Neither reference DAW uses Slint. Both picked permissive toolkits, which is
evidence that a permissive option is workable for this kind of editor:

| Project | Toolkit | Version | Licence | Evidence |
| --- | --- | --- | --- | --- |
| Cadenza | `egui` through `eframe`, with `egui_kittest` snapshot tests and the wgpu renderer | 0.36.2 | MIT OR Apache-2.0 | `cadenza/Cargo.toml:67-69`, `crates/app/Cargo.toml:35-36,55`, `Cargo.lock` |
| Vibez | `iced` with `iced_runtime` and the `iced_tiny_skia` renderer | 0.13.1 | MIT | `vibez/Cargo.toml:47-49`, `crates/vibez-ui/Cargo.toml:16,42,48`, `Cargo.lock` |
| Slint (the blueprint's hypothesis) | `slint` | 1.16.1 | `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0` | `cargo info slint` |

The Slint licence is a triple, not simply GPL: there is a royalty-free option
whose conditions (attribution, scope, revenue limits) need a full read before
use. The earlier wording here, that a GPL-family toolkit necessarily forces
copyleft on the binary, was too strong for Slint and is corrected.

Both reference UI layers are large: Cadenza has 124 source files under
`crates/app/src`, Vibez has 495 under `crates/vibez-ui/src`. That is the scale a
dense editor reaches, and it is a warning against treating the toolkit choice as
cheap to reverse.

## Decision

1. Do not choose a toolkit now. Keep building headless.
2. Keep every domain type and every edit reachable through `app-commands`, so a
   UI is a client of the model rather than a layer inside it. The workspace
   already enforces this: no UI crate can appear below `apps/`.
3. Before choosing, record in this ADR: licence and cost, piano-roll scroll and
   redraw performance with 50k notes, text/score rendering quality, IME and
   accessibility support, and plugin-window embedding.
4. Treat a copyleft toolkit, or a royalty-free licence with conditions, as a
   product-level decision that needs explicit owner approval, the same way
   ADR-007 treats plugin hosting. The reference projects show that `egui`
   (immediate mode, MIT/Apache-2.0) and `iced` (retained Elm-style, MIT) are both
   viable permissive routes and are the natural candidates to prototype.

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
