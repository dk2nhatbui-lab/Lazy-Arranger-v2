# ADR-007: Plugin host selection, isolation, licensing and crash recovery

**Status:** Deferred. Out of scope for the first release.

## Context

The C++ app hosts VST3 and Audio Units in-process. The audit records that JUCE
is AGPL-3.0-or-later or commercial, that plugin state is stored as base64 blobs,
and that in-process hosting means a plugin crash takes the application down
(`docs/audit/licensing.md`, `docs/audit/notes/cpp-surface.md`).

Cadenza hosts CLAP and VST3 out-of-process with a sandbox; its code is GPL-3.0
so only the architecture is reusable.

## Decision

1. **No plugin hosting in the first release.** The MVP ships the internal
   synthesizer path and MIDI routing (Stage 7 of the blueprint).
2. When hosting lands, it is **out-of-process per plugin group**, with an owned
   supervisor that can restart a crashed host and report it, and with a shared
   memory or IPC audio path. In-process hosting is rejected.
3. **Licence gate before loading a plugin.** A plugin's licence can force the
   whole binary's licence; the host records the licence and lets the user
   exclude categories. This is a product policy that needs owner approval.
4. **`nih-plug`/`nice-plug` are authoring frameworks, not hosts.** They are not
   a solution to hosting third-party plugins (blueprint §6).
5. **No allocation, no blocking and no disk I/O in the audio callback.** Plugin
   audio arrives through a prepared buffer; parameter changes go through the
   command path or a lock-free queue.

## Consequences

* Stage 7 needs a process supervisor, shared-memory audio transport and crash
  recovery tests, which is why it is its own milestone.
* Projects cannot reference plugin state until the hosting design exists. The
  schema reserves space by keeping unknown keys.

## Evidence

* `LAZY_ARRANGER_RUST_V2_BLUEPRINT.md` §6, §14 (ADR-007 in the plan).
* `docs/audit/licensing.md` (JUCE, plugin state).
* `docs/audit/notes/refs-cadenza-vibez.md` §4 (sandbox pattern only).
