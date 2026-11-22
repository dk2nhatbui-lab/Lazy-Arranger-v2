# C++ style / arranger-keyboard engine: parity audit

Target: `/Users/far-mini/repos/arranger-midi-daw` at commit `8ca41644a2be5f5677bce8cae060a55f7b19e8fc` (worktree clean).
Method: read-only source reading. No build, no test run, no target execution.
Citations are `path:line` relative to `REPO = /Users/far-mini/repos/arranger-midi-daw`; the file list is in §10.
Markers: **(U)** = could not confirm from source; **(D)** = derived by reading the code, not executed.

Two corrections to the brief:
- `src/core/include/arranger/core/StyleConvert.h` does not exist. The `style_convert` API is declared in `src/core/include/arranger/core/Style.h:98-151`.
- `PsrExport.*` writes a **song** file for the PSR-S970 song player, not a style (`src/core/src/PsrExport.cpp:325-351`). It is out of style scope except as a `midi_file::write` consumer.

---

## 0. Findings that change the parity plan

1. **There is no arranger state machine.** No variation/fill/break object exists in `src/core` or `src/engine`. A section is a scene; launching it is generic clip launch (`Engine.cpp:278-289,326-350`). Fill and break are section *names* (`Style.h:26-31`) plus a separate offline timeline generator (`SongForm.cpp:1418-1444,1560-1640`).
2. **`examples/demo-style.sty` is not a Yamaha-authored style.** It is `MThd` + one `MTrk`: no `CASM`, `CSEG`, `Sdec` or `OTSc` (chunk directory ends at 3940 B; §2.1). Every channel runs on the hard-coded default rules (`StyleConvert.cpp:38-65`).
3. **The demo is written in C natural minor** (channels 11-15 use C D Eb F G Ab Bb; no E or B natural), while the app's start chord is Cmaj7 (`Session.h:504`) and the default "written in" chord is Cmaj7 (`StyleConvert.cpp:45`). At root C the conversion is the identity for every note in the file (D); at another root the minor material is re-spelled through a major-scale source chord (D, §3.7).
4. **Chord following is a whole-clip re-render, not per-event processing.** `applyChord` copies the song, transposes every style slot note, and publishes a new snapshot (`StyleConvert.cpp:494-527`; `Session.cpp:146-159`). There is one chord at a time and no chord timeline.
5. **Round trip is re-serialisation, not byte preservation.** Only `style.extra`, `ruleSets[].extra` and `rule.special` are kept verbatim (`StyleFile.cpp:218-220,288-293,540-547`). Everything else is rebuilt.
6. **RTR is stored and edited but never applied.** Nothing outside file/project IO and the rule editor reads it (`StyleRules.h:25`; `StyleFile.cpp:165,190`; `StyleRuleEditor.cpp:96,114`). The editor says so: "the preview in the app starts the new chord at the next note" (`StyleRuleEditor.cpp:64`).
7. **Only one style fixture ships, and it has no chord rules.** No test pins a real Yamaha `Ctb2`; the one used is a hand-built byte array (`StyleFileTests.cpp:503-545`).

---

## 1. Style data model

| Concern | Representation | Evidence |
|---|---|---|
| Part identity | `enum class StylePart { rhythm1, rhythm2, bass, chord1, chord2, pad, phrase1, phrase2 }`; channel = `9 + part` → 9..16 | `Style.h:19-23` |
| Section identity | string name, matched literally; Yamaha order in `kStyleSectionNames[17]` = Intro A-D, Main A-D, Fill In AA/BB/CC/DD/BA, Ending A-D | `Style.h:26-31` |
| Section | `StyleSection { std::string name; Tick length; std::map<int,Clip> parts; std::vector<StyleEvent> events; }` | `Style.h:34-41` |
| Per-section tracks | one `Clip` per source channel 1..16, keyed by channel in `parts`; no channel field | `Style.h:38` |
| Notes | `Note { Tick start; Tick length; uint8_t pitch; uint8_t velocity; }`, clip-relative, sorted (start, pitch) | `Song.h:23-33`; `StyleFile.cpp:393` |
| Controllers | `ControlEvent { at, kind (controller/pitchBend/program/channelPressure), number, value }` | `Song.h:36-47` |
| Section events | SysEx/meta as raw bytes (`StyleEvent { Tick at; vector<uint8_t> bytes; }`), section-relative | `StyleRules.h:65-70` |
| Section length | implicit: distance to the next marker or end of track; no length chunk | `StyleFile.cpp:381-403`; `Style.h:37` |
| Phrase | **no phrase entity.** "Phrase 1/2" are part names 7/8 only | `Style.h:19-22` |
| Format | `int format` 1=SFF1, 2=SFF2, from the marker text, not from chunk ids | `Style.h:51`; `StyleFile.cpp:423-424` |
| Chord rules | `vector<StyleRuleSet>`; each names sections and holds `vector<StyleChannelRule>` | `StyleRules.h:56-62`; `Style.h:56` |
| Preserved bytes | `Style::extra`, `StyleRuleSet::extra`, `StyleChannelRule::special` | `Style.h:57`; `StyleRules.h:50,60` |
| Song-side mirror | `StyleData { format, setup, ruleSets, sectionEvents, extra }` + `Track::styleChannel` + `Track::slots[scene]` | `StyleRules.h:73-81`; `Song.h:260,443-446` |
| Section end | held notes closed at the section end: `length = max(1, s.length - start)` | `StyleFile.cpp:389-392` |
| Duplicate names | both sections kept, `Style::section()` returns the first; rule lookup by name is ambiguous | `StyleFile.cpp:25-46` |

Drums (channels 9/10): the default rule treats destination 9 **or** 10 as a drum part (`Ntr::rootFixed`, `Ntt::bypass`, `Rtr::stop`) at `StyleConvert.cpp:46-51`; `makeStyle` maps drums to 10 then 9 (`:266-278`); `fillTimeline` sends them to GM channel 10 when `bankMsb >= 126` or channel 10 (`:620-623`); `SongForm` takes fills from `styleChannel == 9 || 10` (`SongForm.cpp:1425-1433`). Conversion has no channel-9/10 special case; passthrough depends on the rule's NTR (§3, A4).

---

## 2. Yamaha SFF1/SFF2 parsing and writing

### 2.1 Chunks

| Chunk | Read | Written | Evidence |
|---|---|---|---|
| `MThd` | yes; format and track count discarded; SMPTE rejected; division used for tick scaling | always `format 0`, `1 track`, division `1920` | `StyleFile.cpp:345-355,664-669,53` |
| `MTrk` | first only, required, length clamped to bytes left | one track | `StyleFile.cpp:357-359,670` |
| 2nd `MTrk` | treated as unknown → `style.extra` (re-emitted after CASM) | as `extra` | `StyleFile.cpp:533-548` |
| `CASM` | yes → `readCasm` | only when `ruleSets` non-empty | `StyleFile.cpp:538-539,671-672` |
| `CSEG` | yes, one rule set per CSEG | one per rule set | `StyleFile.cpp:262-296,300-315` |
| `Sdec` | section-name list, split on `,`, trimmed | first chunk of every CSEG, even empty | `StyleFile.cpp:270-285,306-309` |
| `Ctab` (SFF1) | yes if body ≥ 26 B | when `format < 2` | `StyleFile.cpp:286-287,311` |
| `Ctb2` (SFF2) | yes if body ≥ 40 B | when `format >= 2` | `StyleFile.cpp:286-287,311` |
| other `CSEG` children (`Cntt`, …) | verbatim in `StyleRuleSet::extra` | after the rules (order may change) | `StyleFile.cpp:288-293,312` |
| other `CASM` children | **dropped** (`continue`) | never | `StyleFile.cpp:258-263` |
| `OTSc`, `MDB`, anything after CASM | verbatim in `Style::extra` (id+length+body) | after the rewritten CASM | `StyleFile.cpp:540-547,673` |

### 2.2 `Ctab` / `Ctb2` layout (from `readRule`, confirmed against `StyleFileTests.cpp:503-545`)

| Offset | Field | Encoding | Evidence |
|---|---|---|---|
| 0 | `sourceChannel` | low nibble + 1 | `StyleFile.cpp:197` |
| 1..8 | `name` | 8 bytes, space/`\0` trimmed | `StyleFile.cpp:198-199,129-134` |
| 9 | `destChannel` | low nibble + 1 | `StyleFile.cpp:200` |
| 10 | `editable` | `0` → true | `StyleFile.cpp:201,231` |
| 11..12 | `noteMute` | 16-bit BE, masked `0x0fff` | `StyleFile.cpp:202,232` |
| 13..17 | `chordMute` | 5 bytes BE → 40-bit mask | `StyleFile.cpp:203-205,233-234` |
| 18 | `sourceRoot` | `byte % 12` | `StyleFile.cpp:206` |
| 19 | `sourceChordType` | Yamaha chord-type number, `2` = major 7 | `StyleFile.cpp:207`; `StyleRules.h:44` |
| 20..25 | SFF1 `transpose` | ntr, ntt, highKey, lowLimit, highLimit, rtr | `StyleFile.cpp:216-217` |
| 20..21 | SFF2 `middleLow`, `middleHigh` | raw note numbers | `StyleFile.cpp:210-211` |
| 22..39 | SFF2 `lowRange`, `transpose`, `highRange` | 6 bytes each | `StyleFile.cpp:212-214` |
| 26 / 40..end | `special` | tail verbatim; a single `0` written when empty | `StyleFile.cpp:218-220,247-250` |

### 2.3 Enums, verbatim

```cpp
enum class StylePart : std::uint8_t { rhythm1, rhythm2, bass, chord1, chord2, pad, phrase1, phrase2 };            // Style.h:19

enum class Ntr : std::uint8_t { rootTransposition, rootFixed, guitar };                                          // StyleRules.h:16
enum class Ntt : std::uint8_t { bypass, melody, chord, melodicMinor, melodicMinor5th, harmonicMinor,
                                harmonicMinor5th, naturalMinor, naturalMinor5th, dorian, dorian5th };              // StyleRules.h:17
enum class Rtr : std::uint8_t { stop, pitchShift, pitchShiftToRoot, retrigger, retriggerToRoot, noteGenerator }; // StyleRules.h:18

enum class ChordType : std::uint8_t
{
    major, major6, major7, major7Sharp11, majorAdd9, major7_9, major6_9, augmented,
    minor, minor6, minor7, minor7Flat5, minorAdd9, minor7_9, minor7_11, minorMajor7, minorMajor7_9,
    diminished, diminished7,
    seventh, seventhSus4, seventhFlat5, seventh9, seventhSharp11, seventh13, seventhFlat9, seventhFlat13, seventhSharp9,
    major7Augmented, seventhAugmented,
    octave, fifth, sus4, sus2,
    cancel,
};                                                                                                               // StyleChord.h:13-22
inline constexpr int kChordTypeCount = 35;                                                                       // StyleChord.h:24
```

Yamaha's file chord-type numbers (`PsrStyle.cpp:47-56`, `0x00`..`0x22`) match the first 35 `ChordType` values in order, but `PsrStyle::chordTypes()` labels `0x21` "1 + 2 + 5" and `0x22` "cc", while the app models `0x20` sus4, `0x21` sus2, `0x22` cancel (U: whether 0x22 really means cancel).

### 2.4 SFF1 ↔ internal mapping (lossy, silent)

- Read (`StyleFile.cpp:149-161`): 0→bypass, 1→melody, 3→melody+`bass=true`, 4→melodicMinor, 5→harmonicMinor, anything else→chord.
- Write (`StyleFile.cpp:174-185`): bypass→0, melody→(bass?3:1), chord→2, melodicMinor/melodicMinor5th→4, **all others→5**.
- `Ntr::guitar` becomes `rootFixed`: `w.u8(sff2 ? t.ntr : std::min(t.ntr, StyleTranspose::Ntr::rootFixed))` (`:171`).
- `naturalMinor`, `dorian` and their 5th variants all collapse to harmonic minor in SFF1.
- SFF1 has no ranges; defaults `middleLow/middleHigh = 0/127` mean every note uses `transpose` (`StyleRules.h:48-49`).

### 2.5 Flags read but not acted on

`MThd` format/track count (`StyleFile.cpp:350-351`); `editable` (only `ProjectFile.cpp:267,310`); `rtr` (§0.6); NTT scale flavours beyond chord/bypass (§3 A1); `highKey` only when NTR transposes; second and later `0x03`/`0x51`/`0x58` in the setup bar are dropped (`:450-453`); the first `0x51`/`0x58` anywhere, even inside a section, becomes global and leaves the section (`:438-449`); CASM children other than CSEG are dropped (`:262`).

(U) `chordMute` polarity: the code treats a set bit as "this chord type may play" (`StyleRules.h:42`; `StyleChord.cpp:315`; test `StyleFileTests.cpp:550-551`). No shipped real file verifies it.

---

## 3. Chord-to-note conversion (implementable specification)

Entry point: `std::optional<int> style_chord::transpose (int pitch, const StyleChannelRule& rule, const Chord& chord)` (`StyleChord.cpp:318-398`). Pure. Part gating is a separate predicate, `bool style_chord::plays (const StyleChannelRule&, const Chord&)` (`:309-316`), called once per clip (`StyleConvert.cpp:509-510`).

### 3.1 Inputs, output

- `pitch` 0..127 (`Note::pitch`, `Song.h:29`); `rule` (`StyleRules.h:35-53`); `chord` `{root 0..11, ChordType 0..34, bass -1 or 0..11}` (`StyleChord.h:27-34`).
- Statics: `kTypes[35] = {suffix, third, fifth, seventh, scale[7]}` (`StyleChord.cpp:25-39`).
- Output: `nullopt` = must not sound, else 0..127, one result per input note. Caller keeps `start`, `length`, `velocity` and drops `nullopt` (`StyleConvert.cpp:513-521`).

### 3.2 Ordered steps (exact decision order)

**0. Choose the record.** `pitch < middleLow` → `lowRange`; `pitch > middleHigh` → `highRange`; else `transpose` (`:320`). Strict comparisons, so `pitch == middleLow` uses the middle record. Defaults 0/127 select the middle record only.

**1. Bypass pass, before the no-chord test.** `fixedRoot = (ntr != Ntr::rootTransposition)`. If `fixedRoot && ntt == Ntt::bypass` → return `pitch` unchanged (`:324-326`). Running before step 2 is why drums survive a no-chord (`StyleFileTests.cpp:214-219`).

**2. No chord.** `chord.type == ChordType::cancel` → `nullopt` (`:327-328`).

**3. Target root.** `targetRoot = mod12 ((t.bass && chord.bass >= 0) ? chord.bass : chord.root)` (`:330`).

**4. Guitar** (only `ntr == Ntr::guitar`, `:332-356`): `shift = mod12 (targetRoot - sourceRoot)`, `> 6 → -12`; `moved = pitch + shift`; pick the nearest pitch class of the **target** chord searching outwards with ties **below first** (`for d = 0..6: moved-d, then moved+d`); octave-fold into the limits when `lowLimit <= highLimit && highLimit - lowLimit >= 11`; `clamp 0..127`; return. `ntt` is ignored.

**5. Source geometry** (`:357-360`): `from = info (rule.sourceChordType)`, `to = info (chord.type)`, `rel = mod12 (pitch - sourceRoot)`, `base = pitch - rel`.

**6. Interval mapping** (`:362-371`):

| `ntt` | Mapping |
|---|---|
| `chord` | `chordToneFor (rel, from, to)`, else `scaleToneFor (rel, from, to)` |
| `melody` and every other value except `bypass` | `t = chordToneFor(...)`; use `*t` only if `*t < 12`, else `scaleToneFor(...)` |
| `bypass` | `interval = rel` (the root may still move in step 7) |

`chordToneFor` (`:47-58`): `rel == 0` → `0`; `rel == from.third` → `to.third`; `rel == from.fifth` → `to.fifth`; `rel == from.seventh` → `to.seventh`, or **`12`** when the target has no seventh ("use the root above"); otherwise nothing.
`scaleToneFor` (`:60-69`): highest `from.scale[i] <= rel` is the degree; keep the chromatic offset; return `to.scale[i] + offset`. The scale comes from the **chord type**, never from `ntt`.

**7. Root placement** (`:373-389`):
- `!fixedRoot` (`ntr == rootTransposition`): `hk = clamp (highKey, 0, 11)`; `shift = mod12 (hk - sourceRoot) - mod12 (hk - targetRoot)`; `result = base + interval + shift`.
- `fixedRoot` (guitar never reaches here): nearest pitch with class `mod12 (targetRoot + interval)`; walk `up` and `down` from `pitch` to the first match; **ties go down** (`(up - pitch) < (pitch - down) ? up : down`).

**8. Fold and clamp** (`:391-397`): if `lowLimit <= highLimit && highLimit - lowLimit >= 11`, add/subtract 12 until inside; then `clamp (0, 127)`.

### 3.3 Part gating (`plays`)

```
cancel:    ((chordMute >> 34) & 1) != 0
otherwise: ((noteMute >> mod12(chord.root)) & 1) != 0 && ((chordMute >> int(chord.type)) & 1) != 0
```
(`StyleChord.cpp:309-316`.) Defaults `0x0fff` / `0xffffffffff` let everything play (`StyleRules.h:41-42`). A false `plays` clears the **whole clip** (`StyleConvert.cpp:509-510`).

### 3.4 Required cases

| Case | Behaviour | Evidence |
|---|---|---|
| Slash chord | Only the target root changes, and only for a rule with `bass = true`. No bass note is added; chord parts ignore the slash bass. | `StyleChord.cpp:330`; test `:192-195` |
| Inversion | No effect beyond `detect()` storing a non-root bass when the lowest held key is a chord tone. Conversion sees root/type/bass only. | `StyleChord.cpp:195-247` |
| No chord | `nullopt` for every note except rules that pass step 1. `plays` may still return true; the notes die in `transpose`. | `StyleChord.cpp:324-328`; `StyleConvert.cpp:513-519` |
| Note outside the source chord | `scaleToneFor`: same degree, chromatic offset kept. `Eb` over a Cmaj7 source stays `Eb` in C major. | `StyleChord.cpp:60-69` |
| Source seventh into a triad | `chord`: interval 12 → root an octave above the source root. `melody`: the 12 is rejected and the scale path runs. | `:56,365-370`; test `:210-211` |
| `sus4` target | `to.third = 5`, so a source third becomes the fourth (test: E→F). | `StyleChord.cpp:37`; test `:212` |
| Source type > 34 | `info()` clamps to 34 (cancel's major-like row) while `plays` shifts by the raw type: the two disagree. | `StyleChord.cpp:38,43,315` |
| Result outside 0..127 | Folded by octaves only inside the limits; then clamped. A range narrower than an octave clamps, not folds. | `StyleChord.cpp:392-397` |

### 3.5 Pinned examples (`StyleFileTests.cpp:173-238`)

Source CMaj7 (`sourceRoot 0`, type 2), bass rule `melody`, `highKey 6`, limits 28..60: `36→36` (C), `36→41` (F), `36→31` (G, above highKey so octave down), `40→36` (E→A minor's C, octave down), `43→44` (G→Dm7b5's Ab), `38→38` (D), `36→40` with `bass` on and C/E.

Chord rule `rootFixed` + `chord`: `52→48` (E→A minor C, nearest, tie down), `55→52` (G→E), `59→60` (B→C for a triad), `59→58` (B→Bb for C7), `52→53` (E→F for Csus4).

### 3.6 Where the C++ is ambiguous or incomplete (state these in the Rust spec)

- **A1.** All eight scale-flavoured NTT values are behaviourally identical to `melody` and use the **target chord type's** scale; `info()` never reads NTT (`:366-371`).
- **A2.** `melody` refuses a mapped interval of 12; `chord` accepts it (`:365` vs `:370`).
- **A3.** The transposing shift can move a part **up** an octave when `sourceRoot > highKey`, because `shift` then carries `+12` (D, from `:377-379`). No test covers it.
- **A4.** Drums are not special-cased: a rhythm rule with `ntr = rootTransposition` gets transposed. The default rule avoids this with `rootFixed` (`StyleConvert.cpp:46-51`).
- **A5.** For source type `1+5` both `third` and `fifth` are 7, so a fifth maps to the target **third**; for `1+8` both are 0, so every note maps to `0` (`StyleChord.cpp:37,51-54`).
- **A6.** Mute-mask polarity is assumed, not verified (§2.5). This is the highest-risk parity item for real files.
- **A7.** `fixedRoot` + `bypass` returns before the cancel test, so a non-drum part with that pair also ignores no-chord (U: no known file rule uses it).

### 3.7 The demo style under these rules (D)

Default rules apply (no CASM): ch 10 drums, ch 11 bass (`melody`, `bass`, transposing), ch 12-15 `rootFixed` + `chord`, all source CMaj7.

- At root C (Cmaj7 or Cm) the mapping is the identity for every note in the file: the material is C natural minor with no E or B natural.
- At F major, `Eb4` (rel 3, not a CMaj7 chord tone) → `scaleToneFor` keeps offset 1 over degree 2 → interval 3 → nearest pc 8 → `Ab4`. The minor third is preserved, so a major target gets parallel-minor colour. At F minor the mapping is musically correct.
- The shipped demo therefore cannot catch a wrong `chordToneFor`/`scaleToneFor` table: it only exercises the identity path.

---

## 4. Playback state machine (what actually exists)

### 4.1 Section launch, not variation switching

| Fact | Evidence |
|---|---|
| A section is a scene; `toSong` sets `launchQuantise = ticksPerBar` | `StyleConvert.cpp:79` |
| `launchScene` queues **every** track: the scene's slot, else a stop (`-1`) — even a Timeline track is stopped | `Engine.cpp:278-289` |
| While playing `at = ceil((now - 1e-6)/quantise)*quantise`; while stopped, floor to the grid and locate | `Engine.cpp:326-341` |
| Pending replaces pending per track; no queue, no priority | `Engine.cpp:347-350` |
| The change applies in the block that crosses `pendingTick`: play up to it, release that track's notes, switch | `Engine.cpp:563-579` |
| A launched slot loops over its own length for ever; nothing ends a section by itself | `Engine.cpp:417-444` |
| While any slot plays or is pending, the Timeline is silent on all tracks (`clipMode`) | `Engine.cpp:385-395,407` |

So there is **no variation/fill/break state machine, no fill precedence, no fill-and-return, no break length**. "Fill In AA" is a one-bar demo scene; launching it repeats the fill every bar until another scene launches. `Transition::hold/stop/fill` in `SongForm` is an offline note generator (`SongForm.cpp:1557-1640`).

Fill/break as data: names only (`Style.h:29`), where `"Fill In BA"` is Yamaha's break fill and gets no behaviour. The offline arranger picks a fill by section energy and copies the style's rhythm-part fill into the bar before a join (`SongForm.cpp:1418-1444,1570-1592`); its "break" is `Transition::stop`, one generated beat of silence (`SongForm.cpp:1152-1169`; `Song.h:392-401`).

### 4.2 Late chord change

| Fact | Evidence |
|---|---|
| One chord at a time: `Chord currentChord { 0, ChordType::major7, -1 }`; `setChord` stores and republishes | `Session.h:504`; `Session.cpp:1090-1098` |
| Chord changes are not quantised or scheduled: `publish()` calls `applyChord(doc, currentChord)` at once | `Session.cpp:146-159` |
| The engine takes the new snapshot at sample 0 of the next block (`switchSnapshot (next, 0)`) | `Engine.cpp:836-837` |
| Held notes survive only when track id **and** `contentKey` match; a transposed slot changes the key, so all held notes on that track are released together | `Engine.cpp:488-545`; `Playback.cpp:347` |
| The slot keeps its phase (`launchTick` unchanged), so later notes keep their times with new pitches | `Engine.cpp:417-444,569-571` |

A mid-bar chord change cuts sounding notes and re-pitches the rest. Nothing re-attacks; nothing waits for the bar line.

### 4.3 Note-off ownership

Ownership is per **track**: `ActiveNotes` keeps a `bitset<128>` plus the `channel` and `port` captured at note-on (`Engine.cpp:360-381`). At a section boundary, `releaseNotes` sends note-off for every held pitch, then `CC64 = 0` if the pedal was down (`:453-466,566-573`). `playRange` releases held notes at every loop wrap except the first (`:425-427`). A same-pitch retrigger sends note-off first (`:365-367`); a note-off for a pitch the track does not hold is dropped, not forwarded (`:372-376`). Stop sends `releaseAllNotes` plus `CC123` on every track channel (`:439-451,481-486`). Two tracks on one port+channel keep separate bitsets, so one track's release cannot clear the other's note, but both may send note-offs for the same pitch (U: untested).

---

## 5. Round trip and lossiness

Write skeleton (`StyleFile.cpp:575-675`): `MThd` (format 0, 1 track, division 1920) → `MTrk` → `CASM` (only when `ruleSets` is non-empty) → `style.extra`. Track order: `0x58`, `0x51`, name `0x03`, marker `"SFF2"/"SFF1"`, marker `"SInt"`, setup events, then per section: marker, section events, note-offs / controls / note-ons, then `0x2f`. At one tick the order is fixed: header 0, markers 1, setup 2, note-offs 3, controls 4, note-ons 5 (`:591,650`).

| Class | What happens | Evidence |
|---|---|---|
| Preserved verbatim | every top-level chunk after CASM, unknown CSEG children, `rule.special` | `StyleFile.cpp:540-547,288-293,218-220` |
| Preserved as values | notes, CC/program/pressure/bend, section names and order, tempo, meter, setup events | `StyleFile.cpp:611-648,601-609` |
| Normalised | division → 1920 with `llround`; ticks rescaled; setup bar length recomputed as one bar (or more); CASM/CSEG/Sdec always rewritten; `Sdec` first | `:53,361-363,587,601-609,306-315` |
| Moved | first tempo/meter to tick 0; first in-section tempo/meter promoted to global; later ones dropped from the setup bar | `:438-449,450-453` |
| Reordered | CSEG children (rules before `extra`); events at one tick (class order above) | `:310-313,650` |
| Dropped | empty/zero-length sections; CASM children other than CSEG; 2nd+ track-name metas in the setup bar; events and note starts at or after a section end | `:400-401,262,450-453,615-617,623-624` |
| Clamped | CC/bend values, note velocity minimum 1, tempo 5..500 | `:632,640,595,625` |
| Inconsistent | `fromSong` re-emits voice/mixer messages at tick 240, so open→save through the song model moves them from tick 0 | `StyleConvert.cpp:178-198` |
| Inconsistent | `format` comes from the marker but selects the chunk: an SFF1 marker with `Ctb2` chunks is rewritten as `Ctab` | `:311` |
| Inconsistent | a second `MTrk` lands in `extra` and is written after CASM, an invalid container (U: whether real files do this) | `:533-548` |

Tests assert a weaker property than byte equality: `write (read (bytes)) == bytes` for bytes the app wrote (`StyleFileTests.cpp:118-119`) and `write (back) == written` for real files (`StyleSampleTests.cpp:81-82`). Original-file byte equality is not tested and does not hold.

---

## 6. Tests and fixtures usable as a parity baseline

### 6.1 Tests that pin conversion behaviour

| Test (file:line) | What it pins |
|---|---|
| `StyleFileTests.cpp:173-238` | The whole `transpose` contract: highKey octave drop, third/fifth remap, bass flag on slash chords, octave folding, fixed-root nearest note with tie-down, seventh→root for triads, drum passthrough on cancel, `nullopt` for a chord part on cancel, SFF2 middle/low range selection, note/chord mute gating |
| `StyleFileTests.cpp:163-171,606-633` | `name()`, two `tones()` sets, `parse()` aliases, `-`, rejects, round trip of every type except cancel |
| `StyleFileTests.cpp:240-262` | `detect()`: triads, inversions→slash, no-fifth, sus4, dim/aug/dim7, 1+8, 1+5, unknown sets |
| `StyleFileTests.cpp:421-436` | `guess()` weighting, ties, passing notes, one-note refusal |
| `StyleFileTests.cpp:312-334` | `applyChord`: drums unchanged, bass C→F and G→C, slot id kept, Timeline untouched, cancel silences non-drums |
| `StyleFileTests.cpp:500-604` | Real-shaped `Ctb2`: field decode, byte-exact rule re-emit, cancel gating, guitar nearest chord note, section events (SysEx/tempo/text), `Cntt` kept, `.prs`/`.STY` extensions |
| `StyleFileTests.cpp:266-310,378-419` | `toSong`/`fromSong` grid and setup folding, project-file round trip, per-section vs all-section rule edits through the file |
| `StyleFileTests.cpp:438-498` | `sectionFromTimeline`, including per-section "written in" chord and C-major normalisation |
| `StyleFileTests.cpp:635-697` | `fillTimeline`: bar-wise chords, section repeat, note cut at the bar line, channel reassignment, missing section |
| `StyleFileTests.cpp:17-121` | The only CASM writer fixture (`demoStyle`), both formats, idempotent second write |
| `StyleSampleTests.cpp:43-88` | Every part/rule/setup/extra survives write→read for real files; `refused == 0`; needs `ARRANGER_STYLE_SAMPLES` |
| `AuditFixParsersTests.cpp:105-155` | Short `Ctb2` (26..39 B) → `extra`, no throw; invalid meter and huge setup tick → no divide-by-zero/hang |
| `FileEdgeCaseTests.cpp:116-130,225-230` | Every prefix length reads without `bad_alloc`; non-ASCII filename save/load |
| `ChordTests.cpp:22-90` | Chord finding from tracks (half-bar/bar steps, drums excluded, passing notes) |
| `ChordInputTests.cpp:55-112,175-208,264-280` | Typed chords/numerals/holds, chord lane, voicings, `moveNote` job mapping (not used by style conversion) |
| `PsrStyleTests.cpp:7-28`; `PsrFileRulesTests.cpp:223-242` | PSR remote-control SysEx and PSR **song** export — not style parity |

### 6.2 Fixtures on disk

| Path | Size | Role |
|---|---|---|
| `examples/demo-style.sty` | 3940 B | The only style file. `MThd` (division 1920) + `MTrk` (3918 B): `MThd`, `MTrk` only, **no CASM/CSEG/Sdec/OTSc**. Markers `SFF2`, `SInt`, `Main A`, `Main B`, `Main C`, `Main D`, `Fill In AA` |
| `tests/fuzz/regressions/style/note-number-over-127.bin` | 4520 B | Style-like file (same opening, no CASM) that must be refused or read without crashing; replayed every run (`tests/fuzz/README.md`) |
| `tests/fixtures/*` | README 819 B; tone.mp4 7331, tone.mkv 49151, tone.webm 6503, silent-video.mp4 893 | Video fixtures only; no style fixtures |
| `tests/golden/*.json` | 1180, 4548, 2954, 3188, 52751, 1982, 4654 | Score/beam/drum/render goldens; **none for styles** |

Demo geometry (parsed): setup bar 1 bar (7680 ticks = 4 QN); `Main A`..`Main D` 4 bars each (30720 ticks); `Fill In AA` 1 bar (7680); track ends at 138240; channels 10-15; drums 36/38/42; channels 11-15 are C natural minor.

### 6.3 Baseline gaps

No golden pins transposed output; conversion tests are hand-written assertions. The only shipped style exercises the identity path (§3.7). No real Yamaha style is in the repo. No test pins `plays` for masks read from a file (only the hand-built `0x07ffffffff` case).

---

## 7. Minimal first parity subset (dependency order)

Must work to play `examples/demo-style.sty` and save it back:

1. **Tick base.** 960 PPQ; file division via round-half-away; 4/4 default (`StyleFile.cpp:361-363`; `Time.h:10,13`).
2. **Container read.** `MThd` + first `MTrk`, running status, metas `0x2f/0x03/0x51/0x58/0x06`, SysEx `F0/F7`, channel messages; reject SMPTE and a missing SFF marker (`:342-530`).
3. **Section split.** `SInt` setup bar, marker → section, length = next marker − start, held notes closed at the section end (`:370-403`).
4. **Note capture.** FIFO per (channel, pitch); zero-velocity note-on = off; controls per channel (`:500-521`).
5. **Model.** `Style`/`StyleSection`/`Clip`/`Note`/`StyleEvent`, channel-keyed parts, per-section events (§1).
6. **Default channel rules.** drums 9/10 bypass; bass melody+bass+`pitchShiftToRoot`; others `rootFixed`+`chord`; all source CMaj7. Without this the demo has no rules (`StyleConvert.cpp:38-65`).
7. **Chord table and `tones()`.** 35 rows with third/fifth/seventh and the 7-note scale (`StyleChord.cpp:25-39,170-193`).
8. **`transpose` exactly as §3**, including step 1 before step 2, tie-down, the `12` rule and the octave fold.
9. **`plays`** with the documented polarity, plus whole-clip clear (§3.3).
10. **`applyChord`.** Per-section rule lookup by name, transposed copy of every style slot, Timeline untouched (`StyleConvert.cpp:308-314,494-527`).
11. **Scene playback.** Bar-quantised launch, pending replace, per-track release at the boundary, slot loop from `launchTick`, Timeline suppression (`Engine.cpp:278-289,326-350,398-444,563-579`).
12. **Writer.** `MThd` + markers + setup bar + sections + notes/controls + CASM only when rules exist + `extra` verbatim (§5).
13. **Rule editing and re-save.** Per-section vs all-section sets, `special` and `extra` preservation, `format`-chosen `Ctab`/`Ctb2` (`StyleConvert.cpp:316-369`; `StyleFile.cpp:223-315`).
14. **Real-file rules.** `Ctb2`/`Ctab` codec, SFF1 mapping table, `middleLow/High` selection, mask gating. Needed for any real `.sty`, not for the demo.

Defer: guitar NTR (no fixture), `Rtr` behaviour (nothing consumes it), NTT scale flavours (collapse to melody today; decide before porting), `PsrExport`, PSR SysEx, `StyleFromAudio`, `SongForm` fills, `chord_input`, `chords::find`, MIDI-port output of style parts, audio slot playback.

---

## 8. Recommendation per concern

| Concern | Decision | Reason |
|---|---|---|
| Section/part data model | **Port** | Small, JUCE-free, and the scene↔section mapping is the whole editor contract (`Style.h:34-66`). |
| SFF container read | **Port** | Rules are simple and the safety fixes (prefix truncation, short `Ctb2`) are known. |
| SFF container write | **Port, then redesign the layout** | Output is a normalised rewrite, not Yamaha's layout; byte-stable save needs a decision (§5). |
| `Ctab`/`Ctb2` codec | **Port** | Offsets confirmed by a real-shaped fixture; SFF1/SFF2 differences are contained. |
| Mute-mask polarity | **Verify before porting** | The gating model rests on one unverified assumption (§2.5). |
| `transpose` | **Port exactly, then refactor** | Tests pin many quirks (tie-down, `12`, highKey drop); a clean rewrite would change audible output. |
| NTT flavours / RTR | **Defer, keep data** | Stored and edited but unused; porting behaviour would invent semantics. |
| Chord model + type table | **Port** | Shared by conversion, detection, typing and the score (`StyleChord.cpp:25-39`). |
| Chord-follow trigger | **Redesign** | Whole-clip re-render cuts notes and costs O(style) per chord change (§4.2). |
| Section launch/quantise | **Port behaviour, redesign structure** | Users expect bar-quantised launches; per-track pending is incidental (§4.1). |
| Fill/break/variation state machine | **Redesign** | Nothing exists; define it from keyboard behaviour, not from this code. |
| Note-off ownership | **Port** | Per-track bitset with captured channel/port is the correct minimum. |
| Round-trip preservation policy | **Redesign** | Choose explicitly between model-normalised save and byte preservation; today it is the former (§5). |
| `StyleSampleTests` gate | **Port with a fixture corpus** | Only real-file safety net; it currently needs an external folder. |
| `PsrExport`, `SongForm`, `StyleFromAudio`, `chord_input` | **Defer** | Adjacent features with their own tests. |

---

## 9. Unverified items and what would verify them

| (U) item | How to verify |
|---|---|
| `chordMute`/`noteMute` polarity (1 = plays) | One real PSR style with a known resting part, plus Yamaha's bit definitions, or keyboard playback. |
| Yamaha chord type `0x22` = cancel | Yamaha Data List chord table; compare `PsrStyle.cpp:47-56` with the CASM spec. |
| Real files never use a 2nd `MTrk` | A corpus scan with a chunk walker; the reader keeps only the first. |
| `editable` semantics beyond 0/1 | Corpus histogram of byte 10 in Ctab/Ctb2. |
| `middleLow/High` equal Yamaha's note limits | One real `Ctb2` split part compared note by note on hardware. |
| Bar-quantised launch matches the keyboard | Manual/listening test; the app manual says only "The section starts at the subsequent bar" (`docs/user-manual.md:842-843`). |
| A3 octave-up case (`sourceRoot > highKey`) | A crafted rule plus unit test; no shipped test covers it. |

---

## 10. Sources

Required files, all under `/Users/far-mini/repos/arranger-midi-daw/` (line counts): `src/core/include/arranger/core/Style.h` 153, `StyleRules.h` 83, `StyleChord.h` 80, `PsrStyle.h` 48, `PsrExport.h` 56, `ChordFinder.h` 50, `ChordInput.h` 215; `src/core/src/StyleFile.cpp` 698, `StyleConvert.cpp` 640, `StyleChord.cpp` 400, `PsrStyle.cpp` 101, `PsrExport.cpp` 353 (read 1-120, 325-351), `ChordFinder.cpp` 274, `ChordInput.cpp` 1022; tests `StyleFileTests.cpp` 697, `StyleSampleTests.cpp` 88, `PsrStyleTests.cpp` 29, `PsrFileRulesTests.cpp` 242, `PsrExportTests.cpp` 154, `ChordTests.cpp` 138, `ChordInputTests.cpp` 333.

Also read: `src/core/include/arranger/core/Song.h` (model), `Time.h`, `src/core/src/ProjectFile.cpp:250-345`, `SongForm.cpp` (fill/transition ranges), `src/engine/src/Engine.cpp` (slot/note/snapshot ranges), `Playback.cpp` (contentKey), `src/app/Session.cpp`, `src/app/ui/StyleRuleEditor.cpp`, `tests/AuditFixParsersTests.cpp:105-155`, `tests/FileEdgeCaseTests.cpp:116-130,225-230`, `tests/ProjectRoundTripTests.cpp`, `tests/fuzz/README.md`, `docs/user-manual.md:779-955`, `docs/technical-overview.md:46-48`.
