# Reference notes: SoundCraft and Cecilia

Two further reference DAWs, added after a source review of their score and
engraving code. Both are permissively licensed, unlike Cadenza and Vibez.

## Identity

| Repository | Commit | Date | Language | Licence | Toolkit |
| --- | --- | --- | --- | --- | --- |
| `storytold/soundcraft` | `915481be1b49d7e0502e07b059d0c0198c61911a` | 2026-11-22 | Rust 2024 | MIT OR Apache-2.0 | `egui` 0.37 through `eframe`, with `egui_kittest` for UI tests |
| `kjmcnamara1/cecilia` | `d54e94990b05446d0a24e0467bebfda41e81ced1` | 2026-11-22 | Rust 2024 | MIT OR Apache-2.0 | custom: `winit` + `wgpu` + `vello` (`vello_cpu`, `softbuffer`, `tiny-skia`, `resvg`, `ttf-parser`) |

Both clones are shallow and read-only. Licence evidence:
`soundcraft/Cargo.toml:9` and `cecilia/Cargo.toml:8` declare
`MIT OR Apache-2.0`; both trees carry `LICENSE-MIT` and `LICENSE-APACHE`.

**Licence verdict: code may be reused with attribution, and ideas may always be
reused.** This differs from Cadenza and Vibez (GPL-3.0-or-later), whose code
must not be imported. This project still prefers re-implementation, because a
copied function cannot be tested against this workspace's contracts.

## What SoundCraft actually has

| Area | Finding |
| --- | --- |
| Score view | `crates/ui-egui/src/panels/score_editor.rs` (8.7 KB) describes itself as a read-only sketch: it draws MIDI notes directly, uses fixed tick-to-pixel spacing and spells accidentals with sharps |
| Notation conversion | `crates/engine/src/score.rs` (28 KB) |
| Print | "Print Score" writes SVG through `crates/engine/src/cmd/file.rs` (21 KB); export quantises to sixteenths |
| Breadth | 14 crates including `au-host`, `clap-host`, `vst3-host`, `midi`, `mix`, `playback`, `model`, `interchange`, `video` |

Useful to adopt: connecting a score view to the DAW's selected clip and the
existing command path. Not useful: its score rendering, which is explicitly a
sketch. Its plugin-host crates are a possible later reference for ADR-007,
under the same permissive licence.

## What Cecilia actually has

| Area | Finding |
| --- | --- |
| Engraving | `crates/cecilia-engrave/src` is 10,594 LOC: `layout.rs` (267 KB), `display.rs` (22 KB), `output.rs` (9 KB), `design.rs` (10.6 KB) |
| Display model | engraving produces pages of drawing items, hit regions and musical positions; the GUI and the PDF exporter consume the same outlines |
| Export | every drawing item, text included, becomes vector output through `pdf-writer`; outlined text loses selection and search |
| Interaction | `design.rs` captures starting geometry and turns pointer displacement into engraving overrides independently of the window toolkit |
| GUI | 279 KB `draw.rs` over winit/wgpu/vello, with `tiny-skia` for some raster work |

Useful to adopt: the engraving seam (immutable layout consumed by both
renderers), interaction geometry separated from the GUI, and shared pagination
between Page View and PDF. Its hit regions use track/bar/voice/beat indices;
this project uses stable IDs (ADR-010). Its engraving entry point lays out one
track with several staves, so ensemble scores need explicit rhythmic-column
alignment (ADR-010 §7).

**Correction to a common summary:** Cecilia is not an egui application. Its GUI
is a custom vello renderer. The "egui app plus Cecilia-style engraver" proposal
combines SoundCraft's toolkit choice with Cecilia's toolkit-independent
engraving, which is only possible because the engraver is separate from the GUI.

## What this changes for this project

* ADR-001 gains a fourth data point: three of the four projects use `egui`
  (Cadenza 0.36, SoundCraft 0.37, and Codex's recommendation); Vibez uses
  `iced`; Cecilia uses a custom renderer. All are permissive.
* ADR-010's engraving architecture is confirmed by a second, larger
  implementation, and its "toolkit-agnostic engraver" requirement is exactly
  what lets SoundCraft's UI choice coexist with Cecilia's engraving model.
* Nothing is pulled forward: notation remains Stage 8.
