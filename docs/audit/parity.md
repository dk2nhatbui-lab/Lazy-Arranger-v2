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
| Music Finder books (JSON, CSV, MFD) | `library` | partial: JSON + CSV verified; MFD missing | C++ `core/src/MusicFinder.cpp` |
| Yamaha playlists (TSV) | `library` | verified | 4 round-trip tests, CRLF and ON/OFF flags |
| Music Finder/playlist browser UI | — | missing | no shell panel yet |
| Audio-to-MIDI, monophonic | `audio::analysis` | implemented | `a_440_hz_tone_is_found`, `audio_to_midi_finds_an_a4_note` |
| Audio-to-MIDI, polyphonic/models (Basic Pitch) | — | missing | C++ runs it as an external helper |
| Stem separation (Demucs), instrument tags (PANNs), lyrics (Whisper) | — | missing | external helpers in C++ |
| Audio chord detection | — | missing | C++ `AudioAnalysis.cpp` |
| Link audio (yt-dlp arguments, download parsing) | `audio::link` | verified offline | link/args/parse tests; real download needs yt-dlp |
| YouTube link to MIDI end to end | `link-to-midi` CLI | implemented, not run against YouTube | needs yt-dlp + ffmpeg |
| WAV decoding | `audio::wav` | verified | 16-bit PCM round trip |
| AI/MCP tools for assistants | — | missing | C++ `McpServer`; planned `mcp` crate |
| Style writer: CASM SFF1 and SFF2 records | `style-formats::write_casm_sff1/sff2` | verified round-trip | writer tests check every field per version |
| Style writer: full container (MIDI + CASM) | `style-formats::write_style`, `style-save` | implemented | rewrote a real 15-section style (19,346 bytes) and re-read it |
| Style writer: voices, setup events, keyboard acceptance | — | missing | no PSR-S970 test; voices not written |
| Clip regions (schema v3, migration, split, validation) | `project::song::Clip` | verified | migration and split tests; no UI yet |
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
