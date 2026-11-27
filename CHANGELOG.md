# Changelog

All notable changes to Lazy Arranger (Rust) are listed here. Versions follow
[semver](https://semver.org/); a `-beta.N` suffix marks a beta build.

## [Unreleased]

## [0.1.0-beta.7] - 2026-11-27

### Fixed
- Mixer meters on MIDI tracks show real levels: a muted track (or one
  silenced by a solo) shows nothing, the fader scales the meter, and a
  chord no longer pins it to the top.
- Turning the metronome on or off while the song plays takes effect right
  away, not only after playing again from the start.
- The "Save changes?" question when quitting or opening another project is
  bigger, with clear text, a dimmed background and a highlighted Save
  button. Settings has a new option to turn the question off; the autosave
  is kept, so edits can still be recovered.

## [0.1.0-beta.6] - 2026-11-27

### Fixed
- Right-clicking a part on the timeline opens its menu every time. Before,
  the menu often blinked and closed, mostly with display scaling on.
  Submenus (Gain, Fades, Convert to MIDI) open on hover.
- Menus have a lighter background, a border and a shadow, so they stand
  out from the timeline and panels.
- A selected part is darker with its own outline; the box around the
  whole lane is gone. Audio parts can be selected.
- A click on an audio part or an empty lane no longer adds a note.
- Scrolling the timeline up or down while the song plays no longer snaps
  back.
- A plug-in that can't keep up while the song plays is turned off for the
  session, and its track plays dry, instead of making the sound stutter.
  A plug-in that stops responding no longer freezes playback: after two
  seconds the app restarts playback without it and says which plug-in it
  was.
- Audio clips and VST3 instruments play live while the song plays. A clip
  move, trim or fade and a note edit on a plug-in instrument are heard at
  once, with nothing rendered first. Audio Unit instruments are still
  rendered before play.

### Changed
- Solo is exclusive: soloing a track un-solos the others. Cmd-click (Mac)
  or Ctrl-click (Windows, Linux) a solo button to add a track to the solo.
- Alt/Option+wheel over the timeline zooms audio waveforms (up to 32×, display
  only) to see quiet audio; audio parts also have a Waveform Zoom menu.
  Track lanes can be up to 400 pt tall.

## [0.1.0-beta.5] - 2026-11-26

### Fixed
- Mixer faders, pans, mute, solo, bus faders and the master fader are heard
  right away while the song plays, for MIDI and audio tracks. Before, each
  move waited for the whole song to render again. This now holds for
  tracks with volume, pan or send automation too: the engine follows the
  automation as it plays, and an automation edit is heard at once.
- Edits on a track with recorded audio, a plug-in instrument or plug-in
  automation (a clip move, a take, a fade) are heard while the song plays.
  Only that track is rendered again, in the background, and swapped in.
- The master reverb, chorus and limiter react right away while the song
  plays, with no new render. Note tracks also stay live when the master
  reverb or chorus is on, and a reverb tail rings on when the mix changes.
- Plug-in effects (VST3 and Audio Unit) on tracks, buses and the master
  run live while the song plays. A preset, knob or bypass change is heard
  within a moment, without rendering the song again, and bus plug-ins no
  longer force the tracks on that bus to render. A change that alters a
  track or bus plug-in's latency still renders the song again.
- Loops: a launched or stopped slot changes on the bar line, to the
  sample, with no render wait. Note tracks swap at once; a track with
  recorded audio or a plug-in instrument renders only its own lane in the
  background.
- Export Audio now uses the master gain, limiter, reverb and chorus from
  the mixer, as playback does.

### Changed
- Audio clips on the timeline look like Luna: a pastel body in the track
  colour, a name strip on top, the beat grid showing through, and a sharp
  dark waveform for each channel at any zoom.
- Export Audio (app, CLI and MCP), stems and bounce play the song through
  the same engine as playback, faster than real time, so an export sounds
  exactly like what you heard. The separate offline mix is gone.
- Chord, drum and style previews play live and start without a
  render wait.
- Tracks that only play notes (built-in synth or SoundFont, no recorded
  audio and no plug-ins) are played live by the audio engine instead of
  being rendered before play. Play starts sooner, and note edits are heard
  while the song plays.
- Playback runs through a new real-time engine core. The song is prepared
  off the audio thread and handed to the audio callback, which swaps it in
  at its next buffer. A mix change that needs a new render no longer
  restarts the audio stream, so the swap is gapless.
- The master meter shows what was actually played.
- A track's fader and pan now come after its insert plug-ins, as on a
  mixing console.

## [0.1.0-beta.4] - 2026-11-26

### Changed
- New, Open, loading a demo and Quit now ask Save, Discard or Cancel when
  the song has unsaved edits.
- VST3 plug-ins outside the standard plug-in folders load only after you
  approve them. The app asks once per project and remembers the answer.
- Edits across several tracks (song forms, breaks, transitions, loops,
  record to timeline, regenerate) are one undo step.

### Fixed
- Saving replaces the file in one step on every platform. If the save
  fails, the old file stays. A restored autosave counts as unsaved until
  you save it.
- MCP clients get an answer only after the edit is saved, and an error if
  the save fails.
- On Windows, the local AI assistant, dictation and speech commands work.
- Undo and redo restore exactly what was there before.
- Large MIDI files, styles and MusicXML files open much faster.
- Device windows and the plug-in window no longer slow the app down while
  they look for ports or scan plug-ins.
- More small fixes from the source code audit.

## [0.1.0-beta.3] - 2026-11-26

### Changed
- Slow jobs no longer freeze the window: exporting WAV and stems, importing
  audio, freezing a track, printing, exporting the score PDF, transcribing
  audio and loading takes run in the background. The status bar shows what
  is running and for how long. Auditions of takes and proposals now play.

### Fixed
- Fixes from a second source code audit: plug-ins unload safely, no stuck
  notes after editing during playback, steadier audio and MIDI timing, the
  full score no longer hangs and draws slurs and beams correctly, non-ASCII
  score PDF titles, flat keys named with flats, and tighter checks on files
  that the AI tools write.

## [0.1.0-beta.2] - 2026-11-26

### Changed
- Panels resize when you drag the seams between them, the timeline zooms,
  and parts move and resize on the timeline as in the first version.

### Fixed
- Fixes from a source code audit: safer plug-in hosting (VST3 and Audio
  Units), no stuck notes after stop or seek, glitch-free audio and recording
  callbacks, sturdier file import (SoundFont, MusicXML, C++ projects), and
  tighter limits on what the AI assistant and MCP server may touch. A project
  that fails to open is no longer overwritten by the demo song.
- The score with all tracks now lays out correctly on screen and in the PDF:
  each staff has its own correct clef (Bravura font), and the staves no
  longer overlap. A one-track PDF follows the same notation rules.
- The mixer now changes the song while it plays. Moving a fader or pan knob,
  or pressing mute or solo, used to do nothing until playback was restarted.
- Projects saved by the C++ app with audio tracks now open with their audio:
  each stem plays and shows on the timeline. Paths like `Stems/01 Drums.ogg`
  are found next to the project file. Track volume, pan, mute and solo, and
  the MIDI program and bank, now carry over too.

## [0.1.0-beta.1] - 2026-11-25

The first public build of the Rust rewrite of Lazy Arranger Midi DAW. It is a
beta: please report what breaks.

### Highlights
- Arrange view with a track list aligned to the timeline lanes, an inspector,
  piano roll and score editor, in the look of the C++ app.
- Loops view like the C++ app: each scene has its own clips, and slots launch
  on the bar. Clips open in a docked piano roll.
- Drums (step grid) and MIDI Map views rebuilt to work like the C++ views.
- Mixer with a metallic, console-style look: faders, meters, sends, buses,
  master effects, and VST3/AU plug-in slots.
- Full menu bar (File, Edit, Track, Style, Transport, View, AI, Window, Help),
  native on macOS, plus right-click menus on the timeline, tracks, piano roll,
  score and mixer strips.
- Optional instrument editors (Roland D-05, Yamaha PSR, XG, Zoom L6max and
  more), chosen at first run, with AI-assisted instrument creation.
- Yamaha style import/export (SFF1/SFF2), MIDI import/export, MusicXML import,
  opening C++ app `.arranger` projects, SoundFont 2 playback, audio rendering,
  MIDI and audio recording, score PDF and printing.
- Eight interface languages, light and dark themes, and Kids Mode.
- `lazy-arranger` command-line tool and an MCP server for AI assistants.

### Known limits
- macOS builds are ad-hoc signed and not notarized; Windows builds are not
  code-signed.
- Not yet tested on a real Mac: Loops playback and the native menus.
- Plug-in editor windows open on macOS only.
- The editable score shows one track at a time (the full score is read-only).
- Controller profiles from the C++ app (X-Touch Mini, Launchkey) are not
  ported yet.
- Menu labels are English only.
- Feature parity with the C++ app is not claimed; see
  `docs/audit/cpp-features-crosscheck.md`.
