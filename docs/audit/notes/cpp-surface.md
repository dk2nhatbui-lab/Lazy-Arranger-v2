# C++ surface audit — arranger-midi-daw @ 8ca4164

Read-only audit of `/Users/far-mini/repos/arranger-midi-daw` at commit
`8ca41644a2be5f5677bce8cae060a55f7b19e8fc` (tree clean). Method: `wc -l` for LOC,
`grep` for counts, headers read for behaviour. Nothing was built or modified.
Source size: **71,723** lines of C/C++ under `src/` (226 files). `scripts/check-no-juce.sh`
forbids JUCE in `src/core`, `src/engine`, `src/soundfont`, `src/whisper`.

## Scope A — device and controller profiles

### A1. Files
| Path | LOC | Role |
|---|---|---|
| `resources/devices/generic-gs.yaml` | 187 | Roland GS baseline (SC-55/88), 24 voices |
| `resources/devices/generic-xg.yaml` | 188 | Yamaha XG baseline (MU), 32 voices |
| `resources/devices/korg-pa.yaml` | 194 | Korg Pa arrangers, `dialect: gm` |
| `resources/devices/roland-d-05.yaml` | 3,634 | Roland D-05, full SysEx editor |
| `resources/devices/yamaha-psr-s900.yaml` | 133 | PSR-S900, XG fallback |
| `resources/devices/yamaha-psr-s970.yaml` | 2,050 | PSR-S970, voice list + 16-part editor |
| `resources/devices/novation-xiosynth.yaml` | 389 | Novation XioSynth, `dialect: none` |
| `resources/devices/zoom-l6max.yaml` | 196 | Zoom L6max mixer + USB audio interface |
| `resources/controllers/behringer-x-touch-mini.yaml` | 39 | X-Touch Mini, 2 layers, 8 strips |
| `resources/controllers/novation-launchkey-mini-mk3.yaml` | 61 | Launchkey Mini MK3, 6 modes |
| `resources/controllers/novation-xiosynth-49.yaml` | 24 | XioSynth mixer template, ch 16 |
| `src/core/.../DeviceProfile.h` + `src/core/src/DeviceProfile.cpp` | 261 + 689 | Schema, `DeviceLibrary`, YAML parse, validation, JSON |
| `src/core/.../ControllerProfile.h` + `src/core/src/ControllerProfile.cpp` | 143 + 453 | Bindings, pads, pad lights, surface |
| `src/core/include/arranger/core/SynthEditor.h` | 198 | `editor:` schema (CC/NRPN/RPN/SysEx) |
| `tests/DeviceProfileTests.cpp` | 390 | 18 cases |
| `tests/ControllerProfileTests.cpp` | 240 | 9 cases |
| `tests/DeviceChartTests.cpp` | 201 | 4 cases, asserts real MIDI bytes |

### A2. Yamaha PSR-S970 — full key structure

Sound list is 1,910 lines of `items:`; every other key is shown.

```yaml
profile: 1                     # literal; parser rejects anything else
id: yamaha-psr-s970
name: Yamaha PSR-S970
maker: Yamaha
dialect: xg                    # gm | gs | xg | none
ports: ["PSR-S970", "Digital Workstation"]   # substrings, case-insensitive
drumChannels: [10]
programBase: 1                 # 1 = numbers as printed in the manual
mixer: { volume: 7, pan: 10, reverb: 91, chorus: 93 }  # CC numbers; none = not sent
capabilities: { polyphony: 128, parts: 16, lowest: 0, highest: 127, source: manual }
reset:
  - "F0 43 10 4C 00 00 7E 00 F7"   # XG System On
notes: >                       # free text, shown in the app
  Song channels 1-16 play the keyboard's MIDI parts. ...
sounds:                        # 21 groups: Piano, E.Piano, Bass, Accordion,
  - group: "Piano"             # Strings, Brass, Woodwind, Choir&Pad, Synth,
    items:                     # Percussion, DrumKit, OrganFlutes, Legacy *
      - ["ConcertGrand", 0, 122, 1]  # [name, bankMSB, bankLSB, program]
      - ["SwingTrumpet", -, -, 57]   # "-" = byte not sent
editor:
  sysex:
    write: "F0 43 {dev} 4C {addr} {value} F7"
    device: 16                 # {dev}; "channel" = MIDI channel - 1
    addressBytes: 3
  pages:                       # 16 pages, base 08 0N 00
    - name: "XG Part 1"
      base: "08 00 00"
      params:
        - { name: "Volume", at: "00 00 0B", max: 127 }
        - { name: "Pan", at: "00 00 0E", min: 1, max: 127, offset: -64 }
        - { name: "Reverb Send", at: "00 00 13", max: 127 }
        - { name: "Chorus Send", at: "00 00 12", max: 127 }
```

### A3. Roland D-05 — full key structure

Shows `sections:` reuse and a `read` template, which the Yamaha file lacks.

```yaml
profile: 1
id: roland-d-05
name: Roland D-05
maker: Roland
dialect: none
ports: ["D-05"]
receiveChannel: 1              # monotimbral: forces the track's channel
drumChannels: []               # empty overrides the {10} default
programBase: 1
mixer: { volume: 7, pan: none, reverb: none, chorus: none }
capabilities: { polyphony: 8, parts: 2, lowest: 0, highest: 127,
                source: "manual: 16 in WHOLE, 8 in DUAL/SPLIT; Separate modes use 2 parts" }
notes: >
  One part, on the D-05's MIDI channel (set in its SYSTEM menu). ...
sounds:
  - group: "Preset 1: Original D-50"
    items:
      - ["Fantasia", 87, 8, 1]     # bank MSB 87, LSB 8-13 = Preset 1-6
editor:
  sysex:
    write: "F0 41 {dev} 14 12 {addr} {value} {sum} F7"
    read:  "F0 41 {dev} 14 11 {addr} {size} {sum} F7"
    device: channel            # keyword, not a number
    addressBytes: 3
    sizeBytes: 3
  sections:                    # reusable param lists, expanded by pages
    partial:                   # 54 params
      - { name: "Pitch Coarse", at: "00", max: 72, noteFrom: C1, default: 36 }
      - { name: "TVF Bias Point", at: "10", choices: ["<A1", ..., ">C7"] }
    common:                    # 60 params
      - { name: "Structure", at: "0A", max: 6, offset: 1 }
  pages:
    - name: Patch
      base: "00 03 00"
      params:
        - { name: "Key Mode", at: "12", choices: ["Whole", "Dual", "Split", ...] }
        - { name: "Split Point", at: "13", max: 60, noteFrom: C2 }
    - { name: Upper Common,    base: "00 01 00", section: common }
    - { name: Upper Partial 1, base: "00 00 00", section: partial }
    # ... plus 16 "Reverb 17..32" pages of 191 SysEx params each
```

### A4. Schema fields and validation
| Key | Type / default | Validation (`DeviceProfile.cpp`) |
|---|---|---|
| `profile` | int, required | must be `1` (L331) |
| `id`, `name` | string, required | both non-empty (L336-338) |
| `maker`, `notes` | string, `""` | none |
| `dialect` | enum, `gm` | `gm`/`gs`/`xg`/`none` (L345-347) |
| `ports` | list<string>, empty | must be a sequence (L351) |
| `drumChannels` / `programBase` / `receiveChannel` | int(s) | ch 1-16 (L360); base 0 or 1 (L390); channel 1-16 (L394) |
| `mixer.{volume,pan,reverb,chorus}` | int \| `none` | 0-127 or `none`/`-` → `-1` (L36-49) |
| `reset[]` | hex string | `parseHex`: hex bytes, non-empty, ends `F7` (L297-317) |
| `sounds[].items[]` | 4-tuple or map | msb/lsb 0-127; program `programBase`..128 (L120-145) |
| `capabilities` | map | must be a map (L75); `polyphony` 0-4096, `parts` 0-64, notes 0-127, `lowest <= highest` (L83), `controllers` each 0-127, `source` free text |
| `editor` | map | `synth_edit::parseEditor` (L405); `addressBytes` 1-4 |
| `audio` | map | `devices` required; group `channels` 1-2 (L204) |

Errors carry a YAML line number via `fail()`: `"Line " + (mark.line+1) + ": " + message`
(L24-33). Validation is schema-shape only, never against a live device.

### A5. MIDI dialect model
| Dialect | Meaning | Evidence |
|---|---|---|
| `gm` | General MIDI fallback; banks not assumed | `generalMidi()`, `DeviceProfile.h:176` |
| `gs` | Roland GS: bank-select variations, kit on ch 10 | `generic-gs.yaml:3-16` |
| `xg` | Yamaha XG: XG System On reset, 16 parts | `generic-xg.yaml:3-16` |
| `none` | Non-GM; CCs and banks fully per profile | `roland-d-05.yaml:9`, `zoom-l6max.yaml:23` |

- **Banks:** always the MSB/LSB/Program triple; `-1` on any byte means "do not
  send it" (`byteOrNone`, L99-118) — needed by devices that reject bank select.
- **Drum kits / reset:** kits are only `drumChannels`; the Yamaha note that XG
  kits live on bank 126/127 and play on *any* channel
  (`yamaha-psr-s970.yaml:18-20`) is prose, not schema. Reset is a raw SysEx list,
  sent only on explicit Reset Device.
- **SysEx:** templates with `{dev} {addr} {value} {size} {sum}`. Checksums:
  `roland`, `sum7`, `bitXor`, `none` (`SynthEditor.h:66`). Values 1-4 bytes,
  nibble-packed, or two's complement (`SynthEditor.h:31-35`).
- **Mixer:** per-device CC map, not a GM assumption; `-1` suppresses the message.

### A6. Capabilities → hardware check

`DeviceCapabilities` = `{polyphony, parts, lowestNote, highestNote, controllers,
source}` (`DeviceProfile.h:35-46`). **`0` means not known** and the check says so
(`known()` = `polyphony>0 || parts>0`). JSON round-trip for settings and the AI
tools (`capabilitiesJson`, L470-545). User numbers beat profile numbers
(`setCapabilities`, `:193`).

### A7. Versioning and loading
| Aspect | Behaviour | Evidence |
|---|---|---|
| Format version | `profile: 1` hard-required first key, no upgrade path | `DeviceProfile.cpp:331` |
| Controller version | `controller: 1`, same pattern | `behringer-x-touch-mini.yaml:7` |
| Built-in source | YAML compiled into `BinaryData` | `src/app/CMakeLists.txt:5-25` |
| Load order | built-ins, then user folder; same `id` replaces | `MainComponent.cpp:2627-2640` |
| User folder | `userApplicationDataDirectory`/`Arranger MIDI DAW`/Devices | `MainComponent.cpp:2508,2589` |
| Add-ons | `<id>.sounds.yaml` with `soundsFor:`; merged, never replaces | `DeviceProfile.h:148-155` |
| Merge rule | same group joins; same bank+program renames; else append | `mergeGroups`, L167 |
| Port binding | case-insensitive substring; `assign()` overrides | `matchesPort` L270, `assign` L236 |
| Fallback | `forPort` returns GM when nothing matches | `DeviceProfile.h:199` |

### A8. What a Rust `midi-devices` crate must preserve — the schema quirks that carry behaviour

1. Positional `[name, msb, lsb, program]` plus the `-`/`none` sentinel; `programBase` converted at parse time (`parseSound`, L120).
2. `dialect` as a closed enum where `none` is a real value, not "unknown".
3. Per-field suppression of bank bytes (`-1`), not a boolean "has banks".
4. `editor.sections` reuse — one param list expanded by many pages at different `base` addresses; flattening duplicates ~3,000 D-05 lines.
5. Checksum kinds and multi-byte / nibble / signed encodings — protocol facts, not UI.
6. `capabilities.source` (`manual`/`user`/`estimate`) so the check states provenance; `0` stays unknown.
7. Add-on layering: built-in + user override by `id` + `soundsFor:` rename-by-bank/program.
8. The `audio:` channel-count guard — names apply only at exactly that input count (`zoom-l6max.yaml:186-196`).
9. Line-numbered errors; hand-written YAML is the authoring surface.

## Scope B — feature surface

`UI` = drawing/interaction only. `core` = JUCE-free logic with tests.

| # | Subsystem | Files (LOC) | What it does | Layer | Rec |
|---|---|---|---|---|---|
| 1 | Notation | `Notation.h/.cpp` (889) | Display-only quantise, bar split, ties, spelling, beams, drum notation | core | **port** — pure model→symbols, 25 tests |
| 2 | Score layout | `ScoreLayout.h/.cpp` (1,251) | Part → pages of glyphs/lines/texts in staff spaces, SMuFL codes | core | **port** — one layout feeds screen and PDF, 6 goldens |
| 3 | MusicXML | `MusicXml.h/.cpp` (1,026) | Read/write `.musicxml`/`.xml`; `.mxl` read-only (app unzips) | core | **port** — interchange, 9 round-trip tests |
| 4 | PDF | `Pdf.h/.cpp` (172) | Glyph outlines → PDF, no embedded fonts | core | **port** — small, already font-free |
| 5 | Score UI | `ScoreView.cpp/.h` (1,533) | Paint, edit marks, export PDF | UI | **redesign** — 1.4k lines of JUCE painting |
| 6 | Piano roll | `PianoRoll.cpp/.h` (1,053) | Draw/drag/lengthen notes, velocity lane, snap | UI | **redesign** — no logic worth carrying |
| 7 | Clip grid | `ClipGridView` (1,198); model in `Song.h`, `Session.cpp` (2,316) | Scenes × tracks, launch quantise, per-track takeover of Timeline | UI + core | **port** launch/phase; redesign the view |
| 8 | Groove | `Groove.h/.cpp` (1,391), `Grooves.cpp` (126), `GrooveView` (515) | Timing/velocity templates, 3 drum lanes, feel transfer | core | **port** — 9 tests incl. a listening test |
| 9 | Arranging doctor | `Arranging.h/.cpp` (1,300) | Rule-based proposals (density, register, repetition, contrast), A/B preview | core | **port** — no model, plain music rules |
| 10 | Hardware fit | `HardwareFit.h/.cpp` (784) | Polyphony/parts/range/CC per device; proposes compromises | core | **port** — reads `capabilities`, protects bass/melody/drums |
| 11 | Song forms | `SongForm.h/.cpp` (2,131), `SongFormView` (564) | Alternative section orders, transitions, preview, one-undo commit | core | **port** — 14 tests; 2nd largest core file |
| 12 | Quantise | `Quantise.h/.cpp` (98) | Grid, strength, swing, ends, window | core | **port** — trivial but exact |
| 13 | Tempo map | `TempoMap.h/.cpp` (155) | Constant-tempo segments; ticks ↔ seconds | core | **port** — no ramps yet; extend, don't copy |
| 14 | Time-stretch | `TimeStretch.h/.cpp` (134) | WSOLA 0.25-4×; tempo from loop length | core | **port** — small, swappable later |
| 15 | Samples / sampler | `Sample.h/.cpp` (247), `engine/DrumSampler` (286), `SlicerView` (292) | In-memory audio, pads, slices, choke/gate, voice stealing | core | **port** — WAV 16/24/32, 6 tests |
| 16 | Mixer / buses / automation | model `Song.h` (469), `MixerView` (1,040), `RackTests` (479) | Strips, buses, sends, master, insert/master effects, automation lines | core + UI | **port** model, **redesign** strip drawing |
| 17 | Plugin hosting | `PluginHost.cpp/.h` (405), `PluginViews` (424), 3 test VST3 | VST3 + AU (macOS), out-of-process scan, latency comp | app | **redesign** — JUCE format manager; Rust needs its own host |
| 18 | Audio import/export | `AudioFiles.cpp/.h` (177), `MainComponent::exportAudio` | WAV/FLAC/MP3 in, video via ffmpeg, WAV-only render out | app | **port** import policy; **redesign** codecs |
| 19 | Audio analysis | `AudioAnalysis.h/.cpp` (1,044), `app/AudioAnalysis` (1,013), `AnalysisView` (298) | Reads helper JSON; finds chords locally (chroma + Viterbi); builds tracks | core | **port** — chords/transients are ours, models external |
| 20 | Whisper lyrics | `whisper/Lyrics.cpp/.h` (176) | whisper.cpp with word timestamps; model downloaded | core | **port** — thin wrapper, MIT upstream |
| 21 | Speech (TTS) | `Speech.cpp/.h` (748) | Reads answers aloud via sherpa-onnx runtime download | app | **defer** — separate concern, external runtime |
| 22 | MidiAi helpers | `MidiAi.h/.cpp` (801), `MidiAiService` (503) | Request/answer JSON for Magenta and Anticipatory Transformer; humanize local | core | **port** — no model code, 14 canned-answer tests |
| 23 | MCP server | `McpServer.h/.cpp` (4,705), `src/mcp/Main.cpp` (163), `McpService` (747) | **93 tools** + 5 AI tools (98 total), JSON-RPC stdio, local socket to app | core + app | **port** — the real product surface |
| 24 | Skills | `Skills.h/.cpp` (341) | Built-in + user JSON packs; text/data only, never code | core | **port** — extensible without rebuild |
| 25 | Kids mode | `KidsMode.h/.cpp` (111), `KidsView` (287) | Any key plays; pause 0.5-5 s; resume from there | core | **defer** — low value, self-contained |
| 26 | Autosave | `Autosave.h/.cpp` (380) | Interval write, heartbeat, crash recovery | core | **port** — well specified, 13 tests |
| 27 | Localization | `Localization.cpp/.h` (150) + 4 `.txt` (827) | da/fr/vi + pseudo, ~203 entries each; font must cover Vietnamese | app | **port** — plain key/value |
| 28 | Update checker | `UpdateChecker.cpp/.h` (156), `Version.cpp` (104) | GitHub releases API + semver compare | app | **port** semver, **redesign** transport |
| 29 | Playlists / Music Finder | `Playlist` (85), `MusicFinder` (230) | Yamaha `.pgl` TSV and Music Finder books (`.mfd`, CSV) | core | **defer** — device-specific, small |
| 30 | Recorder import (L6max) | `RecorderImport` (88), `L6maxMixerView` (456), `LinkAudio` (210) | Newest take per channel, 2 GB splits, mix last; yt-dlp link intake | core | **port** — narrow but tested |
| 31 | SysEx files | `SysexFile` (81), `D05Reverb` (115) | `.syx` split/join, interior byte ≥0x80 rejected; D-05 addresses | core | **port** — tiny, exact |
| 32 | Synth editors | `SynthEditor.h/.cpp` (1,166), `SoundEditorView` (664), `MidiMapView` (1,080) | YAML-driven CC/NRPN/RPN/SysEx pages, name dumps | core + UI | **port** core — the profile *is* the editor |

Largest core files: `McpServer.cpp` 4,423; `SongForm.cpp` 1,994;
`StyleFromAudio.cpp` 1,561; `SongEdit.cpp` 1,170; `Groove.cpp` 1,160.

## Scope C — test and fixture inventory

**74 `.cpp` files in `tests/`.** 480 `TEST_CASE`, **0** `SCENARIO` (no BDD macros).
72 files carry tests; 2 are harness. Nine CMake targets: `arranger_tests`,
`arranger_realtime_tests`, `arranger_fuzz`, `arranger_plugin_tests`,
`arranger_localization_tests`, `arranger_async_tests`, `arranger_audit_app_tests`,
`arranger_media_tests`, `arranger_audit_ui_tests`.

| File | LOC / cases | Covers |
|---|---|---|
| `McpTests.cpp` | 875 / 15 | Protocol, tracks/notes/clips, grid, styles, synth editors, theory, chords, lyrics, analysis |
| `StyleFileTests.cpp` | 697 / 15 | Yamaha style read/write both formats, chord rules, style↔song |
| `NotationTests.cpp` | 587 / 25 | Spelling, beams, ties, drums, grand staff, PDF parts |
| `SongFormTests.cpp` | 550 / 14 | Section finding, forms, transitions, gig ending, AI tools |
| `AudioTrackTests.cpp` | 546 / 14 | Clip placement, gain/fades, stretch, tempo-follow, audio→MIDI |
| `AuditFixParsersTests.cpp` | 485 / 6 | H15-H20 parser regressions (MIDI, style, XML, SysEx, recorder, profiles) |
| `RackTests.cpp` | 479 / 8 | Inserts/master, buses, latency comp, overlapping takes |
| `AuditFixAppTests.cpp` | 474 / 10 | C4-C5 / H1-H12 app security and lifetime fixes |
| `AuditFixCoreMcpTests.cpp` | 472 / 7 | C6 / H11-H23 core and MCP guards |
| `ClipGridTests.cpp` | 464 / 13 | Launch quantise, scene launch, phase, takeover from Timeline |
| `AuditFixUiTests.cpp` | 429 / 6 | C1-C3 / H2-H4 UI lifetime and ID collisions |
| `SequencerTests.cpp` | 424 / 17 | Sample-exact output, loops, mute/solo, thru, panic, held notes |
| `GrooveTests.cpp` | 419 / 9 | Grid choice, extraction, transfer, drum lanes, words, listening |
| `ArrangingTests.cpp` | 396 / 12 | Doctor proposals, preferences, hardware repairs |
| `DeviceProfileTests.cpp` | 390 / 18 | Every profile loads; per-device quirks; validation errors |
| `MidiAiTests.cpp` | 351 / 14 | 5 AI tools, humanize determinism, helper errors, two-step call |
| `SynthEditorTests.cpp` | 341 / 10 | Editor messages, listener, multi-byte SysEx, checksums, name dump |
| `ChordInputTests.cpp` | 333 / 9 | Typed chords, suggestions, palette, voicings, reharmonise |
| `ClarifyTests.cpp` | 328 / 8 | Near-tie questions, answers, locking, MCP exposure |
| `StyleFromAudioTests.cpp` | 316 / 8 | Tempo/downbeat, stem→part map, sections, cancel, preview |
| `FileEdgeCaseTests.cpp` | 314 / 7 | Truncation, 1k tracks, Unicode paths, failed saves, clamping |
| `McpToolGapTests.cpp` | 312 / 5 | `set_track`, `remove_track`, `find_chords`, `get_beat`, style-from-audio |
| `UndoRedoTests.cpp` | 298 / 6 | Exact undo/redo, 1,000 random edits, history cap |
| `AutosaveTests.cpp` | 284 / 13 | Interval, heartbeat, crash recovery, stale ids, unwritable folder |
| `RenderTests.cpp` | 280 / 3 | Buffer-size independence; SF2 onset within 64 samples; golden mix |
| `AutomationTests.cpp` | 280 / 6 | Linear interpolation, CC out, audio level, sends, plugin lanes |
| `EngineStressTests.cpp` | 275 / 4 | Thousands of transitions, note pairing, device restart, extreme songs |
| `TheoryTests.cpp` | 273 / 9 | Keys, scales, key finding, modulations, roman numerals, voice leading |
| `MusicXmlTests.cpp` | 269 / 9 | Round-trip, triplets, drums, foreign files, chord/lyric/mark survival |
| `AssistantTests.cpp` | 251 / 3 | Program quoting/parsing, API tool loop |
| `PsrFileRulesTests.cpp` | 242 / 2 | PSR-S970 file rules across demo songs |
| `ControllerProfileTests.cpp` | 240 / 9 | All pages, modes, layers, channel scoping, pad rows, round-trip |
| `SongTests.cpp` | 228 / 11 | Tempo map, colours, immutable clips, undo, quantise, solo |
| `ScoreSnapshotTests.cpp` | 223 / 4 | Layout vs `tests/golden/score-*.json` |
| `PluginHostTests.cpp` | 223 / 8 | VST3 load/state/note, scan child, crash/hang limits, latency change |
| `AudioAnalysisTests.cpp` | 220 / 10 | Chord finding, slash bass, resampling, helper JSON, stems, words |
| `MidiInputPolicyTests.cpp` | 207 / 9 | Port recognition, controller faders, user-off memory, key ports |
| `DeviceChartTests.cpp` | 201 / 4 | Exact bank/program/reset bytes per device chart |
| `TimingTests.cpp` | 174 / 4 | Tempo-map exactness, loop repeat, grid launch, drift |
| `SamplerTrackTests.cpp` | 174 / 4 | Looping clips, per-track sampler, mixer level, project save |
| `ProjectFileTests.cpp` | 174 / 9 | JSON shape, clamping, bad files, duplicate ids, v1 compat |
| `ExchangeRoundTripTests.cpp` | 174 / 3 | MIDI + MusicXML round-trip across example songs |
| `DrumSamplerTests.cpp` | 170 / 6 | Pads, velocity/pan, rate/tune, slices, choke, voice stealing |
| `MidiFileTests.cpp` | 159 / 5 | Export/import, format 0 split, lyrics, corruption |
| `PsrExportTests.cpp` | 154 / 2 | PSR-S970 song export, loops, automation, chords |
| `McpArgumentTests.cpp` | 154 / 1 | Every tool against missing/wrong/extreme arguments |
| `MetronomeTests.cpp` | 150 / 7 | Exact beat samples, 1-hour drift, buffer independence, accents |
| `AuditRegressionTests.cpp` | 150 / 5 | Handoff lifetime, engine restart, loop join/split, stale AI answer |
| `LocalizationTests.cpp` | 145 / 6 | Codes, full key coverage, pseudo locale, Vietnamese glyphs |
| `ClipboardTests.cpp` | 144 / 4 | Clip/note/automation/slot copy-paste |
| `ChordTests.cpp` | 138 / 4 | Chord finding, passing notes, guitar shapes |
| `RealtimeTests.cpp` | 128 / 2 | No allocation on the audio thread; counter sanity |
| `SampleTests.cpp` | 120 / 6 | WAV 16/24/32, disk I/O, transients, slicing |
| `VideoAudioTests.cpp` | 116 / 6 | Video extensions, first audio stream, no-audio, Unicode, damaged |
| `ProjectRoundTripTests.cpp` | 115 / 4 | Kitchen-sink model coverage, byte-stable re-save, old releases |
| `StyleSampleTests.cpp` | 88 / 1 | Real styles round-trip every part and rule |
| `SoundFontTests.cpp` | 85 / 3 | SF2 presets, missing/broken file, timed playback |
| `EngineTests.cpp` | 70 / 4 | Play/stop/resume, locate, lock-free queue, buffer growth |
| `CrashReport.cpp` | 68 / 0 | Harness: no dialogs on Windows, readable crash output |
| `SkillsTests.cpp` | 65 / 2 | Built-in pack, user pack discovery and error reporting |
| `KidsModeTests.cpp` | 65 / 6 | Quiet/tap/held keys, lost note-off, range, reset |
| `LinkAudioTests.cpp` | 58 / 3 | Link validation, yt-dlp arguments, install help |
| `MusicFinderTests.cpp` | 57 / 2 | Book round-trip, Yamaha MFD import |
| `RecentFilesTests.cpp` | 51 / 3 | Ordering, dedupe, missing files, limit |
| `MessageThreadTests.cpp` | 49 / 2 | Cancellation of queued work, exception propagation |
| `MidiPanicTests.cpp` | 44 / 3 | Panic silences all, drops queued, empty-queue safety |
| `TimeTests.cpp` | 41 / 3 | Bars/beats by meter, display, tempo range |
| `RecorderImportTests.cpp` | 37 / 1 | L6max folder: newest take per channel, mix last |
| `D05ReverbTests.cpp` | 31 / 1 | Documented D-05 reverb addresses |
| `PsrStyleTests.cpp` | 29 / 3 | PSR section/tempo/chord/split-point messages |
| `VersionTests.cpp` | 27 / 3 | Semver compare, stable > prerelease, invalid |
| `PlaylistTests.cpp` | 20 / 1 | Yamaha Playlist round-trip |
| `SysexFileTests.cpp` | 19 / 1 | Complete-message split/join |
| `PluginTestsMain.cpp` | 15 / 0 | Catch2 main + plugin-scan child entry |

### C1. Fixtures, golden, fuzz
| Path | Size | Format |
|---|---|---|
| `tests/fixtures/tone.mp4` / `.mkv` / `.webm` | 7,331 / 49,151 / 6,503 B | FFmpeg 9.0.2 synthetic: one black 16x16 frame + 440 Hz then 880 Hz, 0.25 s each. mp4 = MPEG-4+AAC, mkv = MPEG-4+PCM s16, webm = VP9+Opus |
| `tests/fixtures/silent-video.mp4` | 893 B | video only, no audio stream |
| `tests/fixtures/README.md` | 819 B | exact `ffmpeg` commands; states "contain no third-party media" |
| `tests/golden/score-{ties-accidentals,beams-triplets,grand-staff-marks,six-eight,drums}.json` | 4,654 / 4,548 / 3,188 / 1,982 / 2,954 B | layout snapshots, one per hard-bar case |
| `tests/golden/score-long.json` | 52,751 B | 3-page system/page-break snapshot |
| `tests/golden/render-reference-song.json` | 1,180 B | render baseline |
| `tests/fuzz/` — `FuzzMain.cpp`, `FuzzTargets.cpp`, `FuzzTargets.h`, `README.md`, `regressions/style/note-number-over-127.bin` | 25,717 / 5,860 / 730 / 934 B + 1 seed | driver (`--list`, `--seconds`, `--seed`, `--out`, replay); 8 targets: `project`, `midi`, `musicxml`, `style`, `wav`, `device`, `controller`, `mcp`; 5 s per target in normal runs, 10 min nightly; saved regressions replay every run |

**Golden format.** Score snapshots are JSON from `describe()`
(`ScoreSnapshotTests.cpp:34`). Per page: `size [w,h]` plus four **sorted** arrays
— `glyphs [[codepoint,x,y,scale],…]` (2 dp; SMuFL codes such as 57424 = 0xE050),
`lines [[x1,y1,x2,y2,width],…]`, `shapes [flat point list,…]`,
`texts [{text,x,y},…]`. `systems` and `pages` counts sit at the top. Sorting plus
rounding makes the diff show *what moved*. Comparison is tolerant (`near()`);
`ARRANGER_UPDATE_GOLDEN=1` rewrites the files.
`render-reference-song.json` holds `firstOnsets` (sample indices) and `windows`
(`peak`, `rms` per quarter second).

### C2. Timing and parity tests that already exist
| Concern | Test |
|---|---|
| Sample-exact dispatch through 30 tempo changes; no drift at an odd rate | `TimingTests.cpp:48,145` |
| Loop repeats on identical samples; grid launch lands on the next bar | `TimingTests.cpp:78,110`; `SequencerTests.cpp:82,96` |
| Metronome 1-hour drift, buffer independence, accents | `MetronomeTests.cpp:37-136` |
| No allocation on the audio thread | `RealtimeTests.cpp:88` |
| Buffer-size independence; SF2 onset within 64 samples; golden mix | `RenderTests.cpp:11,251` |
| Score layout against goldens | `ScoreSnapshotTests.cpp` (4 cases) |
| MIDI and MusicXML parity across every example song | `ExchangeRoundTripTests.cpp` |
| Project save→load→save byte stability | `ProjectRoundTripTests.cpp` |
| Plugin latency compensation keeps tracks together | `RackTests.cpp` |
| Panic, All Notes Off, sustain lift on stop | `SequencerTests.cpp`, `MidiPanicTests.cpp` |

## Scope D — licence inventory

### D1. No licence, and never one

- Repo root: **no** `LICENSE`, `COPYING`, `NOTICE` (verified).
- `git log --diff-filter=D --name-only -- 'LICENSE*' 'COPYING*'` → **empty**; a removed licence would appear here.
- `git log --all --oneline -- 'LICENSE*'` → **empty**. Never committed on any ref.
- Whole-tree scan finds only two licence files, both for bundled fonts: `resources/fonts/Bravura-LICENSE.txt`, `resources/fonts/Inter-LICENSE.txt`.
- `docs/release-notes-v0.1.0.md:61`: *"The application source code is private. Use the binaries only according to the licences of their components."* Repeated at `docs/release-notes-v0.1.1.md:42`.
- No `license`/`GPL`/`MIT`/`Apache` claim about the app in `README.md` or `docs/`. Default: **all rights reserved**.

### D2. Third-party inventory
| Dependency | Version | Licence | Evidence (file:line) | Flag |
|---|---|---|---|---|
| JUCE | 8.0.15 | Dual: AGPLv3 **or** commercial | `CMakeLists.txt:75-78`; terms discussed `docs/build-plan.md:28,178` | **RISK — copyleft or paid** |
| FluidSynth | v2.5.7 | LGPL-2.1 | `src/soundfont/CMakeLists.txt:1,16-17` | **weak copyleft, statically linked** |
| whisper.cpp | v1.8.7 | MIT | `src/whisper/CMakeLists.txt:1,3-5` | ok |
| nlohmann/json | 3.11.3 | MIT (upstream; not in repo) | `CMakeLists.txt:39-41` | ok |
| yaml-cpp | 0.9.0 | MIT (upstream; not in repo) | `CMakeLists.txt:53-56` | ok |
| Catch2 | v3.7.1 | BSL-1.0 (upstream; not in repo) | `tests/CMakeLists.txt:1-4` | ok, test-only |
| Bravura (SMuFL) | — | SIL OFL 1.1 | `resources/fonts/Bravura-LICENSE.txt:4` | **OFL: derivatives must stay OFL** |
| Inter Regular/SemiBold | — | SIL OFL 1.1 | `resources/fonts/Inter-LICENSE.txt:3` | same |
| GeneralUser GS SoundFont | downloaded | Custom permissive ("use and redistribution in software") | `src/app/GmSoundFont.cpp:11-14` | **terms unverified — read upstream** |
| `@magenta/music` | 1.23.1 | Apache-2.0 | `resources/ai/magenta_package.json:5` | ok; checkpoints separate |
| `tone` (JS) | 14.7.77 | MIT (upstream; not in repo) | `resources/ai/magenta_package.json:7-8` | ok |
| Anticipatory Music Transformer | — | Apache-2.0 | `resources/ai/arranger_generate.py:12-14` | ok; weights from Stanford CRFM |
| Demucs (htdemucs) | pip | MIT | `docs/technical-overview.md:56` | ok |
| Basic Pitch | pip `basic-pitch[onnx]` | Apache-2.0 | `docs/technical-overview.md:57` | ok |
| PANNs (Cnn14) | pip | MIT code; **weights CC BY 4.0** | `docs/technical-overview.md:58` | **attribution required** |
| sherpa-onnx (TTS) | v2.0.1 | Apache-2.0 (upstream; not in repo) | `src/app/Speech.cpp:26-37` | unverified in-repo |
| yt-dlp + ffmpeg | user-installed | Unlicense / LGPL+ | `LinkAudio.h:1-9` | not bundled |
| FluidR3_GM.sf2 | system package | — | `tests/GrooveTests.cpp:111,293` | optional; skipped when absent |

Explicitly **rejected**, `docs/technical-overview.md:67`: Chordino (GPL) and the
madmom models (non-commercial) — the only documented licence-policy decision.

### D3. Verdict for a Rust derivative

The repo grants **no licence** (`docs/release-notes-v0.1.0.md:61` calls the source
private). A Rust product cannot take code, data or assets from it; only behaviour
can be re-implemented.

| Category | Can a Rust product… | Why |
|---|---|---|
| Protocol behaviour (MIDI bytes, SysEx checksums, banks) | **Re-implement freely** | Facts of the MIDI spec, not copyrightable |
| File formats (`.sty`, `.arranger`, `.pgl`) | **Re-implement from observation** | Format facts; do not copy parser code |
| Algorithms (WSOLA, Krumhansl-Schmuckler, Viterbi chords, groove) | **Re-implement** | Published methods; code must be independent |
| YAML device profiles (8 + 3) | **Re-author** | The files are the repo's expression; device facts are not |
| SMuFL codepoints / Bravura glyph IDs | **Re-derive from the SMuFL spec** | Public spec; do not lift the mapping |
| Bravura.otf, Inter-*.ttf | **Ship OFL copies with notices, or re-license** | OFL 1.1 forbids relicensing |
| GeneralUser-GS.sf2 | **Do not bundle** until terms are read | Redistribution wording is not in the repo |
| AI model weights | **Re-download at runtime** | Apache/MIT code ≠ weights; PANNs weights CC BY 4.0 |
| Test goldens, example songs, `docs/images` | **Do not copy** | Repo-authored expression |
| JUCE-derived UI design | **Design afresh** | JUCE is AGPL-or-paid; a Rust UI avoids it entirely |

A derived Rust product may claim **any licence it chooses** (MIT, Apache-2.0,
proprietary) if it copies no code from this repo, ships OFL notices with any
Bravura/Inter copies, attributes CC BY 4.0 PANNs weights and Apache-2.0 helpers it
redistributes, and does not statically link FluidSynth without LGPL-2.1 relinking
rights — which a rewrite sidesteps by not using FluidSynth.

**Must be re-created, not copied:** the two fonts (or fetch upstream under OFL),
the GeneralUser GS SoundFont, all AI model weights, the 8 device + 3 controller
profiles, the 6 score goldens + 1 render baseline, the example songs, and the
Yamaha/Korg style fixtures.

---

### Unverified in this audit

- Exact JUCE 8.0.15 terms: the repo cites a revenue-cap model matching JUCE 7; JUCE 8's split was not read upstream.
- GeneralUser GS redistribution terms: only the repo's one-line summary.
- sherpa-onnx, nlohmann/json, yaml-cpp, Catch2, `tone` licence text: upstream-known, not read from this repo.
- Whether `docs/` prose or the example songs embed third-party lyrics or melodies.
