//! MusicXML export.
//!
//! Writes a partwise document with one part per track and one measure per bar.
//! Gaps become rests and notes are clipped at bar lines. Durations that do not
//! match a plain note value are written without a `<type>`, which is valid
//! MusicXML; tuplets and ties are not notated yet.

pub mod audit;
pub mod import;

#[cfg(test)]
mod import_tests;

use music_core::{Pitch, Tick, TimeSignature};
use project::song::{Note, Song};

/// Pitch as step, alter and octave for MusicXML, using sharps.
#[must_use]
pub fn pitch_parts(pitch: Pitch) -> (&'static str, i32, i32) {
    const STEPS: [&str; 12] = ["C", "C", "D", "D", "E", "F", "F", "G", "G", "A", "A", "B"];
    const ALTERS: [i32; 12] = [0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0];
    let midi = pitch.as_u8();
    let class = usize::from(midi % 12);
    (STEPS[class], ALTERS[class], i32::from(midi / 12) - 1)
}

/// The `<type>` value for a duration, when it is a plain note value.
#[must_use]
pub fn note_type(duration: Tick) -> Option<&'static str> {
    match duration {
        3840 => Some("whole"),
        1920 => Some("half"),
        960 => Some("quarter"),
        480 => Some("eighth"),
        240 => Some("16th"),
        120 => Some("32nd"),
        60 => Some("64th"),
        _ => None,
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Writes the song as a MusicXML partwise document.
#[must_use]
pub fn export(song: &Song, title: &str) -> String {
    let bar = song.time_signature.bar_ticks().max(1);
    let end = song.end_tick().max(bar);
    let bars = (end + bar - 1) / bar;
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 4.0 Partwise//EN\" \"http://www.musicxml.org/dtds/partwise.dtd\">\n");
    out.push_str("<score-partwise version=\"4.0\">\n");
    out.push_str(&format!(
        "  <work><work-title>{}</work-title></work>\n",
        escape(title)
    ));
    out.push_str("  <part-list>\n");
    if song.tracks.len() > 1 {
        out.push_str("    <part-group type=\"start\" number=\"1\"><group-symbol>bracket</group-symbol><group-barline>yes</group-barline></part-group>\n");
    }
    for (index, track) in song.tracks.iter().enumerate() {
        out.push_str(&format!(
            "    <score-part id=\"P{}\"><part-name>{}</part-name>",
            index + 1,
            escape(if track.name.is_empty() {
                "Track"
            } else {
                &track.name
            })
        ));
        let drums = track
            .notes
            .iter()
            .any(|note| note.channel.zero_based() == 9);
        if drums {
            let mut pitched: Vec<u8> = track
                .notes
                .iter()
                .filter(|note| note.channel.zero_based() == 9)
                .map(|note| note.pitch.as_u8())
                .collect();
            pitched.sort_unstable();
            pitched.dedup();
            for pitch in pitched {
                out.push_str(&format!(
                    "<score-instrument id=\"P{part}-I{pitch}\"><instrument-name>Drum {pitch}</instrument-name></score-instrument><midi-instrument id=\"P{part}-I{pitch}\"><midi-channel>10</midi-channel><midi-unpitched>{one}</midi-unpitched></midi-instrument>",
                    part = index + 1,
                    one = u16::from(pitch) + 1
                ));
            }
        }
        out.push_str("</score-part>\n");
    }
    if song.tracks.len() > 1 {
        out.push_str("    <part-group type=\"stop\" number=\"1\"/>\n");
    }
    out.push_str("  </part-list>\n");
    for (index, track) in song.tracks.iter().enumerate() {
        out.push_str(&format!("  <part id=\"P{}\">\n", index + 1));
        let drums = track
            .notes
            .iter()
            .any(|note| note.channel.zero_based() == 9);
        let mut lyric_written: std::collections::HashSet<Tick> = std::collections::HashSet::new();
        for bar_index in 0..bars {
            let start = bar_index * bar;
            let stop = start + bar;
            out.push_str(&format!("    <measure number=\"{}\">\n", bar_index + 1));
            if bar_index == 0 {
                out.push_str("      <attributes>\n");
                out.push_str(&format!(
                    "        <divisions>{}</divisions>\n",
                    music_core::PPQ
                ));
                let key = score::spelling::detect_key(song);
                out.push_str(&format!(
                    "        <key><fifths>{}</fifths><mode>{}</mode></key>\n",
                    key_fifths(key),
                    if key.minor { "minor" } else { "major" }
                ));
                out.push_str(&format!(
                    "        <time><beats>{}</beats><beat-type>{}</beat-type></time>\n",
                    song.time_signature.numerator, song.time_signature.denominator
                ));
                let average = if track.notes.is_empty() {
                    60.0
                } else {
                    track
                        .notes
                        .iter()
                        .map(|note| f32::from(note.pitch.as_u8()))
                        .sum::<f32>()
                        / track.notes.len() as f32
                };
                out.push_str(if drums {
                    "        <clef><sign>percussion</sign><line>2</line></clef>\n"
                } else if average < 52.0 {
                    "        <clef><sign>F</sign><line>4</line></clef>\n"
                } else {
                    "        <clef><sign>G</sign><line>2</line></clef>\n"
                });
                out.push_str("      </attributes>\n");
            }
            if index == 0 || bar_index > 0 {
                let tempo = song.tempo_map.bpm_at(start);
                let changed =
                    bar_index == 0 || (tempo - song.tempo_map.bpm_at(start - 1)).abs() > 1.0e-6;
                if changed && tempo.is_finite() && tempo > 0.0 {
                    out.push_str(&format!("      <sound tempo=\"{tempo}\"/>\n"));
                }
            }
            if index == 0 {
                for marker in song
                    .markers
                    .iter()
                    .filter(|marker| marker.tick >= start && marker.tick < stop)
                {
                    out.push_str(&format!(
                        "      <direction placement=\"above\"><direction-type><rehearsal>{}</rehearsal></direction-type><offset>{}</offset></direction>\n",
                        escape(&marker.name),
                        marker.tick - start
                    ));
                }
                for mark in song
                    .marks
                    .iter()
                    .filter(|mark| mark.tick >= start && mark.tick < stop)
                {
                    write_direction(&mut out, mark.kind, mark.tick - start);
                }
                for chord in song
                    .chords
                    .iter()
                    .filter(|event| event.tick >= start && event.tick < stop)
                {
                    write_harmony(&mut out, chord.chord, chord.tick - start);
                }
            }
            let ties = tie_flags(track);
            let slurs = slur_flags(track);
            let mut notes: Vec<&Note> = track
                .notes
                .iter()
                .filter(|note| note.start >= start && note.start < stop)
                .collect();
            notes.sort_by_key(|note| note.start);
            let mut cursor = start;
            let mut last_start: Option<Tick> = None;
            for note in notes {
                if note.start < cursor && last_start != Some(note.start) {
                    write_backup(&mut out, cursor - note.start);
                    cursor = note.start;
                }
                if note.start > cursor {
                    write_rest(&mut out, note.start - cursor);
                }
                let duration = note.duration.min(stop - note.start).max(1);
                let lyric = if index == 0 && lyric_written.insert(note.start) {
                    song.lyrics
                        .iter()
                        .find(|event| event.tick == note.start)
                        .map(|event| event.text.as_str())
                } else {
                    None
                };
                let (tie_start, tie_stop) =
                    ties.get(&note.id.get()).copied().unwrap_or((false, false));
                let (slur_start, slur_stop) =
                    slurs.get(&note.id.get()).copied().unwrap_or((false, false));
                let is_drum = note.channel.zero_based() == 9;
                let instrument = is_drum.then(|| format!("P{}-I{}", index + 1, note.pitch.as_u8()));
                let chord = last_start == Some(note.start);
                let marks = NoteMarks {
                    instrument,
                    lyric,
                    tie_start,
                    tie_stop,
                    slur_start,
                    slur_stop,
                    chord,
                };
                write_note(&mut out, note, duration, &marks);
                last_start = Some(note.start);
                if !chord {
                    cursor = note.start + duration;
                }
            }
            if cursor < stop {
                write_rest(&mut out, stop - cursor);
            }
            out.push_str("    </measure>\n");
        }
        out.push_str("  </part>\n");
    }
    out.push_str("</score-partwise>\n");
    out
}

fn tie_flags(track: &project::song::Track) -> std::collections::BTreeMap<u32, (bool, bool)> {
    let mut flags: std::collections::BTreeMap<u32, (bool, bool)> =
        std::collections::BTreeMap::new();
    let mut open: std::collections::BTreeMap<u8, (u32, Tick)> = std::collections::BTreeMap::new();
    let mut notes: Vec<&Note> = track.notes.iter().collect();
    notes.sort_by_key(|note| note.start);
    for note in notes {
        let pitch = note.pitch.as_u8();
        if let Some((previous_id, end)) = open.get(&pitch).copied()
            && end == note.start
        {
            flags.entry(previous_id).or_default().0 = true;
            flags.entry(note.id.get()).or_default().1 = true;
        }
        open.insert(pitch, (note.id.get(), note.start + note.duration.max(1)));
    }
    flags
}

fn slur_flags(track: &project::song::Track) -> std::collections::BTreeMap<u32, (bool, bool)> {
    let mut flags: std::collections::BTreeMap<u32, (bool, bool)> =
        std::collections::BTreeMap::new();
    for slur in &track.slurs {
        let start_note = track
            .notes
            .iter()
            .filter(|note| note.start >= slur.start)
            .min_by_key(|note| note.start);
        let end_note = track
            .notes
            .iter()
            .filter(|note| note.start <= slur.end)
            .max_by_key(|note| note.start);
        if let (Some(start_note), Some(end_note)) = (start_note, end_note) {
            flags.entry(start_note.id.get()).or_default().0 = true;
            flags.entry(end_note.id.get()).or_default().1 = true;
        }
    }
    flags
}

fn key_fifths(key: score::spelling::Key) -> i32 {
    let major = if key.minor {
        (key.tonic.as_u8() + 3) % 12
    } else {
        key.tonic.as_u8()
    };
    match major {
        0 => 0,
        7 => 1,
        2 => 2,
        9 => 3,
        4 => 4,
        11 => 5,
        6 => 6,
        1 => 7,
        5 => -1,
        10 => -2,
        3 => -3,
        8 => -4,
        _ => 0,
    }
}

fn write_harmony(out: &mut String, chord: music_core::Chord, offset: Tick) {
    let pc = chord.root.as_u8();
    let letter = match pc {
        0 | 1 => "C",
        2 | 3 => "D",
        4 => "E",
        5 => "F",
        6 | 7 => "G",
        8 | 9 => "A",
        _ => "B",
    };
    let alter = match pc {
        1 | 3 | 6 | 8 | 10 => 1,
        _ => 0,
    };
    let kind = match chord.quality {
        music_core::ChordQuality::Minor => "minor",
        music_core::ChordQuality::Dominant7 => "dominant",
        music_core::ChordQuality::Major7 => "major-seventh",
        music_core::ChordQuality::Minor7 => "minor-seventh",
        music_core::ChordQuality::Diminished | music_core::ChordQuality::Diminished7 => {
            "diminished"
        }
        music_core::ChordQuality::HalfDiminished7 => "half-diminished",
        music_core::ChordQuality::Augmented => "augmented",
        _ => "major",
    };
    out.push_str(&format!(
        "      <harmony><root><root-step>{letter}</root-step>{alter_xml}</root><kind>{kind}</kind>",
        alter_xml = if alter == 1 {
            "<root-alter>1</root-alter>"
        } else {
            ""
        }
    ));
    if let Some(bass) = chord.bass {
        let pc = bass.as_u8();
        let letter = match pc {
            0 | 1 => "C",
            2 | 3 => "D",
            4 => "E",
            5 => "F",
            6 | 7 => "G",
            8 | 9 => "A",
            _ => "B",
        };
        let alter = matches!(pc, 1 | 3 | 6 | 8 | 10);
        out.push_str(&format!("<bass><bass-step>{letter}</bass-step>"));
        if alter {
            out.push_str("<bass-alter>1</bass-alter>");
        }
        out.push_str("</bass>");
    }
    if offset != 0 {
        out.push_str(&format!("<offset>{offset}</offset>"));
    }
    if let Some(frame) = score::guitar::shape(&chord) {
        out.push_str("<frame><frame-strings>6</frame-strings><frame-frets>5</frame-frets>");
        if frame.base_fret > 1 {
            out.push_str(&format!("<first-fret>{}</first-fret>", frame.base_fret));
        }
        for (index, fret) in frame.frets.iter().enumerate() {
            if let Some(fret) = fret {
                out.push_str(&format!(
                    "<frame-note><string>{}</string><fret>{fret}</fret></frame-note>",
                    6 - index
                ));
            }
        }
        out.push_str("</frame>");
    }
    out.push_str("</harmony>\n");
}

fn write_direction(out: &mut String, kind: project::song::MarkKind, offset: Tick) {
    use project::song::{Articulation, MarkKind};
    let text = match kind {
        MarkKind::Dynamic(dynamic) => format!(
            "<direction-type><dynamics><{} /></dynamics></direction-type>",
            dynamic.text()
        ),
        MarkKind::Articulation(articulation) => {
            let words = match articulation {
                Articulation::Staccato => "stacc.",
                Articulation::Accent => "acc.",
                Articulation::Tenuto => "ten.",
                Articulation::Marcato => "marc.",
                Articulation::Fermata => "ferm.",
            };
            format!("<direction-type><words>{words}</words></direction-type>")
        }
    };
    out.push_str("      <direction>");
    out.push_str(&text);
    if offset != 0 {
        out.push_str(&format!("<offset>{offset}</offset>"));
    }
    out.push_str("</direction>\n");
}

fn write_rest(out: &mut String, duration: Tick) {
    out.push_str("      <note><rest/>");
    out.push_str(&format!("<duration>{duration}</duration>"));
    if let Some(kind) = note_type(duration) {
        out.push_str(&format!("<type>{kind}</type>"));
    }
    out.push_str("</note>\n");
}

struct NoteMarks<'a> {
    instrument: Option<String>,
    lyric: Option<&'a str>,
    tie_start: bool,
    tie_stop: bool,
    slur_start: bool,
    slur_stop: bool,
    chord: bool,
}

fn write_backup(out: &mut String, amount: Tick) {
    if amount > 0 {
        out.push_str(&format!(
            "      <backup><duration>{amount}</duration></backup>\n"
        ));
    }
}

fn write_note(out: &mut String, note: &Note, duration: Tick, marks: &NoteMarks<'_>) {
    out.push_str("      <note>");
    if marks.chord {
        out.push_str("<chord />");
    }
    if let Some(ref id) = marks.instrument {
        out.push_str(&format!(
            "<instrument id=\"{id}\" /><unpitched><display-step>C</display-step><display-octave>5</display-octave></unpitched>"
        ));
    } else {
        let (step, alter, octave) = pitch_parts(note.pitch);
        out.push_str(&format!(
            "<pitch><step>{step}</step><alter>{alter}</alter><octave>{octave}</octave></pitch>"
        ));
    }
    out.push_str(&format!("<duration>{duration}</duration>"));
    out.push_str(&format!(
        "<sound dynamics=\"{velocity}\" />",
        velocity = note.velocity.as_u8()
    ));
    if let Some(kind) = note_type(duration) {
        out.push_str(&format!("<type>{kind}</type>"));
    }
    if marks.tie_stop {
        out.push_str("<tie type=\"stop\" />");
    }
    if marks.tie_start {
        out.push_str("<tie type=\"start\" />");
    }
    if marks.slur_start || marks.slur_stop {
        out.push_str("<notations>");
        if marks.slur_start {
            out.push_str("<slur number=\"1\" type=\"start\" />");
        }
        if marks.slur_stop {
            out.push_str("<slur number=\"1\" type=\"stop\" />");
        }
        out.push_str("</notations>");
    }
    if let Some(text) = marks.lyric {
        out.push_str("<lyric><syllabic>single</syllabic><text>");
        out.push_str(&escape(text));
        out.push_str("</text></lyric>");
    }
    out.push_str("</note>\n");
}

/// Meter helper for callers building attributes.
#[must_use]
pub fn meter_text(meter: &TimeSignature) -> String {
    format!("{}/{}", meter.numerator, meter.denominator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Velocity};
    use project::TrackId;
    use project::song::{Note, Track};

    fn song() -> Song {
        let mut song = Song::new("Score", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        track.notes.push(Note {
            id: project::NoteId::new(2).expect("valid"),
            start: 2 * PPQ,
            duration: PPQ,
            pitch: Pitch::from_name("F#4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        song
    }

    #[test]
    fn slurs_round_trip() {
        let mut song = song();
        song.tracks[0].slurs.push(project::song::Slur {
            start: 0,
            end: 2 * PPQ,
        });
        let xml = export(&song, "Slur test");
        assert!(xml.contains("<slur number=\"1\" type=\"start\" />"));
        assert!(xml.contains("<slur number=\"1\" type=\"stop\" />"));
        let imported = crate::import::import(&xml).expect("import");
        assert_eq!(imported.tracks[0].slurs, song.tracks[0].slurs);
        let delta = crate::audit::audit(&song, &imported);
        assert_eq!(delta.slurs, (1, 1));
        assert!(delta.is_clean(), "{:?}", delta.changes);
    }

    #[test]
    fn a_document_has_parts_measures_and_notes() {
        let xml = export(&song(), "Test");
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("<score-partwise version=\"4.0\">"));
        assert!(xml.contains("<part-name>Piano</part-name>"));
        assert!(xml.contains("<measure number=\"1\">"));
        assert!(xml.contains("<divisions>960</divisions>"));
        assert!(xml.contains("<step>C</step>"));
        assert!(xml.contains("<step>F</step><alter>1</alter><octave>4</octave>"));
        assert!(xml.contains("<type>quarter</type>"));
        assert!(xml.contains("<rest/>"));
        assert!(xml.ends_with("</score-partwise>\n"));
    }

    #[test]
    fn empty_bars_are_rests_and_two_bars_give_two_measures() {
        let mut song = song();
        song.tracks[0].notes[1].start = 5 * PPQ;
        let xml = export(&song, "Two bars");
        assert_eq!(xml.matches("<measure number=").count(), 2);
        assert!(xml.contains("<rest/>"));
    }

    #[test]
    fn drum_tracks_use_percussion_clef_and_unpitched_notes() {
        let mut song = Song::new("Drums", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Drums");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::new(36).expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::new(9).expect("valid"),
        });
        song.tracks.push(track);
        let xml = export(&song, "Drums");
        assert!(xml.contains("<sign>percussion</sign>"));
        assert!(xml.contains("<unpitched>"));
    }

    #[test]
    fn harmony_can_carry_a_guitar_frame() {
        use music_core::Chord;
        let mut song = song();
        song.chords.push(project::song::ChordEvent {
            tick: 0,
            chord: Chord::parse("C").expect("chord"),
        });
        let xml = export(&song, "Frame");
        assert!(xml.contains("<frame-strings>6</frame-strings>"));
        assert!(xml.contains("<frame-note>"));
    }

    #[test]
    fn section_markers_export_as_rehearsal_marks() {
        let mut song = song();
        song.markers.push(project::Marker {
            tick: 0,
            name: String::from("Intro"),
        });
        let xml = export(&song, "Markers");
        assert!(xml.contains("<rehearsal>Intro</rehearsal>"), "{xml}");
    }

    #[test]
    fn slash_chords_export_a_bass_note() {
        use music_core::Chord;
        let mut song = song();
        song.chords.push(project::song::ChordEvent {
            tick: 0,
            chord: Chord::parse("C/E").expect("chord"),
        });
        let xml = export(&song, "Slash");
        assert!(xml.contains("<bass-step>E</bass-step>"), "{xml}");
    }

    #[test]
    fn multiple_tracks_get_a_bracket_group() {
        let mut song = song();
        song.tracks
            .push(Track::new(project::TrackId::new(2).expect("valid"), "Bass"));
        let xml = export(&song, "Bracket");
        assert_eq!(xml.matches("<part-group").count(), 2);
        assert!(xml.contains("<group-symbol>bracket</group-symbol>"));
    }

    #[test]
    fn tempo_changes_are_written_at_measure_starts() {
        let mut song = song();
        let mut note = song.tracks[0].notes[0].clone();
        note.id = project::NoteId::new(9).expect("valid");
        note.start = PPQ * 4;
        song.tracks[0].notes.push(note);
        song.tempo_map = song
            .tempo_map
            .clone()
            .with_change(PPQ * 4, 90.0)
            .expect("tempo");
        let xml = export(&song, "Tempo");
        assert_eq!(xml.matches("<sound tempo=").count(), 2);
        assert!(xml.contains("tempo=\"90\""));
    }

    #[test]
    fn chords_and_marks_keep_their_offsets() {
        use music_core::Chord;
        use project::song::{Dynamic, Mark, MarkKind};
        let mut song = song();
        song.chords.push(project::song::ChordEvent {
            tick: PPQ / 2,
            chord: Chord::parse("C").expect("chord"),
        });
        song.marks.push(Mark {
            tick: PPQ / 2,
            kind: MarkKind::Dynamic(Dynamic::F),
        });
        let xml = export(&song, "Offsets");
        assert_eq!(xml.matches("<offset>480</offset>").count(), 2);
    }

    #[test]
    fn key_fifths_covers_the_circle() {
        use music_core::PitchClass;
        use score::spelling::Key;
        assert_eq!(
            key_fifths(Key {
                tonic: PitchClass::new(0),
                minor: false
            }),
            0
        );
        assert_eq!(
            key_fifths(Key {
                tonic: PitchClass::new(2),
                minor: false
            }),
            2
        );
        assert_eq!(
            key_fifths(Key {
                tonic: PitchClass::new(5),
                minor: false
            }),
            -1
        );
        assert_eq!(
            key_fifths(Key {
                tonic: PitchClass::new(9),
                minor: true
            }),
            0
        );
        assert_eq!(
            key_fifths(Key {
                tonic: PitchClass::new(4),
                minor: true
            }),
            1
        );
    }

    #[test]
    fn low_tracks_use_a_bass_clef() {
        let mut song = Song::new("Bass", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Bass");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("E2").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        let xml = export(&song, "Bass");
        assert!(xml.contains("<clef><sign>F</sign><line>4</line></clef>"));
    }

    #[test]
    fn pitch_parts_and_types_cover_the_common_values() {
        assert_eq!(
            pitch_parts(Pitch::from_name("C4").expect("valid")),
            ("C", 0, 4)
        );
        assert_eq!(
            pitch_parts(Pitch::from_name("F#4").expect("valid")),
            ("F", 1, 4)
        );
        assert_eq!(
            pitch_parts(Pitch::from_name("B3").expect("valid")),
            ("B", 0, 3)
        );
        assert_eq!(note_type(960), Some("quarter"));
        assert_eq!(note_type(100), None);
    }
}
