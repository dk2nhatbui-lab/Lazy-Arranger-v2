# Final status (2026-11-24)

Commit basis: audit-remediation work at round 707. Goal: C++ v1.0.0-beta.1 feature parity.
**Parity is not claimed.** All 39 crosscheck rows remain `partial`; several need
material this environment does not have (hardware, proprietary formats, native
plug-in editor support or model services).

## Gate and CI

- `cargo test --workspace`: **923 passed, 0 failed** (including all 5 VST3 test-plugin fixture tests in `plugin-host` and `audio`).
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
theory and lessons; MCP server with **112 tools**, token socket and project-root
write sandbox; five-view egui shell with eight languages, Kids Mode, named songs
and tutorial; background rendering for Cecilia previews, waveforms and playback.

## Concrete blockers to full parity

1. **Native plug-in editors** — macOS native `NSWindow` editors now exist for
   both VST3 (`Vst3Editor`, `IPlugView`/`IPlugFrame` `NSView`) and Audio Units
   (`AuEditor`, `kAudioUnitProperty_CocoaUI` with `AUGenericView` fallback),
   wired to Mixer slot **UI** / **Close UI** buttons with state capture on close
   and live-verified on `R0Verb.vst3` (850×596), `appl:dcmp`, and `appl:dls`.
   Windows/Linux native editor windows and live real-time audio inside open
   editor windows remain unimplemented.
2. **Korg PCG and L6max project files** — Korg PCG is deferred by request (the
   Pa5X manual does not specify the PCG binary layout, and no PCG fixture was
   supplied). The L6max device page sends the documented CC map and transport
   bytes, live USB 48 kHz audio capture works, and `L6max for L6 Editor Port`
   answers Universal Identity (`Zoom fw 0110`); proprietary project-file parsing
   and full mixer-state readback remain absent without a published Zoom schema.
3. **Full score** — a read-only all-track canvas and printable review PDF exist.
   They put two systems on each page with clefs, stems, accidentals,
   dots, ledger lines, shaped rests, beams, ties, slurs, tuplets, chords, optional
   guitar frames, lyrics, marks, section markers, measure numbers, a staff
   bracket, final double bar and tempo; clicks or drags seek, pages follow the
   playhead, and a Tracks menu hides chosen tracks. The editable Score view still
   engraves one track.
4. **SF3** — SF2 is supported; SF3 compressed samples are out of scope by request.
5. **PDF Unicode / Vietnamese / CJK text** — `crates/score/src/unicode_font.rs`
   now automatically embeds a glyph-subsetted TrueType `Type0`/`CIDFontType2`
   (`Identity-H` + `ToUnicode` CMap + `FlateDecode` `/FontFile2`, plus UTF-16BE
   `/Title`) whenever non-WinAnsi characters (Vietnamese, Japanese, Chinese,
   Korean, Cyrillic, Greek) appear in titles, track names, lyrics, chords, or
   markers, resolving `ARRANGER_PDF_FONT` or installed system/OFL TrueType fonts
   (keeping Latin-1 scores lightweight with `/F1` Helvetica). Bundling an OFL
   font inside the release artefact for font-less containers remains optional.
6. **AI/speech input** — the assistant is a local rule engine with an optional
   OpenAI-compatible endpoint or local assistant CLI; local offline speech now
   supports 16 kHz mono WAV microphone dictation with auto-detected `whisper.cpp`
   / `sherpa-onnx` Moonshine STT and `System` / `Kokoro-82M` / `KittenTTS` /
   `Piper` TTS presets in Settings. No model weights are bundled in the repo,
   and live neural-model inference remains unverified until binaries/weights are
   installed under `<data_dir>/models`.
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

- The Roland D-05 now has a dedicated SysEx sound editor (Patch, Upper/Lower
  Tone + Partials, Names & Banks, Separate-mode next-channel, User Reverb 17–32,
  `Read from Synth`, `Get Sound Names`, `Send All`, Save/Load `.syx`), live-verified
  on powered hardware (`"Neo Horizon"`, 448 B / 468 B `.syx`, bank scans + DT1
  restore); XioSynth sound pages remain generic.
- Take comping, buses, automation, plug-in state, freeze, and macOS native
  VST3/AU editor windows have model, command, UI, and smoke coverage;
  Windows/Linux native plug-in editors remain unimplemented.
