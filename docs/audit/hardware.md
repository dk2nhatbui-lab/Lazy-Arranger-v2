# Hardware and plugin environment check (2026-11-23)

Reported by the user as connected. Commands were run on the local Mac; results
below are what the tools actually returned.

## MIDI ports

`lazy-arranger ports` found:

- inputs: Launchkey Mini MK3 MIDI Port, Launchkey Mini MK3 DAW Port, XioSynth,
  Yamaha PSR-S970 Port 1, X-TOUCH MINI
- outputs: Launchkey Mini MK3 MIDI Port, Launchkey Mini MK3 DAW Port, XioSynth,
  Yamaha PSR-S970 Port 1, Yamaha PSR-S970 Port 2, X-TOUCH MINI

## Send checks

```text
midi-channel-check --output "Launchkey Mini MK3 MIDI Port" --note 60 --seconds 0.2
sent [144, 60, 100] then [128, 60, 0]
midi-panic --output "Launchkey Mini MK3 MIDI Port"
sent 16 all-notes-off message(s)
```

The same channel check and panic succeeded on `Yamaha PSR-S970 Port 1`. Audibility
was not independently measured; no note was deliberately sustained.

A one-second `record` from the Launchkey opened the input and wrote a 78-byte,
one-track MIDI file with **0 notes** because no key was pressed during the test.
Live key recording therefore remains unverified.

## Printers

`lpstat -p` lists Brother_DCP_L3550CDW_series, Canon_SELPHY_CP1300 and
DYMO_LabelManager_280, all idle. No print job was sent during this check.

## Installed plugins

Audio Units under `/Library/Audio/Plug-Ins/Components` include LABS, Lounge
Lizard Session 4, R0Verb, Strum Session 2, Ultra Analog Session 2, ZENOLOGY,
sforzando and Band-in-a-Box. VST3 bundles include LABS, R0Verb, sforzando,
Band-in-a-Box and Applied Acoustics/Roland folders. `plugin-scan` lists these
bundles. No plugin was instantiated, processed or opened; hosting/editor support
does not exist yet.

## Not verified

- audible PSR-S970 output or voice/bank response
- physical key capture from the Launchkey or PSR
- XioSynth and X-TOUCH MINI messages
- any print job
- AU/VST3 instantiation, processing or editor windows

## Final-round checklist

The instruments can stay powered off until the final round. Run
`./scripts/hardware-smoke.sh` first; it reads ports, plugins and printers without
sending MIDI or printing.

When the instruments are powered on, verify in this order:

1. **MIDI input**: open Windows > MIDI monitor; a key press, pitch bend and one
   knob should appear. Check the held-chord label and Kids key audition.
2. **Launchkey mixer**: Controllers > Load Launchkey mixer preset; CC 21-28
   should move track faders 1-8.
3. **PSR-S970 remote**: select the PSR output; send voice, volume and one chord
   audition; listen to the keyboard.
4. **PSR song export**: export a song for the PSR and play it from USB.
5. **XioSynth/D-05 editor**: send one CC control and one `.syx` dump; check the
   synth changed or accepted the dump.
6. **Audio output**: Settings > Find audio outputs; play; confirm the inspector
   shows the device, buffer and latency, and audio is audible.
7. **Audio input**: record a short take; repeat with CH1 and Mix; monitor with
   headphones at low gain.
8. **Print**: print one score page to a real printer.
9. **Plug-ins**: instantiate R0Verb on a track, capture its state, reopen the
   project and confirm the state returns. Native plugin editor windows are not
   implemented.
10. **L6max**: import a take folder; master/mix take muted; mixer scene A saves
    and recalls. Device EQ/MONO/USB/transport need a device project file that is
    not implemented.

Record each result in this file with date, device, command/UI step and the
observed outcome. Do not mark an item verified from a successful send alone.

## Final hardware round (2026-11-24)

The owner powered the instruments on. `ports` then listed inputs and outputs:
Launchkey Mini MK3 MIDI/DAW, XioSynth, D-05, PSR-S970 Port 1/2, X-TOUCH MINI
and three L6max ports. `lazy-arranger audio-devices` listed XioSynth, D-05, ZOOM L6max,
Camo Microphone and Microsoft Teams Audio inputs; outputs added DELL U4320Q and
Mac mini Speakers.

### What passed

- `midi-channel-check` sent note on/off on all five tested outputs: PSR-S970
  Port 1, X-TOUCH MINI, L6max Mixer Control Port, XioSynth and D-05.
- `midi-panic` sent 16 all-notes-off messages to each of those outputs.
- `record-audio --device XioSynth` and the default/Camo inputs opened and wrote
  WAV files; the input stream and file writer work.
- L6max Mixer Control accepted CC 93 = 127 on channel 1; the owner saw CH1 mute on the mixer, and CC 93 = 0 cleared it. The L6max CC map is hardware-verified for mute.
- cpal playback: `play-audio` rendered the demo and played 12 seconds on **Mac mini Speakers**; the owner confirmed the music was audible. Output is hardware-verified.
- Printing: a one-page score PDF was sent to `Brother_DCP_L3550CDW_series` (request 105) and the owner confirmed a page printed. Print is hardware-verified for that printer.
- `lpstat -p` reports the Brother, Canon and DYMO queues idle.
- `plugin-scan` still lists 21 bundles.

### What did not produce evidence

- The first two 30-second input recordings wrote 0 notes because the owner
  pressed keys after they had finished. A note-triggered recorder (`record
  --wait`) then captured physical input:
  - Launchkey Mini MK3 MIDI Port: **9 notes** in a 4-second window.
  - Yamaha PSR-S970 Port 1: **28 notes** in a 4-second window.
  - XioSynth: **8 notes** in a 4-second window (one held note was dropped at
    stop).
  - X-TOUCH MINI: **10 pads/keys** in a 4-second window.
  All four files imported cleanly. This verifies physical input from these four
  devices.
- A 60-second `midi-monitor` on X-TOUCH MINI captured **635 messages, all 635
  control changes** (CC 10 on channel 10) while the owner moved knobs and
  faders. Control-change capture is verified.
- A 60-second `midi-monitor` on the Launchkey captured **1,634 messages, 1,118
  control changes** including CC 21, 25 and 1. The mixer preset binds CC 21–28;
  the tested knobs are in that range, while CC 1 is outside it.
- A PSR echo test (send to the PSR output while recording its input) wrote 0
  notes.
- XioSynth input recordings during a channel check and a 35-second demo playback
  peaked around 0.0002, effectively silence. The app's cpal output is verified on
  Mac mini Speakers, but the XioSynth USB audio path remains unverified.
- D-05 input opened at 44.1 kHz and captured peaks around 0.031, but RMS before,
  during and after four channel-check notes showed no repeatable note envelope.
  This is not enough to call the D-05 audio path verified. The owner reports the
  D-05 has no keyboard attached, so its physical key input could not be tested.
- A 60-second XioSynth audio capture stopped at **0.54 s** and peaked around
  0.0004, so its USB audio path is inconclusive. This is outside the app's MIDI
  capture claim.
- L6max readback and transport are not verified; only CC sends and the CH1 mute response are. A 45-second monitor on Mixer Control while the owner moved CH1 level and mute captured **0 messages**, so the port appears receive-only.
- No plug-in editor window was opened.

These are transport/send and stream-open checks, not audible or physical
acceptance. The hardware rows remain partial.

### Reproduction commands

```sh
lazy-arranger ports
lazy-arranger audio-devices
lazy-arranger midi-channel-check --output "Yamaha PSR-S970 Port 1"
lazy-arranger record --in "Launchkey Mini MK3 MIDI Port" --wait --seconds 4 --out take.mid
lazy-arranger midi-panic --output "X-TOUCH MINI"
lazy-arranger audio-devices
lazy-arranger record-audio --device XioSynth --channel 0 --seconds 3 --out take.wav
```

The instruments were powered down after this round. The release bundle then showed `No MIDI inputs`, as expected.

## Roland D-05 & Zoom L6max live round (2026-11-24)

The owner powered on the Roland D-05 and Zoom LiveTrak L6max. `lazy-arranger ports` listed `D-05`, `L6max MIDI I/O Port`, `L6max Mixer Control Port`, and `L6max for L6 Editor Port` (plus Launchkey, XioSynth, X-TOUCH); `lazy-arranger audio-devices` listed `ZOOM L6max` and `D-05` inputs and outputs.

### What passed (observed on hardware)

- **Roland D-05 SysEx RQ1 temporary-patch read (`d05-read`):** decoded all 448 bytes (`00 00 00`..`00 03 3F`) from the live `D-05` port: patch `"Neo Horizon"`, upper tone `"Bass Fx"`, lower tone `"SweepPad"`, Key mode `Dual (1)`, split `24`, Reverb type `24` (User slot 24) / balance `65`, Volume `60`, Tone balance `50`. `d05-read --save` wrote a 468-byte `.syx` dump (two valid DT1 packets).
- **Roland D-05 bank name scan + DT1 restore (`d05-names`):** using Bank Select `CC 0 = 87`, `CC 32 = 0..13` + Program Change + `RQ1(00 03 00, 00 00 12)`:
  - Internal 1 (`--bank 0 --count 8`): `Fantasia`, `Metal Harp`, `Jazz Guitar Duo`, `Arco Strings`, `Horn Section`, `Living Calliope`, `D-49 Voices`, `Slow Rotor`.
  - Preset 2 (`--bank 9 --count 4`): `Neo Horizon`, `Aqua Bells`, `Lead with Joystk`, `D-49SynBrass`.
  - After scanning, chunked DT1 restore (`write_temp_patch`) restored `"Neo Horizon"` on the synth, confirmed by a subsequent `d05-read`.
- **Roland D-05 internal memory address check:** direct `RQ1` and `RQD` against `02 00 00` / `02 03 00` / `01 00 00` returned 0 packets on the D-05; bank scanning therefore uses Bank Select + Program Change + Temporary Patch `RQ1` with DT1 restore.
- **Zoom L6max Editor Port SysEx identity (`midi-identity --in "L6max for L6 Editor Port" --out "L6max for L6 Editor Port"`):** returned 15-byte Universal Identity Reply `F0 7E 00 06 02 52 72 00 0D 00 30 31 31 30 F7` (Zoom `0x52`, family `7200`, member `0D00`, firmware `"0110"`). Idle 2 s raw monitor on `L6max for L6 Editor Port` received 0 unsolicited packets.
- **Zoom L6max USB audio capture:** `record-audio --device "ZOOM L6max" --channel 0 --seconds 2` opened the stream and wrote a 2.01 s 48 kHz WAV file.
- **Zoom L6max transport send:** `l6max-transport --out "L6max MIDI I/O Port" stop` sent realtime `0xFC` cleanly.

### Still unverified

- Roland D-05 audible audio output and physical key input (no keyboard attached).
- Novation XioSynth dedicated sound editor page and knob-feedback sync.
- Zoom L6max proprietary project-file layout, mixer-state SysEx readback beyond Universal Identity, and visual confirmation of hardware recorder reaction to `0xFA`/`0xFB`/`0xFC`.
