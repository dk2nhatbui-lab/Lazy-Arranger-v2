# Stage 0 source inventory

Inventory of the C++ application and the two reference DAWs, for the Rust rewrite. It synthesises the
five notes in `docs/audit/notes/`; nothing new was researched. Spot checks against the repositories were
made on 2026-11-22 and are marked where they add a fact. C++ paths are relative to
`/Users/far-mini/repos/arranger-midi-daw`. Reference paths are relative to `.reference/<repo>`.
A bare line number or range after a full path continues the file named most recently in the same
paragraph, bullet or table row.

## 1. Pinned identities

| Repository | Location | Commit | Branch | Commit date | Languages | Size | Build | Verified |
|---|---|---|---|---|---|---|---|---|
| C++ app `arranger-midi-daw` | `/Users/far-mini/repos/arranger-midi-daw` | `8ca41644a2be5f5677bce8cae060a55f7b19e8fc` | `fix/audit-critical-high` | 2026-11-22 12:01:41 +0100 | C++20; no `.c` under `src/` | 71,723 LOC, 226 files under `src/` | CMake + FetchContent | 2026-11-22 |
| Cadenza | `.reference/cadenza` | `6bd80233c20482eba53167233f065039ea42fc36` | `dev` | 2026-11-01 17:23:33 -0500 | Rust 2024, MSRV 1.88 | 194,959 non-test LOC; 489 `.rs` files | Cargo, resolver 3, 17 members | 2026-11-22 |
| Vibez | `.reference/vibez` | `1d2c708ffedfbe03256e1cc9003c57be417cdfcd` | `dev` | 2026-11-21 00:28:29 +0000 | Rust 2021 | 235,802 non-test LOC; 955 `.rs` files | Cargo, resolver 2, 10 members | 2026-11-22 |
| This repository | `.` | staged in commits `21f3b8f` and `a36137a` on `master` | `master` | 2026-11-22 | Rust 2024, MSRV 1.88 | 7,884 LOC, 21 `.rs` files | Cargo, resolver 3, 7 crates + 1 app | 2026-11-22 |

The audited commit is the tip of `fix/audit-critical-high`, and it is **10 commits ahead of `origin/main`**
(`975e97d1004b8a9813339fddbc258c3bcf5eb6a5`). `git rev-list --left-right --count origin/main...HEAD`
returns `0 10`. The audit pinned the newest branch tip; `origin/main` lags it.

The C++ repository was audited **read-only**. It was never built, never run and never modified, and the
worktree was clean at audit time. No build artefacts were produced for this inventory.

LOC method: `wc -l` over `src/**/*.{cpp,h,hpp,cc,mm}`. The C++ total reproduces
`docs/audit/notes/cpp-surface.md:6` exactly, which confirms the counting rule.

## 2. C++ module inventory

| Layer | Files | LOC | Owns | JUCE |
|---|---|---|---|---|
| `src/core` | 103 | 34,573 | Song model, project file, tempo/time, edit functions, style SFF + chord rules, chord theory/detection/input, song forms, notation/layout/MusicXML/PDF, groove, arranging, hardware fit, device and controller profiles, synth editors, autosave, audio analysis, MCP server implementation | No; gated by `scripts/check-no-juce.sh:6` |
| `src/engine` | 16 | 3,171 | `Engine`, `Transport`, `Playback`, `Metronome`, `MidiEventBuffer`, `Handoff`. No threads of its own | No; same gate |
| `src/platform` | 4 | 695 | `src/platform/include/arranger/platform/DeviceHost.h` interface,
`src/platform/include/arranger/platform/MidiPanic.h`, `src/platform/juce/JuceDeviceHost.cpp/.h`: device callback, MIDI in/out, panic | Partly. Headers are JUCE-free; the implementation is JUCE |
| `src/app` | 98 | 32,643 | JUCE UI and `Session`/`MainComponent`, score, piano roll, mixer, clip grid, plugin host, audio import/export, TTS, localization, update check, profile loading | Yes; `juce_add_gui_app` at `src/app/CMakeLists.txt:28` |
| `src/soundfont` | 2 | 302 | FluidSynth behind the `Processor` interface | No; `src/soundfont/CMakeLists.txt:1` |
| `src/whisper` | 2 | 176 | whisper.cpp lyrics wrapper; model downloaded, not built in | No; `src/whisper/CMakeLists.txt:1-2` |
| `src/mcp` | 1 | 163 | `arranger-mcp` stdio helper | Yes; links `juce::juce_core` for the local socket, `src/mcp/CMakeLists.txt:1-11` |
| **Total** | **226** | **71,723** | | |

Two corrections to the notes:

1. The JUCE gate covers two layers, not four. `scripts/check-no-juce.sh:6` greps `src/core` and
   `src/engine` only; `docs/audit/notes/cpp-surface.md:7` says it also covers `src/soundfont` and `src/whisper`. Those two
   are JUCE-free by their build files, not by a test.
2. `src/core` carries the MCP server implementation; the `src/mcp` layer is the launcher only
   (`docs/audit/notes/cpp-surface.md:217`).

Largest core files: `src/core/src/McpServer.cpp` 4,423, `src/core/src/SongForm.cpp` 1,994,
`src/core/src/StyleFromAudio.cpp` 1,561, `src/core/src/SongEdit.cpp` 1,170,
`src/core/src/Groove.cpp` 1,160 (`docs/audit/notes/cpp-surface.md:228-229`).

## 3. Song model

### 3.1 The 24 structs

All 24 are in `src/core/include/arranger/core/Song.h`. 23 are saved; `SlotStatus` is runtime-only.

| Struct | Lines | Holds | Default / invariant |
|---|---|---|---|
| `Note` | 23-32 | start, length, pitch, velocity | start is clip-relative; 240 ticks = one 16th |
| `ControlEvent` | 35-45 | at, kind, number, value | 0..127; bend -8192..8191 |
| `Clip` | 52-64 | id, name, start, length, notes, controls, `loopLength` | length 15360 = 16 quarters; notes sorted by (start, pitch) |
| `AudioClip` | 72-88 | id, name, file, start, length, offset, gain, fades, bpm, layer | audio stays in its file; higher layer wins on overlap |
| `AutomationPoint` | 93-99 | at, value | 0..127; volume 100 = unity, pan 64 = centre |
| `ControlLine` | 126-133 | control, name, points | one line per lane |
| `BusSend` | 136-142 | bus, level | post-fader 0..127 |
| `PluginSlot` | 145-156 | format, id, name, state, bypass, automation | empty iff `id` empty; `state` is opaque base64 |
| `TrackOutput` | 160-169 | kind, port, channel | kinds none/soundFont/midiPort/sampler/audio/plugin; channel 1..16 |
| `TrackSound` | 172-180 | name, bankMsb, bankLsb, program | -1 = do not send that byte |
| `KitPad` | 183-197 | name, note, sample file, slice, gain, pan, tune, choke, oneShot | nonzero choke groups cut each other |
| `KitSettings` | 200-214 | name, pads | first pad for a note wins; no duplicate check |
| `MixerStrip` | 217-227 | volume, pan, reverbSend, chorusSend, mute, solo | MIDI-scaled to map 1:1 to CC7/10/91/93 |
| `ScoreMark` | 230-239 | kind, at, text | `text` is the chord symbol, lyric, dynamic or articulation |
| `Track` | 241-333 | id, name, colour, output, sound, kit, mixer, armed, inputs, clips, slots, styleChannel, marks, audio, audioSlots, 4 automation lines, instrument, effects, bus, sends | clips sorted by start; `slots` indexed by scene |
| `MetronomeSettings` | 335-342 | enabled, level, accentDownbeat | — |
| `LoopRange` | 344-352 | enabled, start, end | valid iff `end > start && start >= 0` |
| `MasterStrip` | 355-360 | gainDb, effects | — |
| `Bus` | 368-379 | id, name, colour, volume, pan, mute, effects | same number space as tracks |
| `SongSection` | 382-390 | name, start, end, scene | `scene >= 0` = clip-grid row, not Timeline |
| `FormPart` | 404-413 | section, bars, muted, into, label | `muted` holds **track ids**; bars 0 = whole section |
| `SongForm` | 416-423 | name, parts, ending | — |
| `Song` | 428-458 | title, tempo, timeSignature, key, guitarChords, tracks, metronome, loop, master, buses, scenes, launchQuantise, soundFont, nextId, style, sections, forms | `operator==` includes `nextId` |
| `SlotStatus` | 461-467 | playing, pending, launchTick | runtime only; never saved |

Two more structs live outside `src/core/include/arranger/core/Song.h`:
`StyleData` (`src/core/include/arranger/core/StyleRules.h:73-81`) and `StyleChannelRule` (`:35-52`).
`AutomationLine` is a `using` alias (`src/core/include/arranger/core/Song.h:101`) and `Transition` is an
enum (`src/core/include/arranger/core/Song.h:392-401`), so neither counts as a struct.

### 3.2 Identity and ids

| Question | Answer | Evidence |
|---|---|---|
| Id type | `TrackId` and `ClipId` are both `uint32_t`; the compiler cannot separate them | `src/core/include/arranger/core/Song.h:17-18` |
| Allocation | one monotone `Song::nextId` through `edit::takeId` | `src/core/src/SongEdit.cpp:45-48` |
| Consumers | tracks 369-372, clips 501-502, slots 532-533, buses 411, audio clips 269-270, audio slots 329-330 | `src/core/src/SongEdit.cpp` |
| Scope | one global space. Buses share the track number space; audio clips share the MIDI clip space | `src/core/include/arranger/core/Song.h:370`; `src/core/include/arranger/core/SongEdit.h:57-59` |
| Lookup | linear scans only: `findTrack`, `findClip`, `trackOfClip`, `findAudioClip`, `sceneOfClip` | `src/core/src/SongEdit.cpp:50-64,91-115,348-367,134-145,627-638` |
| Repair on load | ids `0`/duplicate renumbered from `nextId`; `nextId` raised to `max(maxId+1, saved)`, so no id is reused | `src/core/src/ProjectFile.cpp:833-921,851` |
| What breaks | anything holding a vector index (`ClipRef`, `sceneOfClip`, slot position) or a stale id (form `muted`) | `src/core/include/arranger/core/SongEdit.h:15-21` |

### 3.3 Timing

| Item | Value | Evidence |
|---|---|---|
| Tick | `std::int64_t` | `src/core/include/arranger/core/Time.h:10` |
| PPQ | `kTicksPerQuarter = 960` | `src/core/include/arranger/core/Time.h:13` |
| PPQ in the file | **None.** No `ppq`/`ticksPerQuarter` key in reader or writer; the constant is used once as a length default | `src/core/src/ProjectFile.cpp:484`; spot check: grep finds only that line |
| Tempo | sorted `TempoPoint{Tick at; double bpm}`, always a point at 0, **step changes only**, 20..400 BPM, NaN → 120 | `src/core/include/arranger/core/TempoMap.h:10-24`; `src/core/include/arranger/core/Time.h:15-16`; `src/core/src/Time.cpp:8-13` |
| Meter | one `TimeSignature` for the whole song; no meter map | `src/core/include/arranger/core/Song.h:432`; `src/core/include/arranger/core/Time.h:20-33` |
| tick → seconds | `startSeconds[i] + (tick - at_i) * 60/(bpm*960)`, `double`, integrated from song start | `src/core/src/TempoMap.cpp:79-83` |
| tick → sample | `anchorSample + (int64)ceil((secondsAt(tick) - anchorSeconds) * rate - 1e-6)` | `src/engine/src/Transport.cpp:8-16,15` |
| MIDI export / import | export division 960; import rescales with `(t*960 + division/2)/division` | `src/core/src/MidiFile.cpp:637,174` |

Three rounding rules coexist: `ceil(x-1e-6)` in the engine, `llround` in
`src/core/src/AudioToMidi.cpp:196`, `src/core/src/AudioAnalysis.cpp:611`,
`src/app/ui/MainComponent.cpp:2113-2114`, `src/app/ui/TimelineViews.cpp:2305,2377`,
`src/app/Session.cpp:953`, and truncation in `src/app/ui/TimelineViews.cpp:1598`. `toBarBeat` silently
treats an invalid signature as 4/4 and always labels a 240-tick unit "sixteenth" (`src/core/src/Time.cpp:22-49`).

### 3.4 Chord storage

| Layer | Representation | Evidence |
|---|---|---|
| Model and file | free-text symbol in `ScoreMark{kind=chord, at, text}` | `src/core/include/arranger/core/Song.h:230-239`; JSON sample `src/core/src/ProjectFile.cpp:434-441` |
| Runtime | `Chord{int root; ChordType type; int bass}`, 35 Yamaha types | `src/core/include/arranger/core/StyleChord.h:13-34` |
| Text → value | `style_chord::parse`, then `theory::fromRoman` | `src/core/include/arranger/core/StyleChord.h:39,46`; `src/core/include/arranger/core/Theory.h:71`; `src/core/src/ChordInput.cpp:227-241` |
| Chord lane | derived on demand from one track's marks; two marks at one tick = last wins | `src/core/src/ChordInput.cpp:187-201` |
| Song key | free text `Song::key`; unparsable means C major | `src/core/include/arranger/core/Song.h:433`; `src/core/include/arranger/core/Theory.h:44` |

Chord text is the only durable form. `Chord → name → Chord` is asserted by no test
(`docs/audit/notes/cpp-song-model.md:115`).

### 3.5 Project file

Envelope: `{"format": "arranger-midi-daw-project", "version": 4, "song": {...}}`. Format must match
exactly (`src/core/src/ProjectFile.cpp:694-695`); `version < 1` and `version > 4` are refused (`697-701`); the writer
always emits 4 (`617`). `kVersion = 4` is at `src/core/include/arranger/core/ProjectFile.h:28`; the header
comment above it still shows `"version": 1` (`:13-24`) and is stale.

| Version | Commit | Schema delta |
|---|---|---|
| 1 | `62251ea` | title, scalar `tempoBpm`, `timeSignature`, key, tracks without clips, metronome |
| 2 | `ec0efc3` | tempo becomes `[[tick,bpm],…]`; adds loop, soundFont, master, nextId, track id/output/sound/mixer/clips |
| 3 | `9386141` | clip `loopLength`; track `kit` |
| 4 | `1878ea0` | scenes, launchQuantise, style, sections, track styleChannel |

**Version 4 is a floor, not a schema id.** 21 later commits added keys (`audio`, `audioSlots`, `marks`,
`automation`, `buses`, `sends`, `effects`, `instrument`, `audioInput`, `forms`, `bus`) without bumping
`kVersion` (`docs/audit/notes/cpp-song-model.md:137-139`). A v4 file in the wild may contain any subset, so a reader must
default everything it does not find.

Migration code exists only for v1 (`midiChannel` → `output.channel`, `src/core/src/ProjectFile.cpp:535-536`; scalar
`tempoBpm` → `TempoMap`, `730-733`). There is no v2→v3 or v3→v4 code; old files gain keys through
defaults only.

Reader coercions in `src/core/src/ProjectFile.cpp` matter for a port:

| Behaviour | Evidence |
|---|---|
| Unknown keys at any level are ignored and **dropped on re-save** | `:30-44` |
| Unknown enum strings coerce silently: output kind → soundFont, control kind → controller, mark kind → chord | `134-142,156-162,517-519` |
| Wrong type for a known scalar → `Error("Field … has the wrong type")`, but inside `clipFromJson` a raw `get<Tick>()` escapes as `json::exception` | `30-44,924-927` |
| Load repairs are lossy: invalid loop resets the whole range, empty marks dropped, bad sections dropped, bus routes repointed | `745-752,522-523,802,852-876` |

Save is `path + ".saving"`, flush, `fs::rename` (`src/core/src/ProjectFile.cpp:959-1001`).

## 4. Edit path

`Document::edit(description, change, mergeKey)` is the atomic unit (`src/core/include/arranger/core/Document.h:29`).
One call stores one full `Song` snapshot: `Step{Song before; std::string description; uint64 revisionBefore}`
(`src/core/include/arranger/core/Document.h:53-58`).

| Property | Behaviour | Evidence |
|---|---|---|
| Change records | **None.** `SongEdit` is free functions that mutate `Song&` | `src/core/include/arranger/core/SongEdit.h:46,52,137,191` |
| Transactions / inverses | **None.** Undo restores snapshots, not inverse operations | `src/core/include/arranger/core/Document.h:29,53-58` |
| No-op edits | deep-compared away with `if (next == current) return;` | `src/core/src/Document.cpp:12` |
| Merge rule | same non-empty `mergeKey` AND history non-empty AND redo empty: no new step, first description kept | `src/core/src/Document.cpp:15-25` |
| Gesture keys | the UI embeds a per-gesture counter (`"master-<gesture>"`) and ends with `Session::endMerge()` | `src/app/ui/MixerView.cpp:707,736`; `src/app/Session.h:60` |
| History cap | 200 steps, oldest erased | `src/core/include/arranger/core/Document.h:24`; `src/core/src/Document.cpp:19-20` |
| Cost | full `Song` copy plus full deep compare per edit; `nextId` is part of equality, so an id-consuming edit counts as a change | `src/core/include/arranger/core/Song.h:457`; `src/core/src/Song.cpp:8-51` |
| Memory | clip payloads are shared and copy-on-write, but `PluginSlot::state`, automation vectors and `Track` structs are deep-copied per step | `src/core/include/arranger/core/Song.h:66,90`; `src/core/src/SongEdit.cpp:645-647` |
| App coupling | `Session::edit` wraps the document call, then prunes selection and republishes to the engine; 141 `session.edit(...)` sites in `src/app` | `src/app/Session.cpp:56-63,98-118` |

Failures are silent no-ops, not errors: a missing track returns `0`/`void`
(`src/core/src/SongEdit.cpp:91-102,264-267,496-500`), `moveClip` to a non-audio track falls back to the
source (`669-670`), and `setSlot` does not check `output.kind` (`527-541`) while `setAudioSlot` does (`327`).

## 5. Real-time engine and MIDI

### 5.1 Callback and clock ownership

| Fact | Evidence |
|---|---|
| The audio device owns the sample clock; JUCE calls `JuceDeviceHost::audioDeviceIOCallbackWithContext` once per device block | `src/platform/juce/JuceDeviceHost.cpp:377-379` |
| That callback calls `Engine::process(AudioBlock&, MidiEventBuffer&, MidiEventBuffer&, const AudioBlock&)` | `src/engine/include/arranger/engine/Engine.h:295,298`; `src/engine/src/Engine.cpp:774,811` |
| `Transport` owns the musical clock: `sampleTime`, `anchorSample`, `anchorTick`, `anchorSeconds` | `src/engine/include/arranger/engine/Transport.h:53-56` |
| `sampleTime` counts samples since play started, not song time; `play()` zeroes and re-anchors it | `src/engine/src/Transport.cpp:55-62` |
| Time advances only while playing | `src/engine/src/Transport.cpp:111-115` |
| Block size is whatever the device gives; the engine prepares for `maxBlockSize` and degrades when a block is larger (racks disabled, monitor skipped) | `src/engine/src/Engine.cpp:161-195,851-873,800-801` |
| No fixed internal block size and no sub-block split except loop wrap | `src/engine/src/Engine.cpp:888-952` |

### 5.2 Tick conversion

Forward (`src/engine/src/Transport.cpp:8-16`):
`anchorSample + (int64)ceil((secondsAt(tick) - anchorSeconds) * sampleRate - 1e-6)`. The result is absolute,
computed from song start through the tempo map, never summed per block. Rounding is
ceiling, so an event is never early and at most one sample late; the `1e-6` sample epsilon keeps an exact
sample exact. Reverse (`src/engine/src/Transport.cpp:18-23`) returns a `double` tick.

Boundary cases that are undefined or only accidentally consistent:

1. A tick before the anchor returns a sample below `anchorSample`; nothing clamps it (`src/engine/src/Engine.cpp:356`).
2. The epsilon disagrees with the test oracle by up to one sample; the test allows ±1
   (`tests/TimingTests.cpp:20-24,71-75`).
3. A loop period is a difference of two ceils, so it can differ by ±1 sample per tempo segment; the test
   asserts `<= 1` (`tests/TimingTests.cpp:98-105`).

### 5.3 Scheduling

- **Zero look-ahead.** A span is scheduled only when it is rendered, bounded by `tickAtStart`/`tickAtEnd`
  (`src/engine/src/Engine.cpp:891-934`). The only ahead-of-time work is the output delay in the host.
- **Order key.** `SequenceEvent::order`: 0 note-off, 1 controls/setup, 2 note-on
  (`src/engine/include/arranger/engine/Playback.h:26`). Build sort is `(at, order)` with `std::stable_sort`
  (`src/engine/src/Playback.cpp:253,267-268,332`).
- **Final block sort.** `events.sortByTime()` is a stable insertion sort by `sampleOffset` only
  (`src/engine/include/arranger/engine/MidiEventBuffer.h:73-88`; called at `src/engine/src/Engine.cpp:956`). Equal offsets
  therefore order by track index, then `(at, order)`.
- **Note ownership is per track.** `ActiveNotes` keeps a `bitset<128>`, the channel and the port captured at
  note-on (`src/engine/src/Engine.cpp:360-381`). A same-pitch retrigger sends note-off first (`365-367`); a note-off for a
  pitch this track does not hold is dropped (`372-376`); a boundary or stop releases every held pitch and
  lifts CC64 (`453-466,566-573`); stop also sends CC123 (`439-451`).

### 5.4 Threads and queues

| Thread | Created at | Role |
|---|---|---|
| JUCE audio device thread | `src/platform/juce/JuceDeviceHost.cpp:148` | Callback, `Engine::process`, `Transport` |
| "MIDI out" `Sender` (high priority) | `src/platform/juce/JuceDeviceHost.cpp:12,149` | Pops timed hardware MIDI, sorts, sends, sleeps 1 ms |
| JUCE MIDI input threads | one per enabled device, `src/platform/juce/JuceDeviceHost.cpp:147` | Timestamps and queues input |
| UI/message thread | `src/app/Session.cpp:21` (30 Hz timer) | Publishes snapshots, garbage collection, input policy |
| Plugin / SFX / AI / whisper | `src/app/*` | Outside the RT path |

| Link | Type | Capacity | Overflow behaviour |
|---|---|---|---|
| UI → audio commands | `SpscQueue<Command,1024>` (`src/engine/include/arranger/engine/Engine.h:367`) | 1023 | push result ignored (`src/engine/src/Engine.cpp:29-32`) |
| Audio → UI recorded MIDI | `SpscQueue<RecordedEvent,16384>` (`src/engine/include/arranger/engine/Engine.h:368`) | 16383 | ignored (`src/engine/src/Engine.cpp:652`) |
| Audio → UI slot changes | `SpscQueue<SlotChange,1024>` (`src/engine/include/arranger/engine/Engine.h:373`) | 1023 | ignored (`src/engine/src/Engine.cpp:575`) |
| Snapshot / instrument / racks / samplers | `Handoff<T>` (`src/engine/include/arranger/engine/Engine.h:369-372`) | 1 current + 1 previous | whole-object publish; old objects freed by `collectGarbage` (`src/engine/include/arranger/engine/Handoff.h:25-38,74-81`) |
| Engine → host MIDI | `SpscQueue<TimedMessage,8192>` (`src/platform/juce/JuceDeviceHost.h:106`) | 8191 | ignored (`src/platform/juce/JuceDeviceHost.cpp:421`) |
| MIDI thread → audio | `vector<LiveInput>` + SpinLock (`src/platform/juce/JuceDeviceHost.h:92-93`) | 1024 | beyond 1024 dropped (`src/platform/juce/JuceDeviceHost.cpp:331`) |
| Audio input → UI take | `AudioCapture` ring (`src/engine/include/arranger/engine/AudioCapture.h:21`) | 2^18 frames, ~5.5 s at 48 kHz | newest dropped and counted (`src/engine/include/arranger/engine/AudioCapture.h:39-40`) |
| Per-block event buffers | `events` 8192, `instrumentIn` 4096, `samplerIn` 4096, `scratch` 256 (`src/engine/include/arranger/engine/Engine.h:387-388`) | fixed | silent note loss |

### 5.5 Deadline hazards on the callback path

| Hazard | Location | Consequence |
|---|---|---|
| `Handoff::acquire()` is a `for(;;)` spin of two atomics | `src/engine/include/arranger/engine/Handoff.h:48-62` | Unbounded loop; livelock needs two publishes between two loads and was not observed |
| `while (auto c = commands.pop()) apply(*c);` | `src/engine/src/Engine.cpp:875-876` | Up to 1024 commands, each O(state), drained in one block |
| `apply(panic)`: 16×128 loop plus three controller passes | `src/engine/src/Engine.cpp:230-242` | Thousands of iterations in one block |
| `sendToTrackChannels` scans all earlier tracks per track | `src/engine/src/Engine.cpp:465-479` | O(n²) on stop/panic; ~500k comparisons for 1000 tracks. A 1000-track test exists (`tests/EngineStressTests.cpp:218-228`) |
| Slot and span loops capped at `guard < 64` | `src/engine/src/Engine.cpp:423,891` | Bounded, but the remaining span is silently not rendered |
| `MidiEventBuffer::add` drops and counts on overflow | `src/engine/include/arranger/engine/MidiEventBuffer.h:60-69` | `droppedCount()` is never read in production |
| Queue push results ignored (commands, recorded, slot changes, hardware out) | `src/engine/src/Engine.cpp:29-32,575,652`; `src/platform/juce/JuceDeviceHost.cpp:421` | Silent loss, including recorded takes and hardware note-offs |
| System clock read twice per block; `liveLock.tryEnter()` | `src/platform/juce/JuceDeviceHost.cpp:377,381,386,390` | Syscall on the RT thread; input deferred by up to one block |
| Metronome `sin`/`exp` per sample | `src/engine/src/Metronome.cpp:47-59` | Transcendental math on the audio thread |
| `Engine::prepare` takes the `Handoff` mutex | `src/engine/src/Engine.cpp:168-190` | Documented not real-time safe; called from device start, not the callback |

The allocation claim is narrower than stated: `tests/RealtimeTests.cpp:82` calls the 3-arg `process`, so the
monitor and audio-capture preamble (`src/engine/src/Engine.cpp:774-830`) is not covered by the operator-new counter, and
the test is excluded from sanitizer builds (`tests/CMakeLists.txt:94-95`).

### 5.6 MIDI I/O and SMF, in brief

| Fact | Evidence |
|---|---|
| Input timestamp is a wall-clock millisecond value mapped into the next block: `offset = clamp(numSamples-1-int(ageSamples), 0, numSamples-1)` | `src/platform/juce/JuceDeviceHost.cpp:324,388-391` |
| SysEx, messages over 3 bytes, active sensing and clock never reach the engine | `src/platform/juce/JuceDeviceHost.cpp:293-305` |
| Hardware output resolves at ~1 ms through the `Sender` thread, plus driver timing | `src/platform/juce/JuceDeviceHost.cpp:70-100` |
| The engine has no SysEx path at all (3-byte `MidiEvent`) | `src/engine/include/arranger/engine/MidiEventBuffer.h:11-24` |
| SMF: format 0/1 only, division 960, one meter at tick 0, tempo rounded to integer µs, no markers, no SysEx import | `src/core/src/MidiFile.cpp:168-175,486-641` |

## 6. Style system

### 6.1 What exists

| Piece | State | Evidence |
|---|---|---|
| SFF1/SFF2 container | read and write, first `MTrk` only, division normalised to 1920 | `src/core/src/StyleFile.cpp:345-359,664-672` |
| Part identity | `StylePart` = 8 parts, channel = `9 + part` | `src/core/include/arranger/core/Style.h:19-23` |
| Section identity | string name, matched literally; 17 Yamaha names | `src/core/include/arranger/core/Style.h:26-31` |
| Section length | implicit: distance to the next marker or end of track | `src/core/src/StyleFile.cpp:381-403` |
| Chord rules | `StyleRuleSet` → `StyleChannelRule` with `sourceRoot`, `sourceChordType`, note/chord mute masks, three transpose ranges | `src/core/include/arranger/core/StyleRules.h:35-52` |
| Transposition spec | pure `style_chord::transpose(pitch, rule, chord)`, eight ordered steps, outputs `nullopt` or 0..127 | `src/core/src/StyleChord.cpp:318-398` |
| Part gating | `style_chord::plays(rule, chord)`, called once per clip; a false result clears the whole clip | `src/core/src/StyleChord.cpp:309-316`; `src/core/src/StyleConvert.cpp:509-510` |
| Default channel rules | drums 9/10 bypass; bass `melody`+`bass`+transposing; others `rootFixed`+`chord`; source CMaj7 | `src/core/src/StyleConvert.cpp:38-65` |
| Sections in playback | a section is a scene; launching it is generic clip launch, bar-quantised | `src/engine/src/Engine.cpp:278-289,326-350` |
| Chord following | whole-clip re-render: copy the song, transpose every slot note, publish a new snapshot | `src/core/src/StyleConvert.cpp:494-527`; `src/app/Session.cpp:146-159` |

### 6.2 What does not exist

| Missing piece | Reality |
|---|---|
| Variation / fill / break state machine | **None.** No such object in `src/core` or `src/engine` |
| Fill precedence, fill-and-return, break length | None. `"Fill In AA"` is a one-bar demo scene that repeats every bar |
| Phrase entity | None; "Phrase 1/2" are part names only (`src/core/include/arranger/core/Style.h:19-22`) |
| Chord timeline | One chord at a time; `setChord` publishes immediately, unquantised (`src/app/Session.h:504`; `src/app/Session.cpp:1090-1098`) |
| RTR behaviour | Stored and edited, never applied (`src/core/include/arranger/core/StyleRules.h:25`; `src/app/ui/StyleRuleEditor.cpp:64`) |

The only fill logic is offline: `SongForm` picks a fill by section energy and copies a rhythm-part fill into
the bar before a join (`src/core/src/SongForm.cpp:1418-1444,1570-1592`); its "break" is `Transition::stop`,
one generated beat of silence (`src/core/src/SongForm.cpp:1152-1169`;
`src/core/include/arranger/core/Song.h:392-401`).

### 6.3 CASM status (`src/core/src/StyleFile.cpp`)

| Aspect | State |
|---|---|
| Read | `readCasm` parses `CASM`→`CSEG`→`Sdec`/`Ctab` (SFF1) or `Ctb2` (SFF2) |
| Write | `CASM` is written **only when `ruleSets` is non-empty** (`:671-672`) |
| Preservation | other `CSEG` children go to `StyleRuleSet::extra` (`:288-293`); other `CASM` children are **dropped** (`:258-263`); everything after `CASM` is kept verbatim in `Style::extra` (`:540-547`) |
| Round trip | re-serialisation, not byte preservation: division rescales to 1920, `Sdec` rewritten, events reordered by class |
| Fixtures | one hand-built CASM writer fixture (`tests/StyleFileTests.cpp:17-121`) and one hand-built `Ctb2` byte array (`:503-545`); **no real Yamaha style** |
| Mute-mask polarity | assumed (bit set = may play), not verified against a real file (`docs/audit/notes/cpp-style-arranger.md:122,360`) |

### 6.4 The demo style fixture

`examples/demo-style.sty`, 3,940 B: `MThd` (division 1920) + one `MTrk` (3,918 B). No `CASM`, `CSEG`,
`Sdec` or `OTSc`; the chunk directory ends at 3,940 B. Markers `SFF2`, `SInt`, `Main A`..`Main D`,
`Fill In AA`. Geometry: setup bar 1 bar (7,680 ticks), mains 4 bars each (30,720), fill 1 bar (7,680),
track end 138,240. Channels 10-15; the material is C natural minor.

Under the default rules the conversion is the identity at root C for every note in the file, so the shipped
demo exercises only the identity path and cannot catch a wrong chord-tone table
(`docs/audit/notes/cpp-style-arranger.md:204-210`, marked (D)).

## 7. Tests and fixtures

| Count | Value | Method |
|---|---|---|
| `.cpp` files in `tests/` | 74 | `ls tests/*.cpp` |
| `TEST_CASE` | 480 | `grep TEST_CASE tests/*.cpp` |
| `SCENARIO` | 0 | no BDD macros |
| Files with tests / harness files | 72 / 2 | `tests/CrashReport.cpp`, `tests/PluginTestsMain.cpp` |
| CMake test targets | 9 | `arranger_tests`, `arranger_realtime_tests`, `arranger_fuzz`, `arranger_plugin_tests`, `arranger_localization_tests`, `arranger_async_tests`, `arranger_audit_app_tests`, `arranger_media_tests`, `arranger_audit_ui_tests` |

Golden format: score snapshots are JSON from `describe()` (`tests/ScoreSnapshotTests.cpp:34`). Per page:
`size [w,h]` plus four sorted arrays — `glyphs [[codepoint,x,y,scale],…]` at 2 dp, `lines`,
`shapes`, `texts`. Numbers are rounded and lists sorted, so a diff shows what moved; comparison is
tolerant, and `ARRANGER_UPDATE_GOLDEN=1` rewrites the files (`tests/ScoreSnapshotTests.cpp:24-68,80-133`,
`tests/RenderTests.cpp:256-262`). The render baseline stores `firstOnsets` and `windows[{rms,peak}]` per 0.25 s,
not a waveform or MIDI bytes (`tests/RenderTests.cpp:168-198`).

What the timing tests pin:

| Test | File:line | Assertion |
|---|---|---|
| Note dispatch through 30 tempo changes | `tests/TimingTests.cpp:48-76` | per note `abs(got-expected) > 1` counts as off; `CHECK(off == 0)` |
| Loop repeats sample-exactly | `tests/TimingTests.cpp:78-108` | `abs(delta - loopSamples) <= 1` |
| Bar-quantised grid launch | `tests/TimingTests.cpp:110-143` | exact samples 96000, 120000, 192000 |
| Long song at an odd rate | `tests/TimingTests.cpp:145-174` | 600 beats at 97.3 BPM, 44.1 kHz, `<= 1` sample |
| Metronome over one hour | `tests/MetronomeTests.cpp:48-66` | `CHECK(worst == 0)` at 112.37/97/133.33 BPM × 44.1/48/96 kHz |
| Buffer-size independence | `tests/MetronomeTests.cpp:68-82` | identical click samples and bit-identical audio, 64 vs random |
| Click starts on the beat sample | `tests/MetronomeTests.cpp:84-94` | `audio[23999] == 0`, `audio[24000] != 0` |
| No allocation on the audio thread | `tests/RealtimeTests.cpp:88-118` | `audioAllocations == 0` over 2300+ blocks |
| Mix is buffer-size independent | `tests/RenderTests.cpp:201-224` | `maxDifference < 1e-6` for 64 vs 512 vs random |

Parity fixtures that exist:

| Path | Size | Role |
|---|---|---|
| `examples/demo-style.sty` | 3,940 B | the only style file; `MThd`+`MTrk`, no CASM |
| `tests/fuzz/regressions/style/note-number-over-127.bin` | 4,520 B | style-like file that must be refused or read without crashing |
| `tests/golden/score-ties-accidentals.json` | 4,654 B | layout golden |
| `tests/golden/score-beams-triplets.json` | 4,548 B | layout golden |
| `tests/golden/score-grand-staff-marks.json` | 3,188 B | layout golden |
| `tests/golden/score-drums.json` | 2,954 B | layout golden |
| `tests/golden/score-six-eight.json` | 1,982 B | layout golden |
| `tests/golden/score-long.json` | 52,751 B | 3-page system/page-break golden |
| `tests/golden/render-reference-song.json` | 1,180 B | render baseline |
| `examples/demo.arranger` | 26,085 B | project file, version 3 |
| `examples/Copper Lights/Copper Lights (Audio).arranger` | 9,603 B | project file, version 4 |
| `examples/Copper Lights/Copper Lights (MIDI).arranger` | 775,525 B | project file, version 4 |
| `examples/Northern Fire/Northern Fire (Audio).arranger` | 7,835 B | project file, version 4 |
| `examples/Northern Fire/Northern Fire (MIDI).arranger` | 622,387 B | project file, version 4 |
| `tests/fixtures/tone.mp4` / `.mkv` / `.webm`, `silent-video.mp4` | 7,331 / 49,151 / 6,503 / 893 B | video only; not parity |

The five example-project sizes are measured with `stat` on 2026-11-22; the notes record the other sizes.
No golden pins transposed style output, and no real Yamaha style ships in the repo
(`docs/audit/notes/cpp-style-arranger.md:305-307`).

Three fixtures in this repository are byte-identical copies of the C++ examples:
`tests/fixtures/legacy-cpp-v3.arranger` = `examples/demo.arranger`,
`tests/fixtures/legacy-cpp-v4.arranger` = `examples/Copper Lights/Copper Lights (Audio).arranger`, and
`tests/fixtures/demo-style.sty` = `examples/demo-style.sty`. Provenance and licence consequences are in
`docs/audit/licensing.md` §1.

## 8. Reference repositories

| Repository | SHA | Language | Non-test LOC | Implemented | Not implemented |
|---|---|---|---|---|---|
| Cadenza | `6bd80233c20482eba53167233f065039ea42fc36` | Rust 2024, MSRV 1.88, 17 crates | 194,959 | Off-thread graph compile; RT player over two `rtrb` rings, 32-frame quantum; CLAP and VST3 hosting with a child-process sandbox; inverse-command undo with atomic `Batch`; 95 MCP tools; JSON persistence + sidecar media; 89k-line egui UI | `arc-swap` is declared and never used, so there is no `ArcSwap` graph publication; no single document+media artefact |
| Vibez | `1d2c708ffedfbe03256e1cc9003c57be417cdfcd` | Rust 2021, 10 members | 235,802 | Perform/Arrange/Mix modes; capture into ordinary clips; launch quantisation as data; clip launcher; 125k-line iced UI; cpal/midir/symphonia/rubato; in-process VST3+CLAP | No note quantisation by design; undo is snapshot-based and UI-only; no plugin crash isolation; multiple MIDI ports and generic `.vdc` mapping absent |

Both are GPL-3.0-or-later. Their code is unavailable to this MIT/Apache-2.0 repository; the licence
evidence and consequences are in `docs/audit/licensing.md` §2.

## 9. What is not verified

Taken from the notes' own unverified markers. None of these was resolved for this inventory. In the first
two tables the file is the note named in the heading; in the third, each row names its note.

### 9.1 Style and arranger

From `docs/audit/notes/cpp-style-arranger.md` §9.

| Claim | Why it is open | Probe |
|---|---|---|
| Note/chord mute-mask polarity (bit set = may play) | assumed from code and one hand-built fixture; no real file | one real PSR style with a known resting part, or Yamaha bit definitions (`:360`) |
| Yamaha chord type `0x22` = cancel | the app models it that way; `PsrStyle::chordTypes()` labels it "cc" | Yamaha Data List chord table (`:361`) |
| Real files never carry a second `MTrk` | the reader keeps the first and stores the rest in `extra` | corpus scan with a chunk walker (`:362`) |
| `editable` byte semantics beyond 0/1 | only `src/core/src/ProjectFile.cpp:267,310` reads it | histogram of byte 10 in real `Ctab`/`Ctb2` (`:363`) |
| `middleLow`/`middleHigh` equal Yamaha note limits | offsets match the fixture, semantics do not | split part compared note by note on hardware (`:364`) |
| Bar-quantised launch matches keyboard behaviour | the manual says only "starts at the subsequent bar" | listening test (`:365`) |
| Octave-up shift when `sourceRoot > highKey` | derived from `src/core/src/StyleChord.cpp:377-379`; no test | crafted rule plus unit test (`:366`) |
| Demo style conversion at root C is the identity | derived by reading (D marker), not executed | run the conversion (`:6,204-210`) |

### 9.2 Song model and project file

From `docs/audit/notes/cpp-song-model.md` §9.

| Claim | Why it is open | Probe |
|---|---|---|
| `Chord → name → Chord` stable for all 35 types | no test found | round-trip over `src/core/include/arranger/core/StyleChord.h:13-22` (`:344`) |
| Re-saving a v3 file is byte-stable | the writer is v4-only and adds keys | compare `examples/demo.arranger` before and after a C++ save (`:345`) |
| Engine behaviour for a tick before the anchor | `src/engine/src/Engine.cpp:356` only subtracts; nothing clamps | runtime test (`:346`) |
| A note past the clip end is suppressed at note-on | `src/engine/src/Playback.cpp:263-266` is read, not test-verified | runtime test (`:347`) |
| All 34 `.edit(` sites in `src/core/src/McpServer.cpp` reach `Document::edit` | 4 of the 34 were opened | read the rest (`:348`) |
| Nothing prunes `FormPart::muted`; cost of 200 undo steps with plugin state | read, not measured | `src/core/src/SongForm.cpp:1194+`; measure the snapshot described at `src/core/include/arranger/core/Document.h:53-58` and `src/core/include/arranger/core/Song.h:145-156` (`:349`) |

### 9.3 Engine, MIDI and licence checks

| Claim | Why it is open | Probe |
|---|---|---|
| JUCE persists MIDI input enable flags in its state XML | the design comment assumes it | restart test (`docs/audit/notes/cpp-rt-midi.md:293`) |
| Cost of `getMillisecondCounterHiRes()` on the RT thread | not measured | profile (`docs/audit/notes/cpp-rt-midi.md:293`) |
| `MidiOutput::sendMessageNow` on an unplugged device | not exercised | unplug test (`docs/audit/notes/cpp-rt-midi.md:293`) |
| The `Handoff` spin can livelock in practice | needs two publishes between two loads | stress test (`docs/audit/notes/cpp-rt-midi.md:293`) |
| Real-time costs are structural | no profiling data was read | profile (`docs/audit/notes/cpp-rt-midi.md:296`) |
| `tests/RealtimeTests.cpp` covers only the 3-arg `process` | the monitor and capture path is untested for allocation | instrument the 4-arg path (`docs/audit/notes/cpp-rt-midi.md:295`) |
| The JUCE-free claim is enforced by token grep, not link analysis | `scripts/check-no-juce.sh` only greps | link inspection (`docs/audit/notes/cpp-rt-midi.md:294`) |
| JUCE 8.0.15 terms; GeneralUser GS redistribution terms | upstream terms were not read | read upstream (`docs/audit/notes/cpp-surface.md:421-422`) |
| nlohmann/json, yaml-cpp, Catch2, `tone`, sherpa-onnx licence texts | upstream-known, not read in this repository | read upstream (`docs/audit/notes/cpp-surface.md:423`) |
| Cadenza and Vibez dependency licences | from `Cargo.toml`/`deny.toml` plus the crates.io API; `Cargo.lock` has no licence field | audit the lock file (`docs/audit/notes/refs-cadenza-vibez.md:5`) |
