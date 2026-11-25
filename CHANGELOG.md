# Changelog

All notable changes to Lazy Arranger (Rust) are listed here. Versions follow
[semver](https://semver.org/); a `-beta.N` suffix marks a beta build.

## [Unreleased]

### Fixed
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
