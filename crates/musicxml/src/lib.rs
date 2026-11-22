//! MusicXML export.
//!
//! Writes a partwise document with one part per track and one measure per bar.
//! Gaps become rests and notes are clipped at bar lines. Durations that do not
//! match a plain note value are written without a `<type>`, which is valid
//! MusicXML; tuplets and ties are not notated yet.

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
    for (index, track) in song.tracks.iter().enumerate() {
        out.push_str(&format!(
            "    <score-part id=\"P{}\"><part-name>{}</part-name></score-part>\n",
            index + 1,
            escape(if track.name.is_empty() {
                "Track"
            } else {
                &track.name
            })
        ));
    }
    out.push_str("  </part-list>\n");
    for (index, track) in song.tracks.iter().enumerate() {
        out.push_str(&format!("  <part id=\"P{}\">\n", index + 1));
        let drums = track
            .notes
            .iter()
            .any(|note| note.channel.zero_based() == 9);
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
                out.push_str("        <key><fifths>0</fifths></key>\n");
                out.push_str(&format!(
                    "        <time><beats>{}</beats><beat-type>{}</beat-type></time>\n",
                    song.time_signature.numerator, song.time_signature.denominator
                ));
                out.push_str(if drums {
                    "        <clef><sign>percussion</sign><line>2</line></clef>\n"
                } else {
                    "        <clef><sign>G</sign><line>2</line></clef>\n"
                });
                out.push_str("      </attributes>\n");
            }
            for mark in song
                .marks
                .iter()
                .filter(|mark| mark.tick >= start && mark.tick < stop)
            {
                write_direction(&mut out, mark.kind);
            }
            let mut notes: Vec<&Note> = track
                .notes
                .iter()
                .filter(|note| note.start >= start && note.start < stop)
                .collect();
            notes.sort_by_key(|note| note.start);
            let mut cursor = start;
            for note in notes {
                if note.start > cursor {
                    write_rest(&mut out, note.start - cursor);
                }
                let duration = note.duration.min(stop - note.start).max(1);
                let lyric = song
                    .lyrics
                    .iter()
                    .find(|event| event.tick == note.start)
                    .map(|event| event.text.as_str());
                write_note(&mut out, note, duration, drums, lyric);
                cursor = note.start + duration;
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

fn write_direction(out: &mut String, kind: project::song::MarkKind) {
    use project::song::{Articulation, MarkKind};
    let text = match kind {
        MarkKind::Dynamic(dynamic) => format!(
            "<direction><direction-type><dynamics><{} /></dynamics></direction-type></direction>",
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
            format!(
                "<direction><direction-type><words>{words}</words></direction-type></direction>"
            )
        }
    };
    out.push_str("      ");
    out.push_str(&text);
    out.push('\n');
}

fn write_rest(out: &mut String, duration: Tick) {
    out.push_str("      <note><rest/>");
    out.push_str(&format!("<duration>{duration}</duration>"));
    if let Some(kind) = note_type(duration) {
        out.push_str(&format!("<type>{kind}</type>"));
    }
    out.push_str("</note>\n");
}

fn write_note(out: &mut String, note: &Note, duration: Tick, drums: bool, lyric: Option<&str>) {
    out.push_str("      <note>");
    if drums {
        out.push_str("<unpitched><display-step>C</display-step><display-octave>5</display-octave></unpitched>");
    } else {
        let (step, alter, octave) = pitch_parts(note.pitch);
        out.push_str(&format!(
            "<pitch><step>{step}</step><alter>{alter}</alter><octave>{octave}</octave></pitch>"
        ));
    }
    out.push_str(&format!("<duration>{duration}</duration>"));
    if let Some(kind) = note_type(duration) {
        out.push_str(&format!("<type>{kind}</type>"));
    }
    if let Some(text) = lyric {
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
