# Crosscheck: C++ feature sheet vs the Rust build

Sources, after fetching origin on 2026-11-22:

* `docs/Arranger-MIDI-DAW-Feature-Sheet.docx` — the product sheet.
* `docs/intelligent-feature-ideas-2026-11-17.md` — established features and the
  Arrangement Doctor / Fit to My Synths / Check the Transcription / Groove /
  Song Forms work.
* `docs/music-finder.md` and `docs/playlists.md` — formats and AI tools.

`docs/features.md` is **not on origin**. The fresh clone of `main` is at
`94c093b` ("Release v1.0.0-beta.1"), and `find -iname '*feature*'` returns only
the sheet, the market research and the ideas document; no branch history has
`docs/features.md`. If it lives on another remote, branch or a local commit, it
has not been pushed yet.

Status: **verified** (tested here), **partial**, **missing**.

## 01 Styles and keyboard export

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| Open Yamaha `.sty` / `.prs` | verified | 16,219 containers, 856,370 rules |
| Save Yamaha `.sty` / `.prs` | partial | `write_casm_sff1` round-trips rules; no MIDI track, voices or full container |
| Open Korg SFF styles | missing | Stage 8 extension |
| Fill tracks with parts that follow chords | verified | `arranger::Style::compile`, style import |
| Export the song for PSR-S970 with voices and lyrics | missing | plain SMF export exists; no keyboard export |

## 02 Style from Audio

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| Style from a recording or a link | partial | link → monophonic MIDI implemented; no style output |
| Finds stems, notes, chords and lyrics | partial | monophonic notes only; stems/chords/lyrics need external models |
| Asks A or B when unsure | missing | no candidates model |

## 03 Arranging help and 04 Smart chords

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| Arrangement Doctor | missing | no analysis of flat choruses or crowding |
| Fit to My Synths | missing | no voice/range checking |
| Groove copies a feel | missing | no groove extraction |
| Song Forms (short/gig versions) | missing | no form model |
| Type `Am F C G` or `vi IV I V` | partial | chord parsing and roman numerals in `music-core`; no progression editor |
| Next-chord ideas, 16 pads, Launchkey pads | missing | no pad UI or hardware mapping |
| Record played chords, reharmonize a bar | missing | — |

## 05 Score and notation

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| Full scores, grand staves, drum staves | missing | ADR-010 design only |
| Chord symbols, guitar chords, lyrics, dynamics | missing | — |
| Mouse / MIDI entry | missing | — |
| Print, PDF, MusicXML export | missing | — |

## 06 Synths, controllers and views

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| Inputs turn on by themselves, device profiles | partial | one PSR-S970 voice subset with keyword matching; no YAML profiles, banks or auto-enable |
| D-05 and XioSynth sound editors | missing | — |
| Launchkey / X-Touch mixer pages | missing | — |
| Zoom LiveTrak L6max import | missing | not even in the profile list |
| Timeline · Score · Beat · Clips · Mixer · Sound Editor | partial | Arrange (timeline/piano roll) only; others are placeholders |

## Established C++ features named in the ideas document

These are described as already present in the C++ app, beyond the sheet.

| C++ feature | Rust | Gap |
| --- | --- | --- |
| AI conversation about the song | missing | MCP/assistant not built |
| Melody continuation, part generation | missing | no generative helpers |
| Humanization (timing/velocity) | missing | exact timing only |
| Audio analysis and chord recognition | partial | monophonic pitch only; no chords |
| Voice-leading checks | missing | — |
| Chord-following styles | verified | `arranger::Style::compile` |
| Clip scenes | missing | no clip model |
| Check the Transcription (A/B questions) | missing | no candidate model |
| Music Finder AI tools | missing | books/CSV only |
| MFD layout reader | missing | documented in `docs/music-finder.md`; not implemented |
| Yamaha playlist AI tools | missing | TSV only |

## AI, files, plugins, platforms

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| AI assist: plain-word changes, one undo step | missing | MCP server not built; the command path already gives one undo step |
| Use a Claude/ChatGPT/Gemini subscription or API key | missing | — |
| User skill files read as data | missing | — |
| Voice requests via Whisper; spoken replies | missing | — |
| MIDI + audio tracks, drum patterns, slicer, clip grid, mixer buses, automation | partial | MIDI tracks only; no audio tracks, drums, slicer, buses, automation |
| VST3 and Audio Unit plugins | missing | ADR-007 deferred |
| Files: MIDI, MusicXML, `.sty`, `.arranger` | partial | all but MusicXML; `.sty` read and partially written |
| Hardware: Yamaha/Korg arrangers, synths, controllers, mixers | partial | Yamaha PSR-S970 profile; no Korg, no L6max |
| Kids Mode | missing | — |
| Light theme | missing | dark theme verified; a light theme is new work |
| Windows, macOS, Linux | verified builds | CI green on all three; runtime manual |

## What the sheet does not mention but the Rust build has

Dual intent/rendered capture with provenance-aware regeneration, offline WAV
export, cpal playback, autosave/recovery, Music Finder JSON/CSV and Yamaha
playlist TSV support, and a feature-parity table with evidence.

## Order of work this crosscheck implies

1. Finish the style writer (MIDI track + SFF2) so "save Yamaha styles" is true.
2. Score/notation (ADR-010): the single largest missing user-facing block.
3. Smart chords pads, then Arrangement Doctor/Groove/Song Forms.
4. Korg SFF import, MusicXML, PSR-S970 export.
5. AI: MCP tool server first (testable without a model), then subscriptions and
   voice; model-heavy audio features behind optional dependencies.
6. Mixer pages, sound editors, L6max, Kids Mode, light theme.
