# Licensing and provenance

What the audit found, and what the Rust rewrite may do with it. Evidence is cited as `path:line`. C++
paths are relative to `/Users/far-mini/repos/arranger-midi-daw`; reference paths to
`.reference/<repo>`; this repository's paths to its root. A bare line number after a full path continues
that file. Verified 2026-11-22.

## 1. The C++ application: no licence, all rights reserved

| Check | Result |
|---|---|
| Root files on the audited commit `8ca4164` | no `LICENSE`, `COPYING` or `NOTICE` |
| Root files on `origin/main` (`975e97d`) | no `LICENSE`, `COPYING` or `NOTICE` |
| GitHub API `GET /repos/nhth-mibu/arranger-midi-daw` | `"license": null`, `"private": false`, `"default_branch": "main"` |
| GitHub API `GET /contents/?ref=main` | root listing contains no licence file |
| `git log --all --diff-filter=D -- 'LICENSE*' 'COPYING*' 'NOTICE*'` | empty: no licence was ever deleted |
| `git ls-tree -r --name-only HEAD` | no licence file |
| Only licence files in the tree | `resources/fonts/Bravura-LICENSE.txt`, `resources/fonts/Inter-LICENSE.txt`, both third-party fonts |
| The repository's own statement | `docs/release-notes-v0.1.0.md:61`: "The application source code is private. Use the binaries only according to the licences of their components." Restated at `docs/release-notes-v0.1.1.md:42` ("stays private") |
| Licence claim in `README.md` or `docs/` | none |

**Verdict.** No grant of rights on the audited commit, on `origin/main`, or on any other ref. Public
visibility is not a licence. The default is **all rights reserved** by the copyright holder. The only documented
licence-policy decision is negative: Chordino (GPL) and the madmom models (non-commercial) were rejected
(`docs/technical-overview.md:67`).

What that means per artefact class:

| Category | Status | Consequence |
|---|---|---|
| C++ source | No grant | Copying any file, function or comment makes the Rust tree a derivative of an unlicensed work |
| Device and controller profiles (8 device + 3 controller YAML files) | Repo-authored data files | Re-author from device manuals and MIDI/SysEx facts. Device facts are not copyrightable; these files are |
| Styles and style fixtures (`examples/demo-style.sty`, any `.sty`) | Repo-authored expression on a public format | The SFF layout is a format fact; the note data and marker text in this file are the repo's expression |
| Fonts (`Bravura.otf`, `Inter-*.ttf`) | Third-party, SIL OFL 1.1 | Not the app's to license; the OFL notice must travel with any copy |
| SoundFont (`GeneralUser-GS.sf2`) | Third-party, downloaded at runtime | Separate terms, not granted by this repo; redistribution wording is not in the repo |
| AI model weights and Python helpers | Third-party, separate licences | Apache/MIT code does not license the weights |
| Golden files, example songs, `docs/` text and images | Repo-authored expression | Do not copy |
| Test fixtures the owner supplies | Owner's call | Usable only with the owner's grant on record |

### 1.1 The three fixtures already in this repository

`tests/fixtures/` holds three files that are **byte-identical** to files in the C++ repository. SHA-256
equality was checked on 2026-11-22.

| File here | Size | SHA-256 | C++ source | Used by |
|---|---|---|---|---|
| `tests/fixtures/legacy-cpp-v3.arranger` | 26,085 B | `f7b241460937f1a93f43f98792be80b0d8353dd9d2381dbbd5df4b1b6a48b97e` | `examples/demo.arranger` | `crates/project/src/legacy.rs:533`; `apps/cli/tests/sprint_exit.rs:17` |
| `tests/fixtures/legacy-cpp-v4.arranger` | 9,603 B | `a9e4f729ff1a86628d76f79f2602cce174b8de8cb7bc86b1669677a5e325f6f8` | `examples/Copper Lights/Copper Lights (Audio).arranger` | `crates/project/src/legacy.rs:534` |
| `tests/fixtures/demo-style.sty` | 3,940 B | `b0e241a2b027c7279bda7d8c8ab31168c12e9ff0e4801cbd564735d26bc99141` | `examples/demo-style.sty` | `crates/style-formats/src/lib.rs:213` |

All three are embedded into test binaries with `include_str!`/`include_bytes!`. They come from the C++
repository, not from a third party, and that repository grants nothing. Distributing this repository
publicly redistributes them. Record the owner's written grant, or replace them with synthetic fixtures
before any public release. This is the only known provenance exception in the tree.

## 2. Cadenza and Vibez: GPL-3.0-or-later

| Repository | SPDX declaration | Licence file | Dual licence |
|---|---|---|---|
| Cadenza | `license = "GPL-3.0-or-later"` (`Cargo.toml:8`) | `LICENSE`, verbatim GPLv3, 674 lines, md5 `1ebbd3e34237af26da5dc08a4e440464` | none |
| Vibez | `license = "GPL-3.0-or-later"` (`Cargo.toml:19`); `pub const LICENSE: &str = "GPL-3.0-or-later";` (`crates/vibez-ui/src/about.rs:12`) | `LICENSE`, byte-identical to Cadenza's | none |

Quoted lines:

```text
cadenza/Cargo.toml:8                     license = "GPL-3.0-or-later"
vibez/Cargo.toml:19                      license = "GPL-3.0-or-later"
vibez/crates/vibez-ui/src/about.rs:12    pub const LICENSE: &str = "GPL-3.0-or-later";
```

No `LICENSE-MIT`, no `LICENSE-APACHE`, no per-file SPDX headers, no carve-out module. Operative GPLv3
text: **§5** (`LICENSE:208`) requires a work "based on the Program" to be licensed "as a whole" under
GPLv3; **§10** (`:446`) forbids added downstream terms, so GPL code cannot be re-offered as MIT; **§11**
(`:471`) ties the patent grant to GPL terms. **§13** (`:552`) is the AGPL interop clause, not a network
clause, so running a fork as a service is not a GPL trigger.

Consequences for this repository, which declares `license = "Apache-2.0"` (`Cargo.toml:9`) and states the
same in `README.md` under Licence:

| Action | Result |
|---|---|
| Read the source and re-implement the architecture | Allowed. Ideas and interfaces are not copyrightable |
| Copy a function, module or file | Forbidden. The whole Rust work becomes GPLv3 |
| Link a Cadenza or Vibez crate as a dependency | Forbidden. A statically linked Rust binary is one work under §5 |
| Vendor a snippet "to check the algorithm" | Forbidden, and the hardest mistake to unwind |
| Run their binary as a separate process over IPC | Allowed as mere aggregation, and useless here |
| Cite their SHAs in design notes | Allowed and expected |

This workspace is Apache-2.0, so neither repository's code may be imported. Re-implementing an idea is the
only route.

**Vibez carries an extra in-tree hazard: an aubio port.** `crates/vibez-core/src/onset/aubio.rs:1-9`
declares it verbatim:

```text
//! Rust port of aubio's offline onset detector.
//! ... are derived from aubio 0.4.9. ...
//! Original work copyright (C) 2003-2014 Paul Brossier <piem@aubio.org>.
//! aubio is licensed under GPL-3.0-or-later. Vibez carries the same compatible
//! licence.
```

That file (552 lines), `crates/vibez-core/src/onset.rs` (877) and the onset tests derive from GPL-3
aubio, so they are not Vibez's to relicense. Do not read `crates/vibez-core/src/onset/aubio.rs` while
writing our own onset detector. Nothing from either
repository has been copied into this tree. `.reference/` is gitignored (`.gitignore:2`), so no GPL source
is committed; keep it that way.

## 3. Third-party dependencies of the C++ application

| Component | Version | Licence | Evidence | Risk |
|---|---|---|---|---|
| JUCE | 8.0.15 | AGPLv3 **or** commercial | `CMakeLists.txt:75-79` (pin at `:77`); repo's own statement `docs/build-plan.md:28` ("Free personal tier under the revenue cap, or AGPLv3") | **High.** Copyleft, or a paid licence |
| FluidSynth | v2.5.7 | LGPL-2.1 | `src/soundfont/CMakeLists.txt:1,17` | **Medium.** Weak copyleft; statically linked here |
| yaml-cpp | 0.9.0 | MIT | `CMakeLists.txt:51-62` (pin at `:55`) | Low |
| nlohmann/json | 3.11.3 | MIT | `CMakeLists.txt:38-44` (pin at `:40`) | Low |
| whisper.cpp | v1.8.7 | MIT | `src/whisper/CMakeLists.txt:1,5` | Low |
| Bravura (SMuFL) | — | SIL OFL 1.1 | `resources/fonts/Bravura-LICENSE.txt:4` | **Medium.** OFL: derivatives must stay OFL |
| Inter Regular/SemiBold | — | SIL OFL 1.1 | `resources/fonts/Inter-LICENSE.txt:3` | **Medium.** Same |
| GeneralUser GS SoundFont | downloaded at first use | Custom permissive ("use and redistribution in software") | `src/app/GmSoundFont.cpp:11-14` | **Unverified.** Only the repo's summary is available |
| PANNs (Cnn14) | pip | MIT code, **weights CC BY 4.0** | `docs/technical-overview.md:58` | **Medium.** Attribution required |
| Demucs (htdemucs) | pip | MIT | `docs/technical-overview.md:56` | Low |
| Basic Pitch | pip `basic-pitch[onnx]` | Apache-2.0 | `docs/technical-overview.md:57` | Low |
| Chordino | — | GPL | `docs/technical-overview.md:67` | **Rejected** by the project |
| madmom models | — | non-commercial | `docs/technical-overview.md:67` | **Rejected** by the project |

Also present, lower risk:

| Component | Version | Licence | Evidence |
|---|---|---|---|
| Catch2 (tests only) | v3.7.1 | BSL-1.0 (upstream) | `tests/CMakeLists.txt:1-4` |
| `@magenta/music` / `tone` helper | 1.23.1 / 14.7.77 | Apache-2.0 / MIT (upstream) | `resources/ai/magenta_package.json:5,7-8` |
| Anticipatory Music Transformer helper | — | Apache-2.0 | `resources/ai/arranger_generate.py:10-16`; the file states its token layout and sampling loop are "taken from that project", so Apache-2.0 attribution applies |
| sherpa-onnx (TTS) | v2.0.1 | Apache-2.0 (upstream) | `src/app/Speech.cpp:26-37` |
| yt-dlp, ffmpeg | user-installed | Unlicense / LGPL+ | `src/core/include/arranger/core/LinkAudio.h:1-9`; not bundled |

The project's own AI helper copies code and layout from an Apache-2.0 project. Attribution and notice
retention are conditions of that licence; nothing in the repo records them.

## 4. What the Rust rewrite may and may not do

| May | Why |
|---|---|
| Re-implement behaviour, file formats and protocols from observation | Format and protocol facts are not copyrightable; the code must be independent |
| Re-author device and controller profiles | Device facts come from manuals and the MIDI/SysEx specs |
| Re-derive SMuFL codepoints from the SMuFL spec | Public specification; do not lift the mapping table |
| Ship Bravura or Inter copies under OFL 1.1 with the OFL text and reserved font names | OFL permits redistribution with notice; it forbids relicensing the font itself |
| Keep owner-supplied test fixtures | The owner can grant this; record the grant in the repository |
| Choose any project licence | True only while no GPL or unlicensed code enters the tree |

| May not | Why |
|---|---|
| Copy C++ source, comments or tests | Unlicensed; all rights reserved |
| Copy the YAML device/controller profiles | Repo-authored expression; `resources/devices/*.yaml` |
| Copy the fonts without their OFL notices, or relicense them | OFL 1.1 |
| Bundle `GeneralUser-GS.sf2` before its terms are read | Terms unverified |
| Bundle or mirror AI model weights | Separate licences; PANNs weights are CC BY 4.0 |
| Copy style fixtures, example songs, goldens or `docs/images` | Repo expression |
| Import Cadenza or Vibez code, or link their crates | GPL-3.0-or-later, whole-work effect |
| Read `crates/vibez-core/src/onset/aubio.rs` while writing an onset detector | GPL-3 derivation; risks copying expression |

Rule for contributors: treat both reference repositories as documentation with an executable proof. Read
them, cite their SHAs, re-derive the patterns in our own words, then write the code from the description.

## 5. Actions required before any public release

1. Licence text is present: the destination repository (`nhth-mibu/Lazy-Arranger-v2`) already
   carried `LICENSE` with the Apache-2.0 text, and `Cargo.toml:9` now declares `Apache-2.0` to match. No
   separate MIT grant exists, so the workspace is Apache-2.0 only.
2. Record the owner's written grant for the three copied fixtures, or replace them with synthetic
   fixtures (§1.1).
3. Add a `THIRD-PARTY.md` (or `NOTICE`) listing the Rust dependencies and their licences. MIT and
   Apache-2.0 require retaining copyright notices on redistribution.
4. Add a `deny.toml` allow-list and a `cargo-deny` CI step. `.github/workflows/ci.yml` has no licence
   gate today; Cadenza's `deny.toml:22-39` is a usable model.
5. Keep `/.reference` gitignored (`.gitignore:2`) and out of every release artefact.
6. Before tagging, diff the tree against the C++ repo and confirm no source, YAML, font, soundfont,
   weight or style file entered.
7. If fonts are added later, ship the OFL text and keep the reserved font names.
8. If PANNs weights or the Anticipatory helper are redistributed, add the CC BY 4.0 attribution and the
   Apache-2.0 notice.
9. Do not bundle `GeneralUser-GS.sf2` until its redistribution terms are read from upstream.
10. Add SPDX headers or a per-crate `license` field only after the licence texts exist, so the
    declaration and the files agree.

## 6. Rust dependencies used today

`Cargo.lock` carries no licence field, so each licence below was read from the crate's own
`Cargo.toml` in `.cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/`.

| Direct dependency | Version | Licence | Declared at | Registry evidence |
|---|---|---|---|---|
| `serde` | 1.0.229 | MIT OR Apache-2.0 | `Cargo.toml:12` | `serde-1.0.229/Cargo.toml:41` |
| `serde_json` | 1.0.154 | MIT OR Apache-2.0 | `Cargo.toml:13` | `serde_json-1.0.154/Cargo.toml:36` |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | `Cargo.toml:14` | `thiserror-2.0.21/Cargo.toml:36` |
| `midly` | 0.5.3 | Unlicense | `Cargo.toml:15` | `midly-0.5.3/Cargo.toml:34` |
| `midir` | 0.10.4 | MIT | `Cargo.toml:16` | `midir-0.10.4/Cargo.toml:35` |
| `clap` | 4.6.9 | MIT OR Apache-2.0 | `Cargo.toml:17` | `clap-4.6.9/Cargo.toml:42` |

Development-only dependencies add nothing new: `serde_json` (already listed) and the path crate
`sequencer` (`crates/smf/Cargo.toml`). The eight workspace crates inherit `license = "Apache-2.0"`
from `Cargo.toml:9`.

Full lock-file scan, 2026-11-22: 98 packages. 87 are external crates present in the local registry cache,
and **all 87 are permissive** — 66 `MIT OR Apache-2.0`, 11 `MIT`, 4 `Apache-2.0 OR MIT`, 1
`Apache-2.0/MIT`, 1 `MIT/Apache-2.0`, 1 `Zlib` (`foldhash` 0.2.0), 1 `Unlicense OR MIT` (`memchr`
2.8.3), 1 `Unlicense` (`midly`), 1 `(MIT OR Apache-2.0) AND Unicode-3.0` (`unicode-ident` 1.0.27).
No GPL, AGPL, LGPL, MPL, SSPL or non-commercial licence is present in the checked set.

Three external crates are not in the cache, so their licences are **unverified**:
`anstyle-wincon` 3.0.11, `once_cell_polyfill` 1.70.2 and `windows-sys` 0.61.2. They are Windows-only
targets and were not downloaded on this macOS host: `anstyle-wincon` is pulled in by `anstream`
(`Cargo.lock:36`), and both it and `windows-sys` build only on Windows. Check them on a Windows build or
from crates.io before release.

`Unlicense` is a public-domain dedication with an express permission grant, so `midly` is not in the
"no licence" position that the C++ application is in.
