# Source Code Audit Report — Lazy Arranger v2

- **Audited commit:** `a3f786d` (`v0.1.0-beta.1`, branch `main`)
- **Scope:** Full workspace (~103,023 LOC Rust across 18 crates + `apps/desktop` + `apps/cli`, plus `scripts/` and `.github/workflows/`), audited against [`docs/architecture/contracts.md`](../architecture/contracts.md).
- **Method:** Static source-code inspection across all subsystems (`plugin-host`/`plugins`/`test-plugin`, `audio`/`sequencer`/`performance`/`midi-devices`, `project`/`app-commands`/`music-core`/`arranger`/`arranging`/`library`, `smf`/`style-formats`/`musicxml`/`score`, `mcp`, `apps/desktop`, `apps/cli`).
- **Verification notes:** `./scripts/rust.sh fmt --all --check` passed locally; full `clippy`/`test`/`doc` could not fetch crates offline in the audit VM, while GitHub Actions CI was green on `main` (`a3f786d`).

## Status (2026-11-26)

- **Fixed:** All five handoff batches (Critical, High and the key Medium findings) in PR #26. The Round 2 audit ([`source-code-audit-round2.md`](source-code-audit-round2.md)) checked these fixes; its gaps were fixed in PR #28, PR #29 and the round 2 follow-up PR.
- **Fixed later:** M17 (`validate.rs` duplicate loops, exact undo), the remaining UI-thread blocking from H25, and the Low items listed under "Status" in the Round 2 report.
- **Left as is:**
  - macOS VST3 `bundleEntry`/`bundleExit` is not called. It needs a `CFBundleRef`, and plug-ins load without it.
  - `// SAFETY:` notes are per module (a `# Safety` section at the top of each `plugin-host` file), not on every block.

## Summary

| Severity | Count | Primary Areas |
| --- | --- | --- |
| **Critical** | 11 | FFI UAF & heap buffer overflows (`plugin-host`), MCP sandbox/RCE/file-write bypasses, desktop project-overwrite data loss, RT audio callback mutex/allocs, deterministic DSP OOB panic, SF2 OOM bomb |
| **High** | 26 | VST3/AU lifecycle & COM leaks, subprocess OOM/hang & argument injection, schema/validation & undo contract violations, audio/MIDI races & stuck notes, parser overflows, UI-thread blocking & Cecilia deadlock |
| **Medium** | 25 | Windows atomic-save breakage, macOS dictation path quoting, CWD state files, music-theory/spelling bugs, CASM/PDF bugs, non-atomic multi-track edits, CLI edge cases, per-frame UI polling |
| **Low** | 10 | Edge-case integer overflows, minor error-mapping & documentation mismatches |

---

## Critical Findings

### C1. Use-After-Free & Off-Main-Thread AppKit UB in `HostPlugFrame::resizeView`
- **File:** `crates/plugin-host/src/editor.rs:206-233`, `296-310`, `342-351`
- **Details:**
  1. `HostPlugFrame.window` (`AtomicPtr<AnyObject>`) is never cleared to null before `window.close_and_release()` in `Vst3Editor::close()` or on `attached != kResultOk` in `Vst3Editor::open()`. Any VST3 plug-in that `AddRef`s `IPlugFrame*` and invokes `resizeView` during teardown or from a timer sends `setContentSize:` to a freed `NSWindow` (UAF).
  2. `resizeView` never calls `ensure_main_thread()` before invoking AppKit `setContentSize:` (off-main-thread AppKit UB/crash on macOS).
  3. `rect.right - rect.left` and `rect.bottom - rect.top` use unchecked `i32` subtraction (panic across `extern "system"` FFI).
- **Fix:** Share an `Arc<AtomicPtr<AnyObject>>` between `Vst3Editor` and `HostPlugFrame`, atomically `store(ptr::null_mut(), Ordering::Release)` before `close_and_release()` (and on error in `open()`), load with `Ordering::Acquire` in `resizeView`, enforce `ensure_main_thread()`, and use `saturating_sub`.

### C2. Heap Buffer Overflows via `maxSamplesPerBlock` / `MaximumFramesPerSlice` Violations
- **Files:** `crates/plugin-host/src/vst3_instrument.rs:159-165, 213-284`; `crates/plugin-host/src/instance.rs:171-196, 250-285`; `crates/plugin-host/src/au.rs:596-609, 677-718`
- **Details:**
  - `Vst3Instrument::new_inner` passes `maxSamplesPerBlock` to `setupProcessing`, then discards `max_block`. `process_block` passes `numSamples: frames as i32` unchunked and computes `frames as i32 - 1`.
  - `Vst3Effect::new_inner` passes `max_block.min(i32::MAX as usize)` (allowing `0`) to `setupProcessing`, while storing unclamped `max_block.max(1)` (sending 1 sample to a 0-block plug-in; `self.max_block * 2` overflows if `max_block > usize::MAX / 2`).
  - `AuInstrument::configure` sets `kAudioUnitProperty_MaximumFramesPerSlice = 4096`, yet `AuInstrument::process_block` never chunks into 4096-frame slices (unlike `AuEffect::process_stereo`), and `u32::try_from(frames * 4).unwrap_or(0)` overflows or sets `data_byte_size = 0` while passing large `inNumberFrames`.
- **Fix:** Clamp `max_block` once to `1..=(i32::MAX as usize).min(usize::MAX / 2)` before `setupProcessing`, store that exact value on both `Vst3Effect` and `Vst3Instrument`, and chunk `Vst3Instrument::process_block` and `AuInstrument::process_block` (rebasing event `sampleOffset` per chunk).

### C3. Heap Buffer Overflow & Unbounded Tail Query in `unit_parameters` (`AudioUnitGetProperty`)
- **File:** `crates/plugin-host/src/au.rs:184-215`
- **Details:** `count = size as usize / 4` allocates `count * 4` bytes, yet the original unaligned `&mut size` is passed to the second `AudioUnitGetProperty` call. If `size % 4 != 0`, CoreAudio can write up to 3 bytes past `ids`. If `size` shrinks between calls, `for id in ids` still iterates trailing unwritten `0` IDs.
- **Fix:** Pass `let mut byte_size = u32::try_from(count * 4).unwrap_or(0);` to the second call (returning early if `0`) and call `ids.truncate((byte_size as usize / 4).min(count))` afterward.

### C4. `AuEditor` & `try_custom_cocoa_au_view` Use-After-Free, Leaks, and `CFRelease` on Garbage Pointers
- **File:** `crates/plugin-host/src/editor.rs:389-394, 406-483, 526-576`
- **Details:**
  - In `AuEditor::open`, if `NativeWindow::open(...)` fails (line 536), `+1`-retained `au_view` is leaked while `handle` disposes the `AudioUnit` (UAF).
  - In `AuEditor::close`, `au_view` is never detached (`removeFromSuperview` / `setContentView: nil`) before `handle` drops; an autoreleased `NSWindow` keeps `au_view` alive past `AudioUnit` disposal.
  - In `try_custom_cocoa_au_view`, `buffer` is sized with `div_ceil` and `size` is never re-checked after the second `AudioUnitGetProperty`, so `for &ptr in &buffer { CFRelease(ptr); }` can `CFRelease` partial/garbage pointer words. Early `?` on `AnyClass::get(c"NSBundle")?` (line 440) bypasses `CFRelease` (leak). Sending `uiViewForAudioUnit:withSize:` without `respondsToSelector:` risks an uncaught ObjC exception across Rust FFI (UB/abort).
- **Fix:** Wrap `au_view` and CF buffers in RAII drop guards; detach `au_view` inside `objc2::rc::autoreleasepool` before releasing `window`/`handle`; validate `size >= min_size && size % size_of::<*const c_void>() == 0`; check `respondsToSelector:`.

### C5. Unconditional COM Reference Count Leak in `ParameterChanges::getParameterData`
- **File:** `crates/plugin-host/src/instance.rs:230-306, 735-748`
- **Details:** Per VST3 ABI (`IParameterChanges::getParameterData`), the returned `IParamValueQueue*` is borrowed; plug-ins must not call `Release()`. Calling `IParamValueQueue::add_ref(pointer)` permanently leaks `ComWrapper<ParameterQueue>` on every call. Compounding this, `process_stereo` passes the same `changes` pointer to every chunk in `for block in samples.chunks_mut(...)`, multiplying the leak and re-delivering offset-0 changes on every chunk.
- **Fix:** Remove `add_ref` in `getParameterData` (update unit test at `instance.rs:871` to `ComRef::from_raw`), clear `changes = None` after block 0 in `process_stereo`, and don't drain `pending_parameters` when `samples.is_empty()`.

### C6. MCP `set_plugins` & `plugin_scan` Bypass Plug-In Root Allowlist (Arbitrary `dlopen`)
- **Files:** `crates/mcp/src/lib.rs:2311-2315` (`add_plugin`), `2901-2918` (`plugin_scan`), `3072-3128` (`set_plugins`)
- **Contract violated:** `docs/architecture/contracts.md:68-69` (*"Tool arguments are untrusted. File paths and plug-in roots are checked"*).
- **Details:**
  - `add_plugin` checks `plugin_path_allowed(&slot_path, format)`, but **`set_plugins` skips `plugin_path_allowed` completely**, allowing any caller to attach an arbitrary path (e.g. `/tmp/evil.vst3`) that is later `dlopen`ed during rendering.
  - `plugin_scan` passes caller-supplied `"directories"` straight to `plugins::scan_roots(&roots)`, triggering `dlopen` in the scanner helper on any `.vst3` dir without `moduleinfo.json`.
- **Fix:** Enforce `plugin_path_allowed` for every non-empty slot `path` in `set_plugins`, and restrict `plugin_scan` directories to canonical descendants of `plugins::default_roots()` (or an explicit allowlist).

### C7. MCP `OUTPUT_ROOT` Arbitrary File-Write Bypasses & Symlink Escape in `checked_output`
- **Files:** `crates/mcp/src/lib.rs:1272-1287` (`checked_output`), `2732-2745` (`reverse_audio`), `3531` (`groove_from_audio`)
- **Details:**
  - `reverse_audio` writes `{stem}-reversed.wav` beside `reference.path` without calling `checked_output` (chained with `set_audio_ref`, writes anywhere).
  - `groove_from_audio` passes untrusted `dir` directly to `arranging::groove_file::save(Path::new(dir), ...)` without `checked_output` (unlike `save_groove` at line 3976).
  - Inside `checked_output`: (a) `canonicalize(parent)` does not check whether the leaf file is a **symlink** pointing outside `OUTPUT_ROOT`; (b) a bare filename `"foo.wav"` has `parent() == Some("")`, causing `canonicalize("")` to fail; (c) paths ending in `..` have `file_name() == None`, resolving to `parent`.
  - `apps/desktop` never calls `mcp::set_output_root`, leaving `OUTPUT_ROOT` unset for desktop assistant proposals.
- **Fix:** Route `reverse_audio` and `groove_from_audio` through `checked_output`; in `checked_output`, normalize empty parent to `"."`, reject `..` components, require a valid `file_name()`, reject symlink targets via `symlink_metadata`, and configure a default `OUTPUT_ROOT` in `apps/desktop`.

### C8. Desktop Startup Validation Failure Silently Falls Back to Demo While Keeping `project_path` (Data Loss)
- **File:** `apps/desktop/src/app.rs:1025-1028, 1399, 1406`
- **Contract violated:** `docs/architecture/contracts.md:80-81` (*"A failed open shows the error and clears the saved path; it never falls back to the demo while keeping the old path."*).
- **Details:** If `Editor::new(project)` fails in `ShellApp::new`, it silently loads `demo_project()`, discards the error (`load_error: None`), and retains `project_path = Some(path)`. Saving or autosaving overwrites the user's file on disk with the demo project.
- **Fix:** On `Err(err)`, set `project_path = None`, disable `recovery`, and populate `load_error = Some(err.to_string())`.

### C9. `load_kids_demo()` Replaces Project with `dirty = true` While Keeping `project_path` & Active Playback (Data Loss)
- **Files:** `apps/desktop/src/app/layout.rs:5095-5103`; `apps/desktop/src/editor.rs:1657-1664`
- **Details:** Unlike `replace_project`, `load_kids_demo()` replaces `self.session` and marks it dirty without clearing `self.project_path`, stopping `self.player`, or clearing `render_pending`/`live_remix`/selections/score caches. Save or autosave overwrites the user's open project with the Kids demo.
- **Fix:** Replace with `self.replace_project(ProjectFile::new(arranging::demo::kids_song()), "Kids demo")` and remove `Editor::load_kids_demo`.

### C10. Blocking `Mutex::lock()` & Heap Reallocations Inside cpal RT Callbacks (`monitor.rs`)
- **File:** `crates/audio/src/monitor.rs:95, 157-184, 198-218`
- **Contract violated:** `docs/architecture/contracts.md:30-31`.
- **Details:** Both input and output cpal callbacks acquire a blocking `Mutex<VecDeque<f32>>` (initialized with 0 capacity), reallocate on the RT thread, and hold the lock across the entire output buffer loop. Also `sample * gain` (line 208) is neither finite-checked nor clamped to `[-1.0, 1.0]` before `T::from_sample`.
- **Fix:** Replace with a pre-allocated lock-free SPSC ring buffer and sanitize/clamp `sample * gain` to `[-1.0, 1.0]`.

### C11. Deterministic Out-of-Bounds Panic in `stretch_interleaved` & Quadratic OOM Bomb in `soundfont::parse`
- **Files:** `crates/audio/src/lib.rs:198-263, 296`; `crates/audio/src/soundfont.rs:280-301, 376-448`
- **Details:**
  - `stretch_interleaved`: `overlap = if has_previous { HOP.min(length) } else { 0 };` (line 243) is not clamped to `previous.len() / 2`. When a short tail shrinks `previous` below `HOP` (512) and the next backward correlation search finds a larger `length`, `previous[index * 2]` (line 247) panics out of bounds. Also `weight = index as f32 / HOP as f32` clicks when `overlap < HOP`, `target_frames * 2` is unchecked, and `mix_audio_reference` skips resampling before stretching when sample rates differ.
  - `soundfont::parse`: `zones()` does not validate monotonic non-decreasing `wInstBagNdx` / `wPresetBagNdx`. A crafted ~650 KB SF2 alternating `0` and `65535` allocates >15 GB of `InstrumentZone` structs (OOM crash).
- **Fix:** Clamp `overlap` to `previous.len() / 2`, divide `weight` by `overlap.max(1) as f32`, use `checked_mul`/cap for `target_frames`, resample before stretching, and enforce monotonic non-decreasing bag indices in `soundfont::parse`.

---

## High Findings

### Security, Subprocesses & Network

#### H1. Unbounded Line-Read OOM DoS in MCP Socket Server (`socket.rs`) & Fragile CLI Loop (`apps/cli`)
- **Files:** `crates/mcp/src/socket.rs:86-100`; `apps/cli/src/lib.rs:3137, 3187-3214`
- **Details:**
  - `socket.rs:87`: `reader.read_until(b'\n', &mut bytes)` allocates unbounded memory *before* checking `bytes.len() > MAX_LINE_BYTES` on line 91.
  - `apps/cli/src/lib.rs:3187-3214`: `mcp_socket` bypasses `serve_stream` entirely — no `MAX_LINE_BYTES`, no read timeout, non-atomic `fs::write` on every tool call (also line 3137), and any client disconnect or bad UTF-8 byte propagates `?` and crashes the entire server.
- **Fix:** Use `reader.by_ref().take((MAX_LINE_BYTES + 1) as u64).read_until(...)`, handle per-connection errors without aborting the accept loop, set a read timeout, and write `project_path` atomically.

#### H2. Argument Injection, `curl` SSRF/File Read, Header Injection & Zombie Processes in `ai.rs` and `speech.rs`
- **Files:** `apps/desktop/src/ai.rs:123-130`; `apps/desktop/src/speech.rs:224, 343-348, 371-384`
- **Details:**
  - `ai.rs:129`: `config.url.trim()` is passed to `curl` without scheme validation (`http://`/`https://`) or `--`, allowing `-` flag injection or `file://` reads. Line 123 writes `Authorization: Bearer {key}` without stripping `\r`/`\n` (header injection). `-d &body.to_string()` passes full chat history via `argv` (`ps` exposure + `E2BIG` risk on large tool histories).
  - `speech.rs:224, 371-383`: `clean(text)` is passed to `say`, `espeak-ng`, and `sherpa-onnx-offline-tts` without `--`; assistant replies starting with `-` are interpreted as CLI flags (e.g. `espeak-ng -f /etc/passwd`). `.spawn().map(|_| ())` drops `Child` without reaping -> zombie processes on Unix.
- **Fix:** Require `http://`/`https://` URLs, pass `--` and `--data-binary @-` via stdin, and strip `\r`/`\n` from `api_key` in `ai.rs`; pass `--` before text (or feed via stdin) and reap spawned children on a worker thread in `speech.rs`.

#### H3. Arbitrary Scanner Binary via `ARRANGER_PLUGIN_SCANNER`, Host OOM, and Grandchild FD Hang in `crates/plugins`
- **File:** `crates/plugins/src/lib.rs:173-196, 313-375`
- **Details:**
  - `ARRANGER_PLUGIN_SCANNER` unconditionally executes any file path; `for base in [dir, dir.parent()?]` short-circuits if `dir.parent()` is `None` and searches the parent directory for generic `"plugin-scan"`.
  - `scan_vst3_isolated`: `stdout.read_to_end(&mut bytes)` has no byte cap (malicious/noisy plug-in OOMs host); if a plug-in spawns a grandchild inheriting stdout, `reader.join()` hangs forever past the 5s timeout; plug-in `printf` corrupts JSON on fd 1.
- **Fix:** Remove or gate `ARRANGER_PLUGIN_SCANNER`, drop generic `"plugin-scan"` parent fallback, cap reads with `stdout.take(1024 * 1024)`, don't block forever on `reader.join()` after timeout (or use `setpgid`/`killpg`), and redirect fd 1 inside the scanner helper.

#### H4. Path Traversal & Integer Overflow Panic in Legacy `.arranger` Importer
- **File:** `crates/project/src/legacy.rs:541-545, 560-575`
- **Details:** `resolve_audio_paths` joins relative paths containing `..` (`../../../etc/passwd`) without checking `Component::ParentDir` or verifying containment in `dir`, and leaves absolute paths untouched. `read_audio_clips` computes unchecked `reference.start + reference.length` (panics in debug on `i64` overflow).
- **Fix:** Reject `ParentDir`/`RootDir`/`Prefix` (or verify canonical containment) and use `checked_add`.

### Memory Safety & FFI (`crates/plugin-host` & `crates/test-plugin`)

#### H5. VST3 Lifecycle & COM Ownership Bugs (Double `terminate()`, Missing `disconnect`, Error-Path Leaks/UAF)
- **Files:** `crates/plugin-host/src/instance.rs:37-49, 102-188, 480-491`; `crates/plugin-host/src/vst3_instrument.rs:128-176`
- **Details:** Single-component VST3 plug-ins get double `terminate()` in `Vst3Effect::drop`; `setComponentHandler(null)` and `IConnectionPoint::disconnect` are never called (ref-cycle leak past `dlclose` and teardown UAF); `Vst3Effect` drops `component`/`processor` before `controller`; early returns in `new_inner` drop `handler` before `controller` and skip `setActive(0)`/`terminate()` (violating `contracts.md:49-50`).
- **Fix:** Track `separate_controller: bool` and connection points, disconnect and clear component handler before `terminate()`, fix field drop order (`controller` before `component`/`processor` before `_handler`/`_host`/`_library`), and use an RAII unwind guard in `new_inner`.

#### H6. Null-Pointer Deref & Uninitialized `&mut` Reference UB in `HostApplication::getName`
- **File:** `crates/plugin-host/src/instance.rs:786-796`
- **Fix:** Check `name.is_null()` and write via raw `*mut u16` pointer instead of `&mut *name`.

#### H7. `MemoryStream` (`IBStreamTrait`) Rejects Valid Null Count Pointers & OOMs/Panics Across FFI
- **File:** `crates/plugin-host/src/instance.rs:542-636`
- **Details:** Rejecting null `bytes_read`/`bytes_written` breaks real VST3 plug-ins; `seek(i64::MAX)` + 1-byte `write` calls `bytes.resize(8 EiB)` (OOM) or panics in `copy_from_slice` when `saturating_add` saturates; negative `num_bytes` returns `kResultOk`.
- **Fix:** Allow null count pointers, reject `num_bytes < 0`, cap stream size (e.g. 64 MiB), and use `checked_add`.

#### H8. Incorrect `CFRelease` Gating on `AudioUnitParameterInfo::unit_name` / `cf_name`
- **File:** `crates/plugin-host/src/au.rs:245-269`
- **Fix:** Release `cf_name` iff both `HasCFNameString (1 << 27)` and `CFNameRelease (1 << 4)` are set; release `unit_name` iff `info.unit == CustomUnit (26) && !info.unit_name.is_null()`.

#### H9. Unnegotiated Stereo Assumption & Inactive Event Bus in VST3 Hosts
- **Files:** `crates/plugin-host/src/instance.rs:143-170, 258-275`; `crates/plugin-host/src/vst3_instrument.rs:144-158, 256-263`
- **Details:** Neither host negotiates stereo via `setBusArrangements`/`getBusInfo` (stack OOB on >2ch plug-ins), and `Vst3Instrument` never activates event input bus 0.
- **Fix:** Negotiate `SpeakerArr::kStereo` (or adapt channel pointers) and activate event input bus 0 when `getBusCount(kEvent, kInput) > 0`.

#### H10. 13+ Unchecked Null-Pointer Dereferences in `crates/test-plugin`
- **File:** `crates/test-plugin/src/lib.rs:46, 104, 135, 150, 227, 243, 251, 447, 470, 490, 495, 719, 782, 797, 890, 907, 916, 925, 942, 963`
- **Fix:** Guard every raw pointer parameter with `.is_null()` returning `kInvalidArgument`.

### Audio, Concurrency & MIDI

#### H11. Blocking `Mutex::lock()` + Unbounded `Vec` Growth in RT Record Callback & `Duration::from_secs_f64` Panic
- **File:** `crates/audio/src/record.rs:84-87, 109-110, 121, 160-169`
- **Fix:** Use a lock-free SPSC ring buffer + atomic error flag; cap `seconds` (e.g. `<= 86_400.0`) before `Duration::from_secs_f64`.

#### H12. Torn Loop Reads, Double-Load Race & ABA Race in `Playback::fill()`
- **File:** `crates/audio/src/player.rs:70-81, 106-108, 141-223`
- **Details:** `fill()` loads `self.looping` twice (lines 165 and 143); `set_loop` updates `loop_start`/`loop_end` as separate `Relaxed` atomics (torn bounds); `stop()`/`seek_frame(0)` suffers ABA against `position.compare_exchange`.
- **Fix:** Pack loop state atomically and read once per `fill()`; include a monotonic epoch counter with `position` in a single `AtomicU64`.

#### H13. SoundFont Index Shifting, Biquad Instability, `Inf % loop = NaN`, and OOB Interpolation
- **File:** `crates/audio/src/soundfont.rs:350-448, 484-506, 596-696`
- **Details:** Dropping records with `name.is_empty()` shifts positional `shdr`/`inst` indices while retaining `"EOS"`/`"EOI"`/`"EOP"` sentinels; `Lowpass::new` allows `cutoff >= nyquist` when `rate <= 42` Hz (unstable poles / div-by-zero); large `coarse_tune` overflows `step` to `Inf` -> `(Inf - loop_end) % loop_length = NaN`; `seconds = NaN` bypasses `<= 0.0`; non-looped `next_index = end` reads adjacent sample; `sample.correction` is ignored.
- **Fix:** Drop only final sentinel records; clamp `cutoff < nyquist * 0.95` and `semitones`; guard `!seconds.is_finite()` and cap frames; clamp `next_index` to `end - 1`; apply `sample.correction`.

#### H14. `NaN`/`Inf` Propagation, Unbounded Symphonia Decompression & `metronome::clicks()` Infinite Loop
- **Files:** `crates/audio/src/wav.rs:76-147`; `crates/audio/src/decode.rs:74-127`; `crates/audio/src/lib.rs:306-315, 387-390, 456, 493, 531, 607-609, 942-956`; `crates/audio/src/mix.rs:83, 105-156`; `crates/audio/src/metronome.rs:20-36`; `crates/audio/src/master.rs:41-198`; `crates/audio/src/edit.rs:64-79`
- **Details:**
  - `f32::NAN.clamp(-1.0, 1.0)` returns `NaN` across WAV/Symphonia decoders, mixer, master, and edit (`Inf * 0.0 = NaN` in `normalize_peak`); `ReverbChannel` IIR feedback permanently locks to `NaN`.
  - `decode_internal` has no decoded-sample cap (compressed silence OOM bomb).
  - `metronome::clicks()` applies `.max(1)` *before* `div_euclid`, yielding `beat == 0` and an infinite loop until OOM when `bar_ticks < numerator`.
  - `render_with_soundfont` / `mix_one` divide by zero at `sample_rate == 0`; `gains()` has no `0.0` lower clamp; `fade_out > length` underflows signed `Tick`; `comp.length <= 0` still renders `cross` ticks; `mix_one` ignores `gain_mb` and measures `fade_out` from `start` instead of end; `chorus`/`reverb` allocate unbounded delay lines for huge `sample_rate`.
- **Fix:** Add `sanitize(f32)` (`if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 }`), cap Symphonia decoded samples and delay-line rates, move `.max(1)` after `div_euclid` in `metronome::clicks()`, guard `sample_rate == 0`, clamp `volume >= 0.0` and `fade_out <= length`, and skip `comp.length <= 0`.

#### H15. Stuck MIDI Notes, Unchecked Overflows, Bad XG Bulk Checksum & D-05 Protocol Bugs
- **Files:** `crates/sequencer/src/lib.rs:37-39, 98, 117, 271-355`; `crates/midi-devices/src/dispatch.rs:130-207`; `crates/midi-devices/src/devices.rs:275-277, 484-503, 530, 607`; `crates/midi-devices/src/sysex.rs:3-52`; `crates/midi-devices/src/d05.rs:94, 110, 219-273, 457, 555, 596-625, 743-778`
- **Details:**
  - `sequencer::Scheduler`: no active-note tracking or NoteOff/CC 123 on `stop()`/`seek()`/`publish()`; drops `NoteOff` on `EventBuffer` overflow; O(N) scan stall after stop; unchecked `note.start + note.duration` and `clamp(0, negative)` panic when `frames > i64::MAX`.
  - `midi_devices::dispatch`: `play_timed` checks sort order *after* sending earlier messages; `play_with` skips `all_notes_off_messages` on error (violating `contracts.md:39-40`).
  - `devices.rs`: `Duration::from_secs_f64` panics on `NaN`/`Inf` in `channel_check`, `monitor_raw`, and `echo`.
  - `sysex.rs`: Yamaha XG bulk-dump checksum omits `[bh, bl]` byte count (wrong per XG spec); `ParameterChange::to_bytes()` allows empty `data`.
  - `d05.rs` / `devices.rs`: `upper_common`/`lower_common` lack `offset < 64` bounds and use unchecked arithmetic; `parse_data_packet` doesn't validate 7-bit bytes or `device` ID; `sysex_handshake`/`handshake_read` return `Ok(vec![])` instead of `Err` on `RJC`/`ERR`/timeout and lack a total deadline.
- **Fix:** Track active notes in `[u128; 16]` in `Scheduler`, pre-validate sort order and always send CC 123 in `dispatch`, validate finite durations, split XG bulk checksum (`[bh, bl] + addr + data`) from Roland checksum, enforce D-05 bounds/7-bit/device-ID checks, and return `Err` on abort/timeout.

### Project Schema, Commands, Music Core, Arranger, Arranging & Performance

#### H16. `ProjectFile::from_json` Deserializes `Song` Before Checking `format` and `version`
- **File:** `crates/project/src/song.rs:983-993`
- **Details:** Violates `contracts.md:8-10`. Any v21+ project with a new enum variant or changed field type fails with `ProjectError::Json` instead of `ProjectError::NewerSchema`.
- **Fix:** Parse header (`format`, `version`) first via `serde_json::Value` or a lightweight header struct, validate `format` and `1..=SCHEMA_VERSION`, then deserialize `Self`.

#### H17. Validation Gaps in `project::validate` (`Patch`, `slots`, `Slur.start`, `PluginParam.slot`, Tick Overflow)
- **File:** `crates/project/src/validate.rs:73-360`; `crates/project/src/song.rs:287-360`
- **Details:** `Track.patch` (`NaN`/`Inf` poison audio & emit invalid JSON `null`), `Track.slots`, `Slur.start < 0`, `AutomationTarget::PluginParam.slot` existence, and `start.checked_add(length)` on `Clip`/`AudioRef`/`AudioComp` are all unvalidated.
- **Fix:** Add `Severity::Error` checks for all five in `validate::validate`, and clamp `Patch::envelope` to `0.0..=1.0`.

#### H18. Duplicate JSON Keys on Legacy `.arranger` Round-Trip
- **File:** `crates/project/src/legacy.rs:174-188, 367-376`
- **Details:** Keys matching typed `Song`/`Track` fields (notably `"key"`, `"marks"`, `"lyrics"`, `"muted"`, etc.) are inserted into `song.extra`/`track.extra`, causing duplicate JSON keys and reload failure once the typed field is set.
- **Fix:** Namespace any colliding key with `"legacy_"` (and import `"key"` / add `"scenes"` to `consumed`).

#### H19. `EditSession` Contract Violations (No-Op Undo Entries, Inexact `RemoveNote` Undo, `u32::MAX` ID Collision)
- **Files:** `crates/app-commands/src/lib.rs:529-547, 608-618, 772-782, 877-892, 1168-1176, 1353-1401`; `crates/app-commands/src/clipboard.rs:128-143`
- **Details:**
  - `apply()` appends no-op undo entries and wipes the redo stack whenever `self.project == before` (violating `contracts.md:14-15`).
  - `RemoveNote` -> `InsertNote` reorders notes sharing the same `start` tick on `undo()`; `SetLoops` strips trailing `None`s; `ReplaceTrackNotesAndClips`/`SetMarkers` re-sort.
  - `next_note_id` / `next_track_id` / `clipboard::paste_notes` use `saturating_add(1)`, returning duplicate `u32::MAX` IDs instead of an error.
- **Fix:** Skip `push_undo` / `redo.clear()` when `self.project == before`; preserve exact index/order on undo; use `checked_add(1)`.

#### H20. Panics & Ordering Bugs in `music-core` and `arranger`
- **Files:** `crates/music-core/src/time.rs:18-121`; `crates/music-core/src/pitch.rs:184-193`; `crates/music-core/src/tempo.rs:34-38, 165-222`; `crates/arranger/src/transition.rs:114-127`
- **Details:**
  - `TimeSignature` has public unvalidated fields; `denominator == 0` / `numerator == 0` panics with div-by-zero; boundary helpers overflow `i64`.
  - `Pitch::from_name` overflows `(octave + 1) * 12` (panics in debug, wraps `"C357913941"` to valid MIDI pitch `8` in release).
  - `TempoMap::tick_at_seconds` overflows `i64` for large `seconds`; `TempoMap::constant(0.0)` divides by zero.
  - `TransitionState::poll` retains past-due requests when an earlier-sequenced, later-due request wins (`retain(|r| r.sequence > winner.sequence)`), firing stale transitions backwards in time.
- **Fix:** Validate `TimeSignature`/`TempoMap`, use checked/saturating arithmetic in `time.rs`, `pitch.rs`, and `tempo.rs`, and change `TransitionState::poll` to `retain(|r| r.due > now && r.sequence > winner.sequence)`.

#### H21. Corruption & Non-Atomicity in `arranging` and `performance`
- **Files:** `crates/arranging/src/groove_file.rs:65-74`; `crates/arranging/src/groove.rs:113-154`; `crates/arranging/src/loops.rs:35-99, 298-316`; `crates/arranging/src/grid.rs:302-340, 506-518`; `crates/arranging/src/forms.rs:196-251, 820-1004`; `crates/performance/src/lib.rs:149-188, 261, 360-414`
- **Details:**
  - `groove_file::parse` accepts `NaN`/`Inf` levels -> `groove::apply` produces MIDI velocity `0` (NoteOff); `feel_match` divides by 0 on empty `levels` and overflows `i32`.
  - `loops::fill_from_timeline` checks `scene_length == 0` instead of `<= 0`, creating 128 corrupted negative-length `SlotClip`s on negative input.
  - `grid::record_to_timeline`, `grid::copy_slot`, `forms::apply`, `forms::join_loop`, and `performance::regenerate` apply multiple separate commands non-atomically (leaving partial mutations on error).
  - `performance::regenerate` resets empty edited drum/bass tracks to Channel 1 (`unwrap_or_else(Channel::first)`) and `Policy::Duplicate` drops mix/instrument settings.
- **Fix:** Reject non-finite groove levels, guard `scene_length <= 0`, use atomic/compound commands, store channel in `Provenance`, and preserve track metadata on `Duplicate`.

### Parsers & Score Exporters

#### H22. MusicXML Import Overflow Panics & Negative-Tick Corruption
- **File:** `crates/musicxml/src/import.rs:161-166, 274-360, 405-423`
- **Details:** Subnormal `<divisions>1e-308</divisions>`, huge `<duration>`/`<offset>`, or extreme `<octave>`/`<alter>` trigger `i64`/`i32` overflow panics; negative `<offset>` produces negative ticks.
- **Fix:** Validate `divisions.is_finite() && (1e-3..=1e6).contains(&divisions)`, clamp durations/offsets/octaves/alters, and use `saturating_add`.

#### H23. Unbounded Memory Allocation in TTC/TTF Subsetter & Score/MusicXML Exporters
- **Files:** `crates/score/src/unicode_font.rs:296-309, 365-380, 451-518`; `crates/score/src/lib.rs:803-845`; `crates/score/src/cecilia.rs:115-122, 154`; `crates/musicxml/src/lib.rs:52-55, 111`
- **Details:**
  - Crafted `.ttc`/`.ttf` with overlapping table/glyph ranges amplifies 1 MiB into ~64 GiB in `extract_standalone_ttf` / `subset_ttf_preserving_gids`; `be_u16`/`be_i16`/`be_u32` use unchecked `off + 2` / `off + 4`.
  - A single note at a huge tick overflows `end + bar - 1` or OOMs allocating billions of empty measures in `score::engrave_with`, `cecilia::build_score`, and `musicxml::export`.
- **Fix:** Cap copied bytes and `num_tables` in `unicode_font.rs` (plus `checked_add`), and use `saturating_add` + a hard measure-count cap in score/Cecilia/MusicXML exporters.

### Desktop UI Threading & Lifecycle

#### H24. Permanent Cecilia Preview Deadlock on Concurrent Edit or Worker Panic
- **File:** `apps/desktop/src/score_view.rs:157-201`
- **Details:** When the song is edited while Cecilia is rendering key `K1` so current key is `K2 != K1`, `self.cecilia_pending` is never polled or cleared, permanently freezing the preview and repainting every 100 ms forever. `.try_recv().ok()` also conflates `Disconnected` (worker panic) with `Empty`.
- **Fix:** Always poll `try_recv()` on `Some((pending_key, rx))`; clear `cecilia_pending` on `Ok` or `Disconnected`, apply only if `pending_key == key`, otherwise spawn a new worker.

#### H25. Widespread Synchronous UI-Thread Blocking (Contract Violation) & Per-Frame Serialization
- **Contract violated:** `docs/architecture/contracts.md:76-77` (*"The egui update loop never blocks on network, subprocess or full-song render"*).
- **Locations:**
  - `apps/desktop/src/app/mixer.rs:1310-1320`: Runs `audio::render_stems` on the UI thread, and when all tracks are silent (`stem_levels.is_empty()` stays `true`), **re-renders all stems at 60 FPS**.
  - `apps/desktop/src/app.rs:4616-4627`: `run_autosave` calls `self.editor.project().to_json_pretty()` on **every UI frame** while dirty, before `Autosave::maybe_save` checks the interval.
  - Synchronous full renders / subprocesses on UI thread: Freeze track (`app/mixer.rs:2025-2033` -> `editor.rs:2248`, also overwrites global `data_dir/freeze-{id}.wav` across projects); `ExportWav`/`ExportStems`/`ImportAudio` (`ffmpeg`) (`app.rs:2356-2421`); `lp`/`lpstat` (`app.rs:645, 3185-3325`); Score PDF export (`app/layout.rs:4681-4698`); audio transcription (`app.rs:3327-3430`); audio-to-style & groove/style audition (`app/style.rs:214-730`, `app.rs:2019-2028`); proposal audition (`app/assistant.rs:366`).
- **Fix:** Check autosave elapsed time before serializing; track stem render generation with a flag instead of `is_empty()`; move renders, audio analysis, PDF generation, and subprocesses (`lp`, `lpstat`, `ffmpeg`) to background worker threads; scope freeze WAVs by project/timestamp.

#### H26. Stale Player & UI State on Autosave Restore and Incomplete Cleanup in `ShellApp::open`
- **File:** `apps/desktop/src/app.rs:4641-4658, 4840-4896`
- **Contract violated:** `docs/architecture/contracts.md:78-79`.
- **Details:** Clicking Restore in `recovery_dialog` replaces `self.editor` without stopping `self.player` or resetting caches/selections. `open()` destroys `self.player` *before* validating `Editor::new(project)`, reads/parses the file twice, and fails to stop/clear `live_remix`, `launcher`, `stem_levels`, `score.*`, and stale `recovery`.
- **Fix:** Validate `Editor::new(project)` first, then route both `open()` and `recovery_dialog` Restore through one helper that calls `stop_playback()` and resets all per-project UI state.

---

## Medium Findings

1. **Windows atomic-save breakage & 60 FPS autosave retry loop:** `apps/desktop/src/persistence.rs:23-27, 64-80` (`fs::rename` over existing file fails on Windows; `with_extension("tmp")` risks collision/leak; `self.last` isn't updated on error -> 60 FPS retry). Also `apps/desktop/src/settings.rs:820-836` omits `create_dir_all(parent)` before writing the settings temp file.
2. **Unquoted macOS path breaks dictation presets:** `apps/desktop/src/dictate.rs:36-47` interpolates `models_dir.display()` unquoted into `sh -c` templates, breaking on macOS (`~/Library/Application Support/Lazy Arranger/models`).
3. **State files written to CWD instead of `Settings::data_dir()` (violating `contracts.md:71-72`):** `lazy-arranger-midi-map.txt` (`app.rs:1163`, `app/devices.rs`, `app/map.rs:75`), `plugins::CATALOG_FILE`/`BLOCKLIST_FILE` (`app.rs:2449`, `app/mixer.rs:765-831`), and `"grooves"` (`app/style.rs:755`).
4. **C minor tonic mapped to A in score spelling:** `crates/score/src/spelling.rs:54-56` maps `pc = 0, minor = true` to `(5, 0)` (A) instead of `(0, 0)` (C), giving 0 flats instead of 3 flats.
5. **Malformed WinAnsi PDF encoding:** `crates/score/src/pdf.rs:733, 1524-1525` embeds raw UTF-8 `·` (renders as `Â·`) and maps smart quotes to wrong WinAnsi octals.
6. **Cecilia `nearest_duration` ignores `limit` for exact durations:** `crates/score/src/cecilia.rs:47-64`, overflowing bar lengths.
7. **CASM writer flattens multi-group `SectionRules` into one `CSEG`:** `crates/style-formats/src/casm.rs:719-729, 862-871`, corrupting multi-group styles on round-trip.
8. **`style-formats/audio_style` div-by-zero & infinite loop:** `crates/style-formats/src/audio_style.rs:48-57, 140-153, 220-224` on `sample_rate == 0` or `bpm = Inf`.
9. **Quadratic $O(N^2)$ parser CPU DoS:** `crates/smf/src/lib.rs:543-550` (overlapping notes), `crates/style-formats/src/casm.rs:108-113` (`Sdec` dedup), `crates/style-formats/src/import.rs:112-178`, `crates/musicxml/src/import.rs:403` (tie merging).
10. **`plugin-host` platform/ABI issues:** `parse_cid` panics on non-ASCII 32-byte input (`instance.rs:821-830`); `lazy-arranger-plugin-scan` panics on non-UTF-8 argv (`src/bin/lazy-arranger-plugin-scan.rs:9`); `vst3_binary` hardcodes `Contents/MacOS` breaking Linux/Windows and falls back to arbitrary files (`lib.rs:64-78`, `instance.rs:832-846`); missing `ModuleEntry`/`ModuleExit` & `InitDll`/`ExitDll`; `NativeWindow::open` leaks `NSWindow` on `nsstring` error (`editor.rs:141-153`); `Vst3Effect::parameters` truncates UTF-16 to `u8` (`instance.rs:468-473`); `CFStringGetCString` returns Rust `bool` instead of `u8` (`au.rs:160-165`); ~95 `unsafe` blocks lack `// SAFETY:` comments (`contracts.md:47-48`).
11. **`crates/plugins` blocklist bypass & unbounded metadata reads:** `crates/plugins/src/lib.rs:180-194, 231, 417` (non-canonical path equality allows symlink bypass; missing `is_dir()`; unbounded `fs::read` on `moduleinfo.json`/`Info.plist`).
12. **`project` validation & parsing gaps:** `validate.rs:86-323` runs automation validation 3x (with conflicting `Warning` vs `Error` severities) and plugin validation 2x; `song.rs:750-792` drops `"Am"`/`"F#m"` shorthand minor keys; `song.rs:1012-1014` (`to_json_pretty`) skips `check()` (`contracts.md:16-17`); `legacy.rs:114-555` accepts `version == 0`, fails unsorted tempos, drops `bankLsb`-only sounds; `provenance.rs:173-193` hashes tempo changes outside `start..end` and misses sustained chords before `start`.
13. **`app-commands` edge cases:** `SetTrackGain`/`SetTrackRouting` silently clamp `> 127` (`lib.rs:1311-1326`); `inverse_for` skips `UnknownTrack` for 4 commands (`lib.rs:917-990`); `audio_ref::{trim_start, set_length, split}` don't clamp `fade_in`/`fade_out` (`audio_ref.rs:33-138`); `quantize` is non-atomic and can overflow `note.start + grid / 2` (`quantize.rs:13-45`); `bounce_to_track` writes an orphaned WAV before checking `length > 0` (`bounce.rs:29-60`).
14. **Music theory & arranging logic bugs:** `theory::cadence` misclassifies `V -> IV/ii/bII` as `Half` and `roman_label` emits `"bI"` / `"#V"` (`music-core/src/theory.rs:127-179`); `LessonPack::kind` panics on empty `kinds` (`lesson.rs:72-74`); `Style::compile` unchecked ticks & unsorted chords (`arranger/src/style.rs:354-421`); `arranging::analyze` emits velocity `> 127` and edits melody (`arranging/src/lib.rs:166-195`); `BeatPattern::to_notes` panics on short grid and `put_in_bars` overflows `total_bars` (`beat.rs:147-251`); `DrumFill` negative ticks (`forms.rs:494-514`); `chords::find_chords` feeds drum channel 10 into chord recognition (`chords.rs:25-54`); `generate::melody` ignores `seed` (`generate.rs:225-266`); `Book::from_json`/`from_mfd` accept `0` tempo/meter (`library/src/lib.rs:89-199`).
15. **Audio & mixer logic bugs:** `Playback::fill()` L/R channel swap on odd lengths, `eprintln!` in cpal error callback, missing de-click ramp / single-stream hot-swap (`audio/src/player.rs:65-202, 398`); bus mute/solo inconsistency, instrument tracks ignoring automation, macOS `AuEffect` never calling `apply_state` (`audio/src/lib.rs:585-918`); unchecked chunk offsets in `wav::info` and full-file `fs::read` in `l6max::scan` (`wav.rs:298-316`, `l6max.rs:85`); `duration_seconds()` div-by-zero and >4 GiB WAV truncation (`audio/src/lib.rs:68-143`).
16. **MIDI-devices edge cases:** `profiles::parse_yaml` lets inner `name:` overwrite top-level `profile.name` and ignores mismatched keys (`profiles.rs:395-455`); unbounded SysEx assembly in `collect_sysex_chunks` (`devices.rs:353-371`); inconsistent 0-based vs 1-based raw `u8` channel APIs; `echo` omits CC 123 on exit and never increments `skipped`; `capture.rs:156-160` unchecked microsecond math; `xiosynth.rs:946` `clamp` panic if `min > max`.
17. **Desktop UI polling & channel disconnects:** Per-frame dir/catalog/MIDI-port scans (`app/style.rs:753-756`, `app/mixer.rs:788-793`, `app/devices.rs:1347, 1728`); unhandled `TryRecvError::Disconnected` in device windows and unguarded concurrent download threads (`app/devices.rs:1348-1713`, `app/style.rs:471-483`, `app/layout.rs:3271-3288`).
18. **CLI bugs (`apps/cli/src/lib.rs`):** `midi_monitor` panics on `--seconds inf` (`lib.rs:3941`); `ExportStems`/`Bounce`/`run_transport` lack `--sample-rate`/`--block` bounds (`lib.rs:1469, 1733, 2831`); `midi-map-bind` silently wipes existing bindings on read error (`lib.rs:2350, 2365`); `wav-gain --db -6.0` rejected for missing `allow_hyphen_values = true` (`lib.rs:152-153`).

---

## Low Findings

- `apps/desktop/src/app/devices.rs:645-658`: `send_chord_notes` returns early on send failure without sending NoteOff / CC 123 (`contracts.md:39-40`).
- `crates/smf/src/keyboard.rs:92, 106, 164, 340`: unchecked offset arithmetic in `audit()` and possible 5-byte VLQ emission in `export()`.
- `crates/musicxml/src/lib.rs:42-47`: `escape()` doesn't strip XML 1.0 illegal control characters.
- `crates/project/src/validate.rs:442`: nested note overlap missed (`sounding.insert` overwrites longer outer end); `song.rs:911-912` assumes sorted markers.
- `crates/app-commands/src/clipboard.rs:124-126, 174-177`: `paste_notes` returns `Ok(0)` on unknown track; `paste_chords_repeated` coerces `spacing <= 0` to `1`.
- `crates/music-core/src/roman.rs:48-129` & `reharmonize.rs:83-93`: `pads()` secondary-dominant list mismatch; `Minor6` missing from `is_minor`; `progression.rs:112-118` stacks chords when `chords.len() > bar`.
- `crates/arranger/src/transpose.rs:23-36` & `recognize.rs:41-57`: `ChordType::Fifth` maps 5th to 3rd; missing suspended/6th qualities in `recognize`.
- `crates/audio/src/waveform.rs:23-33`: `peaks()` OOM on huge `bins`; `visible()` float rounding overshoot for `len > 2^24`.
- `crates/plugin-host/src/au.rs:324-763`, `vst3_instrument.rs:50-63, 191-201`, `editor.rs:102-111`, `crates/test-plugin/src/lib.rs:113, 813, 865`: `AudioUnitUninitialize` on failed init, missing `autoreleasepool`, `ViewRect` unchecked subtraction, minor `test-plugin` bugs.
- `apps/desktop/src/app.rs:3192-3298` & `scripts/package-release.sh:28`: temp score PDFs never deleted; `INSTALL.txt` mentions "Check for updates" although `settings::update_status()` reports no update channel.
- `apps/cli/src/lib.rs:1188-1192, 3509-3541`: `ExportKeyboard` silently bitmasks out-of-range channels/programs; `link-to-midi` leaks temp WAV on error.

---

## Handoff Checklist (Suggested PR Batches)

- [ ] **Batch 1 — Security & MCP Sandbox:** C6, C7, H1, H2, H3, H4, M11
- [ ] **Batch 2 — Desktop Data Loss, Persistence & UI Threading:** C8, C9, H24, H25, H26, M1, M2, M3, M17
- [ ] **Batch 3 — `plugin-host` & `test-plugin` Memory Safety / FFI:** C1, C2, C3, C4, C5, H5, H6, H7, H8, H9, H10, M10
- [ ] **Batch 4 — Audio RT Safety, Concurrency, DSP & MIDI:** C10, C11, H11, H12, H13, H14, H15, M15, M16
- [ ] **Batch 5 — Schema, Undo/Redo, Parsers, Score & CLI:** H16, H17, H18, H19, H20, H21, H22, H23, M4–M9, M12–M14, M18
