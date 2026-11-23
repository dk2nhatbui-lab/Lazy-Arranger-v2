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
languages, persistent right inspector, bottom mixer and a Cecilia Score tab; the Score view uses Cecilia 0.51, Kids Mode, named songs and tutorial.

## Concrete blockers to full parity

These are not ordinary bugs; each needs an asset, format, model or product
decision that is absent here.

1. **Plugin hosting/processing/editors** — 21 AU/VST3 bundles are installed; the manager reads AU Info.plist and VST3 moduleinfo metadata, caches the catalog and lists it in the My Plug-Ins browser. No binary loading or host integration exists yet (needs an isolated unsafe FFI crate). The UAD
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

- Audio decode now covers WAV and symphonia formats (FLAC/MP3/M4A/AIFF/OGG); compressed references mix and draw peaks.
- A generic Synth patch window edits waveform, ADSR and brightness as one undoable command (`docs/audit/synth-patch.png`); device-specific D-05/XioSynth/S970 panels remain absent.
- AU parameter enumeration and setting work in `plugin-host` (Apple AU readback test); no parameter UI or project persistence yet.
- AU parameters can be edited in a generic window, applied with Defaults, and saved as `id=value;` slot state that the render restores; VST3 parameter automation is absent.
- VST3 state can be captured from the Mixer into base64 slot state and restored during render; AU parameters have a generic editor and persisted state.
- VST3 class scans run in an isolated helper with a five-second timeout; the in-process path remains as a fallback when the helper is not packaged.
- AU instruments are routed into track render: a track with a non-bypassed AU instrument slot renders its notes through DLSMusicDevice (ignored wet/dry test); VST3 instruments are hosted and routed through track render; the local test synth renders audible track notes.
- AU instrument note events carry sample offsets inside 512-frame blocks; block-boundary scheduling remains the resolution limit.
- The AU parameter window opens instruments too; DLSMusicDevice exposes editable parameters and slot state.
- MIDI recording supports an audible 0–2 bar count-in and appends to a record-armed track (R in the track row) or inserts a new track when none is armed; a header record button starts the same capture; the same audible count-in precedes audio-input capture, which replaces the record-armed track's audio reference or inserts a new audio track.
- Shortcut editor detects duplicate bindings and has Reset defaults; six bindings remain editable.
- Plugin scanning runs on a worker thread; Rescan/Scan controls disable while scanning and the cache is saved when the worker returns.
- Settings lists audio input devices and persists a preferred input; audio capture uses it with a default fallback.
- Input monitoring restarts when the preferred audio input changes; it uses the named device or the default.
- `style-from-clips` writes a one-section SFF from project tracks; the demo produced 4 parts/21 notes and re-imported as Main A.
- `style-from-clips --from-markers` splits the demo into A/B sections and writes a re-readable SFF; default channel rules only.
- MCP `style_from_clips` writes the same multi-section SFF as the CLI; registry is 94 tools.
- Windows > Style from clips exposes the writer in the desktop app (`docs/audit/style-clips.png`).
- Plugin slots can be reordered up/down through one undoable command.
- Master effect slots run after the bus sum (capture `docs/audit/master-effects.png`); bus effect chains are still absent.
- Buses carry effect chains (`Bus.plugins`), applied to the bus sum; the Bus effects window edits them.
- A basic Style rules window edits destination channel, written root/type, NTR/NTT, held-note RTR, bass and note limits in the export draft.
- Track/bus plug-in chains apply reported latency compensation; the local test VST3 gain fixture reports 32 samples and the delayed/dry track alignment test passes.
- VST3 parameter changes use an `IParameterChanges` queue; a generic window edits them and saves state, but no installed VST3 exposes parameters, so it is unverified.
- Windows > MIDI out sends program changes, bank select, CC, test notes, chords and panic to a selected output, persists the output choice (capture `docs/audit/midi-out.png`); sends remain hardware-unverified.
- Failed isolated VST3 scans are blocklisted persistently and shown/clearable in My Plug-Ins.
- Style from clips has Audition/Stop, compiling the section(s) to a temporary song and playing it.
- Beat view offers 1/2/4-bar patterns in a horizontal scroller.
- `style-from-audio` writes a one-part style from monophonic detected notes; `--chord auto` picks the strongest chroma chord, and the source chord is persisted in the written rules. `--split-parts` produces Bass and Chord 1 parts; `--section-bars` splits into uniform sections with clipped notes. No analysis-driven sections, stems or match scoring.
