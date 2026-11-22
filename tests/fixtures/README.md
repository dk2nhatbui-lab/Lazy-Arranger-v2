# Test fixtures

Every file here is copied from the owner's C++ repository
`arranger-midi-daw`, git commit `8ca41644a2be5f5677bce8cae060a55f7b19e8fc`.
None of it is covered by this repository's MIT/Apache-2.0 licence; it is
owner-supplied test data. Do not redistribute these files without an explicit
decision from the owner (see `docs/audit/licensing.md`).

| File | Source path | Size | What it proves |
| --- | --- | --- | --- |
| `legacy-cpp-v3.arranger` | `examples/demo.arranger` | 26,085 B | C++ schema 3 import: 6 tracks, 265 notes, tempo 112, time signature 4/4 |
| `legacy-cpp-v4.arranger` | `examples/Copper Lights/Copper Lights (Audio).arranger` | 9,603 B | C++ schema 4 import: 10 named tracks with no MIDI notes, tempo 126.000126, schema-4 keys preserved |
| `demo-style.sty` | `examples/demo-style.sty` | 3,940 B | Yamaha container scan: `MThd` + `MTrk` only, format 0, one track, 1920 PPQ, no CASM |

## What is still missing

* A real Yamaha style with `CASM` and `CSEG`, needed before any chord-rule work.
  The only style here has neither, so `style-formats` claims container parsing
  and nothing more.
* A C++ build with `ARRANGER_STYLE_SAMPLES` pointing at a style sample library,
  which the C++ `StyleSampleTests` need and which this repository does not have.
* Captured golden outputs (MIDI files and expected sample tables) for the
  scenarios listed in `docs/audit/compatibility-baseline.md`. Capturing them
  needs the C++ application running on the owner's machine (ADR-008).

## Rules for adding a fixture

1. Record the source repository, commit, source path and size in the table.
2. State what the fixture proves and which test consumes it.
3. State its licence status. If the licence is unknown, the fixture may be used
   locally and must not be published.
