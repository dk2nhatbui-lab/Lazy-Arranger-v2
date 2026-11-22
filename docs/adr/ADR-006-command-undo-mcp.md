# ADR-006: Typed command API, undo/redo and AI/MCP approval

**Status:** Accepted for commands and undo. Proposed for the MCP approval model.

## Context

The blueprint requires one command path for GUI, shortcuts, automation, CLI, AI
and MCP, with validation, undo/redo and inert real-time callbacks. The C++ app
uses whole-model snapshots coalesced by a merge key, with failures as silent
no-ops and no central validation (`docs/audit/notes/cpp-song-model.md` §7).

Cadenza's inverse-command model was reviewed; it is GPL-3.0, so only the pattern
is adopted (`docs/audit/notes/refs-cadenza-vibez.md`).

## Decision

1. **One `Command` enum**, serializable in principle, applied only through
   `EditSession::apply`.
2. **Validate, mutate, validate again, roll back on failure.** The session
   clones the project before applying and restores it if `ProjectFile::check`
   fails, so no invalid state is observable and nothing is silently ignored.
3. **Undo stores inverses, not snapshots.** Each command computes its inverse
   before mutation; `SetTempo` restores an exact `TempoMap`. Undo depth is
   exposed for the UI.
4. **One command is one undo step.** Coalescing (typing, drags) stays out until
   the UI exists; it will be a session-level option that merges adjacent
   commands with the same target, not a change to command semantics.
5. **The real-time thread never applies commands.** It consumes a prepared
   snapshot (ADR-002).
6. **MCP and AI proposals are the same commands.** A tool call builds a
   `Command` plus a dry-run result. Destructive commands (delete track, replace
   generated clip) require an explicit approval token from the caller and are
   logged with the command payload and the resulting undo depth.
7. **No hidden mutations.** Anything that changes the project outside
   `EditSession` is a bug; tests assert the session is the only writer.

## Consequences

* A command that needs a new inverse must extend `inverse_for` and add a
  round-trip test.
* Whole-project cloning per command is acceptable while songs are small. If
  profiling shows cost, replace it with a sparse change set, keeping the
  rollback guarantee.
* Undo does not survive process restart until a command journal exists; that is
  a later feature.

## Evidence

* `crates/app-commands/src/lib.rs`.
* `docs/audit/notes/refs-cadenza-vibez.md` §4 (inverse-command pattern).
* `docs/architecture/contracts.md` §7.
