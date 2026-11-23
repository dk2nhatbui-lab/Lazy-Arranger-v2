# Feature parity with the C++ application

The target is behavioural parity: same features, same observable results, no
regressions. Status values: **verified** (tested here), **implemented**
(builds and runs, needs comparison), **missing**, **not planned**.

## State at round 217

This table grew over many rounds and some middle rows describe earlier states.
The reconciled view is
[`cpp-features-crosscheck.md`](cpp-features-crosscheck.md): 39 sections, **39
partial and 0 missing at row level**, no section fully verified. `cargo test
--workspace` passes **655 tests**; fmt, clippy with `-D warnings` and rustdoc
pass.

Verified by tests or offline evidence: project schema v20 with migrations,
undo/inverses and validation; legacy import; Yamaha style parse/import/compile
and SFF1/SFF2 writing with track programs and bank select; clip model, slots, launcher and take recording; chord
parsing, roman numerals, reharmonisation and find-chords; doctor/fit with A/B,
groove files, forms and transitions; score layout with ties, beams, tuplets,
rests, lyrics (spaced), slurs, percussion, chord symbols, guitar frames,
accidental spacing, canvas and PDF/MusicXML round trip; audio render with six
timbres and SF2, references with fades/stretch/waveform zoom, mixing, buses, master reverb/chorus/peak-RMS/per-track meters,
automation, stems/freeze, float WAV and input capture (capture unverified);
MIDI map with learn and a Launchkey pad preset, style catalog, theory helpers,
settings with editable shortcuts, six languages with a translated Help window, named demos and the
**93-tool** MCP server; marquee and Shift-click multi-clip selection, lyric Go, meter/measure numbers,
printer choice and the Songs & tutorial window.

Unverified: real MIDI hardware, PSR-S970 acceptance, audible cpal output on
every host, microphone capture, printing, YouTube download, and visual
comparison with the C++ app.

Still missing: plugin hosting/processing/editors, SF3 and modulators, Korg
program/combi decoding, L6max project/mixer-page parsing, takes and destructive
waveform/slice editing, full plural rules and other locales, a real AI model and
speech input. Input monitoring exists but is hardware-unverified.

| C++ capability | Rust module | Status | Evidence |
| --- | --- | --- | --- |
| Song/project model, undo, validation | `project`, `app-commands` | verified | schema v20 with tested migrations, inverses and validation |
| Legacy C++ project import (v1..v4) | `project::legacy` | verified | fixtures `legacy-cpp-v3/v4` |
| Style parser (SFF1/SFF2, CASM) | `style-formats` | verified | 16,219 containers, 856,370 rules |
| Style section import to playable style | `style-formats::import` | verified | 16,158 styles, 48.8M notes |
| Chord conversion and rules | `arranger` | verified | oracle-pinned cases |
| Scheduling with late-event clamping | `sequencer` | verified | clock tests |
| MIDI devices, capture, all-notes-off | `midi-devices` | verified (hardware manual) | unit tests; hardware untested |
| Device profiles: YAML voices, banks and kits drive voices and bank select | `midi-devices::profiles`, `smf::keyboard`, CLI `export-keyboard --profile` | verified | parse tests; export audit banks [63,0,0,0]/[0,1,1,1]; bass resolved to 33 |
| Music Finder books (JSON, CSV, MFD) and CSV→playlist | `library`, CLI `music-finder-import`/`playlist-from-csv` | verified | MFD read 1,653 records; CSV→TSV wrote 2 entries with paths and REGIST |
| Yamaha playlists (TSV) | `library` | verified | 4 round-trip tests, CRLF and ON/OFF flags |
| Music Finder/playlist browser UI, tempo apply and style import | `library_view`, `app_commands::style_import`, Library window, CLI `library-import` | verified | filter/copy tests; real import 1 track/1,926 notes; ignored env test passes |
| Audio-to-MIDI, monophonic | `audio::analysis` | implemented | `a_440_hz_tone_is_found`, `audio_to_midi_finds_an_a4_note` |
| Audio chord recognition (chroma + recogniser) | `audio::chords` | verified | 4 tests; 4 s C triad WAV found C twice and wrote ticks 0/3840 |
| Audio-to-MIDI, polyphonic/models (Basic Pitch) | `arranging::audio` monophonic | partial | monophonic/chroma only; no model or polyphony |
| Stem separation (Demucs), instrument tags (PANNs), lyrics (Whisper) | `audio::render_stems`, lyrics model | partial | offline stems and lyric events exist; no Demucs/PANNs/Whisper |
| Link audio (yt-dlp arguments, download parsing) | `audio::link` | verified offline | link/args/parse tests; real download needs yt-dlp |
| YouTube link to MIDI end to end | `link-to-midi` CLI | implemented, not run against YouTube | needs yt-dlp + ffmpeg |
| WAV decoding | `audio::wav` | verified | 16-bit PCM, 24-bit PCM, 32-bit PCM and 32-bit float; L6max float file imported |
| Style writer: CASM SFF1 and SFF2 records | `style-formats::write_casm_sff1/sff2` | verified round-trip | writer tests check every field per version |
| Style writer: full container (MIDI + CASM) | `style-formats::write_style`, `style-save` | implemented | rewrote a real 15-section style (19,346 bytes) and re-read it |
| Style writer: voices, setup events, keyboard acceptance | — | missing | no PSR-S970 test; voices not written |
| Clip regions (schema v3, migration, split, validation) | `project::song::Clip` | verified | migration and split tests |
| MIDI/MusicXML/WAV/stem import and export (CLI and shell) | `smf::decode`, `musicxml::import`, `load_project`, Open/Save-as window | verified model | MIDI and MusicXML reload tests; file window manual |
| Recent list (Clear) and Save-as extension | `project::recent`, shell sidebar, CLI `recent` | verified | recent tests + `normalize_save_path` test |
| Clip edit commands (insert, move, resize, split, remove) | `app-commands::EditSession` | verified | one undo step each; overlap rejected |
| Clip lane: draw, select, arrow-move, shift+arrow resize, J join, N add, Delete remove | `apps/desktop` | implemented | geometry test; UI manual |
| Clip join command | `app-commands::join_clips` | verified | spans both, one undo step |
| Clipboard: notes (multi), chord ranges incl. Shift+V x4, Score/chord-lane selection | `clipboard`, Editor, canvases | verified | clipboard/multi/repeat tests; canvas hit tests |
| Quantise all starts and set velocity | `Editor::quantize`, `set_velocity`, Arrange buttons | verified | off-grid insert test; UI manual |
| Drag notes, velocity lane, piano keys and select-all delete | `piano_roll`, Editor `delete_notes` | verified mapping | hit/drag/lane tests; multi-delete undo test; audition manual |
| Drum onset detection, slicing, GM mapping, pad window and MIDI-triggered pads | `audio::drums`, `pads`, `midi_devices::map`, shell | verified core | click-loop and map tests; pad audio playback needs hardware |
| Seeded generators: humanise, drumify, bass/pad/arp, continue, melody | `arranging::generate`, CLI and MCP `generate` | verified | 5 generator tests + tool test; CLI wrote a melody and a continuation |
| Chord progression text parser (`\|` bar, `%` repeat, `/` hold) | `music-core::progression` | verified | 6 parser tests |
| Chord lane: draw and K dialog writing through commands | `apps/desktop` | implemented | editor progression test; UI manual |
| Roman numerals in the song key | `music-core::roman` | verified | 5 tests, key transposition, suffix rejection |
| Next-chord suggestions (core) | `music-core::roman::suggest_next` | verified | functional IV/V/vi/ii and minor set |
| Chord inversions and A/B voicing audition | `music-core::voicing`, CLI `voicing` | verified | triad/seventh inversion tests; A/B WAVs written |
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
| MusicXML export/import (harmony, lyrics, marks, ties, drums, chords) and round-trip audit | `musicxml`, CLI `music-xml-audit` | verified with a known limitation | demo clean (186=186); style project keeps 1,423/1,423 counts with one start mismatch inside an overlap |
| Score layout engine (pages, systems, measures, positioned notes) | `crates/score` | verified core | 4 tests + real project layout |
| Score canvas renderer, click note entry (quantised tick and staff pitch) | `apps/desktop/src/score_view.rs`, `score` mapping | verified mapping | entry mapping test; UI manual |
| Key detection, spelling and accidentals | `score::spelling` | verified | 6 tests: scales, chromatic, signatures, flat keys, key detection |
| Vector PDF score export | `score::pdf`, CLI `score-pdf` | verified | pdfinfo: 3 pages A4, title/page text extractable; xref test |
| Ties and lyrics in the layout and both renderers | `score::engrave_with` | verified | per-pitch tie tracking; 40-bar two-page acceptance test |
| Song lyrics (schema v10) and MusicXML lyric interchange | `project`, `musicxml`, `score` | verified | JSON round trip; export/import lyric test; layout places them |
| Dynamics and articulations (schema v11) | `project`, `musicxml`, `score`, renderers | verified | round trip; placed above/below staff; canvas/PDF text |
| Settings file (language, grade, MIDI keyboard, metronome, reduced motion, Kids Mode), editable undo/redo/save/open/play bindings and shortcut window | `apps/desktop/src/settings.rs`, Shortcuts window | verified | round trip, clamping, missing file, parser tests, F1/window capture |
| Six languages; Kids Mode restricts tabs, enlarges controls, shows a synced chord lesson plus kids demo and a simplified Arrange sidebar with Lessons/Theory | `strings.rs`, `kids.rs`, `demo::kids_song`, Perform view | verified lesson | sync/step tests; kids demo validates; UI manual |
| MIDI control map (notes/CC to actions) with text persistence and learn capture | `midi-devices::map`, CLI `midi-map-*` | verified | 6 tests + CLI bind/show run |
| Live MIDI input, MIDI map/Controllers windows, channel-check messages, desktop record | `midi-devices`, shell, CLI | implemented | map/learn/controller/byte and recorded-insert tests; no MIDI ports, hardware untested |
| Numbered style catalog, resolution and by-number rendering | `style-formats::catalog`, CLI `style-catalog`/`style-resolve`/`style-by-number` | verified | resolve test; real library matched 'ABBA Fernando'; number 2 rendered 124 notes |
| MCP tools for automation, lyrics/marks, style catalog, theory, generation, plugins, L6max, Korg scan, quantise, velocity, tempo, SoundFonts, buses and routing, plus track/note/chord/marker/clip/scene and track-property edits, note/chord edits, single-object reads, lyrics/marks/patches, groove files, markers/lyrics/queries, clips, buses, plugins and PDF | `crates/mcp` | verified | tool tests and CLI calls; 93 tools total |
| Local assistant panel (help/song/undo/tool commands over the registry) | `mcp::assistant`, shell window | verified core | parse and dispatch tests; UI manual |
| Speech output for replies (macOS `say`) | `apps/desktop/src/speech.rs` | verified builder | cleaning, truncation and argument tests; audible speech unverified |
| Synth families: piano, bass, organ, strings, pad, drums (with noise percussion) | `audio::Timbre` | verified | timbre tests, pad swell, drum energy; demo RMS 0.129 |
| Settings device list and update answer | `apps/desktop/src/settings.rs`, settings window | verified helper | port summary and update text tests; device enumeration needs hardware |
| Built-in sixteen-bar demo (drums, bass, piano, pad, chords, marks, lyrics) | `arranging::demo` | verified | validation, counts, render test; shell first run; CLI `demo-project` |
| Theory helpers: scales, roman labels, cadences, voice-leading cost | `music-core::theory`, CLI `theory-*` | verified | 4 tests + CLI runs |
| PSR-S970 keyboard song export (setup bar, voices, chords, drums ch10) | `smf::export_keyboard` | implemented | 3 tests; real project 94 notes, 5 tracks; programs from profile name matching |
| Style Remote, style-by-number, keyboard acceptance | — | missing | no hardware test |
| Transitions: drum fill, lead-in, automatic | `arranging::forms::transition` | verified | 4 tests incl. apply/undo; T key |
| Transitions: hold chord, break, straight cut presets | `arranging::forms` | verified | hold/break command tests; straight cut is a no-op |
| Assistant tool registry and JSON dispatch | `crates/mcp` | verified (6 tools) | get_song/add_track/add_notes/set_chords/get_undo/undo; CLI mcp-tools and mcp-call |
| MCP stdio transport (initialize, ping, tools/list, tools/call, parse errors) | `mcp::serve`, `mcp-serve` CLI | verified | in-memory tests + real stdin/stdout run saved an added note |
| MCP socket transport, per-start secret | `mcp::socket`, CLI `mcp-socket` | verified | loopback exchange: token accepted, wrong token rejected; token is 32 hex chars |
| MCP tools (song, score, audio, arranging, files) | `crates/mcp` | verified | 93 tools; per-tool tests and CLI calls |
| Setup events, voices, CSEG children | `style-formats` | partial | SFF write verified; voices/setup events not written |
| Retrigger on section change | typed, not acted on | missing | ADR-003 |
| Notation/score engraving | `score`, `score_view`, `pdf`, `musicxml` | verified | layout/ties/beams/tuplets/grand-staff/rest/slur/percussion/lyric/accidental/rhythmic-spacing/dots/dotted-rests/frame/chord-symbol/PDF tests; slurs round-trip; Save PDF, Print dialog (printer/copies/page ranges via lp; unverified), score drag-move/Shift-resize; PDF/canvas checks, four-measure systems, meter digits and a headless canvas-text test |
| Record-and-detect chords toggle | `record_chords`, `editor.find_chords` | verified pieces | settings round trip; chord detection tests |
| Editable shortcuts and Shortcuts window | `parse_shortcut`, `handle_keys`, Shortcuts window | verified parser | parse/round-trip tests; `docs/audit/shortcuts.png` |
| Settings/Inspector windows (captures checked) | `apps/desktop` | implemented | `docs/audit/settings.png`, `docs/audit/inspector.png`; interactions manual |
| Songs & tutorial window | `ShellApp::replace_project`, named demos | verified model | named songs validate; UI manual |
| Perform view (Now/Next, chord pads) | `apps/desktop` | implemented | capture checked (`docs/audit/perform.png`); pad audition manual |
| UI shell (Perform/Arrange/Mix) with dark/light palettes | `apps/desktop` | implemented | palette test; refreshed light screenshot `docs/audit/light-theme.png`; audio status accurate |
| Audio playback | `audio` | implemented | cpal; audible check pending |
| WAV/offline render | `audio` | verified | 5.36 s real-style render |
| Device profile voice matching | `midi-devices::profiles` | partial | PSR-S970 subset + keyword fallback; EP/Rhodes mapping; 3 tests |
| Named style parts in recorded tracks | `performance::part_label` | verified | style part names, else drums/bass/chords/melody/pad heuristics |
| Per-track keyboard program (schema v4) | `project`, `app-commands`, `mcp` | verified | migration, command, MCP tool; export honors it |
| Mute / solo / arm (schema v5) | `project`, `app-commands`, `sequencer`, `audio` | verified | mix rules tested; render/sequencer honor them; M/S in the track list |
| Track volume and pan (schema v8) | `project`, `audio`, `smf` export | verified | fader/pan render tests; CC 7/10 written; defaults omitted |
| Mixer panel (tracks, buses, master, routing) | `apps/desktop`, `SetBuses` | implemented | bus/routing tests; capture checked |
| Song rename and meter edit (undoable) | `SetSongName`/`SetTimeSignature`, sidebar/Inspector | verified | rename/meter/undo and MCP tests |
| Theory and Lessons windows (captures) | `theory_summary`, `music_core::lesson`, Windows menu | verified model | summary/lesson tests; `theory.png`, `lessons.png` |
| Multi-clip selection (Shift-click, move/resize/delete) | `ShellApp::selected_clips`, piano roll | verified helper | selection unit test; UI manual |
| Clip-grid headers and slot context menu | `apps/desktop`, `arranging::grid` | implemented | capture checked; slot methods tested |
| Snap toggle (drags and insertions) | `RollState.snap`, `drag_result`, Arrange | verified | snapped/free drag test; UI manual |
| Marker editor (add/edit/remove/Go, ruler lines) | `SetMarkers`, Markers window, ruler | verified model | add/replace/remove/undo tests; UI manual |
| Track Inspector, live ruler seek and window openers | `apps/desktop`, `audio::player` | implemented | seek unit test; tempo tests; UI manual |
| Buses and sends (schema v13) | `project::song::Bus`, `audio` render, `app-commands` | verified | route/send/pan render tests; command undo; JSON round trip |
| Audio references, fades/stretch, waveform zoom, import, capture, float WAV | `audio`, Editor import, CLI and sidebar | verified helpers | fade/stretch/window/mix/import tests; input hardware/permission unverified |
| Lyric lane, editor and Go navigation | `piano_roll` lane, Editor lyrics, Lyrics window | verified model | add/replace/remove/undo tests; `docs/audit/lyrics.png` |
| Per-track synth patch (schema v14, waveform + ADSR + brightness) | `project::song::Patch`, `audio` render, `app-commands` | verified | square-vs-sine, slow-attack, command and round-trip tests |
| SysEx parameter-change builder, checksum and XG System On dump | `midi-devices::sysex`, CLI `sysex-xg-on` | verified | checksum/end-byte tests; 10-byte dump written |
| Plugin slots (schema v15), discovery and Mixer browser (no hosting) | `project::song::PluginSlot`, `plugins`, CLI/MCP, Mixer panel | verified discovery/state | slot round trip; synthetic scan; Editor add/toggle/remove test; 21 real bundles |
| L6max take scan/import (CLI and shell) | `audio::l6max`, CLI and shell | verified | scan test; two synthetic takes imported at computed lengths |
| Volume and pan automation lanes (schema v9) | `project`, `audio`, `app-commands` | verified | interpolation/hold, per-note render, command undo |
| Automation lane UI (click to add/remove volume points) | `apps/desktop/src/automation_view.rs` | implemented | mapping tests; UI manual |
| Beat view (4 rows × 16 steps, velocity cycle, apply/undo) | `arranging::beat`, `apps/desktop` | verified core | 4 tests; B opens the grid |
| Clip grid model (scenes × tracks, slots) | `arranging::grid`, MCP `get_clip_grid` | verified core | 3 tests + tool |
| Scene launcher (bar-quantised, waiting/cancel, log) | `arranging::grid::Launcher` | verified core | 2 tests |
| Record played scenes to the timeline | `arranging::grid::record_to_timeline` | verified core | notes and clips appended, undoable |
| Loops view: scene rows, slots, launch/queue, record take | `apps/desktop` | implemented | UI manual; launcher/recorder tested |
| Scene names (schema v6) and rename | `project`, `app-commands`, `apps/desktop` | verified | migration round trip, command undo, rename test |
| Slot editing: set length, clear, copy to a tick | `arranging::grid`, MCP `edit_slot`, right-click Loops menu | verified | 4 grid tests + tool + editor wrapper test |
| Device profiles: YAML loader, banks, drum kits, other devices, editors, mixer maps | `midi-devices::profiles` | partial | loader/banks/kits tested; no other devices, editors or mixer maps |
| AI/MCP | `mcp`, `assistant` | partial | local assistant + 93 tools; no model, network or keys |
| Korg Pa import | `arranger` PCG scanner | partial | recognition only; no program/combi import |
| Plugin hosting (VST3/CLAP) | — | missing | ADR-007, deferred |

This table is the working checklist: each row moves only with evidence, never
by assertion.
