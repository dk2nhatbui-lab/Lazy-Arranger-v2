# Cadenza / Vibez engine study (2026-11-23)

Read-only source study of `.reference/cadenza` and `.reference/vibez`. Both are
**GPL-3.0-or-later**; no code was copied. The patterns below are re-implemented
from their public architecture and from permissively licensed bindings.

## Plugin engine and manager

Cadenza (`plugin-host`, ~13.3k impl lines):

- one `PluginDescriptor` for CLAP and VST3: format, id, name, vendor, version,
  path, role, features, parameters, audio/note ports, latency, sidechains and
  auxiliary outputs;
- discovery per-OS, a JSON `PluginCatalog` with file stamps and a blocklist,
  scanned by a separate `cadenza-plugin-scanner` process;
- a shared plugin main thread for every VST3/CLAP component call, editors and
  timers; the audio thread only calls `IAudioProcessor::process`;
- optional sandbox process with shared-memory audio and JSON control, plus a
  crash/hang taxonomy (`late` vs. hung; preserve PDC through failures);
- plain-value parameters converted to VST3 normalized values on the audio
  thread; GUI edits come back through a wait-free queue.

Vibez (`vibez-plugin-host`, ~8.5k lines) uses the same shape: VST3/CLAP only,
metadata scanning in a throwaway `vibez-plugin-scan` subprocess, a serializable
plugin cache and a shared GUI/main-thread model.

**Implemented here so far (clean room):**

- `plugins::FoundPlugin` now carries vendor, version, identifier and
  instrument/effect role;
- AU metadata is read from `Info.plist` (`AudioComponents`: type, subtype,
  manufacturer, version);
- VST3 metadata is read from `Contents/Resources/moduleinfo.json` when present,
  with a bundle-name fallback otherwise;
- a JSON plugin catalog cache (`lazy-arranger-plugins.json`) and a
  richer My Plug-Ins browser (vendor, role, count);
- unit tests for AU, VST3 and catalog parsing.

**Isolated FFI now exists:** `crates/plugin-host` is the only crate with
`unsafe`, and it scans real VST3 modules through `IPluginFactory2`. It reports
class id, name, vendor, version and subcategories for installed R0Verb, LABS,
sforzando and Band-in-a-Box. `plugins` uses it when `moduleinfo.json` is absent.
The crate has no lints inheritance by design; all other crates still forbid
`unsafe`.

**Stereo effect processing now works** in `plugin-host`: component + optional
controller, a minimal `IHostApplication`, main-bus activation, `setupProcessing`,
`setActive`, `setProcessing` and a block `ProcessData` loop. Verified by an
ignored test that runs R0Verb over an impulse: output is non-silent. **Track effect slots are now applied**: an offline track render sends the track
buffer through each non-bypassed VST3 slot, before the bus/主 mix; an ignored
R0Verb integration test asserts the wet and dry renders differ. Remaining:
parameters/automation, state, offline/realtime modes, AU v2 (AudioToolbox C
API), instrument plugins, sandbox and editors; production scanning should move to
a subprocess, as both references do.

## MIDI engine

Cadenza `midi-io` (~2.9k lines):

- `midir` callback pushes timestamped raw messages into an SPSC queue; the
  realtime side drains into sample-accurate events with no allocation or locks;
- recording runs off the audio thread; SMF import/export includes MPE and
  preserves unmodelled events as warnings;
- running-status decoder kept separate from the parsed model.

Ours already has MIDI ports, live input, recording, maps and SMF/MusicXML. Gaps:
timestamped lock-free drain, MPE zones/import-export, and preserving
unmodelled MIDI instead of dropping it.

## Wave engine

Cadenza `audio-io` (~2k lines):

- symphonia decoding for WAV, AIFF, FLAC, MP3, OGG, M4A/AAC/ALAC and CAF;
- rubato sinc resampling, multi-resolution peaks, an LRU decoded-audio cache;
- realtime-safe clip DSP (fades, crossfades, gain, regions), WAV/FLAC export
  with TPDF dither, optional MP3; tags and sidecar metadata.

Ours handles WAV 16/24/32f, resampling, peaks, fades, stretch and normalize, but
not compressed formats, FLAC/AIFF, cache or dither. Symphonia is MPL-2.0
(file-level copyleft) and fits the permissive policy.

## Sampler engine

Vibez `vibez-instruments` (~5.4k lines):

- polyphonic sampler with eight voices, oldest-voice stealing;
- one sample mapped over key/velocity, forward/reverse, loop with crossfade,
  one-shot fade, trim;
- per-voice amp and filter envelopes, LFO, glide, SVF filter;
- 64-pad drum rack with per-pad sampler state and choke groups.

Ours has SF2 presets and a built-in synth but no loop/crossfade region model,
velocity layers, per-voice filter or streamed sample cache. A clean-room region
model (start/end/loop/crossfade/reverse/root/tune) is the first step, then
velocity zones and drum choke groups.

## Engine graph (not adopted yet)

Cadenza compiles the project to a flat graph off the audio thread, publishes it
through two `rtrb` rings, and returns the old graph on a second ring so the audio
thread never drops it. Our renderer is offline and re-renders on play; this is
fine for the current cache-based player but would need the compiled-graph/ring
pattern for live plugin and parameter changes.

## Suggested order

1. Isolated FFI `plugin-host`: VST3 class scan, descriptor (params/ports),
   instantiate, process an effect chain, then AU v2.
2. MIDI lock-free timestamped input + MPE import/export.
3. Symphonia decode + FLAC/AIFF export + dither/cache.
4. Sampler region/loop crossfade + velocity layers, then drum choke groups.
