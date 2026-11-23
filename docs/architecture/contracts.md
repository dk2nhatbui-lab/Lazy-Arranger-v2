# Crosscheck: `features.md` (v1.0.0-beta.1) vs the Rust build

Source: the 39-section feature list supplied by the owner, archived verbatim at
[`docs/reference/cpp-features.md`](../reference/cpp-features.md) (v1.0.0-beta.1, 512 lines). Every
section below is a row, so nothing is skipped. Status: **verified**,
**partial**, **missing**.

| § | C++ feature | Rust | Key gap / evidence |
| --- | --- | --- | --- |
| 1 | Projects and files | partial | `.arranger` open/save, autosave/recovery, SMF in/out, WAV export in 16 and 24 bit, MusicXML export/import, style save, PSR-S970 song export. Recent list and audio-reference import (WAV). No editing of audio, PDF, Korg, L6max, SysEx |
| 2 | Window: header, views, inspector, mixer panel, AI panel, light theme, zoom | partial | Header, LCD, five views, track list, Mix strips, volume automation lane, dark theme, square corners, wheel and slider zoom. No inspector, AI panel or light theme |
| 3 | Tracks | partial | MIDI tracks with program, volume, pan, mute/solo/arm and audio references (schema v11); track list, mixer strips and keyboard export honour them; tracks carry SoundFont and audio references. No per-track input routing or multiple outputs |
| 4 | Transport: play/stop, start, record, loop, metronome, ruler, tempo/meter edit, MIDI record, all-notes-off, auto inputs | partial | Audio play/pause/stop with silence on stop; CLI MIDI record/echo and clip-grid take recording. No transport record arm, loop, metronome, ruler, tempo edit, quantise or auto inputs |
| 5 | Timeline clips | partial | Clip and chord lanes: click select, arrow move, shift+arrow resize, J join, N add, Delete remove; clip commands are undoable with overlap rejection. No snap, timeline lyrics or multi-clip editing |
| 6 | Note editor: add/select/move/resize/delete, arrows, velocity lane, grid, quantise, piano keys, select all | partial | Click insert, select, delete, undo/redo, save, scroll/zoom. No drag-move, resize, velocity lane, quantise, piano keys, select all or copy/paste |
| 7 | Cut/copy/paste in every view | partial | Notes copy, cut and paste in Arrange with Cmd/Ctrl+C/X/V at the playhead, each paste one undo step; a chord range can be copied and pasted through the same clipboard. No clipboard in other views, pasting at the mouse position or multi-loop paste |
| 8 | Audio tracks | partial | Audio references (schema v12) with WAV import at 16/24/32-bit PCM and 32-bit float, an in-point, waveform peaks drawn in Arrange, trim-from-front through one undoable command, tempo-derived length, and mixing into render and cpal playback with mute/solo, resampling and reported files. A bounce command renders the mix, writes a WAV and inserts it as an audio track (one undoable insert). No input recording, monitoring, fades, takes, stretching or waveform zoom/edit |
| 9 | Audio analysis | partial | Monophonic audio→MIDI, chroma-based chord recognition with a CLI, transcription answers still missing. No stems, instrument tags, lyrics or model-based polyphony |
| 10 | Drum sampler and slicer | partial | Onset detection, slicing, GM pad mapping, `drum-slice`, and a P-key Drum pads window that loads a loop, grids 16 pads with names and lengths, and auditions the clicked pad through cpal. No keyboard-triggered pads, slice editing or time-stretch |
| 11 | Beat view | partial | Four drum rows by sixteen steps with off/soft/normal/loud cycling, read from and written to a bar as one undo step, B opens the grid. No pattern length, copy/paste rows, Put-in-bars or scrolling long clips |
| 12 | Clip grid | partial | Scenes of N bars, slots, a bar-quantised launcher with waiting/cancel/log, scene recording to the timeline as undoable edits, `get_clip_grid`. Loops view shows scene rows and track slots, queues on click and records a take. Scene names persist and can be renamed; slot length, clear and copy-to-tick work and are exposed as `edit_slot`. No audio loops, slot UI controls or Launchkey mapping |
| 13 | Smart chords | partial | Chord parsing, transposition, progression text, roman numerals in the chosen key, chord lane with K entry, 16 key-aware pads, and a suggestion function with dialog buttons. Reharmonize rules and dialog. Find Chords from the Music (pitch-class recognition, slash bass). No A/B audition, Launchkey pads, inversions/voicings, chord recording |
| 14 | Styles | partial | Yamaha open, import, compile on chords, **save container (SFF1/SFF2, real 15-section style rewritten)**. No Korg, live sections, rules editor UI, style from clips, voices/setup events. Catalog resolution by number or name (`style-resolve`) matched a real library entry |
| 15 | Style from Audio and Link | partial | Link→monophonic MIDI; no style output, no match scores |
| 16 | Score view | partial | MusicXML export/import with `<harmony>` chords, drum instrument ids and `<sound dynamics>`, and a round-trip audit. On the demo tracks, chords (16), lyrics (2), marks (2), drums and the pitched detail are preserved (186 = 186 after merging 22 ties). A style-derived project with mixed drum/pitched tracks and off-grid overlapping notes keeps 1,403 of 1,423 pitched notes with a 10-tick shift on one long note after per-note drum export was fixed; MusicXML's measured model cannot hold those overlaps exactly, which is recorded as a limitation plus a toolkit-independent layout engine (pages, systems, measures, positioned notes, CLI report). An egui page canvas renders staff lines, barlines, noteheads, page navigation and zoom from the layout. Key detection, spelling, accidentals, canvas, vector PDF, per-pitch ties and a lyric line (40-bar two-page acceptance test). Dynamics and articulations are marks in schema v11, exchanged as MusicXML directions and drawn by both renderers. Note entry: with Note entry enabled, a click on the score inserts a note at a quarter-quantised tick and the staff pitch through one undoable command. No print dialog, grand/drum staves, beams, collision avoidance, tuplets, guitar frames or rests/move/resize in the score |
| 17 | Mixer and buses | partial | Tracks carry fader, pan, mute/solo and automation; buses (schema v13) have volume and pan, tracks route to a bus and can send a copy at a level, and the render sums bus buffers after the tracks. Mix view shows bus faders and a per-track routing picker. No reverb/chorus processing, meters, master effects or send-level UI |
| 18 | Automation | partial | Volume and pan lanes with linear interpolation and held ends, applied per note in the render, edited through one undoable command. Arrange shows a volume lane for the selected track; clicking adds a point, clicking a point removes it, and each edit is one command. No pan lane UI, plugin-parameter lanes or MIDI-port automation |
| 19 | VST3/AU plugins | partial | Project model only: `Track.plugins` slots (id, name, format VST3/AU, path, bypass, opaque state) in schema v15, one undoable `SetPlugins` command and duplicate/no-name validation. No plugin hosting, scanning, audio processing or editor windows; the render ignores slots |
| 20 | SF2/SF3 SoundFont player, metronome | partial | Built-in synth now has piano, bass, organ, strings, pad and noise drums chosen from channel, program and name, with per-family attack and decay. No SoundFont loading, sampler or metronome click |
| 21 | MIDI devices and profiles | partial | One built-in PSR-S970 voice subset with keyword matching and GM fallback; port enumeration, live input, CLI record/echo. No YAML loader, banks, drum kits, other devices, overrides or recent-MIDI buffer |
| 22 | Synth sound editors (D-05, XioSynth, S970 XG, L6max), sysex dumps | partial | A per-track synth patch (sine/saw/square/triangle, attack, decay, sustain, release, brightness) in schema v14, honoured by the render and edited through one undoable command. No editor UI, device-specific panels or sysex dumps |
| 23 | Controller pages, MIDI Map, MIDI Learn, channel check | partial | MIDI map model with text persistence, learn capture, CLI bind/show, and live application of play/stop/undo/scene/fader. No controller pages, on-screen learn dialog or channel check |
| 24 | PSR-S993 tools | partial | Song export with setup bar, program, volume, pan, chord markers, lyrics and drums on channel 10, plus a byte-level audit that re-parses the file; the demo exported 5 tracks, 4 programs/volumes/pans, 16 chord markers and 128 drum notes. Numbered style catalog and `style-by-number` rendering. No remote panel, audition queue or hardware verification |
| 25 | Music Finder and Playlists | partial | JSON, CSV, TSV and MFD import (1,653-record real file), CLI, and an L-key Library window that loads any of those, searches title/detail, applies an entry's tempo and imports the row's style through the catalog (setting or ARRANGER_STYLE_LIBRARY), inserting its tracks with fresh ids and a prefix. Playlist rows are listed but not opened; no AI tools or CSV→playlist writer. A real import was run through `library-import`: 1 track and 1,926 notes from ABBA Fernando; the ignored integration test passes when ARRANGER_STYLE_LIBRARY is set |
| 26 | Zoom L6max mixer page and import | partial | `audio::l6max` scans a folder of takes, reads headers, flags the L6max's 32-bit float recordings, reports problems and imports each take as an audio track at a start tick; `l6max-import` did so for two synthetic takes (3.20 s, one float). No device project-file parsing, mixer page or transport control |
| 27 | AI chat with subscriptions/keys, one-undo-step changes, proposal cards | partial | A local assistant panel and command layer (`mcp::assistant`) parses help/song/undo and `tool <name> {json}`, reaches any of the current 93 tools through `tool`, logs replies and marks the project dirty only for changing calls; each tool call is one undo step. No model, network, subscriptions or keys, no proposal cards |
| 28 | MCP server, ~93 tools | partial | Stdio JSON-RPC server with 26 tools (song, tracks, notes, chords, undo, doctor, grooves, forms, reharmonise, clip grid, slots, programs, automation, lyrics/marks, style catalog, theory, generation, plugins, L6max scan) and save-after-call; `mcp-tools`/`mcp-call`. Loopback TCP transport with a per-start 32-hex token required on every line and save-after-call; a real client was accepted with the token and rejected without it. No TLS, multi-client concurrency or most of the ~90 target tools |
| 29 | Speech in/out (Whisper, Kitten, Kokoro) | partial | Assistant replies can be spoken through the macOS `say` command (cleaned, length-bounded, optional voice) behind a settings toggle; other hosts fall back silently. No speech input, Whisper/Kitten/Kokoro models or waveform narration |
| 30 | AI music tools (humanize, drumify, continue, write part, melody) | partial | Rule-based, seeded generators: humanise, drumify, bass/pad/arp parts, continue the last bars and a chord-following melody, each one undoable edit. Exposed by the `generate` CLI and MCP tool. No learned or model-based generation, UI or chat-driven tool use |
| 31 | Theory: keys, roman numerals, scales, voice-leading, skill packs | partial | Nine scale modes, roman labels, cadence classification and voice-leading cost, as CLI and MCP tools; key detection in the score. No skill packs, graded lessons or theory panel |
| 32 | Arranging help | partial | Arrangement Doctor (crowded bars, repeats) and Fit to My Synths (range octaves, polyphony removal, drum-kit warning) with proposal cards and undo. No A/B audition |
| 33 | Groove | partial | Timing offset and velocity level per 1/16 slot, extract/apply with timing and accent amounts, feel-match score, G applies the selected track's groove to the others. No audio groove, words, library files or Play Before/After |
| 34 | Song Forms | partial | Sections from markers, Short/Gig/Long-intro/Instrumental presets, plan and apply to timeline with chords and markers, undo. Drum-fill and lead-in transitions with automatic choice, applied as one undo step (T). No hold/break transitions, join loops or learned preferences |
| 35 | Kids Mode | partial | Settings switch restricts tabs to Perform and Arrange, forces a simple view, enlarges header controls and shows a translated banner. No kid-specific songs, lessons or simplified editor |
| 36 | Languages (EN/DA/FR/VI) | partial | English and Danish string tables with a runtime switch and a completeness test. French and Vietnamese missing, as are plural rules and localized help |
| 37 | Settings, shortcut editor, updates | partial | Settings file, settings window, shortcut list, MIDI port picker, and an update answer that states no channel is configured. No updater, editable shortcut bindings or telemetry controls |
| 38 | Performance and safety: native, idle CPU, plugin scan, damaged-file tolerance | partial | Native Rust, audio thread separate, damaged styles counted; idle CPU and plugin scan unmeasured/absent |
| 39 | Demo content (2 songs + 540 styles) | partial | Sixteen-bar built-in demo (drums, bass, piano, pad, chords, marks, lyrics) on first run and via `demo-project`; the short MIDI `demo` remains. No named song set, style pack or tutorial |

## Scorecard

39 sections at round 169: 0 fully verified beyond their Rust subset, 39 partial,
0 missing. No section is missing at row level; partials need hardware,
proprietary formats, plugin hosting or a model. See
`docs/audit/cpp-features-crosscheck.md` for row evidence.

## State at round 169

`cargo fmt`, `clippy -D warnings`, `cargo test --workspace` (605 tests) and
rustdoc pass. The deeper implemented areas are styles, project files, score
engraving/PDF/MusicXML, audio references and mix, arranging help and the 93-tool
MCP surface. Plugin hosting, SF3, Korg program/combi decoding, L6max project
parsing, audio monitoring/takes/waveform editing, light theme, localized help
and a real AI model/speech input remain absent or unverified. Parity is not
achieved.

## Next blocks, in dependency order

1. Score general collision avoidance and drag-resize; then a real print dialog.
2. Audio takes and monitoring; destructive waveform and slice editing.
3. Plugin hosting/load validation and editor windows; SF3 and modulators.
4. Korg PCG program/combi decoding; L6max project/mixer-page parsing.
5. Light theme, localized help/plural rules.
6. A real AI model/speech input; hardware acceptance runs.

### Playlist conversion and languages

`library::List::from_book` turns a Music Finder book into playlist items, using
the style name against a folder, the entry number as REGIST and the viewer flag
off; `playlist-from-csv` writes the Yamaha TSV. Strings now ship in English,
Danish, French, Vietnamese, German and Spanish: every key has text in all six tables, and the
language setting switches them at once. Plural rules and localized help do not
exist.

### Kids lesson and Perform

Kids Mode keeps two tabs and builds a `kids::Lesson` from the song's chords; the
Perform view shows the current chord large, progress, Next chord and Start
over, plus 16 big chord pads that audition through the cpal player. `Lesson::sync` rebuilds it when the chord list changes while keeping the step
when possible, and the Perform view can load `demo::kids_song` (four slow
chords). In Arrange the sidebar swaps to large Undo/Redo/Play/Stop/Settings and hides the advanced window buttons; there is no separate simplified song flow.

### Device profiles

`profiles::parse_yaml` reads a small YAML subset: `id`, `name`, and the
`voices`, `banks` and `drum_kits` lists, each item with `name` and `program`.
Unknown keys, missing names/programs, programs above 127 and profiles without a
voice are rejected with line numbers. `nearest_in_profile` scores track words
against voice names and falls back to the GM keyword guess. Banks and drum kits
are stored but export does not use them, and no device other than the built-in
PSR-S970 subset ships.

### Profile banks in export

`Voice` carries an optional `(MSB, LSB)` bank. Keyboard export writes CC 0 and
CC 32 before the program when present, and the audit records both. With
`--profile`, non-drum tracks take the profile's nearest voice and its first
bank; drum tracks take the first kit's program and bank. A run on the demo
produced msb [63, 0, 0, 0] and lsb [0, 1, 1, 1].

### SysEx

`midi_devices::sysex` builds Yamaha-style parameter changes (`F0 43 1n 4C
address data checksum F7`) with the 7-bit checksum, rejecting device numbers
above 15 or bytes above 127. `xg_on` is the only named message; `sysex-xg-on`
wrote its 10 bytes for device 2. Device-specific editor panels and patch dumps
are not implemented.

### Chord voicings

`voicing::inversions` produces close-position inversions by rotating chord tones
and lifting wrapped tones an octave; each voicing exposes ascending pitches and
a label such as `C/E (1st inversion)`. `voicing C --out-dir DIR` prints them and
writes `a.wav`/`b.wav` from the first two for an A/B comparison. Open spread,
drop voicings and keyboard-pad audition are not implemented.

### Fit from the command line

`fit-project` builds a `SynthSpec` from `--low`, `--high`, `--polyphony` and
`--no-drum-kit`, prints every fit proposal, and with `--apply --out` applies the
non-empty ones as undoable edits. On the demo with 40..80 it moved 12 bass notes.
Before/after audition is not implemented.

### Groove library

A `.groove` file is text: `name`, `grid`, comma-separated `offsets` and `levels`;
parsing rejects missing keys, unequal lists and bad numbers. `groove-library
save` extracts one bar at a 1/16 grid from a track, `list` shows names and paths,
and `apply` loads a groove by name and writes adjusted notes with timing and
accent amounts. Audio-derived grooves and Play Before/After are not implemented.

### Hold and break transitions

`Transition::Hold` plans the last chord at or before the join as a one-bar hold
on the first pitched track; `Transition::Break` adds no notes, and `break_range`
plus `apply_break` remove every note starting in the bar before the join (one
edit per affected track, undoable). `form-transition` plans or applies any kind;
on the demo hold added 3 notes and break removed 21. `join_loop` repeats a marker section in place (total copies), copying notes,
chords, lyrics and marks inside it and shifting every later note, chord, lyric,
mark and marker by the inserted length; clips fully inside the section are copied and later clips, automation points and
audio reference starts shift with the rest. `join-loop A --times 2` added 168 notes and 8 chords to the
demo. Learned preferences are not implemented.

### Loop region

The header adds Loop (on/off) and From bar (set the region to the bar under the
playhead). With Loop on, playback renders the loop window from the full mix with
`RenderedAudio::slice`, and when the playhead reaches the end the player restarts
from the loop start; `transport::wrap` covers the arithmetic. The metronome mixes short clicks on every beat (downbeats accented) into
playback and `export-audio --metronome`. Record arm and ruler editing are still
absent.

### Quantise and velocity

`Editor::quantize` rounds every note start in every track to a grid with
half-step rounding, one ReplaceTrackNotes edit per changed track; Arrange has
1/16 and 1/8 buttons. `Editor::set_velocity` sets one note's velocity but has no
lane UI yet. Drag-move, resize, piano keys and select-all are still absent.

### Note dragging

`piano_roll::hit_note` classifies a pointer over a note as Move (body) or
Resize (right 6 px), and `drag_result` converts the pointer delta into a 1/16
snapped start plus row pitch, or a snapped duration with an eighth-note floor.
The roll returns a `RollEdit` on drag release and the shell applies it through
`Editor::move_note`/`resize_note` (one command each). A drag preview is not
drawn and panning is suppressed while a note drag is active.

### Velocity lane

A 44 px lane under the roll draws a bar per note of the selected track (height
proportional to velocity) and maps a click or drag to 1..127, top to bottom,
through `velocity_at`; `note_at_tick` picks the covering note or the nearest one
and the shell writes it with `set_velocity` as one edit.

### Piano keys and selection

The pitch strip on the roll's left edge draws black and white keys; a click
there returns the pitch for audition (the shell renders a short note). Cmd/Ctrl+A
fills `selected_notes` with every note of the selected track, and Delete removes
them per track through `delete_notes` (one edit per track, undoable).

### Quantise tool and mouse paste

`app_commands::quantize::quantize` snaps every note start with half-step
rounding, one edit per changed track, and is shared by the editor and the MCP
`quantize` tool; `set_velocity` sets one note. Pasting uses the tick under the
pointer when it is over the roll, otherwise the playhead.

### Score selection

`score_view::hit_note` finds the notehead under a click (within 1.6 staff
spaces) and the canvas returns either that note or an insertion position. The
shell selects the matching project note, so the existing copy, cut, paste and
delete keys work in the Score view. Clicking the chord lane selects the nearest chord and returns its tick; C/X/V
copy the range from that chord to the next (or one bar), remove it, or paste
chords at the hover/playhead tick. `paste_chords_repeated` (Shift+V) adds the range four times, spaced by the
selected range's length, in one SetChords edit; a duplicate tick is rejected by
validation. Note ranges still paste once.

### Beams and tuplets

`beams_and_tuplets` groups consecutive short notes by measure: eighth-or-shorter
runs become a beam (a second line joins sixteenths), and runs of exactly three
equal notes whose duration triples to a quarter or eighth become a tuplet
bracket with its count. The canvas draws both. Slurs and articulations are not
implemented.

### Grand staff

`Settings.grand_staff` splits a track at middle C: notes below MIDI 60 sit on a
second schematic staff 7.5 staff spaces below the first, and the canvas draws
five more lines plus a brace. The split is not a true clef layout. The PDF renderer draws nine staff lines
and a brace when `Engraved.staves` is two, matching the canvas; `score_view` maps a click below three staff spaces back by six spaces before
`pitch_at_y`, so bass-staff entry and selection land in the lower octave.

### Ties and articulation glyphs

`tie_pairs` joins a note marked `tie_start` to the next `tied` note of the same
pitch, and the canvas and PDF stroke a shallow curve for each. The canvas draws
articulation glyphs (staccato dot, accent, tenuto, marcato, fermata) and
dynamics at their placed positions; the PDF keeps text abbreviations. Phrase
slurs, which are not derived from ties, are not implemented.

### Master bus

`audio::master::apply` scales by `gain_db` and, when enabled, passes samples
through `tanh` before a final -1..1 clamp; `meter` reports peak and RMS. Settings
persist `master_gain_db` and `master_limiter`; the Mixer has a master gain slider
and limiter toggle, playback applies them, and `export-audio` takes `--gain-db`
and `--no-limiter` then prints peak/RMS. Reverb/chorus and drawn meters are not
implemented.

### Track freeze

`Editor::freeze_track` renders the track alone (no buses) to a WAV path, then
applies `FreezeTrack`, which clears the track's notes and sets its audio
reference atomically; `RestoreTrack` is the inverse, so one undo restores both.
The Mixer's Freeze button writes `freeze-<track>.wav` in the working directory
and reports the path. `render_stems` renders each audible track alone (no buses)
and `export-stems` writes `<id>-<slug>.wav` files plus a `stems.txt` manifest
with name, file and duration.

### Korg PCG recognition

`style_formats::korg::scan` looks for a case-insensitive `KORG` marker in the
first 4096 bytes and a model name (KRONOS, KROME, KINGKORG, TRITON), reporting
the offset and size. It does not decode programs or combis; the CLI and MCP tool
return that limit explicitly. Real Korg files remain untested.

### L6max takes in the Mixer

The Mixer's L6max panel scans a folder with `audio::l6max::scan`, shows the take
count, how many are 32-bit float and total seconds, and imports every take at
the playhead as its own audio track through `Editor::import_l6max` (one
InsertTrack per take). Device project/mixer files are not parsed.

### SF2 reader

`audio::soundfont::parse` reads the RIFF structure and the `pdta` preset,
instrument, zone and sample-header tables, resolving preset zones to instrument
indices and instrument zones to sample indices, key/velocity ranges, root
override, coarse/fine tuning and loop mode. Modulators, filters and sample
rendering are not implemented, and SF3 (compressed samples) is not decoded.
`sf2-info` lists bank, preset, instrument and sample counts; a real 137-preset
bank parsed in a test gated on `ARRANGER_SF2`. `render_note` plays one note mono
with linear interpolation, a pitch ratio from key/root/coarse/fine tuning and
loop mode 1; `sf2-render` writes it to WAV. Envelopes, filters, modulators,
stereo samples, sampler integration and SF3 are not implemented.

### Track SoundFonts

Schema v16 adds `Track.soundfont: Option<SoundFontRef { path, bank, program }>`.
`SetSoundFont` is the one undoable edit, validation rejects an empty path, and
`render_with_soundfont` loads each referenced bank once per track and renders
notes through `soundfont::render_note`, applying the track's fader, pan, buses
and send. A missing or malformed file falls back to the built-in timbre with no
error surfaced; `sf2-use` validates the bank and assigns it. The Mixer's SoundFont panel edits
path, bank and program, validates the bank, applies one `SetSoundFont`, loads the
current values and clears the preset. Instrument zones carry the six volume-
envelope generators; `render_note` converts timecents to seconds and sustain
centibels to a linear level, then applies delay, attack, hold, decay, sustain
and a release ending at the note boundary. A per-note biquad lowpass applies the initial filter cutoff (absolute cents
from 8.176 Hz, clamped below Nyquist) and Q (centibels). Linked stereo headers (kind 2 right, 4 left, `link` partner) render both
channels with the same envelope and filter; `render_stereo` feeds them to the
project mix separately, while `render_note` averages them to mono for the
one-note CLI. Modulators and SF3 remain absent.

### Audio-model MCP tools

`get_soundfonts`/`set_soundfont`, `get_buses`/`set_buses` and
`set_track_routing` expose the schema-v16 SoundFont preset, mix buses and
bus/send routing through the same undoable commands as the UI and CLI. Missing
or null path/bus/send values clear the field; the registry now holds 37 tools.

### Audio fades

Schema v17 adds `fade_in` and `fade_out` ticks to `AudioRef`; validation rejects
negative fades or a sum longer than the reference. `mix_audio_references`
converts them to seconds with the tempo map and applies linear ramps at the
start and end of the reference. `Editor::set_audio_fades` writes one
`SetAudioRef`, and the Arrange trim row has one-bar fade-in/out and clear
buttons.

### Plug-in discovery

`plugins::scan_roots` reads each root's immediate children and keeps entries
whose extension matches the root's format (`.vst3` or `.component`, case
insensitive), naming them by bundle stem. `default_roots` covers the macOS
system/user folders, Windows Common Files and Linux VST3 folders; missing roots
are skipped. `plugin-scan` and the `plugin_scan` MCP tool list the result. The Mixer's
Plugins panel scans on demand, adds a found bundle as a slot, toggles bypass and
removes slots (`Editor::add_plugin`, `toggle_plugin`, `remove_plugin`, each one
`SetPlugins`). This proves discovery and stored state only: bundles are not
loaded, validated or hosted, and processing still ignores slots.

### Project-reading tools

`get_tracks`, `get_notes`, `get_chords`, `get_markers`/`set_markers`,
`get_clips`/`set_clips` and `get_scene_names`/`set_scene_names` expose the
project's basic state and edits. Clip ids are optional on input (the next free
id is used) and markers/clips are sorted by tick on write. The registry now
holds 93 tools.

### Track Inspector

A non-modal Inspector window follows the selected track (or the first track)
and offers a track chooser, rename (`Editor::rename_track`), volume/pan sliders
(`set_track_gain`), mute/solo/arm (`set_track_flags`), an optional program
(`set_track_program`) and read-only summaries of clips, automation, audio,
SoundFont and plug-in slots. Sidebar buttons now open Inspector, Library, Pads,
Assistant and Settings; those windows previously had no opener.

### MIDI map window

The MIDI map window lists bindings, chooses an action (play, stop, undo, scene,
first-track fader, pad 38), starts a learn session and cancels it; the shell
feeds live messages to `Learn::capture` before action dispatch, binds the
captured control, writes `lazy-arranger-midi-map.txt` and clears the session.
Clear all replaces the map and rewrites the file. Channel check and controller
pages are not implemented, and hardware capture is untested.

### MIDI channel check

`channel_check_messages(channel, note)` builds `90|channel note 100` and
`80|channel note 0`, rejecting channels above 15 and notes above 127.
`channel_check` opens the named output, sends, waits 0.05–10 s, sends note-off
and returns the bytes; `midi-channel-check` exposes it. This machine reported no
MIDI input or output ports, so the send path is byte-tested but unverified
against hardware. Controller pages remain absent.

### Track-property tools

`set_track_name`, `set_track_gain`, `set_track_flags`, `set_track_program` and
`set_audio_ref` edit the same fields as the Inspector. An empty or missing audio
path clears the reference. The registry holds 93 tools; earlier counts were two
high because the counter also matched nested `name` keys in input schemas.

### Controller page

`MidiMap::controller_bindings` returns the CC bindings sorted by number. The
Controllers window lists each with the latest value (the shell records every
incoming CC in a 128-entry table), offers the same six actions and a learn
button, and reflects new bindings from the shared map. Live values are untested
without hardware.

### Desktop MIDI recording

The sidebar Record button reads the keyboard chosen in Settings and records for
`record_seconds` (1–60 s) on a background thread through
`midi_devices::devices::record_to_song`. The finished song's first-track notes
are inserted by `Editor::insert_recorded` as one `InsertTrack` with reassigned
note ids, so one undo removes the take. Without a configured keyboard the button
reports an error. Audio input, record-arm transport and count-in are absent;
hardware capture is unverified here because no MIDI ports exist.

### Score rests

`placed_rests` merges each measure's note spans and decomposes the gaps into
whole, half, quarter and eighth rests (in that order), placing each at its
fractional x on the measure and the staff's middle line. Canvas and PDF draw
schematic rectangular glyphs sized by kind. A test checks half+quarter rests in
a partly filled measure and a whole rest in an empty one.

### Phrase slurs

Schema v18 adds `Track.slurs` (`start`, `end` ticks); validation requires end >
start. `SetSlurs` is one undoable edit, and `get_slurs`/`set_slurs` expose it to
MCP. Engraving finds the notes inside each range and draws one curve above the
highest note on canvas and PDF. MusicXML export writes `<notations><slur type="start|stop" number="1"/>` on the boundary notes and import rebuilds `Track.slurs`; the audit compares slur counts.

### Percussion staff

A track whose notes are all on channel 10 is engraved as percussion: every note
sits on one line two staff spaces above the system baseline, the canvas/PDF draw
a single staff line and x-shaped noteheads, and barlines are omitted. Pitched
tracks keep the five-line staff. A test checks the flag and equal note heights.

### Lyric spacing

`spread_lyrics` groups syllables by page and system, estimates each width as
4.6 px per character (minimum 4), and shifts a syllable right to at least the
previous right edge plus 4 px. It only handles lyrics; noteheads and rests are
not collision-avoided.

### Guitar frames

`score::guitar::shape` maps C, D, E, F, G, A majors, Am/Em/Dm/Bm, six dominant
sevenths, three minor sevenths and four major sevenths to six-string fret
shapes; slash chords and other qualities return `None`. Engraving places one
frame above the staff per chord (shifting right by 50 units when crowded), and
canvas/PDF draw the grid, mute/open markers, dots and the chord name.

### Score PDF from the app

The Score view keeps a PDF path (default `score.pdf`) and a Save PDF button. It
reuses `score::pdf::to_pdf` with the current engravings and grand-staff setting,
writes the file and reports success or the OS error in the score status line.
No OS print dialog is implemented; the CLI already had a PDF command.

### Score drag

The score canvas uses click-and-drag. A drag started on a notehead records its
tick/pitch; on release `drag_target` maps the pointer to a quarter-quantised tick
and a staff pitch (staff-aware for the grand staff), and the shell finds the
project note and applies one `move_note`. Resizing a note in the score is not
implemented.

### Lyrics window

The Lyrics window lists syllables by tick, adds the text field at the playhead,
loads a row for editing and removes rows. Each action is one `SetLyrics` edit:
`add_lyric` replaces any syllable at the tick and sorts, `remove_lyric` filters,
and `set_lyrics` writes the whole list. The score and MusicXML consume the same
events; there is no timeline lyric lane.

### Export tools

`save_project`, `export_midi`, `export_musicxml`, `render_wav` and `export_stems`
write project, keyboard MIDI, MusicXML, 16-bit WAV and per-track stem output.
MIDI uses each track's program and sends drum tracks to channel 10; the WAV mix
includes audio references and reports problem count; stems write a manifest.
One test writes all five into a temporary folder and checks non-empty files. The
registry holds 93 tools.

### Tempo in the Inspector

The Inspector lists the first five tempo-map changes and offers a BPM field, a
Current button (loads the playhead tempo) and Set at playhead
(`Editor::set_tempo_at`, one command). The transport has no ruler or record arm.

### Languages

`Language` covers English, Danish, French, Vietnamese, German and Spanish; every
key has an entry for each, and a missing one still falls back to English. The
Settings language list offers all six tags. Help text and plural rules remain
English-only.

### Ruler seek

The piano roll reserves an 18 px ruler above the clip lane, draws bar numbers at
the current zoom and returns a tick when the ruler is clicked. The shell stops
playback if needed and moves `position` to that tick; seeking during playback
therefore restarts from a stopped state rather than resuming audio at the tick.

### Single-note and chord tools

`add_note` inserts one note with a fresh id and optional duration/velocity,
`remove_note` deletes by id, `set_chord` parses a symbol and writes it, and
`remove_chord` deletes at a tick. The registry holds 93 tools.

### Resample to fit

Schema v19 adds `AudioRef.stretch`. When set, `mix_one` maps the source segment
after the in-point onto the reference's frame count (step = available frames /
target frames) instead of playing at native speed; fades still apply. This
compresses or expands by resampling, so pitch changes; it is not a
phase-vocoder time-stretch. The trim row has a Stretch checkbox and the editor
writes one `SetAudioRef`.

### Single-object reads

`get_track`, `get_note` and `get_audio_ref` return one track, note or audio
reference (including fades and stretch), or an explicit unknown-id error. The
registry holds 93 tools.

### Beat rows

`BeatPattern::row` returns a row slice, `set_row` replaces its 16 steps
(padding with zeros) and `clear_row` zeroes it. The Beat window adds C, P and X
per row, with P disabled until a row is copied. Pattern length is still one bar.

### Lyrics, marks and patch tools

`set_lyrics` replaces syllables; `get_marks`/`set_marks` exchange dynamics and
articulations by short names; `get_patch`/`set_patch` exchange waveform, ADSR and
brightness (null clears). The registry holds 93 tools.

### Multi-bar beat patterns

`BeatPattern::set_bars` resizes each row to 16 steps per bar (1–8, UI offers 1
or 2); `from_track_bars` reads the full span. `apply` clears notes from `start`
through `bar * pattern.bars()` and writes every slot, so a two-bar pattern
replaces both bars. The Beat window shows a 1/2-bar selector that reloads the
pattern from the track; reloading discards unsaved edits.

### Waveform window

`audio::waveform::visible(len, zoom, position)` returns the column range for the
Arrange waveform; zoom 1 shows all and position slides from start to end. The
trim row has zoom and position sliders. This is display only: sample editing and
recording are still absent.

### Audio input capture

`audio::record::record_seconds` opens the default cpal input, captures f32/i16/u16
samples until the duration ends, mixes channels to mono with `mix_channels` and
returns a stereo buffer; `record-audio` writes it to 16-bit WAV. Microphone
permission, device availability and real capture are unverified on this machine;
there is no monitoring, takes or waveform editing.

### Float WAV export

`RenderedAudio::to_wav32f` writes a WAVE format-3 (IEEE float) file at 32 bits;
`export-audio --bits 32` and MCP `render_wav {bits:32}` use it, and
`wav::decode_mono` reads it back. The demo exported a format-3, 32-bit file.

### Groove-library tools

`list_grooves` returns names and paths, `save_groove` extracts one bar at a
1/16 grid to `<slug>.groove`, and `apply_groove_file` loads by name and writes
adjusted notes with timing/accent amounts. A temp-folder test saves, lists and
applies. The registry holds 93 tools.

### Put-in-bars

`put_in_bars` clears `start..start + bar × total_bars`, writes the pattern
repeatedly by its span and applies one `ReplaceTrackNotes`. The Beat window's
Put in 4 bars button calls it with four bars; a test checks the four starts and
undo.

### Fit in the Doctor

The Arrangement Doctor window has low/high/polyphony fields and a drum-kit
checkbox; Compute fit calls `fit_to_synth` and shows its proposals with a Use
button that applies through `Editor::apply_proposal`. The CLI path remains
`fit-project`. A/B audition is implemented for both Doctor and fit cards through one helper: B applies the proposal to a cloned session, renders both versions with audio references, and plays through a separate audition player so transport state is untouched.

### Chord symbols in the score

`placed_chord_symbols` writes one symbol per chord event at its measure x,
shifting right by 34 units when crowded. The baseline sits above a guitar frame
when the shape table has one, otherwise just above the staff; canvas and PDF
draw the sharp-spelled name. Tests cover names, the above-frame offset and
symbols without a frame.

### Marker, lyric and query tools

`add_marker`/`remove_marker` and `add_lyric`/`remove_lyric` replace one event by
tick; `get_chord_at`, `get_note_at` and `get_time_signature` read one position
or the meter. The registry holds 93 tools.

### Accidental spacing

`shift_accidentals` groups accidentals by page and measure, walks them in x
order and shifts one left when its base would cross the previous notehead's
right edge (plus a small gap); notes at the same x (chord tones) keep their
columns. Tests force a C-major key with C#/D# sixteenths. Noteheads, rests and
dots are not otherwise collision-avoided.

### Score print

The Score view Print button writes the vector PDF to a process-specific temp
file and runs `lp <path>` with no dialog or printer selection, reporting lp's
stdout/stderr or a command error. A default printer exists on this machine, but
no job was sent, so printing is unverified.

### Clip and PDF tools

`export_pdf` writes the vector score for a chosen or first non-empty track;
`get_clip`, `add_clip` (fresh id), `move_clip` and `remove_clip` read and rewrite
one track's clip list through `SetClips`. The registry holds 93 tools.

### Bus, plugin and scene tools

`add_bus`/`remove_bus` rewrite the bus list, `add_plugin`/`remove_plugin` and
`get_plugin_slots` manage per-track slots, `get_scene_name`/`set_scene_name`
handle one scene, and `get_marker_at` reads an exact marker. The registry holds
93 tools.

### Kids Arrange

With `kids_mode` on, the track sidebar hides the recording row and the
Inspector/Library/Pads/Assistant/MIDI/Controllers/Lyrics buttons and shows large
Undo, Redo, Play/Pause, Stop and Settings buttons instead. The piano roll,
mixer and windows that were already open keep working; there is no separate
simplified song flow.

### MIDI import

`midi-import <file.mid> --out project.json` reads the file, decodes it with
`smf::decode`, wraps the song in a schema-19 `ProjectFile` and writes pretty
JSON, printing source track/note counts and decoder warnings. The demo round
trip imported 5 source tracks and 336 notes into 4 project tracks.

### MIDI open in the desktop

`load_project` reads bytes, detects `.mid`/`.midi` case-insensitively and decodes
through `smf::decode` into a schema-19 project. `ShellApp::open` marks such an
import untitled (`project_path = None`) and says so, so Save cannot overwrite the
MIDI with JSON. A test encodes the demo, writes a temp file and reloads it.

### File window

Cmd+O opens a path window seeded with the current project path; Cmd+Shift+S
opens Save as with `project.arranger`; Cmd+S saves in place, and on an untitled
import (MIDI) it opens Save as instead of writing a default JSON silently. Open
reuses `load_project` (JSON or MIDI), Save reuses `Editor::save`, and both update
the recent list. The window is manual UI; no native dialog exists.

### Audio import from the shell

`Editor::import_audio(path, start)` reads the WAV header, derives the tick length
from seconds and the tempo at `start`, and inserts one audio track with an
`AudioRef`, as one undoable edit. The sidebar's Import audio button opens the
file window in WAV mode; the test writes a one-second WAV and checks the track,
length and undo.

### Recording from the shell

Rec audio records `audio_record_seconds` on a background thread through
`audio::record::record_seconds`, writes a process-tagged temp WAV and sends the
path back; `advance` imports it with `Editor::import_audio` at the tick that was
current when recording started. Capture and microphone permission remain
unverified here; there is no monitoring.

### Song rename

`SetSongName` renames the song with the old name as inverse; the sidebar SONG
field and Rename button call `Editor::rename_song`, and MCP exposes
`get_song_name`/`set_song_name`. The registry holds 93 tools.

### Seek while playing

`Playback::seek_frame`/`seek_seconds` move the position without changing the play
state; `Player::seek_seconds` locks and delegates. A ruler click converts the
tick through `TempoMap::seconds_at` and seeks the live player instead of
stopping playback. A unit test checks both play states and clamping.

### Marker editor

`Editor::add_marker` replaces by tick and sorts, `remove_marker` filters and
`set_markers` writes once; the Markers window adds at the playhead, loads a row
for editing and removes rows, each as one `SetMarkers`. Tests cover add, replace,
remove and undo.

### Meter edit

`SetTimeSignature` validates through `TimeSignature::new`, stores the signature
and returns the old one as inverse. The Inspector has numerator and denominator
controls; MCP exposes `set_time_signature` (registry 93). Tests cover 3/4, 6/8,
invalid 0/4 and 4/3, and undo.

### MusicXML open

`project_kind` classifies `.mid`/`.midi` as MIDI, `.musicxml`/`.xml` as MusicXML
and everything else as JSON. `load_project` decodes MIDI or imports MusicXML
into a schema-19 project, and `ShellApp::open` leaves imported songs untitled so
Save cannot overwrite the source. A test exports the demo and reloads it.

### WAV gain and normalize

`audio::edit::gain` applies a decibel factor and `normalize_peak` scales the peak
to a target (silence stays silent); CLI `wav-gain` and `wav-normalize` write
32-bit float. They decode through `decode_stereo`, which duplicates mono frames,
so a 36.31 s demo stays 36.31 s instead of halving. Tests cover the dB factor,
peak target and silence.

### Light palette

`theme` exposes color accessors (`panel()`, `text()`, `accent()`, ...) that read
a process-wide light/dark switch; `set_light` flips it and `refresh` reapplies
the visuals. The Settings window has a Light theme checkbox that applies
immediately and persists as `light_theme`. Track colours switch to a deeper
seven-colour set for contrast. The dark palette values are unchanged, and a test
covers both palettes, track cycling and font binding.

### Export from the shell

`export_midi_file` builds per-track keyboard voices (program, volume, pan and
channel 10 for drums) and writes `smf::keyboard::export`; `export_musicxml_file`
writes `musicxml::export`. The sidebar Export MIDI / Export XML buttons open the
file window in export mode. A test writes both, audits the MIDI header and
re-imports the MusicXML with matching track count.

### WAV export from the shell

`export_wav_file` renders at `SAMPLE_RATE`, mixes audio references and writes a
24-bit WAV, returning duration and problem count. The sidebar Export WAV button
opens the file window. A test checks the RIFF/format/bits header and non-empty
frames.

### Normalize in the shell

`Editor::normalize_audio(track, target)` decodes the reference through
`decode_stereo`, normalizes the peak, writes `<stem>-normalized.wav` beside the
source and applies one `SetAudioRef`. Undo restores the path but leaves the file.
The trim row has a Normalize button; the test checks the switch, the 0.9 peak
and undo.

### Stem export from the shell

`export_stems_folder` renders `audio::render_stems`, writes `<track>-<slug>.wav`
in 24-bit and `stems.txt` with duration. The sidebar Export stems button opens
the file window in folder mode; a test checks that the manifest line count and
WAV count equal the returned stem count.

### L6max import from the shell

`import_l6max_folder` scans WAVs through `audio::l6max::scan` and calls
`Editor::import_l6max` at a start tick, returning imported and problem counts.
The sidebar Import L6max button uses folder mode. A test writes two synthetic
takes and checks two tracks and no problems.

### Lyric lane

The piano roll reserves a 14 px lyric lane below the chord lane, draws each
syllable at its tick and returns the nearest lyric tick on click; the shell opens
the Lyrics window with that row selected. The lane is display and selection
only; edits still go through the Lyrics window.

### Marker ruler and navigation

The piano-roll ruler draws each marker as an accent line with its name. The
Markers window adds a Go button per row that seeks the live player through the
tempo map, like a ruler click.

### MIDI panic

`panic_messages` builds CC 123 / 0 on all 16 channels; `panic(output)` opens the
named port and sends them in channel order. CLI `midi-panic --output NAME`
reports the count. No MIDI ports are attached here, so only the builder is
verified; a missing port reports the available list.

### Transport loop controls

Loop and From bar live at the top of the track sidebar, not in the painted
header; the earlier position overlapped the title badge. From bar sets the loop
to the current bar and enables it. A light-theme window capture confirmed the
header is clear and the controls are visible.

### Sidebar Windows menu

The advanced window openers and import/export actions live in one Windows menu
at the bottom of the sidebar, so the wrapped button rows no longer clip against
the status bar. The light-theme capture `docs/audit/light-theme.png` shows the
fixed header and the menu layout.

### Rhythmic measure widths

`required_measure_width` finds the smallest gap between distinct note starts in
a measure and widens that measure to at least `staff_space * 1.15` per slot
(never below the base quarter width). Systems pack measures by their actual
widths. A sixteenth-note bar is wider than a quarter-note bar in the test; other
layout tests are unchanged. Vertical rest/dot collisions are still not resolved.

### Score drag-resize

Shift is captured at drag start; on release the horizontal destination sets a
new duration (minimum one sixteenth) and the shell applies one
`SetNoteDuration`, with the old duration as the inverse. Without Shift the drag
still moves start/pitch. The Score toolbar notes the gesture. A command/editor
test covers set, rejection of zero and undo.

### Augmentation dots

`dots_for` recognizes one dot (`3/2` of a base note) and two dots (`7/4`) for
whole through 64th values; `PlacedNote.dots` carries the count. Canvas and PDF
draw 0.34-radius dots to the right of the notehead. Tests cover plain, dotted,
double-dotted and a non-matching duration.

### Dotted rests

Gap decomposition prefers whole, dotted half, half, dotted quarter, quarter,
dotted eighth and eighth; `PlacedRest.dots` carries one dot. Canvas now draws
rest glyphs (it previously drew none) and both canvas/PDF add the dot. Tests
cover a 2880-tick gap (dotted half) and a 1440-tick gap (dotted quarter).

### Score vertical reserve and PDF text

When a page has room for several systems, the engraver reserves 8.2 staff
spaces for chord frames (3 when only names exist), so first-system chord symbols
and frames stay on the page. PDF text sets the dark fill before any text; it
previously inherited the near-white page fill and was invisible. Lyrics are
deduplicated by tick, so a chord prints one syllable. The rendered demo page is
saved at `docs/audit/score-page.png`.

### Score canvas chord/frame check

The canvas now draws chord symbols and guitar frames (previously PDF only) and
percussion barlines around the one-line staff. `ARRANGER_VIEW=score` opens the
Score view at startup for capture checks; `docs/audit/score-canvas.png` is the
captured result.

The Score canvas also draws the song title inside the page margin; the title is
passed from the shell. `ARRANGER_VIEW` and the captured canvas are used for
visual checks.

### Headless canvas test

`score_view` tests run one frame in an `egui::Context`, collect `Shape::Text`
galleys and assert that the song title, chord name and measure number are
present. This checks canvas drawing without a window capture. The macOS capture
that still showed the older frame is therefore not the authority; the test is.

### Theory window

`theory_summary` detects the key with the score speller, prints the matching
major/natural-minor scale, the first eight chords as roman labels, the cadence
and the first voice-leading cost. The Windows menu opens a read-only Theory
window. A test covers the demo and an empty song.

### Four-measure systems

The default measure gap is 16 pt, so four 4/4 measures fit the 499 pt content
width (4 × 112 + 3 × 16 = 496). The demo page is re-rendered at
`docs/audit/score-page.png`; the wide-page test now expects one system of four
measures.

### Graded lessons

`music_core::lesson::question(kind, seed)` deterministically builds a
chord-quality, roman-numeral or interval question; `is_correct`, `answer_text`
and `explanation` wrap it. The Lessons window offers kind selection, New
question, answer buttons and an in-memory score. Tests cover every kind across
50 seeds, determinism and answer checking.

Kids Mode's simplified sidebar now has Settings, Lessons and Theory buttons along
with the large transport controls, so the graded exercises remain reachable.

### Snap toggle

`RollState.snap` (default true) is passed into `drag_result`; when off, move and
resize use the raw rounded tick instead of the sixteenth grid. Arrange shows a
Snap 1/16 checkbox; insertion uses the same flag and now snaps to a sixteenth
instead of a quarter. The drag test checks a free 13-tick move.

### Mixer bus rows

The Mix view lists each bus with volume and pan sliders and a Remove button plus
an Add bus action; edits are one `SetBuses` call. The demo has no buses, so the
section is a header with Add bus. The captured layout is
`docs/audit/mix.png`.

### Perform view

Outside Kids Mode the Perform view shows the current chord (last chord at or
before the playhead), the next chord and a four-by-four pad grid sized to the
available width. Kids Mode keeps its large lesson controls above the pads. The
captured layout is `docs/audit/perform.png`.

### Clip-grid layout

The Loops view uses a grid with Scene and per-track headers, a Rename button per
scene row and a context menu per slot (Clear, Length 1/2 bars, Copy to
playhead). The captured layout is `docs/audit/loops.png`; slot operations go
through the existing Editor grid methods.

### Printer choice

`printers_from_lpstat` parses `printer NAME ...` lines; the Score view queries
`lpstat -p` once and offers a Default/printer combo. Print adds `lp -d NAME`
when one is chosen. The parser has a unit test; no job was sent.

### Audio status label

The status bar reports `audio 48 kHz (cpal)` instead of the stale
`no audio engine (shell only)` claim; the schema/track/note counts stay. The
refreshed light-theme capture is `docs/audit/light-theme.png`.

### Time signature

`Engraved.time_signature` carries the song meter. Canvas and PDF draw the
numerator and denominator in the left margin at every system start; the canvas
text test and PDF header test assert both digits. Clefs and key signatures are
still absent.

### Named songs and tutorial

The Windows menu opens Songs & tutorial. It loads the arranger demo, kids demo
or an empty song through `replace_project` (new Editor, untitled, position 0)
and lists four tutorial steps. A test validates all three songs.

### Developer window override

`ARRANGER_WINDOW=settings|inspector|library|pads|assistant|midi-map|controllers|
lyrics|markers|theory|lessons|songs` opens that window at startup. The Settings
capture is `docs/audit/settings.png`; the same override is used for visual QA.

The Inspector capture is `docs/audit/inspector.png`; count labels now use a
regular plural helper (`1 note`, `2 clips`) instead of `note(s)`.

### MIDI map and controller hints

Both windows now explain the flow: choose an action, press Learn and move a
control; the map saves automatically on a successful learn. Empty states say no
bindings/controller bindings and how to add one.

Theory, Lessons and Songs & tutorial were captured for QA at
`docs/audit/theory.png`, `lessons.png` and `songs.png`. Theory and Lessons use
wider defaults and wrapped hint labels; Songs lists three named songs and four
tutorial steps.

### Lyric navigation

Each Lyrics row has Go, which seeks the live player through the tempo map via the
shared `seek_to` helper (also used by the ruler and Markers). The captured
window is `docs/audit/lyrics.png`.

### Editable shortcuts

`settings::parse_shortcut` parses `cmd/ctrl/shift/alt` chords; Settings stores
six bindings (`shortcut.undo`, `redo`, `save`, `save_as`, `open`, `play`) that
`handle_keys` consumes, with the old defaults as fallback. The Shortcuts window
(Windows menu or F1) edits and saves them and validates every binding. A parser
and settings round-trip test cover it; the capture is
`docs/audit/shortcuts.png`.

### Multi-clip selection

Shift-click toggles a clip in `selected_clips`; a plain click replaces the
selection. `apply_clip_click` returns the primary clip. Arrow keys move or
resize (Shift) every selected clip, and Delete removes them all. Clip outlines
mark every selected clip. A unit test covers add/remove/replace. There is no
marquee drag.

PDF and canvas both draw measure numbers above each staff; the PDF header test
asserts the `(1) Tj` number.

### Record-and-detect chords

`record_chords` (Settings, default off) runs `Editor::find_chords` after the
recorded MIDI take is inserted, replacing that bar's chords with the recognised
ones and reporting the count. Detection covers the whole song. Settings
round-trip and chord recognition have tests.
