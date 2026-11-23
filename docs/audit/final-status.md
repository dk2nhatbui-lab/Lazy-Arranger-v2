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
- Style from audio is shared by CLI, MCP and a desktop dialog (`docs/audit/style-from-audio.png`); an optional onset/band-energy Drums part and onset-spacing tempo detection are heuristics, not stem separation; 95 MCP tools.
- Groove templates can be extracted from audio (`groove-from-audio`, MCP `groove_from_audio`); fixed multi-bar extraction offsets. 96 MCP tools.
- Audio tags are a model-free heuristic (drums/bass/pad/melodic); 97 MCP tools.
- Language offers System default, resolved from LC_ALL/LC_MESSAGES/LANG.
- Track plug-in parameter automation lanes render block-by-block; VST3 test gain verifies 0→127 gain; an ignored Apple dynamics test verifies AU min/max mapping. The AU/VST3 parameter windows can set the automation target with A. Bus/master lanes remain absent.
- About box, device-profiles folder and YAML profile picker added; profiles supply keyboard-export programs and CC controls in a generic Device editor. Update channel still unconfigured; CC sends are hardware-unverified.
- Groove library window lists/loads/applies `.groove` files with timing and accent sliders (`docs/audit/groove-library.png`).
- L6max take scan supports per-take selection; no mixer page/transport or project parsing.
- MIDI out sends transport realtime messages; Device editor can send SysEx; both hardware-unverified.
- UI pass started: C++-style square transport, light tabs, Mixer toggle, Ask AI and resizable left/right/mixer panels.
- Timeline lane view added as the Arrange default, with a Piano roll toggle (`docs/audit/ui-timeline.png`).
- UI pass: Tracks header with add, C++-style mixer strips with knobs/faders, resizable panels, song rename in the inspector (`docs/audit/ui-mixer.png`).
- UI pass: instrument icons in track rows, right-aligned track flags, master fader pinned to the mixer's far right, song title in the window bar (`docs/audit/ui-master.png`).
- Track icons mirror the C++ `Icons.h` geometry and `TimelineViews.cpp` mapping; GM program names/picker added.
- Wider default window reveals the CoreAudio/zoom cluster; inspector program picker and Audio/MIDI settings button; `docs/audit/ui-wide.png`.
- UI pass: wider default window, editable inspector track name, patch text in track rows, Chords-on line (`docs/audit/ui-wide.png`).
- Added Drums and Map tabs; right inspector keeps an inline assistant chat at the bottom (`docs/audit/ui-chat-panel.png`).
- Metronome uses the C++-style vector icon and has an accent slider backed by `metronome::mix_accent` (`docs/audit/ui-metronome.png`).
- Transport uses vector icons instead of glyph text (`docs/audit/ui-transport.png`).
- Right inspector now spans the full height; mixer ends at its edge so the master fader no longer sits under the inspector; chat pinned to the panel bottom (`docs/audit/ui-chat-panel.png`).
- Mixer faders use one fixed-height custom widget for tracks and master; tooltips delay 0.6 s (`docs/audit/ui-master-fader.png`).
- Inspector channel picker moves all track notes to a channel as one undoable edit (`docs/audit/ui-channel.png`).
- Timeline scrolls in both directions, follows the playhead while playing, and header +/- zooms it (`docs/audit/ui-timeline-scroll.png`).
- Mixer meters follow active notes at the playhead; master meter takes the loudest track (`docs/audit/ui-meters.png`).
- Undo glyph redrawn as a left arrow with a curved tail (`docs/audit/ui-undo.png`).
- Tracks default to the first row; M/S/R clicks no longer change row selection; inspector key has Find.
- Transport uses green play, red record and an undo arrow rotated 10° CCW (`docs/audit/ui-transport-colors.png`).
- Kids Mode is full-window Perform with a minimal header: any key, click or MIDI message plays; an optional toggle auditions held keys, loops, and auto-pauses after the configured delay; undo arrow now points 45° CCW (`docs/audit/ui-kids.png`, `docs/audit/ui-undo45.png`).
- PSR-S970 remote panel sends voice/CC/chord auditions; hardware sends remain unverified (`docs/audit/psr-remote.png`).
- Light palette and track colours now use the C++ Studio Paper token values (`docs/audit/ui-cpp-theme.png`).
- Audio output device can be selected and is used by the playback player.
- Audio settings choose output device and fixed buffer; inspector shows sample rate/frames/latency when playing (`docs/audit/audio-settings.png`).
- Track rows use 60 px C++ spacing (`docs/audit/ui-track-rows.png`).
- PSR panel has eight persisted registration slots; hardware unverified.
- XioSynth and D-05 profile files seed the profiles folder and expose CC controls in the Device editor; hardware unverified.
- Track list scrolls vertically when the sidebar is full.
- Timeline view has no extra toolbar; the Piano roll switch sits in the status bar (`docs/audit/ui-no-toolbar.png`).
- Assistant supports an optional OpenAI-compatible endpoint with background request with chat history, one `tool` reply and Apply/Cancel proposal cards for changes; local fallback kept (`docs/audit/ai-settings.png`); external endpoint unverified.
- Timeline lanes fill the panel; track names live only in the sidebar (`docs/audit/ui-no-labels.png`).
- Timeline clips drag to move or resize on a 1/16 grid with a live ghost preview.
- Settings can auto-open all MIDI inputs; no hardware verification yet.
- Check for updates reads the GitHub latest release and compares versions; Open release page, Download to ~/Downloads and Skip this version; no install; external endpoint unverified.
- Style file player loads a Yamaha style and auditions selected sections at an entered chord with optional loop; new section selections and chord pads queue at the next bar; held MIDI notes are recognised as chord changes; fills return to the main section and endings stop looping (`docs/audit/style-file.png`).
- Window title and About use the C++ app name "Lazy Arranger Midi DAW".
- Timeline draws labelled audio-reference blocks with cached waveform peaks (`docs/audit/ui-timeline-waveform.png`); the lane peak map covers the whole file, not the trimmed offset.
- Master strip uses the same custom fader height and a live player meter (`docs/audit/ui-master-meter.png`).
- Status bar reports MIDI inputs; metronome has four beat dots; meters read empty while stopped (`docs/audit/ui-metronome-dots.png`).
- Inspector edits song meter from a picker (`docs/audit/ui-meter-edit.png`).
- Timeline chord lane follows the detected key's sharp/flat signature.
- L6max import mutes a master/mix take; test covers it.
- Held MIDI notes show the recognised chord in Perform and the Inspector; hardware-unverified.
- Assistant Dictate records 5 s and transcribes through a configurable Whisper command; microphone/model unverified (`docs/audit/dictate.png`).
- Assistant can call a local CLI template with {prompt}; after Apply it sends the tool result back for a short summary. Mock-command tested.
- Seeded profiles include PSR-S970, generic GS/XG and Korg Pa; an input or output port name auto-selects a matching profile.
- Device editor sends all controls and saves/loads `.syx` dumps (`docs/audit/device-dump.png`); hardware unverified.
- Timeline lane height is 64 px, closer to the C++ layout (`docs/audit/ui-lane-height.png`).
- Chords from audio window recognises WAV segments in a background thread and writes them as one undoable chord set (`docs/audit/audio-chords.png`, synthetic C major recognised).
- Score view draws an accent playback cursor at the transport position (`docs/audit/score-cursor.png`).
- Video import extracts a mono 48 kHz WAV with FFmpeg; extraction test creates a 1 s MP4.
- Audio input can record one of eight channels or a mono mix (`docs/audit/audio-settings.png`); hardware unverified.
- Bus/master VST3 slots capture/clear state; no capture hardware fixture.
- Generate part window exposes all seven rule-based generators with a seed (`docs/audit/generate-part.png`).
