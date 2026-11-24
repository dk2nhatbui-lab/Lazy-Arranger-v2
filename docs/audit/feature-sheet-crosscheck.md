# Crosscheck: C++ feature sheet vs the Rust build

Superseded status, corrected 2026-11-24 at round 462. The earlier version of
this file described the round-169 build and was **stale**: it still said score,
MusicXML, audio tracks, buses, automation, MCP, plug-ins, Kids Mode, light theme
and several hardware tools were missing. Those claims are corrected below.

The detailed 39-row audit is
[`cpp-features-crosscheck.md`](cpp-features-crosscheck.md). **Parity is not
claimed for any row.** Rows marked verified cover the Rust subset and its tests;
hardware, imported proprietary formats and native plug-in editors remain
partial.

Sources retained from the earlier crosscheck:

* `docs/Arranger-MIDI-DAW-Feature-Sheet.docx` — the product sheet.
* `docs/intelligent-feature-ideas-2026-11-17.md` — established C++ features.
* `docs/music-finder.md`, `docs/playlists.md` — format notes.

`docs/features.md` is still not on origin; the C++ release is `94c093b`
("Release v1.0.0-beta.1").

## 01 Styles and keyboard export

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| Open Yamaha `.sty` / `.prs` | verified | SFF1/SFF2 parse, 16,219 containers and 856,370 rules in the library scan |
| Save Yamaha `.sty` / `.prs` | partial | Native SFF1/SFF2 writer rewrites a real 15-section style and writes clip-derived sections; Korg, XG voices and the SInt setup region are absent |
| Open Korg SFF styles | missing | Korg recognition only; no program/combi decode |
| Fill tracks with parts that follow chords | verified | `arranger::Style::compile`, style import and next-bar section/chord queues |
| Export the song for PSR-S970 with voices and lyrics | verified | Keyboard export with setup bar, program, volume, pan, chord markers, lyrics and drums on channel 10; byte-level reparse test |

## 02 Style from Audio and Style from Link

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| Style from a recording or a link | partial | Style from audio builds a native style from monophonic audio; Style from link downloads via yt-dlp and opens the same builder; no model-based separation |
| Finds stems, notes, chords and lyrics | partial | Monophonic notes and a chroma chord recognizer exist; no stem separation or lyrics model |
| Asks A or B when unsure | missing | No candidate model or transcription questions |
| Match percentage per section | missing | No style-match score |
| Save style, Open in Loops | partial | Save SFF1/SFF2 and audition; loops integration is not automatic |

## 03 Arranging help and 04 Smart chords

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| Arrangement Doctor | verified | Crowding/repeat proposals with Apply and before/after audition |
| Fit to My Synths | verified | Range, polyphony and drum-kit proposals with A/B audition |
| Groove copies a feel | verified | Extract/apply timing and accents, feel score and `.groove` files |
| Song Forms (short/gig versions) | verified | Marker sections, presets, plan/apply, hold/break transitions and join loops |
| Type `Am F C G` or `vi IV I V` | verified | Chord parser, progression text and roman numerals |
| Next-chord ideas, 16 pads, Launchkey pads | verified | Chord lane, 16 key-aware pads, suggestion function, Launchkey pad and mixer presets |
| Record played chords, reharmonize a bar | verified | Held-MIDI chord capture, chord write at playhead and reharmonize rules |
| Full band arrangement generator | partial | Rule-based bass/pad/arp/melody/continue/drumify plus humanise; no learned model |

## 05 Score and notation

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| Full scores, grand staves, drum staves | partial | One track at a time with grand staff and a drum staff; all-track full score is absent |
| Chord symbols, guitar frames, lyrics, dynamics | partial | Chords, common guitar frames, lyrics, marks, slurs and ties draw; notation is one track and collision handling is limited |
| Mouse / MIDI entry | verified | Click at staff pitch, A–J keyboard row, octave/length/dot/rest, entry cursor and MIDI chord entry; physical score MIDI entry is not separately tested |
| Print, PDF, MusicXML export | verified | Vector PDF, `lp` print dialog, MusicXML export/import round-trip; Cecilia 0.51 engraver pages |

## 06 Synths, controllers and views

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| Inputs turn on by themselves, device profiles | verified | YAML profiles, auto-open inputs, port-name profile selection and CC panels |
| D-05 and XioSynth sound editors | partial | Profiles and CC editors exist; no full patch editor or program dump decode |
| Launchkey / X-Touch mixer pages | partial | Launchkey CC 21–28 mixer preset, pad preset and physical CC capture; no full controller-page editor |
| Zoom LiveTrak L6max import | partial | Take-folder import, master/mix mute, A–D scenes, compact/reset, and an 8-channel device page for EQ, sub, AUX, EFX, pan, level, mute, MONO, USB, effect type, compressor and scenes. Hardware mute verified; device project/readback/transport absent |
| Timeline · Score · Beat · Clips · Mixer · Sound Editor | verified | Timeline, Score, Drums/Beat, Loops/clip grid, Mixer, Patch/SoundFont and plug-in windows exist |

## Established C++ features named in the ideas document

| C++ feature | Rust | Gap |
| --- | --- | --- |
| AI conversation about the song | partial | Local assistant, OpenAI-compatible endpoint, local CLI, one tool call and Apply/Cancel cards; no subscription sign-in or multi-turn tool loop |
| Melody continuation, part generation | verified | Seeded rule-based generators |
| Humanization (timing/velocity) | verified | `generate` humanise command |
| Audio analysis and chord recognition | partial | Monophonic transcription, chroma chords, tags and onset detection; no trained polyphonic model |
| Voice-leading checks | verified | Cost function, CLI and Theory window |
| Chord-following styles | verified | `arranger::Style::compile` |
| Clip scenes | verified | Clip grid, scenes, slots, launcher and recording |
| Check the Transcription (A/B questions) | missing | No candidate model |
| Music Finder AI tools | partial | JSON/CSV/MFD import, search and style import; no AI row suggestions |
| MFD layout reader | verified | Real 1,653-record MFD imported |
| Yamaha playlist AI tools | partial | TSV/CSV creation and listing; rows are not opened |

## AI, files, plug-ins, platforms

| Sheet claim | Rust | Evidence / gap |
| --- | --- | --- |
| AI assist: plain-word changes, one undo step | partial | MCP command layer and assistant proposals; external endpoint unverified |
| Use a Claude/ChatGPT/Gemini subscription or API key | partial | API key path exists; subscription sign-in absent |
| User skill files read as data | missing | — |
| Voice requests via Whisper; spoken replies | partial | Whisper command template and custom TTS command; no bundled model |
| MIDI + audio tracks, drum patterns, slicer, clip grid, mixer buses, automation | verified | Audio refs render with fades/stretch/automation, drum slicer, clip grid, buses, sends and mixer |
| VST3 and Audio Unit plug-ins | partial | Effect and instrument hosting, parameters and automation; no native editors or sandbox scan |
| Files: MIDI, MusicXML, `.sty`, `.arranger` | verified | All round-trip; video audio through FFmpeg |
| Hardware: Yamaha/Korg arrangers, synths, controllers, mixers | partial | MIDI sends and physical input on Launchkey, PSR, XioSynth and X-TOUCH; L6max/PSR audible/control response unverified; D-05 has no keyboard |
| Kids Mode | verified | Full-window Perform, big pads, auto-pause, loop and reduced motion |
| Light theme | verified | Light Studio Paper palette |
| Windows, macOS, Linux | verified builds | CI green on all three platforms; runtime manual |

## What the sheet does not mention but the Rust build has

Dual intent/rendered capture with provenance-aware regeneration, offline WAV
export, cpal playback, autosave/recovery, Music Finder JSON/CSV/MFD and Yamaha
playlist TSV support, audio takes with comp spans, a stored audio-take list,
MIDI learn, computer-keyboard score entry and a feature-parity table with
evidence.

## Remaining order of work

1. Full-score all-track layout and richer notation collisions.
2. Native AU/VST3 editor windows.
3. Korg PCG program/combi decoding.
4. L6max device project, EQ, MONO, USB, effects and transport.
5. AI subscription sign-in, user skill files and model-backed transcription.
6. Hardware acceptance for audible PSR/L6max output, printing and D-05/L6max control.

Hardware results are in [`hardware.md`](hardware.md). The full test suite is 774
tests at this correction; see `final-status.md`.
