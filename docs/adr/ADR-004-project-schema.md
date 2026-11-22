# ADR-004: Versioned project schema and legacy `.arranger` migration

**Status:** Accepted.

## Context

The C++ project file is JSON with `format`, `version` and `song`. The audit
found that `kVersion` is 4 but is treated as a floor, not a schema id: keys were
added for 21 commits without bumping it. Unknown keys are ignored on load and
**dropped on re-save**. Identifiers come from one global `nextId` counter shared
by tracks, clips, audio clips and buses (`docs/audit/notes/cpp-song-model.md`).

A rewrite that mirrors that file layout would inherit its silent data loss.

## Decision

1. **Own schema, explicit version.** This project writes
   `{"format":"lazy-arranger-project","version":1,...}`. The schema is the Rust
   type graph; it is not a copy of the C++ layout.
2. **Unknown keys survive.** Envelope, song and track carry
   `#[serde(flatten)] BTreeMap<String, Value>`; load/save keeps data the build
   does not understand. A test round-trips three unknown keys.
3. **Validation is explicit and typed.** Deserialization validates scalar
   newtypes; `ProjectFile::validate` reports every structural problem with a
   path and a severity. Load fails on any error.
4. **One-way legacy import.** `project::legacy::import_cpp_project` reads C++
   schema 1..=4, flattens clips onto tracks, maps the track output channel onto
   notes, and records everything it deferred in a report. No `.arranger` writer
   exists until golden round-trip tests justify one.
5. **Identifier rules are ours.** Positive integers, unique in their scope.
   Imported track ids are preserved even though the C++ counter is shared, which
   is why demo imports show ids 1, 3, 5, ... .
6. **No derived state in the file.** PPQ is a constant; sample positions are
   compiled at runtime. The file holds intent and notes, not caches.
7. **Migration is code, not a version number.** `migrate()` gains an explicit
   step per version bump and errors on a version it cannot upgrade.

## Consequences

* A project saved by this build cannot be opened by the C++ app. Interchange is
  SMF, not the C++ project file.
* Clip structure, scenes, mixer state, sound selection and routing arrive as
  schema 2 features with their own migration and tests, instead of being
  half-imported now.
* The importer's guarantees are "nothing is silently dropped from the JSON" and
  "notes and tempo are carried"; it does not claim musical equivalence.

## Evidence

* `crates/project/src/{lib,song,validate,legacy}.rs`.
* `tests/fixtures/legacy-cpp-v3.arranger`, `tests/fixtures/legacy-cpp-v4.arranger`.
* `docs/audit/notes/cpp-song-model.md` §5, §6.
