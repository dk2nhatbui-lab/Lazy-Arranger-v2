# Source Code Audit Report — Round 2 (Post-PR #24 & PR #26)

- **Audited commit:** `503d0aa` (`v0.1.0-beta.2`, branch `main`)
- **Delta audited:** `a3f786d..503d0aa` (77 files changed, +4,113 / −2,183 LOC — PR #24 all-tracks score layout & PR #26 Round 1 audit fixes)
- **Baseline checks:** `./scripts/rust.sh fmt --all --check` passed locally; GitHub Actions CI passed on PR #26 (`aa255b9`).

## Status (2026-11-26)

| Finding | Status | Where |
| --- | --- | --- |
| R2-C1, R2-H1 to R2-H12, M1 to M15, M18 (groove), M19, M21 | Fixed | PR #28 |
| R2-H13 (UI-thread blocking: export, ffmpeg, print, PDF, transcription, auditions) | Fixed | PR #29 |
| M16 (`performance::regenerate` is one undo step, no channel 1 guess) | Fixed | Follow-up PR |
| M17 (exact undo for `SetLoops`, `SetMarkers`, `ReplaceTrackNotesAndClips`; `validate.rs` dedup) | Fixed | Follow-up PR |
| M18 (grid and forms edits are one undo step) | Fixed | Follow-up PR |
| M20 (MIDI ports listed at most every 2 s, no catalog read during a scan, grooves listed once; disconnects reported) | Fixed | Follow-up PR |
| M22 / M9 (linear-time SMF, style and MusicXML parsing) | Fixed | Follow-up PR |
| `ModuleEntry`/`ModuleExit` (Linux), `InitDll`/`ExitDll` (Windows) | Fixed | Follow-up PR |
| `// SAFETY:` comments | Fixed per module | Follow-up PR |
| Low: test-plugin `iid` null check and `i32` sizes, AU `apply_state` non-finite, `wav::info` offsets, `to_wav32f` and `mix_accent` non-finite, D-05 patch index, `Book::from_mfd` zero tempo/meter, `draw_system` bar ends, PDF CJK width and control characters, dotted-eighth beamlet, `DrumFill` negative ticks | Fixed | Follow-up PR |
| Low: `player.rs` `looping` vs `loop_bounds` order | No change needed: bounds are stored before the flag with Release/Acquire | — |
| macOS VST3 `bundleEntry` | Left: needs a `CFBundleRef`; plug-ins load without it | — |

## Executive Summary

PR #26 resolved the majority of Round 1 Critical and High issues (MCP `set_plugins`/`plugin_scan`/`reverse_audio`/`groove_from_audio`, MCP socket line caps & CLI resilience, desktop startup & Kids-demo data loss, Cecilia preview deadlock, `ParameterChanges` COM leak, `unit_parameters` overflow, `HostApplication::getName`, `MemoryStream`, `ProjectFile::from_json` header-first check, no-op undo guard, `RemoveNote` undo order, `Pitch::from_name`, `TempoMap`, `TransitionState::poll`, `unicode_font` caps, and C-minor spelling).

Round 2 verified every Round 1 fix and audited all new/refactored code (`crates/audio/src/spsc.rs`, `crates/audio/src/player.rs`, `crates/score/src/full.rs`, `crates/score/src/clef.rs`, `apps/desktop/src/score_view/full.rs`, `crates/plugin-host/src/*`, `crates/mcp/src/*`, `apps/desktop/src/*`, `apps/cli/src/lib.rs`), finding:

| Severity | Count | Key Themes |
| --- | --- | --- |
| **Critical** | 1 | **New regression in PR #26:** `ComPtr<IPluginFactory>` drops after `dlclose()` on init errors (`Vst3Effect`/`Vst3Instrument`) |
| **High** | 13 | `AuEditor`/`Vst3Editor` `autoreleasepool` UAFs, `AuInstrument` MIDI chunk offset rebasing, `spsc.rs` non-pow2 wrap corruption, `player.rs` `(pos, playing)` race + F32 sanitization + odd L/R swap, `sequencer::publish` stuck notes, `ai.rs` `BrokenPipe` zombie/error mask, `checked_output` dir/leaf symlinks, `score/full.rs` `slots` overflow & `Inf` hang, `track.slots` validation, `casm.rs` multi-group `CSEG`, remaining desktop UI-thread blocking |
| **Medium** | 22 | New regressions (`time.rs` `at_or_after` rounding down near `i64::MAX`, VST3 `Drop` calling `setProcessing(0)`/`setActive(0)` on partial init, `mix_audio_reference` panic on stretch overflow, `sequencer` `TrackId(1)`/1-bit tracking, `sysex_handshake` deadline bypass, `monitor.rs` 2s latency ratchet, `score/full.rs` multi-system slurs & odd-meter beaming & negative column scale, PDF `/Title` PDFDocEncoding) + Round 1 carryovers |
| **Low** | 16 | Edge-case overflows, minor ABI/PDF/UI polish, `// SAFETY:` comments |

---

## Round 1 Remediation Scorecard

| Area | Verified Fixed | Partially Fixed / Regressed | Deferred / Unfixed |
| --- | --- | --- | --- |
| **Plugin-Host & Plugins** | C1 (mostly), C3, C5, H3, H6–H8, M10 (`parse_cid`, `args_os`, `vst3_binary`, `CFStringGetCString`, UTF-16) | **C2** (`AuInstrument` MIDI offsets), **C4** (`autoreleasepool` UAF), **H5** (**NEW Critical `factory` UAF** + partial-init `Drop`), H9 (aux buses), H10 (`iid.is_null`), M11 (`read_capped` TOCTOU) | M10 (`ModuleEntry`/`InitDll`, `// SAFETY:` comments) |
| **Security, MCP, AI, Speech, CLI** | C6, H1 (`socket.rs` + CLI), M2 (`dictate.rs`), M18 (most CLI flags) | **C7** (`checked_output` dir/leaf symlinks), **H2** (**NEW `ai.rs` `BrokenPipe` zombie/mask**), M18 (`midi-map-show`, `run_transport`) | H4 (`legacy` `..` paths — intentionally kept) |
| **Desktop Data Loss & UI** | C8, C9, H24, H26 (`open`/`Restore`), M1 (`persistence.rs`), M3 (`data_dir`) | **H25** (`run_autosave`, mixer 60 FPS, freeze names fixed), `replace_project` misses `recovery = None` | Rest of H25, M17 (per-frame polling & channel disconnects) |
| **Audio RT, Concurrency, MIDI** | M16 (`sysex.rs` XG bulk checksum) | **C10** (`spsc.rs` non-pow2 wrap + `record.rs` error mutex), **C11** (`mix_audio_reference` OOB on overflow), **H11–H15** (`player.rs` race + F32 unsanitized + odd L/R swap; `soundfont` loop bounds; `sequencer::publish` + `TrackId(1)`; `sysex_handshake` timeout bypass) | `crates/performance` (`regenerate` channel loss & non-atomic Replace) |
| **Project, Commands, Core, Parsers, Score** | H16, H18, H22, H23, M4, M5, M6, M8 | **H17** (`track.slots` unvalidated), **H19** (`SetLoops`/`SetMarkers` undo), **H20** (**NEW `time.rs` `at_or_after` rounds down near `i64::MAX`**), **H21** (`groove` `NaN` `Amounts` + non-atomic `grid`/`forms`) | M7 (`casm.rs` multi-group `CSEG`), M9 ($O(N^2)$ parsers), M12 (duplicate `validate.rs` loops), M13, M14, `musicxml` control chars |

---

## Critical Finding (1 — New Regression in PR #26)

### R2-C1. `ComPtr<IPluginFactory>` Drops After `libloading::Library` (`dlclose`) on Init Errors (UAF / SIGSEGV)
- **Files:**
  - `crates/plugin-host/src/instance.rs:100-136, 175-216` (`Vst3Effect::new_inner`)
  - `crates/plugin-host/src/vst3_instrument.rs:120-152, 159-206` (`Vst3Instrument::new_inner`)
- **Cause:** PR #26 moved `library` into `effect`/`instrument` *after* `factory` was bound as an outer local:
  ```rust
  let library = unsafe { libloading::Library::new(&binary) }...;
  ...
  let factory = unsafe { ComPtr::from_raw(get_factory()) }...; // outer local
  ...
  let mut effect = Self { ..., _library: library };            // moves `library`!
  if inputs == 0 {
      return Err(...); // drops `effect` -> `_library` (`dlclose`), THEN drops `factory` -> vtbl->Release() in unmapped memory!
  }
  ```
  Triggers deterministically on **every** early error return after constructing `effect`/`instrument` (e.g., opening a synth as `Vst3Effect`, opening an effect as `Vst3Instrument`, or any `negotiate_stereo`/`setupProcessing`/`setActive`/`setProcessing` failure).
- **Fix:** Drop `factory` *before* moving `library` into `effect`/`instrument` (in `Vst3Effect`, instantiate the separate `IEditController` before constructing `Self`; in `Vst3Instrument`, `drop(factory)` right after creating `component`).

---

## High Findings (13)

### Plugin-Host / FFI

#### R2-H1. `AuEditor` `au_view` Pinned in Outer Autorelease Pool Past `AudioComponentInstanceDispose` (UAF — Partial C4)
- **File:** `crates/plugin-host/src/editor.rs:466-518, 562-581, 613-636`
- **Details:** `try_custom_cocoa_au_view` calls `[factory uiViewForAudioUnit:au_ptr withSize:preferred]` (returning an autoreleased view in the caller's *outer* pool) and calls `[view retain]` (RC=2). On error in `AuEditor::open` or same-turn open+close, `AuEditor::close`'s inner `autoreleasepool` cannot drain the outer pool, so `au_view` stays alive past `AudioComponentInstanceDispose` (UAF when outer pool drains). Additionally, `AuEditor::close` releases `au_view` *outside* its `autoreleasepool` (line 630) and calls `removeFromSuperview` before `setContentView: nil`; `release_all` is a closure rather than a `Drop` guard.
- **Fix:** Wrap `try_custom_cocoa_au_view`'s ObjC body in `objc2::rc::autoreleasepool` (and guard `buffer` with a `Drop` struct), call `setContentView: nil` before `removeFromSuperview`, and move `[self.au_view release]` inside `autoreleasepool` (including `AuEditor::open`'s error branch).

#### R2-H2. `Vst3Editor::close` Has No `autoreleasepool` Before `dlclose()` (UAF — Related to C4)
- **File:** `crates/plugin-host/src/editor.rs:359-377`
- **Details:** `Vst3Editor::drop` calls `close()` without an `autoreleasepool`, then drops `self.effect` (`dlclose()`). Any autoreleased plug-in `NSView` subclass runs `-dealloc` after its dylib is unmapped (SIGSEGV).
- **Fix:** Wrap `Vst3Editor::close()` (and the `attached != kResultOk` cleanup in `open`) in `objc2::rc::autoreleasepool`.

#### R2-H3. `AuInstrument::note_on_at` / `note_off_at` Do Not Queue/Rebase MIDI Offsets Across `process_block` Chunks (Partial C2)
- **File:** `crates/plugin-host/src/au.rs:669-700`
- **Details:** `AuInstrument::process_block` now chunks `AudioUnitRender` into 4096-frame slices, but `note_on_at`/`note_off_at` still call `MusicDeviceMIDIEvent` immediately with raw caller offsets. Offsets `>= 4096` are delivered before slice 0 with `inOffsetSampleFrame >= inNumberFrames`.
- **Fix:** Queue pending `(offset, status, key, velocity)` events on `AuInstrument` and dispatch them with `(offset - chunk_start) as u32` right before each 4096-frame `render_slice`, matching `Vst3Instrument`.

### Audio RT Safety, Concurrency & Sequencer

#### R2-H4. `SampleQueue` Non-Power-of-Two Wrap-Around Corrupts Unread Slots (NEW in PR #26)
- **Files:** `crates/audio/src/spsc.rs:22-51` (called with `96_000` in `crates/audio/src/monitor.rs:97-100` and `192_000` in `crates/audio/src/record.rs:113-117`)
- **Details:** `head`/`tail` increment via `wrapping_add(1)` while indexing `slots[head % slots.len()]`. When `slots.len()` is not a power of two, wrapping `usize::MAX -> 0` jumps the modulo index and overwrites unread slots (on 32-bit at 192 kHz stereo, after ~3.1 hours). Also runs integer `%` on every RT sample and shares a cache line between `head` and `tail`.
- **Fix:** Round capacity to `capacity.max(1).next_power_of_two()`, index with `& self.mask`, and align `head`/`tail` to separate 64-byte cache lines (`#[repr(align(64))]`).

#### R2-H5. `Playback::fill()` vs `play()`/`pause()` Race Can Silently Kill Playback (Remaining/New in PR #26)
- **File:** `crates/audio/src/player.rs:89-90, 119-136, 275-281`
- **Details:** `playing: AtomicBool` is separate from `position: Cursor(AtomicU64)`. If `fill()` finishes, succeeds in `position.advance(start_raw, 0)` (line 276), and is preempted before `playing.store(false)` (line 277), a concurrent `seek_frame` + `play()` (or `pause()` + `play()`, since `pause()` does not bump the epoch) gets immediately killed when `fill()` resumes line 277.
- **Fix:** Pack the 1-bit `playing` flag inside `Cursor(AtomicU64)` (19-bit epoch + 1-bit playing + 44-bit position) so finishing clears `playing` and rewinds `position` in a single CAS, and bump the epoch in `pause()`/`play()`.

#### R2-H6. `Player` F32 Output Path & `record.rs` Still Do Not Sanitize `NaN`/`Inf`; Odd-Length L/R Channel Swap Remains
- **Files:** `crates/audio/src/player.rs:100, 249-280, 462-469`; `crates/audio/src/record.rs:180-182`
- **Details:** The default F32 output callback writes `shared.fill(output)` without sanitization (only I16 sanitizes), and `record.rs` pushes raw F32 input samples without `sanitize()`. Also, `Playback::new` does not truncate odd `samples.len()` and `fill()` advances `position` by odd `output.len()`, permanently swapping Left & Right channels on subsequent stereo callbacks.
- **Fix:** Sanitize samples once up front in `Playback::new` and in `record.rs`; truncate `samples` to `len & !1` and process only `output.len() & !1` samples in `fill()` (zeroing any trailing odd slot).

#### R2-H7. `Scheduler::publish()` Fails to Release Active Notes on Live Pattern Swap
- **File:** `crates/sequencer/src/lib.rs:278-281`
- **Details:** `publish()` swaps `self.prepared` and calls `seek_to_position()` without setting `self.release_pending = self.has_active_notes()`, leaving stuck MIDI notes whenever a sounding note is edited, deleted, or muted during playback.
- **Fix:** Set `self.release_pending = self.has_active_notes()` in `publish()` (or diff active notes against remaining scheduled NoteOffs).

### Security & Subprocesses

#### R2-H8. `ai.rs` Stdin Write Error Leaks Zombie `curl` & Masks Real Network/HTTP Error (NEW in PR #26)
- **File:** `apps/desktop/src/ai.rs:149-155`
- **Details:**
  ```rust
  if let Some(mut stdin) = child.stdin.take() {
      stdin.write_all(body.to_string().as_bytes())?;
  }
  child.wait_with_output()
  ```
  If `curl` exits early (connection refused, DNS error, TLS failure, 401/413) before reading stdin, `write_all` returns `BrokenPipe` and propagates `?` **without calling `child.wait_with_output()`**, leaking a zombie `curl` process and replacing `curl`'s real stderr message with `"curl: Broken pipe"`.
- **Fix:** Ignore `ErrorKind::BrokenPipe` from `write_all` (dropping `stdin` first) and always call `child.wait_with_output()`.

#### R2-H9. `checked_output` Rejects `OUTPUT_ROOT` Itself for Directory Tools & Does Not Check Leaf Symlinks Inside Output Directories (Partial C7)
- **Files:** `crates/mcp/src/lib.rs:1272-1308, 2547-2582, 3573-3574`; `apps/desktop/src/main.rs:14-16`
- **Details:**
  1. For directory-target tools (`export_stems`, `groove_from_audio`, `save_groove`), passing `OUTPUT_ROOT` itself checks `path.parent().starts_with(root)` which is `false` — falsely rejecting writes directly inside `OUTPUT_ROOT`.
  2. Those directory tools only run `checked_output` on the directory, **not** on the child files written inside it (`directory.join(&file)`, `directory.join("stems.txt")`, `dir.join("{slug}.groove")`), so a pre-existing symlink inside the directory still escapes `OUTPUT_ROOT`.
  3. `apps/desktop/src/main.rs:14-16` sets desktop `OUTPUT_ROOT` to `$HOME` (which includes `~/.bashrc`, `~/.ssh`, etc.) and silently leaves it unrestricted if `HOME` is unset.
- **Fix:** Add `checked_output_dir` allowing `canonicalize(dir).starts_with(root)` and check each child output file for symlinks before writing.

### New Score Engine (`crates/score/src/full.rs`)

#### R2-H10. Overflow Panic in `FullScore::slots`/`tick_at` & Infinite Hang / `NaN` Propagation on Non-Finite `Settings` (NEW in PR #24)
- **File:** `crates/score/src/full.rs:229-234, 726-759`
- **Details:**
  1. `FullScore::slots` computes `page * self.layout.systems_per_page` with unchecked multiplication; public `FullScore::tick_at` does not clamp `page` (overflow panic in debug, page wrap in release).
  2. `Layout::new` does not check `page_height`/`margin` for finiteness: `NaN` propagates into every coordinate (corrupting PDF syntax); `page_height = f32::INFINITY` sets `systems_per_page = usize::MAX`, causing `(0..self.layout.systems_per_page).filter_map(...)` in `slots` to iterate `usize::MAX` times without short-circuiting (**permanent infinite hang**).
- **Fix:** Sanitize non-finite `Settings` fields in `Layout::new`; use `page.checked_mul(...)` in `slots` and bound the range by `self.reference.len().saturating_sub(first)`.

### Remaining High Carryovers from Round 1

#### R2-H11. Unvalidated `track.slots` Bypasses `validate()` and Panics/DoSs `loops::jam_notes` (Unfixed from H17)
- **Files:** `crates/project/src/validate.rs`, `crates/project/src/legacy.rs:736-764`, `crates/arranging/src/loops.rs:296-316`
- **Details:** `track.slots` is still completely absent from `validate.rs`. A `SlotClip` with `length <= 0` or note `start = i64::MIN` passes `ProjectFile::from_json` and overflows `length - note.start` (debug panic) or spins tick-by-tick in `loops::jam_notes`.
- **Fix:** Validate every `Some(slot)` in `track.slots` (`length > 0`, valid unique note IDs, `start >= 0`, `duration > 0`, `start.checked_add(duration)`), and guard `jam_notes`.

#### R2-H12. `write_casm_sff1` and `write_casm_sff2` Corrupt Multi-Group Yamaha Styles (Unfixed from M7)
- **File:** `crates/style-formats/src/casm.rs:719-729, 862-871`
- **Details:** Both functions merge all `SectionRules` groups into a single `CSEG` chunk instead of emitting one `CSEG` per group, silently corrupting every multi-group style on write/re-import.
- **Fix:** Emit one `chunk(b"CSEG", &cseg_children)` per `group` in `groups` inside `CASM`.

#### R2-H13. Remaining Synchronous UI-Thread Blocking in `apps/desktop` (Deferred portion of H25)
- **Files:** `apps/desktop/src/app.rs:645, 2356-2421, 3185-3430`; `apps/desktop/src/app/mixer.rs:2015-2035`; `apps/desktop/src/app/layout.rs:4681-4698`; `apps/desktop/src/app/style.rs:214-730`; `apps/desktop/src/app/assistant.rs:366`
- **Details:** `ExportWav`/`ExportStems`/`ImportAudio` (`ffmpeg`), Freeze track, `lp`/`lpstat`, Score PDF export, audio transcription, and groove/style/proposal auditions still run synchronously on the egui UI thread.

---

## Medium Findings (22)

### New Bugs / Regressions Introduced in PR #24 & PR #26
1. **`TimeSignature::beat_boundary_at_or_after` / `bar_boundary_at_or_after` round DOWN near `i64::MAX`:** `crates/music-core/src/time.rs:114-128` — `tick.saturating_add(beat - 1).div_euclid(beat) * beat` returns `i64::MAX - 127 < tick` for `tick = i64::MAX`. Fix with `tick.checked_add(beat - 1)` returning `Tick::MAX` on `None`.
2. **`Vst3Effect::drop` / `Vst3Instrument::drop` call `setProcessing(0)` & `setActive(0)` on partial init failure:** `crates/plugin-host/src/instance.rs:123-218, 504-524`; `crates/plugin-host/src/vst3_instrument.rs:143-208, 396-404` — Track `is_active`/`is_processing` booleans and guard teardown calls.
3. **`mix_audio_reference` panics if `stretch_interleaved` returns `Vec::new()` on overflow (plus silence gaps when `source_frames < HOP`):** `crates/audio/src/lib.rs:204-269, 357-358`.
4. **`Scheduler::release_active()` hardcodes `TrackId(1)` and uses 1-bit active tracking:** `crates/sequencer/src/lib.rs:253, 388-421` — Breaks per-track NoteOff routing on Track 2+ and loses tracking on overlapping same-pitch notes.
5. **`sysex_handshake` `HANDSHAKE_LIMIT` bypass & unbounded `sysex_exchange`:** `crates/midi-devices/src/devices.rs:427-501` — `recv_timeout(Duration::ZERO)` still succeeds when chunks are queued, bypassing the deadline; `sysex_exchange` has no overall deadline.
6. **`monitor.rs` 2.0-second latency ratchet:** `crates/audio/src/monitor.rs:97-100, 179` — Switching to drop-newest with a 2s buffer locks monitoring at 2.0s latency on startup skew or clock drift.
7. **`unpack_loop` overflows `usize` on 32-bit targets:** `crates/audio/src/player.rs:72-82` — Use `.saturating_mul(2)`.
8. **`SampleQueue::len()` underflows from a 3rd observer thread + public `&self` SPSC API + cache-line false sharing:** `crates/audio/src/spsc.rs:11-72`.
9. **Multi-system slurs placed on wrong system + $O(S \cdot L \cdot N)$ cubic scan in `score/full.rs`:** `crates/score/src/full.rs:1168-1191` — `tick_near` ignores `y`/system so slurs on system 2+ match system 0 notes. Store `start_tick`/`end_tick` directly on `SlurArc`.
10. **Negative horizontal scale in `Columns::new` draws bars backwards on narrow pages:** `crates/score/src/full.rs:275-300` — Clamp `(right - start).max(1.0)`.
11. **Eighth/sixteenth notes beamed across barlines in odd-eighth meters (3/8, 5/8, 7/8, 9/8):** `crates/score/src/full.rs:1220-1238` — Check `previous.measure == group.measure`.
12. **PDF `/Title` metadata writes WinAnsi octals into a PDFDocEncoding string:** `crates/score/src/pdf.rs:820-825` — Use `pdf_utf16_hex_string(title)` for any non-ASCII title.
13. **`replace_project` does not clear `self.recovery = None` (and `ShellApp::new` computes `initial_key` before fallback):** `apps/desktop/src/app.rs:1020-1034, 3088-3097`.

### Remaining Medium Carryovers
14. **`plugin-host` & `plugins` remaining edge cases:** Missing VST3 `ModuleEntry`/`ModuleExit` & `InitDll`/`ExitDll`; unchecked `rect.right - rect.left` in `probe_view` (`instance.rs:461-462`) and no upper clamp in `HostPlugFrame::resizeView` (`editor.rs:237-240`); `negotiate_stereo` skips `getBusInfo` errors, leaves default-active aux buses enabled, and passes non-null dangling pointer when `inputs.is_empty()` (`instance.rs:533-572`); `read_capped` TOCTOU / `/dev/zero` FIFO bypass (`crates/plugins/src/lib.rs:323-328`).
15. **Audio RT error callbacks & caps:** `eprintln!` in `player.rs:456` and `Mutex::lock` + `format!` in `record.rs:184-188`; `soundfont.rs:605-607` doesn't constrain `loop_start`/`loop_end` to `start..=end` (plus `ibag`/`pbag` monotonicity & 32-bit `8 + size` overflow at line 313); `decode.rs` `MAX_DECODED_SAMPLES` is 4.15 GiB (~8 GiB peak).
16. **`performance::regenerate()` channel loss & non-atomic Replace:** `crates/performance/src/lib.rs:361-402`.
17. **`app-commands` & `project::validate` carryovers:** Non-exact undo for `SetLoops`, `ReplaceTrackNotesAndClips`, `SetMarkers` (`crates/app-commands/src/lib.rs:1366-1407`); 3× duplicate automation / 2× duplicate plugin validation loops in `crates/project/src/validate.rs:86-382`.
18. **`arranging` carryovers:** `groove::apply`/`feel_match` still mishandle `NaN` in `Amounts` or in-memory `Groove.levels` (`crates/arranging/src/groove.rs:113-154`); non-atomic multi-command ops & `DrumFill` negative ticks / `u32` overflow in `crates/arranging/src/grid.rs:302-340` & `crates/arranging/src/forms.rs:196-970`.
19. **`musicxml` carryovers & performance bug:** `escape()` omits XML 1.0 control-char stripping (`crates/musicxml/src/lib.rs:42-47`); `tie_flags`/`slur_flags` recomputed inside `for bar_index in 0..bars` (up to 10,000× redundant sorting + unchecked `note.start + duration` at line 263); `crates/musicxml/src/import.rs:416-420` can emit `Slur { start >= end }` and unsorted/duplicate multi-part chords/marks/lyrics.
20. **Desktop UI per-frame polling & unhandled disconnects:** `plugins_window` (`apps/desktop/src/app/mixer.rs:788-793`), `groove_window` (`apps/desktop/src/app/style.rs:753-756`), `l6max_device_window`/`d05_editor_window` MIDI port scan & `TryRecvError::Disconnected` (`apps/desktop/src/app/devices.rs:1347-1728`).
21. **CLI remaining edge cases:** `midi-map-show` still swallows all read errors via `unwrap_or_default()` (`apps/cli/src/lib.rs:2350`); public `run_transport` still lacks `block == 0` guard (`apps/cli/src/lib.rs:2831`).
22. **Quadratic $O(N^2)$ parsers (deferred M9):** `crates/smf/src/lib.rs:543-550`, `crates/style-formats/src/casm.rs:108-113`, `crates/style-formats/src/import.rs:112-178`, `crates/musicxml/src/import.rs:403`.

---

## Low Findings (Selected Highlights)

- **Score / PDF:** Unvalidated `zoom` div-by-zero/`NaN` (`apps/desktop/src/score_view/full.rs:52`); stale Cecilia textures retained on error (`apps/desktop/src/score_view.rs:189-193`); flat keys printed as sharps (`"A# major"` for Bb major, `crates/score/src/full.rs:796-800`); missing secondary beamlet for dotted-8th + 16th (`crates/score/src/full.rs:1321-1326`); CJK width underestimate & raw control chars (`crates/score/src/pdf.rs:794-804, 1059-1062`); unchecked `measure.start + measure.length` in `draw_system`.
- **Plugin-Host / Plugins / Test-Plugin:** Missing `iid.is_null()` in `Factory::createInstance` & unchecked `i32` in `GainPlugView` (`crates/test-plugin/src/lib.rs:646-675, 978`); `scan_vst3_isolated` waits for `recv_timeout` even after non-zero exit (`crates/plugins/src/lib.rs:378-385`); `Vst3Instrument::note_on_at` wraps `offset > i32::MAX` to `0`; `AuEffect`/`AuInstrument::apply_state` skip `is_finite()`; zero `// SAFETY:` comments across `crates/plugin-host`.
- **Audio / MIDI / Core:** `player.rs:173-175` separate `looping` vs `loop_bounds` store order; `to_wav32f` & `metronome::mix_accent` non-finite inputs; `wav::info` unchecked offsets; `d05.rs` `internal_patch_address` unchecked mul; `library` `Book::from_mfd` `0` tempo/meter; unchecked tick/ID math in `arranging/{loops,grid,forms,generate}.rs`.

---

## Round 2 Handoff Checklist

- [ ] **Batch 1 — Critical/High Plugin-Host & FFI:** R2-C1 (`factory` drop order before `_library`), M2 (guard `setProcessing(0)`/`setActive(0)` on partial init), R2-H1 (`AuEditor` `autoreleasepool` UAF), R2-H2 (`Vst3Editor::close` `autoreleasepool`), R2-H3 (`AuInstrument` chunked MIDI offsets), M14 (`probe_view` saturating_sub, `resizeView` upper clamp, aux bus deactivation, `read_capped` `File::open` + `.take`)
- [ ] **Batch 2 — Audio RT, Concurrency & Sequencer:** R2-H4 (`SampleQueue` power-of-two masking & cache-line padding), R2-H5 (`Playback::fill()` packed `playing` bit & `pause()` epoch), R2-H6 (`Playback::new`/`record.rs` sanitize & even-frame stereo alignment), R2-H7 (`Scheduler::publish()` active-note release), M3–M8, M15 (`mix_audio_reference`, `release_active` track/refcount, `sysex_handshake` deadline, `monitor.rs` latency cap, `unpack_loop`, error callbacks, `soundfont` loop bounds)
- [ ] **Batch 3 — New Score Engine (`full.rs` / `pdf.rs`):** R2-H10 (`slots` overflow & non-finite `Settings` hang), M9 (multi-system slurs), M10 (`Columns::new` narrow clamp), M11 (odd-meter cross-barline beaming), M12 (PDF `/Title` UTF-16), plus Low `score_view` stale textures & flat-key names
- [ ] **Batch 4 — Security, Schema, Parsers & Core:** R2-H8 (`ai.rs` `BrokenPipe` zombie/mask), R2-H9 (`checked_output_dir` + leaf symlinks), R2-H11 (`track.slots` validation), R2-H12 (`casm.rs` multi-group `CSEG`), M1 (`time.rs` `at_or_after` near `i64::MAX`), M13 (`replace_project` `recovery = None`), M17–M19, M21 (`validate.rs` dedup, `musicxml` `tie_flags` hoist & control chars, `groove` `NaN` `Amounts`, CLI `midi-map-show`/`run_transport`)
- [ ] **Batch 5 — Follow-Up Polish:** R2-H13 (remaining desktop UI-thread blocking), M16 (`performance::regenerate`), M20 (desktop per-frame polling/disconnects), M22 ($O(N^2)$ parsers), `ModuleEntry`/`InitDll`, `// SAFETY:` comments
