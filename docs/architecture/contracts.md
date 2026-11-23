# Crosscheck: `features.md` (v1.0.0-beta.1) vs the Rust build

Source: the 39-section feature list supplied by the owner, archived verbatim at
[`docs/reference/cpp-features.md`](../reference/cpp-features.md) (v1.0.0-beta.1, 512 lines). Every
section below is a row, so nothing is skipped. Status: **verified**,
**partial**, **missing**.

| § | C++ feature | Rust | Key gap / evidence |
| --- | --- | --- | --- |
| 1 | Projects and files | partial | `.arranger` open/save, autosave/recovery, SMF in/out, WAV export in 16 and 24 bit, MusicXML export/import, style save, PSR-S970 song export. Recent list and audio-reference import (WAV). No editing of audio, PDF, Korg, L6max, SysEx |
| 2 | Window: header, views, inspector, mixer panel, AI panel, light theme, zoom | partial | Header, LCD, five views, track list, Mix strips, volume automation lane, dark theme, square corners, wheel and slider zoom. No inspector, AI panel or light theme |
| 3 | Tracks | partial | MIDI tracks with program, volume, pan, mute/solo/arm and audio references (schema v11); track list, mixer strips and keyboard export honour them. No per-track input routing, outputs, SoundFont or waveform audio tracks |
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
| 24 | PSR-S970 tools | partial | Song export with setup bar, program, volume, pan, chord markers, lyrics and drums on channel 10, plus a byte-level audit that re-parses the file; the demo exported 5 tracks, 4 programs/volumes/pans, 16 chord markers and 128 drum notes. Numbered style catalog and `style-by-number` rendering. No remote panel, audition queue or hardware verification |
| 25 | Music Finder and Playlists | partial | JSON, CSV, TSV and MFD import (1,653-record real file), CLI, and an L-key Library window that loads any of those, searches title/detail, applies an entry's tempo and imports the row's style through the catalog (setting or ARRANGER_STYLE_LIBRARY), inserting its tracks with fresh ids and a prefix. Playlist rows are listed but not opened; no AI tools or CSV→playlist writer. A real import was run through `library-import`: 1 track and 1,926 notes from ABBA Fernando; the ignored integration test passes when ARRANGER_STYLE_LIBRARY is set |
| 26 | Zoom L6max mixer page and import | partial | `audio::l6max` scans a folder of takes, reads headers, flags the L6max's 32-bit float recordings, reports problems and imports each take as an audio track at a start tick; `l6max-import` did so for two synthetic takes (3.20 s, one float). No device project-file parsing, mixer page or transport control |
| 27 | AI chat with subscriptions/keys, one-undo-step changes, proposal cards | partial | A local assistant panel and command layer (`mcp::assistant`) parses help/song/undo and `tool <name> {json}`, runs the existing 22 tools, logs replies and marks the project dirty only for changing calls; each tool call is one undo step. No model, network, subscriptions or keys, no proposal cards |
| 28 | MCP server, ~90 tools | partial | Stdio JSON-RPC server with 26 tools (song, tracks, notes, chords, undo, doctor, grooves, forms, reharmonise, clip grid, slots, programs, automation, lyrics/marks, style catalog, theory, generation, plugins, L6max scan) and save-after-call; `mcp-tools`/`mcp-call`. Loopback TCP transport with a per-start 32-hex token required on every line and save-after-call; a real client was accepted with the token and rejected without it. No TLS, multi-client concurrency or most of the ~90 target tools |
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

39 sections at round 71: 0 fully verified beyond their Rust subset, 39 partial, 0 missing.
The partials with the most depth are styles (open, import, compile, SFF1/SFF2 write), project files, the score layout/PDF path, audio references, the MIDI map and the MCP surface.

## State at round 60

`cargo fmt`, `clippy -D warnings`, `cargo test --workspace` (438 tests) and
`cargo doc` pass. No section is missing at the row level, but §19 is only a state model: no plugin
is hosted, scanned or processed. Row-level completeness is not parity.
Korg import, PSR style import, SoundFont loading, the L6max page (its 32-bit float audio
recordings already decode), waveform audio editing,
grand-staff/beam engraving, note entry, socket transport and a 90-tool MCP
surface are also absent.

Parity is **not** achieved. Hardware-dependent claims (real MIDI keyboards,
PSR-S970 acceptance, cpal output on every host, YouTube download) remain
unverified; CI builds and tests are the only continuous evidence.

## Next blocks, in dependency order

1. Clipboard in the other views (§7), then a drum pad UI and slice editing (§10).
2. Waveform audio editing and recording takes (§8), then buses/sends/effects (§17).
3. Korg import, PSR style import and SoundFont loading (§14, §20, §24).
4. Grand staff, beams, tuplets and note entry in the score (§16).
5. Speech input and the socket transport (§29, §27).
6. Plugin hosting/scanning (§19), a synth editor UI and sysex (§22), an L6max
   mixer page and project-file parsing (§26).

### Playlist conversion and languages

`library::List::from_book` turns a Music Finder book into playlist items, using
the style name against a folder, the entry number as REGIST and the viewer flag
off; `playlist-from-csv` writes the Yamaha TSV. Strings now ship in English,
Danish, French and Vietnamese: every key has text in all four tables, and the
language setting switches them at once. Plural rules and localized help do not
exist.

### Kids lesson and Perform

Kids Mode keeps two tabs and builds a `kids::Lesson` from the song's chords; the
Perform view shows the current chord large, progress, Next chord and Start
over, plus 16 big chord pads that audition through the cpal player. `Lesson::sync` rebuilds it when the chord list changes while keeping the step
when possible, and the Perform view can load `demo::kids_song` (four slow
chords). There is no simplified editor.

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
holds 50 tools.

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
path clears the reference. The registry holds 50 tools; earlier counts were two
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
