//! Vector PDF export over the engraving layout (ADR-010).
//!
//! A small PDF 1.4 writer: catalog, page tree, a Helvetica font for text and a
//! content stream per page. Staff lines, barlines and stems are stroked lines,
//! noteheads are filled circles, and accidentals are drawn as paths because the
//! base-14 fonts carry no music sharp or flat.

use std::fmt::Write as _;

use crate::clef::{self, ClefImage};
use crate::{Engraved, Settings};

/// Writes the engraved document as PDF bytes.
#[must_use]
pub fn to_pdf(engraved: &Engraved, settings: Settings, title: &str) -> Vec<u8> {
    let page_count = engraved.pages.len().max(1);
    let treble_id = 4 + page_count * 2;
    let treble_mask = treble_id + 1;
    let bass_id = treble_id + 2;
    let bass_mask = bass_id + 1;
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
                        let image = clef::treble();
                        draw_image(
                            &mut content,
                            "ClefTreble",
                            settings.margin - 42.0,
                            system.y - 2.0 * space,
                            7.0 * space,
                            image,
                            settings.page_height,
                        );
                    }
                    if bass {
                        let image = clef::bass();
                        let offset = if engraved.staves == 2 {
                            6.0 * space
                        } else {
                            0.0
                        };
                        draw_image(
                            &mut content,
                            "ClefBass",
                            settings.margin - 48.0,
                            system.y + offset - 0.3 * space,
                            3.2 * space,
                            image,
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
                let _ = writeln!(
                    content,
                    "BT /F1 9 Tf {x:.1} {y:.1} Td ({text}) Tj ET",
                    x = lyric.x - 6.0,
                    y = settings.page_height - lyric.y,
                    text = escape(&lyric.text)
                );
            }
            for symbol in engraved
                .chord_symbols
                .iter()
                .filter(|symbol| symbol.page == index)
            {
                let _ = writeln!(
                    content,
                    "BT /F1 10 Tf {:.1} {:.1} Td ({}) Tj ET",
                    symbol.x,
                    settings.page_height - symbol.y,
                    escape(&symbol.name)
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
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /Font << /F1 3 0 R >> /XObject << /ClefTreble {treble_id} 0 R /ClefBass {bass_id} 0 R >> >> /Contents {contents} 0 R >>",
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

    finish_pdf(objects)
}

struct FullStaff {
    name: String,
    systems: Vec<(usize, crate::System)>,
    notes: Vec<crate::PlacedNote>,
    ties: Vec<crate::Tie>,
    tuplets: Vec<crate::Tuplet>,
    chords: Vec<crate::ChordSymbol>,
    lyrics: Vec<crate::Lyric>,
    marks: Vec<crate::PlacedMark>,
    rests: Vec<crate::PlacedRest>,
    key: crate::spelling::Key,
    time_signature: music_core::TimeSignature,
}

/// Writes an all-track full score to PDF bytes.
///
/// Each page holds one system, with every track as its own staff. Noteheads are
/// placed from each track's engraving; stems, clefs and accidentals are omitted.
/// This is a compact review score, not a replacement for the single-track
/// engraving.
#[must_use]
pub fn to_full_pdf(
    song: &project::Song,
    settings: Settings,
    title: &str,
    track_ids: &[project::TrackId],
) -> Vec<u8> {
    let mut staves: Vec<FullStaff> = Vec::new();
    for id in track_ids {
        let Some(track) = song.tracks.iter().find(|track| track.id == *id) else {
            continue;
        };
        let engraved = crate::engrave(song, *id, settings);
        let systems = engraved
            .pages
            .iter()
            .enumerate()
            .flat_map(|(page, page_data)| {
                page_data
                    .systems
                    .iter()
                    .cloned()
                    .map(move |system| (page, system))
            })
            .collect();
        staves.push(FullStaff {
            name: track.name.clone(),
            systems,
            notes: engraved.notes,
            ties: engraved.ties,
            tuplets: engraved.tuplets,
            chords: engraved.chord_symbols,
            lyrics: engraved.lyrics,
            marks: engraved.marks,
            rests: engraved.rests,
            key: engraved.key,
            time_signature: engraved.time_signature,
        });
    }
    if staves.is_empty() {
        staves.push(FullStaff {
            name: String::from("Empty"),
            systems: Vec::new(),
            notes: Vec::new(),
            ties: Vec::new(),
            tuplets: Vec::new(),
            chords: Vec::new(),
            lyrics: Vec::new(),
            marks: Vec::new(),
            rests: Vec::new(),
            key: crate::spelling::Key {
                tonic: music_core::PitchClass::new(0),
                minor: false,
            },
            time_signature: music_core::TimeSignature::default(),
        });
    }
    let page_count = staves
        .iter()
        .map(|staff| staff.systems.len())
        .max()
        .unwrap_or(0)
        .max(1);
    let treble_id = 4 + page_count * 2;
    let treble_mask = treble_id + 1;
    let bass_id = treble_id + 2;
    let bass_mask = bass_id + 1;
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
    let roster = staves.len().max(1);
    let available = (settings.page_height - 2.0 * settings.margin - 24.0).max(24.0);
    let span = (available / roster as f32).max(12.0);
    let line_gap = ((span - 4.0) / 4.0).clamp(1.5, settings.staff_space);
    for (page_index, _) in (0..page_count).enumerate() {
        let mut content = String::new();
        let _ = writeln!(
            content,
            "0.98 0.97 0.94 rg 0 0 {w} {h} re f 0.15 0.15 0.15 rg 0.2 0.2 0.2 RG 0.7 w",
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
        if let Some(staff) = staves.first() {
            let _ = writeln!(
                content,
                "BT /F1 8 Tf {x:.1} {y:.1} Td ({} {} / {}/{}) Tj ET",
                staff.key.tonic.name_sharp(),
                if staff.key.minor { "minor" } else { "major" },
                staff.time_signature.numerator,
                staff.time_signature.denominator,
                x = settings.page_width - settings.margin - 80.0,
                y = settings.page_height - settings.margin
            );
        }
        if page_count > 1 {
            let _ = writeln!(
                content,
                "BT /F1 8 Tf {x:.1} 20 Td ({} / {}) Tj ET",
                page_index + 1,
                page_count,
                x = settings.page_width / 2.0 - 16.0
            );
        }
        let first_system = staves
            .first()
            .and_then(|staff| staff.systems.get(page_index))
            .map(|(_, system)| system);
        for (staff_index, staff) in staves.iter().enumerate() {
            let Some((page, system)) = staff.systems.get(page_index) else {
                continue;
            };
            let top = settings.margin + 12.0 + staff_index as f32 * span;
            for line in 0..5 {
                let y = settings.page_height - (top + line as f32 * line_gap);
                let _ = writeln!(
                    content,
                    "{:.1} {y:.1} m {:.1} {y:.1} l S",
                    settings.margin,
                    settings.page_width - settings.margin
                );
            }
            let average = staff
                .notes
                .iter()
                .filter(|note| note.page == *page)
                .map(|note| f32::from(note.pitch.as_u8()))
                .sum::<f32>()
                / staff
                    .notes
                    .iter()
                    .filter(|note| note.page == *page)
                    .count()
                    .max(1) as f32;
            let image = if average < 52.0 {
                clef::bass()
            } else {
                clef::treble()
            };
            draw_image(
                &mut content,
                if average < 52.0 {
                    "ClefBass"
                } else {
                    "ClefTreble"
                },
                settings.margin - 20.0,
                top - 2.0 * settings.staff_space,
                7.0 * settings.staff_space,
                image,
                settings.page_height,
            );
            let _ = writeln!(
                content,
                "BT /F1 7 Tf 4 {y:.1} Td ({}) Tj ET",
                escape(&short_track_name(&staff.name)),
                y = settings.page_height - (top + 2.0 * line_gap)
            );
            for rest in staff.rests.iter().filter(|rest| rest.page == *page) {
                let x = first_system.map_or(rest.x, |sys| crate::full_score_x(system, sys, rest.x));
                let y = top + (rest.y - system.y) * (line_gap / settings.staff_space);
                let _ = writeln!(
                    content,
                    "{:.1} {:.1} m {:.1} {:.1} l S",
                    x - 3.0,
                    settings.page_height - y,
                    x + 3.0,
                    settings.page_height - y
                );
            }
            if staff_index == 0 {
                for chord in staff.chords.iter().filter(|chord| chord.page == *page) {
                    let x = first_system
                        .map_or(chord.x, |sys| crate::full_score_x(system, sys, chord.x));
                    let y = top + (chord.y - system.y) * (line_gap / settings.staff_space);
                    let _ = writeln!(
                        content,
                        "BT /F1 8 Tf {x:.1} {:.1} Td ({}) Tj ET",
                        settings.page_height - y,
                        escape(&chord.name)
                    );
                }
            }
            for note in staff.notes.iter().filter(|note| note.page == *page) {
                if note.measure + 1 < system.measures.first().map_or(0, |m| m.number)
                    || note.measure + 1 > system.measures.last().map_or(0, |m| m.number)
                {
                    continue;
                }
                let x =
                    first_system.map_or(note.x, |first| crate::full_score_x(system, first, note.x));
                let relative = (note.y - system.y) * (line_gap / settings.staff_space);
                let note_y = top + relative;
                let y = settings.page_height - note_y;
                circle(&mut content, x, y, line_gap * 0.55);
                if let Some(alter) = note.spelled.accidental {
                    let accidental = match alter {
                        1 => "#",
                        -1 => "b",
                        2 => "x",
                        -2 => "bb",
                        _ => "",
                    };
                    if !accidental.is_empty() {
                        let _ = writeln!(
                            content,
                            "BT /F1 7 Tf {:.1} {:.1} Td ({accidental}) Tj ET",
                            x - 5.5,
                            y - 2.0
                        );
                    }
                }
                let stem = if note.pitch.as_u8() < 71 {
                    note_y + 3.0 * line_gap
                } else {
                    note_y - 3.0 * line_gap
                };
                let _ = writeln!(
                    content,
                    "{:.1} {y:.1} m {:.1} {:.1} l S",
                    x,
                    x,
                    settings.page_height - stem
                );
            }
            if staff_index == 0 {
                for lyric in staff.lyrics.iter().filter(|lyric| lyric.page == *page) {
                    let x = first_system
                        .map_or(lyric.x, |sys| crate::full_score_x(system, sys, lyric.x));
                    let y = top + (lyric.y - system.y) * (line_gap / settings.staff_space);
                    let _ = writeln!(
                        content,
                        "BT /F1 7 Tf {x:.1} {:.1} Td ({}) Tj ET",
                        settings.page_height - y,
                        escape(&lyric.text)
                    );
                }
            }
            if staff_index == 0 {
                for mark in staff.marks.iter().filter(|mark| mark.page == *page) {
                    let x =
                        first_system.map_or(mark.x, |sys| crate::full_score_x(system, sys, mark.x));
                    let y = top + (mark.y - system.y) * (line_gap / settings.staff_space);
                    let text = match mark.kind {
                        project::song::MarkKind::Dynamic(dynamic) => dynamic.text().to_string(),
                        project::song::MarkKind::Articulation(articulation) => {
                            format!("{articulation:?}").to_lowercase()
                        }
                    };
                    let _ = writeln!(
                        content,
                        "BT /F1 7 Tf {x:.1} {:.1} Td ({}) Tj ET",
                        settings.page_height - y,
                        escape(&text)
                    );
                }
            }
            let mut shorts: Vec<&crate::PlacedNote> = staff
                .notes
                .iter()
                .filter(|note| note.page == *page && note.duration <= music_core::PPQ / 2)
                .collect();
            shorts.sort_by_key(|note| note.tick);
            for pair in shorts.windows(2) {
                let [first, second] = pair else {
                    continue;
                };
                if first.measure != second.measure || second.tick - first.tick > music_core::PPQ / 2
                {
                    continue;
                }
                let first_y = top + (first.y - system.y) * (line_gap / settings.staff_space);
                let second_y = top + (second.y - system.y) * (line_gap / settings.staff_space);
                let upward = first.pitch.as_u8() < 71 && second.pitch.as_u8() < 71;
                let offset = if upward {
                    3.0 * line_gap
                } else {
                    -3.0 * line_gap
                };
                let first_x =
                    first_system.map_or(first.x, |sys| crate::full_score_x(system, sys, first.x));
                let second_x =
                    first_system.map_or(second.x, |sys| crate::full_score_x(system, sys, second.x));
                let _ = writeln!(
                    content,
                    "{:.1} {:.1} m {:.1} {:.1} l S",
                    first_x,
                    settings.page_height - (first_y + offset),
                    second_x,
                    settings.page_height - (second_y + offset)
                );
                if first.duration <= music_core::PPQ / 4 && second.duration <= music_core::PPQ / 4 {
                    let _ = writeln!(
                        content,
                        "{:.1} {:.1} m {:.1} {:.1} l S",
                        first_x,
                        settings.page_height - (first_y + offset + 1.5),
                        second_x,
                        settings.page_height - (second_y + offset + 1.5)
                    );
                }
            }
            for tie in staff.ties.iter().filter(|tie| tie.page == *page) {
                let y = settings.page_height
                    - (top + (tie.y - system.y) * (line_gap / settings.staff_space));
                let start_x = first_system.map_or(tie.start_x, |sys| {
                    crate::full_score_x(system, sys, tie.start_x)
                });
                let end_x = first_system
                    .map_or(tie.end_x, |sys| crate::full_score_x(system, sys, tie.end_x));
                let control = (end_x - start_x).abs() * 0.33;
                let _ = writeln!(
                    content,
                    "{:.1} {y:.1} m {:.1} {:.1} {:.1} {:.1} {:.1} {y:.1} c S",
                    start_x,
                    start_x + control,
                    y + 3.0,
                    end_x - control,
                    y + 3.0,
                    end_x
                );
            }
            for tuplet in staff.tuplets.iter().filter(|tuplet| tuplet.page == *page) {
                let y = settings.page_height
                    - (top + (tuplet.y - system.y) * (line_gap / settings.staff_space));
                let _ = writeln!(
                    content,
                    "{:.1} {y:.1} m {:.1} {y:.1} l S {:.1} {:.1} m {:.1} {:.1} l S {:.1} {:.1} m {:.1} {:.1} l S",
                    tuplet.start_x,
                    tuplet.end_x,
                    tuplet.start_x,
                    y,
                    tuplet.start_x,
                    y + 3.0,
                    tuplet.end_x,
                    y,
                    tuplet.end_x,
                    y + 3.0
                );
                let _ = writeln!(
                    content,
                    "BT /F1 7 Tf {:.1} {:.1} Td ({} ) Tj ET",
                    (tuplet.start_x + tuplet.end_x) / 2.0,
                    y + 4.0,
                    tuplet.count
                );
            }
        }
        if let Some(staff) = staves.first()
            && let Some((_, system)) = staff.systems.get(page_index)
        {
            let top = settings.margin + 12.0;
            let bottom = top + roster as f32 * span - 4.0;
            for measure in &system.measures {
                let _ = writeln!(
                    content,
                    "BT /F1 6 Tf {:.1} {:.1} Td ({}) Tj ET",
                    measure.x + 1.0,
                    settings.page_height - (top - 5.0),
                    measure.number
                );
                for x in [measure.x, measure.x_end] {
                    let _ = writeln!(
                        content,
                        "{x:.1} {:.1} m {x:.1} {:.1} l S",
                        settings.page_height - bottom,
                        settings.page_height - top
                    );
                }
            }
        }
        let contents = 5 + page_index * 2;
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /Font << /F1 3 0 R >> /XObject << /ClefTreble {treble_id} 0 R /ClefBass {bass_id} 0 R >> >> /Contents {contents} 0 R >>",
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
    finish_pdf(objects)
}

fn finish_pdf(objects: Vec<Vec<u8>>) -> Vec<u8> {
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

fn short_track_name(name: &str) -> String {
    if name.is_empty() {
        return String::from("Track");
    }
    let mut short: String = name.chars().take(5).collect();
    if name.chars().count() > 5 {
        short.push_str("..");
    }
    short
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
            '\u{2013}' => out.push_str("\\226"),
            '\u{2014}' => out.push_str("\\227"),
            '\u{2018}' | '\u{2019}' => out.push_str("\\221"),
            '\u{201c}' | '\u{201d}' => out.push_str("\\222"),
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
        assert!(text.contains("(la) Tj"));
        assert!(text.contains("(f) Tj"));
        assert!(text.contains("/ClefTreble"), "clef image resource");
        assert!(
            text.contains("(#) Tj") || text.contains("(b) Tj"),
            "accidental"
        );
        assert!(page_count(&pdf).is_some_and(|count| count >= 1));
        let one = to_full_pdf(&song, small(), "One", &[TrackId::new(1).expect("valid")]);
        let text = String::from_utf8_lossy(&one);
        assert!(text.contains("(Piano) Tj"));
        assert!(!text.contains("(Bass) Tj"));
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
}
