# C++ song/project model and edit path — Rust migration audit

| | |
|---|---|
| Target | `/Users/far-mini/repos/arranger-midi-daw` at `8ca41644a2be5f5677bce8cae060a55f7b19e8fc` (read only; not built, not modified) |
| Scope | song model, project file, tempo/time, chord storage, edit path, tests, `examples/demo.arranger` |
| Method | full read of the in-scope sources; `python3` + `json` dump of the example project; `git log`/`git show` for version history. Test corpus: 74 `tests/*.cpp` files (`ls tests/*.cpp \| wc -l`) |

**Two corrections to the brief, up front**

1. The current project-file version is **4**, not 3 — `kVersion = 4` at `src/core/include/arranger/core/ProjectFile.h:28`. Version 3 survives only as data: `examples/demo.arranger:3`.
2. `SongEdit` is **not** a command/undo layer. It is free functions that mutate `Song&` in place; undo is a whole-song snapshot stack in `src/core/include/arranger/core/Document.h` (§6).

---

## 1. Entity model (`src/core/include/arranger/core/Song.h`)

`Song.h:425-427` states the contract: *"The song: everything that is saved in a project file."* All line numbers below are in that file unless stated.

### 1.1 Structs and defaults

| Struct (lines) | Fields (type = default) | Invariants / notes |
|---|---|---|
| `Note` (23-32) | `start: Tick=0`, `length: Tick=240`, `pitch: uint8=60`, `velocity: uint8=100` | `start` relative to clip start; `end()` = start+length; 240 = one 16th at PPQ 960 |
| `ControlEvent` (35-45) | `at: Tick=0`, `kind: {controller,pitchBend,program,channelPressure}=controller`, `number: uint8=0`, `value: int=0` | 0..127, or -8192..8191 for bend (42); playback clamps bend to 0..16383 (`src/engine/src/Playback.cpp:282`) |
| `Clip` (52-64) | `id: ClipId=0`, `name`, `start=0`, `length=15360`, `notes`, `controls`, `loopLength=0` | "immutable once shared; edits make a new copy" (47-51); `notes` sorted by start then pitch (58), `controls` by time (59); `loopLength>0` ⇒ first `loopLength` ticks repeat (60); 15360 = 16 quarters |
| `AudioClip` (72-88) | `id`, `name`, `file`, `start=0`, `length=3840`, `offset: double=0` (seconds into file), `gainDb=0`, `fadeIn/fadeOut=0`, `bpm=0` (>0 ⇒ follow tempo), `layer=0` | audio stays in its file (69-70); higher layer wins on overlap, equal layer ⇒ later start (84) |
| `AutomationPoint` (93-99), `AutomationLine` (101), `ControlLine` (126-133), `automation::valueAt` (106-122) | point `{at: Tick=0, value: float=100}`; line = `vector<point>`; control line `{control:int=0, name, points}` | 0..127, volume 100 = unity, pan 64 = centre (96); line "sorted by time; empty = no automation"; level before/after the ends, linear between |
| `PluginSlot` (145-156) | `format`, `id`, `name`, `state` (base64, empty = plugin defaults), `bypass=false`, `automation: vector<ControlLine>` | `empty()` ⇔ `id.empty()` (154); automation stays with the plugin when the slot moves (152) |
| `TrackOutput` (160-169), `TrackSound` (172-180) | output `kind: {none,soundFont,midiPort,sampler,audio,plugin}=soundFont`, `port`, `channel=1`; sound `name`, `bankMsb=-1`, `bankLsb=-1`, `program=-1` | 1..16; `audio` tracks have no MIDI, `plugin` uses `Track::instrument` (158-159); -1 = "don't send" |
| `KitPad` (183-197), `KitSettings` (200-214) | pad: `name`, `note=36`, `sampleFile`, `sliceStart=0`, `sliceEnd=0` (0 = end of file), `gainDb=0`, `pan=0`, `tuneSemitones=0`, `chokeGroup=0`, `oneShot=true`; kit: `name`, `pads` | nonzero choke groups cut each other (193); `padForNote` returns the first match, no duplicate check (205-211) |
| `ScoreMark` (230-239) | `kind: {chord,lyric,dynamic,articulation}=chord`, `at: Tick=0`, `text` | `text` is the chord symbol / lyric / dynamic / articulation word (236) |
| `MixerStrip` (217-227); `MetronomeSettings` (335-342); `LoopRange` (344-352); `MasterStrip` (355-360) | strip `{volume=100, pan=64, reverbSend=40, chorusSend=0, mute=false, solo=false}`; metronome `{enabled=true, level=0.7f, accentDownbeat=true}`; loop `{enabled=false, start=0, end=15360}`; master `{gainDb=0, effects}` | mixer values are MIDI-scaled to map 1:1 to hardware CC7/CC10/CC91/CC93 (216); `LoopRange::isValid()` = `end>start && start>=0` (350) |
| `Bus` (368-379) | `id: uint32=0`, `name="Bus"`, `colour="blue"`, `volume=100`, `pan=64`, `mute=false`, `effects` | "from the same numbers as the tracks" (370) |
| `SongSection` (382-390) | `name`, `start=0`, `end=0`, `scene=-1` | with a scene, `end-start` is the scene's play length (385); `scene>=0` ⇒ Clip-grid row, not Timeline |
| `FormPart` (404-413), `Transition` (393-401), `SongForm` (416-423) | part `{section, bars=0 (0=whole, >0 first bars, <0 last bars), muted: vector<TrackId>, into=automatic, label}`; form `{name="New Form", parts, ending=automatic}` | `muted` holds **track ids**; "Timeline stays as it is until one is used" (415) |
| `SlotStatus` (461-467) | `playing=-1`, `pending=kNone`, `launchTick=0` | **runtime only, never saved** |

### 1.2 The two big structs

| Struct (lines) | Fields (type = default) | Invariants / notes |
|---|---|---|
| `Track` (241-333) | `id=0`, `name="Track"`, `colour="teal"`, `output`, `sound`, `kit`, `mixer`, `armed=false`, `inputPort`, `inputChannel=0` (0=all), `audioInput=0`, `audioInputChannels=0`, `clips` (sorted by start, 255), `slots` (index=scene, null=empty, 256), `styleChannel=0` (1..16 = style source channel), `marks` (sorted by time, 258), `audio` (sorted by start, 259), `audioSlots` (index=scene, clip starts at 0 and loops, 260), 4 automation lines, `instrument`, `effects`, `bus=0`, `sends` (each `BusSend{bus: uint32=0, level=100}` 136-142, post-fader 0..127) | lane numbering 0 volume, 1 pan, 2 reverb, 3 chorus, from 4 the plugin control lines (instrument first, then each effect) (272-274); `laneCount`/`controlLine`/`findLine`/`mixerValue` are derived (275-332) |
| `Song` (428-458) | `title="Untitled"`, `tempo: TempoMap`, `timeSignature`, `key="C major"`, `guitarChords=false`, `tracks`, `metronome`, `loop`, `master`, `buses` (mixer order, 439), `scenes: vector<string>`, `launchQuantise=3840` (0 = at once), `soundFont`, `nextId=1`, `style: optional<StyleData>`, `sections` ("sorted by start", 445), `forms` | `baseTempo()` = `tempo.getPoints().front().bpm` (449) — UB if the map were empty, which `TempoMap` prevents; `sceneCount()` min 8 rows (`src/core/src/Song.cpp:66-79`); `contentEnd()` min one bar, ignores slots (`Song.cpp:53-64`); defaulted `operator==` (457) ⇒ **includes `nextId`** |

`StyleData` (`src/core/include/arranger/core/StyleRules.h:73-81`): `format=2`, `setup`, `ruleSets`, `sectionEvents: map<string, vector<StyleEvent>>`, `extra` — raw Yamaha payload "kept as they are". `StyleChannelRule` (35-52) carries the chord-following numbers: `sourceRoot`, `sourceChordType`, `noteMute: uint16=0x0fff`, `chordMute: uint64=0xffffffffff`, three `StyleTranspose` ranges.

### 1.3 Invariants that are *not* enforced

| Claim in a comment | Enforcement |
|---|---|
| `Track::marks` "sorted by time" (258) | `edit::setMark` sorts (`src/core/src/SongEdit.cpp:1110`), but load appends in file order (`ProjectFile.cpp:511-524`) |
| `Song::sections` "sorted by start" (445) | **never sorted** on load (`ProjectFile.cpp:792-804`) or in `SongEdit`; duplicate names/overlaps accepted from a file, only the UI validates (`form::problemWith`, `src/core/src/SongForm.cpp:676-694`) |
| Note inside its clip | not checked: negative `start` and `end() > clip.length` are storable. Playback truncates at the cycle end (`src/engine/src/Playback.cpp:266`), so data survives but is partly hidden |
| `FormPart::muted` track ids exist | not maintained: `edit::removeTrack` erases the track and leaves stale ids (`SongEdit.cpp:480-483`); inert because lookups miss |
| `Clip::loopLength < length` | not enforced on load (`ProjectFile.cpp:188`); `>= length` means "not looping" (`Playback.cpp:256`); `splitClip` clears it (`SongEdit.cpp:785-788`) |

---

## 2. ID strategy

| Question | Answer | Evidence |
|---|---|---|
| Type | `using TrackId = uint32_t; using ClipId = uint32_t;` — **two aliases of one type**; the compiler cannot separate a track id from a clip id | `Song.h:17-18` |
| Stable or index? | Stable opaque handles. `Song::tracks` order is mixer/display order and does not affect ids | `moveTrack` only reorders the vector (`SongEdit.cpp:485-494`) |
| Allocation | one monotone counter `Song::nextId` via `edit::takeId` (`SongEdit.cpp:45-48`), used for tracks (369-372), clips (501-502), slots (532-533), buses (411, same number space as tracks per `Song.h:370`), audio clips (269-270), audio slots (329-330) ⇒ **one global id space** | `SongEdit.cpp:45-48` |
| Audio vs MIDI clips | share the id space ("Audio clips share their ids with MIDI clips", `SongEdit.h:57-59`); `moveClip`/`splitClip`/`removeClip` dispatch on id across all four containers | `SongEdit.cpp:512-525, 658-688, 708-727` |
| Lookup cost | linear scans, no index: `findTrack` (50-64), `findClip`/`findClipPtr` (91-115), `trackOfClip` (348-367), `findAudioClip` (134-145), `sceneOfClip` (627-638) | O(tracks × clips) |
| Positional handle | `ClipRef {Track*, size_t index, bool inSlot}` — invalidated by any insert/remove/sort of that vector | `SongEdit.h:15-21` |
| Repair on load | ids `0`/duplicated renumbered from `nextId`; `nextId` raised to `max(maxId+1, saved)`, so no id is ever reused; broken bus ids reset routes and drop sends | `ProjectFile.cpp:833-921, 851` |
| What breaks on reorder | ids survive; what breaks is anything holding a **vector index** (`ClipRef`, `sceneOfClip`, `Track::slots` position = scene) or a **stale id** (form `muted`, `ClipId` after delete) | as above |

---

## 3. Timing

| Item | Value | Evidence |
|---|---|---|
| Tick type | `using Tick = std::int64_t` | `Time.h:10` |
| PPQ | `kTicksPerQuarter = 960` ("divides cleanly into triplets, quintuplets and 64ths"); **not stored in the file** — no `ppq`/`ticksPerQuarter` key in writer or reader, only used as a length default there | `Time.h:12-13`; `ProjectFile.cpp:484` |
| MIDI export/import | exported division 960 (`src/core/src/MidiFile.cpp:637`); imported ticks `(t*960 + division/2)/division` (integer, half-up) (`MidiFile.cpp:174`) | |
| Tempo map | `TempoPoint{Tick at; double bpm}`, sorted, **always a point at 0**, **step changes only — "no ramps yet"**; range 20..400 BPM, NaN → 120 | `TempoMap.h:10-24`; `Time.h:15-16`; `Time.cpp:8-13` |
| `set`/`remove`/`setPoints` | insert or replace (clamped `at>=0`), `remove(0)` no-op; bulk set clamps, stable-sorts, last-wins per tick, prepends `{0, first.bpm}` if the first point > 0, `{0,120}` if empty | `TempoMap.cpp:8-49` |
| Cached seconds | `startSeconds[i]` rebuilt from tick 0 on every mutation; conversions integrate from song start ("never accumulate error") | `TempoMap.cpp:51-57`; `TempoMap.h:22-23` |
| Time-signature map | **none** — one `TimeSignature{numerator=4, denominator=4}` for the whole song | `Song.h:432`; `Time.h:20-33` |
| Bar/beat | `ticksPerBeat = 960*4/denominator`, `ticksPerBar = ticksPerBeat*numerator` (integer division) | `Time.h:29-30` |
| tick→bar.beat.sixteenth | `toBarBeat` (`Time.cpp:22-49`): floor division with negative correction (31-38); invalid signature silently becomes 4/4 (23-25); the "sixteenth" is always 240 ticks regardless of denominator (29, 46) | `TimeTests.cpp:19-34` |
| tick↔seconds | `secondsAt(double)` = `startSeconds[i] + (tick - at_i) * 60/(bpm*960)` — double, no rounding (`TempoMap.cpp:79-83`); `tickAtSeconds(double)` = binary search on `startSeconds` with `<=` then linear inverse — double, no rounding (85-97); `secondsPerTick` (`Time.h:53`) | tolerances 1e-12 / 1e-9 (`SongTests.cpp:29-33`) |
| tick→sample | `sampleForTick` = `anchorSample + (int64)ceil((secondsAt(tick)-anchorSeconds)*sampleRate - 1e-6)` — **ceil with a 1e-6-sample epsilon** ("keeps an event that lands exactly on a sample from slipping one sample late"); sample rate default 48000, lives only in the engine | `src/engine/src/Transport.cpp:8-16`; `Processor.h:14-18, 35-45` |
| sample→tick | `tickAtSample` = `tickAtSeconds(anchorSeconds + (sample-anchorSample)/rate)`, `anchorTick` when stopped | `Transport.cpp:18-23` |

**Three rounding rules coexist** (no single canonical conversion): `ceil(x-1e-6)` in engine scheduling (`Transport.cpp:15`); `llround` in `AudioToMidi.cpp:196`, `AudioAnalysis.cpp:611`, `MainComponent.cpp:2113-2114`, `TimelineViews.cpp:2305,2377`, `Session.cpp:953`; truncation in `TimelineViews.cpp:1598`.

**Boundary cases undefined or only accidentally consistent**

1. **Tick before the anchor** — `sampleForTick` can return `< anchorSample` (negative offset); nothing clamps it to `>= sampleTime`, callers merely subtract (`Engine.cpp:356`).
2. **Epsilon vs test oracle** — the engine's `-1e-6` can put an on-sample event one sample before `llround(seconds*rate)`, which is what `TimingTests.cpp:20-24` predicts; the test allows ±1 sample (`TimingTests.cpp:71-75`). "Exact" here means ±1 sample.
3. **Loop period** — the loop end uses `sampleForTick` (`Engine.cpp:899`), so a period is a difference of two ceils and may differ by ±1 sample per tempo segment; the test asserts `<= 1` (`TimingTests.cpp:98-105`). Also, `tickAtSeconds` is not an exact inverse of `secondsAt` in floating point (see tolerances).
4. **Audio clip offset** is derived from a *seconds difference* (`SongEdit.cpp:720`) and assumes one constant stretch per clip (`audioSpeed` uses the tempo at the clip **start**, `SongEdit.cpp:125-132`); a tempo change inside a clip does not re-time the file.
5. **`toBarBeat` at denominator 8/16/32** labels a 240-tick unit "sixteenth" even when a beat is 480 or 1920 ticks — a display naming defect, not an arithmetic one.

---

## 4. Chord representation

| Layer | Representation | Evidence |
|---|---|---|
| Model / project file | **free-text symbol string** in `ScoreMark{kind=chord, at, text}`. No pitch classes, nothing parsed is stored | `Song.h:229-239`; JSON `{"kind":"chord","at":3840,"text":"F#m7b5/C"}` at `ProjectFile.cpp:434-441` |
| Runtime chord value | `Chord{int root /*0=C..11=B*/; ChordType type; int bass /*-1=root*/}`, 35 Yamaha chord types | `src/core/include/arranger/core/StyleChord.h:13-34` |
| Text↔`Chord` | to value: `style_chord::parse` first, then `theory::fromRoman` (so "Am" is never a numeral); to text: `style_chord::name` ("C", "F#m7", "Bb7sus4", "C/E"), key-aware spelling via `theory::chordName(Chord, Key)` | `StyleChord.h:39,46`; `Theory.h:71`; `src/core/src/ChordInput.cpp:227-241` |
| Chord lane | derived on demand from one track's chord marks; two marks at one tick ⇒ "the last wins"; each chord ends at the next chord or the bar end | `ChordInput.cpp:187-201` |
| Song key | free text `Song::key="C major"`, `theory::parseKey`; unparsable ⇒ C major | `Song.h:433`; `Theory.h:44`; `ChordInput.cpp:222-225` |
| Style chord policy | numbers, not symbols: `sourceRoot`, `sourceChordType`, `noteMute`, `chordMute` | `StyleRules.h:41-44`; `ProjectFile.cpp:268-269, 311-314` |

Consequence: chord text is the only durable form. `Chord→name→Chord` is **not** asserted anywhere in the suite (no test found); text round-trips exactly, parsed meaning is parser-version dependent.

---

## 5. Project file

### 5.1 Envelope and gate

```json
{ "format": "arranger-midi-daw-project", "version": 4, "song": { ... } }
```

* `format` must match exactly, else `Not a Lazy Arranger Midi DAW project` (`ProjectFile.cpp:694-695`). `version < 1` → error, `version > 4` → "saved by a newer version" (`ProjectFile.cpp:697-701`); accepted range **1..4**, writer always emits 4 (`ProjectFile.cpp:617`).
* The header doc comment still shows `"version": 1` (`ProjectFile.h:13-24`) — **stale**, contradicted by line 28.

### 5.2 Version history (`git log -L` on the `kVersion` line)

| Version | Commit | Schema delta |
|---|---|---|
| 1 | `62251ea` Phase 0 | `song{title, tempoBpm: scalar, timeSignature:[n,d], key, tracks[{name,colour,midiChannel}], metronome}` — **tracks carry no clips** |
| 2 | `ec0efc3` Phase 1 | `tempo` becomes `[[tick,bpm],…]`; adds `loop`, `soundFont`, `master.gainDb`, `nextId`, track `id`, `output{kind,port,channel}`, `sound`, `mixer`, `armed`, `inputPort`, `clips` |
| 3 | `9386141` Sampler tracks + looping clips | adds clip `loopLength` and track `kit` (only these two keys are new in that commit's writer) |
| 4 | `1878ea0` A style lives in the Clip grid | adds `scenes`, `launchQuantise`, `style`, `sections`, track `styleChannel`; **not bumped since**, while 21 later commits added keys (`audio`, `audioSlots`, `marks`, `automation`, `buses`, `sends`, `effects`, `instrument`, `audioInput`, `forms`, `bus`) |

**⇒ version 4 is a floor, not a schema id.** A v4 file in the wild may contain any subset; the reader must default the rest. Evidence: `git log --oneline 1878ea0..HEAD -- src/core/src/ProjectFile.cpp` = 21 commits.

### 5.3 Migration code paths

| Migration | Code |
|---|---|
| v1 `midiChannel` → `output.channel` | `if (fileVersion == 1) t.output.channel = clamp(valueOr(j,"midiChannel",1),1,16);` — `ProjectFile.cpp:535-536` |
| v1 scalar `tempoBpm` → `TempoMap` | only when the `tempo` array is absent — `ProjectFile.cpp:730-733` |
| v2→v3, v3→v4 | **no code.** Additive defaults only; nothing rewrites an old file's shape |

`grep -n version src/core/src/ProjectFile.cpp` confirms the only `fileVersion` uses are those two plus the pass-through at 789.

### 5.4 Schema at HEAD, `song` level

Always written (`ProjectFile.cpp:615-640`): `title`, `tempo`, `timeSignature`, `key`, `guitarChords`, `loop{enabled,start,end}`, `soundFont`, `scenes`, `launchQuantise`, `master{gainDb,effects}`, `buses[]`, `nextId`, `metronome{enabled,level,accentDownbeat}`, `tracks[]`. Conditionally: `style` (642-643), `sections[]` (644-655), `forms[]` (656-678).

| Object | Always | Only when non-default |
|---|---|---|
| `track` (336-443) | `id,name,colour,output{kind,port,channel},sound{name,bankMsb,bankLsb,program},mixer{volume,pan,reverb,chorus,mute,solo},armed,inputPort,inputChannel,clips[]` | `kit{name,pads[]}`, `audioInput{first,channels}`, `slots[]`, `styleChannel`, `audio[]`, `audioSlots[]`, `automation{volume,pan[,reverb][,chorus]}`, `instrument`, `effects[]`, `bus`, `sends[{bus,level}]`, `marks[{kind,at,text}]` |
| `clip` (164-177) | `id,name,start,length,notes[],controls[]`; note row `[start,length,pitch,velocity]`, control row `[at,"cc"\|"bend"\|"program"\|"pressure",number,value]` | `loopLength` (>0) |
| `audioClip` (383-389) | `id,name,file,start,length,offset,gainDb,fadeIn,fadeOut,bpm` | `layer` (≠0) |
| slot entry | a clip object, or `null`; array length = last used scene + 1 | — |
| `pluginSlot` (48-64) | `format,id,name,state,bypass` | `automation[{control,name,points[[tick,value]]}]` |
| `kitPad` (347-348) | `name,note,file,start,end,gainDb,pan,tune,choke,oneShot` | — |
| `mark`, `section`, `formPart`, `form`, `bus` | `kind,at,text` (436-440); `name,start,end` (649-652); `section` (664-673); `name,ending,parts` (675); `id,name,colour,volume,pan,mute,effects[]` (78) | `scene` (≥0); `bars` (≠0), `muted[]`, `into` (≠automatic: `automatic\|none\|fill\|pickup\|hold\|stop`), `label` |
| `style` (257-283) | `format,setup[[tick,hex]],ruleSets[{sections,rules[],extra}],sectionEvents{name:[[tick,hex]]},extra` | raw Yamaha bytes as uppercase hex |

### 5.5 `examples/demo.arranger` (version 3) — full key structure

From `python3 -c "json.load(...)"`. Top level `{format: str, version: int(3), song: object}`.

```
song: title "Late Night Sketch" | tempo [[0,112.0]] | timeSignature [4,4] | key "C minor"
      loop {enabled:true,start:0,end:15360} | soundFont "" | master {gainDb:-1.5} (no effects in v3)
      nextId 31 | metronome {enabled:true,level:0.7,accentDownbeat:true}
      tracks[6] ids 3,5,7,9,11,13 | scenes ["Intro","Verse","Chorus","Break","Fill"]
track: id | name | colour "coral" | armed | inputPort | output {kind,port,channel}
       sound {name,bankMsb,bankLsb,program} | mixer {volume,pan,reverb,chorus,mute,solo}
       clips[] {id,name,start,length,notes[],controls[]} | slots[] with null holes
note:  [start,length,pitch,velocity] ints, e.g. [0,120,36,110]; controls [] when empty
slots: keys id,name,start,length,notes -- "controls" ABSENT (v3); track 3 [clip,clip,clip,null,clip]
```

Absent from the v3 file and added by the v4 writer on save: `guitarChords`, `launchQuantise`, `buses`, `sections`, `forms`, `style`, and `controls` on slot clips. Key order also differs: the demo is in declaration order while HEAD's writer emits nlohmann `std::map` (alphabetical) order, visible in `examples/Copper Lights/*.arranger`; the reader is order-independent. The four other examples are **version 4**, which is what `ProjectRoundTripTests.cpp:99-115` (`opened >= 5`) exercises.

### 5.6 Unknown keys and silent coercions

Unknown keys at any level are **ignored and dropped on re-save** (`valueOr` + explicit `find`, `ProjectFile.cpp:30-44`), so no extension survives a save. Unknown enum strings coerce silently: `output.kind` → `soundFont` (134-142), control kind → `controller` (156-162), mark kind → `chord` (517-519), transition → `automatic` (812, 825). Wrong type for a known scalar → `Error("Field \"x\" has the wrong type")` (30-44), but inside `clipFromJson` the raw `get<Tick>()` escapes as `json::exception` and reads as `"Project file is damaged: …"` (924-927) — two error surfaces for one fault class.

### 5.7 Load-time repairs (lossy)

| Repair | Lines |
|---|---|
| invalid `loop` ⇒ whole `LoopRange` reset (loses `enabled`) | 745-752 |
| marks with empty `text` dropped; plugin slots with empty `id` dropped | 522-523, 115 |
| sections with empty name or `end <= start` dropped | 802 |
| `bus`/`sends` pruned to existing buses **and** `makesSound()` kinds; `level <= 0` sends removed | 867-876 |
| broken bus id ⇒ tracks re-pointed to master, its sends erased; ids deduped/renumbered, `nextId` raised | 852-866, 833-921 |
| clips: `start>=0`, `length>=1`, `loopLength>=0`, slot clips `start=0` | 186-188, 570 |
| notes: pitch 0..127, velocity 1..127 (**velocity 0 → 1**), zero-length dropped, sorted | 195-196, 206; `SongEdit.cpp:885-897` |

### 5.8 Save, autosave, crash recovery

| Concern | Behaviour | Evidence |
|---|---|---|
| Atomic save | write `path + ".saving"`, flush, `fs::rename`; rename failure removes the temp and throws | `ProjectFile.cpp:959-1001` |
| Audio paths | absolute paths inside the project folder are stored relative; load rebases them; root-relative (`/x`) kept as written | `ProjectFile.cpp:966-979, 1013-1022`; test `ProjectFileTests.cpp:71-107` |
| Autosave pair | `<appdata>/Arranger MIDI DAW/Autosave/autosave-<pid>-<ms>-<counter>.arranger` + `.json`; meta is `{format:"arranger-midi-daw-autosave", version:1, pid, original, title, savedAt, heartbeat}` | `Autosave.cpp:113-131`; folder `MainComponent.cpp:2506-2513` |
| Cadence | 60 000 ms from the first unsaved change, heartbeat 30 000 ms, only while `isModified()` | `Autosave.h:29-33`; `Autosave.cpp:167-194` |
| Removal | on save, on undo back to the saved revision, on clean quit | `Autosave.cpp:169-175`; `MainComponent.cpp:499-502` |
| Orphans | `.json` whose `.arranger` exists, format matches, age ≤ 30 days, pid gone **or** heartbeat older than 5 min; newest first | `Autosave.cpp:207-255` |
| Recovery | prompt; on accept: load, `markUnsaved()`, delete the pair, immediately write a fresh copy under the same id | `MainComponent.cpp:2528-2586` |
| Path compat | the data folder is still **"Arranger MIDI DAW"** although the product was renamed (commit `ec23cbc`) — recovery depends on that literal path | `MainComponent.cpp:2508-2513` |

---

## 6. Edit / command path

### 6.1 What exists

| Concept | Present? | Evidence |
|---|---|---|
| Command objects / typed change records | **No** — free functions on `Song&` | `SongEdit.h`: `void modifyClip (Song&, ClipId, const std::function<void (Clip&)>&)` (46), `ClipId splitClip (Song&, ClipId, Tick)` (52), `NoteKey addTimelineNote (Song&, TrackId, Tick, Tick, int, int=90)` (137), `void normalise (Clip&)` (118), `std::uint32_t takeId (Song&)` (191) |
| Transactions (begin/commit/rollback), inverse operations | **No** — one `Document::edit` call is the atomic unit; undo restores snapshots, not inverses | `Document.h:29` |
| Undo stack | **Yes**, snapshot-based `Step{ Song before; std::string description; uint64 revisionBefore; }` | `Document.h:53-58`; `Document.cpp:8-26` |
| Coalescing | **Yes**, by string `mergeKey` | `Document.cpp:15` |
| Validation layer | **No central one** — per-function clamps plus load-time repairs | §5.7, §6.4 |

Quoted signatures:

```cpp
// Document.h:29
void edit (const std::string& description, const std::function<void (Song&)>& change,
           const std::string& mergeKey = {});
// Document.h:42, 45, 47
void closeMerge() noexcept;  std::uint64_t revision() const noexcept;
bool isModified() const noexcept { return rev != savedRev; }
// Session.h:57 — the app wrapper: one undo step, then engine/UI sync
void edit (const std::string&, const std::function<void (Song&)>&, const std::string& = {});
```

### 6.2 Granularity and cost

| Property | Behaviour | Evidence |
|---|---|---|
| Undo unit | one `Document::edit` call ⇒ one version of the **entire Song** | `Document.cpp:8-26`; `UndoRedoTests.cpp:174-201` |
| No-op edits | deep-compared away (`if (next == current) return;`) via `Song::operator==` → `Track::operator==` (clip contents, not pointers) | `Document.cpp:12`; `Song.cpp:8-51` |
| Cost per edit | full `Song` copy for the snapshot + full deep compare; `nextId` is part of equality (`Song.h:457`), so an id-consuming edit counts as a change | as above |
| Memory shape | `Clip`/`AudioClip` payloads shared and copy-on-write; but `PluginSlot::state` base64 strings, automation vectors and `Track` structs are **deep-copied per step** | `Song.h:66, 90`; `SongEdit.cpp:645-647, 302-306, 1163-1166` |
| History cap | default 200, oldest erased | `Document.h:24`; `Document.cpp:19-20`; `UndoRedoTests.cpp:260-270` |
| Merge rule | same **non-empty** key AND history non-empty AND redo empty; a merged edit pushes no step and keeps the first description. The UI embeds a per-gesture counter in the key (`"master-<gesture>"`, `keyPrefix + "-mixer-<track>-<part>-<gesture>"`) and ends the gesture with `Session::endMerge()` | `Document.cpp:15-25`; `MixerView.cpp:707, 736`; `Session.h:60` |
| Revisions | `rev = nextRev++` per change; undo restores `revisionBefore`; `markUnsaved()` sets `~0` | `Document.cpp:24,49`; `Document.h:50` |
| App coupling | `Session::edit` wraps `Document::edit`, then `songChanged(before)` prunes selection, drops a stale `editClip`, republishes to the engine. 141 `session.edit(...)` calls in `src/app` (e.g. `MainComponent.cpp:328-337`, `PianoRoll.cpp:221`, `ScoreView.cpp:774`) | `Session.cpp:56-63, 98-118` |
| Quantise | a pure `Clip&` mutator applied through `modifyClip` inside one edit, so a multi-clip quantise is a single undo step | `Quantise.cpp:41-67`; `MainComponent.cpp:1616-1619` |

### 6.3 What the edit layer guarantees (and only this)

* Mutators are surgical and id-based: `removeClip` walks all four containers (`SongEdit.cpp:512-525`); `moveClip` keeps audio clips on audio tracks (`658-677`); `modifyClip` re-sorts the track and forbids changing a slot clip's `start` (648-653). Values are clamped locally: `normalise` (885-897), `normaliseAudio` (282-292), `addTimelineNote` (929-932), `changeNotes` (998-1001), `setAutomationPoint` (1038-1039), `setMark` (1094), `setSend` (466).
* Failures are **silent no-ops**, not errors: missing track ⇒ `0`/`void` (91-102, 264-267, 496-500); `moveClip` to a non-audio track falls back to the source (669-670); `setTrackBus`/`setSend` ignore non-sounding tracks and unknown buses (448, 464); `setSlot` does **not** check `output.kind` (527-541) while `setAudioSlot` does (327).

### 6.4 Gaps

1. Every edit is O(model) to compare and to snapshot; 200 steps × plugin state is the main memory/CPU risk of the undo model.
2. No check that timeline MIDI clips do not overlap — unlike audio, MIDI has no layering rule, both simply play.
3. `removeTrack` leaves `FormPart::muted` and `SongSection` untouched (`SongEdit.cpp:480-483`); nothing prunes them later. Undo history is not persisted and `reset()` clears it (`Document.cpp:28-36`).

---

## 7. Compatibility constraints for a Rust port

### 7.1 Must preserve to reopen and re-save losslessly

"Lossless" = what the tests assert: `fromJson(toJson(song)) == song` (`ProjectRoundTripTests.cpp:51-73`), and save→load→save byte equality *for files of the same writer* (`ProjectRoundTripTests.cpp:75-97`). It does **not** mean byte equality with v3 input.

Must-keep: every always-written key in §5.4 plus every conditional key, because each maps to a model field in `Song::operator==` — note rows, `loopLength`, **slot array positions**, audio `offset/bpm/layer/fades`, automation pairs, mark `kind`/`at`/`text`, section `scene`, form `muted` ids and `into`/`ending` ids, bus ids plus `track.bus`/`sends.bus`, plugin `state` verbatim, kit slices, `style` hex, `soundFont` and audio `file` strings (path form matters), `nextId`.

### 7.2 View / UI state (cosmetic, but persisted)

| Key | Why UI |
|---|---|
| `song.guitarChords` | "the score shows a guitar chord box above each chord symbol" (`Song.h:434`) |
| `track.colour`, `bus.colour` | palette names from `kTrackColours` (`Song.h:21`), theme-relative |
| `track.name`, `bus.name`, `clip.name`, form `label`/`name`, `section.name` | labels, except `section.name`, which is also a musical key — forms reference sections **by name** (`Song.h:406`) |
| `track.armed`, `inputPort`, `inputChannel`, `audioInput*` | record-arm/monitor state, persisted for a ready-to-record reopen |
| `metronome`, `loop.enabled`, `launchQuantise` | transport preferences, not score content |

Everything else UI-ish is **not** in the file: `snapGrid`, selection, `editClip`, zoom/scroll, `SlotStatus` (`src/app/Session.h:204, 69-73`; `Song.h:461-467`).

### 7.3 Derived (safe to recompute)

`nextId` (`ProjectFile.cpp:851`), `contentEnd`/`sceneCount` (`Song.cpp:53-79`), `Track::laneCount/controlLine/findLine/mixerValue` (`Song.h:275-332`), `Track::operator==` (`Song.cpp:8-51`), `audioSpeed` (`SongEdit.cpp:125-132`), `playedClip` (`SongEdit.cpp:18-43`), the chord lane (`ChordInput.cpp:187-201`), `form::sectionsOf` when `sections` is empty (`Song.h:445`).

### 7.4 JSON keys in risk order

| # | Key / area | Risk | Why |
|---|---|---|---|
| 1 | `tracks[].slots[]`, `audioSlots[]` | **Critical** | position *is* the scene number; `null` holes and trailing-null trimming are semantic (`ProjectFile.cpp:352-359, 397-406, 561-572`) |
| 2 | `song.style` (setup/ruleSets/sectionEvents/extra hex) | **Critical** | opaque Yamaha bytes; any byte change silently changes style playback |
| 3 | `instrument.state`, `effects[].state` | **Critical** | base64 plugin state, opaque, not re-derivable |
| 4 | `song.tempo` | High | `[int,double]` pairs, step-only, point at 0 mandatory; tick rounding changes every event sample |
| 5 | `clips[].notes[]`, `controls[]` | High | positional 4-tuples; velocity 0 silently becomes 1; negative starts legal |
| 6 | `marks[].text` | High | chord symbols are the only chord record; meaning depends on the parser |
| 7 | `audio[].file` + `offset/bpm/layer/start/length` | High | relative-path rewrite on save/load; stretch derived from the tempo at clip start |
| 8 | `song.buses[]`, `track.bus`, `track.sends[]` | High | id cross-references, pruned on load when inconsistent |
| 9 | `song.sections[]`, `song.forms[]` | High | sections referenced **by name**; `muted` by track id; `into`/`ending` enum ids |
| 10 | `clips[].loopLength` | Medium | one int changes playback of the whole clip |
| 11 | `kit.pads[]` | Medium | 10 fields; `sliceEnd=0` means "to end" |
| 12 | `song.nextId` | Medium | monotone; a wrong value ⇒ collisions until the next load repairs it |
| 13 | `mixer`, `master.gainDb`, bus `volume/pan` | Medium | ints 0..127 vs float dB; easy off-by-one on clamping |
| 14 | `song.scenes[]` | Medium | user text; row count is `max(8, len, used+2)` (`Song.cpp:66-79`) |
| 15 | `timeSignature`, `key`, `guitarChords`, `launchQuantise`, `loop` | Low | single scalars, validated/defaulted on read |
| 16 | `format`, `version` | Low but gating | exact string; `>4` and `<1` refused |

---

## 8. Recommendations

| Concern | Call | One-line reason |
|---|---|---|
| Entity model shape | **Port** | 24 plain value structs with documented defaults map 1:1 to Rust and stay cheap. |
| Clip immutability + COW | **Port** | It is what makes 200-step snapshot undo affordable (`Song.h:47-51`, `SongEdit.cpp:645-647`); dropping it forces a real command log. |
| Track/Clip ids | **Port, then tighten** | Keep `u32` + one `next_id` for file compat, but use newtypes so a clip id cannot be passed as a track id. |
| Scene = array index | **Port wire format, redesign in memory** | Positional scene slots are the riskiest structure in the file (§7.4 #1). |
| PPQ 960, `i64` ticks | **Keep behaviour** | Constant, not stored, and used by MIDI export (`MidiFile.cpp:637`); changing it invalidates every example. |
| Tempo map (step, no ramps) | **Keep behaviour** | The format has no ramp encoding; ramps are a new file version, not a port decision. |
| tick↔seconds in `f64` from song start | **Keep behaviour** | Exact-by-construction (`TempoMap.h:22-23`); integer tick math would move event samples. |
| tick↔sample rounding | **Redesign — one rule, the engine's** | Three rules coexist (§3); `ceil(x-1e-6)` is the only one that matches playback, so make it the only conversion and fix the `llround`/truncating sites. |
| Single time signature | **Defer** | A meter map is a schema change; nothing in the file or tests needs it now. |
| Chord as text | **Keep wire form; port the parser separately** | The file stores strings only and `Chord` is derived; changing the stored form breaks every project. |
| Version gate 1..4, refuse >4 | **Keep behaviour** | Cheap forward protection; a Rust writer emits 4 and never 5 without an agreed migration. |
| Unknown-key handling | **Redesign** | Unknown keys are silently dropped today; preserving them costs nothing on read and protects forward compatibility. |
| Load-time repairs | **Port, but report them** | Needed to open real files (§5.7) yet lossy (loop reset, dropped marks), so a port should say what it repaired. |
| Undo model (snapshot + merge key) | **Port semantics, redesign storage** | Merge-key coalescing is the user-visible contract (`SongTests.cpp:96-105`); whole-song deep copies incl. plugin `state` are the cost to remove. |
| `SongEdit` free functions | **Port as a command layer** | They are already the only mutation points; wrapping each in a typed command is mechanical and gives typed inverses. |
| Silent no-ops on bad ids | **Redesign** | `addClip` returning `0` and `moveClip` falling back hide bugs; return a typed error while keeping UI behaviour. |
| Validation layer | **Add a thin one** | Nothing checks notes-past-clip-end, overlapping MIDI clips, stale `FormPart::muted`, or section order; enforce on ingest, not on edit. |
| Autosave / crash recovery | **Port** | Simple and well tested (`tests/AutosaveTests.cpp`, 13 cases); keep the pair, the 60 s/30 s cadence and the `Arranger MIDI DAW` folder name. |
| `StyleData` blobs | **Port as opaque bytes** | Only the style-file code understands them; do not model Yamaha data in the first Rust pass. |

---

## 9. Unverified / needs another probe

| Claim | How to verify |
|---|---|
| `Chord → name → Chord` is stable for all 35 `ChordType`s | no test found; round-trip over the enum (`StyleChord.h:13-22`) against `style_chord::name`/`parse` |
| Re-saving a v3 file is byte-stable | the writer is v4-only and adds keys; compare `examples/demo.arranger` before/after a C++ save (not run here) |
| Engine behaviour for a tick before the anchor | needs a runtime test; `Engine.cpp:356` only subtracts |
| Note beyond the clip end is suppressed at note-on | `Playback.cpp:263-266` is read, not test-verified |
| All 34 `.edit (` sites in `McpServer.cpp` reach `Document::edit` | only 835, 1077, 1097, 1277 were opened |
| Nothing prunes `FormPart::muted` after `removeTrack`; real memory cost of 200 undo steps with plugin state | read `form::use` fully (`SongForm.cpp:1194+`); measure the snapshot size described by `Document.h:53-58` and `Song.h:145-156` |
