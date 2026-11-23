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
