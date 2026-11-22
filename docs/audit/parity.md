# Feature parity with the C++ application

The target is behavioural parity: same features, same observable results, no
regressions. Status values: **verified** (tested here), **implemented**
(builds and runs, needs comparison), **missing**, **not planned**.

| C++ capability | Rust module | Status | Evidence |
| --- | --- | --- | --- |
| Song/project model, undo, validation | `project`, `app-commands` | verified | 234 tests, schema v2, inverses |
| Legacy C++ project import (v1..v4) | `project::legacy` | verified | fixtures `legacy-cpp-v3/v4` |
| Style parser (SFF1/SFF2, CASM) | `style-formats` | verified | 16,219 containers, 856,370 rules |
| Style section import to playable style | `style-formats::import` | verified | 16,158 styles, 48.8M notes |
| Chord conversion and rules | `arranger` | verified | oracle-pinned cases |
| Scheduling with late-event clamping | `sequencer` | verified | clock tests |
| MIDI devices, capture, all-notes-off | `midi-devices` | verified (hardware manual) | unit tests; hardware untested |
| Style writer | — | missing | Stage 4 remainder |
| Setup events, voices, CSEG children | — | missing | Stage 4 remainder |
| Retrigger on section change | typed, not acted on | missing | ADR-003 |
| Notation/score engraving | — | missing | ADR-010 (Stage 8) |
| UI shell (Perform/Arrange/Mix) | `apps/desktop` | implemented | themed shell, editing, audio |
| Audio playback | `audio` | implemented | cpal; audible check pending |
| WAV/offline render | `audio` | verified | 5.36 s real-style render |
| Device profiles (Launchkey, XioSynth, D-05, S970, X-TOUCH) | `midi-devices` | partial | profiles typed, not exercised |
| AI/MCP | `apps/mcp` | missing | Stage 8; `mcp` crate planned |
| Korg Pa import | — | missing | Stage 8 extension |
| Plugin hosting (VST3/CLAP) | — | missing | ADR-007, deferred |

This table is the working checklist: each row moves only with evidence, never
by assertion.
