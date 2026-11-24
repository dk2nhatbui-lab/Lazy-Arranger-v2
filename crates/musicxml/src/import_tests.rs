#[cfg(test)]
mod tests {
    use crate::export;
    use crate::import::{ImportError, import};
    use project::song::{Song, Track};

    #[test]
    fn our_export_round_trips_through_the_importer() {
        let mut song = Song::new("Round trip", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        for (index, (start, name)) in [(0_i64, "C4"), (960, "E4"), (1920, "G4")]
            .into_iter()
            .enumerate()
        {
            track.notes.push(project::song::Note {
                id: project::NoteId::new(index as u32 + 1).expect("valid"),
                start,
                duration: 960,
                pitch: music_core::Pitch::from_name(name).expect("valid"),
                velocity: music_core::Velocity::new(100).expect("valid"),
                channel: music_core::Channel::first(),
            });
        }
        song.tracks.push(track);
        let imported = import(&export(&song, "Round trip")).expect("import");
        assert_eq!(imported.tracks.len(), 1);
        let pitches: Vec<u8> = imported.tracks[0]
            .notes
            .iter()
            .map(|note| note.pitch.as_u8())
            .collect();
        assert_eq!(pitches, vec![60, 64, 67]);
        let starts: Vec<i64> = imported.tracks[0]
            .notes
            .iter()
            .map(|note| note.start)
            .collect();
        assert_eq!(starts, vec![0, 960, 1920]);
        assert_eq!(imported.tracks[0].notes[0].duration, 960);
        assert!((imported.tempo_map.bpm_at(0) - 120.0).abs() < 1.0e-6);
    }

    #[test]
    fn selected_key_survives_our_export() {
        use music_core::PitchClass;
        use project::song::KeySignature;
        let mut song = Song::new("Key", 120.0);
        song.key = Some(KeySignature {
            tonic: PitchClass::new(2),
            minor: false,
        });
        song.tracks.push(Track::new(
            project::TrackId::new(1).expect("valid"),
            "Piano",
        ));
        let imported = import(&export(&song, "Key")).expect("import");
        let key = imported.key.expect("key");
        assert_eq!(key.tonic.as_u8(), 2);
        assert!(!key.minor);
        song.key = Some(KeySignature {
            tonic: PitchClass::new(9),
            minor: true,
        });
        let imported = import(&export(&song, "Minor")).expect("import");
        let key = imported.key.expect("key");
        assert_eq!(key.tonic.as_u8(), 9);
        assert!(key.minor);
    }

    #[test]
    fn tempo_changes_survive_our_export() {
        let mut song = Song::new("Tempo", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 4 * music_core::PPQ,
            duration: music_core::PPQ,
            pitch: music_core::Pitch::from_name("C4").expect("valid"),
            velocity: music_core::Velocity::new(100).expect("valid"),
            channel: music_core::Channel::first(),
        });
        song.tracks.push(track);
        song.tempo_map = song
            .tempo_map
            .clone()
            .with_change(4 * music_core::PPQ, 90.0)
            .expect("tempo");
        let imported = import(&export(&song, "Tempo")).expect("import");
        let changes = imported.tempo_map.changes();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[1].tick, 4 * music_core::PPQ);
        assert!((changes[1].bpm - 90.0).abs() < 0.01);
    }

    #[test]
    fn offsets_survive_our_export() {
        use music_core::Chord;
        use project::song::{Dynamic, Mark, MarkKind};
        let mut song = Song::new("Offsets", 120.0);
        song.tracks.push(Track::new(
            project::TrackId::new(1).expect("valid"),
            "Piano",
        ));
        song.chords.push(project::song::ChordEvent {
            tick: 480,
            chord: Chord::parse("G").expect("chord"),
        });
        song.marks.push(Mark {
            tick: 480,
            kind: MarkKind::Dynamic(Dynamic::F),
        });
        let imported = import(&export(&song, "Offsets")).expect("import");
        assert_eq!(imported.chords[0].tick, 480);
        assert_eq!(imported.marks[0].tick, 480);
    }

    #[test]
    fn chords_stack_and_unpitched_notes_become_drums() {
        let xml = r#"<?xml version="1.0"?>
        <score-partwise version="4.0">
          <work><work-title>Chords</work-title></work>
          <part-list><score-part id="P1"><part-name>Keys</part-name></score-part></part-list>
          <part id="P1"><measure number="1">
            <attributes><divisions>960</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
            <note><pitch><step>C</step><octave>4</octave></pitch><duration>960</duration></note>
            <note><chord/><pitch><step>E</step><octave>4</octave></pitch><duration>960</duration></note>
            <note><unpitched><display-step>C</display-step><display-octave>5</display-octave></unpitched><duration>480</duration></note>
          </measure></part>
        </score-partwise>"#;
        let song = import(xml).expect("import");
        assert_eq!(song.tracks[0].notes.len(), 3);
        assert_eq!(song.tracks[0].notes[0].start, 0);
        assert_eq!(song.tracks[0].notes[1].start, 0);
        assert_eq!(song.tracks[0].notes[2].start, 960);
        assert_eq!(song.tracks[0].notes[2].channel.zero_based(), 9);
        assert_eq!(song.time_signature.numerator, 4);
    }

    #[test]
    fn divisions_scale_durations_and_the_work_title_is_read() {
        let xml = r#"<?xml version="1.0"?>
        <score-partwise version="4.0">
          <work><work-title>External score</work-title></work>
          <part-list><score-part id="P1"><part-name>Keys</part-name></score-part></part-list>
          <part id="P1"><measure number="1">
            <attributes><divisions>480</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
            <note><pitch><step>C</step><octave>4</octave></pitch><duration>480</duration></note>
          </measure></part>
        </score-partwise>"#;
        let song = import(xml).expect("import");
        assert_eq!(song.name, "External score");
        assert_eq!(song.tracks[0].notes[0].duration, music_core::PPQ);
    }

    #[test]
    fn overlapping_notes_keep_their_starts_through_backup() {
        let mut song = Song::new("Overlap", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        for (index, (start, duration)) in [(3840_i64, 1000_i64), (3900, 500)].iter().enumerate() {
            track.notes.push(project::song::Note {
                id: project::NoteId::new(index as u32 + 1).expect("valid"),
                start: *start,
                duration: *duration,
                pitch: music_core::Pitch::new(60 + index as u8).expect("valid"),
                velocity: music_core::Velocity::new(90).expect("valid"),
                channel: music_core::Channel::first(),
            });
        }
        song.tracks.push(track);
        let xml = export(&song, "Overlap");
        assert!(xml.contains("<backup>"));
        let imported = import(&xml).expect("import");
        let starts: Vec<i64> = imported.tracks[0]
            .notes
            .iter()
            .map(|note| note.start)
            .collect();
        assert_eq!(starts, vec![3840, 3900]);
    }

    #[test]
    fn chord_tones_keep_their_common_start() {
        let mut song = Song::new("Chords", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        for (index, name) in ["C4", "E4", "G4"].iter().enumerate() {
            track.notes.push(project::song::Note {
                id: project::NoteId::new(index as u32 + 1).expect("valid"),
                start: 0,
                duration: 960,
                pitch: music_core::Pitch::from_name(name).expect("valid"),
                velocity: music_core::Velocity::new(90).expect("valid"),
                channel: music_core::Channel::first(),
            });
        }
        song.tracks.push(track);
        let xml = export(&song, "Chords");
        assert_eq!(xml.matches("<chord />").count(), 2);
        let imported = import(&xml).expect("import");
        assert_eq!(imported.tracks[0].notes.len(), 3);
        assert!(imported.tracks[0].notes.iter().all(|note| note.start == 0));
    }

    #[test]
    fn drum_pitch_and_velocity_survive_an_export_import_round_trip() {
        let mut song = Song::new("Drums", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Drums");
        track.notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: 240,
            pitch: music_core::Pitch::new(42).expect("valid"),
            velocity: music_core::Velocity::new(73).expect("valid"),
            channel: music_core::Channel::drums(),
        });
        song.tracks.push(track);
        let xml = export(&song, "Drums");
        let imported = import(&xml).expect("import");
        let note = &imported.tracks[0].notes[0];
        assert_eq!(note.pitch.as_u8(), 42);
        assert_eq!(note.velocity.as_u8(), 73);
        assert_eq!(note.channel.zero_based(), 9);
    }

    #[test]
    fn tied_notes_survive_an_export_import_round_trip() {
        let mut song = Song::new("Ties", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        for (index, start) in [(1_u32, 0_i64), (2, 960)] {
            track.notes.push(project::song::Note {
                id: project::NoteId::new(index).expect("valid"),
                start,
                duration: 960,
                pitch: music_core::Pitch::from_name("C4").expect("valid"),
                velocity: music_core::Velocity::new(100).expect("valid"),
                channel: music_core::Channel::first(),
            });
        }
        song.tracks.push(track);
        let xml = export(&song, "Ties");
        assert!(xml.contains("type=\"start\""));
        assert!(xml.contains("type=\"stop\""));
        let imported = import(&xml).expect("import");
        assert_eq!(imported.tracks[0].notes.len(), 1, "merged into one note");
        assert_eq!(imported.tracks[0].notes[0].duration, 1920);
    }

    #[test]
    fn dynamics_and_articulations_round_trip() {
        use project::song::{Articulation, Dynamic, Mark, MarkKind};
        let mut song = Song::new("Marks", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: 960,
            pitch: music_core::Pitch::from_name("C4").expect("valid"),
            velocity: music_core::Velocity::new(100).expect("valid"),
            channel: music_core::Channel::first(),
        });
        song.tracks.push(track);
        song.marks.push(Mark {
            tick: 0,
            kind: MarkKind::Dynamic(Dynamic::Ff),
        });
        song.marks.push(Mark {
            tick: 0,
            kind: MarkKind::Articulation(Articulation::Staccato),
        });
        let xml = export(&song, "Marks");
        assert!(xml.contains("<ff />"));
        assert!(xml.contains("<words>stacc.</words>"));
        let imported = import(&xml).expect("import");
        assert_eq!(imported.marks.len(), 2);
        let has_dynamic = imported
            .marks
            .iter()
            .any(|mark| matches!(mark.kind, MarkKind::Dynamic(Dynamic::Ff)));
        let has_articulation = imported
            .marks
            .iter()
            .any(|mark| matches!(mark.kind, MarkKind::Articulation(Articulation::Staccato)));
        assert!(has_dynamic && has_articulation);
    }

    #[test]
    fn lyrics_survive_an_export_import_round_trip() {
        let mut song = Song::new("Lyrics", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Voice");
        track.notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: 960,
            pitch: music_core::Pitch::from_name("C4").expect("valid"),
            velocity: music_core::Velocity::new(100).expect("valid"),
            channel: music_core::Channel::first(),
        });
        song.tracks.push(track);
        song.lyrics.push(project::song::LyricEvent {
            tick: 0,
            text: String::from("hel"),
        });
        let xml = export(&song, "Lyrics");
        assert!(xml.contains("<text>hel</text>"), "export writes the lyric");
        let imported = import(&xml).expect("import");
        assert_eq!(imported.lyrics.len(), 1);
        assert_eq!(imported.lyrics[0].text, "hel");
    }

    #[test]
    fn tempo_and_meter_are_read() {
        let xml = r#"<?xml version="1.0"?>
        <score-partwise version="4.0">
          <part-list><score-part id="P1"><part-name>Piano</part-name></score-part></part-list>
          <part id="P1"><measure number="1">
            <attributes><divisions>960</divisions><time><beats>3</beats><beat-type>4</beat-type></time></attributes>
            <sound tempo="90"/>
            <note><pitch><step>D</step><alter>1</alter><octave>4</octave></pitch><duration>960</duration></note>
          </measure></part>
        </score-partwise>"#;
        let song = import(xml).expect("import");
        assert!((song.tempo_map.bpm_at(0) - 90.0).abs() < 0.01);
        assert_eq!(song.time_signature.numerator, 3);
        assert_eq!(song.tracks[0].notes[0].pitch.as_u8(), 63);
    }

    #[test]
    fn other_documents_are_errors() {
        assert!(import("not xml").is_err());
        assert!(matches!(
            import("<score-partwise><part-list/></score-partwise>"),
            Err(ImportError::NoParts)
        ));
    }
}
