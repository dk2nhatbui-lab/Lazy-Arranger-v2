//! MusicXML import.
//!
//! Reads a partwise document into a [`Song`]: one track per part, measures
//! advanced by the meter, notes placed by divisions, chords stacked, rests
//! skipped and unpitched notes treated as drums. Tempo changes come from
//! `<sound tempo>` elements; chords, lyrics, marks and dynamics are imported.
//! The song has no key field, so `<key>` is not imported.

use music_core::{Channel, Pitch, TempoChange, TempoMap, Tick, TimeSignature, Velocity};
use project::song::{Note, Song, Track};
use project::{NoteId, TrackId};
use thiserror::Error;

/// Errors from MusicXML import.
#[derive(Debug, Error)]
pub enum ImportError {
    /// The document is not valid XML.
    #[error("not valid XML: {0}")]
    Xml(#[from] roxmltree::Error),
    /// The document has no parts.
    #[error("the document has no parts")]
    NoParts,
}

fn read_harmony(harmony: roxmltree::Node<'_, '_>) -> Option<music_core::Chord> {
    let root = harmony.children().find(|node| node.has_tag_name("root"))?;
    let step = text_of(root, "root-step")?;
    let alter = text_of(root, "root-alter")
        .and_then(|text| text.parse::<i32>().ok())
        .unwrap_or(0);
    let kind = text_of(harmony, "kind").unwrap_or_else(|| String::from("major"));
    let suffix = match kind.as_str() {
        "minor" => "m",
        "dominant" => "7",
        "major-seventh" => "maj7",
        "minor-seventh" => "m7",
        "diminished" => "dim",
        "half-diminished" => "m7b5",
        "augmented" => "aug",
        _ => "",
    };
    let accidental = match alter {
        1 => "#",
        -1 => "b",
        _ => "",
    };
    let mut symbol = format!("{step}{accidental}{suffix}");
    if let Some(bass) = harmony.children().find(|node| node.has_tag_name("bass"))
        && let Some(step) = text_of(bass, "bass-step")
    {
        let alter = text_of(bass, "bass-alter")
            .and_then(|text| text.parse::<i32>().ok())
            .unwrap_or(0);
        let accidental = match alter {
            1 => "#",
            -1 => "b",
            _ => "",
        };
        symbol.push('/');
        symbol.push_str(&step);
        symbol.push_str(accidental);
    }
    music_core::Chord::parse(&symbol).ok()
}

fn read_rehearsal(direction: roxmltree::Node<'_, '_>, tick: Tick) -> Option<project::Marker> {
    let body = direction
        .children()
        .find(|node| node.has_tag_name("direction-type"))?;
    let name = body
        .children()
        .find(|node| node.has_tag_name("rehearsal"))
        .and_then(|node| node.text())?
        .trim();
    (!name.is_empty()).then(|| project::Marker {
        tick,
        name: name.to_string(),
    })
}

fn read_mark(direction: roxmltree::Node<'_, '_>, tick: Tick) -> Option<project::song::Mark> {
    use project::song::{Articulation, Dynamic, Mark, MarkKind};
    let body = direction
        .children()
        .find(|node| node.has_tag_name("direction-type"))?;
    if let Some(dynamics) = body.children().find(|node| node.has_tag_name("dynamics")) {
        for child in dynamics.children().filter(|node| node.is_element()) {
            let level = match child.tag_name().name() {
                "pp" => Some(Dynamic::Pp),
                "p" => Some(Dynamic::P),
                "mp" => Some(Dynamic::Mp),
                "mf" => Some(Dynamic::Mf),
                "f" => Some(Dynamic::F),
                "ff" => Some(Dynamic::Ff),
                _ => None,
            };
            if let Some(level) = level {
                return Some(Mark {
                    tick,
                    kind: MarkKind::Dynamic(level),
                });
            }
        }
    }
    let words = body
        .children()
        .find(|node| node.has_tag_name("words"))
        .and_then(|node| node.text())
        .unwrap_or_default();
    let articulation = match words.trim() {
        "stacc." => Some(Articulation::Staccato),
        "acc." => Some(Articulation::Accent),
        "ten." => Some(Articulation::Tenuto),
        "marc." => Some(Articulation::Marcato),
        "ferm." => Some(Articulation::Fermata),
        _ => None,
    }?;
    Some(Mark {
        tick,
        kind: MarkKind::Articulation(articulation),
    })
}

fn key_from_fifths(fifths: i32, minor: bool) -> Option<project::song::KeySignature> {
    let major = match fifths.clamp(-7, 7) {
        0 => 0,
        1 => 7,
        2 => 2,
        3 => 9,
        4 => 4,
        5 => 11,
        6 => 6,
        7 => 1,
        -1 => 5,
        -2 => 10,
        -3 => 3,
        -4 => 8,
        -5 => 1,
        -6 => 6,
        -7 => 11,
        _ => return None,
    };
    let tonic = if minor { (major + 9) % 12 } else { major };
    Some(project::song::KeySignature {
        tonic: music_core::PitchClass::new(tonic),
        minor,
    })
}

fn text_of(node: roxmltree::Node<'_, '_>, tag: &str) -> Option<String> {
    node.children()
        .find(|child| child.has_tag_name(tag))
        .and_then(|child| child.text())
        .map(|text| text.trim().to_string())
}

fn number_of(node: roxmltree::Node<'_, '_>, tag: &str) -> Option<f64> {
    text_of(node, tag).and_then(|text| text.parse().ok())
}

/// The furthest tick an imported event may sit at, about 230 days at 120 bpm.
/// Keeps every later addition far from `i64` overflow.
const MAX_IMPORT_TICK: Tick = 1 << 40;

/// `units` of `divisions`-per-quarter as ticks, kept in `-MAX..=MAX`.
fn units_to_ticks(units: f64, divisions: f64) -> Tick {
    let ticks = (units / divisions * music_core::PPQ as f64).round();
    if ticks.is_finite() {
        (ticks as Tick).clamp(-MAX_IMPORT_TICK, MAX_IMPORT_TICK)
    } else {
        0
    }
}

fn offset_ticks(node: roxmltree::Node<'_, '_>, divisions: f64) -> Tick {
    let Some(offset) = number_of(node, "offset") else {
        return 0;
    };
    units_to_ticks(offset, divisions)
}

/// Imports a partwise MusicXML document.
///
/// # Errors
///
/// Returns [`ImportError::Xml`] for malformed documents and
/// [`ImportError::NoParts`] when no `<part>` exists.
pub fn import(xml: &str) -> Result<Song, ImportError> {
    // MusicXML files carry a DOCTYPE; roxmltree rejects it unless allowed.
    // It reads the internal subset only and never fetches external entities.
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    let document = roxmltree::Document::parse_with_options(xml, options)?;
    let title = document
        .descendants()
        .find(|node| node.has_tag_name("work-title"))
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("Imported score")
        .to_string();
    let tempo = document
        .descendants()
        .filter(|node| node.has_tag_name("sound"))
        .find_map(|node| node.attribute("tempo"))
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(120.0);
    let mut song = Song::new(title, tempo);
    song.key = document
        .descendants()
        .find(|node| node.has_tag_name("key"))
        .and_then(|key| {
            let fifths = number_of(key, "fifths")? as i32;
            let minor = text_of(key, "mode").is_some_and(|mode| mode.eq_ignore_ascii_case("minor"));
            key_from_fifths(fifths, minor)
        });
    let mut divisions = 1.0_f64;
    let names: Vec<(String, String)> = document
        .descendants()
        .filter(|node| node.has_tag_name("score-part"))
        .map(|part| {
            (
                part.attribute("id").unwrap_or_default().to_string(),
                text_of(part, "part-name").unwrap_or_default(),
            )
        })
        .collect();
    let parts: Vec<roxmltree::Node<'_, '_>> = document
        .descendants()
        .filter(|node| node.has_tag_name("part"))
        .collect();
    if parts.is_empty() {
        return Err(ImportError::NoParts);
    }
    let mut next_track = 0_u32;
    let mut next_note = 0_u32;
    let mut meter = TimeSignature::default();
    let mut tempo_changes = vec![TempoChange {
        tick: 0,
        bpm: tempo,
    }];
    for part in parts {
        next_track += 1;
        let id = part.attribute("id").unwrap_or_default();
        let name = names
            .iter()
            .find(|(part_id, _)| part_id == id)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| format!("Part {next_track}"));
        let mut track = Track::new(
            TrackId::new(next_track).map_err(|_| ImportError::NoParts)?,
            name,
        );
        let mut measure_start: Tick = 0;
        let mut measure_meter = meter;
        let mut pending_slur: Option<Tick> = None;
        for measure in part.children().filter(|node| node.has_tag_name("measure")) {
            if next_track == 1
                && let Some(tempo) = measure
                    .children()
                    .find(|node| node.has_tag_name("sound"))
                    .and_then(|node| node.attribute("tempo"))
                    .and_then(|value| value.parse::<f64>().ok())
                    .filter(|value| value.is_finite() && *value > 0.0)
            {
                tempo_changes.push(TempoChange {
                    tick: measure_start,
                    bpm: tempo,
                });
            }
            if let Some(attributes) = measure
                .children()
                .find(|node| node.has_tag_name("attributes"))
            {
                if let Some(time) = attributes.children().find(|node| node.has_tag_name("time")) {
                    let beats = number_of(time, "beats").unwrap_or(4.0);
                    let beat_type = number_of(time, "beat-type").unwrap_or(4.0);
                    if let Ok(found) =
                        TimeSignature::new(beats.max(1.0) as u8, beat_type.max(1.0) as u8)
                    {
                        measure_meter = found;
                        meter = found;
                    }
                }
                // A subnormal or huge value would blow every duration up.
                if let Some(value) = number_of(attributes, "divisions")
                    && value.is_finite()
                    && (1e-3..=1e6).contains(&value)
                {
                    divisions = value;
                }
            }
            for direction in measure
                .children()
                .filter(|node| node.has_tag_name("direction"))
            {
                let tick = measure_start
                    .saturating_add(offset_ticks(direction, divisions))
                    .max(0);
                if let Some(marker) = read_rehearsal(direction, tick) {
                    song.markers.push(marker);
                }
                if let Some(mark) = read_mark(direction, tick) {
                    song.marks.push(mark);
                }
            }
            for harmony in measure
                .children()
                .filter(|node| node.has_tag_name("harmony"))
            {
                if let Some(chord) = read_harmony(harmony) {
                    song.chords.push(project::song::ChordEvent {
                        tick: measure_start
                            .saturating_add(offset_ticks(harmony, divisions))
                            .max(0),
                        chord,
                    });
                }
            }
            let mut cursor = measure_start;
            let mut last_start = measure_start;
            for node in measure.children() {
                if node.has_tag_name("backup") {
                    let units = number_of(node, "duration").unwrap_or(0.0);
                    let back = units_to_ticks(units, divisions).max(0);
                    cursor = cursor.saturating_sub(back).max(measure_start);
                    last_start = cursor;
                    continue;
                }
                if !node.has_tag_name("note") {
                    continue;
                }
                let note = node;
                let duration_units = number_of(note, "duration").unwrap_or(0.0);
                let duration = units_to_ticks(duration_units, divisions).max(0);
                let is_rest = note.children().any(|child| child.has_tag_name("rest"));
                let is_chord = note.children().any(|child| child.has_tag_name("chord"));
                let start = if is_chord { last_start } else { cursor };
                if !is_chord {
                    last_start = start;
                }
                if is_rest {
                    if !is_chord {
                        cursor = start.saturating_add(duration);
                    }
                    continue;
                }
                next_note += 1;
                let velocity = note
                    .children()
                    .find(|child| child.has_tag_name("sound"))
                    .and_then(|sound| sound.attribute("dynamics"))
                    .and_then(|value| value.parse::<u8>().ok())
                    .map_or(100, |value| value.clamp(1, 127));
                let drum_pitch = note
                    .children()
                    .find(|child| child.has_tag_name("instrument"))
                    .and_then(|instrument| instrument.attribute("id"))
                    .and_then(|id| id.rsplit_once("-I"))
                    .and_then(|(_, pitch)| pitch.parse::<u8>().ok());
                let (pitch, channel) = if let Some(pitch_node) =
                    note.children().find(|child| child.has_tag_name("pitch"))
                {
                    let step = text_of(pitch_node, "step").unwrap_or_default();
                    let alter = number_of(pitch_node, "alter").unwrap_or(0.0) as i32;
                    let octave = number_of(pitch_node, "octave").unwrap_or(4.0) as i32;
                    let base = match step.as_str() {
                        "C" => 0,
                        "D" => 2,
                        "E" => 4,
                        "F" => 5,
                        "G" => 7,
                        "A" => 9,
                        "B" => 11,
                        _ => 0,
                    };
                    let midi = (octave.clamp(-1, 10) + 1) * 12 + base + alter.clamp(-2, 2);
                    (
                        Pitch::new(midi.clamp(0, 127) as u8)
                            .unwrap_or_else(|_| Pitch::new(60).expect("valid middle C")),
                        Channel::first(),
                    )
                } else {
                    let pitch = drum_pitch.unwrap_or(38);
                    (
                        Pitch::new(pitch).unwrap_or_else(|_| Pitch::new(38).expect("valid snare")),
                        Channel::new(9).expect("valid drum channel"),
                    )
                };
                if let Some(text) = note
                    .children()
                    .find(|child| child.has_tag_name("lyric"))
                    .and_then(|lyric| lyric.children().find(|child| child.has_tag_name("text")))
                    .and_then(|node| node.text())
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty())
                {
                    song.lyrics
                        .push(project::song::LyricEvent { tick: start, text });
                }
                let tie_stop = note.children().any(|child| {
                    child.has_tag_name("tie") && child.attribute("type") == Some("stop")
                });
                let slur_start = note.descendants().any(|child| {
                    child.has_tag_name("slur") && child.attribute("type") == Some("start")
                });
                let slur_stop = note.descendants().any(|child| {
                    child.has_tag_name("slur") && child.attribute("type") == Some("stop")
                });
                if slur_start {
                    pending_slur = Some(start);
                }
                if slur_stop && let Some(slur_start) = pending_slur.take() {
                    track.slurs.push(project::song::Slur {
                        start: slur_start,
                        end: start,
                    });
                }
                if tie_stop
                    && let Some(previous) = track.notes.iter_mut().find(|held| {
                        held.pitch.as_u8() == pitch.as_u8()
                            && held.start.saturating_add(held.duration.max(1)) == start
                    })
                {
                    previous.duration = previous.duration.saturating_add(duration.max(1));
                    if !is_chord {
                        cursor = start.saturating_add(duration.max(1));
                    }
                    continue;
                }
                track.notes.push(Note {
                    id: NoteId::new(next_note).unwrap_or_else(|_| NoteId::new(1).expect("valid")),
                    start,
                    duration: duration.max(1),
                    pitch,
                    velocity: Velocity::new(velocity).expect("valid"),
                    channel,
                });
                if !is_chord {
                    cursor = start.saturating_add(duration.max(1));
                }
            }
            measure_start = measure_start.saturating_add(measure_meter.bar_ticks().max(1));
        }
        song.tracks.push(track);
    }
    song.time_signature = meter;
    tempo_changes.sort_by_key(|change| change.tick);
    tempo_changes.dedup_by_key(|change| change.tick);
    if let Ok(map) = TempoMap::new(tempo_changes) {
        song.tempo_map = map;
    }
    Ok(song)
}
