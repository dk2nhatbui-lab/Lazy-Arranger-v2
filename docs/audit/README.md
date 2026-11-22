# Stage 0 source audit

The audit answers one question before any product code is written: which
behaviour must be preserved, which must be redesigned, and what may legally be
reused.

Source pinned for the C++ application: `arranger-midi-daw`, branch
`fix/audit-critical-high`, commit `8ca41644a2be5f5677bce8cae060a55f7b19e8fc`
(10 commits ahead of `origin/main` at `975e97d1004b8a9813339fddbc258c3bcf5eb6a5`).
Reference repositories: Cadenza `6bd8023`, Vibez `1d2c708f`.

## Deliverables

| Document | Answers |
| --- | --- |
| [source-inventory.md](source-inventory.md) | What exists, where, how big, and how it is verified |
| [feature-matrix.md](feature-matrix.md) | Keep / port / redesign / defer per subsystem, with target milestone and current status |
| [compatibility-baseline.md](compatibility-baseline.md) | What parity means, the chord-conversion specification, golden scenarios and tolerances |
| [licensing.md](licensing.md) | What may be copied, what may not, and the dependency licence inventory |

## Evidence notes

`notes/` holds the raw evidence these documents summarise, kept because they
carry file paths and line numbers that the summaries compress:

| Note | Scope |
| --- | --- |
| [cpp-song-model.md](notes/cpp-song-model.md) | Song model, identifiers, timing, project file, edit path |
| [cpp-style-arranger.md](notes/cpp-style-arranger.md) | Style container, CASM, transposition specification, playback reality |
| [cpp-rt-midi.md](notes/cpp-rt-midi.md) | Clock ownership, scheduling order, real-time hazards, MIDI, SMF |
| [cpp-surface.md](notes/cpp-surface.md) | Device profiles, 32 subsystems with LOC, tests, licences |
| [refs-cadenza-vibez.md](notes/refs-cadenza-vibez.md) | Cadenza and Vibez: identity, licence, what is implemented |

The notes are evidence, not decisions. Where a note says `unverified`, the
summary documents keep that word. The decisions live in
[`../adr/`](../adr/) and the shared types in
[`../architecture/contracts.md`](../architecture/contracts.md).

## How the audit was done

* Read-only: the C++ repository was never built or modified. `git status` was
  clean after the audit.
* Counts are programmatic (`wc -l`, `grep -c TEST_CASE`), not estimates.
* Claims carry a file path and, where possible, a line number.
* Nothing about hardware was claimed without a device run. Nothing about style
  compatibility was claimed without a CASM fixture.
