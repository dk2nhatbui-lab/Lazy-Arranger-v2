# Feature matrix — C++ Lazy Arranger to Rust v2

**Oracle:** `/Users/far-mini/repos/arranger-midi-daw` at `8ca41644a2be5f5677bce8cae060a55f7b19e8fc`, the tip of
`fix/audit-critical-high`; 10 commits ahead of `origin/main` `975e97d1004b8a9813339fddbc258c3bcf5eb6a5`,
0 behind. Read-only; not built, not modified.

**Evidence:** the five notes in `docs/audit/notes/`. Citations are `path:line` from those notes, or the
primary C++ declaration where a note gives no line. LOC measured with `wc -l` in the pinned tree.
Composite rows overlap: one file can serve several subsystems.

**Rust status today (round 215):** `cargo test --workspace` passes 643 tests; `cargo clippy --workspace --all-targets
-- -D warnings`, `cargo fmt --all --check` and `cargo doc` (warnings denied) pass. No subsystem is
`parity complete`. This matrix is the original file-level inventory; the current row-level view and
evidence are in [`cpp-features-crosscheck.md`](cpp-features-crosscheck.md).

**Status key**

| Status | Meaning |
|---|---|
| `not started` | No Rust code for this subsystem. |
| `prototype` | Code exists and is tested in part; behaviour or data model is incomplete against the oracle. |
| `verified` | The Rust deliverable named for this row is proven by a named test. Parity gaps stay in the notes column. |
| `parity complete` | The golden scenarios for this subsystem pass against the C++ oracle on every required input. |

**Decision key:** `keep` = same behaviour and design; `port` = re-implement the audited behaviour;
`redesign` = keep the user-visible contract, replace the implementation; `defer` = out of the first release.

## 1. Subsystem decisions

| # | Subsystem | C++ evidence (file:line) | C++ LOC | Rust target crate | Decision | Stage | Status | Notes / risk |
|---|---|---|---|---|---|---|---|---|
| 1 | Song model | `Song.h:23-32,241-333,428-458`; `SongEdit.cpp:45-48` | 1,913 | `project` | port | 1 | prototype | Schema v1 has tracks, notes, chords, arranger events, markers and a tempo map. Clips, slots, scenes, mixer, sends, forms, audio and the shared id counter are absent. |
| 2 | Project file | `ProjectFile.cpp:694-701,833-921,959-1001` | 1,070 | `project` | port | 1 | prototype | One-way import of C++ schema 1..4 into schema v1. No C++ writer, no byte-stable save, no load-repair report. Unknown keys are preserved here, dropped by C++. |
| 3 | Tempo / timing | `Time.h:10-13`; `TempoMap.cpp:79-83`; `Transport.cpp:8-16`; `Engine.cpp:354-358` | 442 | `music-core`, `sequencer` | keep | 2 | prototype | 960 PPQ, step tempo and the anchor-0 ceiling are tested (`rounding_never_lands_before_the_tick`). No anchor state, loop wrap or per-track release; a located start can differ by ≤1 sample. |
| 4 | Chord model and chord input | `StyleChord.h:13-34`; `ChordInput.cpp:187-201,227-241` | 1,717 | `music-core`, `project`, `arranger` | port | 3 | prototype | `music-core` parses 15 qualities; `arranger` now carries all 35 Yamaha types with their interval and scale tables, and maps the 15 onto them. No chord lane, typing, suggestions, voicings or reharmonise. |
| 5 | Smart Chords / key finding / theory | `Theory.h:44`; `ChordFinder.cpp`; `MainComponent.cpp:1330`; `ChordTests.cpp:22-90` | 1,531 | `music-core`, `arranger` | port | 3 / 8 | not started | Smart Chords is the `chords` AI panel. Rust has no key finding, roman numerals, modulation or voice leading. The panel itself belongs to Stage 8. |
| 6 | Notation and MusicXML/PDF | `Notation.h:31`; `ScoreLayout.h:37`; `MusicXml.h:18`; `Pdf.h:15` | 3,338 | new `notation` | port | 8 | not started | Pure model→symbols and one layout feeding screen and PDF. Six C++ score goldens are unlicensed and must be recaptured, not copied. Re-derive SMuFL codes from the spec. |
| 7 | Piano roll | `PianoRoll.h:17` | 1,053 | `arranger` UI | redesign | 6 | not started | All editing and painting lives in JUCE. No logic worth carrying; the snap and drag rules must be restated as tests. |
| 8 | Clip grid and scenes | `Engine.cpp:278-289`; `ClipGridView.cpp:19` | 1,198 | `project`, `sequencer` | port | 3 / 6 | not started | Scene = slot-array index is the riskiest wire structure. `ArrangerEvent` exists; no scene, slot, launch-quantise or Timeline-takeover model. |
| 9 | Arranger style files (SFF) | `StyleFile.cpp:345-355` | 698 | `style-formats`, `arranger` | port | 4 | verified | Container scan is padding-aware; rule tables parse per section; section markers and notes import into a playable `arranger::Style`. 16,158 real styles imported (234,332 sections, 48.8M notes). Still missing: style writer, setup events, voices, `CSEG` vendor children. |
| 10 | CASM chord conversion | `StyleFile.cpp:197-234`; `StyleConvert.cpp:38-65` | 1,421 | `style-formats`, `arranger` | port | 4 | verified | The eight-step conversion, the 35-row table, the part gate and the three note ranges are implemented and pinned to the C++ cases. `style-formats` decodes SFF1/SFF2 rule tables, including both bass encodings, and parsed 16,189 real styles (856,370 rules) read-only. Mute-mask polarity stays unverified against a device. |
| 11 | Style playback / sections / fills | `Engine.cpp:278-289,326-350,563-579`; `StyleConvert.cpp:494-527` | 1,818 | `arranger`, `sequencer` | redesign | 3 | prototype | `Style::compile` splits a section at chord changes, gates parts, converts notes and cuts at boundaries; imported sections compile and render to MIDI; `TransitionState` owns the four modes. No fills-return sugar, no retrigger (`Rtr`), no live playback. |
| 12 | Quantise | `Quantise.cpp:41-67` | 98 | `music-core`, `app-commands` | port | 3 | not started | Small but exact: grid, strength, swing, ends and window. A multi-clip quantise must stay one undo step. |
| 13 | Groove | `Groove.h:55` | 2,032 | new `groove` | port | 8 | not started | Timing and velocity templates, three drum lanes, feel transfer, listening test. |
| 14 | Arrangement doctor | `Arranging.h:71` | 1,300 | new `arranger` | port | 8 | not started | Rule-based proposals over density, register, repetition and contrast. No model dependency. |
| 15 | Hardware fit | `HardwareFit.h:26` | 784 | `midi-devices`, `arranger` | port | 8 | not started | Reads device `capabilities`; protects bass, melody and drums. Depends on device-profile import. |
| 16 | Song forms | `SongForm.cpp:1418-1444` | 2,695 | `project`, `arranger` | port | 8 | not started | Alternative section orders, transitions, preview and one-undo commit. Second-largest core file. |
| 17 | Mixer / buses / automation | `Song.h:217-227,368-379`; `MixerView.cpp:325` | 1,509 | `project` | port | 7 | not started | Values are MIDI 0..127 so they map 1:1 to CC7/10/91/93. Buses, sends, plugin lanes and automation are model work first, UI later. |
| 18 | Audio tracks / samples / time-stretch | `Song.h:72-88`; `Sample.h:13`; `TimeStretch.h:11` | 959 | `project`, `audio-engine` | port | 8 | not started | WSOLA 0.25–4×; audio file stays external; higher layer wins overlap; stretch uses the tempo at clip start. |
| 19 | Plugin hosting | `PluginHost.h:61` | 829 | new `plugin-host` | redesign | 8 | not started | JUCE format manager cannot be reused. Needs out-of-process scan, state, latency compensation and crash policy. |
| 20 | Audio import/export and codecs | `AudioFiles.cpp:10`; `MainComponent.cpp:2198` | 177 | `audio-engine` | redesign | 7 / 8 | not started | WAV/FLAC/MP3 in, video through ffmpeg, WAV-only render out. SMF export is done at Stage 2. Codec set is undecided. |
| 21 | Audio analysis | `AudioAnalysis.cpp:611` | 2,413 | new `audio-analysis` | port | 8 | not started | Reads helper JSON; local chroma + Viterbi chord finding. Models stay external. |
| 22 | Whisper lyrics | `Lyrics.cpp:25` | 176 | new `ai-services` | port | 8 | not started | Thin wrapper over whisper.cpp (MIT). Model downloaded, never bundled. |
| 23 | MidiAi helpers | `MidiAi.h:30` | 1,304 | new `ai-services` | port | 8 | not started | Request/answer JSON for Magenta and Anticipatory Transformer; humanise is local. 14 canned-answer tests. |
| 24 | MCP server and skills (93+5 tools) | `McpServer.cpp:835` | 5,956 | new `mcp-server` | port | 8 | not started | 93 tools plus 5 AI tools, JSON-RPC over stdio plus a local socket. Largest single surface. |
| 25 | Kids mode | `KidsMode.h:13` | 398 | `arranger` | defer | 8 | not started | Any key plays; pause 0.5–5 s; resume from there. Self-contained, low value. |
| 26 | Device profiles | `DeviceProfile.cpp:331` | 950 + 6,971 YAML | `midi-devices` | port | 7 | not started | `profile: 1` hard gate; sounds, editor pages, mixer CC map, capabilities. YAML must be re-authored, not copied. |
| 27 | Controller profiles | `ControllerProfile.h:12` | 596 + 124 YAML | `midi-devices` | port | 7 | not started | Bindings, pads, pad lights and surface pages; page/mode/layer and channel scoping. |
| 28 | Mixer pages / MIDI learn | `MidiMapView.cpp:607`; `MixerView.cpp:325` | 2,120 | `arranger`, `app-commands` | port | 7 | not started | Learn waits for the first ≥3-byte message, then assigns track/action/row. Learn timeout and conflict rules need tests. |
| 29 | SysEx files | `SysexFile.h:8` | 196 | `midi-devices` | port | 7 | not started | `.syx` split/join; an interior byte ≥0x80 is rejected. |
| 30 | Synth editors | `SynthEditor.h:66` | 2,910 | `midi-devices` | port | 7 | not started | YAML-driven CC/NRPN/RPN/SysEx pages; checksums `roland`, `sum7`, `bitXor`, `none`; 1–4 address bytes. |
| 31 | Playlists / Music Finder | `Playlist.h:18`; `MusicFinder.h:22` | 315 | `style-formats`, `project` | defer | 8 | not started | Yamaha `.pgl` and Music Finder books. Device-specific and small. |
| 32 | Recorder import | `RecorderImport.cpp:10-57` | 754 | new `recorder-import` | port | 8 | not started | Zoom L6max file names, newest take per channel, 2 GB splits, mix last; yt-dlp link intake. |
| 33 | Autosave / recent files | `Autosave.cpp:113-131` | 494 | `project`, app | port | 7 | not started | 60 s first write, 30 s heartbeat, 30-day orphan rule, and the literal `Arranger MIDI DAW` folder name. |
| 34 | Localization | `Localization.cpp:11-34` | 977 | `arranger` | port | 6 | not started | da/fr/vi plus pseudo, about 203 keys each. Font must cover Vietnamese. |
| 35 | Update checker | `UpdateChecker.cpp:48` | 277 | app | port | 8 | not started | GitHub releases plus semver. Keep the comparison, replace the transport. |
| 36 | Metronome | `Metronome.cpp:47-59` | 177 | `sequencer`, `audio-engine` | port | 3 | not started | Click is synthesised in the engine. Rust emits no audio, so only schedule-level parity is possible until Stage 7. |
| 37 | Session / transport | `Session.cpp:146-159`; `Transport.cpp:55-62` | 5,139 | `sequencer`, `midi-devices` | port | 2 | prototype | Play, stop, seek and publish are tested. `midi-devices::dispatch` sends a prepared schedule to a port with an injected clock and ends with all-notes-off. No re-anchor, loop wrap, held-note policy or in-app recording of arranger output. |

LOC totals overlap. Examples: `Song.h` counts in rows 1, 17 and 18; `Engine.cpp` in rows 3, 8, 11 and 37;
`MidiMapView.cpp` in rows 28 and 30.

## 2. Stage 1–3 deliverables

`verified` means the named test calls the production function and asserts the behaviour. It does not mean
the subsystem is parity complete.

| Deliverable | Rust location | Test that proves it | Status | Not covered |
|---|---|---|---|---|
| 960 PPQ, tick type, meter geometry | `crates/music-core/src/time.rs` | `quarter_conversion_round_trips`, `signature_geometry`, `boundary_helpers_handle_negative_ticks` | verified | Meter map; song-level meter changes. |
| Step tempo map and seconds from tick 0 | `crates/music-core/src/tempo.rs` | `constant_tempo_geometry`, `tempo_changes_are_piecewise_constant`, `long_maps_do_not_drift` | verified | Ramps; tempo clamping (rejected here). |
| Ceiling tick→sample with 1e-6 epsilon | `crates/music-core/src/tempo.rs::seconds_to_samples`, `TempoMap::samples_at` | `rounding_never_lands_before_the_tick` | verified | Anchor state; locate re-anchor. |
| Chord parse, format, tones | `crates/music-core/src/chord.rs` | `parses_plain_triads_and_sevenths`, `renders_with_either_spelling`, `tones_and_bass_follow_the_symbol` | verified | 20 Yamaha qualities; 35-type CASM table. |
| MIDI messages and order rank | `crates/music-core/src/midi.rs` | `note_messages_encode_to_wire_bytes`, `ordering_puts_note_offs_first`, `decoding_inverts_encoding` | verified | SysEx (deliberately absent). |
| Project schema v1 round trip | `crates/project/src/song.rs` | `model_round_trips_with_content`, `empty_song_round_trips_through_json` | verified | Clips, scenes, mixer, audio, style. |
| Unknown-key preservation | `crates/project/src/song.rs` | `unknown_keys_survive_a_round_trip` | verified | — |
| Format and version gate | `crates/project/src/song.rs` | `rejects_wrong_format_and_newer_schema` | verified | C++ version 1..4 gate lives in the importer. |
| Structural validation with paths | `crates/project/src/validate.rs` | `catches_geometry_errors`, `warns_on_overlapping_same_pitch_notes`, `catches_invalid_tempo_map_from_json` | verified | Clip geometry, sections, stale form ids. |
| Legacy C++ v3 import | `crates/project/src/legacy.rs` | `imports_the_demo_project`, `clip_offsets_become_absolute_ticks` | verified | Slots, scenes, mixer, style. |
| Legacy C++ v4 import | `crates/project/src/legacy.rs` | `imports_a_schema_4_audio_project`, `keeps_track_ids_from_the_shared_cpp_counter` | verified | Audio clips, automation, buses. |
| Typed commands and inverse undo | `crates/app-commands/src/lib.rs` | `add_note_then_undo_then_redo`, `invalid_note_is_rolled_back`, `unknown_targets_are_rejected_without_mutation` | verified | Coalescing; preview/approval. |
| Identifier allocation | `crates/app-commands/src/lib.rs` | `identifiers_are_allocated_above_the_highest_in_use`, `duplicate_identifiers_are_rejected` | verified | Shared C++ nextId space. |
| Absolute sample compilation | `crates/sequencer/src/lib.rs::PreparedPattern::prepare` | `events_land_on_exact_samples`, `every_prepared_event_is_emitted_once`, `tempo_changes_are_compiled_into_sample_positions` | verified | Anchor, loop cycles, slots. |
| Block-size independence | `crates/sequencer/src/lib.rs::Scheduler::process_block` | `block_size_does_not_change_event_positions`, `recompiled_snapshot_keeps_the_position` | verified | Loop wrap at block edge. |
| Ordering, clamp, overflow | `crates/sequencer/src/lib.rs` | `note_offs_sort_before_note_ons_in_one_sample`, `simultaneous_events_order_by_track`, `block_offsets_clamp_inside_the_block`, `overflow_is_counted_not_panicking` | verified | Per-track active notes; redundant off suppression. |
| SMF encode/decode, 960 PPQ rescale | `crates/smf/src/lib.rs` | `round_trip_preserves_notes_tempo_and_markers`, `decodes_foreign_ppq_by_rescaling`, `simultaneous_note_off_comes_before_note_on_in_the_file` | verified | SysEx, later meters, markers in file. |
| MIDI capture pairing | `crates/midi-devices/src/capture.rs` | `pairs_note_on_and_note_off`, `repeated_pitch_closes_the_oldest_note_first`, `conversion_to_ticks_follows_the_tempo_map` | verified | Hardware timestamps; jitter. |
| MIDI port enumeration | `crates/midi-devices/src/devices.rs` | `enumeration_reports_ports_or_an_unavailable_backend`, `backend_failure_is_classified` | verified | Hardware behaviour; reconnect. |
| Record and echo on hardware | `crates/midi-devices/src/devices.rs::record_to_song`, `echo` | No test exists; hardware check only | prototype | Everything; see baseline §6. |
| Real-time MIDI dispatch | `crates/midi-devices/src/dispatch.rs::play_timed` | `sends_messages_at_their_offsets`, `equal_timestamps_do_not_wait_between_messages`, `unsorted_messages_are_rejected_before_playback`, `a_sink_error_stops_playback` | verified | Port latency, jitter, hardware. |
| All-notes-off panic path | `crates/midi-devices/src/dispatch.rs::all_notes_off_messages` | `panic_messages_cover_each_channel_once` | verified | Device acknowledgement. |
| 35-type chord vocabulary | `crates/arranger/src/chord_type.rs` | `every_type_has_table_data_and_a_stable_index`, `tones_match_the_oracle_examples`, `names_match_the_oracle` | verified | CASM byte mapping. |
| Chord-rule transposition | `crates/arranger/src/transpose.rs::transpose` | `pinned_bass_cases_from_the_oracle`, `pinned_fixed_root_cases_from_the_oracle`, `bass_flag_follows_the_slash_bass`, `split_ranges_choose_their_own_rule`, `scale_flavours_behave_like_melody` | verified | Guitar mode has no oracle case; retrigger. |
| Part gating and mute masks | `crates/arranger/src/transpose.rs::plays` | `mute_masks_gate_parts`, `cancel_gating_follows_the_chord_mute_bit` | verified | Mask polarity against a real file. |
| Section compile with chord following | `crates/arranger/src/style.rs::Style::compile` | `chord_changes_split_the_section`, `notes_are_cut_at_a_chord_change`, `no_chord_stops_every_part_but_rhythm`, `notes_are_cut_at_the_section_end` | verified | Fill lengths; retrigger; real style data. |
| Transition modes and precedence | `crates/arranger/src/transition.rs::TransitionState` | `next_bar_uses_the_bar_boundary`, `the_latest_request_wins_at_one_boundary`, `end_of_section_uses_the_supplied_section_end` | verified | UI wiring; fill-return convenience. |
| CLI sprint exit | `apps/cli/tests/sprint_exit.rs` | `four_bar_phrase_schedules_at_exact_samples`, `block_size_never_changes_the_rendered_phrase`, `scheduled_events_match_the_exported_midi_file`, `legacy_project_imports_renders_and_exports`, `odd_sample_rate_keeps_note_ordering`, `a_legacy_import_reports_what_it_deferred` | verified | Style playback; MIDI out. |
| Style container scan | `crates/style-formats/src/lib.rs` | `scans_the_demo_style_chunks`, `describes_a_style_without_casm`, `handles_a_container_with_casm_and_cseg` | verified | CASM parse, rules, sections, writer. |

## 3. Top 10 port risks, ordered by cost

Cost basis is the audited C++ surface plus the missing oracle. Figures are LOC, not estimates of weeks.

| Rank | Risk | Why it costs | Cost basis | Mitigation |
|---|---|---|---|---|
| 1 | Audio engine, SoundFont and plugin hosting | No Rust audio path exists. C++ relies on JUCE and FluidSynth; both are ruled out. Plugin state, latency compensation and crash policy are unported. | `Engine.cpp` 1,178 + `PluginHost` 829 + JUCE host; large redesign | Keep Stage 3–6 MIDI-only. Choose a SoundFont crate and a plugin host by spike before Stage 7. |
| 2 | MCP server and skills | 98 tool behaviours, JSON-RPC framing, local socket and app-side service. Nothing exists in Rust. | 5,956 LOC, 93+5 tools | Port the command layer first. Generate tool schemas from typed commands. Do not hand-write 98 paths. |
| 3 | Chord conversion and CASM | Audible output of every style. The 35-row table is not in the notes; polarity is unverified; no real style fixture. | 1,421 LOC + 35-row data + corpus | Transcribe the table, implement the 8-step spec, verify polarity on one real PSR style before coding. |
| 4 | Song forms, arranging doctor, groove, hardware fit | Four large pure-music systems with separate tests and no shared model. | 5,606 LOC | Port one at a time; each is testable headless. Defer all to Stage 8. |
| 5 | Notation, score layout, MusicXML and PDF | 3,338 LOC plus six goldens that cannot be copied. SMuFL mapping and font licensing need re-derivation. | 3,338 LOC + 6 goldens + OFL fonts | Stage 8 only. Re-derive SMuFL from the spec. Author new score fixtures; if C++ output is used, record provenance and permission. |
| 6 | UI shell: piano roll, clip grid, mixer, score view | Every editor is JUCE painting. Logic and drawing are entangled. | About 4,824 LOC across four views + `MainComponent` 2,981 | Decide the toolkit (ADR-001) before Stage 6. Move snap/drag rules into headless tests. |
| 7 | Project, scene and mixer model migration | C++ v4 is a floor with 21 unversioned keys. Scene position is semantic. Mixer and automation are absent from schema v1. | `Song.h` 469 + `ProjectFile.cpp` 1,026 + `SongEdit.cpp` 1,170 | Add schema 2 with clips, scenes and mixer. Keep one-way import until a byte-stable writer has goldens. |
| 8 | MIDI output, timing and stuck-note policy | Rust has no output scheduler, latency delay, panic or active-note bookkeeping. Hardware is unverified. | Part of `Engine.cpp` 1,178 + host path | Port per-track `ActiveNotes`, panic and stop. Test with loopback before a keyboard. |
| 9 | Audio analysis, whisper, MidiAi | Large helpers with external models and model-weight licensing. | 2,413 + 176 + 1,304 LOC | Stage 8. Keep models out of the repository. Gate on licence review. |
| 10 | Fixture replacement and golden recapture | Three fixtures are byte-identical copies from an unlicensed repository. Goldens do not exist yet. | Low engineering, high schedule risk | Replace or obtain permission before release. Capture goldens at a pinned C++ commit with hashes. |

## 4. Out of scope for the first release

The first release is the blueprint §16 definition of done, not the whole C++ surface.

* Korg Pa native import, export and style model. Research track only.
* VST3/CLAP hosting, plugin scanning, plugin GUI embedding and plugin-state migration.
* Multitrack audio recording, waveform editing, time-stretch and audio clip editing.
* Notation, score layout, MusicXML and PDF export.
* The 93+5-tool MCP server, skills packs and the AI chat surface.
* MidiAi, whisper lyrics, speech/TTS and audio analysis.
* Kids mode, playlists, Music Finder and recorder import.
* Controller profiles, synth editors, MIDI learn, mixer pages and SysEx device files.
* Localization beyond English; update checker.
* Autosave and crash recovery (Stage 7, after the document model settles).
* Audio import, video import and audio render. MIDI import/export stays in scope.
* Hardware capability checking and hardware fit.
* Byte-stable C++ project save. Interchange is SMF plus one-way import.
* Universal remote control of Yamaha/Korg arranger functions.
* A guarantee of bit-for-bit parity with all C++ output. Parity is the chosen golden subset.

## 5. Recommendation after Stage 2

Build the Stage 3 arranger vertical slice next, and build it against goldens before building a style
importer. That slice is: transcribe the 35-row chord table, implement `plays` and the 8-step `transpose`,
add scene/section launch with bar quantisation, add per-track active-note bookkeeping with note-off
suppression, and run `demo-style.sty` through `Main A`..`Main D` and `Fill In AA` at fixed chords. Capture
the first goldens (G01–G20 in `compatibility-baseline.md`) from the pinned C++ build, pin the commit and
build configuration, and only then start SFF/CASM import. Doing SFF first would produce a style importer
whose output nobody can check; doing the chord and launch path first gives every later style test an
oracle.

## 4. Stage 5 progress (2026-11-22)

| Deliverable | Location | Proof | Status |
| --- | --- | --- | --- |
| Track provenance, intent and notes digests | `crates/project/src/provenance.rs` | `fresh_edited_stale_and_both_are_distinguished`, `intent_outside_the_range_does_not_make_a_track_stale`, `note_digest_ignores_order_and_ids` | verified |
| Schema v2 and migration | `crates/project/src/song.rs` | `a_version_1_document_migrates_to_version_2` | verified |
| Dual capture of a performance | `crates/performance/src/lib.rs` | `a_section_is_recorded_as_intent_plus_rendered_tracks` | verified |
| Regeneration policies | `crates/performance` | `policy_keep_changes_nothing_and_reports_stale_tracks`, `policy_replace_recompiles_and_refreshes`, `policy_duplicate_adds_fresh_copies_and_keeps_the_old_tracks`, `a_hand_edit_is_visible_and_replaced_only_by_request` | verified |
| Freeze / detach | `crates/performance::freeze` | `freezing_a_track_makes_it_manual_and_immune` | verified |

Live microphone or keyboard capture against a device remains manual. The capture
here records the arranger's compiled output, which is the rendered half.

## 5. Stage 6 progress (2026-11-22)

| Deliverable | Location | Proof |
| --- | --- | --- |
| egui decision and v1 visual language | `docs/adr/ADR-001`, `docs/design/v1-visual-language.md` | tokens extracted from `resources/themes/dark.json` and `AppLookAndFeel.cpp` |
| Themed shell: header LCDs, track browser, status bar, Perform/Arrange/Mix tabs | `apps/desktop/src/app.rs` | builds on all three OSes; UI itself is manual |
| Read-only piano roll with shared draw/hit geometry | `apps/desktop/src/piano_roll.rs` | `ticks_and_points_invert_each_other`, `notes_grow_with_duration_and_move_with_tick`, `higher_pitches_are_higher_on_screen`, `minimum_note_width_is_visible` |
| Theme as data (palette, metrics, Inter fonts) | `apps/desktop/src/theme.rs` | `palette_matches_the_v1_theme`, `the_clear_colour_is_not_white` |
| Startup policy against first-frame flicker | `docs/adr/ADR-011` | manual cold-start check, not automated |

Editing, playback with audio, and the Perform/Mix views are not started.

## 6. Stage 7 progress (2026-11-22)

| Deliverable | Location | Proof |
| --- | --- | --- |
| Offline renderer and WAV encoder | `crates/audio` | `a_note_produces_sound_where_it_sounds`, `output_never_clips`, `wav_header_is_standard`, `where_the_note_ends_the_tail_decays` |
| `export-audio` CLI | `apps/cli` | rendered a real style section to a 5.36 s stereo WAV; Python `wave` reports 2 channels, 48 kHz, 16-bit |
| Realtime playback device | not started | needs a cpal backend; the voice already exists |
