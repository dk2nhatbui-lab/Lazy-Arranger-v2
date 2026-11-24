# Final status (2026-11-24)

Commit basis: audit-remediation work at round 653. Goal: C++ v1.0.0-beta.1 feature parity.
**Parity is not claimed.** All 39 crosscheck rows remain `partial`; several need
material this environment does not have (hardware, proprietary formats, native
plug-in editor support or model services).

## Gate and CI

- `cargo test --workspace`: **884 passed, 0 failed**.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all`
  and rustdoc (warnings denied): clean.
- Release bundle builds; the app launches; the bundled scanner returns R0Verb
  metadata from the installed VST3.
- GitHub Actions: ubuntu, macos and windows have been green through the
  audit-remediation series; the newest pushes were still draining at this
  snapshot. Earlier platform failures and their fixes are recorded in
  `docs/audit/ci-windows-failures.md`.

## Verified areas (representative)

Project schema v20 (program + bank select), migrations, undo and validation
(notes, clips, audio takes/comps, buses, plug-ins, routing, automation, marks
and lyrics); style parse/import/write with SFF1/SFF2, section ordering, programs
and bank select; score engraving (ties, beams, tuplets, rests, dots, slurs,
percussion, lyrics, chords, guitar frames, spacing, meter, measure numbers,
PDF/canvas/print dialog, Latin-1 PDF text); audio render, SF2 with presets,
mixer buses/sends/master reverb/chorus/meters, stems and freeze; MIDI maps,
learn, Launchkey preset and panic builders; arrangements, grooves, forms,
theory and lessons; MCP server with **111 tools**, token socket and project-root
write sandbox; five-view egui shell with eight languages, Kids Mode, named songs
and tutorial; background rendering for Cecilia previews, waveforms and playback.

## Concrete blockers to full parity

1. **Native plug-in editors** — VST3/AU scanning, hosting, processing, parameter
   enumeration and opaque state work. No native editor window exists for either
   format, and no stable header/plugin fixture was supplied to test one.
2. **Korg PCG and L6max project files** — the Pa5X manual does not specify the
   PCG binary layout, and no PCG/L6max project fixture was supplied. The L6max
   device page sends the documented CC map and transport bytes; project readback
   and device feedback remain unverified.
3. **Full score** — a read-only all-track canvas and printable review PDF exist.
   They put two systems on each page with clefs, stems, accidentals,
   beams, ties, tuplets, rests, chords, lyrics, marks and measure numbers, and a Tracks menu hides chosen tracks; the editable
   Score view still engraves one track.
4. **SF3** — SF2 is supported; SF3 compressed samples are out of scope by request.
5. **PDF CJK text** — WinAnsi covers Latin-1; unsupported scripts render as `?`.
6. **AI/speech input** — the assistant is a local rule engine with an optional
   OpenAI-compatible endpoint; live speech input needs a model and microphone
   permission and is optional.
7. **Hardware acceptance** — earlier L6max CC, MIDI send and print visibility
   tests are documented. The hardware is powered down, so key capture, audible
   output, transport response, monitoring latency and actual print jobs remain
   unverified in this snapshot. See `docs/audit/hardware.md`.

## Scoped limitations that remain implementable

Native plug-in editors, editable multi-track scoring, destructive waveform editing,
broader vertical rest collision avoidance and CJK font embedding are
implementable with more rounds, but none removes the blockers above without the
missing fixtures or product decisions.

## How to resume

Provide one or more of: a VST3/AU editor fixture and platform harness; Korg PCG
and L6max project fixtures/specs; an SF3 fixture or decoder budget; a CJK font
choice; an AI/speech endpoint with credentials; or the hardware powered on.

- The device editor sends profile CC controls and SysEx dumps; D-05/XioSynth
  sound pages remain generic, not device-specific.
- Take comping, buses, automation, plug-in state and freeze have model, command,
  MCP and UI coverage; real plugin editor windows do not.
