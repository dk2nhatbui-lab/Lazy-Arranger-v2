use super::devices::{
    L6Scene, PsrRegistration, decode_l6_scenes, encode_l6_scenes, profile_for_port,
    seed_default_device_profiles,
};
use super::*;

#[test]
fn l6max_folder_import_adds_tracks() {
    let folder = std::env::temp_dir().join("lazy-arranger-shell-l6max");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("dir");
    for name in ["TAKE0.WAV", "TAKE1.WAV", "MASTER.WAV"] {
        let audio = audio::RenderedAudio {
            samples: vec![0.1; 4 * 2],
            sample_rate: 48_000,
        };
        std::fs::write(folder.join(name), audio.to_wav()).expect("take");
    }
    let mut editor = Editor::new(demo_project()).expect("editor");
    let before = editor.song().tracks.len();
    let (imported, problems) = import_l6max_folder(&mut editor, &folder, 0).expect("import");
    assert_eq!(imported, 3);
    assert_eq!(problems, 0);
    assert_eq!(editor.song().tracks.len(), before + 3);
    assert!(
        editor
            .song()
            .tracks
            .iter()
            .any(|track| track.name == "MASTER" && track.muted)
    );
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn stem_export_writes_files_and_a_manifest() {
    let directory = std::env::temp_dir().join("lazy-arranger-shell-stems");
    let _ = std::fs::remove_dir_all(&directory);
    let song = arranging::demo::demo_song();
    let stems = export_stems_folder(&song, &directory).expect("stems");
    assert!(stems > 0);
    let manifest = std::fs::read_to_string(directory.join("stems.txt")).expect("manifest");
    assert_eq!(manifest.lines().count(), stems);
    let wavs = std::fs::read_dir(&directory)
        .expect("dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "wav"))
        .count();
    assert_eq!(wavs, stems);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn wav_export_writes_a_24_bit_file() {
    let song = arranging::demo::demo_song();
    let path = std::env::temp_dir().join("lazy-arranger-export-shell.wav");
    let (seconds, problems) = export_wav_file(&song, &path).expect("wav");
    assert!(seconds > 0.0);
    assert_eq!(problems, 0);
    let bytes = std::fs::read(&path).expect("read");
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 1);
    assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 24);
    let info = audio::wav::info(&bytes).expect("info");
    assert!(info.frames > 0);
    let _ = std::fs::remove_file(path);
}

#[test]
fn l6_scenes_round_trip_as_lines() {
    let mut scenes: [Option<L6Scene>; 4] = [None, None, None, None];
    scenes[1] = Some(L6Scene {
        channels: vec![(100, 64, false, 0), (90, 40, true, 30)],
    });
    let back = decode_l6_scenes(&encode_l6_scenes(&scenes));
    assert_eq!(back[1].as_ref().expect("scene").channels.len(), 2);
    assert_eq!(
        back[1].as_ref().expect("scene").channels[1],
        (90, 40, true, 30)
    );
}

#[test]
fn psr_registrations_round_trip_as_lines() {
    let registration = PsrRegistration {
        channel: 3,
        bank_msb: 63,
        bank_lsb: 1,
        program: 42,
        volume: 99,
        pan: 64,
        reverb: 55,
        chorus: 12,
    };
    assert_eq!(
        PsrRegistration::decode(&registration.encode()),
        Some(registration)
    );
    assert!(PsrRegistration::decode("1,2,3").is_none());
}

#[test]
fn removing_an_instrument_hides_its_editor_and_profile() {
    use crate::instruments::{Installed, Instrument};
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    app.apply_instruments(Installed::all());
    assert!(
        app.device_profiles
            .iter()
            .any(|profile| profile.id == "d05")
    );
    app.d05.open = true;
    app.l6max.mixer_open = true;
    let mut choice = Installed::all();
    choice.set(Instrument::RolandD05, false);
    choice.set(Instrument::ZoomL6max, false);
    app.apply_instruments(choice);
    assert!(!app.d05.open);
    assert!(!app.l6max.mixer_open);
    assert!(
        !app.device_profiles
            .iter()
            .any(|profile| profile.id == "d05")
    );
    assert!(
        app.device_profiles
            .iter()
            .any(|profile| profile.id == "psr-s970")
    );
    assert!(
        app.device_profiles
            .iter()
            .any(|profile| profile.id == "generic-gs")
    );
    // A window flag set some other way still draws nothing.
    app.d05.open = true;
    let _ = context.run(egui::RawInput::default(), |context| {
        app.d05_editor_window(context)
    });
}

#[test]
fn a_port_name_selects_a_profile() {
    let folder = std::env::temp_dir().join("lazy-arranger-port-profiles");
    let _ = std::fs::remove_dir_all(&folder);
    seed_default_device_profiles(&folder);
    let (profiles, _) = midi_devices::profiles::load_directory(&folder).expect("load");
    assert_eq!(
        profile_for_port("Yamaha PSR-S970", &profiles).as_deref(),
        Some("psr-s970")
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn default_device_profiles_are_seeded() {
    let folder = std::env::temp_dir().join("lazy-arranger-seed-profiles");
    let _ = std::fs::remove_dir_all(&folder);
    seed_default_device_profiles(&folder);
    let (profiles, problems) = midi_devices::profiles::load_directory(&folder).expect("load");
    assert_eq!(profiles.len(), 6);
    assert!(problems.is_empty());
    assert!(profiles.iter().any(|profile| profile.id == "xiosynth"));
    assert!(profiles.iter().any(|profile| profile.id == "d05"));
    assert!(profiles.iter().any(|profile| profile.id == "generic-gs"));
    assert!(profiles.iter().any(|profile| profile.id == "generic-xg"));
    assert!(profiles.iter().any(|profile| profile.id == "korg-pa"));
    assert!(profiles.iter().any(|profile| profile.id == "psr-s970"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn track_meters_follow_active_notes() {
    let mut song = project::Song::new("Meter", 120.0);
    let mut track = project::song::Track::new(project::TrackId::new(1).expect("track"), "Bass");
    track.notes.push(project::song::Note {
        id: project::NoteId::new(1).expect("note"),
        start: 0,
        duration: music_core::PPQ,
        pitch: music_core::Pitch::from_name("C2").expect("pitch"),
        velocity: music_core::Velocity::new(100).expect("velocity"),
        channel: music_core::Channel::first(),
    });
    song.tracks.push(track);
    assert!(track_level(&song.tracks[0], 0) > 0.5);
    assert_eq!(track_level(&song.tracks[0], music_core::PPQ), 0.0);
    let mut audio_track =
        project::song::Track::new(project::TrackId::new(2).expect("track"), "Audio");
    audio_track.audio = Some(project::song::AudioRef {
        path: String::from("take.wav"),
        start: 0,
        length: music_core::PPQ,
        offset: 0,
        fade_in: 0,
        fade_out: 0,
        stretch: false,
        gain_mb: 0,
    });
    let mut cache = WaveformCache::new();
    cache.insert(
        audio_track.id,
        (
            String::from("take.wav"),
            vec![audio::waveform::Peak {
                min: -0.5,
                max: 0.5,
            }],
        ),
    );
    assert!(track_meter_level(&audio_track, 0, &cache) > 0.4);
    assert_eq!(
        track_meter_level(&audio_track, music_core::PPQ, &cache),
        0.0
    );
}

#[test]
fn sysex_text_is_wrapped_and_parsed() {
    assert_eq!(
        parse_sysex("7E 7F 09 01").expect("sysex"),
        vec![0xF0, 0x7E, 0x7F, 0x09, 0x01, 0xF7]
    );
    assert!(parse_sysex("F0 F7").is_ok());
    assert!(parse_sysex("F0 nope").is_err());
    assert!(parse_sysex("   ").is_err());
}

#[test]
fn a_loaded_profile_supplies_the_keyboard_program() {
    let mut song = project::Song::new("Profile", 120.0);
    let mut track =
        project::song::Track::new(project::TrackId::new(1).expect("track"), "Grand Piano");
    track.notes.push(project::song::Note {
        id: project::NoteId::new(1).expect("note"),
        start: 0,
        duration: music_core::PPQ,
        pitch: music_core::Pitch::from_name("C4").expect("pitch"),
        velocity: music_core::Velocity::new(100).expect("velocity"),
        channel: music_core::Channel::first(),
    });
    song.tracks.push(track);
    let profile = midi_devices::profiles::LoadedProfile {
        id: String::from("test"),
        name: String::from("Test"),
        voices: vec![(String::from("Grand Piano"), 42)],
        banks: Vec::new(),
        drum_kits: Vec::new(),
        controls: Vec::new(),
    };
    let path = std::env::temp_dir().join("lazy-arranger-profile.mid");
    export_midi_file_with_profile(&song, &path, Some(&profile)).expect("midi");
    let bytes = std::fs::read(&path).expect("read");
    let audit = smf::keyboard::audit(&bytes).expect("audit");
    assert!(audit.programs.contains(&42), "{:?}", audit.programs);
    let _ = std::fs::remove_file(path);
}

#[test]
fn midi_and_musicxml_exports_write_reloadable_files() {
    let song = arranging::demo::demo_song();
    let midi = std::env::temp_dir().join("lazy-arranger-export-shell.mid");
    let tracks = export_midi_file(&song, &midi).expect("midi");
    assert!(tracks > 0);
    let bytes = std::fs::read(&midi).expect("read midi");
    assert_eq!(&bytes[0..4], b"MThd");
    let audit = smf::keyboard::audit(&bytes).expect("audit");
    assert!(audit.tracks > 0);
    let xml = std::env::temp_dir().join("lazy-arranger-export-shell.musicxml");
    export_musicxml_file(&song, &xml, "Shell").expect("xml");
    let text = std::fs::read_to_string(&xml).expect("read xml");
    let imported = musicxml::import::import(&text).expect("import xml");
    assert_eq!(imported.tracks.len(), song.tracks.len());
    let _ = std::fs::remove_file(midi);
    let _ = std::fs::remove_file(xml);
}

#[test]
fn a_musicxml_file_loads_as_a_project() {
    let song = arranging::demo::demo_song();
    let xml = musicxml::export(&song, "Round trip");
    let path = std::env::temp_dir().join("lazy-arranger-desktop-import.musicxml");
    std::fs::write(&path, xml).expect("write");
    assert_eq!(project_kind(&path), ProjectKind::MusicXml);
    let loaded = load_project(&path).expect("load");
    assert_eq!(loaded.song.tracks.len(), song.tracks.len());
    assert!(
        loaded
            .song
            .tracks
            .iter()
            .any(|track| !track.notes.is_empty())
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_midi_file_loads_as_an_untitled_project() {
    let song = arranging::demo::demo_song();
    let bytes = smf::encode(&song).expect("encode");
    let path = std::env::temp_dir().join("lazy-arranger-desktop-import.mid");
    std::fs::write(&path, bytes).expect("write");
    assert!(is_midi_path(&path));
    let loaded = load_project(&path).expect("load");
    assert_eq!(loaded.song.tracks.len(), song.tracks.len());
    assert!(
        loaded
            .song
            .tracks
            .iter()
            .any(|track| !track.notes.is_empty())
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn theory_summary_reports_key_and_cadence() {
    let song = arranging::demo::demo_song();
    let lines = theory_summary(&song);
    assert!(lines[0].starts_with("Key: "), "{:?}", lines[0]);
    assert!(lines.iter().any(|line| line.starts_with("Chords: ")));
    assert!(lines.iter().any(|line| line.starts_with("Cadence:")));
    let empty = project::song::Song::new("Empty", 120.0);
    assert_eq!(theory_summary(&empty)[2], "No chords yet");
}

#[test]
fn recorded_starts_snap_to_the_grid() {
    use music_core::{Channel, Pitch, Velocity};
    use project::NoteId;
    let mut notes = vec![
        project::song::Note {
            id: NoteId::new(1).expect("valid"),
            start: 100,
            duration: 120,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        },
        project::song::Note {
            id: NoteId::new(2).expect("valid"),
            start: 940,
            duration: 120,
            pitch: Pitch::from_name("E4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        },
    ];
    quantise_recorded(&mut notes, music_core::PPQ / 4);
    assert_eq!(notes[0].start, 0);
    assert_eq!(notes[1].start, 960);
}

#[test]
fn style_section_names_choose_fill_and_ending_roles() {
    assert_eq!(style_section_role("Main A"), StyleSectionRole::Main);
    assert_eq!(style_section_role("Fill In AA"), StyleSectionRole::Fill);
    assert_eq!(style_section_role("Ending A"), StyleSectionRole::Ending);
    assert_eq!(style_section_role("Intro"), StyleSectionRole::Main);
}

#[test]
fn held_midi_notes_name_a_chord() {
    let mut held = [false; 128];
    held[60] = true;
    held[64] = true;
    held[67] = true;
    assert_eq!(held_chord_name(&held).as_deref(), Some("C"));
    held[60] = false;
    held[57] = true;
    held[64] = true;
    held[67] = false;
    held[60] = true;
    assert_eq!(held_chord_name(&held).as_deref(), Some("Am"));
    held[60] = false;
    held[57] = false;
    assert!(held_chord_name(&held).is_none());
}

#[test]
fn style_section_queue_lands_on_the_next_bar() {
    assert_eq!(next_bar_seconds(0.0, 2.0), 2.0);
    assert_eq!(next_bar_seconds(1.9, 2.0), 2.0);
    assert_eq!(next_bar_seconds(2.0, 2.0), 4.0);
    assert_eq!(next_bar_seconds(5.2, 2.0), 6.0);
}

#[test]
fn a_style_section_song_contains_compiled_notes() {
    let song = arranging::demo::demo_song();
    let native = arranger::style::from_song(
        &song,
        "Demo",
        arranger::ArrangerChord::new(music_core::PitchClass::new(0), arranger::ChordType::Major),
        music_core::PPQ * 4,
    );
    let imported = style_formats::ImportedStyle {
        native,
        source_ppq: 960,
        tempo_bpm: 120.0,
        time_signature: song.time_signature,
        song,
        sections: Vec::new(),
        stats: style_formats::ImportStats::default(),
    };
    let preview = style_section_song(&imported, 0, "C").expect("section");
    assert_eq!(preview.tracks.len(), 1);
    assert!(!preview.tracks[0].notes.is_empty());
}

#[test]
fn a_style_audition_song_contains_compiled_notes() {
    let mut song = project::Song::new("Source", 120.0);
    let mut track = project::song::Track::new(project::TrackId::new(1).expect("track"), "Bass");
    track.notes.push(project::song::Note {
        id: project::NoteId::new(1).expect("note"),
        start: 0,
        duration: music_core::PPQ,
        pitch: music_core::Pitch::from_name("C2").expect("pitch"),
        velocity: music_core::Velocity::new(100).expect("velocity"),
        channel: music_core::Channel::from_one_based(11).expect("channel"),
    });
    song.tracks.push(track);
    let style = arranger::style::from_song(
        &song,
        "Source",
        arranger::chord_type::ArrangerChord::new(
            music_core::PitchClass::new(0),
            arranger::chord_type::ChordType::Major,
        ),
        music_core::PPQ,
    );
    let preview = style_preview_song(&style).expect("preview");
    assert_eq!(preview.tracks.len(), 1);
    assert_eq!(preview.tracks[0].notes.len(), 1);
}

#[test]
fn count_in_audio_has_one_accented_click_per_bar() {
    let audio = count_in_audio(120.0, 1, music_core::TimeSignature::default());
    assert_eq!(audio.sample_rate, 48_000);
    assert_eq!(audio.samples.len(), 96_000 * 2);
    let frames: Vec<f32> = audio.samples.chunks(2).map(|frame| frame[0]).collect();
    let peaks: Vec<usize> = frames
        .windows(480)
        .enumerate()
        .filter(|(_, window)| window.iter().any(|sample| sample.abs() > 0.25))
        .map(|(index, _)| index)
        .take(8)
        .collect();
    assert!(!peaks.is_empty());
    assert!(peaks[0] < 480, "the first click starts the count-in");
}

#[test]
fn an_audio_track_transcribes_into_new_tracks() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let rate = 48_000_u32;
    let mut samples = vec![0.0_f32; rate as usize];
    for (index, sample) in samples.iter_mut().enumerate() {
        let time = index as f32 / rate as f32;
        *sample = (std::f32::consts::TAU * 440.0 * time).sin() * 0.5;
    }
    let path = std::env::temp_dir().join(format!(
        "lazy-arranger-transcribe-{}.wav",
        std::process::id()
    ));
    std::fs::write(
        &path,
        audio::RenderedAudio {
            samples,
            sample_rate: rate,
        }
        .to_wav(),
    )
    .expect("wav");
    let id = project::TrackId::new(90).expect("track");
    let mut track = project::song::Track::new(id, "Transcribe");
    track.audio = Some(project::song::AudioRef {
        path: path.display().to_string(),
        start: 0,
        length: 960,
        offset: 0,
        fade_in: 0,
        fade_out: 0,
        stretch: false,
        gain_mb: 0,
    });
    app.editor
        .session_for_test()
        .apply(app_commands::Command::InsertTrack { track })
        .expect("track");
    let before = app.song().tracks.len();
    app.transcribe_audio_track(id).expect("transcribe");
    assert_eq!(app.song().tracks.len(), before + 1);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_audio_track_transcribes_drums_to_channel_nine() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let rate = 48_000_u32;
    let mut samples = vec![0.0_f32; rate as usize];
    let burst = (0.08 * f64::from(rate)) as usize;
    let start = (0.2 * f64::from(rate)) as usize;
    for index in 0..burst {
        let time = index as f64 / f64::from(rate);
        samples[start + index] = (std::f64::consts::TAU * 60.0 * time).sin() as f32 * 0.8;
    }
    let path = std::env::temp_dir().join(format!("lazy-arranger-drums-{}.wav", std::process::id()));
    std::fs::write(
        &path,
        audio::RenderedAudio {
            samples,
            sample_rate: rate,
        }
        .to_wav(),
    )
    .expect("wav");
    let id = project::TrackId::new(91).expect("track");
    let mut track = project::song::Track::new(id, "Drums source");
    track.audio = Some(project::song::AudioRef {
        path: path.display().to_string(),
        start: 0,
        length: 960,
        offset: 0,
        fade_in: 0,
        fade_out: 0,
        stretch: false,
        gain_mb: 0,
    });
    app.editor
        .session_for_test()
        .apply(app_commands::Command::InsertTrack { track })
        .expect("track");
    app.transcribe_audio_drums(id).expect("drums");
    let recorded = app
        .song()
        .tracks
        .iter()
        .find(|track| track.name == "Drums source Drums")
        .expect("drums track");
    assert!(
        recorded
            .notes
            .iter()
            .all(|note| note.channel.zero_based() == 9)
    );
    assert!(!recorded.notes.is_empty());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn waveform_fraction_maps_the_visible_range() {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 20.0));
    assert_eq!(ShellApp::waveform_fraction(rect, 50.0, 0, 100, 200), 0.25);
    assert_eq!(ShellApp::waveform_fraction(rect, 100.0, 50, 100, 100), 1.0);
}

#[test]
fn a_playlist_midi_opens_as_a_project() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let song = demo_project().song;
    let bytes = smf::encode(&song).expect("midi");
    let path =
        std::env::temp_dir().join(format!("lazy-arranger-playlist-{}.mid", std::process::id()));
    std::fs::write(&path, bytes).expect("write");
    app.open_library_path(&path).expect("open");
    assert_eq!(app.song().tracks.len(), song.tracks.len());
    assert_eq!(app.project_path.as_deref(), Some(path.as_path()));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn window_title_shows_song_and_dirty_state() {
    assert_eq!(
        ShellApp::window_title("Demo", false),
        "Demo — Lazy Arranger Midi DAW"
    );
    assert_eq!(
        ShellApp::window_title("Demo", true),
        "Demo* — Lazy Arranger Midi DAW"
    );
    assert_eq!(ShellApp::window_title("", true), "Lazy Arranger Midi DAW*");
}

#[test]
fn view_names_round_trip_through_settings() {
    for view in [
        View::Perform,
        View::Arrange,
        View::Drums,
        View::Loops,
        View::Score,
        View::Map,
        View::Mix,
    ] {
        assert_eq!(View::parse(view.as_str()), Some(view), "{view:?}");
    }
}

#[test]
fn recent_projects_load_from_the_settings_folder() {
    let context = egui::Context::default();
    let app = ShellApp::new(&context, demo_project(), None);
    assert_eq!(
        app.recent_file,
        crate::settings::Settings::path().with_file_name("recent-projects.txt")
    );
}

#[test]
fn playlist_paths_resolve_against_the_book_folder() {
    let base = std::path::Path::new("/Music/books/list.tsv");
    assert_eq!(
        resolve_playlist_path(base, "song.mid"),
        std::path::PathBuf::from("/Music/books/song.mid")
    );
    assert_eq!(
        resolve_playlist_path(base, "/tmp/song.mid"),
        std::path::PathBuf::from("/tmp/song.mid")
    );
}

#[test]
fn an_audio_track_transcribes_chords() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let rate = 48_000_u32;
    let mut samples = vec![0.0_f32; rate as usize];
    for (index, sample) in samples.iter_mut().enumerate() {
        let time = index as f32 / rate as f32;
        *sample = [261.63_f32, 329.63, 392.0]
            .iter()
            .map(|hz| (std::f32::consts::TAU * hz * time).sin())
            .sum::<f32>()
            / 3.0;
    }
    let path =
        std::env::temp_dir().join(format!("lazy-arranger-chords-{}.wav", std::process::id()));
    std::fs::write(
        &path,
        audio::RenderedAudio {
            samples,
            sample_rate: rate,
        }
        .to_wav32f(),
    )
    .expect("wav");
    let id = project::TrackId::new(92).expect("track");
    let mut track = project::song::Track::new(id, "Chords source");
    track.audio = Some(project::song::AudioRef {
        path: path.display().to_string(),
        start: 0,
        length: 960,
        offset: 0,
        fade_in: 0,
        fade_out: 0,
        stretch: false,
        gain_mb: 0,
    });
    app.editor
        .session_for_test()
        .apply(app_commands::Command::InsertTrack { track })
        .expect("track");
    let found = app.transcribe_audio_chords(id).expect("chords");
    assert!(found >= 1);
    assert!(!app.song().chords.is_empty());
    assert_eq!(app.song().chords[0].chord.root.as_u8(), 0);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn detected_notes_convert_to_ticks_and_pitches() {
    let detected = vec![
        audio::analysis::DetectedNote {
            start: 0.0,
            duration: 0.5,
            pitch: 69,
            level: 0.5,
        },
        audio::analysis::DetectedNote {
            start: 0.5,
            duration: 0.5,
            pitch: 72,
            level: 0.25,
        },
    ];
    let notes = detected_to_notes(&detected, 120.0, music_core::Channel::first()).expect("notes");
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].pitch.as_u8(), 69);
    assert_eq!(notes[0].start, 0);
    assert_eq!(notes[0].duration, music_core::PPQ);
    assert_eq!(notes[1].pitch.as_u8(), 72);
    assert_eq!(notes[1].start, music_core::PPQ);
}

#[test]
fn count_in_seconds_follow_the_meter() {
    assert!((count_in_seconds(120.0, 1, music_core::TimeSignature::default()) - 2.0).abs() < 1e-9);
    let six_eight = music_core::TimeSignature::new(6, 8).expect("meter");
    assert!((count_in_seconds(120.0, 1, six_eight) - 1.5).abs() < 1e-9);
}

#[test]
fn count_in_follows_the_meter() {
    let six_eight = music_core::TimeSignature::new(6, 8).expect("meter");
    let audio = count_in_audio(120.0, 1, six_eight);
    assert_eq!(audio.samples.len(), 72_000 * 2);
}

#[test]
fn plugin_filter_checks_name_path_and_format() {
    use plugins::{Format, FoundPlugin};
    let plugin = FoundPlugin {
        name: String::from("R0Verb"),
        vendor: String::from("Klevgrand"),
        version: String::from("1.0.4"),
        identifier: String::from("Klev:rvrb"),
        role: plugins::Role::Effect,
        path: std::path::PathBuf::from("/Library/Audio/Plug-Ins/Components/R0Verb.component"),
        format: Format::AudioUnit,
    };
    assert!(plugin_matches(&plugin, "", None));
    assert!(plugin_matches(&plugin, "r0v", None));
    assert!(plugin_matches(&plugin, "components", None));
    assert!(!plugin_matches(&plugin, "LABS", None));
    assert!(plugin_matches(&plugin, "", Some(Format::AudioUnit)));
    assert!(!plugin_matches(&plugin, "", Some(Format::Vst3)));
}

#[test]
fn sysex_hex_round_trips() {
    let bytes = parse_sysex("F0 7E 7F 09 01 F7").expect("bytes");
    assert_eq!(sysex_hex(&bytes), "F0 7E 7F 09 01 F7");
    assert_eq!(parse_sysex(&sysex_hex(&bytes)).expect("back"), bytes);
}

#[test]
fn midi_messages_have_compact_labels() {
    use music_core::{Channel, ControlNumber, MidiMessage, Pitch, Velocity};
    assert_eq!(
        midi_label(&MidiMessage::NoteOn {
            channel: Channel::first(),
            key: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
        }),
        "ch1 note-on C4 vel 100"
    );
    assert_eq!(
        midi_label(&MidiMessage::ControlChange {
            channel: Channel::first(),
            controller: ControlNumber::new(7).expect("valid"),
            value: 64,
        }),
        "ch1 CC 7 = 64"
    );
}

#[test]
fn print_arguments_include_printer_copies_and_pages() {
    let path = std::path::Path::new("/tmp/score.pdf");
    assert_eq!(
        print_arguments(Some("Office"), 2, "1-4, 7", path),
        vec![
            "-d",
            "Office",
            "-n",
            "2",
            "-o",
            "page-ranges=1-4, 7",
            "/tmp/score.pdf"
        ]
    );
    assert_eq!(print_arguments(None, 1, "", path), vec!["/tmp/score.pdf"]);
}

#[test]
fn a_save_path_without_extension_gets_json() {
    assert_eq!(
        normalize_save_path(std::path::PathBuf::from("song")),
        std::path::PathBuf::from("song.json")
    );
    assert_eq!(
        normalize_save_path(std::path::PathBuf::from("song.arranger")),
        std::path::PathBuf::from("song.arranger")
    );
    assert_eq!(
        normalize_save_path(std::path::PathBuf::from("")),
        std::path::PathBuf::from("")
    );
}

#[test]
fn clip_clicks_toggle_a_multi_selection() {
    use project::{ClipId, TrackId};
    let first = (
        TrackId::new(1).expect("valid"),
        ClipId::new(1).expect("valid"),
    );
    let second = (
        TrackId::new(1).expect("valid"),
        ClipId::new(2).expect("valid"),
    );
    let mut selection = Vec::new();
    assert_eq!(apply_clip_click(&mut selection, first, false), Some(first));
    assert_eq!(selection.len(), 1);
    assert_eq!(apply_clip_click(&mut selection, second, true), Some(second));
    assert_eq!(selection.len(), 2);
    assert_eq!(apply_clip_click(&mut selection, first, true), Some(second));
    assert_eq!(selection, vec![second]);
    assert_eq!(apply_clip_click(&mut selection, first, false), Some(first));
    assert_eq!(selection, vec![first]);
}

#[test]
fn plurals_read_naturally() {
    assert_eq!(plural(0, "note"), "0 notes");
    assert_eq!(plural(1, "note"), "1 note");
    assert_eq!(plural(2, "clip"), "2 clips");
    assert_eq!(plural(1, "automation lane"), "1 automation lane");
}

#[test]
fn named_songs_validate() {
    for song in [
        arranging::demo::demo_song(),
        arranging::demo::kids_song(),
        project::song::Song::new("Untitled", 120.0),
    ] {
        let project = ProjectFile::new(song);
        assert!(
            project
                .validate()
                .iter()
                .all(|issue| issue.severity != project::Severity::Error)
        );
    }
}

#[test]
fn the_audio_engine_label_is_accurate() {
    let label = audio_engine_label();
    assert!(label.contains("48 kHz"), "{label}");
    assert!(label.contains("cpal"), "{label}");
    assert!(!label.contains("no audio engine"));
}

#[test]
fn lpstat_output_parses_to_printer_names() {
    assert_eq!(
        printers_from_lpstat(
            "printer Alpha is idle.  enabled since today\nprinter Beta_2 is idle.\n"
        ),
        vec!["Alpha", "Beta_2"]
    );
    assert!(printers_from_lpstat("no printers").is_empty());
}

#[test]
fn dotted_entry_lengths_are_three_halves() {
    assert_eq!(entry_length(music_core::PPQ, false), music_core::PPQ);
    assert_eq!(entry_length(music_core::PPQ, true), music_core::PPQ * 3 / 2);
}

#[test]
fn score_typing_needs_the_score_view_and_focus() {
    assert!(score_typing(View::Score, true, false));
    assert!(!score_typing(View::Score, true, true));
    assert!(!score_typing(View::Score, false, false));
    assert!(!score_typing(View::Arrange, true, false));
}

#[test]
fn score_keys_map_to_pitches() {
    assert_eq!(score_key_pitch(egui::Key::A, 4), Some(60));
    assert_eq!(score_key_pitch(egui::Key::W, 4), Some(61));
    assert_eq!(score_key_pitch(egui::Key::J, 4), Some(71));
    assert_eq!(score_key_pitch(egui::Key::A, 5), Some(72));
    assert_eq!(score_key_pitch(egui::Key::Z, 4), None);
}

#[test]
fn view_names_parse_for_the_developer_override() {
    assert_eq!(View::parse("score"), Some(View::Score));
    assert_eq!(View::parse("MIX"), Some(View::Mix));
    assert_eq!(View::parse("arrange"), Some(View::Arrange));
    assert_eq!(View::parse("nope"), None);
}

#[test]
fn a_failed_startup_open_clears_the_path() {
    let path = std::env::temp_dir().join(format!(
        "lazy-arranger-broken-load-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, b"not a project").expect("write");
    let (project, loaded_path, error) = load_startup_project(Some(&path));
    let _ = std::fs::remove_file(&path);
    assert_eq!(project.song.name, demo_project().song.name);
    assert!(loaded_path.is_none());
    assert!(error.is_some_and(|message| message.contains("could not open")));
}

#[test]
fn the_tool_windows_draw_without_panicking() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let audio_id = project::TrackId::new(99).expect("track");
    let mut audio = project::song::Track::new(audio_id, "Audio Takes");
    audio.audio = Some(project::song::AudioRef {
        path: String::from("take.wav"),
        start: 0,
        length: music_core::PPQ,
        offset: 0,
        fade_in: 0,
        fade_out: 0,
        stretch: false,
        gain_mb: 0,
    });
    if let Some(take) = audio.audio.clone() {
        audio.takes.push(take);
        audio.comps.push(project::song::AudioComp {
            take: 0,
            start: 0,
            length: music_core::PPQ / 2,
        });
    }
    app.editor
        .session_for_test()
        .apply(app_commands::Command::InsertTrack { track: audio })
        .expect("audio track");
    app.roll.selected = Some(audio_id);
    app.file_mode = Some(FileMode::Open);
    app.file_path_input = String::from("");
    app.print_dialog = true;
    app.about_open = true;
    app.assistant.open = true;
    app.audio_chords.open = true;
    app.audio_style.open = true;
    app.bus_fx_open = true;
    app.controller_open = true;
    app.device_editor_open = true;
    app.generate_open = true;
    app.groove_open = true;
    app.help_open = true;
    app.inspector_open = true;
    app.l6max.mixer_open = true;
    app.library_open = true;
    app.lyrics_open = true;
    app.markers_open = true;
    app.master_fx_open = true;
    app.midi_map_open = true;
    app.midi_monitor_open = true;
    app.midi_out.open = true;
    app.pads_open = true;
    app.plugins_open = true;
    app.psr.open = true;
    app.songs_open = true;
    app.lessons_open = true;
    app.theory_open = true;
    app.style.clips_open = true;
    app.style.file_open = true;
    app.style.link_open = true;
    app.l6max.device_open = true;
    app.d05.open = true;
    app.marks_open = true;
    app.takes_open = true;
    app.xg.open = true;
    app.instruments = crate::instruments::Installed::all();
    app.instruments_open = true;
    let _ = context.run(egui::RawInput::default(), |context| {
        app.about_window(context);
        app.assistant_window(context);
        app.audio_chords_window(context);
        app.audio_style_window(context);
        app.bus_fx_window(context);
        app.controller_window(context);
        app.device_editor_window(context);
        app.file_window(context);
        app.generate_window(context);
        app.groove_window(context);
        app.help_window(context);
        app.inspector_window(context);
        app.l6max_mixer_window(context);
        app.library_window(context);
        app.lyrics_window(context);
        app.markers_window(context);
        app.master_fx_window(context);
        app.midi_map_window(context);
        app.midi_monitor_window(context);
        app.midi_out_window(context);
        app.pads_window(context);
        app.plugins_window(context);
        app.print_window(context);
        app.psr_window(context);
        app.songs_window(context);
        app.lessons_window(context);
        app.theory_window(context);
        app.style_clips_window(context);
        app.style_file_window(context);
        app.style_link_window(context);
        app.l6max_device_window(context);
        app.d05_editor_window(context);
        app.marks_window(context);
        app.takes_window(context);
        app.xg_window(context);
        app.instruments_window(context);
    });
}

#[test]
fn kids_mode_offers_two_views_and_languages_change_labels() {
    assert_eq!(View::allowed(true), vec![View::Perform, View::Arrange]);
    assert_eq!(View::allowed(false).len(), 5);
    let danish = crate::strings::Language::parse("da");
    assert_eq!(crate::strings::text(danish, View::Score.key()), "Partitur");
    assert_eq!(crate::strings::text(danish, View::Perform.key()), "Spil");
}

#[test]
fn playhead_ticks_stay_accurate_at_44100_hz() {
    // 120 BPM = 2 beats per second = 1920 ticks per second at PPQ 960.
    let tempo_map = music_core::TempoMap::constant(120.0);
    let playback = audio::player::Playback::new(audio::RenderedAudio {
        samples: vec![0.0_f32; 44_100 * 4 * 2],
        sample_rate: 44_100,
    });
    // Advance 2 full seconds at 44.1 kHz (88,200 frames = 1 bar of 4/4 at 120 BPM).
    playback.seek_frame(88_200);
    assert!((playback.seconds() - 2.0).abs() < 1.0e-9);
    assert_eq!(tempo_map.tick_at_seconds(playback.seconds()), 3840);
}

fn menu_items(menus: &[menu::Menu]) -> Vec<(menu::MenuCommand, String)> {
    fn walk(entries: &[menu::MenuEntry], out: &mut Vec<(menu::MenuCommand, String)>) {
        for entry in entries {
            match entry {
                menu::MenuEntry::Item { command, label, .. } => out.push((*command, label.clone())),
                menu::MenuEntry::Submenu { entries, .. } => walk(entries, out),
                menu::MenuEntry::Separator => {}
            }
        }
    }
    let mut out = Vec::new();
    for menu in menus {
        walk(&menu.entries, &mut out);
    }
    out
}

#[test]
fn the_menu_bar_has_the_usual_menus() {
    let context = egui::Context::default();
    let app = ShellApp::new(&context, demo_project(), None);
    let titles: Vec<&str> = app
        .menu_model(false)
        .iter()
        .map(|menu| menu.title)
        .collect();
    assert_eq!(
        titles,
        [
            "File",
            "Edit",
            "Track",
            "Style",
            "Transport",
            "View",
            "AI",
            "Window",
            "Help"
        ]
    );
    let drawn = menu_items(&app.menu_model(false));
    assert!(
        drawn
            .iter()
            .any(|(command, _)| *command == menu::MenuCommand::Quit)
    );
    // The macOS app menu holds Settings, About and Quit instead.
    let native = menu_items(&app.menu_model(true));
    for command in [
        menu::MenuCommand::Quit,
        menu::MenuCommand::Show(menu::Panel::Settings),
        menu::MenuCommand::Show(menu::Panel::About),
    ] {
        assert!(!native.iter().any(|(other, _)| *other == command));
    }
}

#[test]
fn every_window_in_the_window_menu_opens() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    for group in menu::WINDOW_GROUPS {
        for (panel, label) in *group {
            app.open_panel(*panel);
            assert!(!label.is_empty());
        }
    }
    assert!(app.inspector_open && app.library_open && app.theory_open && app.groove_open);
    assert!(app.l6max.mixer_open && app.d05.open && app.psr.open && app.style.file_open);
}

#[test]
fn the_menus_hide_instruments_that_are_not_installed() {
    use crate::instruments::{Installed, Instrument};
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let shows = |app: &ShellApp, command: menu::MenuCommand| {
        menu_items(&app.menu_model(false))
            .iter()
            .any(|(other, _)| *other == command)
    };
    app.instruments = Installed::all();
    assert!(shows(&app, menu::MenuCommand::Show(menu::Panel::RolandD05)));
    assert!(shows(&app, menu::MenuCommand::ImportL6max));
    let mut choice = Installed::all();
    choice.set(Instrument::RolandD05, false);
    choice.set(Instrument::ZoomL6max, false);
    app.instruments = choice;
    assert!(!shows(
        &app,
        menu::MenuCommand::Show(menu::Panel::RolandD05)
    ));
    assert!(!shows(
        &app,
        menu::MenuCommand::Show(menu::Panel::L6maxMixer)
    ));
    assert!(!shows(&app, menu::MenuCommand::ImportL6max));
    assert!(shows(&app, menu::MenuCommand::Show(menu::Panel::PsrRemote)));
    assert!(shows(
        &app,
        menu::MenuCommand::Show(menu::Panel::InstrumentEditors)
    ));
}

#[test]
fn menu_commands_change_the_app() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let looped = app.loop_enabled;
    app.run_menu_command(&context, menu::MenuCommand::Loop);
    assert_ne!(app.loop_enabled, looped);
    assert!(app.loop_region.is_some());

    app.settings.kids_mode = false;
    app.run_menu_command(&context, menu::MenuCommand::ShowView(View::Score));
    assert_eq!(app.view, View::Score);
    app.run_menu_command(&context, menu::MenuCommand::KidsMode);
    assert_eq!(app.view, View::Perform);
    // Kids Mode does not offer the score.
    app.run_menu_command(&context, menu::MenuCommand::ShowView(View::Score));
    assert_eq!(app.view, View::Perform);
    app.run_menu_command(&context, menu::MenuCommand::KidsMode);
    assert!(!app.settings.kids_mode);
}

#[test]
fn edit_menu_items_send_their_key_press() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    app.run_menu_command(&context, menu::MenuCommand::Copy);
    let mut raw = egui::RawInput::default();
    app.feed_menu_input(&mut raw);
    assert!(raw.events.iter().any(|event| matches!(
        event,
        egui::Event::Key {
            key: egui::Key::C,
            pressed: true,
            modifiers,
            ..
        } if modifiers.command
    )));
    assert!(raw.events.contains(&egui::Event::Copy));
    // The queue empties once fed.
    let mut again = egui::RawInput::default();
    app.feed_menu_input(&mut again);
    assert!(again.events.is_empty());
}

#[test]
fn undo_is_greyed_out_until_there_is_something_to_undo() {
    let context = egui::Context::default();
    let app = ShellApp::new(&context, demo_project(), None);
    let menus = app.menu_model(false);
    let edit = menus
        .iter()
        .find(|menu| menu.title == "Edit")
        .expect("edit");
    let undo = edit.entries.iter().find_map(|entry| match entry {
        menu::MenuEntry::Item {
            command: menu::MenuCommand::Undo,
            enabled,
            ..
        } => Some(*enabled),
        _ => None,
    });
    assert_eq!(undo, Some(app.editor.undo_depth() > 0));
}

#[test]
fn a_cpp_app_project_opens_as_an_import() {
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/legacy-cpp-v3.arranger");
    assert_eq!(project_kind(&fixture), ProjectKind::Json);
    assert_eq!(file_kind(&fixture), ProjectKind::CppProject);
    let project = load_project(&fixture).expect("the C++ project imports");
    assert!(!project.song.tracks.is_empty());
    // Opening it must not let Save write over the C++ file.
    let (_, path, error) = load_startup_project(Some(&fixture));
    assert!(error.is_none());
    assert!(path.is_none());
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    app.open(&fixture);
    assert!(app.load_error.is_none());
    assert!(app.project_path.is_none());
}

#[test]
fn a_cpp_audio_project_opens_with_its_stems() {
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/legacy-cpp-v4.arranger");
    let project = load_project(&fixture).expect("the C++ audio project imports");
    let folder = fixture
        .parent()
        .expect("folder")
        .canonicalize()
        .expect("folder");
    for track in &project.song.tracks {
        let audio = track
            .audio
            .as_ref()
            .expect("each stem track keeps its audio");
        let path = std::path::Path::new(&audio.path);
        assert!(path.is_absolute(), "{}", audio.path);
        assert!(path.starts_with(&folder), "{}", audio.path);
    }
}

fn first_clip(app: &ShellApp) -> (TrackId, project::ClipId, project::song::Clip) {
    app.song()
        .tracks
        .iter()
        .find_map(|track| {
            track
                .clips
                .first()
                .map(|clip| (track.id, clip.id, clip.clone()))
        })
        .expect("the demo has a clip")
}

fn notes_between(app: &ShellApp, track: TrackId, from: Tick, to: Tick) -> usize {
    app.song()
        .tracks
        .iter()
        .find(|entry| entry.id == track)
        .map_or(0, |entry| {
            entry
                .notes
                .iter()
                .filter(|note| note.start >= from && note.start < to)
                .count()
        })
}

#[test]
fn timeline_menu_duplicates_a_clip_with_its_notes() {
    use super::timeline_menu::TimelineAction;
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let (track, clip, entry) = first_clip(&app);
    // Make room after the clip.
    let later: Vec<project::ClipId> = app
        .song()
        .tracks
        .iter()
        .find(|other| other.id == track)
        .map(|other| {
            other
                .clips
                .iter()
                .filter(|other| other.id != clip)
                .map(|other| other.id)
                .collect()
        })
        .unwrap_or_default();
    for other in later {
        app.run_timeline_action(TimelineAction::DeleteClip(track, other));
    }
    let inside = notes_between(&app, track, entry.start, entry.end());
    app.run_timeline_action(TimelineAction::DuplicateClip(track, clip));
    let copies = notes_between(&app, track, entry.end(), entry.end() + entry.length);
    assert_eq!(copies, inside, "{:?}", app.status);
    let clips = app
        .song()
        .tracks
        .iter()
        .find(|other| other.id == track)
        .map_or(0, |other| other.clips.len());
    assert_eq!(clips, 2);
}

#[test]
fn timeline_menu_splits_and_deletes_clips() {
    use super::timeline_menu::TimelineAction;
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let (track, clip, entry) = first_clip(&app);
    let count = |app: &ShellApp| {
        app.song()
            .tracks
            .iter()
            .find(|other| other.id == track)
            .map_or(0, |other| other.clips.len())
    };
    let before = count(&app);
    let middle = entry.start + entry.length / 2;
    app.run_timeline_action(TimelineAction::SplitClip(track, clip, middle));
    assert_eq!(count(&app), before + 1, "{:?}", app.status);
    app.run_timeline_action(TimelineAction::DeleteClip(track, clip));
    assert_eq!(count(&app), before);
    assert_eq!(notes_between(&app, track, entry.start, middle), 0);
}

#[test]
fn track_menu_moves_and_deletes_tracks() {
    use super::timeline_menu::TimelineAction;
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let first = app.song().tracks[0].id;
    let tracks = app.song().tracks.len();
    app.run_timeline_action(TimelineAction::MoveTrack(first, false));
    assert_eq!(app.song().tracks[1].id, first);
    app.run_timeline_action(TimelineAction::DuplicateTrack(first));
    assert_eq!(app.song().tracks.len(), tracks + 1);
    app.run_timeline_action(TimelineAction::DeleteTrack(first));
    assert_eq!(app.song().tracks.len(), tracks);
    assert!(app.song().tracks.iter().all(|track| track.id != first));
}

#[test]
fn track_menu_works_on_the_selected_track() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let tracks = app.song().tracks.len();
    app.roll.selected = None;
    // With no track selected, only Add Track does anything.
    let track_menu = |app: &ShellApp| {
        app.menu_model(false)
            .into_iter()
            .find(|menu| menu.title == "Track")
            .map(|menu| menu.entries)
            .unwrap_or_default()
    };
    assert!(track_menu(&app).iter().any(|entry| matches!(
        entry,
        menu::MenuEntry::Item { label, enabled: false, .. } if label == "Delete Track"
    )));
    app.run_menu_command(&context, menu::MenuCommand::AddTrack);
    assert_eq!(app.song().tracks.len(), tracks + 1);
    let added = app.roll.selected.expect("the new track is selected");
    let delete = menu_items(&app.menu_model(false))
        .into_iter()
        .find(|(_, label)| label == "Delete Track")
        .map(|(command, _)| command)
        .expect("a Delete Track item");
    app.run_menu_command(&context, delete);
    assert_eq!(app.song().tracks.len(), tracks);
    assert!(app.song().tracks.iter().all(|track| track.id != added));
}

#[test]
fn style_and_ai_menus_open_their_windows() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    app.run_menu_command(&context, menu::MenuCommand::StyleRules);
    assert!(app.style.rules_open);
    app.run_menu_command(&context, menu::MenuCommand::SongForm);
    assert!(app.form_dialog);
    app.run_menu_command(&context, menu::MenuCommand::ArrangementDoctor);
    assert!(app.doctor_dialog);
    let items = menu_items(&app.menu_model(false));
    for (_, op) in menu::GENERATE_OPS {
        assert!(
            items
                .iter()
                .any(|(command, _)| *command == menu::MenuCommand::Generate(op))
        );
    }
}

#[test]
fn ai_menu_writes_a_part_on_the_selected_track() {
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let track = app.song().tracks[0].id;
    app.roll.selected = Some(track);
    let depth = app.editor.undo_depth();
    app.run_menu_command(&context, menu::MenuCommand::Generate("humanize"));
    assert_eq!(app.editor.undo_depth(), depth + 1, "{:?}", app.status);
}

#[test]
fn notes_menu_copies_pastes_and_deletes_notes() {
    use super::notes_menu::NoteAction;
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let track = app
        .song()
        .tracks
        .iter()
        .find(|entry| !entry.notes.is_empty())
        .map(|entry| entry.id)
        .expect("a track with notes");
    app.roll.selected = Some(track);
    let count = |app: &ShellApp| {
        app.song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map_or(0, |entry| entry.notes.len())
    };
    let before = count(&app);
    let first = app
        .song()
        .tracks
        .iter()
        .find(|entry| entry.id == track)
        .and_then(|entry| entry.notes.first())
        .map(|note| note.id)
        .expect("a note");
    app.selected_note = Some((track, first));
    app.run_note_action(NoteAction::Copy);
    assert!(app.editor.has_note_clipboard());
    let far = app.song().end_tick() + 4 * music_core::PPQ;
    app.run_note_action(NoteAction::Paste(Some(far)));
    assert_eq!(count(&app), before + 1, "{:?}", app.status);
    app.run_note_action(NoteAction::Delete);
    assert_eq!(count(&app), before);
    app.run_note_action(NoteAction::SelectAll);
    assert_eq!(app.selected_notes.len(), before);
    app.run_note_action(NoteAction::Lyrics);
    assert!(app.lyrics_open);
}

#[test]
fn mixer_menu_routes_tracks_and_manages_buses() {
    use super::mixer::MixerAction;
    let context = egui::Context::default();
    let mut app = ShellApp::new(&context, demo_project(), None);
    let track = app.song().tracks[0].id;
    let buses = app.song().buses.len();
    app.run_mixer_action(MixerAction::AddBus);
    assert_eq!(app.song().buses.len(), buses + 1);
    let bus = app.song().buses.last().map(|bus| bus.id).expect("a bus");
    app.run_mixer_action(MixerAction::Output(track, Some(bus)));
    app.run_mixer_action(MixerAction::Send(track, Some(bus)));
    let routed = |app: &ShellApp| {
        app.song()
            .tracks
            .iter()
            .find(|entry| entry.id == track)
            .map(|entry| (entry.bus, entry.send))
    };
    assert_eq!(
        routed(&app),
        Some((Some(bus), Some(bus))),
        "{:?}",
        app.status
    );
    app.run_mixer_action(MixerAction::RemoveBus(bus));
    assert_eq!(app.song().buses.len(), buses);
    assert_eq!(routed(&app), Some((None, None)));
    app.run_mixer_action(MixerAction::Effects(track));
    assert!(app.plugins_open);
    assert_eq!(app.roll.selected, Some(track));
}
