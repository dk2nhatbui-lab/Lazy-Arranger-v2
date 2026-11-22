# C++ DAW engine: real-time, MIDI I/O and SMF audit

Target: `/Users/far-mini/repos/arranger-midi-daw` at commit `8ca41644a2be5f5677bce8cae060a55f7b19e8fc` (worktree clean).
Method: read-only source reading. All paths below are relative to the repo root. `(U)` marks a claim I could not verify from source.
Scope note: `src/engine` has no threads of its own; all threading comes from the JUCE host in `src/platform/juce`.

---

## 1. Clock ownership, callback, block size, sample rate

| Fact | Evidence |
|---|---|
| The audio device owns the sample clock. JUCE calls the host, the host calls the engine once per device block. | `src/platform/juce/JuceDeviceHost.cpp:377-423` |
| Callback entry point (verbatim): `void JuceDeviceHost::audioDeviceIOCallbackWithContext (const float* const* inputs, int numInputs, float* const* outputs, int numOutputs, int numSamples, const juce::AudioIODeviceCallbackContext&)` | `src/platform/juce/JuceDeviceHost.cpp:377-379` |
| Engine entry: `void Engine::process (AudioBlock&, const MidiEventBuffer& midiIn, MidiEventBuffer& midiOut, const AudioBlock& input) noexcept`; the 3-arg overload is called by it. | `src/engine/include/arranger/engine/Engine.h:295,298`; `src/engine/src/Engine.cpp:774,811` |
| Sample rate is taken from the device, pushed to the engine, and stored for status only. | `JuceDeviceHost.cpp:365-368`; `Engine.cpp:163-166,194` |
| `Transport` owns the musical clock: `std::int64_t sampleTime`, `anchorSample`, `anchorTick`, `anchorSeconds`. | `src/engine/include/arranger/engine/Transport.h:53-56` |
| `sampleTime` is **samples since play started**, not song time: `play()` sets `sampleTime = 0` and re-anchors. | `src/engine/src/Transport.cpp:55-62` |
| `Transport::advance(n)` only adds when playing; time never advances while stopped. | `Transport.cpp:111-115` |
| Block size: whatever `numSamples` the device gives. `prepare(rate, maxBlockSize)` allocates for the maximum and the engine degrades if a block is larger. | `Engine.cpp:161-195`; racks disabled for oversized blocks at `Engine.cpp:851-873`; monitor skipped at `Engine.cpp:800-801` |
| No fixed internal block size, no sub-block division except loop-wrap splitting (`guard < 64` spans). | `Engine.cpp:888-952` |
| Sample-rate change re-anchors the transport so the musical position survives, and re-prepares processors. | `Transport.cpp:34-41`; `Engine.cpp:161-193` |
| `Engine::blockRate` is set and never read (dead field). | `src/engine/include/arranger/engine/Engine.h:399`; `Engine.cpp:163` |
| Output latency is measured as `(outputLatencyInSamples + bufferSize) / rate`, i.e. the whole block is assumed to reach the speaker at block end. | `JuceDeviceHost.cpp:367` |

The sample clock is therefore *the device callback count*, and the engine keeps one `int64` counter plus a floating anchor. There is no PLL, no drift correction against a wall clock, and no resampling of the output.

---

## 2. Tick <-> sample conversion as implemented

Constants: `using Tick = std::int64_t`, `kTicksPerQuarter = 960` (`src/core/include/arranger/core/Time.h:10,13`), `secondsPerTick(bpm) = 60/(bpm*960)` (`Time.h:53`).

Forward (tick -> sample), `ProcessContext::sampleForTick` (`Transport.cpp:8-16`):

```cpp
const double samples = (tempo->secondsAt (tick) - anchorSeconds) * sampleRate;
return anchorSample + static_cast<std::int64_t> (std::ceil (samples - 1.0e-6));
```

- Anchored, absolute: computed from the song start through the tempo map, never summed block by block. This is what "no drift" means here (`Transport.h:8-15`, `Processor.h:35-40`).
- Rounding is **ceiling**: an event is never placed early, it is placed at the first sample at or after its ideal time (max +1 sample late).
- The `1.0e-6` sample epsilon swallows float noise so a mathematically exact sample stays put (`Transport.cpp:11-13`).
- Uses `double` seconds; the subtraction `secondsAt(tick) - anchorSeconds` loses relative precision as the anchor moves away from song start, but the absolute error at 1 h is ~1e-7 samples, i.e. irrelevant.

Reverse (sample -> tick), `ProcessContext::tickAtSample` (`Transport.cpp:18-23`): `tempo->tickAtSeconds (anchorSeconds + (sample - anchorSample) / sampleRate)`; returns a **double tick**. `Transport::positionTicks()` is the same expression (`Transport.cpp:43-48`).

Tempo map (`src/core/src/TempoMap.cpp`):
- `startSeconds[i]` is a cumulative double sum over tempo points, built once in `rebuild()` (`TempoMap.cpp:51-57`); points are constant-tempo segments, no ramps.
- `secondsAt` = `startSeconds[i] + (tick - point.at) * secondsPerTick` (`TempoMap.cpp:79-83`).
- `tickAtSeconds` binary-searches `startSeconds` then divides (`TempoMap.cpp:85-97`).
- Only accumulation that exists: the per-point seconds cache (bounded by the point count) and the metronome oscillator phase (`src/engine/src/Metronome.cpp:57`), which affects timbre, not timing.

Where precision/placement is actually lost:
1. **Ceiling to whole samples** — every event is quantised to a sample, so max error +0.999 sample (`Transport.cpp:15`).
2. **Block-end clamping** in `Engine::offsetFor`: `std::clamp<std::int64_t>(s, 0, ctx.numSamples - 1)`, then `+ blockOffset`. An event whose ideal sample is at or past the span end is moved *back* to the last sample of the span (`Engine.cpp:354-358`).
3. **Recording timestamps** are double ticks from `tickAtSample`, then rounded with `std::llround` only when the take becomes a clip (`Engine.cpp:649`; `src/app/Session.cpp:999-1011`).
4. **SMF import** resamples to 960 PPQ with integer math and half-up rounding: `(t*960 + division/2) / division` (`src/core/src/MidiFile.cpp:173-175`).
5. `ctx.ticksPerSample` is computed but never read anywhere (`Transport.cpp:107`; grep: only the declaration and this assignment).

---

## 3. Event scheduling

Collection: `Engine::sequence(ctx)` walks snapshot tracks in order and, per track, `playRange` binary-searches the pre-sorted event vector with `std::lower_bound` over `[from, to)` (`Engine.cpp:546-582,398-432,410`). Slot playback re-enters the same list per loop cycle (`Engine.cpp:415-431`, `guard < 64` cycles per span).

Look-ahead: **zero**. A span is scheduled only when it is rendered; `tickAtStart/tickAtEnd` bound the window (`Engine.cpp:891-934`). The only "ahead of time" work is the output thread's latency delay in the host (§7).

Ordering keys:
- `SequenceEvent::order`: `0` note-off, `1` controls/setup, `2` note-on (`src/engine/include/arranger/engine/Playback.h:26`).
- Build-time sort: `(at, order)` with `std::stable_sort` (`Playback.cpp:253,267-268,332`).
- Same tick, same track, same order: insertion order survives (clips iterated in `t.clips` order, then cycle order).
- Across tracks: `sequence()` iterates snapshot order (`Engine.cpp:551-580`), so track index breaks ties.
- Final block sort: `events.sortByTime()`, a **stable insertion sort by `sampleOffset` only** (`src/engine/include/arranger/engine/MidiEventBuffer.h:73-88`; call at `Engine.cpp:956`). Therefore ordering at equal sample offsets is: track index, then `(at, order)`.

Tie-breaking, simultaneous note-off vs note-on:
- Same pitch, same tick: the note-off is emitted first because `order` 0 sorts before 2.
- Retrigger of a pitch already held: an extra note-off is emitted immediately before the new note-on at the same offset (`Engine.cpp:366-373`).
- A note-off for a pitch this track does not hold is **dropped** (`Engine.cpp:374-379`), so overlapping duplicate notes cannot cut each other.
- Chords: no special handling; pitch order in the clip is preserved.

Coordinates:
- Each track keeps `ActiveNotes` (bitset 128 + pedal) and releases only what it started (`Engine.h:334-342`; `Engine.cpp:453-463`).
- Loop wrap splits the block, releases everything, re-locates and shifts slot phases (`Engine.cpp:896-951`).

---

## 4. Real-time safety on the callback path

Engine side, `Engine::process` and callees:

| Hazard | Location | Notes |
|---|---|---|
| `Handoff::acquire()` is a `for (;;)` spin of two atomic loads/stores | `src/engine/include/arranger/engine/Handoff.h:48-62` | Not a lock, but an unbounded loop. Livelock needs two publishes between two loads (U: not observed). |
| `while (auto c = commands.pop()) apply(*c);` | `Engine.cpp:875-876` | Unbounded count: up to 1024 commands, each with O(state) work. |
| `apply(panic)`: 16x128 loop over `thruHeld`, plus 3 controller passes | `Engine.cpp:230-242` | Bounded by constants, but thousands of iterations in one block. |
| `sendToTrackChannels`: for each track, scans all earlier tracks (O(n^2)) | `Engine.cpp:465-479` | Called on stop/panic. 1000-track song = ~500k comparisons inside the callback; a 1000-track test exists (`tests/EngineStressTests.cpp:218-228`). |
| `playRange` slot loop `guard < 64`, outer span loop `guard < 64` | `Engine.cpp:423,891` | Bounded, but 64 loop cycles of the whole event scan; on exhaustion the remaining span is silently not rendered. |
| `MidiEventBuffer::add` on overflow drops and counts | `MidiEventBuffer.h:60-69` | No allocation (capacity reserved in ctor). `droppedCount()` is never read in production code. |
| `events` 8192 / `instrumentIn` 4096 / `samplerIn` 4096 / `scratch` 256 | `Engine.h:387-388` | Fixed. Overflow = silent note loss. |
| `std::fill_n`, `std::copy_n`, `std::sort` avoided; insertion sort used deliberately | `Engine.cpp:860-873,805-806`; `MidiEventBuffer.h:76-77` | Good: comment says `std::stable_sort` may allocate. |
| Metronome `std::sin`/`std::exp` per sample | `Metronome.cpp:47-59` | No allocation, but transcendental math on the audio thread. |
| Meters use relaxed atomics; no locks | `Engine.cpp:1144-1147,1169-1170` | UI reads with `exchange` (`Engine.cpp:157`). |
| No mutex, `DBG`, `printf`, `new`, `malloc`, `std::string` in `src/engine/src` | grep over `src/engine/src/*.cpp` | Only `buildSnapshot`/`prepare` use vectors, both off the callback. |
| `Engine::prepare` calls `instruments.current()` which takes the Handoff mutex | `Engine.cpp:168-190`; `Handoff.h:41-45` | Documented "not real-time safe" (`Engine.h:290`); called from `audioDeviceAboutToStart`. |

Host side, callback `JuceDeviceHost.cpp:377-423`:

| Hazard | Location | Notes |
|---|---|---|
| `juce::Time::getMillisecondCounterHiRes()` twice per block | `377,381,390` | System clock read; not a lock, but a syscall-ish call on the RT thread (U: platform cost). |
| `liveLock.tryEnter()` | `386` | Try-only spin lock: on contention the MIDI batch is deferred to the next block instead of blocking (events stay in `live`). Adds up to one block of input latency. |
| `incomingLock` `std::mutex` | `299,339` | **Not** taken on the audio thread; only MIDI/UI threads. |
| `midiIn.add` + `midiIn.sortByTime()` (insertion sort over <=1024) | `392,396` | Bounded. |
| `outgoing.push(m)` return value ignored | `421` | Full queue (8192) = silent hardware MIDI loss, including note-offs. |
| `speechCapture.push` | `407` | Bounded ring, no allocation. |

Allocation test coverage is narrower than the claim: `tests/RealtimeTests.cpp:82` calls the **3-arg** `e.process(block, in, out)`, so the monitor/audio-capture preamble (`Engine.cpp:774-830`) and `speechCapture` are not covered by the operator-new counter. The test is also excluded from sanitizer builds (`tests/CMakeLists.txt:94-95`).

---

## 5. Threads and handoff

| Thread | Created at | Role |
|---|---|---|
| JUCE audio device thread | `manager.addAudioCallback` (`JuceDeviceHost.cpp:148`) | Runs the callback, `Engine::process`, `Transport`. |
| "MIDI out" `Sender` (juce::Thread, high priority) | `JuceDeviceHost.cpp:12,149` | Pops timed hardware MIDI, sorts, sends, sleeps 1 ms. |
| JUCE MIDI input threads | one per enabled device (`JuceDeviceHost.cpp:147`) | `handleIncomingMidiMessage`, timestamps and queues input. |
| UI/message thread | `Session::startTimerHz(30)` (`src/app/Session.cpp:21`) | Publishes snapshots, `collectGarbage`, `pollInput`, input policy. |
| Plugin/SFX/AI/whisper threads | `src/app/*` | Outside the RT path. |

Queues and handoffs:

| Link | Type | Capacity | Overflow |
|---|---|---|---|
| UI -> audio commands | `SpscQueue<Command,1024>` (`Engine.h:367`) | 1023 usable | `push` result ignored (`Engine.cpp:29-32`); comment says the UI re-sends state when a device starts. |
| Audio -> UI recorded MIDI | `SpscQueue<RecordedEvent,16384>` (`Engine.h:368`) | 16383 | Ignored (`Engine.cpp:652`) -> silent take loss. |
| Audio -> UI slot changes | `SpscQueue<SlotChange,1024>` (`Engine.h:373`) | 1023 | Ignored (`Engine.cpp:575`). |
| UI -> audio snapshot / instrument / racks / samplers | `Handoff<T>` (`Engine.h:369-372`) | 1 current + 1 previous | Whole-object publish; old objects freed by `collectGarbage` under a UI mutex (`Handoff.h:25-38,74-81`). |
| Engine -> host MIDI | `SpscQueue<TimedMessage,8192>` (`JuceDeviceHost.h:106`) | 8191 | Ignored (`JuceDeviceHost.cpp:421`). |
| MIDI thread -> audio | `std::vector<LiveInput>` + SpinLock (`JuceDeviceHost.h:92-93`) | 1024 | Beyond 1024 dropped (`JuceDeviceHost.cpp:331`). |
| Audio input -> UI take | `AudioCapture` ring (`AudioCapture.h:21`) | 2^18 frames (~5.5 s @48k) | Newest dropped + counted (`AudioCapture.h:39-40`). |

Edits land at block boundaries, exactly:
- snapshot swap: `Engine.cpp:836-838`, **before** `audio.clear()` (`878`) and before any sequencing;
- queued commands: `Engine.cpp:875-876`, same place;
- note policy for held notes on edit: `switchSnapshot` carries held notes for tracks whose `contentKey` is unchanged (up to 64 tracks, `Engine.cpp:488-544`); `contentKey` hashes events, not pointers (`Playback.cpp:335-347`).
- MIDI events produced by a snapshot swap or a command use offset 0 of that block (`Engine.cpp:837` passes offset 0; `Engine.cpp:517` emits at it; commands emit at offset 0, `Engine.cpp:221-263`).

---

## 6. MIDI input

- Timestamp source: `juce::Time::getMillisecondCounterHiRes()` at message arrival, in milliseconds as a `double` (`JuceDeviceHost.cpp:324`; struct `LiveInput::timeMs` `JuceDeviceHost.h:66`).
- Mapping to sample offset, in the *next* callback: `ageSamples = (nowMs - in.timeMs) * sampleRate / 1000`, `offset = clamp(numSamples - 1 - int(ageSamples), 0, numSamples - 1)` (`JuceDeviceHost.cpp:388-391`). All pending input is then cleared and sorted (`394-396`).
- Consequences: the block is assumed to span `[now - blockDuration, now]`; anything older than one block piles up at offset 0; anything with a future timestamp (negative age) is clamped to `numSamples - 1`. Current `sampleRate` is used, not the rate in force when the message arrived.
- Jitter: none filtered. No median/least-squares fit, no jitter buffer; the only smoothing is the clamp and the per-block sort. Input latency and keyboard scan time are not subtracted. Only output latency is compensated (`JuceDeviceHost.cpp:416-417`).
- Filtering, in the MIDI thread before the audio queue:
  - SysEx (`m.isSysEx()`) is diverted to the UI (`incomingSysEx`, cap 1024) and never reaches the engine (`JuceDeviceHost.cpp:293-303`).
  - `rawDataSize > 3`, active sensing and MIDI clock are dropped (`304-305`).
  - Control-surface filters: port-name substring + optional channel (`310-315`; built in `Session::refreshControlFilters`, `src/app/Session.cpp:1595-1609`).
  - Play filter: port substring + channel; only the record-target input reaches the engine (`JuceDeviceHost.cpp:316-320`; set from `src/app/Session.cpp:747-749`).
  - The UI copy is pushed for every message (cap 4096) even when filtered (`JuceDeviceHost.cpp:335-341`).
- Engine-side: input arrives as `MidiEvent` with port 0 (SoundFont) at its computed offset (`392`). `Engine::playThru` re-maps status/port to the thru target, remembering where each held key and each pedal went (`Engine.cpp:584-638`), and `recordInput` stores double ticks (`640-654`).
- Port open/reconnect: on first run every available input is enabled (`JuceDeviceHost.cpp:142-145`); the `AudioDeviceManager` state is restored from settings on later runs (`131-140`). `MidiInputPolicy` re-enables instruments that appear, every 2 s (`src/core/src/MidiInputPolicy.cpp:20-49`; `Session.cpp:1999-2003`), and remembers user-disabled names (`off` set) across restarts (`MidiInputPolicy.h:45-49`). Software ports (`IAC`, `loopMIDI`, network sessions, ...) stay off unless a profile names them (`MidiInputPolicy.cpp:10-18`). A port that disappears is forgotten (`MidiInputPolicy.cpp:47`). (U: whether JUCE's state XML persists per-input enable flags — the design comment at `MidiInputPolicy.h:16-19` assumes it does.)
- `RecorderImport.cpp` is **not** MIDI: it parses Zoom L6max audio file names (`src/core/src/RecorderImport.cpp:10-57`). MIDI take -> clip pairing/rounding lives in `src/app/Session.cpp:982-1012`.

---

## 7. MIDI output

Path: `Engine::process` -> per-block `events`, split by port (`Engine.cpp:954-969`):
- port 0 -> `instrumentIn` (built-in SoundFont player);
- `kSamplerPortBase + n` -> that sampler / plugin bus (`Engine.cpp:982-994,1045-1053`);
- `kNoPort` -> dropped (metronome, MIDI-copy clicks);
- **port 1..N -> `midiOut`**, with `sampleOffset += hardwareDelay` where `hardwareDelay = currentRacks->totalLatency()` (`Engine.cpp:954,964-967`; `RackSet::totalLatency` `Engine.h:115`).

Host -> hardware:
- Each event becomes `dueMs = max(lastDueMs, nowMs + latencyMs + 1000*offset/sampleRate)`; the `max` is a monotonicity guard against callbacks arriving at uneven times (`JuceDeviceHost.cpp:409-421`).
- Sender thread pops, `std::stable_sort` by `dueMs`, sends everything due by `now + 0.5 ms` via `MidiOutput::sendMessageNow`, erases sent, sleeps 1 ms (`JuceDeviceHost.cpp:70-100`).
- Resolution is therefore ~1 ms plus OS MIDI driver timing, on top of the device-latency estimate. Physical port index = `port - 1` (`91-93`).
- SysEx: the engine has no SysEx path at all (3-byte `MidiEvent`, `MidiEvent.h:11-24`). The host has `sendNow(port, bytes)` for arbitrary bytes including SysEx, executed immediately under `portsLock` on the calling (UI) thread (`JuceDeviceHost.cpp:54-68`), plus optional SysEx at tick 0 on SMF export (`MidiFile.cpp:494-503`).
- Ports are opened/closed on `setOutputPorts` (`JuceDeviceHost.cpp:28-52`), called from `Session::publishSong` on every snapshot publish (`Session.cpp:178`). Same-name ports are reused; a port that leaves the list is panicked and closed. (U: no retry if a device is unplugged and replugged without a new snapshot.)
- Stuck notes / cleanup:
  - Stop: `releaseAllNotes` (per-track note-offs + pedal-up) then CC 123 to every used port/channel (`Engine.cpp:442-451`).
  - Panic: stop, release, CC 64/120/123 to every used port/channel, then note-offs for every held thru key and pedal-up to their remembered destinations (`Engine.cpp:223-243`).
  - Host panic: `Sender::allNotesOff` sets a panic flag, clears pending messages, and sends 16 channels x {CC64, CC120, CC123} on every open output (`JuceDeviceHost.cpp:19-26,103-107`; `MidiPanic.h:42-54`). Called from `audioDeviceStopped` (`371-375`) and on port removal.
  - `PanicQueue::discardBeforeSending` drops queued messages before the next send so a queued note cannot pass the panic (`src/platform/include/arranger/platform/MidiPanic.h:20-28`; used at `JuceDeviceHost.cpp:78-79`).
  - Test: every started note is eventually ended (`tests/EngineStressTests.cpp:134-180`).

---

## 8. Standard MIDI file

Reader (`src/core/src/MidiFile.cpp:143-373`):
- Format 0 and 1; format 2 and SMPTE division are rejected with `Error` (`168-171`); RIFF/RMI wrapper is skipped (`148-155`).
- Division: any ticks-per-quarter; rescaled to 960 with integer half-up rounding (`173-175`).
- Running status supported (`217-223`); SysEx is skipped and lost (`259-263`); unknown chunks skipped (`186-196`).
- Meta: track name `FF 03`, lyrics `FF 05`, tempo `FF 51` (multiple points), time signature `FF 58` (**first one only**), key signature `FF 59` (first only). Markers/cue/text (`FF 06/07/01`) are ignored. End of track `FF 2F`.
- Channel events: note on/off (velocity 0 = off), CC, program, channel pressure, pitch bend; poly pressure ignored (`278-311`). Notes are paired **FIFO per pitch** (`std::deque`, `288-293`), so overlapping same-pitch notes pair oldest-first, not LIFO.
- Setup detection: CC 0/32/7/10/91/93 and program within `960/8 = 120` ticks of 0 and with no notes yet become track sound/mixer values; otherwise they stay as controls (`93,276,295-307`).
- Notes still open at the end of a track are closed at the global `songEnd` (`314-322`). Each (file track, channel) becomes one song track with one clip starting at 0, `length` = end rounded **up to a bar** (`340-341,363-369`). Lyric syllables are reconstructed with hyphen/space heuristics; karaoke `/ \ CR LF` are dropped (`102-140`).
- Title: first track name when there are no channels yet or format 0, else the file stem (`330-331,382-383`).

Writer (`MidiFile.cpp:486-641`):
- Format 1 by default (format 0 with `singleTrack`), division **960 PPQ** (`635-637`).
- Track 0 (conductor): title, optional start SysEx at tick 0, one time signature at tick 0 (`508`, content bytes 24/8 fixed), all tempo points (`510-514`), optional XF chord events (`515-526`, `xfChord` at `449-477`).
- Per song track: name (unless single-track), bank/program/volume/pan/reverb/chorus at `options.setupAt`, lyrics (`548-551`), notes flattened from clips including loop cycles, controller/program/pressure/pitch-bend events, optional automation lanes as CC 7/10/91/93 (`555-620`).
- Event order on write: `0` setup, `1` note-offs, `2` lyrics/controls, `3` note-ons (`410-419,536-546,567-568`).
- Lossy / not representable:
  - tempo is rounded to integer microseconds per quarter: `llround(60e6/bpm)` (`512`), so a round trip shifts BPM by <0.01 (`tests/MidiFileTests.cpp:53`);
  - one global time signature, at tick 0 only — later meter changes are never read or written;
  - key signature is read but never written;
  - no markers (only lyrics and XF chords), no SMPTE, no poly pressure, no SysEx import, no note-off velocity;
  - notes are trimmed at clip end / loop cycle end (`563-566`), and `startSysEx` only exists at tick 0;
  - `options.offset` shifts music, lyrics and tempo points but *not* the initial tick-0 tempo/time signature (`513`), and setup stays at `setupAt`;
  - channels are clamped to 1..16; overlapping notes of one pitch on one channel are not separated.

---

## 9. Timing tests that exist today

All render through `test::Renderer` (`tests/TestRender.h:26-101`), which calls the 3-arg `engine.process`, records absolute sample = `rendered + sampleOffset`, and can feed input per block (`TestRender.h:57-88`).

| Test | File:line | Asserts | Rate / block |
|---|---|---|---|
| Every note goes out on the sample the tempo map gives, through 30 tempo changes | `tests/TimingTests.cpp:48-76` | per note `abs(got - expected) > 1` counts as off, `CHECK(off == 0)` (`72-74`); 30 random tempo points, 300 notes | 48 kHz, blocks random 1..1024 |
| A loop repeats on the same samples, pass after pass | `TimingTests.cpp:78-108` | every event exactly 3 note-ons later repeats, `abs(delta - loopSamples) <= 1` (`106`) | 48 kHz, random 1..1024 |
| Clip grid launches start on the next bar, in time with the metronome | `TimingTests.cpp:110-143` | exact samples `96000`, `120000`, `192000` (`133-136`); click on the bar sample `96000` | 48 kHz, 512 |
| Timing does not drift over a long song at an odd sample rate | `TimingTests.cpp:145-174` | 600 beats @97.3 BPM, `abs(on - round(secondsAt(i)*rate)) <= 1` (`171-172`) | 44.1 kHz, 1024 |
| Clicks land on exact beat samples | `tests/MetronomeTests.cpp:37-46` | exact `k*24000` | 48 kHz, 512 |
| No drift over an hour, at an awkward tempo and sample rate | `MetronomeTests.cpp:48-66` | `CHECK(worst == 0)` over 1 h (`65`), for 112.37/97/133.33 BPM x 44.1/48/96 kHz, random blocks |
| Output does not depend on the audio buffer size | `MetronomeTests.cpp:68-82` | identical click samples and bit-identical audio (`81`) | 44.1 kHz, 64 vs random |
| The click sound starts on the beat sample | `MetronomeTests.cpp:84-94` | `audio[23999] == 0`, `audio[24000] != 0` |
| Downbeats accented, following the meter | `MetronomeTests.cpp:96-118` | 3/4 and 6/8 accent pattern, exact samples |
| A tempo change takes effect from the next beat | `MetronomeTests.cpp:120-134` | exact samples 0, 24000, 60000, then +48000 |
| Notes go to their hardware port on exact samples | `tests/SequencerTests.cpp:39-62` | `ons[i] == i*24000`; note-offs at `% 24000 == 12000` |
| Setup messages sent when play starts | `SequencerTests.cpp:64-80` | order bank MSB, bank LSB, program; 2-byte program |
| Loop plays the same bar again, sample-exact | `SequencerTests.cpp:82-94` | 12 note-ons at exact multiples of 24000 |
| A loop that ends on a block boundary wraps | `SequencerTests.cpp:96-128` | exact samples for 1000- and 512-sample blocks |
| Tempo changes inside the song are followed | `SequencerTests.cpp:130-142` | exact 48000 / 96000 |
| Stop ends held notes | `SequencerTests.cpp:174-186` | first note-off is pitch 60 at sample 6000 |
| Editing during playback keeps unchanged tracks' notes | `SequencerTests.cpp:188-208` | no note-off for a mixer-only change; note-off on content change |
| Recording captures input at its song position | `SequencerTests.cpp:210-228` | `rec->tick == 960 + 240*960/24000` exactly |
| Thru goes to the chosen port and channel | `SequencerTests.cpp:230-245` | port 3, status `0x94`, sample `512+10` |
| Panic silences every used channel and live keys | `SequencerTests.cpp:401-424` | CC 64/120/123 and note-offs present; not playing |
| Play, stop, resume keep the song position | `tests/EngineTests.cpp:10-34` | `WithinAbs(1920.0, 1e-6)`, `WithinAbs(2880.0, 1e-6)` |
| Every note the engine starts, it also ends | `tests/EngineStressTests.cpp:134-180` | per (port, channel, pitch) outstanding count 0 after stop |
| Engine never allocates on the audio thread | `tests/RealtimeTests.cpp:88-118` | `audioAllocations == 0` over 2000 + 300x20 blocks with transport/song churn |
| Buffer-size independence of the mix | `tests/RenderTests.cpp:201-224` | `maxDifference < 1e-6` (sample-exact) for 64 vs 512 vs random |
| SoundFont notes start within 64 samples | `RenderTests.cpp:226-249` | onsets `<= 64` samples; RMS `<= 0.002 + 0.05*x` |
| The mix matches the last good render | `RenderTests.cpp:251-279` | RMS `<= 0.002 + 0.02*goodRms`, peak `<= 0.003 + 0.03*goodPeak`, onsets `<= 2` samples |
| MIDI file round trips / format 0 / lyrics / errors | `tests/MidiFileTests.cpp:44-158` | tempo `WithinAbs(140.0, 0.01)`, structure, 96 PPQ rescale to 960, error strings |
| MIDI input policy | `tests/MidiInputPolicyTests.cpp:50-207` | which ports are auto-enabled/remembered; no timing |

Golden files (`tests/golden/`, 7 files):
- `render-reference-song.json` (1.2 KB): `{firstOnsets:[sample...], windows:[{rms,peak}...]}`, one window per 0.25 s (48 kHz), values rounded to 4 decimals, up to 8 onsets defined as a loud sample after >=10 ms of silence (`RenderTests.cpp:168-198`). Regenerate with `ARRANGER_UPDATE_GOLDEN=1` (`256-262`).
- `score-*.json` (six files, `score-long.json` 52 KB): `{systems:int, pages:[{size:[w,h], glyphs:[[code,x,y,scale]...], lines:[[x1,y1,x2,y2,width]...], shapes:[[x,y,...]...], texts:[[text,x,y]...]}]}`, all numbers rounded to 2 decimals, lists sorted, compared with tolerance `0.05` (`tests/ScoreSnapshotTests.cpp:24-68,80-133`). These are *notation layout* snapshots, not timing.
- Neither golden format stores a waveform or MIDI bytes; they store perceptual/geometric summaries, which is why they survive compiler differences.

---

## 10. Rust design implications

**Must reproduce**
1. Anchored tick->sample conversion with ceiling rounding and a ~1e-6 sample epsilon; no per-block accumulation (`Transport.cpp:8-16`).
2. `sampleTime` = samples since play; musical position derived from `(anchorTick, anchorSample, seconds)`; re-anchor on play/stop/locate/rate-change/tempo-map change (`Transport.cpp:27-41,55-83`).
3. Sort key `(tick, order)` with `order` = note-off < controls < note-on, stable across equal keys; track index breaks cross-track ties (`Playback.cpp:253`; `Engine.cpp:551-580,956`).
4. Per-track active-note bookkeeping: retrigger emits note-off first; unknown note-offs are dropped; pedal state tracked per track (`Engine.cpp:360-383,453-463`).
5. Zero look-ahead; a block's events are computed from `[tickAtStart, tickAtEnd)` only; hardware output is delayed by measured device latency instead (`Engine.cpp:891-934`; `JuceDeviceHost.cpp:409-421`).
6. Loop wrap splits the block at the loop-end sample, releases all notes, re-locates, and shifts slot phases by the loop length (`Engine.cpp:896-951`).
7. Edit handoff: whole immutable snapshot plus a `contentKey`; unchanged tracks keep sounding notes (`Engine.cpp:488-544`).
8. 960 PPQ internal and on export; import rescales any division with integer half-up rounding (`Time.h:13`; `MidiFile.cpp:173-175,637`).

**Must avoid**
1. Any `std::vector` growth, `std::function` call, `std::string`, mutex, `DBG`/log, or filesystem call in the callback — the C++ engine achieves this and still has the hazards in §4; in Rust the equivalents are `Vec::push` past capacity, `Arc::make_mut`, `Mutex`, `println!`, and `std::time` syscalls.
2. `acquire()`-style unbounded spin loops; prefer a seqlock or a single atomic swap with epoch reclamation.
3. O(n^2) per-block scans (`sendToTrackChannels`) and per-block linear lookups (`laneFor`, `routeFor`, `busFor`, `samplerPort` are all linear scans: `Playback.cpp:65-87`).
4. Silently ignoring queue-push failure (commands, recorded, slot changes, hardware out). Prefer counting and exposing drops.
5. Unbounded command drains inside the callback; cap work per block and defer the rest.
6. Background-thread `MidiOutput` open/close triggered from a snapshot publish (`JuceDeviceHost.cpp:28-52` called from `Session.cpp:178`).

**Must decide**
1. Input timestamping: keep the "arrival time mapped into the current block, clamped" rule (`JuceDeviceHost.cpp:388-391`), or adopt device timestamps / a jitter buffer. Current behaviour is jitter-unfiltered and uses the current sample rate.
2. Whether recorded ticks are doubles rounded at commit (`Session.cpp:999-1011`) or quantised at capture.
3. Panic scope: engine-level (per used port/channel, `Engine.cpp:223-243`) plus host-level 48 CCs per port (`MidiPanic.h:42-54`) — both, or one.
4. SysEx: not on the engine path today; decide between a separate raw-bytes queue with its own timing or an out-of-band immediate send.
5. Buffer-size policy: today the engine degrades (racks off, monitor off) when a block exceeds `maxBlockSize` (`Engine.cpp:851-873`). Decide between hard failure, internal chunking, or the same degradation.
6. SMF fidelity: single time signature, no markers, tempo rounded to integer microseconds. Decide whether the Rust writer keeps these limits or extends the format.
7. Whether to fix the one-tick-note / same-sample collision cases explicitly (current tie-break is emission order, which is track order).

**Unverified / limits of this audit**
- (U) JUCE state XML persisting MIDI input enable flags; (U) resolution and cost of `getMillisecondCounterHiRes`; (U) `MidiOutput::sendMessageNow` behaviour for an unplugged device; (U) whether the `Handoff` spin can livelock in practice.
- The JUCE-free claim is enforced by a token grep, not by link analysis: `scripts/check-no-juce.sh:7` fails on `#include juce`, `juce::` or `JUCE_` under `src/core` and `src/engine`, run in CI at `.github/workflows/ci.yml:56,115`.
- `tests/RealtimeTests.cpp` covers only the 3-arg `process`; the monitor/capture path is untested for allocation.
- No profiling data was read; all RT-cost statements are structural, not measured.
