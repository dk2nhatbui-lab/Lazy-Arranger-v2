# ADR-010: Score and engraving architecture

**Status:** Proposed architecture. Implementation is Stage 8; no score code exists yet.

## Context

The blueprint defers notation to Stage 8 ("Future optional crates: `notation`",
§5, §12). The C++ application has an engraving engine of its own, but this
workspace has **no score, notation or engraving code today**: the only match for
"notation" in the tree is the scientific-pitch spelling in `music-core`. A
recommendation from Codex to adopt "the Cecilia-style engraving module ... keep
your existing score logic where it already works" therefore has no existing
score logic to keep. What does exist is the part that matters for the seam:

* stable identifiers (`project::ids::{TrackId, NoteId, StyleId}`);
* one typed edit path with exact inverses (`app-commands`);
* a pure musical model with no UI types in it.

The recommendation was checked against that state, the four reference DAWs
(Cadenza: egui 0.36; Vibez: iced 0.13; SoundCraft: egui 0.37; Cecilia: a custom
winit/wgpu/vello renderer), and the licence of every crate it names. SoundCraft
and Cecilia are pinned in
[`docs/audit/notes/refs-soundcraft-cecilia.md`](../audit/notes/refs-soundcraft-cecilia.md)
at commits `915481b` and `d54e949`; both are MIT OR Apache-2.0, so their code is
reusable with attribution, unlike Cadenza and Vibez. All named crates are permissive: `egui` MIT OR
Apache-2.0, `tiny-skia` BSD-3-Clause, `krilla` MIT OR Apache-2.0, `pdf-writer`
MIT OR Apache-2.0.

## Decision

1. **Engraving is one pure function with an immutable result.** Inputs are a
   score snapshot, layout settings and fonts; the output is an engraved layout
   of pages, positioned drawing items and hit targets. Neither renderer makes a
   musical layout decision.
2. **The score model references project events by stable ID.** Notation-specific
   choices (enharmonic spelling, voice, ties, staff assignment, manual engraving
   adjustments) are stored explicitly, never inferred from MIDI alone. Written
   rhythm uses exact musical durations; performed timing stays in the notes.
3. **Score edits go through `app-commands`.** A vertical note drag is a pitch
   edit; a slur-handle drag is an engraving adjustment anchored to that slur's
   ID. Page coordinates are derived layout data and are never stored.
4. **One layout feeds two renderers.** The egui canvas adapter and the vector
   PDF adapter consume the same engraved document. Page view and PDF export
   share paper size, margins, staff size, fonts and settings; a continuous
   editing view may wrap differently but uses the same engraving code.
5. **The canvas starts with cached raster tiles.** Rasterise visible tiles with
   `tiny-skia` at the current zoom and display scale, draw selection, caret,
   handles and playback feedback above them, and bound the cache to the viewport
   plus a small margin. PDF stays vector. GPU vector rendering (for example
   Vello through an egui callback) is a later optimisation, not a starting point.
6. **Engraving runs on a worker with revisions.** Accept a finished layout only
   when its document revision and settings match the request. The UI thread never
   engraves.
7. **Multi-part systems are designed before ensemble scores.** A single-track,
   multi-staff layout comes first; aligning rhythmic columns across parts is its
   own piece of work.
8. **The score crate must not depend on egui.** The view adapter may; the
   engraver and the PDF writer may not.

## Corrections to the input

* **No existing score logic.** The suggestion to keep it is not applicable; the
  seam is new work at Stage 8.
* **Stable IDs, not indices.** Cecilia's hit references are track/bar/voice/beat
  indices. This project uses stable IDs so a selection survives insertions and
  reflow.
* **Claims to verify in the spike, not adopt on trust:** that `epaint`'s
  `PathShape` fills are convex-only (which motivates raster tiles), and that
  `krilla` preserves positioned glyph runs well enough for score text. Both are
  prototype checks.
* **Toolkit choice is not this ADR.** egui versus iced remains ADR-001 and needs
  an explicit owner decision. This ADR only requires that the engraver is
  toolkit-agnostic, which holds for either.

## Validation before expanding notation coverage

Build a two-page piano score containing chords, voices, tuplets, ties across
systems, lyrics and one dragged notation mark. Check: identical geometry between
screen and PDF, hit testing correct at several zoom levels, and one undo step
for the drag. Only then widen notation coverage.

## Consequences

* Stage 6 stays a UI shell over existing models; no engraving work is pulled
  forward.
* A future `notation` crate (blueprint §5) is the home for these modules, with
  the view adapter behind its own feature or crate so egui never enters the
  engraver.
* PDF export can lose text search if glyphs are outlined; the first version may
  outline and revisit later.

## Evidence

* `docs/adr/ADR-001-ui-toolkit.md` (toolkit comparison and licences).
* `crates/app-commands`, `crates/project/src/ids.rs` (the seam's existing pieces).
* Cadenza, Vibez, SoundCraft and Cecilia clones under `.reference/`, pinned in
  `docs/audit/notes/`.
* `docs/audit/notes/refs-soundcraft-cecilia.md` for the engraving evidence.
