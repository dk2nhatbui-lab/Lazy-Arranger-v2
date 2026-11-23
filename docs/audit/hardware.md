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
