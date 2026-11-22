# Lazy Arranger Midi DAW: all features

Version 1.0.0-beta.1 (feature complete). Built from the code on `main` and the
docs in the repository. Runs on macOS (Universal), Windows x64 and Ubuntu
Linux x64. On Windows and Linux, `Ctrl` replaces `Cmd`.

## Contents

1. [Projects and files](#1-projects-and-files)
2. [Main window and look](#2-main-window-and-look)
3. [Tracks](#3-tracks)
4. [Transport, playing and recording](#4-transport-playing-and-recording)
5. [Timeline and clips](#5-timeline-and-clips)
6. [Note editor (piano roll)](#6-note-editor-piano-roll)
7. [Cut, copy and paste](#7-cut-copy-and-paste)
8. [Audio tracks and audio editing](#8-audio-tracks-and-audio-editing)
9. [Audio analysis and transcription](#9-audio-analysis-and-transcription)
10. [Drum sampler and slicer](#10-drum-sampler-and-slicer)
11. [Beat view (step sequencer)](#11-beat-view-step-sequencer)
12. [Clip grid](#12-clip-grid)
13. [Smart chords](#13-smart-chords)
14. [Styles](#14-styles)
15. [Style from Audio and Style from Link](#15-style-from-audio-and-style-from-link)
16. [Score view](#16-score-view)
17. [Mixer and buses](#17-mixer-and-buses)
18. [Automation](#18-automation)
19. [Plugins](#19-plugins)
20. [Built-in sound](#20-built-in-sound)
21. [MIDI devices and device profiles](#21-midi-devices-and-device-profiles)
22. [Synth sound editors](#22-synth-sound-editors)
23. [Hardware controllers and MIDI Map](#23-hardware-controllers-and-midi-map)
24. [Yamaha PSR-S970 tools](#24-yamaha-psr-s970-tools)
25. [Music Finder and Playlists](#25-music-finder-and-playlists)
26. [Zoom LiveTrak L6max](#26-zoom-livetrak-l6max)
27. [AI chat](#27-ai-chat)
28. [MCP server (outside AI assistants)](#28-mcp-server-outside-ai-assistants)
29. [Speech](#29-speech)
30. [AI music tools](#30-ai-music-tools)
31. [Music theory and skills](#31-music-theory-and-skills)
32. [Arranging help](#32-arranging-help)
33. [Groove](#33-groove)
34. [Song Forms](#34-song-forms)
35. [Kids Mode](#35-kids-mode)
36. [Languages](#36-languages)
37. [Settings, shortcuts and updates](#37-settings-shortcuts-and-updates)
38. [Performance and safety](#38-performance-and-safety)
39. [Demo content](#39-demo-content)

---

## 1. Projects and files

- Project files (`.arranger`): New, Open, Save, Save As.
- Open Recent: last 10 files, newest first, missing files left out, Clear List.
- Each file dialog remembers the last folder for its file type.
- Autosave every minute after a change; crash recovery at next start
  (Recover, Discard, Later). Old copies deleted after 30 days.
- MIDI files: import (one track per channel, with tempo, meter, sounds, mixer
  values and lyrics) and export (with lyrics from the Score).
- MusicXML: import `.musicxml`, `.xml`, `.mxl` (notes, key, meter, tempo,
  sounds, chord symbols, first-verse lyrics, dynamics, articulations) and
  export `.musicxml`.
- Audio import: WAV, AIFF, FLAC, Ogg on all systems; MP3, M4A (AAC), CAF on
  macOS; audio from MP4, MOV, MKV, WebM, AVI, WMV, MPEG video with FFmpeg.
  Drag and drop from the file browser. Up to 45 minutes (non-WAV).
- Audio export: render the mix to a 24-bit WAV.
- PDF export and printing of the score.
- PSR-S970 song export (`.mid` for the keyboard's Song player).
- Yamaha style files: open and save `.sty`/`.prs`; open Korg styles in SFF1/SFF2.
- Zoom LiveTrak L6max multitrack folder import.
- SysEx dumps: save and load `.syx` sound dumps.
- Project files store audio file paths, not the audio. Missing files show
  `File not found`; missing plugins show `(not loaded)` and keep their settings.

## 2. Main window and look

- Top bar: transport, position, tempo, meter, view tabs, Mixer button and the
  copper `Ask AI` button.
- Views: `Timeline`, `Score`, `Drums` (Beat), `Loops` (Clip grid) and `Map`
  (MIDI Map).
- Track list on the left (resizable), inspector on the right (track, song,
  metronome, devices), mixer panel at the bottom, right side panel for AI chat
  and AI tools (resizable).
- Bottom bar: MIDI inputs with an activity dot per input.
- Dark and light themes (warm charcoal and copper), flush panels.
- Tooltips show the shortcut of each button and follow changed shortcuts.
- Zoom time (`Cmd`+wheel or pinch) and track height (`Cmd+Shift`+wheel, `-`/`+`).

## 3. Tracks

- Add MIDI tracks (`Cmd+T`) and audio tracks.
- Outputs: Built-in SoundFont, Drum sampler, Plugin instrument, or a MIDI port.
- MIDI channel per track; sound picker by name from the SoundFont or the
  device profile (General MIDI names as fallback).
- Rename, move, duplicate, colour, delete. Mute (`M`), solo (`S`), arm (`R`).
- Per-track MIDI input: all keyboards, one keyboard, or one channel.
- Per-track audio input (mono or stereo, with named inputs from device profiles).
- `Reset` sends the synth's reset message when its profile has one.

## 4. Transport, playing and recording

- Play/stop (`Space`), go to start (`Home`/`Enter`), record (`R`), loop (`L`),
  metronome (`C`).
- Click the ruler to move the playhead; drag the lower ruler to set the loop.
- Tempo and meter editing in the top bar.
- MIDI recording into new clips on bar lines; records on the armed or
  selected track. Optional `Quantise While Recording`.
- Stop sends note-offs; a second stop sends All Sound Off and All Notes Off.
- Keyboards and controllers turn on automatically when connected; software
  ports stay off; inputs you turned off stay off.
- Plugin delay compensation keeps all tracks in time.
- Uses almost no CPU when nothing plays.

## 5. Timeline and clips

- Add, select (click or drag box), move (also across tracks), resize,
  duplicate (`Cmd+D`), split at playhead (`Cmd+E`), join, delete.
- Snap to bars/beats; `Alt` for free movement.
- Clips show their notes; looping clips; double-click opens the note editor.
- Chord lane under the bar numbers (see Smart chords).
- Song sections shown on the ruler.
- Tempo map and lyrics support.

## 6. Note editor (piano roll)

- Add, select, move, resize and delete notes.
- Arrow keys move by semitone; `Shift`+arrows by octave.
- Velocity lane.
- Grid setting for input, movement and quantise; `Alt` ignores the grid.
- Quantise (`Q`) selected notes or the whole clip.
- Click the piano keys to hear a pitch.
- Select all (`Cmd+A`), cut, copy, paste.

## 7. Cut, copy and paste

- Works in every view: Timeline (clips, MIDI and audio, or automation
  points), note editor and Score (notes), Beat (full pattern or a row), Clip
  grid (slot clip).
- Paste at the playhead, at the score input cursor, or `Paste Here` at the
  mouse.
- Multi-track paste keeps the track layout; notes pasted where no clip exists
  make a new clip; Timeline notes can go into a Clip grid slot.

## 8. Audio tracks and audio editing

- Audio clips with waveforms; the original files never change.
- Record audio to 24-bit WAV (next to the project, or in
  `Music/Arranger MIDI DAW/Recordings`).
- Input monitoring (`Hear input`).
- Trim, split, move, duplicate, delete; clip gain; fade in and fade out.
- Takes: overlapping clips, top clip plays, automatic short crossfades,
  longer crossfades from fades, `Takes Here` menu, `Bring to Front`, comping
  by splitting all takes at once.
- Follow Song Tempo: time-stretch without pitch change; `Set File Tempo...`.
- Convert to MIDI: melody (one note at a time) or drums (kick, snare, hi-hat).

## 9. Audio analysis and transcription

- `Analyse Audio (Stems, Chords, Lyrics)`, all on your own computer:
  - Stems: vocals, drums, bass and other, each on a new audio track.
  - Notes: a MIDI track for each stem.
  - Chords: chord symbols on the score.
  - Instruments: names, with a matching General MIDI sound per track.
  - Lyrics: sung words under the vocal notes (built-in Whisper, with language,
    title and hint words). Whisper model download (for example Large turbo).
- One-time tool install (`Options > Audio Analysis Tools...`).
- `Fix Lyrics with AI` and lyric correction in the Score.
- Check the Transcription: the app finds unclear bass notes and chord roots and
  asks you. Cards with A/B choices, `Original`, `Play A`/`Play B` with a guide
  chord, `Use`, `Dismiss`, and `Same answer in similar places`. Answers update
  the chord symbols and bass notes and are kept for Style from Audio.

## 10. Drum sampler and slicer

- Drum sampler output: load audio files as pads from C1 (36) up.
- Audition, change or remove pads; choke groups.
- Slicer: slice at each hit (with sensitivity), in equal parts, or by hand
  (markers on the waveform). Audition slices; set loop length; make a clip
  that plays the slices and follows the song tempo.

## 11. Beat view (step sequencer)

- Rows are drum sounds (GM drums or the sampler's pads), columns are steps.
- Click or drag to add steps; right-click for soft, normal or loud.
- Step length (for example 1/16) and pattern length (1, 2, 4 or 8 bars).
- Clear, copy and paste a row or the whole pattern.
- `Put in bars...` to place the pattern, also on a new drum track.
- Long drum clips show from bar 1 with scrolling; square steps that resize
  with the window.
- Only the selected track plays while the view is open.

## 12. Clip grid

- Tracks as columns, scenes as rows, slots as boxes.
- `From Timeline...` cuts the song into scenes of N bars.
- Empty clips, copy from the Timeline, edit notes, set length, rename, copy,
  delete, rename scenes, resize slots.
- Audio loops in slots, with tempo following and loop length.
- Launch clips and scenes, quantised start (`Clips start` setting), cancel a
  waiting clip, stop a track, `Stop all`. Waiting clips flash; playing clips
  show progress.
- Record the Clip grid performance to the Timeline.
- Record notes into a playing slot.
- Launch from the Launchkey pads and X-Touch Mini buttons.

## 13. Smart chords

- Chord lane: type chord names or roman numerals (`Am F C G`, `vi IV I V`),
  with `|` for bars, `%` to repeat, `/` to hold.
- Next-chord suggestions while typing and per bar.
- Smart Chords panel (`Cmd+Shift+K`):
  - 16 chord pads for the song key: diatonic, borrowed and secondary
    dominants, with inversion, voicing (including Smooth) and octave.
  - Write the chords you play, while recording or step by step when stopped,
    with a grid and a left-hand split.
  - Reharmonize a chord (for example tritone substitute, borrowed chord, ii-V)
    with matching bass and chord parts; Play A/B/Original, Use, Undo.
- Launchkey pads can play the chord pads, with knobs for inversion, voicing
  and octave.
- `Find Chords from the Music` from the chord lane.

## 14. Styles

- Open Yamaha `.sty` and `.prs` styles and Korg SFF1/SFF2 styles.
- Style folder (also used by the AI); optional unpack of 540 demo styles.
- Fill tracks from a style: choose section, chords, bars and tempo; one track
  per part with the style's voices and mixer values.
- Edit a style in the Clip grid: sections are scenes, parts are tracks; edit
  with the note editor and Beat view.
- Play a style live: start sections, chords from the keyboard below F#3 or
  from the `Chord` button; section changes at the next bar.
- Make a style from Clip grid clips, or a section from a loop range of a MIDI
  file (chord found automatically).
- Chord rules editor per part and section: Written in, Chord root, Chord type,
  Follow the bass note, Highest root, Note limits, Held notes.
- Save as a Yamaha `.sty` file for the keyboard.

## 15. Style from Audio and Style from Link

- Make a PSR-S970 style from an audio clip: finds bars, sections (Intro, Main
  A-D, Fill In, Ending) and chords; makes a pattern for each part; tests it
  against the stems and improves it step by step.
- Shows the match (0-100%) per section and part, and the changes kept.
- Compare `Original`, `Style` or `Both`; `Save Style...`; `Open in Loops`.
- `From Link...`: get the audio from a web link with yt-dlp, then make the
  style. Install help for yt-dlp and FFmpeg on each system.
- Uses your transcription answers.

## 16. Score view

- Show all tracks (full score), the selected track, or chosen tracks.
- Own engraving engine with the Bravura music font.
- Notes quantised for display only (16ths and eighth-note triplets).
- Spelling in the song key, with direction-aware accidentals; key finding.
- Automatic clefs, grand staff for keyboard tracks, drum staff with X
  noteheads for cymbals; staff setting (automatic, one staff, grand staff).
- Page zoom; playback cursor with auto-scroll.
- Edit notes: select, drag, arrows for key step, semitone, octave, time and
  length; delete; cut, copy, paste. Ties handled as one note.
- Note input (`N`): mouse, computer keyboard (A-G) or MIDI keyboard (chords
  too); note lengths `2`-`7`, dots, rests.
- Marks: chord symbols (`K`), lyrics (`L`, with hyphenation), dynamics
  (pp to ff), articulations (staccato, accent, tenuto, marcato, fermata).
- Find chords in the notes (per bar, two per bar, or per beat) with slash
  chords and key-aware spelling; remove chords.
- Guitar chord boxes above chord symbols.
- Export PDF (A4) and print.

## 17. Mixer and buses

- Resizable bottom panel in every view (`Cmd+2`); strips shrink gracefully.
- Per strip: sound name, reverb and chorus knobs, pan, mute, solo (exclusive,
  or multi with `Shift`/`Cmd`), arm, fader, meter.
- Faders glide when moved by the AI or a controller.
- Fine adjust with `Shift`; double-click resets.
- Master strip for the built-in player.
- For MIDI ports: sends CC 7, 10, 91, 93 (changeable per device profile).
- Buses: add, rename, remove; route track outputs (groups) or post-fader sends
  (returns); effects on buses; strip shows routing counts.
- Master effects.

## 18. Automation

- Lines for volume, pan, reverb send and chorus send (volume and pan on audio
  tracks).
- Lines for any plugin parameter (grouped menus for large plugins).
- Add, move, select, delete, cut, copy, paste points; snap or free (`Alt`).
- Show/hide lines; automation stays active when hidden.
- MIDI port tracks send automation as MIDI controllers.

## 19. Plugins

- VST3 on all systems; Audio Units on macOS.
- Plugin scan in a separate process; crashing plugins are skipped from then on.
- Instrument plugins on tracks; effect chains on tracks, buses and the master
  (bypass, reorder, remove, open editor).
- Effects on SoundFont tracks work per MIDI channel.
- Plugin state saved in the project.
- Delay compensation.

## 20. Built-in sound

- GM/GS SoundFont player (SF2 and SF3) with reverb and chorus.
- First-start offer to download GeneralUser GS (32 MB); `Choose SoundFont...`
  for your own files.
- Metronome.

## 21. MIDI devices and device profiles

- Device profiles in YAML: identity, bank select, sound lists, drum kits,
  mixer controllers, reset message, capabilities (polyphony, parts, range),
  audio interface input/output names, and sound editors.
- Built-in profiles: Yamaha PSR-S970, Yamaha PSR-S900 (XG part mixer, no voice
  list yet), Roland D-05, Novation XioSynth (200 factory patch names), Korg Pa
  series, Zoom L6max, generic GS and generic XG.
- Profile chosen from the MIDI port name; you can tell the app which device is
  on a port (for example a USB MIDI cable).
- Your own profiles and sound names in the `Devices` folder override built-ins.
- Profiles are checked; errors report the line number.
- The AI can write a profile from a synth's MIDI chart.
- Recent MIDI buffer (last two minutes) to find what a control sends.

## 22. Synth sound editors

- Knob-and-list editors defined in the device profile (CC, NRPN or SysEx,
  multi-byte values, checksums).
- Roland D-05: all patch and tone pages, names page with `Send Names`, patch
  MIDI settings, Separate modes (next channel for the second track), user
  reverb slots 17-32, `Read from Synth`, `Get Sound Names` (8 Internal banks).
- Novation XioSynth: full sound editor; screen knobs follow the synth's knobs.
- Yamaha PSR-S970 / S900: XG part mixer for 16 parts (volume, pan, reverb and
  chorus send).
- Zoom L6max: every factory controller number as knob pages.
- `Send All`, `Save Dump...` and `Load Dump...` (`.syx`).
- Changes are sent live (`Cmd+Shift+E`).

## 23. Hardware controllers and MIDI Map

- Ready-made controller pages:
  - Novation Launchkey Mini MK3: DAW mode, knobs for volume, pan or sends,
    bank left/right, play, record, 16 pads launch clips (2 scenes), scene
    launch, stop all, pad lights. Pads can also play chord pads.
  - Behringer X-Touch Mini: layer A volume and clip buttons (scenes 3-4),
    layer B pan, mute and solo, fader for master.
  - Novation XioSynth 49 (with a template on channel 16): volume, pan, mute,
    play, stop, record.
- Banks of 8 tracks; movable pad rows in the Clip grid.
- MIDI Map view (`Cmd+6`): every track by every control (volume, pan, sends,
  mute, solo, arm, four clip rows, global controls). Click a cell to map or
  clear it.
- MIDI Learn per cell.
- `Ask AI` writes or fixes a controller page.
- Channel check warns when two tracks share a port and channel.

## 24. Yamaha PSR-S970 tools

- Export a song file for the keyboard: nearest voice per track, drums on
  channel 10, tempo, meter, lyrics, Beat patterns, automation and chord
  symbols; setup bar at bar 1; report of what is in and out.
- Style Remote: start Intro, Main A-D, Fill In, Break and Ending on the
  keyboard; send tempo; send chords with bass note.
- Select a keyboard style by number from the AI chat.
- Play styles with the keyboard's own voices over MIDI.
- Export the mix as WAV for the keyboard's audio player.

## 25. Music Finder and Playlists

- Music Finder books (JSON): songs with style, tempo, voices and notes.
- Through the AI chat: add entries, list them, export and import CSV, import
  Yamaha `.mfd` files (name, style number, meter, tempo, genre, keywords).
- Read Yamaha Playlist `.tsv` files and write a Music Finder book as a
  Playlist.

## 26. Zoom LiveTrak L6max

- L6max Mixer page: EQ, sends, pan, level, mute, MONO and USB for 8 channels,
  effects, compressor, scenes A-D, Reset, Compact layout.
- Named USB inputs (CH1-CH8, MASTER) for recording.
- Import L6max recordings: one audio track per channel; master track muted.

## 27. AI chat

- In-app chat (`Cmd+J`) in the side panel.
- Assistants: Claude Code, Codex, Gemini CLI (using your subscriptions),
  Anthropic API key, DeepSeek API key, any OpenAI-style API (OpenAI, Gemini,
  Ollama and others). Add more programs in `assistants.json`.
- API keys kept in a private `ai-keys.json` file.
- Every AI change is one undo step named `AI: ...`.
- What it can do: read and change the song, tracks, notes, clips, score marks,
  lyrics, drum patterns, Clip grid, styles, synth sounds and sound names, MIDI
  setup and mappings, device profiles, transport, undo and redo; plus all the
  tools in sections 9, 13, 24, 25 and 30-35.
- Proposal cards with A/B audition for arranging tools.

## 28. MCP server (outside AI assistants)

- `Options > Allow AI Assistants` and `Copy AI Assistant Setup` for Claude
  Desktop, Claude Code, Codex and other MCP clients.
- Separate `arranger-mcp` helper; local socket only, new secret key at each
  start.
- About 90 tools, for example: `get_song`, `add_track`, `add_notes`,
  `edit_clips`, `set_chords`, `type_chords`, `suggest_next_chords`,
  `reharmonize`, `get_beat`/`set_beat`, `get_clip_grid`/`set_slot`/`launch`,
  `find_styles`/`fill_from_style`, `find_sounds`/`set_sound`,
  `get_midi_setup`/`set_midi_input`/`set_port_device`, `recent_midi`,
  `read_device_file`/`write_device_file`, `analyse_audio`, `edit_lyrics`,
  `transcription_questions`, `style_from_audio`, `arrangement_doctor`,
  `check_hardware`, `extract_groove`/`apply_groove`, `make_song_form`,
  `analyze_key`, `check_voice_leading`, `find_skills`, `kids_mode`,
  `add_music_finder_entry`, `import_playlist`, `send_psr_style_number`.

## 29. Speech

- Voice input with the built-in Whisper (microphone button).
- Spoken replies (on/off).
- Voices: system voice, Kitten (small, offline) or Kokoro (higher quality,
  offline), set in `Options > Speech...`.

## 30. AI music tools

All run on your own computer after a one-time install
(`AI > Install AI Music Tools...`).

- Humanize: Magenta Groove for drums, or a Simple mode for any track.
- Drumify: a drum part from the rhythm of a track.
- Continue with AI: continue or fill a gap (Magenta RNNs or the Anticipatory
  Music Transformer).
- Write a Part: a new part that fits the others (Anticipatory Music
  Transformer).
- Melody over Chords: a melody over the song's chord symbols.
- Track, bars and Variation controls; progress, time and Stop.

## 31. Music theory and skills

- Keys and modes, key finding with modulations, correct spelling.
- Roman numeral analysis (inversions, secondary dominants, borrowed chords,
  Neapolitan) and roman numerals to chords.
- Scales: seven-note, pentatonic, blues, whole tone.
- Voice-leading check (parallel and hidden fifths and octaves, crossing,
  large leaps).
- Skills layer: built-in common chord progressions (pop, rock, jazz, blues);
  add your own JSON skill packs in the `Skills` folder. Skills hold text and
  data only.

## 32. Arranging help

Rule-based, fast, no AI model needed. Proposals with Play A, Play B, Original,
Use, Dismiss; the app learns which kinds you use.

- Arrangement Doctor: make a section stand out, give the melody room, make
  busy bars clearer, repeat less. Never changes the melody.
- Fit to My Synths: polyphony (with sustain pedal), parts, sound clashes on a
  channel, note range, drums without a drum kit. Protects bass, melody and
  drums. Editable synth numbers (`Save Numbers`).

## 33. Groove

- Take a groove from a MIDI track, an audio track, a MIDI or audio file, or
  words (for example "lazy swing, behind the beat").
- Groove described in words (swing %, ms behind the beat).
- Built-in grooves and your own library (`.groove` files).
- Apply to one track or all others, with Timing, Accents, and Kick, Snare and
  Hi-hats amounts.
- Feel match score; Play Before / Play After; loop range only when the loop
  is on.

## 34. Song Forms

- Find or edit song sections; save them (shown on the ruler).
- Make forms: Short version, Long intro, Gig version, Instrumental break, or
  your own order (`x2`, `(no melody)`, rest named tracks).
- Transitions at joins: drum fill (style Fill In when available), lead-in,
  hold the chord, break, straight cut, or Automatic with a reason.
- Play the form or a join loop; Use writes it to the Timeline (one undo step).

## 35. Kids Mode

- Full-window mode (`Cmd+Alt+K`): any key on any keyboard, computer key or a
  click plays the song; it pauses when keys stop and loops at the end.
- Large animated circle shows the state.
- Adult settings: pause delay (0.5-5 s), keys also play their own notes, exit.

## 36. Languages

- English, Danish, French and Vietnamese (menus and main panels), or System
  Default. Change in `Options > Language` (restart needed).

## 37. Settings, shortcuts and updates

- `Audio and MIDI Settings...` (audio device, buffer, inputs and outputs).
- Keyboard shortcut editor: change, add, remove, conflict warning, reset.
- Show Device Profiles Folder.
- About box with version.
- Update check at start and on demand (Download, Later, Skip This Version);
  sends no song data.

## 38. Performance and safety

- Native C++ app; real-time engine separate from the UI.
- Near-zero CPU when idle.
- Plugin scan in a separate process.
- Damaged style, SysEx, MIDI and JSON files do not crash the app.
- AI access only on this computer; can be turned off at any time.
- Heavy AI and analysis models run outside the app process, offline after
  install.

## 39. Demo content

- Demo songs `Copper Lights` (EDM, 126 BPM, A minor) and `Northern Fire`
  (rock ballad, 72 BPM, E minor, with lyrics), each as MIDI project, audio
  project and General MIDI file.
- Optional 540 demo styles.
