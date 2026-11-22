# Crosscheck: C++ feature sheet vs the Rust build

Source: `arranger-midi-daw/docs/Arranger-MIDI-DAW-Feature-Sheet.docx` (the
`docs/features.md` you mentioned is not in the local C++ clone; that checkout is
on `fix/audit-critical-high` and carries only the DOCX/PDF sheet). Every row
below is taken from the sheet's own wording.

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
| Inputs turn on by themselves, device profiles | partial | profiles typed and tested; no auto-enable |
| D-05 and XioSynth sound editors | missing | — |
| Launchkey / X-Touch mixer pages | missing | — |
| Zoom LiveTrak L6max import | missing | not even in the profile list |
| Timeline · Score · Beat · Clips · Mixer · Sound Editor | partial | Arrange (timeline/piano roll) only; others are placeholders |

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
