# Crosscheck: `features.md` (v1.0.0-beta.1) vs the Rust build

Source: the 39-section feature list supplied by the owner, archived verbatim at
[`docs/reference/cpp-features.md`](../reference/cpp-features.md) (v1.0.0-beta.1, 512 lines). Every
section below is a row, so nothing is skipped. Status: **verified**,
**partial**, **missing**.

| § | C++ feature | Rust | Key gap / evidence |
| --- | --- | --- | --- |
| 1 | Projects and files | partial | `.arranger` open/save, autosave/recovery, SMF in/out, WAV out, MusicXML export/import, style save, PSR-S970 song export. No Recent, audio import, PDF, Korg, L6max, SysEx |
| 2 | Window: header, views, inspector, mixer panel, AI panel, light theme, zoom | partial | Header, LCD, tabs, track list, dark theme, square corners. No inspector, mixer/AI panels, light theme; zoom is wheel-only |
| 3 | Tracks | partial | MIDI tracks with an optional keyboard program (schema v4, shown in the track list, MCP tool, honored by keyboard export). No audio tracks, per-track input, outputs or SoundFont; mute/solo/arm flags exist and sequencer/audio honor mute and solo |
| 4 | Transport: play/stop, start, record, loop, metronome, ruler, tempo/meter edit, MIDI record, all-notes-off, auto inputs | partial | Audio play/pause/stop with silence on stop; no record, loop, metronome, ruler, tempo edit, quantise, auto inputs |
| 5 | Timeline clips | partial | Clip regions in schema v3, migration, and undoable insert/move/resize/split/remove commands with overlap rejection. No UI, join, snap, chord lane or lyrics |
| 6 | Note editor: add/select/move/resize/delete, arrows, velocity lane, grid, quantise, piano keys, select all | partial | Click insert, select, delete, undo/redo, save. No drag-move, resize, velocity lane, quantise, piano keys, copy/paste |
| 7 | Cut/copy/paste in every view | missing | — |
| 8 | Audio tracks: waveforms, record, monitor, trim, fades, takes, stretch, convert to MIDI | missing | — (only offline WAV render) |
| 9 | Audio analysis: stems, notes, chords, instruments, lyrics, transcription A/B | partial | Monophonic pitch detection and audio→MIDI; no stems, chords, tags, lyrics, A/B |
| 10 | Drum sampler and slicer | missing | — |
| 11 | Beat view | partial | Four drum rows by sixteen steps with off/soft/normal/loud cycling, read from and written to a bar as one undo step, B opens the grid. No pattern length, copy/paste rows, Put-in-bars or scrolling long clips |
| 12 | Clip grid, launch, record performance | missing | — |
| 13 | Smart chords | partial | Chord parsing, transposition, progression text, roman numerals in the chosen key, chord lane with K entry, 16 key-aware pads, and a suggestion function with dialog buttons. Reharmonize rules and dialog. Find Chords from the Music (pitch-class recognition, slash bass). No A/B audition, Launchkey pads, inversions/voicings, chord recording |
| 14 | Styles | partial | Yamaha open, import, compile on chords, **save container (SFF1/SFF2, real 15-section style rewritten)**. No Korg, live sections, rules editor UI, style from clips, voices/setup events |
| 15 | Style from Audio and Link | partial | Link→monophonic MIDI; no style output, no match scores |
| 16 | Score view | partial | MusicXML export with parts, measures, rests, sharps and drum clef; no engraving view, PDF, marks or editing |
| 17 | Mixer and buses | missing | — |
| 18 | Automation | missing | — |
| 19 | VST3/AU plugins | missing | ADR-007 deferred |
| 20 | SF2/SF3 SoundFont player, metronome | partial | Additive sine voice; no SoundFont |
| 21 | MIDI devices and profiles | partial | One built-in PSR-S970 voice subset with keyword matching and a GM fallback (used by keyboard export). No YAML loader, banks, drum kits, other devices, overrides, validation or recent-MIDI buffer |
| 22 | Synth sound editors (D-05, XioSynth, S970 XG, L6max), sysex dumps | missing | — |
| 23 | Controller pages, MIDI Map, MIDI Learn, channel check | partial | Launchkey/X-Touch profiles referenced; no pages, map, learn |
| 24 | PSR-S970 tools | partial | Song export with setup bar, program/volume/pan per part, chord markers, lyrics events and drums on channel 10. No Style Remote, style-by-number or hardware verification |
| 25 | Music Finder and Playlists | partial | JSON, CSV, TSV and MFD (1,653-record real file), CLI import. No AI tools or CSV→playlist writer |
| 26 | Zoom L6max mixer page and import | missing | — |
| 27 | AI chat with subscriptions/keys, one-undo-step changes, proposal cards | missing | Commands already provide single undo steps |
| 28 | MCP server, ~90 tools | partial | Tool registry with schemas and JSON dispatch for 6 tools (song summary, add track/notes, set chords, undo), CLI `mcp-tools`/`mcp-call`. Stdio JSON-RPC transport and 12 tools (song summary, add track/notes, set chords, undo, find chords, doctor, extract/apply groove, song forms, reharmonize), saving after each call. No socket transport, per-start secret or most tools |
| 29 | Speech in/out (Whisper, Kitten, Kokoro) | missing | — |
| 30 | AI music tools (humanize, drumify, continue, write part, melody) | missing | — |
| 31 | Theory: keys, roman numerals, scales, voice-leading, skill packs | partial | Chord types, transpose, chord parse; no key finding, roman analysis, voice-leading, skills |
| 32 | Arranging help | partial | Arrangement Doctor (crowded bars, repeats) and Fit to My Synths (range octaves, polyphony removal, drum-kit warning) with proposal cards and undo. No A/B audition |
| 33 | Groove | partial | Timing offset and velocity level per 1/16 slot, extract/apply with timing and accent amounts, feel-match score, G applies the selected track's groove to the others. No audio groove, words, library files or Play Before/After |
| 34 | Song Forms | partial | Sections from markers, Short/Gig/Long-intro/Instrumental presets, plan and apply to timeline with chords and markers, undo. Drum-fill and lead-in transitions with automatic choice, applied as one undo step (T). No hold/break transitions, join loops or learned preferences |
| 35 | Kids Mode | missing | — |
| 36 | Languages (EN/DA/FR/VI) | missing | English only |
| 37 | Settings, shortcut editor, updates | missing | — |
| 38 | Performance and safety: native, idle CPU, plugin scan, damaged-file tolerance | partial | Native Rust, audio thread separate, damaged styles counted; idle CPU and plugin scan unmeasured/absent |
| 39 | Demo content (2 songs + 540 styles) | partial | CLI demo and a four-bar shell demo; no named songs or style pack |

## Scorecard

39 sections: 0 fully verified beyond their Rust subset, 20 partial, 19 missing.
The partials with the most depth are styles (open, import, compile, SFF1 write),
audio-to-MIDI, Music Finder/playlists, project files and the window shell.

## Next blocks, in dependency order

1. Finish the style writer (MIDI track, SFF2) and add `Save .sty`.
2. Clip model (Timeline, Clip grid shared) — unblocks §5, §7, §12, §14 editing.
3. Score/notation per ADR-010 (§16, PDF).
4. Smart chords lane/pads and reharmonize (§13) on the existing chord model.
5. Arrangement Doctor and Fit to My Synths (§32), then Groove and Song Forms.
6. MCP tool server (testable without models), then AI chat/speech/tools.
7. Audio tracks, mixer/buses, automation, plugins; Korg, MusicXML, PSR export,
   L6max, synth editors, Kids Mode, languages.
