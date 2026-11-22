# ADR-005: Intent-to-generated-MIDI provenance, freeze and regeneration

**Status:** Proposed. Required before Stage 5 (live capture).

## Context

The defining product behaviour in the blueprint §1 is that a performance is
recorded in two linked representations: musical intent (chords, style, section,
fills, tempo) and rendered MIDI. Users must be able to edit intent and
regenerate, or detach generated MIDI and edit it by hand.

The C++ application has no provenance model: clips hold notes, and the arranger
is a clip launcher (`docs/audit/notes/cpp-style-arranger.md` §1). Regenerating
over edited notes would silently destroy work.

## Decision (proposed)

1. **Two lanes, one timeline.** Chord and arranger events stay in
   `Song.chords` / `Song.arranger`. Generated notes live on ordinary tracks, so
   the sequencer and SMF export do not need a second note type.
2. **Provenance is a per-clip record**, not a per-note flag:
   `{ source: Arranger { style, section }, revision, frozen: bool,
   generated_from: TickRange }`. Notes of a generated clip inherit it.
3. **Regeneration never overwrites silently.** When intent changes, generated
   clips whose range overlaps the change are marked `stale`. The user chooses
   per clip: replace, keep, or duplicate with a new id. The default is to mark
   stale and change nothing.
4. **Freeze is explicit and reversible.** Freezing detaches provenance and
   makes the clip a plain MIDI clip. It is one undo step.
5. **Manual edits inside a generated clip mark it `edited`**, so regeneration
   also asks before replacing. The check is a digest of the notes, not a flag
   the UI can forget to set.
6. **Capture records intent and output.** Note recording appends to a plain
   track; arranger changes append to `Song.arranger` with the same tick
   positions. Both are one undo transaction.

## Consequences

* Schema version 2 adds clips with provenance; version 1 projects migrate with
  no clips and no loss.
* The arranger becomes a producer of clips, which fits the style-player design
  in ADR-003.
* A/B preview (the C++ app's audition pattern) can reuse the same stale/edited
  flags.

## Open questions

* Whether a generated clip stores the source style snapshot or a reference plus
  revision. A snapshot survives style edits; a reference keeps files small.
* How a tempo change invalidates generated notes that were quantised to the old
  grid.

## Evidence

* `LAZY_ARRANGER_RUST_V2_BLUEPRINT.md` §1, §7.2, §14.
* `docs/audit/notes/cpp-style-arranger.md` §1, §5.
