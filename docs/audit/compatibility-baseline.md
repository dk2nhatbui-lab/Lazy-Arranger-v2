# Compatibility baseline — C++ oracle to Rust parity plan

**Oracle identity.** The audited source is `/Users/far-mini/repos/arranger-midi-daw` at
`8ca41644a2be5f5677bce8cae060a55f7b19e8fc`. Local git reports it as the tip of
`fix/audit-critical-high`: `git rev-list --left-right --count origin/main...HEAD` returns `0 10`, so it is
10 commits ahead of `origin/main` `975e97d1004b8a9813339fddbc258c3bcf5eb6a5` and 0 behind. The tree was
read, not built and not modified.

**Rule for recapture.** Every golden must record the C++ commit, the branch, the build configuration and
the capture command. A golden without that provenance is not admissible. Any later oracle must be pinned
to a commit, not to `main`.

**Rust state.** `cargo test --workspace` passes 116 tests. `cargo clippy --workspace --all-targets --
-D warnings` and `cargo fmt --all --check` pass. No arranger parity exists yet.

**Fixture identity.** `tests/fixtures/demo-style.sty` is byte-identical to `examples/demo-style.sty`
(SHA-256 `b0e241a2…`); `legacy-cpp-v3.arranger` to `examples/demo.arranger` (`f7b24146…`); and
`legacy-cpp-v4.arranger` to `examples/Copper Lights/Copper Lights (Audio).arranger` (`a9e4f729…`).
See §5 for legal status.

## 1. What the C++ behaviour oracle is today

### 1.1 The exact conversion and ordering rules

**Tick to sample.** `ProcessContext::sampleForTick` (`src/engine/src/Transport.cpp:8-16`):

```text
sampleForTick(tick) =
    anchorSample + (i64) ceil((secondsAt(tick) - anchorSeconds) * sampleRate - 1e-6)
```

`secondsAt` is a double computed from the song start through the step tempo map
(`src/core/src/TempoMap.cpp:79-83`). Rounding is ceiling, so an event never sounds early; the `1e-6`
sample epsilon keeps an event that lands exactly on a sample from slipping one sample later.

**What Rust reproduces.** `crates/music-core/src/tempo.rs::seconds_to_samples` computes
`(seconds * sample_rate - 1e-6).ceil() as i64`, and `TempoMap::samples_at` applies it to
`seconds_at(tick)`. That is the C++ formula with `anchorSample = 0` and `anchorSeconds = 0`, which is the
play-from-song-start case. `crates/sequencer/src/lib.rs::PreparedPattern::prepare` compiles every note-on
and note-off to an absolute sample with `samples_at`.

**What Rust does not reproduce.** There are no anchor fields. `Scheduler` keeps an absolute sample cursor
(`position`) and `seek` repositions the cursor; it never re-derives event samples from a new anchor. C++
re-anchors on play, stop, locate, rate change and tempo-map change
(`Transport.cpp:27-41,55-83`). For a start at song start
the two agree exactly. For a start at tick `T > 0`, C++ computes
`ceil((secondsAt(t) - secondsAt(T)) * rate - 1e-6)` per event while Rust uses the absolute
`ceil(secondsAt(t) * rate - 1e-6)` minus the block start. Those can differ by one sample. Pin this with
G20 and keep the tolerance in §4.

**Event order.** The C++ key is `(tick, order)` with `order` = note-off `0`, controls/setup/program/bend
`1`, note-on `2` (`src/engine/include/arranger/engine/Playback.h:26`). Build-time sort is
`std::stable_sort` on `(at, order)` per track (`Playback.cpp:253,267-268,332`). Across tracks,
`Engine::sequence` walks snapshot track order (`Engine.cpp:551-580`). The final block sort is a stable
insertion sort by `sampleOffset` only (`MidiEventBuffer.h:73-88`; call at `Engine.cpp:956`). Therefore at
equal sample offsets C++ orders **track index first**, then `(tick, order)`, then insertion order.

**What Rust reproduces.** `MidiMessage::sort_rank` returns `0` for note-off, `1` for control/program/bend
and `2` for note-on (`crates/music-core/src/midi.rs:314-320`). `PreparedPattern::prepare` sorts with
`sort_by_key(|e| (e.sample, e.message.sort_rank(), e.track.get()))` using a stable sort
(`crates/sequencer/src/lib.rs:120-121`). Within one track this matches the oracle. **Across tracks it does
not**: Rust puts rank before track, so at one sample all note-offs across all tracks precede all controls,
which precede all note-ons; C++ completes track 0 before track 1. Rust also breaks ties by track **id**,
C++ by track **index**. Before any multi-track golden is accepted, either change the Rust key to
`(sample, track_index, sort_rank)` or record this as a deliberate divergence. The current
`simultaneous_events_order_by_track` test only checks that note-ons follow track-id order, so it does not
catch the rank-first difference.

**Block-end clamp.** `Engine::offsetFor` computes `clamp(sample - blockStart, 0, numSamples - 1)` and then
adds `blockOffset` (`src/engine/src/Engine.cpp:354-358`). Rust
`ScheduledEvent::offset_in(block_start, frames)` returns
`(sample - block_start).clamp(0, frames - 1)` (`crates/sequencer/src/lib.rs:36-40`), proven by
`block_offsets_clamp_inside_the_block`. This is an exact match. One caveat: `Scheduler::process_block`
emits only events in `[position, position + frames)` and consumes earlier events without routing them
through `offset_in`; in normal transport use `seek` and `publish` keep the cursor at or after `position`,
so a late event is not expected.

**Per-track note bookkeeping.** C++ keeps `ActiveNotes` per track: a `bitset<128>` plus the channel and
port captured at note-on (`Engine.h:334-342`; `Engine.cpp:360-381`). On stop, section launch and panic it
releases only what that track started, then sends CC64 = 0 if the pedal was down
(`Engine.cpp:439-466,566-573`). A same-pitch retrigger sends note-off before the new note-on
(`Engine.cpp:365-367`). A note-off for a pitch the track does not hold is dropped, not forwarded
(`Engine.cpp:372-376`). This has **no Rust equivalent**: the scheduler emits exactly the note-on and
note-off pairs in the model. `NoteCapture` performs FIFO pairing on input
(`crates/midi-devices/src/capture.rs::NoteCapture::ingest`), which is a different concern.

**Redundant note-off suppression.** Consequence of the same bookkeeping: C++ suppresses a note-off for a
pitch that its track does not hold. Rust does not; it emits one note-off per note in the model. When two
notes of the same pitch overlap, both ends produce an off, and the second off is redundant. This is a
stuck-note and MIDI-traffic risk on hardware, and it must be fixed before the arranger milestone.

### 1.2 Rule-by-rule status

| Rule | C++ evidence | Rust today | Rust location | Verdict |
|---|---|---|---|---|
| Anchored tick→sample with ceiling and `1e-6` epsilon | `Transport.cpp:8-16` | `ceil(seconds_at(tick) * rate - 1e-6)`; no anchor state | `crates/music-core/src/tempo.rs::seconds_to_samples`, `TempoMap::samples_at` | Partial: exact at anchor 0; locate differs by ≤1 sample. |
| Event key `(tick, note-off 0, controls 1, note-on 2)` | `Playback.h:26`; `Playback.cpp:253` | `sort_rank` 0/1/2; sorted by `(sample, rank, track_id)` | `crates/music-core/src/midi.rs::sort_rank`; `crates/sequencer/src/lib.rs::PreparedPattern::prepare` | Partial: same track exact; track order across tracks differs. |
| Block-end clamp `clamp(sample - blockStart, 0, numSamples-1)` | `Engine.cpp:354-358` | `clamp(0, frames-1)` | `crates/sequencer/src/lib.rs::ScheduledEvent::offset_in` | Exact. |
| Zero look-ahead, window `[tickAtStart, tickAtEnd)` | `Engine.cpp:891-934` | Block is `[position, position + frames)` | `crates/sequencer/src/lib.rs::Scheduler::process_block` | Exact for sample windows. |
| Per-track active-note set, channel/port capture, pedal | `Engine.h:334-342`; `Engine.cpp:360-381,453-466` | None | — | Not reproduced. |
| Retrigger sends note-off first | `Engine.cpp:365-373` | Ordering only; no active-note check | `PreparedPattern::prepare` | Partial. |
| Unknown note-off dropped | `Engine.cpp:372-376` | None; all events are emitted | `Scheduler::process_block` | Not reproduced. |
| Stop releases held notes, then CC123 on used channels | `Engine.cpp:439-451,481-486` | None | — | Not reproduced. |
| Loop wrap splits the block, releases, re-locates, shifts slot phase | `Engine.cpp:896-951` | None | — | Not reproduced. |
| Tempo map is step-only, seconds from song start | `TempoMap.cpp:79-83` | Same shape; seconds computed from absolute positions | `TempoMap::seconds_at` | Equivalent numbers. |
| Tick from seconds | Returns a double tick (`Transport.cpp:18-23`) | Floors to `i64` | `TempoMap::tick_at_seconds` | Deliberate divergence, documented in `contracts.md`. |
| Invalid tempo | Clamped 20..400, NaN→120 (`Time.cpp:8-13`) | Rejected with `MusicError::Tempo` | `TempoMap` | Deliberate divergence; the command layer reports it. |
| Buffer overflow | Drops and counts (`MidiEventBuffer.h:60-69`) | Drops, counts, never reallocates | `EventBuffer::push` | Equivalent. |

### 1.3 What this means for capture

Capture goldens from a transport that starts at song start, with the anchor at sample 0 and tick 0. That
is the only configuration where the sample formula is exact. Capture a separate locate/resume golden
(G20) and give it the ≤1 sample tolerance. Do not compare block offsets across different block sizes;
compare absolute samples and the ordered `(status, channel, data1, data2)` list.

## 2. Chord conversion specification (implementable)

Source: `StyleChord.cpp:318-398`, `std::optional<int> style_chord::transpose(int pitch, const
StyleChannelRule& rule, const Chord& chord)`, and `style_chord::plays` at `StyleChord.cpp:309-316`.

### 2.1 Inputs, output and data

* `pitch: u8`, 0..127.
* The rule record: one of `transpose`, `lowRange`, `highRange`, each with `ntr`, `ntt`, `highKey`,
  `lowLimit`, `highLimit`, `rtr`; plus `sourceRoot` 0..11, `sourceChordType`, `bass: bool`, `noteMute`,
  `chordMute`.
* `chord { root: 0..11, type: 0..34, bass: -1 or 0..11 }`.
* `kTypes[35]`, one row per type: `{ suffix, third, fifth, seventh, scale[7] }`. The full table is at
  `StyleChord.cpp:25-39`; the audit note does not print it. Transcribe it before coding. The pinned
  cases in §2.4 constrain the common rows but not all 35.
* Output: `None` means the note must not sound; `Some(0..127)` is the new pitch. The caller keeps the
  original `start`, `length` and `velocity`, and drops `None` (`StyleConvert.cpp:513-521`).

Use `mod12(x) = ((x % 12) + 12) % 12`.

### 2.2 Ordered steps (exact decision order)

**Step 0 — choose the record.** If `pitch < middleLow`, use `lowRange`. If `pitch > middleHigh`, use
`highRange`. Otherwise use `transpose`. The comparisons are strict, so `pitch == middleLow` uses the
middle record. Defaults `middleLow = 0`, `middleHigh = 127` select the middle record for every pitch.

**Step 1 — bypass before the no-chord test.** Let `fixedRoot = (ntr != Ntr::rootTransposition)`. If
`fixedRoot && ntt == Ntt::bypass`, return `Some(pitch)` unchanged. This runs before step 2, which is why a
drum part with this pair survives a no-chord (`StyleFileTests.cpp:214-219`).

**Step 2 — no chord.** If `chord.type == ChordType::cancel`, return `None`.

**Step 3 — target root.** `targetRoot = mod12(if record.bass && chord.bass >= 0 { chord.bass } else
{ chord.root })`. The slash bass changes the root only for records with the bass flag; it never adds a
bass note, and chord parts ignore it.

**Step 4 — guitar.** Only when `ntr == Ntr::guitar` (`:332-356`), and `ntt` is ignored:

1. `shift = mod12(targetRoot - sourceRoot)`; if `shift > 6`, `shift -= 12`.
2. `moved = pitch + shift`.
3. Search outward for the nearest pitch of the **target** chord: for `d = 0..=6`, test `moved - d` first,
   then `moved + d`. Skip results outside 0..127. The first pitch whose class is in the target chord wins;
   ties therefore go down.
4. If `lowLimit <= highLimit && highLimit - lowLimit >= 11`, add or subtract 12 until inside the limits.
5. Clamp to 0..127 and return.

**Step 5 — source geometry.** `from = kTypes[info(sourceChordType)]`, `to = kTypes[info(chord.type)]`,
`rel = mod12(pitch - sourceRoot)`, `base = pitch - rel`. `info()` clamps a type above 34 to 34.

**Step 6 — interval mapping.**

| `ntt` | Rule |
|---|---|
| `chord` | `chord_tone(rel, from, to)`, else `scale_tone(rel, from, to)` |
| `melody` and every other value except `chord` and `bypass` | `t = chord_tone(...)`; use `t` only if `t < 12`, else `scale_tone(...)` |
| `bypass` | `interval = rel`; only reachable when `ntr == rootTransposition`, because step 1 returned for a fixed root. The root may still move in step 7. |

`chord_tone(rel, from, to)`: `rel == 0 → 0`; `rel == from.third → to.third`; `rel == from.fifth →
to.fifth`; `rel == from.seventh → to.seventh`, or `12` when `to.seventh` is absent ("use the root above");
otherwise no result. `scale_tone(rel, from, to)`: find the highest `i` with `from.scale[i] <= rel`; keep
the chromatic offset `offset = rel - from.scale[i]`; return `to.scale[i] + offset`. The scale comes from
the chord type, never from `ntt`.

**Step 7 — root placement.**

* If `ntr == Ntr::rootTransposition`: `hk = clamp(highKey, 0, 11)`; `shift = mod12(hk - sourceRoot) -
  mod12(hk - targetRoot)`; `result = base + interval + shift`.
* Otherwise (`fixedRoot`; guitar never reaches here): find the nearest pitch whose class is
  `mod12(targetRoot + interval)`. Walk `up` and `down` from `pitch` to the first match. Choose `down` when
  `up - pitch >= pitch - down`; ties go down.

**Step 8 — fold and clamp.** If `lowLimit <= highLimit && highLimit - lowLimit >= 11`, add or subtract 12
until the result is inside. Then clamp to 0..127. A range narrower than an octave clamps, not folds.

### 2.3 Part gating (`plays`)

```text
if chord.type == cancel:
    plays = ((chordMute >> 34) & 1) != 0
else:
    plays = ((noteMute >> mod12(chord.root)) & 1) != 0
         && ((chordMute >> chord.type) & 1) != 0
```

Defaults are `noteMute = 0x0fff` and `chordMute = 0xffffffffff`, which let everything play
(`StyleRules.h:41-42`). `plays` is evaluated once per clip; a false result clears the **whole clip**
(`StyleConvert.cpp:509-510`), not one note. Note the disagreement for an out-of-range type: `info()`
clamps a type above 34 to 34, but `plays` shifts `chordMute` by the raw type (`StyleChord.cpp:38,43,315`).
Port both exactly; do not normalise the type at the boundary.

### 2.4 Pinned acceptance cases

From `StyleFileTests.cpp:173-238`. Source CMaj7 (`sourceRoot 0`, type 2), bass rule `melody`, `highKey 6`,
limits 28..60:

| Case | Input | Output |
|---|---|---|
| C | 36, chord C | 36 |
| F | 36, chord F | 41 |
| G | 36, chord G | 31 (above highKey, octave down) |
| E over Am | 40, chord Am | 36 |
| G over Dm7b5 | 43, chord Dm7b5 | 44 |
| D | 38, chord D | 38 |
| C/E with `bass` | 36, chord C, bass E | 40 |

Chord rule `rootFixed` + `chord`:

| Case | Input | Output |
|---|---|---|
| E over Am | 52 | 48 (nearest; tie down) |
| G over E | 55 | 52 |
| B over C triad | 59 | 60 |
| B over C7 | 59 | 58 |
| E over Csus4 | 52 | 53 (third becomes fourth) |

Also required: drum rule with `rootFixed` + `bypass` returns the pitch on a cancel chord
(`StyleFileTests.cpp:214-219`); a cancel chord with a chord part returns `None`.

### 2.5 The seven named ambiguities

| Id | Ambiguity | What the C++ does | Decision needed |
|---|---|---|---|
| A1 | All eight scale-flavoured NTT values | Behaviourally identical to `melody`; `info()` never reads NTT; the scale comes from the target chord type (`StyleChord.cpp:366-371`). | Port as melody, or implement real NTT scales. Do not invent semantics. |
| A2 | `melody` refuses a mapped interval of 12 | `melody` falls through to the scale path; `chord` accepts `12` (`:365` vs `:370`). | Port exactly; add a test. |
| A3 | Transposing shift can move up an octave | When `sourceRoot > highKey`, `shift` carries `+12` (`:377-379`). No test covers it. | Port exactly; add a crafted test before trusting. |
| A4 | Drums are not special-cased | A rhythm rule with `ntr = rootTransposition` is transposed. The default rule avoids this with `rootFixed` (`StyleConvert.cpp:46-51`). | Keep the rule-based behaviour; do not add a drum exception. |
| A5 | Degenerate source intervals | For source type `1+5`, both `third` and `fifth` are 7, so a fifth maps to the target third. For `1+8`, both are 0, so every note maps to 0 (`StyleChord.cpp:37,51-54`). | Port the table verbatim; do not "fix" the data. |
| A6 | Mute-mask polarity | A set bit is treated as "this chord type may play" (`StyleRules.h:42`; `StyleChord.cpp:315`; `StyleFileTests.cpp:550-551`). No shipped real file verifies it. | **Highest-risk item.** Verify on one real PSR style before porting. |
| A7 | `fixedRoot` + `bypass` returns before cancel | A non-drum part with that pair also ignores a no-chord (`:324-326`). No known file uses it. | Port exactly; add a test. |

**Mute-mask polarity risk.** The whole gating model rests on one unverified assumption: bit set = plays.
Defaults hide it, and the only test uses a hand-built mask (`0x07ffffffff`). A real style with a resting
part is the only proof. If polarity is inverted, every part plays or none does, and the failure is
audible but not detectable from the demo style. Verify before Stage 4. Record the result in
`docs/audit/compatibility-baseline.md` and in a unit test that names the file and hash.

## 3. First 20 golden scenarios to capture

Capture them from the C++ build in this order. "Config" is tempo / sample rate / block size. Tolerances
refer to sample position; see §4 for the full policy. Rust test names are planned names.

| Id | Input | What it pins | Tolerance | C++ basis | Rust test |
|---|---|---|---|---|---|
| G01 | `demo-style.sty`; Main A; Cmaj7; 120 / 48 kHz / 512 | Identity path: pitch, velocity, tick start/length, sample positions | 0 samples; 0 ticks | `StyleFileTests.cpp:173-238,312-334` | `golden_demo_style_cmaj7` |
| G02 | `demo-style.sty`; Main A; F; 120 / 48 kHz / 512 | Minor→major scale path (`Eb4`→`Ab4`); derived, not test-pinned | 0 ticks; 0 samples | `StyleConvert.cpp:60-69`; note §3.7 (D) | `golden_demo_style_f_major` |
| G03 | `demo-style.sty`; Main A; Cmaj7 then G; 120 / 48 kHz / 512 | `highKey` octave drop (`36`→`31`); transposing bass | 0 ticks; 0 samples | `StyleFileTests.cpp:173-238` | `golden_demo_style_g` |
| G04 | `demo-style.sty`; Fill In AA; Cmaj7; 120 / 48 kHz / 512 | One-bar fill length, loop period, held-note close at section end | 0 ticks; ≤1 sample at wrap | `StyleFileTests.cpp:635-697`; `StyleFile.cpp:389-392` | `golden_fill_aa` |
| G05 | `demo-style.sty`; Main A→Main B→Fill In AA at bars 1/2/3; C; 120 / 48 kHz / 512 | Bar-quantised launch at exact samples 96000, 120000, 192000; pending replace-per-track | 0 samples | `TimingTests.cpp:110-143`; `ClipGridTests.cpp` | `golden_section_launch` |
| G06 | `demo-style.sty`; Main A; loop 2 passes; C; 120 / 48 kHz / 1..1024 | Loop repeat period and note re-attack | ≤1 sample per wrap | `TimingTests.cpp:78-108`; `SequencerTests.cpp:82-94` | `golden_loop_repeat` |
| G07 | `demo-style.sty`; Main A; loop across a block edge; C; 120 / 48 kHz / 1000 and 512 | Block split at loop end; release and re-locate | 0 samples for both block sizes | `SequencerTests.cpp:96-128`; `Engine.cpp:896-951` | `golden_loop_block_edge` |
| G08 | No style; 300 linear notes; 30 tempo changes; 30..240 / 48 kHz / 1..1024 | Tempo-map exactness through many changes | ≤1 sample per note | `TimingTests.cpp:48-76` | `golden_tempo_scan` |
| G09 | No style; 600 beats at 97.3; 44.1 kHz / 1024 | Drift at an odd rate over a long song | ≤1 sample | `TimingTests.cpp:145-174` | `golden_odd_rate` |
| G10 | Metronome only; 112.37, 97, 133.33; 44.1/48/96 kHz / random | One-hour drift and buffer independence | 0 samples | `MetronomeTests.cpp:48-66,68-82` | `golden_metronome_hour` |
| G11 | Metronome only; 3/4 and 6/8; 120 / 48 kHz / 512 | Accent pattern and downbeat samples | 0 samples | `MetronomeTests.cpp:96-118` | `golden_metronome_accents` |
| G12 | Two notes, same pitch, boundary at tick 960; 120 / 48 kHz / 512 | Note-off before note-on at one sample; retrigger | 0 samples; exact order | `Playback.cpp:253,267-268` (derived; no existing C++ test pins the sample collision) | `golden_same_sample_order` |
| G13 | One held note; stop at sample 6000; resume at tick 1920; 120 / 48 kHz / 512 | Stop release, position retention, resume | 0 samples | `SequencerTests.cpp:174-186`; `EngineTests.cpp:10-34` | `golden_stop_resume` |
| G14 | Two tracks; mixer-only edit, then content edit, during playback; 120 / 48 kHz / 128 | Unchanged tracks keep sounding notes; changed track releases | 0 samples | `SequencerTests.cpp:188-208`; `Engine.cpp:488-544` | `golden_edit_holds_notes` |
| G15 | Thru input on channel 4 to port 3 at input offset 10; 120 / 48 kHz / 512 | Port/channel remap and input offset | 0 samples | `SequencerTests.cpp:230-245` | `golden_thru_route` |
| G16 | Recorded note at `960 + 240*960/24000`; 120 / 48 kHz / 512 | Capture-to-tick conversion and rounding | Exact expression; no rounding of the formula | `SequencerTests.cpp:210-228` | `golden_record_position` |
| G17 | SMF: 140 BPM, 96 PPQ source, 960 PPQ export; 48 kHz | PPQ rescale, tempo µs rounding, track layout | Tempo ±0.01 BPM; ticks exact | `MidiFileTests.cpp:44-158` | `golden_smf_round_trip` |
| G18 | `demo.arranger` (v3); 112 / 48 kHz / 128 | 6 tracks, 265 notes, clip offsets→absolute ticks, sample positions | 0 samples; model equality | `ProjectRoundTripTests.cpp:51-73`; open/export of `demo.arranger` must be captured manually | `golden_legacy_v3` |
| G19 | `Copper Lights (Audio)` (v4); 126.000126 / 48 kHz / 128 | Schema-4 import: 10 named tracks, 0 notes, unknown keys preserved | Model equality | `ProjectRoundTripTests.cpp:99-115` | `golden_legacy_v4` |
| G20 | Locate/resume: start at tick 2880 after playing 1920; 120 / 48 kHz / 512 | Re-anchor behaviour versus the absolute Rust cursor | ≤1 sample | `EngineTests.cpp:10-34`; `Transport.cpp:55-83` | `golden_locate_resume` |

Add one non-sample check outside this table: `demo-style.sty` read→write must equal the bytes the app
wrote (`StyleFileTests.cpp:118-119`). Byte equality with a Yamaha-authored file is **not** claimed; the
C++ writer re-serialises.

## 4. Tolerance policy

| Compared value | Tolerance | Why |
|---|---|---|
| Sample position, start at song start, no loop | **0 samples** | Same `ceil(x - 1e-6)` on the same `f64` seconds gives the same integer. |
| Sample position after locate, loop wrap, rate change or anchor change | **≤1 sample** | C++ ceils a difference from the anchor; Rust ceils an absolute time. A difference of two ceils can move by one sample. Loop period is also a difference of two ceils (`TimingTests.cpp:98-105` allows 1). |
| Note start and length in ticks | **0 ticks** | Ticks are integers; conversion is textual or exact division. |
| Velocity | **Exact** | Integer data, no arithmetic. |
| Event order at one sample | **Exact sequence** | Order is a contract, not a numeric approximation. Fix the cross-track key before comparing (§1.1). |
| Tempo after SMF round trip | **±0.01 BPM** | C++ stores integer microseconds per quarter (`MidiFile.cpp:512`); round trip shifts by less than 0.01 (`MidiFileTests.cpp:53`). |
| Note lengths in samples | Derived from tick ends, **0 or ≤1** by the same rule as start positions | Do not compare durations independently of starts; compare both endpoints. |
| Project model after import | **Exact equality** on the fields carried | Import is lossy by design; compare the report and the preserved `extra` keys, not the source document. |

Comparison procedure: sort each side by absolute sample, then by the agreed order key; compare
`(sample, status, channel, data1, data2)`; then compare tick geometry per note. Never compare block
offsets across different block sizes.

## 5. Fixtures and provenance

| File | Size | SHA-256 | Origin | Legal status | Use |
|---|---|---|---|---|---|
| `tests/fixtures/demo-style.sty` | 3,940 B | `b0e241a2b027c7279bda7d8c8ab31168c12e9ff0e4801cbd564735d26bc99141` | Byte-identical to `examples/demo-style.sty` | C++ repo has no licence; all rights reserved | Local oracle only; replace before release |
| `tests/fixtures/legacy-cpp-v3.arranger` | 26,085 B | `f7b241460937f1a93f43f98792be80b0d8353dd9d2381dbbd5df4b1b6a48b97e` | Byte-identical to `examples/demo.arranger` | Same | Local oracle only; replace before release |
| `tests/fixtures/legacy-cpp-v4.arranger` | 9,603 B | `a9e4f729ff1a86628d76f79f2602cce174b8de8cb7bc86b1669677a5e325f6f8` | Byte-identical to `examples/Copper Lights/Copper Lights (Audio).arranger` | Same | Local oracle only; replace before release |

**Legal finding.** `cpp-surface.md` §D1 records no `LICENSE`, `COPYING` or `NOTICE` in the C++ repository,
on any ref; the release notes call the source private; the default is all rights reserved. §D3 says the
example songs and the Yamaha/Korg style fixtures must not be copied. The three files above are exact
copies, so the repository currently contains unlicensed data. They are acceptable as local audit inputs but
not as distributable fixtures. Replace them with self-authored fixtures, or obtain written permission,
before the first release. The README statement that no C++ code is copied is true for code, not for data.

**Still needed**

| Need | Why | Where it comes from |
|---|---|---|
| Real PSR style with CASM, SFF1 and SFF2, one with a known resting part | Only way to verify mute polarity, `Ctab`/`Ctb2` decoding and range selection. The demo has no CASM. | A licensed PSR style corpus, or a Yamaha Data List plus a user-owned style |
| C++ build with `ARRANGER_STYLE_SAMPLES` pointing at that corpus | `StyleSampleTests.cpp:43-88` is the only real-file safety net; it is skipped without the folder. | Build the pinned commit with the env var set |
| `Northern Fire` (MIDI and Audio), `Copper Lights` (MIDI) | Project parity beyond the one v4 audio fixture | C++ `examples/`, under the same legal restriction |
| C++ timing/sequencer/metronome goldens | G05–G17 need captured event lists, not just the C++ test assertions | Capture at `8ca4164` with rate, block and seed recorded |
| A self-authored style with CASM | Replaces the unlicensed demo and pins rules | Author with the C++ writer or a hex builder, record the generator |
| Fonts and SMuFL mapping | Notation parity needs OFL copies and re-derived codepoints | Bravura/Inter under OFL 1.1; SMuFL spec |

Provenance procedure for each captured golden: write `tests/golden/PROVENANCE.md` with scenario id, C++
commit, branch, build type, sample rate, block size, random seed, command line, capture date and the
SHA-256 of both the input fixture and the output. A golden without these fields is rejected in review.

## 6. Hardware test matrix

Ports seen on this machine with
`./scripts/rust.sh run -q -p lazy-arranger-cli -- ports`:

| Port | Direction | Device role | Checks that need it |
|---|---|---|---|
| Launchkey Mini MK3 MIDI Port | in / out | Keyboard | Note input, velocity, record timing, echo, panic |
| Launchkey Mini MK3 DAW Port | in / out | DAW control surface | Controller messages; not note input |
| XioSynth | in / out | Synth / controller template (ch 16) | Program/bank, echo, synth-editor pages |
| D-05 | in / out | Monotimbral synth | Program/bank, SysEx editor, echo |
| Yamaha PSR-S970 Port 1 | in / out | Arranger keyboard / tone generator | Note input, program/bank/reset, style playback destination |
| Yamaha PSR-S970 Port 2 | out only | Second PSR port (role unverified) | Output routing check only |
| X-TOUCH MINI | in / out | Controller surface | Fader/knob CC, MIDI learn |

**Status: all hardware results are unverified; hardware status stays `manual only`.** Listing is
enumeration only. Record and echo were not run against any of these ports. `enumeration_does_not_fail_on_a_machine_without_hardware` proves only that
enumeration returns without a device.

| Check | Needs a physical keyboard? | Status |
|---|---|---|
| Port enumeration, names, order | No | Test-verified only that enumeration returns without a device; exact names observed from the CLI, not asserted by a test |
| Note-on/off reception and channel | Yes | Unverified |
| Velocity capture | Yes | Unverified |
| Record duration and tick conversion | Yes | Unverified; pure conversion tested in `conversion_to_ticks_follows_the_tempo_map` |
| Echo to one output | Yes (input and output) | Unverified |
| Program/bank and reset bytes to PSR/D-05/XioSynth | Yes | Unverified; no device-profile code exists |
| Stop/panic leaves no stuck notes | Yes | Unverified; no panic code exists |
| Hot-plug disconnect/reconnect | Yes | Unverified |
| Controller faders and MIDI learn | Yes (X-TOUCH MINI, Launchkey) | Unverified |
| Second PSR port routing | Yes | Unverified |

### 6.1 Manual record steps

1. Run `./scripts/rust.sh run -q -p lazy-arranger-cli -- ports` and confirm
   `Launchkey Mini MK3 MIDI Port` appears under `inputs`.
2. Run:
   ```sh
   ./scripts/rust.sh run -q -p lazy-arranger-cli -- record \
     --in "Launchkey Mini MK3 MIDI Port" --out /tmp/take.mid --seconds 10 --tempo 120
   ```
3. Play C4, D4, E4, F4, one per second. Hold each for 400–500 ms and release before the next. Release all
   keys before the 10 s window ends.
4. Expected, **unverified**: `recorded 4 note(s) in 10 s`; no `unmatched note-off` warning; no
   `still held` warning; `wrote /tmp/take.mid`.
5. The CLI has no SMF inspect subcommand. Verify the file with an external MIDI reader or a future
   `smf::decode` check. Record the observed note starts and durations in the test log.
6. Repeat with `--in "Yamaha PSR-S970 Port 1"` and play four notes on the PSR.

**Known capture behaviour (code reading, unverified).** `record_to_song` passes `midir`'s timestamp
straight to `NoteCapture`, and `midir` reports microseconds since the input was connected, not since
recording started. The module doc says time zero is the start of recording, but no rebase exists in
`crates/midi-devices/src/devices.rs::record_to_song`. Expect the take to start at a small nonzero tick,
and do not treat the first note as tick 0 until this is fixed and tested.

### 6.2 Manual echo steps

1. Run:
   ```sh
   ./scripts/rust.sh run -q -p lazy-arranger-cli -- echo \
     --in "Launchkey Mini MK3 MIDI Port" --out "Yamaha PSR-S970 Port 1" --seconds 15
   ```
2. Play notes on the Launchkey for 10 s. The PSR must sound with its current voice.
3. Expected, **unverified**: `forwarded N message(s)` with `N > 0`; no stuck note after release; no
   error. Press sustain and release it; the pedal must pass through.
4. Repeat with `--out "D-05"` and `--out "XioSynth"`.
5. Kill the process during a held note (`Ctrl-C`). Expected failure: no note-off is sent, so the note
   sticks. Record this as a known limitation until panic exists.

## 7. Definition of parity complete (arranger milestone)

* [ ] Every golden G01–G20 is captured from C++ commit `8ca4164` (or a later pinned commit) with build
      configuration, seed and fixture hashes recorded, and passes in CI within §4 tolerances.
* [ ] The event order key matches the oracle at equal samples, including across tracks. Either the Rust
      sort is `(sample, track_index, sort_rank)` or the divergence is accepted in writing and tested.
* [ ] Anchor-0 sample positions are exact for every event. Locate/resume is within 1 sample and G20
      passes.
* [ ] `demo-style.sty` Main A–D and Fill In AA play the same pitch, velocity, tick start and tick length
      as the C++ build at Cmaj7 and at F.
* [ ] `plays` and `transpose` pass all §2.4 cases, including the `12`→root rule, highKey octave drop,
      sus4 third→fourth, tie-down and A3's octave-up case.
* [ ] Mute-mask polarity is verified on one real PSR style with a known resting part, with file hash and
      test recorded.
* [ ] One real SFF1 and one real SFF2 style with CASM import with a `complete`/`partial` report; unknown
      chunks and `special` bytes are preserved.
* [ ] Section launch is bar-quantised at exact samples; a pending launch replaces a pending launch per
      track; Timeline tracks go silent while a slot plays.
* [ ] Variation, fill and break behaviour is defined from keyboard behaviour and tested. It is **not**
      claimed as a C++ port because C++ has no such state machine.
* [ ] Per-track active-note bookkeeping exists: retrigger sends note-off first, unknown note-offs are
      dropped, CC64 is released, stop and panic clear every used port/channel.
* [ ] Loop wrap splits the block at the loop-end sample, releases held notes, re-locates and shifts slot
      phases. Loop period is within 1 sample per pass.
* [ ] A mid-bar chord change releases held notes only on tracks whose transposed content changed; other
      tracks keep sounding; slot phase is unchanged.
* [ ] SMF export matches the scheduled event list; 960 PPQ; tempo round trip within 0.01 BPM.
* [ ] A C++ v1–v4 project imports with a report and no silent JSON loss; unknown keys survive a save.
* [ ] No stuck notes in a stress run of 1,000 section launches with panic and stop interleaved.
* [ ] Hardware is verified on at least one named keyboard for record, echo, program/bank and hot-plug.
      Until then the hardware status stays `manual only`.
* [ ] The unlicensed C++ fixtures are replaced or licensed, and `tests/golden/PROVENANCE.md` is complete.
