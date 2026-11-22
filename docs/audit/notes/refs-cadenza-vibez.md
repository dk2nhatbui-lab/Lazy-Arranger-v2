# Reference audit — Cadenza and Vibez

Shallow clones (depth 1) into `.reference/` on 2026-11-22; nothing built, nothing modified. LOC
counts strip `#[cfg(test)]` bodies and `tests/` files. Dependency licences from
`Cargo.toml`/`deny.toml`/`Cargo.lock` plus the crates.io API.

**Headline.** Both are real, large, working Rust DAWs — not stubs, not doc-only. **Both are
GPL-3.0-or-later.** Their *code* is unavailable to a permissively licensed project; their
*architecture* is first-rate and free to re-implement.

# 1. Cadenza — `github.com/Pep3M/cadenza`

## 1.1 Identity

SHA `6bd80233c20482eba53167233f065039ea42fc36` · branch `dev` (also remote default) · last commit
2026-11-01 17:23:33 -0500 `chore(packaging): cadenza-bin 0.3.3` · v0.3.3, `publish = false`,
edition 2024, MSRV 1.88 (`Cargo.toml:1-12`).

**Language mix** (1016 files): `rs` 489, `flac` 256 (test fixtures), `json` 95, `png` 36, `toml` 29,
`html` 25, `md` 23, `py` 13, `yml` 6, `ttf` 5, 1 `.cdz` fixture. Rust-only product.

**Build.** Cargo workspace, `resolver = "3"`, members `["crates/*"]` → **17 crates**: `core`,
`model`, `theory`, `dsp`, `engine`, `mcp`, `plugin-host`, `agent`, `audio-io`, `midi-io`, `app`,
`library`, `analysis`, `notes`, `update`, `cli`, `test-plugins`. Packaging for three OSes;
`ci.yml:61-67` runs `cargo-deny-action`.

## 1.2 Licence

SPDX **`GPL-3.0-or-later`** at `Cargo.toml:8`. `LICENSE` is verbatim GPLv3, 674 lines, md5
`1ebbd3e34237af26da5dc08a4e440464`. No per-file SPDX headers, no `LICENSE-MIT`/`LICENSE-APACHE`,
no dual licence anywhere.

First 20 lines of `LICENSE`, verbatim:

```
                    GNU GENERAL PUBLIC LICENSE
                       Version 3, 29 June 2007

 Copyright (C) 2007 Free Software Foundation, Inc. <https://fsf.org/>
 Everyone is permitted to copy and distribute verbatim copies
 of this license document, but changing it is not allowed.

                            Preamble

  The GNU General Public License is a free, copyleft license for
software and other kinds of works.

  The licenses for most software and other practical works are designed
to take away your freedom to share and change the works.  By contrast,
the GNU General Public License is intended to guarantee your freedom to
share and change all versions of a program--to make sure it remains free
software for all its users.  We, the Free Software Foundation, use the
GNU General Public License for most of our software; it applies also to
any other work released this way by its authors.  You can apply it to
your programs, too.
```

Section titles in that file: **§5 Conveying Modified Source Versions** (line 208) — strong copyleft,
the derivative work as a whole must be GPLv3. **§11 Patents** (471) — royalty-free patent grant from
every contributor. **§10** (446) — no added restrictions downstream. **Attribution:** notices
retained, and §5d requires Appropriate Legal Notices in an interactive UI. **Network use is not a
trigger:** §13 (552) is *Use with the GNU Affero General Public License* — GPLv3 has no SaaS
disclosure clause; that is AGPL §13.

## 1.3 `deny.toml`

`deny.toml:22-39` is an allow-list: `0BSD`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`,
`BSD-2-Clause`, `BSD-3-Clause`, `BSL-1.0`, `CDLA-Permissive-2.0`, `ISC`, `MIT`, `MPL-2.0`,
`Unicode-3.0`, `Unlicense`, `Zlib`, plus `OFL-1.1`/`Ubuntu-font-1.0` for bundled egui fonts
(`confidence-threshold = 0.9`, `:19`). AGPL/SSPL/non-commercial denied by construction. `:41-59`
grants GPL exceptions **only** to Cadenza's own 17 crates; `[graph] all-features = false` (`:7-9`)
scopes it to default features; `[sources]` (`:67-70`) denies unknown registries and git sources.

## 1.4 Dependency licences in use

`Cargo.lock` carries no licence field, so: declared policy plus a crates.io API check of every
direct dependency. **No GPL/AGPL/SSPL/non-commercial dependency in the default graph.**

| Crate | Licence | Note |
|---|---|---|
| `symphonia` | **MPL-2.0** | file-level copyleft; fine to link |
| `mp3lame-encoder` | **LGPL-3.0** | **not default** — `audio-io/Cargo.toml:14-15` |
| `signalsmith-stretch` | MIT | not default — `audio-io/Cargo.toml:12-13` |
| `clack-host`, `clack-plugin`, `vst3` | MIT OR Apache-2.0 | CLAP and VST3 hosting |
| rest | permissive | `cpal` Apache-2.0 · `eframe`/`egui` MIT OR Apache-2.0 · `midly` Unlicense · `libloading` ISC · `hound`/`flacenc`/`rmcp` Apache-2.0 · `rubato`/`realfft`/`rtrb`/`arc-swap` MIT OR Apache-2.0 |

**Infection risk from dependencies: none.** The risk is Cadenza's own 195k GPL lines.

## 1.5 What it ACTUALLY implements

254,336 raw `.rs` lines, 59,377 tests, **194,959 implementation lines**. Zero `todo!()` or
`unimplemented!()` in non-test source. Module docs describe the code accurately — I spot-checked and
they held.

Non-test LOC per crate: `app` 89,224 (egui UI, `ui/` 52.9k + `state/` 45.4k) · `dsp` 19,227 (10
effects, 7 instruments; `eq.rs` 1,409, `limiter.rs` 931) · `mcp` 13,820 (**95 registered MCP tools**,
20 tool modules) · `plugin-host` 13,303 · `engine` 13,111 · `model` 11,846 · `notes` 9,079 (pYIN,
segmentation, PSOLA) · `agent` 4,382 · `library` 3,392 · `cli` 3,383 · `analysis` 3,132 · `midi-io`
2,875 · `audio-io` 2,007 · `theory` 1,350 · `update` 1,014 · `core` 839.

- **Audio graph** — `Project` compiles to a flat `Graph` off the audio thread
  (`engine/src/compile.rs:1-10`, entry `compile_hosted:68`); `Graph::process` (`graph.rs:1410`) is
  shared by RT playback and offline render.
- **Real-time thread** — `player.rs:3-8` documents it and the code matches: graph handed over an
  `rtrb` ring, previous graph returned on a second ring so drops happen on the control thread
  (`:251-253`: `graph: Option<Box<Graph>>`, `graphs: Consumer<Box<Graph>>`,
  `garbage: Producer<Box<Graph>>`); `load_graph:994-1010` retries with a 2 s deadline. Quantum is
  **32 frames** (`engine/src/lib.rs:78`).
- **Plugin hosting** — both formats, the strongest part: CLAP (`clap/`, ~2.5k lines) and VST3
  (`vst3/`, ~3.9k, own COM shim in `vst3/com.rs`). Each instance can run in a **child process** —
  audio over `mlock`ed shared memory, control over length-prefixed JSON (`sandbox/mod.rs:1-60`,
  `sandbox/shm.rs` 871, `sandbox/protocol.rs:1-10`).
- **Command / undo** — the cleanest pattern in either repo. One `Command` enum
  (`model/src/command.rs:19-23`) carrying every id it creates so replay is deterministic; `apply()`
  returns the inverse (`apply.rs:29`); `Document::execute` pushes `{label, forward, inverse}`
  (`document.rs:73`); groups at `:164`/`:186`, `Batch` = one undo step at `:209`.
- **MCP** — 12k lines; `mcp/src/server.rs:1-10` calls itself a thin `rmcp` adapter over plain tool
  functions, and the code is exactly that. **Persistence** — pretty JSON with a `format_version`
  check (`persist.rs:1-30`) plus sidecar media (`files.rs`, 520). **Transport** — tempo/meter changes,
  count-in, metronome (`metronome.rs` 381), note schedule (`schedule.rs` 717), PDC (`pdc.rs` 260).
- **GUI** — egui/eframe 0.36, 89k impl lines: piano roll, mixer, automation lanes.

## 1.6 Adoptable ideas

**(a) Copyable code — none.** Every line is GPL-3.0-or-later; copying a function makes our tree GPL.

**(b) Patterns worth re-implementing:**

1. **Inverse-command undo, not snapshots.** Serializable `Command`; `apply()` returns `Vec<Command>`
   that undoes it; `Document` keeps `{label, forward, inverse}` (`command.rs:19-23`, `apply.rs:1-13`,
   `document.rs:73`). Because the command is data, UI, CLI and AI share one path
   (`mcp/src/server.rs:1-10`; `CLAUDE.md:5-7`: *"The AI is just another client of the same command API
   the UI uses — never a special path."*). Validate before the first mutation and apply `Batch` on a
   clone, swapping on success (`apply.rs:1-13`) — atomic multi-edit with no rollback code.
2. **Grouped undo with a floor and a ceiling:** `begin_group`/`end_group` record `undo_len` and
   `redo_len` so undo cannot escape a group and redo cannot reach past one (`document.rs:164-209`;
   tests `tests.rs:2203`, `:2227`).
3. **Graph swap over a ring, garbage returned on a second ring** (`player.rs:3-8`, `:251-253`) — the
   audio thread never drops a `Box<Graph>`. This is the pattern the blueprint attributes to
   `ArcSwap`; see (c). Same path for RT and offline (`:10-11`), so a bounce cannot diverge.
4. **Rebuild only what changed:** `swap_schedule` swaps one track's note schedule in place
   (`graph.rs:2118-2135`), `swap_automation` the binding vector (`graph.rs:1075-1078`) — both
   `mem::swap` on pre-built vectors, RT safe. This is what makes "edit while playing" cheap, and it
   matters more for an arranger than a mixer.
5. **"Late is not dead" plugin isolation** (`sandbox/mod.rs:45-60`): a late chunk emits silence or dry
   input and stays outstanding; 2 s with no answer is a hang; `latency`/`tail_seconds` hold their last
   values so PDC stays stable through a crash. Batching costs 96 frames, declared via
   `Processor::latency` so compensation absorbs it (`:24-31`). The failure taxonomy is worth more than
   the SHM implementation.
6. **URI-addressed parameters** (`core/src/param.rs`, 174) — one string names a parameter for
   automation, MIDI mapping and the AI alike. Plus **`deny.toml` as a CI allow-list**.

**(c) Hype / not implemented.** One concrete mismatch the blueprint may be repeating: **`arc-swap` is
declared (`Cargo.toml:38`) and never used** — `grep -rn "arc_swap" --include=*.rs` returns **0 hits**
across all 489 Rust files. There is no `ArcSwap` graph publication; the mechanism is the two `rtrb`
rings. Everything else in the README holds: AI arranger, MCP server, CLAP/VST3 hosting, sandbox and
stems export are present and substantial.

## 1.7 Relevance to our crates

**Copy.** `Command`+inverse+`Document` (§1.6.1-2) → `app-commands`/`project`; the `model`/`engine`
split maps onto `project`/`audio-engine`. Compile off-thread, RT only swaps (`compile.rs:1-10`,
`player.rs:994`) → never walk the project on the audio thread. Per-track `swap_schedule` /
`swap_automation` (`graph.rs:2118`, `:1075`) → `sequencer`. Fixed 32-frame quantum with `max_block`
only validated (`compile.rs:49-51`) → simpler PDC. Tick/PPQ 960 and `TempoMap` in core
(`core/src/tempo.rs` 148) → `music-core`.

**Reject.** egui immediate mode with ~50k lines each of `ui/` and `state/` in one crate — our
`arranger` wants its own render model. JSON project files (`persist.rs:1`) — media in sidecars means
no single movable artefact and no atomic document+media commit. 17 crates at v0.3 — our six are the
right size. Per-instance plugin sandboxing on day one: two process modes, SHM, futex wakeups and a
256 MB frame cap is a multi-month subsystem; take the semantics, defer the transport.

## 1.8 Verdict

**Safe to read for ideas. Do not import code.** GPL-3.0-or-later, no carve-out, no dual licence.

# 2. Vibez — `github.com/alexanderwanyoike/vibez`

## 2.1 Identity

SHA `1d2c708ffedfbe03256e1cc9003c57be417cdfcd` · branch `dev` (also remote default) · last commit
2026-11-21 00:28:29 +0000 `Merge pull request #509 … release-v0.6.8` · v0.6.8, edition 2021
(`Cargo.toml:1-19`).

**Language mix** (1090 files): `rs` 955, `wav` 35, `webp` 20, `png` 13, `md` 12, `toml` 11, `yml` 8,
`ttf` 5, `py` 2, `swift` 1, one `.vibez` fixture.

**Build.** Cargo workspace, `resolver = "2"`, **10 members** (`Cargo.toml:2-13`): the 9 `vibez-*`
crates plus `crates/vibez-plugin-host/tests/fixtures`, a member so the test plugins build. `build.rs`
for `winresource`; `.deb` metadata inline (`:73-95`). **No `deny.toml`** — no CI licence gate.

Non-test LOC per crate: `vibez-ui` 125,352 · `vibez-engine` 68,517 · `vibez-dsp` 16,700 ·
`vibez-plugin-host` 8,550 · `vibez-core` 5,649 · `vibez-instruments` 5,372 · `vibez-audio-io` 2,418 ·
`vibez-project` 1,905 · `vibez-dropbox` 1,297. Totals: 294,139 raw `.rs` lines, 58,337 tests,
**235,802 implementation lines**, zero `todo!()`.

## 2.2 Licence

SPDX **`GPL-3.0-or-later`** (`Cargo.toml:19`; also `crates/vibez-ui/src/about.rs:12`). `LICENSE` is
**byte-identical to Cadenza's** — verbatim GPLv3, 674 lines, md5 `1ebbd3e34237af26da5dc08a4e440464`.
No SPDX headers, no dual licence.

First 20 lines of `LICENSE`, verbatim:

```
                    GNU GENERAL PUBLIC LICENSE
                       Version 3, 29 June 2007

 Copyright (C) 2007 Free Software Foundation, Inc. <https://fsf.org/>
 Everyone is permitted to copy and distribute verbatim copies
 of this license document, but changing it is not allowed.

                            Preamble

  The GNU General Public License is a free, copyleft license for
software and other kinds of works.

  The licenses for most software and other practical works are designed
to take away your freedom to share and change the works.  By contrast,
the GNU General Public License is intended to guarantee your freedom to
share and change all versions of a program--to make sure it remains free
software for all its users.  We, the Free Software Foundation, use the
GNU General Public License for most of our software; it applies also to
any other work released this way by its authors.  You can apply it to
your programs, too.
```

Same restrictions as §1.2: strong copyleft (§5, 208), patent grant (§11, 471), no added downstream
restrictions (§10, 446), attribution plus UI notices (§5d), **no network-use clause** (§13 at 552 is
the AGPL interop clause).

**The extra hazard is in-tree — `crates/vibez-core/src/onset/aubio.rs:1-9` is a declared port:**

```
//! Rust port of aubio's offline onset detector.
//! ... are derived from aubio 0.4.9. ...
//! Original work copyright (C) 2003-2014 Paul Brossier <piem@aubio.org>.
//! aubio is licensed under GPL-3.0-or-later. Vibez carries the same compatible
//! licence. Upstream source: <https://github.com/aubio/aubio/tree/0.4.9>.
```

That file (552 lines), `onset.rs` (877) and `onset/transient_analysis_tests.rs` derive from a GPL-3
upstream, so they are not Vibez's to relicense. **Do not read that file while writing our own onset
detector.**

**Dependency licences.** All permissive or MPL: `cpal` 0.16 Apache-2.0, `iced` 0.13 MIT,
`symphonia` 0.5 **MPL-2.0**, `rubato`/`rtrb` MIT OR Apache-2.0, `midir` MIT, `midly` Unlicense,
`clap-sys` 0.5 and `vst3` 0.3 MIT OR Apache-2.0. **No GPL/AGPL/SSPL/non-commercial dependency** — the
GPL here is Vibez's own code and the aubio port.

## 2.3 What it ACTUALLY implements

Real, and the README tracks the code. `docs/ARCHITECTURE.md` (10 sections, mermaid map at `:6-46`)
and `docs/PATTERNS.md` (10 named patterns, 2 worked traces) describe what is there.

- **Perform / Arrange / Mix** — real modes: `ui/src/state/ui_types.rs:966-970` (`enum Workspace {
  Arrange, Perform, Mix }`), with `domains/perform/`, `domains/arrangement/`, `domains/piano_roll/`
  and one domain per effect (`eq.rs`, `compressor.rs`, `delay.rs`, … 60+ files).
- **Capture** — `engine_capture_lifecycle*.rs`, `engine_capture_commands.rs`,
  `engine_capture_admission.rs`, `engine_capture_start_rearm_*.rs` (~3.5k lines incl. tests) record
  Section launches, mutes, live playing and automation gestures into ordinary clips. `README.md:55-58`
  claims captured material keeps no back-reference to its Sections; `capture_replay`/
  `presentation_state` at `engine.rs:53-60` back that up.
- **Quantisation** — *launch* quantisation is a first-class enum (`MusicalBoundary`,
  `SectionLaunchQuantization`) held per track, overridable per clip (`clip_launcher.rs:26-50`),
  drained as a queued presentation (`engine_drain_commands.rs:227`, `:261`). *Note* quantisation is
  deliberately absent — only opt-in swing, `GrooveGrid::Off` default, "Vibez never guesses the
  rhythmic intent of freely recorded notes" (`core/src/perform.rs:1-14`).
- **Clips** — launcher slots (`clip_launcher.rs`), engine clip batches (`engine_clip_batch.rs` 381 +
  1,509 lines of tests), `engine_clip_record.rs`, `engine_clip_launcher.rs`.
- **UI rendering** — iced 0.13 with custom `iced::widget::canvas` programs
  (`app/views_arrangement.rs:27`, `:341`: `TrackClipCanvas`, `TimeRulerWidget`), 42.7k lines of
  widgets, shared theme/typography, `dev_gallery.rs` for screenshot checks.
- **Audio / MIDI backends** — `cpal` 0.16 (`audio_stream/callback.rs`, `handover.rs`), `midir` 0.10,
  `symphonia` decode, `rubato` resample, `audio_thread_priority` (`audio-io/src/realtime.rs`), PDC
  across five `compensation*.rs` files.
- **Plugin hosting** — VST3 (`vst3_host/`: `abi.rs` 554, `instance.rs` 942, `runloop.rs` 498) and
  CLAP (`clap_host/`: `instance.rs` 848, `host_impl.rs` 579). **In-process**, unlike Cadenza; only
  *scanning* is sandboxed (`scanner.rs:100-146`).

## 2.4 Adoptable ideas

**(a) Copyable code — none.** GPL-3.0-or-later, plus aubio's GPL in the onset module.

**(b) Patterns worth re-implementing:**

1. **`RetirementQueue` with reserve-before-publish allowances** (`engine/src/retirement_queue.rs:1-50`):
   event queue plus atomic `Ledger`; `RetirementAllowance` is `#[must_use = "unused allowance returns
   its reserved capacity on drop"]`, `RetirementPermit` `#[must_use = "consume this permit when
   publishing its original owner"]`. The UI thread *reserves* capacity before transferring ownership,
   so the audio thread can never fail to enqueue and never allocates; `command_ownership.rs` (69)
   encodes the rule as a type. Strictly better than Cadenza's retry-with-deadline (`player.rs:994-1010`)
   — the most transferable engineering in either repo.
2. **Two rings, MVU, and a named pattern language** (`PATTERNS.md:17-31`): widget → `Message` →
   routing → pure domain update → `Action` → one app-layer applier. "Domains never touch iced, never
   spawn tasks, never reach into another domain's state." `:34-40` states the rule we want verbatim:
   every `EngineCommand` variant must be safe to drop on the audio thread. Commands carry prepared,
   already-allocated payloads (`engine/src/commands.rs:25-31`: `Box<PreparedRouting>`,
   `Arc<DecodedAudio>`, `PreparedClip…`), built on the sender's thread so the audio thread only
   installs.
3. **Launch quantisation as data, not a scheduling hack** (`clip_launcher.rs:26-50` plus the
   `scheduled_presentation` queue at `engine.rs:54`) — "start this clip at the next bar" without
   lying about the playhead.
4. **Swing as a pair-preserving beat map** (`core/src/perform.rs:27-40`): pair endpoints fixed, the
   straight midpoint moved to the nearest 96-PPQN tick, interpolation "cannot reorder events". A pure
   `f64 -> f64` groove function is trivially testable and cannot corrupt note order — better than
   nudging stored note times. Same discipline in `warp_marker.rs` (532) and `track/fades.rs` (617),
   value objects owning their invariants (`PATTERNS.md:416`).
5. **Magnetic snap measured in screen pixels** (`ui/src/timeline_snap.rs:8-21`):
   `MAGNET_RADIUS_PX: f32 = 8.0`, "Fixed on screen so the pull feels the same at every zoom", with
   `SnapTargetKind` naming grid/clip-edge/playhead/loop/selection and `Snapped { beat, target }`
   reporting the hit so the UI can draw it. Snap measures from the dragged edge; Alt bypasses
   (`:38-39`).
6. **Docs as a contract** (`PATTERNS.md:1-15`): "Reviews cite these names. A PR that introduces a new
   convention adds it here in the same PR." Cheap, and it is why 235k lines stay readable.

**(c) Hype / not implemented.** Almost nothing; `README.md:43` is honest that multiple MIDI ports and
generic `.vdc` controller mapping are "still to come". Two caveats: **undo is snapshot-based and lives
in the UI crate** (`ui/src/state/snapshot.rs:10-24`, `UndoHistory` capped at `CAPACITY = 100` at
`:86`, `UndoGestureId` at `:36-47` to collapse a drag) — it works (`PATTERNS.md:465`) but nothing
outside the UI can produce an undoable edit; and **plugin crash isolation is absent**, so a misbehaving
plugin takes the app down.

## 2.5 Relevance to our crates

**Copy.** `RetirementQueue` ownership (`retirement_queue.rs`) → `audio-engine`, re-implemented from the
description. Prepared payloads (`commands.rs:25-31`) → `audio-engine`. Launch-boundary quantisation as
data (`clip_launcher.rs:26-50`) → `sequencer`/`arranger`. Swing as a pure beat map
(`perform.rs:27-40`) → `music-core`, keeping MIDI data honest. Pixel-constant magnetic snap with a
reported target (`timeline_snap.rs:8-21`) → `arranger`. Pattern-language discipline
(`PATTERNS.md:17-31`) → `app-commands`.

**Reject.** Undo in the UI crate (`snapshot.rs:74-83`): snapshot undo cannot serve a CLI, an MCP server
or a scripted batch, and 100 snapshots of a shared state tree is a memory cliff on a large arrangement
— take Cadenza's inverse-command model. The SQLite single-blob project (`project_format_v1.rs:1-39`:
whole document as one JSON `BLOB` in `project_document` beside sha256-keyed media blobs,
`APPLICATION_ID = 0x565a_5031` at `:24`): it *does* buy atomic document+media commit, which Cadenza
lacks, but the document is opaque — no partial load, no merge — and `rusqlite` is a C dependency. Take
the container, reject the blob. `vibez-ui` as one 518-file, 125k-line crate depending on `vibez-engine`
directly (`vibez-ui/Cargo.toml:9`) — our `arranger` should sit behind a command/event boundary.
In-process plugin hosting — adopt Cadenza's isolation semantics first. And the missing `deny.toml`.

## 2.6 Verdict

**Safe to read for ideas. Do not import code.** GPL-3.0-or-later plus an in-tree aubio-derived GPL-3
file. Read `docs/PATTERNS.md` and `docs/ARCHITECTURE.md` freely; avoid
`crates/vibez-core/src/onset/aubio.rs` if we write our own onset detector.

# 3. Compatible with staying MIT/Apache-2.0?

**No. Neither.** Same answer for both, and it is not close. Both declare
`license = "GPL-3.0-or-later"` (`cadenza/Cargo.toml:8`, `vibez/Cargo.toml:19`) and ship the verbatim
GPLv3 text. No `LICENSE-MIT`, no `LICENSE-APACHE`, no dual licence, no carve-out module.

Operative text: **§5** (line 208) — a work "based on the Program" must be licensed "as a whole" under
GPLv3. **§10** (446) — downstream may not be given additional or different terms, so GPL code cannot be
re-offered as MIT. **§11** (471) — the patent grant flows only under GPL terms.

| Action | Result |
|---|---|
| Read the source, re-implement the architecture | **Allowed.** Ideas and interfaces are not copyrightable; expression is. |
| Copy a function, module or file into our tree | **Forbidden.** Our crate becomes a derivative, GPLv3 as a whole. |
| Link a Cadenza/Vibez crate as a dependency | **Forbidden.** A statically linked Rust binary is one work (§5). |
| Vendor a snippet to "just check the algorithm" | **Forbidden**, and the hardest mistake to unwind. |
| Run their binary as a separate process over IPC | **Allowed** (§2 "mere aggregation") — and useless to us. |
| Host a modified fork as a service | Not a GPL trigger: §13 (552) is the AGPL interop clause, not a network clause. |

**Rule:** treat both as reference documentation with an executable proof. Read them, cite the SHAs in
design notes, re-derive the patterns in our own words and code. Cadenza's own `CLAUDE.md:9-10` models
the right sentence: *"We take ideas, never code or assets (clean-room)."* If we ever ship GPL-compatible
instead the picture inverts: Cadenza's `vst3` + `clack-*` stack and Vibez's `clap-sys` route are both
MIT/Apache, and Cadenza's LGPL LAME behind a non-default feature shows how to keep the viral surface off.

# 4. Reuse register

| Item | Source | Class | Consequence |
|---|---|---|---|
| Inverse-command undo + `Batch` atomicity | cadenza `model/src/{command,apply,document}.rs` | (b) pattern | none |
| Graph swap over rtrb + garbage ring | cadenza `engine/src/player.rs:3-8,251-253` | (b) pattern | none |
| Per-track `swap_schedule`/`swap_automation` | cadenza `engine/src/graph.rs:2118,1075` | (b) pattern | none |
| "Late is not dead" failure taxonomy | cadenza `plugin-host/src/sandbox/mod.rs:45-60` | (b) pattern | none |
| `deny.toml` allow-list gate | cadenza `deny.toml:22-39` | (b) config | none |
| Retirement queue + allowance/permit | vibez `engine/src/retirement_queue.rs` | (b) pattern | none |
| MVU, pure domains, `Action` applier | vibez `docs/PATTERNS.md:17-31` | (b) pattern | none |
| Launch quantisation as data | vibez `project/src/clip_launcher.rs:26-50` | (b) pattern | none |
| Swing as pair-preserving beat map | vibez `core/src/perform.rs:27-40` | (b) pattern | none |
| Pixel-constant magnetic snap | vibez `ui/src/timeline_snap.rs:8-21` | (b) pattern | none |
| Atomic document+media container | vibez `project/src/project_format_v1.rs` | (b) idea only | reject the blob |
| Any source file, any line | both | (a) unavailable | GPL-3.0-or-later |
| `ArcSwap` graph publication | — (blueprint claim) | (c) not real | `arc-swap` unused |

**Bottom line:** cite both as prior art, licence-gate them out of the dependency tree, and correct the
ArcSwap claim.
