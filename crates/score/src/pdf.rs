//! Vector PDF export over the engraving layout (ADR-010).
//!
//! A small PDF 1.4 writer: catalog, page tree, a Helvetica font for text and a
//! content stream per page. Staff lines, barlines and stems are stroked lines,
//! noteheads are filled circles, and accidentals are drawn as paths because the
//! base-14 fonts carry no music sharp or flat.

use std::fmt::Write as _;
use std::path::Path;

use crate::clef::{self, ClefImage};
use crate::unicode_font::{UnicodePdfFont, pdf_utf16_hex_string};
use std::cell::RefCell;

use crate::full::{Anchor, Canvas, FullScore, Ink, Point};
use crate::{Engraved, Settings};

fn collect_non_winansi_chars<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<char> {
    let mut chars = Vec::new();
    for text in texts {
        for ch in text.chars() {
            if !pdf_encodable(ch) && !chars.contains(&ch) {
                chars.push(ch);
            }
        }
    }
    chars
}

fn write_text(
    content: &mut String,
    size: f32,
    x: f32,
    y: f32,
    text: &str,
    unicode: Option<&mut UnicodePdfFont>,
) {
    if !text.chars().all(pdf_encodable)
        && let Some(font) = unicode
    {
        let hex = font.encode_hex(text);
        let _ = writeln!(content, "BT /F2 {size:.0} Tf {x:.1} {y:.1} Td {hex} Tj ET");
        return;
    }
    let _ = writeln!(
        content,
        "BT /F1 {size:.0} Tf {x:.1} {y:.1} Td ({}) Tj ET",
        escape(text)
    );
}

/// Writes the engraved document as PDF bytes, automatically embedding a
/// subsetted Unicode TrueType font when non-Latin-1 (e.g. Vietnamese or CJK)
/// text is present.
#[must_use]
pub fn to_pdf(engraved: &Engraved, settings: Settings, title: &str) -> Vec<u8> {
    to_pdf_with_font(engraved, settings, title, None)
}

/// Writes the engraved document as PDF bytes using an optional explicit
/// TrueType font path for non-Latin-1 text.
#[must_use]
pub fn to_pdf_with_font(
    engraved: &Engraved,
    settings: Settings,
    title: &str,
    font_path: Option<&Path>,
) -> Vec<u8> {
    let page_count = engraved.pages.len().max(1);
    let treble_id = 4 + page_count * 2;
    let treble_mask = treble_id + 1;
    let bass_id = treble_id + 2;
    let bass_mask = bass_id + 1;
    let unicode_font_id = bass_mask + 1;
    let needed = collect_non_winansi_chars(
        std::iter::once(title)
            .chain(engraved.lyrics.iter().map(|lyric| lyric.text.as_str()))
            .chain(
                engraved
                    .chord_symbols
                    .iter()
                    .map(|symbol| symbol.name.as_str()),
            )
            .chain(engraved.frames.iter().map(|frame| frame.name.as_str())),
    );
    let mut unicode_font = if needed.is_empty() {
        None
    } else {
        UnicodePdfFont::resolve(font_path, &needed)
    };
    let average_pitch = if engraved.notes.is_empty() {
        60.0
    } else {
        engraved
            .notes
            .iter()
            .map(|note| f32::from(note.pitch.as_u8()))
            .sum::<f32>()
            / engraved.notes.len() as f32
    };
    let single_bass = average_pitch < 52.0;
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
            "0.98 0.97 0.94 rg 0 0 {w} {h} re f 0.15 0.15 0.15 rg 0.2 0.2 0.2 RG 0.7 w",
            w = settings.page_width,
            h = settings.page_height
        );
        write_text(
            &mut content,
            12.0,
            settings.margin,
            settings.page_height - settings.margin,
            title,
            unicode_font.as_mut(),
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
                let grand = engraved.staves == 2;
                let staff_lines = if engraved.percussion { 1 } else { 5 };
                let bottom = if grand {
                    top + 10.0 * settings.staff_space
                } else {
                    top + (staff_lines - 1) as f32 * settings.staff_space
                };
                let mut line_offsets = Vec::new();
                if grand {
                    line_offsets.extend((0..5).map(|line| line as f32));
                    line_offsets.extend((6..11).map(|line| line as f32));
                } else {
                    line_offsets.extend((0..staff_lines).map(|line| line as f32));
                }
                for line in line_offsets {
                    let line_y = if engraved.percussion {
                        top - 2.0 * settings.staff_space
                    } else {
                        top + line * settings.staff_space
                    };
                    let y = settings.page_height - line_y;
                    let _ = writeln!(
                        content,
                        "{:.1} {:.1} m {:.1} {:.1} l S",
                        settings.margin,
                        y,
                        settings.page_width - settings.margin,
                        y
                    );
                }
                if !engraved.percussion {
                    let treble = if engraved.staves == 2 {
                        true
                    } else {
                        !single_bass
                    };
                    let bass = if engraved.staves == 2 {
                        true
                    } else {
                        single_bass
                    };
                    let space = settings.staff_space;
                    if treble {
                        let glyph = clef::treble_glyph();
                        draw_image(
                            &mut content,
                            "ClefTreble",
                            settings.margin - 42.0,
                            system.y + (3.0 + glyph.top) * space,
                            glyph.height * space,
                            &glyph.image,
                            settings.page_height,
                        );
                    }
                    if bass {
                        let glyph = clef::bass_glyph();
                        let offset = if engraved.staves == 2 {
                            6.0 * space
                        } else {
                            0.0
                        };
                        draw_image(
                            &mut content,
                            "ClefBass",
                            settings.margin - 42.0,
                            system.y + offset + (1.0 + glyph.top) * space,
                            glyph.height * space,
                            &glyph.image,
                            settings.page_height,
                        );
                    }
                }
                let signature_x = settings.margin - 4.0;
                let numerator_y = settings.page_height - (top + settings.staff_space);
                let denominator_y = settings.page_height - (top + 3.0 * settings.staff_space);
                let _ = writeln!(
                    content,
                    "BT /F1 10 Tf {signature_x:.1} {numerator_y:.1} Td ({}) Tj ET",
                    engraved.time_signature.numerator
                );
                let _ = writeln!(
                    content,
                    "BT /F1 10 Tf {signature_x:.1} {denominator_y:.1} Td ({}) Tj ET",
                    engraved.time_signature.denominator
                );
                if engraved.staves == 2 {
                    let x = settings.margin - 10.0;
                    let _ = writeln!(
                        content,
                        "{x:.1} {:.1} m {x:.1} {:.1} l S",
                        settings.page_height - top,
                        settings.page_height - bottom
                    );
                }
                for measure in &system.measures {
                    let _ = writeln!(
                        content,
                        "BT /F1 9 Tf {:.1} {:.1} Td ({}) Tj ET",
                        measure.x + 2.0,
                        settings.page_height - (top - 12.0),
                        measure.number
                    );
                    for x in [measure.x, measure.x_end] {
                        let bar_top = if engraved.percussion {
                            top - 2.0 * settings.staff_space - 0.5
                        } else {
                            top - 0.5
                        };
                        let bar_bottom = if engraved.percussion {
                            top - 2.0 * settings.staff_space + 0.5
                        } else {
                            bottom + 0.5
                        };
                        let _ = writeln!(
                            content,
                            "{x:.1} {:.1} m {x:.1} {:.1} l S",
                            settings.page_height - bar_top,
                            settings.page_height - bar_bottom
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
                write_text(
                    &mut content,
                    9.0,
                    lyric.x - 6.0,
                    settings.page_height - lyric.y,
                    &lyric.text,
                    unicode_font.as_mut(),
                );
            }
            for symbol in engraved
                .chord_symbols
                .iter()
                .filter(|symbol| symbol.page == index)
            {
                write_text(
                    &mut content,
                    10.0,
                    symbol.x,
                    settings.page_height - symbol.y,
                    &symbol.name,
                    unicode_font.as_mut(),
                );
            }
            for frame in engraved.frames.iter().filter(|frame| frame.page == index) {
                let width = 2.8 * settings.staff_space;
                let height = 2.6 * settings.staff_space;
                let left = frame.x;
                let bottom = settings.page_height - frame.y;
                let top = bottom + height;
                for string in 0..6 {
                    let x = left + width * string as f32 / 5.0;
                    let _ = writeln!(content, "{x:.1} {bottom:.1} m {x:.1} {top:.1} l S");
                }
                for fret in 0..=5 {
                    let y = top - height * fret as f32 / 5.0;
                    let _ = writeln!(content, "{left:.1} {y:.1} m {:.1} {y:.1} l S", left + width);
                }
                let _ = writeln!(
                    content,
                    "BT /F1 8 Tf {left:.1} {:.1} Td ({}) Tj ET",
                    top + 2.0,
                    escape(&frame.name)
                );
                if frame.base_fret > 1 {
                    let _ = writeln!(
                        content,
                        "BT /F1 7 Tf {:.1} {top:.1} Td ({base}fr) Tj ET",
                        left - 12.0,
                        base = frame.base_fret
                    );
                }
                for (string, fret) in frame.frets.iter().enumerate() {
                    let x = left + width * string as f32 / 5.0;
                    match fret {
                        None => {
                            let _ = writeln!(
                                content,
                                "{a:.1} {b:.1} m {c:.1} {d:.1} l S {c:.1} {b:.1} m {a:.1} {d:.1} l S",
                                a = x - 2.0,
                                b = top + 2.0,
                                c = x + 2.0,
                                d = top + 6.0
                            );
                        }
                        Some(0) => circle(&mut content, x, top + 4.0, 1.6),
                        Some(value) if *value >= frame.base_fret => {
                            let row = (*value - frame.base_fret) as f32 + 0.5;
                            let y = top - height * row / 5.0;
                            circle(&mut content, x, y, 1.8);
                        }
                        Some(_) => {}
                    }
                }
            }
            for slur in engraved.slurs.iter().filter(|slur| slur.page == index) {
                let x1 = slur.start_x;
                let x2 = slur.end_x;
                let y = settings.page_height - (slur.y - 2.0);
                let control = settings.page_height - (slur.y - 14.0);
                let mid = (x1 + x2) / 2.0;
                let _ = writeln!(
                    content,
                    "{x1:.1} {y:.1} m {a:.1} {control:.1} {b:.1} {control:.1} {x2:.1} {y:.1} c S",
                    a = mid - (x2 - x1) / 6.0,
                    b = mid + (x2 - x1) / 6.0,
                );
            }
            if engraved.pages.len() > 1 {
                let label = format!("{}", index + 1);
                let width = label.len() as f32 * 4.5;
                let _ = writeln!(
                    content,
                    "BT /F1 8 Tf {:.1} 20 Td ({}) Tj ET",
                    settings.page_width / 2.0 - width / 2.0,
                    label
                );
            }
            for rest in engraved.rests.iter().filter(|rest| rest.page == index) {
                let (width, height) = match rest.kind {
                    crate::RestKind::Whole | crate::RestKind::Half => {
                        (settings.staff_space * 0.9, settings.staff_space * 0.45)
                    }
                    crate::RestKind::Quarter => {
                        (settings.staff_space * 0.45, settings.staff_space * 0.9)
                    }
                    crate::RestKind::Eighth => {
                        (settings.staff_space * 0.35, settings.staff_space * 0.7)
                    }
                };
                let x = rest.x - width / 2.0;
                let y = settings.page_height - rest.y - height / 2.0;
                let _ = writeln!(
                    content,
                    "0.15 0.15 0.15 rg {x:.1} {y:.1} {width:.1} {height:.1} re f"
                );
                let dot_center = engraved
                    .system_for(index, rest.measure)
                    .map_or(rest.y, |system| {
                        crate::dot_y(rest.y, system.y, settings.staff_space)
                    });
                for dot in 0..rest.dots {
                    circle(
                        &mut content,
                        x + width + 1.2 + f32::from(dot) * 2.2,
                        settings.page_height - dot_center,
                        1.0,
                    );
                }
            }
            for tie in engraved.ties.iter().filter(|tie| tie.page == index) {
                let x1 = tie.start_x;
                let x2 = tie.end_x;
                let y = settings.page_height - (tie.y - 3.0);
                let control = settings.page_height - (tie.y - 9.0);
                let mid = (x1 + x2) / 2.0;
                let _ = writeln!(
                    content,
                    "{x1:.1} {y:.1} m {a:.1} {control:.1} {b:.1} {control:.1} {x2:.1} {y:.1} c S",
                    a = mid - (x2 - x1) / 6.0,
                    b = mid + (x2 - x1) / 6.0,
                );
            }
            for beam in engraved.beams.iter().filter(|beam| beam.page == index) {
                let y = settings.page_height - beam.y;
                let _ = writeln!(
                    content,
                    "{:.1} {y:.1} m {:.1} {y:.1} l S",
                    beam.start_x, beam.end_x
                );
                if beam.sixteenth {
                    let second = y - 2.4 * settings.staff_space;
                    let _ = writeln!(
                        content,
                        "{:.1} {second:.1} m {:.1} {second:.1} l S",
                        beam.start_x, beam.end_x
                    );
                }
            }
            for tuplet in engraved
                .tuplets
                .iter()
                .filter(|tuplet| tuplet.page == index)
            {
                let y = settings.page_height - tuplet.y;
                let _ = writeln!(
                    content,
                    "{x1:.1} {y:.1} m {x2:.1} {y:.1} l {x1:.1} {y2:.1} l S",
                    x1 = tuplet.start_x,
                    x2 = tuplet.end_x,
                    y2 = y + 2.0
                );
                let mid = (tuplet.start_x + tuplet.end_x) / 2.0;
                let _ = writeln!(
                    content,
                    "BT /F1 8 Tf {mid:.1} {text_y:.1} Td ({count}) Tj ET",
                    text_y = y + 3.0,
                    count = tuplet.count
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
                if engraved.percussion {
                    let _ = writeln!(
                        content,
                        "{a:.1} {b:.1} m {c:.1} {d:.1} l S {c:.1} {b:.1} m {a:.1} {d:.1} l S",
                        a = x - radius,
                        b = y - radius,
                        c = x + radius,
                        d = y + radius
                    );
                    continue;
                }
                if let Some(alter) = note.spelled.accidental {
                    accidental(
                        &mut content,
                        alter,
                        x - radius * 2.6 - note.accidental_shift,
                        y,
                        radius,
                    );
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
                let dot_center = engraved
                    .system_for(index, note.measure)
                    .map_or(note.y, |system| {
                        crate::dot_y(note.y, system.y, settings.staff_space)
                    });
                for dot in 0..note.dots {
                    circle(
                        &mut content,
                        x + radius * (1.9 + f32::from(dot) * 1.3),
                        settings.page_height - dot_center,
                        radius * 0.34,
                    );
                }
            }
        }
        let font_res = if unicode_font.is_some() {
            format!("/F1 3 0 R /F2 {unicode_font_id} 0 R")
        } else {
            String::from("/F1 3 0 R")
        };
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /Font << {font_res} >> /XObject << /ClefTreble {treble_id} 0 R /ClefBass {bass_id} 0 R >> >> /Contents {contents} 0 R >>",
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

    let treble = clef::treble();
    objects.push(image_stream(treble, treble_mask));
    objects.push(alpha_stream(treble));
    let bass = clef::bass();
    objects.push(image_stream(bass, bass_mask));
    objects.push(alpha_stream(bass));
    if let Some(font) = unicode_font.as_ref() {
        objects.extend(font.build_pdf_objects(unicode_font_id));
    }

    finish_pdf(objects, title)
}

/// Writes an all-track full score to PDF bytes.
///
/// One staff per track with shared barlines, laid out by [`crate::full`] like
/// the desktop's all-tracks score.
#[must_use]
pub fn to_full_pdf(
    song: &project::Song,
    settings: Settings,
    title: &str,
    track_ids: &[project::TrackId],
) -> Vec<u8> {
    to_full_pdf_with_frames(song, settings, title, track_ids, false)
}

/// Writes an all-track full score, optionally with guitar chord frames.
#[must_use]
pub fn to_full_pdf_with_frames(
    song: &project::Song,
    settings: Settings,
    title: &str,
    track_ids: &[project::TrackId],
    frames: bool,
) -> Vec<u8> {
    to_full_pdf_with_options(song, settings, title, track_ids, frames, None)
}

/// Writes an all-track full score with optional guitar chord frames and an
/// optional explicit TrueType Unicode/CJK font path.
#[must_use]
pub fn to_full_pdf_with_options(
    song: &project::Song,
    settings: Settings,
    title: &str,
    track_ids: &[project::TrackId],
    frames: bool,
    font_path: Option<&Path>,
) -> Vec<u8> {
    let tracks: Vec<(String, Engraved)> = track_ids
        .iter()
        .filter_map(|id| {
            let track = song.tracks.iter().find(|track| track.id == *id)?;
            Some((track.name.clone(), crate::engrave(song, *id, settings)))
        })
        .collect();
    let full = FullScore::new(&tracks, frames, &song.markers, settings);
    let page_count = full.page_count();
    let treble_id = 4 + page_count * 2;
    let treble_mask = treble_id + 1;
    let bass_id = treble_id + 2;
    let bass_mask = bass_id + 1;
    let unicode_font_id = bass_mask + 1;
    let needed = collect_non_winansi_chars(
        std::iter::once(title)
            .chain(tracks.iter().map(|(name, _)| name.as_str()))
            .chain(song.markers.iter().map(|marker| marker.name.as_str()))
            .chain(song.lyrics.iter().map(|lyric| lyric.text.as_str()))
            .chain(tracks.iter().flat_map(|(_, engraved)| {
                engraved
                    .chord_symbols
                    .iter()
                    .map(|chord| chord.name.as_str())
            })),
    );
    let mut unicode_font = if needed.is_empty() {
        None
    } else {
        UnicodePdfFont::resolve(font_path, &needed)
    };
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
    let bpm = song.tempo_map.bpm_at(0);
    for page_index in 0..page_count {
        let canvas = PdfCanvas {
            content: RefCell::new(format!(
                "0.957 0.945 0.918 rg 0 0 {w} {h} re f 1 J 1 j\n",
                w = settings.page_width,
                h = settings.page_height
            )),
            page_height: settings.page_height,
            font: RefCell::new(unicode_font.as_mut()),
        };
        full.draw_page(&canvas, page_index, title, bpm, None, frames, &song.markers);
        let content = canvas.content.into_inner();
        let contents = 5 + page_index * 2;
        let font_res = if unicode_font.is_some() {
            format!("/F1 3 0 R /F2 {unicode_font_id} 0 R")
        } else {
            String::from("/F1 3 0 R")
        };
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /Font << {font_res} >> /XObject << /ClefTreble {treble_id} 0 R /ClefBass {bass_id} 0 R >> >> /Contents {contents} 0 R >>",
                w = settings.page_width,
                h = settings.page_height
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
    let treble = clef::treble();
    objects.push(image_stream(treble, treble_mask));
    objects.push(alpha_stream(treble));
    let bass = clef::bass();
    objects.push(image_stream(bass, bass_mask));
    objects.push(alpha_stream(bass));
    if let Some(font) = unicode_font.as_ref() {
        objects.extend(font.build_pdf_objects(unicode_font_id));
    }
    finish_pdf(objects, title)
}

/// Draws the all-tracks score into one page's content stream.
struct PdfCanvas<'f> {
    content: RefCell<String>,
    page_height: f32,
    font: RefCell<Option<&'f mut UnicodePdfFont>>,
}

impl PdfCanvas<'_> {
    fn colour(ink: Ink) -> &'static str {
        match ink {
            Ink::Black => "0.08 0.08 0.08",
            Ink::Grey => "0.31 0.31 0.31",
            Ink::Paper => "0.957 0.945 0.918",
            Ink::Playhead => "0.82 0.19 0.19",
        }
    }

    fn path(&self, points: &[Point]) -> String {
        let mut path = String::new();
        for (index, (x, y)) in points.iter().enumerate() {
            let op = if index == 0 { "m" } else { "l" };
            let _ = write!(path, "{x:.2} {:.2} {op} ", self.page_height - y);
        }
        path
    }
}

impl Canvas for PdfCanvas<'_> {
    fn line(&self, from: Point, to: Point, width: f32, ink: Ink) {
        let path = self.path(&[from, to]);
        let _ = writeln!(
            self.content.borrow_mut(),
            "{} RG {width:.2} w {path}S",
            Self::colour(ink)
        );
    }

    fn polyline(&self, points: &[Point], width: f32) {
        let path = self.path(points);
        let _ = writeln!(
            self.content.borrow_mut(),
            "{} RG {width:.2} w {path}S",
            Self::colour(Ink::Black)
        );
    }

    fn polygon(&self, points: &[Point], fill: Ink, outline: Option<f32>) {
        let path = self.path(points);
        let mut content = self.content.borrow_mut();
        match outline {
            Some(width) => {
                let _ = writeln!(
                    content,
                    "{} rg {} RG {width:.2} w {path}h B",
                    Self::colour(fill),
                    Self::colour(Ink::Black)
                );
            }
            None => {
                let _ = writeln!(content, "{} rg {path}h f", Self::colour(fill));
            }
        }
    }

    fn circle(&self, centre: Point, radius: f32) {
        let mut content = self.content.borrow_mut();
        let _ = write!(content, "{} rg ", Self::colour(Ink::Black));
        circle(&mut content, centre.0, self.page_height - centre.1, radius);
    }

    fn fill_rect(&self, min: Point, max: Point) {
        let _ = writeln!(
            self.content.borrow_mut(),
            "{} rg {:.2} {:.2} {:.2} {:.2} re f",
            Self::colour(Ink::Black),
            min.0,
            self.page_height - max.1,
            max.0 - min.0,
            max.1 - min.1
        );
    }

    fn stroke_rect(&self, min: Point, max: Point, width: f32) {
        let _ = writeln!(
            self.content.borrow_mut(),
            "{} RG {width:.2} w {:.2} {:.2} {:.2} {:.2} re S",
            Self::colour(Ink::Black),
            min.0,
            self.page_height - max.1,
            max.0 - min.0,
            max.1 - min.1
        );
    }

    fn text(&self, at: Point, anchor: Anchor, text: &str, size: f32, ink: Ink) {
        let x = at.0 - anchor.x * self.text_width(text, size);
        let baseline = at.1 + (0.72 - anchor.y * 0.92) * size;
        let mut content = self.content.borrow_mut();
        let _ = write!(content, "{} rg ", Self::colour(ink));
        let mut font = self.font.borrow_mut();
        write_text(
            &mut content,
            size,
            x,
            self.page_height - baseline,
            text,
            font.as_deref_mut(),
        );
    }

    fn text_width(&self, text: &str, size: f32) -> f32 {
        text.chars()
            .map(|ch| match ch {
                'i' | 'l' | 'j' | '.' | ',' | ':' | ';' | '|' | '!' | '\'' | ' ' => 0.28,
                'm' | 'w' | 'M' | 'W' => 0.83,
                'A'..='Z' | '0'..='9' => 0.64,
                ch if ch.is_control() => 0.0,
                // CJK and other wide glyphs fill a full em.
                ch if u32::from(ch) >= 0x1100 => 1.0,
                _ => 0.55,
            })
            .sum::<f32>()
            * size
    }

    fn clef(&self, treble: bool, min: Point, max: Point) {
        let name = if treble { "ClefTreble" } else { "ClefBass" };
        let _ = writeln!(
            self.content.borrow_mut(),
            "q {:.2} 0 0 {:.2} {:.2} {:.2} cm /{name} Do Q",
            max.0 - min.0,
            max.1 - min.1,
            min.0,
            self.page_height - max.1
        );
    }
}

fn finish_pdf(mut objects: Vec<Vec<u8>>, title: &str) -> Vec<u8> {
    // The Info dictionary is PDFDocEncoding, not WinAnsi, so only plain
    // ASCII goes in as a literal string; anything else is UTF-16.
    let title_pdf = if title.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) {
        format!("({})", escape(title))
    } else {
        pdf_utf16_hex_string(title)
    };
    objects.push(format!("<< /Title {title_pdf} /Producer (Lazy Arranger) >>").into_bytes());
    let info = objects.len();
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
            "trailer\n<< /Size {} /Root 1 0 R /Info {info} 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn draw_image(
    content: &mut String,
    name: &str,
    x: f32,
    top: f32,
    height: f32,
    image: &ClefImage,
    page_height: f32,
) {
    let width = height * image.width as f32 / image.height as f32;
    let lower = page_height - (top + height);
    let _ = writeln!(
        content,
        "q {width:.2} 0 0 {height:.2} {x:.2} {lower:.2} cm /{name} Do Q"
    );
}

fn image_stream(image: &ClefImage, mask: usize) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(image.rgba.len() / 4 * 3);
    for pixel in image.rgba.as_chunks::<4>().0 {
        rgb.extend_from_slice(&pixel[..3]);
    }
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6);
    let mut stream = format!(
        "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /SMask {mask} 0 R /Filter /FlateDecode /Length {} >>\nstream\n",
        image.width,
        image.height,
        compressed.len()
    )
    .into_bytes();
    stream.extend_from_slice(&compressed);
    stream.extend_from_slice(b"\nendstream");
    stream
}

fn alpha_stream(image: &ClefImage) -> Vec<u8> {
    let mut alpha = Vec::with_capacity(image.rgba.len() / 4);
    for pixel in image.rgba.as_chunks::<4>().0 {
        alpha.push(pixel[3]);
    }
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&alpha, 6);
    let mut stream = format!(
        "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
        image.width,
        image.height,
        compressed.len()
    )
    .into_bytes();
    stream.extend_from_slice(&compressed);
    stream.extend_from_slice(b"\nendstream");
    stream
}

fn accidental(content: &mut String, alter: i8, x: f32, y: f32, radius: f32) {
    let _ = writeln!(content, "q {} w", radius * 0.22);
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
    let _ = writeln!(content, "Q");
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

fn pdf_encodable(character: char) -> bool {
    character.is_ascii()
        || u32::from(character) <= 0xFF
        || matches!(
            character,
            '\u{2013}'
                | '\u{2014}'
                | '\u{2018}'
                | '\u{2019}'
                | '\u{201c}'
                | '\u{201d}'
                | '\u{2026}'
        )
}

/// The first title, track, lyric or marker text the WinAnsi PDF cannot encode.
#[must_use]
pub fn unencodable_text(song: &project::Song, title: &str) -> Option<String> {
    let mut texts = vec![title];
    texts.extend(song.tracks.iter().map(|track| track.name.as_str()));
    texts.extend(song.lyrics.iter().map(|lyric| lyric.text.as_str()));
    texts.extend(song.markers.iter().map(|marker| marker.name.as_str()));
    texts
        .into_iter()
        .find(|text| !text.chars().all(pdf_encodable))
        .map(str::to_string)
}

fn escape(text: &str) -> String {
    // The PDF uses Helvetica with WinAnsiEncoding. Encode representable
    // characters as octal bytes and replace the rest rather than emitting
    // corrupt UTF-8.
    let mut out = String::new();
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\n' | '\r' | '\t' => out.push(' '),
            character if character.is_control() => {}
            '\u{2013}' => out.push_str("\\226"),
            '\u{2014}' => out.push_str("\\227"),
            // WinAnsi: 0x91 ‘, 0x92 ’, 0x93 “, 0x94 ”.
            '\u{2018}' => out.push_str("\\221"),
            '\u{2019}' => out.push_str("\\222"),
            '\u{201c}' => out.push_str("\\223"),
            '\u{201d}' => out.push_str("\\224"),
            '\u{2026}' => out.push_str("\\205"),
            character if character.is_ascii() => out.push(character),
            character if u32::from(character) <= 0xFF => {
                out.push_str(&format!("\\{:03o}", u32::from(character)));
            }
            _ => out.push('?'),
        }
    }
    out
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
    use music_core::{Channel, PPQ, Pitch, PitchClass, Velocity};
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
            grand_staff: false,
            measure_gap: 8.0,
        }
    }

    #[test]
    fn text_is_encoded_for_win_ansi() {
        assert_eq!(escape("café"), "caf\\351");
        assert_eq!(escape("a–b"), "a\\226b");
        assert_eq!(escape("日本"), "??");
        assert_eq!(escape("a(b)"), "a\\(b\\)");
    }

    #[test]
    fn a_full_score_pdf_lists_every_track() {
        let mut song = Song::new("Full", 120.0);
        song.key = Some(project::song::KeySignature {
            tonic: PitchClass::new(0),
            minor: false,
        });
        song.chords.push(project::song::ChordEvent {
            tick: 0,
            chord: music_core::Chord::parse("C").expect("chord"),
        });
        song.lyrics.push(project::song::LyricEvent {
            tick: 0,
            text: String::from("la"),
        });
        song.marks.push(project::song::Mark {
            tick: 0,
            kind: project::song::MarkKind::Dynamic(project::song::Dynamic::F),
        });
        song.markers.push(project::Marker {
            tick: 0,
            name: String::from("Intro"),
        });
        for (index, name) in [(1_u32, "Piano"), (2, "Bass")] {
            let mut track = Track::new(TrackId::new(index).expect("valid"), name);
            track.notes.push(Note {
                id: project::NoteId::new(index).expect("valid"),
                start: 0,
                duration: PPQ,
                pitch: Pitch::from_name(if index == 1 { "C4" } else { "C#4" }).expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
            if index == 1 {
                track.notes.push(Note {
                    id: project::NoteId::new(10).expect("valid"),
                    start: PPQ,
                    duration: PPQ,
                    pitch: Pitch::from_name("E4").expect("valid"),
                    velocity: Velocity::new(100).expect("valid"),
                    channel: Channel::first(),
                });
                track.slurs.push(project::song::Slur { start: 0, end: PPQ });
            }
            song.tracks.push(track);
        }
        let tracks = vec![
            TrackId::new(1).expect("valid"),
            TrackId::new(2).expect("valid"),
        ];
        let pdf = to_full_pdf(&song, small(), "Full score", &tracks);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("(Piano) Tj"));
        assert!(text.contains("(Bass) Tj"));
        assert!(text.contains("(C) Tj"));
        assert!(text.contains("(= 120) Tj"), "tempo mark");
        assert!(text.contains("/Title (Full score)"), "PDF metadata title");
        assert!(text.contains("(la) Tj"));
        assert!(text.contains("(f) Tj"));
        assert!(text.contains("(Intro) Tj"), "section marker");
        assert!(text.contains("/ClefTreble"), "clef image resource");
        let mut unslurred = song.clone();
        unslurred.tracks[0].slurs.clear();
        let plain = to_full_pdf(&unslurred, small(), "Full score", &tracks);
        assert!(
            text.matches(" l ").count() > String::from_utf8_lossy(&plain).matches(" l ").count(),
            "slur arc"
        );
        assert!(page_count(&pdf).is_some_and(|count| count >= 1));
        let one = to_full_pdf(&song, small(), "One", &[TrackId::new(1).expect("valid")]);
        let text = String::from_utf8_lossy(&one);
        assert!(text.contains("(Piano) Tj"));
        assert!(!text.contains("(Bass) Tj"));
        let framed = to_full_pdf_with_frames(&song, small(), "Framed", &tracks, true);
        let framed = String::from_utf8_lossy(&framed);
        assert!(framed.contains("(3) Tj"), "guitar frame fret");
    }

    #[test]
    fn a_full_pdf_draws_each_chord_once() {
        use project::song::{ChordEvent, Note, Song, Track};
        let mut song = Song::new("Chords", 120.0);
        let mut track = Track::new(project::TrackId::new(1).expect("valid"), "Piano");
        for bar in 0..8 {
            track.notes.push(Note {
                id: project::NoteId::new(bar + 1).expect("valid"),
                start: music_core::PPQ * 4 * i64::from(bar),
                duration: music_core::PPQ * 4,
                pitch: music_core::Pitch::from_name("C4").expect("valid"),
                velocity: music_core::Velocity::new(100).expect("valid"),
                channel: music_core::Channel::first(),
            });
            song.chords.push(ChordEvent {
                tick: music_core::PPQ * 4 * i64::from(bar),
                chord: music_core::Chord::parse("C").expect("valid"),
            });
        }
        song.tracks.push(track);
        let pdf = to_full_pdf(
            &song,
            Settings::default(),
            "Chords",
            &[project::TrackId::new(1).expect("valid")],
        );
        let text = String::from_utf8_lossy(&pdf);
        assert_eq!(text.matches("(C) Tj").count(), 8, "one chord per bar");
    }

    #[test]
    fn unsupported_pdf_text_is_reported() {
        let mut song = Song::new("Text", 120.0);
        song.lyrics.push(project::song::LyricEvent {
            tick: 0,
            text: String::from("日本語"),
        });
        assert_eq!(unencodable_text(&song, "Title").as_deref(), Some("日本語"));
        song.lyrics[0].text = String::from("café");
        assert!(unencodable_text(&song, "Title").is_none());
    }

    #[test]
    fn a_long_full_score_fills_pages_with_systems() {
        let mut song = Song::new("Long", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for bar in 0..16 {
            track.notes.push(Note {
                id: project::NoteId::new(bar + 1).expect("valid"),
                start: i64::from(bar) * 4 * PPQ,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        let settings = small();
        let engraved = crate::engrave(&song, TrackId::new(1).expect("valid"), settings);
        let systems = engraved
            .pages
            .iter()
            .map(|page| page.systems.len())
            .sum::<usize>()
            .max(1);
        let pdf = to_full_pdf(&song, settings, "Long", &[TrackId::new(1).expect("valid")]);
        let tracks = vec![(String::from("Piano"), engraved.clone())];
        let per_page = FullScore::new(&tracks, false, &[], settings).systems_per_page();
        assert_eq!(page_count(&pdf), Some(systems.div_ceil(per_page)));
        assert!(systems >= 2, "test song needs more than one system");
        let text = String::from_utf8_lossy(&pdf);
        assert_eq!(
            text.matches("(Piano) Tj").count(),
            systems,
            "one label per system"
        );
    }

    #[test]
    fn a_long_full_score_keeps_each_track_label_per_system() {
        let mut song = Song::new("Duo", 120.0);
        let mut piano = Track::new(TrackId::new(1).expect("valid"), "Piano");
        let mut bass = Track::new(TrackId::new(2).expect("valid"), "Bass");
        for bar in 0..16 {
            piano.notes.push(Note {
                id: project::NoteId::new(bar + 1).expect("valid"),
                start: i64::from(bar) * 4 * PPQ,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
            bass.notes.push(Note {
                id: project::NoteId::new(bar + 1).expect("valid"),
                start: i64::from(bar) * 4 * PPQ,
                duration: PPQ,
                pitch: Pitch::from_name("C2").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(piano);
        song.tracks.push(bass);
        let settings = small();
        let engraved = crate::engrave(&song, TrackId::new(1).expect("valid"), settings);
        let systems = engraved
            .pages
            .iter()
            .map(|page| page.systems.len())
            .sum::<usize>()
            .max(1);
        let pdf = to_full_pdf(
            &song,
            settings,
            "Duo",
            &[
                TrackId::new(1).expect("valid"),
                TrackId::new(2).expect("valid"),
            ],
        );
        let text = String::from_utf8_lossy(&pdf);
        assert_eq!(text.matches("(Piano) Tj").count(), systems);
        assert_eq!(text.matches("(Bass) Tj").count(), systems);
        let tracks = vec![
            (String::from("Piano"), engraved.clone()),
            (
                String::from("Bass"),
                crate::engrave(&song, TrackId::new(2).expect("valid"), settings),
            ),
        ];
        let per_page = FullScore::new(&tracks, false, &[], settings).systems_per_page();
        assert_eq!(page_count(&pdf), Some(systems.div_ceil(per_page)));
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
        assert!(text.contains("/Title (Test score)"), "PDF metadata title");
        assert!(text.contains("(4) Tj"), "time signature digits");
        assert!(text.contains("(1) Tj"), "measure number");
        assert!(text.contains("/ClefTreble"), "treble clef image object");
        assert_eq!(text.matches(" 20 Td").count(), 2, "page numbers");
        assert!(text.contains("/ClefBass"), "bass clef image object");
        assert!(!text.contains(" C? "), "no music glyphs from base fonts");
    }

    #[test]
    fn chord_symbols_use_the_dark_fill_and_fit_on_the_page() {
        let settings = Settings::default();
        let bytes = to_pdf(&engraved(1, settings), settings, "Chord");
        let pdf = String::from_utf8_lossy(&bytes);
        let dark = pdf.find("0.15 0.15 0.15 rg").expect("dark fill");
        let chord = pdf.find("(C) Tj").expect("chord text");
        assert!(dark < chord, "the dark fill is set before the chord text");
        let line = pdf[..chord].lines().last().expect("line");
        let y: f32 = line
            .split_whitespace()
            .nth(5)
            .expect("y coordinate")
            .parse()
            .expect("number");
        assert!(
            (0.0..=settings.page_height).contains(&y),
            "chord y {y} on page"
        );
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
    fn a_grand_staff_page_draws_extra_lines_and_a_brace() {
        let settings = Settings {
            page_width: 320.0,
            page_height: 200.0,
            margin: 20.0,
            staff_space: 6.0,
            system_gap: 60.0,
            quarter_width: 20.0,
            measure_gap: 8.0,
            grand_staff: true,
        };
        let engraved = engraved(1, settings);
        let pdf = String::from_utf8_lossy(&to_pdf(&engraved, settings, "Grand")).to_string();
        let top = 20.0 + 4.0 * 6.0;
        let bottom = top + 10.0 * 6.0;
        let low = 200.0 - bottom;
        assert!(
            pdf.contains(&format!("20.0 {low:.1} m 300.0 {low:.1} l S")),
            "the lowest bass-staff line is stroked"
        );
        assert!(
            pdf.contains(&format!(
                "20.0 {y:.1} m 300.0 {y:.1} l S",
                y = 200.0 - (top + 6.0 * 6.0)
            )),
            "the highest bass-staff line is stroked"
        );
        assert!(
            pdf.contains(&format!("10.0 {:.1} m 10.0 {:.1} l S", 200.0 - top, low)),
            "a brace joins the staves"
        );
    }

    #[test]
    fn empty_documents_still_write_one_page() {
        let pdf = to_pdf(&engraved(0, small()), small(), "Empty");
        assert_eq!(page_count(&pdf), Some(1));
    }

    #[test]
    fn unicode_vietnamese_and_cjk_text_embeds_type0_font_when_available() {
        let latin = to_pdf(&engraved(1, small()), small(), "Plain Title");
        assert!(
            !String::from_utf8_lossy(&latin).contains("/Subtype /Type0"),
            "Latin-1 scores should stay lightweight without embedding Type0"
        );
        let title = "Tiếng Việt · 夜の星 · 한국어";
        let pdf = to_pdf(&engraved(1, small()), small(), title);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Title <FEFF"));
        let chars: Vec<char> = title.chars().filter(|ch| !pdf_encodable(*ch)).collect();
        if UnicodePdfFont::resolve(None, &chars).is_some() {
            assert!(text.contains("/Subtype /Type0"));
            assert!(text.contains("/Subtype /CIDFontType2"));
            assert!(text.contains("/ToUnicode "));
            assert!(text.contains("/F2 12 Tf"));
            assert!(!text.contains("(???)"));
        }
    }

    #[test]
    fn full_score_unicode_pdf_embeds_vietnamese_and_cjk_lyrics_and_markers() {
        let mut song = project::Song::new("Đêm Khuya 夜の星", 120.0);
        let mut track =
            project::song::Track::new(project::TrackId::new(1).expect("id"), "Đàn Tranh 琴");
        track.notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("note"),
            start: 0,
            duration: music_core::PPQ,
            pitch: music_core::Pitch::from_name("C4").expect("pitch"),
            velocity: music_core::Velocity::new(100).expect("vel"),
            channel: music_core::Channel::first(),
        });
        song.tracks.push(track);
        song.lyrics.push(project::song::LyricEvent {
            tick: 0,
            text: String::from("Trăng sáng 月"),
        });
        song.markers.push(project::ids::Marker {
            tick: 0,
            name: String::from("Đoạn A 第一部"),
        });
        let ids: Vec<_> = song.tracks.iter().map(|t| t.id).collect();
        let pdf = to_full_pdf(&song, small(), &song.name, &ids);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Title <FEFF"));
        let chars: Vec<char> = "Đêm Khuya 夜の星 Trăng sáng 月 Đoạn A 第一部"
            .chars()
            .filter(|ch| !pdf_encodable(*ch))
            .collect();
        if UnicodePdfFont::resolve(None, &chars).is_some() {
            assert!(text.contains("/Subtype /Type0"));
            assert!(text.contains("/ToUnicode "));
            assert!(text.contains("/F2 16 Tf"), "title");
            assert!(text.matches("/F2 ").count() >= 3, "name, lyric and marker");
        }
    }
}
