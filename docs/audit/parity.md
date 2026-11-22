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
| Music Finder books (JSON, CSV, MFD) | `library` | verified | MFD read 1,653 records from the real Nhat.mfd, matching the C++ doc; CLI music-finder-import |
| Yamaha playlists (TSV) | `library` | verified | 4 round-trip tests, CRLF and ON/OFF flags |
| Music Finder/playlist browser UI | — | missing | no shell panel yet |
| Audio-to-MIDI, monophonic | `audio::analysis` | implemented | `a_440_hz_tone_is_found`, `audio_to_midi_finds_an_a4_note` |
| Audio chord recognition (chroma + recogniser) | `audio::chords` | verified | 4 tests; 4 s C triad WAV found C twice and wrote ticks 0/3840 |
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
| Clip regions (schema v3, migration, split, validation) | `project::song::Clip` | verified | migration and split tests |
| Recent projects list (10 entries, missing dropped, clear) | `project::recent`, shell sidebar, CLI `recent` | verified | 3 tests + CLI run |
| Clip edit commands (insert, move, resize, split, remove) | `app-commands::EditSession` | verified | one undo step each; overlap rejected |
| Clip lane: draw, select, arrow-move, shift+arrow resize, J join, N add, Delete remove | `apps/desktop` | implemented | geometry test; UI manual |
| Clip join command | `app-commands::join_clips` | verified | spans both, one undo step |
| Chord progression text parser (`\|` bar, `%` repeat, `/` hold) | `music-core::progression` | verified | 6 parser tests |
| Chord lane: draw and K dialog writing through commands | `apps/desktop` | implemented | editor progression test; UI manual |
| Roman numerals in the song key | `music-core::roman` | verified | 5 tests, key transposition, suffix rejection |
| Next-chord suggestions (core) | `music-core::roman::suggest_next` | verified | functional IV/V/vi/ii and minor set |
| 16 chord pads (diatonic, borrowed, secondary dominants) | `music-core::roman::pads` | verified | 16-pad test in C and G |
| Pads and suggestions in the chord dialog | `apps/desktop` | implemented | buttons append to the text field; UI manual |
| Reharmonize rules (tritone, borrowed, ii-V, relative) | `music-core::reharmonize` | verified | 5 rule tests |
| Reharmonize dialog and bar replacement | `apps/desktop`, `Editor::reharmonize` | implemented | editor undo test; UI manual |
| Find Chords from the Music | `arranger::recognize`, `Editor::find_chords` | verified | triad, sevenths, inversion, minor, dim tests; F key |
| Arrangement Doctor (crowded bars, literal repeats) | `arranging::doctor` | partial verified | 4 rule/apply tests; D dialog |
| Fit to My Synths | `arranging::fit_to_synth` | verified core | 3 tests: range octaves, polyphony removal, apply; default synth limits |
| Groove extraction, application, feel match | `arranging::groove` | verified core | 5 tests: averaging, amounts, match, bad grids |
| Groove applied from the selected track (G) | `apps/desktop` | implemented | editor test moves a second track and undoes |
| Song Forms (sections, presets, plan, apply) | `arranging::forms` | verified | section/plan tests and apply/undo test |
| Song Forms dialog (Y) | `apps/desktop` | implemented | four presets |
| MusicXML export (partwise, rests, drum clef) | `crates/musicxml` | verified | 4 tests + valid XML from a real project |
| MusicXML import (parts, measures, notes, rests, chords, drums, tempo/meter) | `musicxml::import` | verified | 4 tests + real round trip 94 notes; drum pitch/velocity lost, no lyrics/marks |
| Score layout engine (pages, systems, measures, positioned notes) | `crates/score` | verified core | 4 tests + real project layout |
| Score canvas renderer (staff, barlines, noteheads, page nav, zoom) | `apps/desktop/src/score_view.rs` | implemented | geometry tests; UI manual |
| Key detection, spelling and accidentals | `score::spelling` | verified | 6 tests: scales, chromatic, signatures, flat keys, key detection |
| Vector PDF score export | `score::pdf`, CLI `score-pdf` | verified | pdfinfo: 3 pages A4, title/page text extractable; xref test |
| Ties and lyrics in the layout and both renderers | `score::engrave_with` | verified | per-pitch tie tracking; 40-bar two-page acceptance test |
| PSR-S970 keyboard song export (setup bar, voices, chords, drums ch10) | `smf::export_keyboard` | implemented | 3 tests; real project 94 notes, 5 tracks; programs from profile name matching |
| Style Remote, style-by-number, keyboard acceptance | — | missing | no hardware test |
| Transitions: drum fill, lead-in, automatic | `arranging::forms::transition` | verified | 4 tests incl. apply/undo; T key |
| Transitions: hold chord, break, straight cut presets | — | partial | straight cut is a no-op; hold/break missing |
| Assistant tool registry and JSON dispatch | `crates/mcp` | verified (6 tools) | get_song/add_track/add_notes/set_chords/get_undo/undo; CLI mcp-tools and mcp-call |
| MCP stdio transport (initialize, ping, tools/list, tools/call, parse errors) | `mcp::serve`, `mcp-serve` CLI | verified | in-memory tests + real stdin/stdout run saved an added note |
| MCP socket transport, per-start secret | — | missing | — |
| MCP tools over arranging engine (doctor, find chords, grooves, forms, reharmonize) | `crates/mcp` | verified | 12 tools total; tests per tool |
| Remaining MCP tools (~80: styles, clips, MIDI, profiles, lyrics) | — | missing | — |
| Setup events, voices, CSEG children | — | missing | Stage 4 remainder |
| Retrigger on section change | typed, not acted on | missing | ADR-003 |
| Notation/score engraving | — | missing | ADR-010 (Stage 8) |
| UI shell (Perform/Arrange/Mix) | `apps/desktop` | implemented | themed shell, editing, audio |
| Audio playback | `audio` | implemented | cpal; audible check pending |
| WAV/offline render | `audio` | verified | 5.36 s real-style render |
| Device profile voice matching | `midi-devices::profiles` | partial | PSR-S970 subset + keyword fallback; EP/Rhodes mapping; 3 tests |
| Named style parts in recorded tracks | `performance::part_label` | verified | style part names, else drums/bass/chords/melody/pad heuristics |
| Per-track keyboard program (schema v4) | `project`, `app-commands`, `mcp` | verified | migration, command, MCP tool; export honors it |
| Mute / solo / arm (schema v5) | `project`, `app-commands`, `sequencer`, `audio` | verified | mix rules tested; render/sequencer honor them; M/S in the track list |
| Track volume and pan (schema v8) | `project`, `audio`, `smf` export | verified | fader/pan render tests; CC 7/10 written; defaults omitted |
| Mixer panel (per-track fader, pan, mute, solo) | `apps/desktop` | implemented | one undoable command per change; UI manual |
| Volume and pan automation lanes (schema v9) | `project`, `audio`, `app-commands` | verified | interpolation/hold, per-note render, command undo |
| Automation lane UI (click to add/remove volume points) | `apps/desktop/src/automation_view.rs` | implemented | mapping tests; UI manual |
| Beat view (4 rows × 16 steps, velocity cycle, apply/undo) | `arranging::beat`, `apps/desktop` | verified core | 4 tests; B opens the grid |
| Clip grid model (scenes × tracks, slots) | `arranging::grid`, MCP `get_clip_grid` | verified core | 3 tests + tool |
| Scene launcher (bar-quantised, waiting/cancel, log) | `arranging::grid::Launcher` | verified core | 2 tests |
| Record played scenes to the timeline | `arranging::grid::record_to_timeline` | verified core | notes and clips appended, undoable |
| Loops view: scene rows, slots, launch/queue, record take | `apps/desktop` | implemented | UI manual; launcher/recorder tested |
| Scene names (schema v6) and rename | `project`, `app-commands`, `apps/desktop` | verified | migration round trip, command undo, rename test |
| Slot editing: set length, clear, copy to a tick | `arranging::grid`, MCP `edit_slot` | verified | 4 grid tests + tool test |
| Device profiles: YAML loader, banks, drum kits, other devices, editors, mixer maps | — | missing | earlier claim of five typed profiles was wrong; corrected |
| AI/MCP | `apps/mcp` | missing | Stage 8; `mcp` crate planned |
| Korg Pa import | — | missing | Stage 8 extension |
| Plugin hosting (VST3/CLAP) | — | missing | ADR-007, deferred |

This table is the working checklist: each row moves only with evidence, never
by assertion.
