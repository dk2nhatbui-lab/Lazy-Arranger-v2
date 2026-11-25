# Screen-by-Screen Comparison: C++ v1 (`arranger-midi-daw`) vs Rust v2 (`Lazy Arranger v2`)

_Audit date: 2026-11-24 | Basis: `origin/main` (`91d9d01`) + open branches `claude/project-thread-053s5u` & `claude/project-thread-81i9dn` | Reference: `/Volumes/LazyRepos/arranger-midi-daw/src/app/ui/`_

This document compares every main screen of the C++/JUCE v1 application against the Rust/egui v2 desktop shell, separates bugs already fixed on branch `agy/ui-parity-fixes` (zero-overlap with Claude's open branches) from items deliberately deferred to avoid conflicts with `053s5u` (`app/mixer.rs`, `widgets.rs`) and `81i9dn` (`app.rs`, `app/layout.rs`, `app/files.rs`, `menu.rs`), and ends with a live visual verification checklist for Claude Computer Use.

---

## 1. Fixed on `agy/ui-parity-fixes` (Zero-Conflict with `053s5u` & `81i9dn`)

| Screen / Module | C++ v1 Reference | Gap / Bug Found in Rust v2 | Fix Applied |
| --- | --- | --- | --- |
| **Theme** (`theme.rs`) | `resources/themes/{dark,light}.json` (`ok`, `warning`) | Missing `ok` and `warning` semantic tokens needed for active `S`/`M` states and audio status dot | Added `theme::ok()` (`#91ad7c` / `#517d45`) and `theme::warning()` (`#dca45f` / `#b37632`) with unit tests |
| **Timeline** (`timeline.rs`) | `TimelineViews.cpp` | **Inverted pitch Y-axis**: `top + LANE_HEIGHT * 0.55 + offset` drew higher pitches *below* lower pitches | Fixed via tested `note_lane_y()` (subtracts offset so higher MIDI pitch draws higher) |
| **Timeline** (`timeline.rs`) | `TimelineViews.cpp`, `v1-visual-language.md` | Playhead used red `theme::recording()` instead of v1 gold `theme::playhead()`; no beat subdivisions, no song markers in ruler, no loop highlight helper | Switched playhead to `theme::playhead()` + ruler triangle cap; added `grid_bar()`/`grid_beat()` lines, song markers in ruler, exported `HEADER_HEIGHT`/`LANE_HEIGHT`, and added `show_with_loop()` while keeping `show()` signature unchanged |
| **Piano Roll** (`piano_roll.rs`) | `PianoRoll.cpp:450-620` | **Velocity lane drops all edits**: clicks/drags on `lane_response` were discarded because `response.interact_pointer_pos()` returned `None` -> early `RollOutcome::default()` | Early-return `RollOutcome { velocity, .. }` when `velocity.is_some()`, plus `"Velocity"` label and top separator |
| **Piano Roll** (`piano_roll.rs`) | `PianoRoll.cpp:566-603` | **Piano keys misaligned by 54 px (~5.4 semitones)** and painted over Clip/Chord/Lyric lanes (`draw_keys` used `rect.top()` instead of `geometry.y_at(pitch)`); keyboard clicks lacked `pointer.y >= geometry.origin.y` guard | Fixed via tested `key_row_rect()` aligned to `geometry.y_at(pitch)`, clipped to `y >= geometry.origin.y`, octave labels `C0..C8` on white keys, and guarded audition clicks |
| **Piano Roll** (`piano_roll.rs`) | `PianoRoll.cpp:459-524` | Duplicate bar numbers drawn inside clip lane by `draw_grid`; no black-key row shading or octave lines; hardcoded sharp chord spelling; flat note alpha; no note drag/resize ghost preview, note pitch labels, or hovered key highlight | Removed duplicate bar numbers, added black-key row shading & `C` octave dividers, velocity-shaded note fills, key-aware chord spelling, playhead cap, live drag/resize ghost preview, note pitch labels inside wide notes (`pitch_name`), and hovered piano-key highlight |
| **Automation Lane** (`automation_view.rs`) | Track automation lanes | Hardcoded `"volume automation"` label even when editing Pan/Send/Brightness/PluginParam; no 50% midpoint line or drag-to-move points | Dynamic `target_label(&lane.target)` (tested), midpoint guide line, border, hover tooltip, and drag-to-move existing automation points |
| **Assistant** (`app/assistant.rs`) | `AiPanel.cpp`, `ChatView.cpp` | Empty-log hint had 37 stray spaces before `tool add_track`; `assistant_inline` bypassed configured AI endpoint, local CLI command, and TTS speech | Extracted shared `submit_assistant_input()`, fixed hint formatting, and added waiting/proposal indicator in `assistant_inline` |
| **Beat Grid & Dialog** (`crates/arranging/src/beat.rs`, `app/style.rs`) | `BeatView.cpp:15` | Only 4 drum rows (`Kick, Snare, Hat, Open`), flat unshaded buttons, no beat grouping or Clear All | Upgraded to C++ v1's 8-row GM drum kit (`Kick, Snare, Hat, Open, Clap, Tom, Crash, Ride`, preserving indices 0..3), added velocity shading, 4-step beat spacing, fixed-width row labels, and `Clear all` |
| **Groove Library** (`app/style.rs`) | `GrooveView.cpp` | Missing Play Before / Play After A/B audition | Added `Play before`, `Play after`, and `Stop` audition buttons using `audition_player` |
| **Score Engraving** (`crates/score/src/lib.rs`) | `ScoreLayout.cpp` | Fixed staff-center rest placement without vertical note-collision avoidance | Added `avoid_rest_collisions()` shifting rests to half-space increments away from colliding nearby notes, with unit tests |
| **Yamaha Style Writer** (`crates/style-formats/src/write.rs`) | `StyleFile.cpp:591-675` | `write_style` lacked `SFF1`/`SFF2` + `SInt` setup bar with per-channel voice/mixer initialization | Added `SetupVoice`, `setup_voices_from_song()`, and Format 0 `write_style_with_setup()` emitting `SFF1`/`SFF2` + `SInt` + Bank MSB/LSB, Program Change, Volume CC 7, Pan CC 10, verified by round-trip import test |
| **Novation XioSynth Editor** (`crates/midi-devices/src/xiosynth.rs`, `app/devices.rs`, `assets/profiles/xiosynth.yaml`) | `resources/devices/novation-xiosynth.yaml`, `SoundEditorView.cpp` | Generic CC panel with wrong GM CC numbers (including CC 7 which XioSynth ignores) | Fixed `xiosynth.yaml` CCs and added full 10-page / 104-parameter CC+NRPN XioSynth model + 200 factory Bank A/B patches wired into `device_editor_window` |
| **MIDI Map Window** (`app/devices.rs`) | `MidiMapView.cpp` | Only exposed `TrackFader` for track 1 and had no per-binding delete | Lists `TrackFader` for all tracks and adds per-binding `×` unbind button |

---

## 2. Deferred Findings in Claude's Open-Branch Files (Fold After Merging `053s5u` / `81i9dn`)

To avoid merge conflicts, the following verified bugs were **not** edited on `agy/ui-parity-fixes` and should be folded in once `053s5u` and `81i9dn` land (a reference pre-split patch is also saved in Antigravity's scratch folder):

### A. `apps/desktop/src/widgets.rs` (owned by `053s5u`)
1. **LCD cell width overflow:** `LCD_SIZE.x = 330.0`, while `CELLS = [142.0, 116.0, 92.0]` sums to `350.0` — pushing the `METER` text (`4 / 4`) into the right bezel. Scale `CELLS` to `[132.0, 110.0, 88.0]` (`= 330.0`) or widen `LCD_SIZE.x` to `350.0`.
2. **Non-interactive LCD:** `lcd()` uses `Sense::hover()`. In C++ v1 (`HeaderBar.cpp:192-237`), dragging vertically on `TEMPO` adjusts BPM (`0.25` normal / `0.01` with Shift), scroll wheel adjusts ±1 BPM, double-click edits tempo, and clicking `METER` opens a time-signature popup (`2/4, 3/4, 4/4, 5/4, 6/8, 7/8, 9/8, 12/8`).
3. **Duplicate active-tab underline in `view_tabs()`:** Draws both a full-width bottom stroke at `tab.bottom()` AND a centered 26 px accent bar at `tab.bottom() - 7.0`. C++ v1 (`HeaderBar.cpp:317-327`) draws only the 14% accent fill, 40% accent outline, and single centered 26 px underline.

### B. `apps/desktop/src/app/mixer.rs` (owned by `053s5u`)
1. **Wrong dB readout formula (`mixer.rs:201, 273`):** Displays `f32::from(volume) - 100.0` (e.g. `+27.0 dB` at 127, `-30.0 dB` at 70, `-100.0 dB` at 0). C++ v1 (`MixerView.cpp:18, 192-195`) uses the GM curve: `0 => "-inf"`, else `40.0 * (v / 127.0).log10()` (`127 -> 0.0 dB`, `100 -> -4.2 dB`, `70 -> -10.3 dB`).
2. **Uncolored `M`/`S`/`R` buttons (`mixer.rs:130-154, 238-244`):** Plain `ui.small_button("M")` / `"S"` / `"R"` give no visual active state. Color active `M` (`theme::warning()`), `S` (`theme::ok()`), `R` (`theme::recording()`), highlight the selected track strip, and show accurate track subtitles (`→ BusName` / `Audio` / `SoundFont` / GM program name instead of hardcoded `"Built-in SoundFont"`).
3. **Stale `My Plug-Ins` banner (`mixer.rs:714`):** Still says *"Installed bundles are listed here. Hosting and editors are not implemented."* even though VST3/AU hosting and macOS native UI editors are implemented.
4. **Knob UX (`widgets.rs` / `mixer.rs`):** Ensure `mixer_knob` supports Shift-drag fine adjustment, scroll wheel, and double-click reset (Pan → 64, Rev → 40).

### C. `apps/desktop/src/app/layout.rs` (owned by `81i9dn`)
1. **Backwards Timeline zoom (`+` / `-`) in `top_bar`:** Clicking `+` multiplies `timeline_zoom * 0.8` (zooms OUT) and `-` multiplies `timeline_zoom * 1.25` (zooms IN) because `direction` was written for `roll.ticks_per_point` (inverse scale). Invert the factor for `timeline_zoom`.
2. **Sidebar `M`/`S` buttons disarm `R` in `track_row()`:** Toggling Mute or Solo passes `false` for `armed` instead of `row.armed`! Also color active `M`/`S`/`R` buttons using `theme::warning()`, `theme::ok()`, `theme::recording()`.
3. **Track List vs Timeline vertical misalignment (`app-bundle.png`):** The `Loop` / `From bar` row sits above the track list and each row is `60 + 4 + 4 = 68 px` tall, drifting against Timeline's `50 px` header (`timeline::HEADER_HEIGHT`) and `64 px` lanes (`timeline::LANE_HEIGHT`). Match the top header to `timeline::HEADER_HEIGHT`, use `item_spacing.y = 0.0` with exact `timeline::LANE_HEIGHT` rows, and move `Loop` / `From bar` (and wire `timeline::show_with_loop`!) cleanly.
4. **Header Bar polish:** Add detected song key to subtitle (`108 BPM · 4/4 · A minor`), draw the v1 4-point sparkle icon on `Ask AI` and make it toggle `assistant.open`, and drive the audio pill dot from `theme::ok()` / `theme::warning()`.
5. **Right Inspector polish:** The `"Find"` button next to the detected key currently sets `self.songs_open = true` (`"Find songs"`) instead of re-detecting/reporting the song key (`Panels.cpp:166-185`). Add `9/8` & `12/8` to the Meter ComboBox and show `Effects (N)…` when plug-ins are loaded.
6. **Inline `View::Drums` & `View::Map`:** Add current bar range label (`Bar 1`), horizontal `ScrollArea` (prevents 2-bar / 4-bar overflow), velocity shading, beat gaps, row `C`/`P`/`X` + `Clear all` to inline `View::Drums`; expand `View::Map` with track color badges, `M/S/R` columns, inline `Learn` buttons, and Global bindings (`Play`, `Stop`, `Undo`).

---

## 3. Screen-by-Screen Coverage Matrix (C++ v1 vs Rust v2)

| C++ v1 Screen | Rust v2 Location | Visual Parity Status | Notes for Computer Use Comparison |
| --- | --- | --- | --- |
| `HeaderBar` | `app/layout.rs`, `widgets.rs` | Partial (bugs in §2A, §2C) | Compare LCD bezel/cells, single active-tab underline, sparkle on `Ask AI`, zoom `+`/`-`, menu bar (`81i9dn`) |
| `TrackList` + `Timeline` | `app/layout.rs`, `timeline.rs` | Improved (alignment in §2C) | Verify row-to-lane alignment, upward pitch rendering, gold playhead cap, beat grid, markers |
| `PianoRoll` | `piano_roll.rs` | Fixed on this branch | Verify keys align 1:1 with notes, no overlap on clip/chord/lyric lanes, velocity bars respond to drag |
| `Automation` | `automation_view.rs` | Fixed on this branch | Verify dynamic target label (Pan/Send/Brightness/Plugin) & point dragging |
| `MixerView` | `app/mixer.rs`, `widgets.rs` | Active in `053s5u` | Compare `053s5u` brushed metal look + fold in §2B (GM dB curve, colored M/S/R, stale plugin text) |
| `Inspector` | `app/layout.rs` | Partial (§2C #5) | Compare Track/Song/Metronome/Audio/MIDI sections + inline Assistant |
| `BeatView` | `app/layout.rs`, `app/style.rs` | Dialog upgraded; inline in §2C #6 | Compare 8-row default drum kit vs 4-row grid, velocity shading, horizontal scroll |
| `ClipGridView` (`Loops`) | `app/layout.rs` (`grid_panel`) | Basic | C++ v1 has launch quantise picker, From Timeline, Stop All, Style chord menu, colored scene grid |
| `MidiMapView` (`Map`) | `app/layout.rs`, `app/devices.rs` | Window upgraded; inline in §2C #6 | C++ v1 has full 11-column controller matrix + Global row |
| `ScoreView` | `score_view.rs`, `app/layout.rs` | Strong (Cecilia + Full Score) | Compare toolbar density, page zoom, dark/light contrast around white page |
| `AiPanel` / `ChatView` | `app/assistant.rs` | Fixed on this branch | Verify inline Assistant routes through configured endpoint/CLI; compare v1 right-docked `AiPanel` tool switcher |
| `KidsView` | `app/layout.rs`, `kids.rs` | Good (`docs/audit/ui-kids.png`) | Compare pulsing play circle & chord pad layout |
| `L6maxMixerView` / `SoundEditorView` | `l6max_device.rs`, `d05_editor.rs`, `app/devices.rs` | Strong (`l6max-device.png`, D-05 verified) | Compare knob/slider alignment and light/dark readability |

---

## 4. Next Steps for Claude at 12:50 AM (Computer Use Checklist)

1. Run `git fetch origin` and check `agy/ui-parity-fixes` (zero-conflict branch off `91d9d01`).
2. Merge or finish `053s5u` (brushed metal mixer) and `81i9dn` (menu bar + native file dialogs), folding in §2A–§2C.
3. Rebuild the macOS bundle (`./scripts/rust.sh build --release -p lazy-arranger-desktop && ./scripts/bundle-macos.sh`) and run side-by-side Computer Use screenshot comparisons of:
   - Timeline + TrackList row alignment + Piano Roll key/note alignment + Velocity lane drag
   - Header Bar (LCD meter popup, TEMPO drag, single tab underline, `+`/`-` zoom)
   - Brushed metal Mixer (`053s5u`) with GM dB labels and colored `M`/`S`/`R`
   - Drums (`View::Drums` & floating Beat dialog), Loops (`ClipGridView`), and MIDI Map (`View::Map`)
   - Dark theme vs Light theme contrast on all five main tabs
