//! Sprint exit checks: the headless path from a song to a MIDI file.
//!
//! These tests drive the same library functions the CLI uses. They prove the
//! three claims the blueprint makes for the first sprint:
//!
//! 1. a four-bar phrase plays through the scheduler at exact sample positions;
//! 2. block size does not change what is played or when;
//! 3. what the scheduler played is what the exported MIDI file says.

use std::collections::BTreeSet;

use lazy_arranger_cli::{TransportSummary, demo_song, run_transport};
use music_core::{MidiMessage, SamplePosition};
use project::ProjectFile;
use sequencer::{EventBuffer, PreparedPattern, ScheduledEvent, Scheduler};

const LEGACY_V3: &str = include_str!("../../../tests/fixtures/legacy-cpp-v3.arranger");

/// Runs a pattern to the end and returns every scheduled event.
fn schedule(song: &project::song::Song, sample_rate: u32, block: usize) -> Vec<ScheduledEvent> {
    let prepared = PreparedPattern::prepare(song, sample_rate).expect("pattern prepares");
    let end_sample = prepared.end_sample();
    let mut scheduler = Scheduler::new(prepared);
    scheduler.play();
    let mut buffer = EventBuffer::with_capacity(256);
    let mut events = Vec::new();
    let mut blocks = 0usize;
    let limit = usize::try_from(end_sample / block as i64 + 2).expect("limit fits");
    while blocks < limit {
        let report = scheduler.process_block(block, &mut buffer);
        assert_eq!(report.overflowed, 0, "block {blocks} dropped events");
        events.extend_from_slice(buffer.events());
        blocks += 1;
        if report.finished && report.emitted == 0 {
            break;
        }
    }
    events
}

#[test]
fn four_bar_phrase_schedules_at_exact_samples() {
    let song = demo_song().expect("demo song");
    let summary = run_transport(&song, 48_000, 128, false).expect("transport runs");
    assert_eq!(
        summary,
        TransportSummary {
            events: 40,
            blocks: 3002,
            overflowed: 0
        },
        "20 notes, two events each, four bars plus rounding"
    );

    let events = schedule(&song, 48_000, 128);
    assert_eq!(events.len(), 40);

    // 120 bpm in 4/4 at 48 kHz: one quarter is 24000 samples, one bar 96000.
    let on_samples: Vec<SamplePosition> = events
        .iter()
        .filter(|event| event.message.is_note_on())
        .map(|event| event.sample)
        .collect();
    assert!(on_samples.contains(&0));
    assert!(on_samples.contains(&48_000), "bass beat 3 is one second in");
    assert!(
        on_samples.contains(&96_000),
        "bar 2 starts at 96000 samples"
    );
    assert!(
        on_samples.contains(&288_000),
        "bar 4 starts at 288000 samples"
    );

    let off_samples: BTreeSet<SamplePosition> = events
        .iter()
        .filter(|event| !event.message.is_note_on())
        .map(|event| event.sample)
        .collect();
    assert!(off_samples.contains(&384_000), "last chord releases at 8 s");
}

#[test]
fn block_size_never_changes_the_rendered_phrase() {
    let song = demo_song().expect("demo song");
    let reference = schedule(&song, 48_000, 128);
    for block in [1, 17, 64, 137, 256, 480, 1024, 4096] {
        let events = schedule(&song, 48_000, block);
        assert_eq!(events, reference, "block size {block} changed playback");
    }
}

#[test]
fn scheduled_events_match_the_exported_midi_file() {
    let song = demo_song().expect("demo song");
    let events = schedule(&song, 48_000, 128);

    let bytes = smf::encode(&song).expect("encode");
    let (decoded, report) = smf::decode(&bytes).expect("decode");
    assert_eq!(report.notes, 20);
    assert_eq!(decoded.tracks.len(), song.tracks.len());

    // Every scheduled note-on and note-off must exist in the file's tick
    // timeline, at the sample the scheduler placed it.
    let mut scheduled: Vec<(music_core::Tick, bool, u8, u8)> = events
        .iter()
        .map(|event| {
            let on = event.message.is_note_on();
            match event.message {
                MidiMessage::NoteOn { key, channel, .. } => {
                    (event.tick, on, channel.zero_based(), key.as_u8())
                }
                MidiMessage::NoteOff { key, channel, .. } => {
                    (event.tick, on, channel.zero_based(), key.as_u8())
                }
                other => panic!("unexpected scheduled message: {other:?}"),
            }
        })
        .collect();
    scheduled.sort_unstable();

    let mut in_file: Vec<(music_core::Tick, bool, u8, u8)> = Vec::new();
    for track in &decoded.tracks {
        for note in &track.notes {
            in_file.push((
                note.start,
                true,
                note.channel.zero_based(),
                note.pitch.as_u8(),
            ));
            in_file.push((
                note.end(),
                false,
                note.channel.zero_based(),
                note.pitch.as_u8(),
            ));
        }
    }
    in_file.sort_unstable();
    assert_eq!(in_file, scheduled, "file and transport disagree");

    // And the file's ticks convert to the same samples the transport used.
    for event in &events {
        assert_eq!(
            decoded.tempo_map.samples_at(event.tick, 48_000),
            event.sample,
            "tick {} lands at a different sample after a round trip",
            event.tick
        );
    }
}

#[test]
fn legacy_project_imports_renders_and_exports() {
    let imported = project::legacy::import_cpp_project(LEGACY_V3).expect("legacy import");
    assert_eq!(imported.report.notes, 265);
    let song = imported.project.song.clone();

    let events = schedule(&song, 48_000, 128);
    assert_eq!(events.len(), 530, "two events per imported note");
    let summary = run_transport(&song, 48_000, 128, false).expect("transport runs");
    assert_eq!(summary.events, 530);
    assert_eq!(summary.overflowed, 0);

    let bytes = smf::encode(&song).expect("encode");
    let (decoded, report) = smf::decode(&bytes).expect("decode");
    assert_eq!(report.notes, 265);
    assert_eq!(decoded.tracks.len(), song.tracks.len());
    // The imported project round-trips through the current schema too.
    let json = imported.project.to_json_pretty().expect("serialize");
    let reparsed = ProjectFile::from_json(&json).expect("reparse");
    assert_eq!(reparsed.song.tracks.len(), song.tracks.len());
}

#[test]
fn odd_sample_rate_keeps_note_ordering() {
    let song = demo_song().expect("demo song");
    let events = schedule(&song, 44_100, 137);
    assert_eq!(events.len(), 40);
    // Notes never overlap in the phrase, so every note-off precedes the next
    // note-on for the same pitch and channel.
    let mut previous_off: Option<(u8, u8, SamplePosition)> = None;
    for event in &events {
        if let MidiMessage::NoteOn { key, channel, .. } = event.message
            && let Some((channel_before, key_before, off)) = previous_off
            && (channel_before, key_before) == (channel.zero_based(), key.as_u8())
        {
            assert!(
                off <= event.sample,
                "note-off at {off} comes after note-on at {}",
                event.sample
            );
        }
        if let MidiMessage::NoteOff { key, channel, .. } = event.message {
            previous_off = Some((channel.zero_based(), key.as_u8(), event.sample));
        }
    }
}

#[test]
fn a_legacy_import_reports_what_it_deferred() {
    let imported = project::legacy::import_cpp_project(LEGACY_V3).expect("legacy import");
    let text = imported.report.to_text();
    assert!(text.contains("deferred source features"), "{text}");
    for key in ["loop", "scenes", "key", "metronome"] {
        assert!(imported.project.song.extra.contains_key(key), "song.{key}");
    }
    // Mixer, sound selection and routing are track-level keys.
    let track = &imported.project.song.tracks[0];
    for key in ["sound", "mixer", "output", "colour"] {
        assert!(track.extra.contains_key(key), "track.{key}");
    }
}
