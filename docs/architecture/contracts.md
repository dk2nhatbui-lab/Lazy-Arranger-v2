# Architecture contracts

These are the invariants the crates rely on. ADRs in `docs/adr/` record the
decisions behind them; the feature crosscheck records current coverage.

## Project and commands

- `ProjectFile` is the only persisted document. The current schema is v20;
  older documents migrate through `ProjectFile::from_json`, and files with a
  newer schema are rejected rather than guessed.
- Every mutation goes through `app_commands::Command` and `EditSession`.
  Commands validate before changing the song, invert exactly, and form one undo
  step. A failed command leaves the song unchanged.
- Unknown track, clip, note, bus or plug-in identifiers are errors. Commands
  must not append no-op undo entries.
- `project::validate` is the final gate: errors make the project invalid;
  warnings describe recoverable problems. JSON load and save both pass it.
- Audio references point at real files. Paths, lengths, offsets, fades and comp
  spans are validated before they reach the renderer.

## Audio

- `audio::render` and `render_with_soundfont` produce one interleaved stereo
  buffer at the requested rate. Audio references are mixed **once**, before
  track plug-ins and bus routing.
- The render is deterministic for the same song, sample rate and loaded
  SoundFonts. Permanent test plug-ins have no hidden state.
- Mixing applies track gain, pan, mute/solo, automation, plug-in chains, sends,
  buses and master gain in that order. The master limiter is opt-in.
- Offline code may allocate. The cpal output callback must not block; it copies
  from a prepared buffer.
- Playback opens a stereo output configuration and resamples the render to the
  device rate. It does not assume 48 kHz or a stereo device.

## MIDI and devices

- MIDI channel values are stored zero-based in `music-core` and displayed
  one-based. Drums are channel 10 on display.
- A MIDI schedule is ordered by time. A failed send never silently leaves
  notes held; all-notes-off is sent for every used channel.
- Device profiles are data, not code. Malformed profiles are reported and
  skipped.
- Yamaha XG parameter changes carry no checksum; only bulk dumps do.

## Plug-ins

- `plugin-host` is the only crate with `unsafe` code. Every `unsafe` block
  documents the C ABI contract it relies on.
- A loaded VST3/AU object must outlive its module, host application and COM
  pointers. Failed construction releases every acquired resource.
- Scanning, hosting and rendering are separate steps. A plug-in that fails to
  load is skipped; it may not take down the render.
- Plug-in state is opaque bytes owned by the slot; the host never interprets
  it.

## Score

- Score layout and PDF are separate from the desktop canvas. Both consume the
  same positioned notes, beams, ties, slurs and marks.
- The vector renderer remains available if the Cecilia preview fails.
- Printing shells out to `lp`; it is not an OS print panel.

## MCP and assistant

- The MCP socket binds to 127.0.0.1 only and requires a random per-start token.
  Tokens come from the operating-system CSPRNG and are compared in constant
  time.
- Tool arguments are untrusted. File paths and plug-in roots are checked; a
  malformed argument is an error, not a panic.
- One tool call is one undoable command where the operation is a single edit.
- Secrets never appear in process arguments. Settings and keys live under the
  per-user data directory, not the current working directory.

## UI

- The egui update loop never blocks on network, subprocess or full-song render.
  Long work runs on a worker thread and reports back through a channel.
- Opening a project stops the previous player. A paused player is rebuilt
  before resuming so edits are reflected.
- A failed open shows the error and clears the saved path; it never falls back
  to the demo while keeping the old path.
