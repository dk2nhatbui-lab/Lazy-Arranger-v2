//! Vector PDF export over the engraving layout (ADR-010).
//!
//! A small PDF 1.4 writer: catalog, page tree, a Helvetica font for text and a
//! content stream per page. Staff lines, barlines and stems are stroked lines,
//! noteheads are filled circles, and accidentals are drawn as paths because the
//! base-14 fonts carry no music sharp or flat.

use std::fmt::Write as _;

use crate::{Engraved, Settings};

/// Writes the engraved document as PDF bytes.
#[must_use]
pub fn to_pdf(engraved: &Engraved, settings: Settings, title: &str) -> Vec<u8> {
    let page_count = engraved.pages.len().max(1);
    let mut objects: Vec<Vec<u8>> = Vec::new();
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = (0..page_count)
        .map(|index| format!("{} 0 R", 4 + index * 2))
        .collect();
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {page_count} >>",
            kids.join(" ")
        )
        .into_bytes(),
    );
    objects.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );

    for index in 0..page_count {
        let mut content = String::new();
        let _ = writeln!(
            content,
            "0.98 0.97 0.94 rg 0 0 {w} {h} re f 0.2 0.2 0.2 RG 0.7 w",
            w = settings.page_width,
            h = settings.page_height
        );
        let _ = writeln!(
            content,
            "BT /F1 12 Tf {x:.1} {y:.1} Td ({title}) Tj ET",
            x = settings.margin,
            y = settings.page_height - settings.margin,
            title = escape(title)
        );
        let _ = writeln!(
            content,
            "BT /F1 9 Tf {x:.1} {y:.1} Td (Page {} of {}) Tj ET",
            index + 1,
            page_count,
            x = settings.page_width / 2.0 - 24.0,
            y = settings.margin / 2.0
        );
        if let Some(page) = engraved.pages.get(index) {
            for system in &page.systems {
                let top = system.y;
                let bottom = top + 4.0 * settings.staff_space;
                for line in 0..5 {
                    let y = settings.page_height - (top + (line as f32) * settings.staff_space);
                    let _ = writeln!(
                        content,
                        "{:.1} {:.1} m {:.1} {:.1} l S",
                        settings.margin,
                        y,
                        settings.page_width - settings.margin,
                        y
                    );
                }
                for measure in &system.measures {
                    for x in [measure.x, measure.x_end] {
                        let _ = writeln!(
                            content,
                            "{x:.1} {:.1} m {x:.1} {:.1} l S",
                            settings.page_height - top - 0.5,
                            settings.page_height - bottom + 0.5
                        );
                    }
                }
            }
            for mark in engraved.marks.iter().filter(|mark| mark.page == index) {
                let text = match mark.kind {
                    project::song::MarkKind::Dynamic(dynamic) => dynamic.text().to_string(),
                    project::song::MarkKind::Articulation(articulation) => match articulation {
                        project::song::Articulation::Staccato => String::from("stacc."),
                        project::song::Articulation::Accent => String::from("acc."),
                        project::song::Articulation::Tenuto => String::from("ten."),
                        project::song::Articulation::Marcato => String::from("marc."),
                        project::song::Articulation::Fermata => String::from("ferm."),
                    },
                };
                let _ = writeln!(
                    content,
                    "BT /F1 9 Tf {x:.1} {y:.1} Td ({text}) Tj ET",
                    x = mark.x - 6.0,
                    y = settings.page_height - mark.y,
                    text = escape(&text)
                );
            }
            for lyric in engraved.lyrics.iter().filter(|lyric| lyric.page == index) {
                let _ = writeln!(
                    content,
                    "BT /F1 9 Tf {x:.1} {y:.1} Td ({text}) Tj ET",
                    x = lyric.x - 6.0,
                    y = settings.page_height - lyric.y,
                    text = escape(&lyric.text)
                );
            }
            for note in engraved
                .notes
                .iter()
                .filter(|note| note.page == index && !note.tied)
            {
                let x = note.x;
                let y = settings.page_height - note.y;
                let radius = 0.58 * settings.staff_space;
                if let Some(alter) = note.spelled.accidental {
                    accidental(&mut content, alter, x - radius * 2.6, y, radius);
                }
                circle(&mut content, x, y, radius);
                let _ = writeln!(
                    content,
                    "{:.1} {:.1} m {:.1} {:.1} l S",
                    x + radius,
                    y,
                    x + radius,
                    y + 3.4 * settings.staff_space
                );
            }
        }
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /Font << /F1 3 0 R >> >> /Contents {contents} 0 R >>",
                w = settings.page_width,
                h = settings.page_height,
                contents = 5 + index * 2
            )
            .into_bytes(),
        );
        let mut stream = b"<< /Length ".to_vec();
        stream.extend_from_slice(content.len().to_string().as_bytes());
        stream.extend_from_slice(b" >>\nstream\n");
        stream.extend_from_slice(content.as_bytes());
        stream.extend_from_slice(b"endstream");
        objects.push(stream);
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn accidental(content: &mut String, alter: i8, x: f32, y: f32, radius: f32) {
    let _ = writeln!(content, "{} w", radius * 0.22);
    match alter {
        1 => {
            for offset in [-0.35_f32, 0.05] {
                let _ = writeln!(
                    content,
                    "{:.2} {:.2} m {:.2} {:.2} l S",
                    x + offset * radius,
                    y - 1.3 * radius,
                    x + offset * radius,
                    y + 1.1 * radius
                );
            }
            for offset in [-0.4_f32, 0.15] {
                let _ = writeln!(
                    content,
                    "{:.2} {:.2} m {:.2} {:.2} l S",
                    x - 0.55 * radius,
                    y + offset * radius,
                    x + 0.45 * radius,
                    y + (offset - 0.45) * radius
                );
            }
        }
        -1 => {
            let _ = writeln!(
                content,
                "{:.2} {:.2} m {:.2} {:.2} l S",
                x,
                y - 1.6 * radius,
                x,
                y + 0.7 * radius
            );
            let _ = writeln!(
                content,
                "{:.2} {:.2} m {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c S",
                x,
                y - 0.1 * radius,
                x + 0.9 * radius,
                y + 0.2 * radius,
                x + 0.9 * radius,
                y - 0.7 * radius,
                x,
                y - 0.8 * radius
            );
        }
        _ => {
            for offset in [-0.35_f32, 0.15] {
                let _ = writeln!(
                    content,
                    "{:.2} {:.2} m {:.2} {:.2} l S",
                    x + offset * radius,
                    y - 1.1 * radius,
                    x + offset * radius,
                    y + 1.1 * radius
                );
            }
            for offset in [-0.2_f32, 0.4] {
                let _ = writeln!(
                    content,
                    "{:.2} {:.2} m {:.2} {:.2} l S",
                    x - 0.4 * radius,
                    y + offset * radius,
                    x + 0.4 * radius,
                    y + offset * radius
                );
            }
        }
    }
}

fn circle(content: &mut String, x: f32, y: f32, radius: f32) {
    let k = radius * 0.552_284_8;
    let _ = writeln!(
        content,
        "{:.2} {:.2} m {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c {:.2} {:.2} {:.2} {:.2} {:.2} {:.2} c f",
        x + radius,
        y,
        x + radius,
        y + k,
        x + k,
        y + radius,
        x,
        y + radius,
        x - k,
        y + radius,
        x - radius,
        y + k,
        x - radius,
        y,
        x - radius,
        y - k,
        x - k,
        y - radius,
        x,
        y - radius,
        x + k,
        y - radius,
        x + radius,
        y - k,
        x + radius,
        y
    );
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// Reads the declared page count back out of the bytes, for tests.
#[must_use]
pub fn page_count(pdf: &[u8]) -> Option<usize> {
    let text = String::from_utf8_lossy(pdf);
    let start = text.find("/Type /Pages")?;
    let tail = &text[start..];
    let count = tail.find("/Count ")?;
    tail[count + 7..]
        .split(|character: char| !character.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, Song, Track};

    fn engraved(measures: usize, settings: Settings) -> Engraved {
        let mut song = Song::new("PDF", 120.0);
        song.chords.push(project::song::ChordEvent {
            tick: 0,
            chord: music_core::Chord::parse("C").expect("valid"),
        });
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for index in 0..measures {
            track.notes.push(Note {
                id: project::NoteId::new(index as u32 + 1).expect("valid"),
                start: (index as i64) * 3840 + 120,
                duration: PPQ,
                pitch: Pitch::from_name("C#4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        crate::engrave(&song, TrackId::new(1).expect("valid"), settings)
    }

    fn small() -> Settings {
        Settings {
            page_width: 320.0,
            page_height: 200.0,
            margin: 20.0,
            staff_space: 6.0,
            system_gap: 60.0,
            quarter_width: 20.0,
            measure_gap: 8.0,
        }
    }

    #[test]
    fn a_pdf_has_header_pages_and_trailer() {
        let pdf = to_pdf(&engraved(12, small()), small(), "Test score");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Type /Catalog"));
        assert_eq!(page_count(&pdf), Some(2));
        assert!(text.contains("(Test score) Tj"));
        assert!(!text.contains(" C? "), "no music glyphs from base fonts");
    }

    #[test]
    fn the_xref_offset_points_at_the_table() {
        let pdf = to_pdf(&engraved(12, small()), small(), "Xref");
        let text = String::from_utf8_lossy(&pdf);
        let startxref = text.rfind("startxref").expect("startxref");
        let offset: usize = text[startxref + 9..]
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .expect("offset")
            .parse()
            .expect("number");
        assert!(pdf[offset..].starts_with(b"xref"));
    }

    #[test]
    fn empty_documents_still_write_one_page() {
        let pdf = to_pdf(&engraved(0, small()), small(), "Empty");
        assert_eq!(page_count(&pdf), Some(1));
    }
}
