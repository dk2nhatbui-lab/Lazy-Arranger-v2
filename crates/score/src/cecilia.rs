//! Adapter from the project song to Cecilia's notation model and engraver.
//!
//! We do not lay out staves ourselves here: the project notes become a Cecilia
//! score (bars, beats, durations, key and clefs), and `cecilia_engrave` produces
//! the pages, PNGs and PDF.

use cecilia_core::{
    Bar, Beat, Clef, Duration, DurationValue, KeySignature, MasterBar, Metadata,
    Note as CeciliaNote, Score as CeciliaScore, Staff, TempoChange, TimeSignature, TrackKind,
    Tuplet, Voice,
};
use cecilia_engrave::{Mode, Options, engrave};
use music_core::Tick;
use project::Song;
use project::song::Note;

use crate::spelling;

fn duration_for_ticks(ticks: Tick) -> Option<Duration> {
    let ticks = u32::try_from(ticks.max(1)).ok()?;
    for value in DurationValue::ALL {
        for dots in 0..=2 {
            let duration = Duration {
                value,
                dots,
                tuplet: None,
            };
            if duration.ticks() == ticks {
                return Some(duration);
            }
            let triplet = Duration {
                value,
                dots,
                tuplet: Some(Tuplet::TRIPLET),
            };
            if triplet.ticks() == ticks {
                return Some(triplet);
            }
        }
    }
    None
}

/// The written value closest to `ticks` that fits in `limit` ticks, for
/// notes that are a little short or long of a note value, as played notes
/// usually are. Exact values, dots and triplets come first.
fn nearest_duration(ticks: Tick, limit: Tick) -> Option<Duration> {
    let limit = u32::try_from(limit.max(1)).ok()?;
    if let Some(exact) = duration_for_ticks(ticks)
        && exact.ticks() <= limit
    {
        return Some(exact);
    }
    let ticks = u32::try_from(ticks.max(1)).ok()?;
    DurationValue::ALL
        .into_iter()
        .flat_map(|value| {
            (0..=2).map(move |dots| Duration {
                value,
                dots,
                tuplet: None,
            })
        })
        .filter(|duration| duration.ticks() <= limit)
        .min_by_key(|duration| duration.ticks().abs_diff(ticks))
}

/// Circle-of-fifths position for the detected project key.
fn fifths(key: spelling::Key) -> i8 {
    let major = if key.minor {
        key.tonic.transpose(3)
    } else {
        key.tonic
    };
    match major.as_u8() {
        0 => 0,
        7 => 1,
        2 => 2,
        9 => 3,
        4 => 4,
        11 => 5,
        6 => 6,
        1 => -5,
        5 => -1,
        10 => -2,
        3 => -3,
        8 => -4,
        _ => 0,
    }
}

fn key_name(key: spelling::Key) -> String {
    let name = if fifths(key) >= 0 {
        key.tonic.name_sharp()
    } else {
        key.tonic.name_flat()
    };
    format!("{name} {}", if key.minor { "minor" } else { "major" })
}

/// Builds a one-track Cecilia score for the selected project track.
///
/// # Errors
///
/// Returns a message when the track index is missing or a note duration cannot
/// be represented by Cecilia's note values.
pub fn build_score(
    song: &Song,
    track_index: usize,
    title: &str,
    grand_staff: bool,
) -> Result<CeciliaScore, String> {
    let track = song
        .tracks
        .get(track_index)
        .ok_or_else(|| String::from("track index out of range"))?;
    let bar = song.time_signature.bar_ticks().max(1);
    let bars = (song.end_tick().max(bar).saturating_add(bar - 1) / bar)
        .clamp(1, crate::MAX_MEASURES) as usize;
    let key = spelling::detect_key(song);
    let drums = track
        .notes
        .iter()
        .any(|note| note.channel.zero_based() == 9);
    let mut master_bars = Vec::with_capacity(bars);
    for index in 0..bars {
        let start = index as Tick * bar;
        let mut master = MasterBar {
            time_signature: TimeSignature {
                numerator: song.time_signature.numerator,
                denominator: song.time_signature.denominator,
            },
            key_signature: KeySignature {
                accidentals: fifths(key),
                minor: key.minor,
            },
            ..MasterBar::default()
        };
        if let Some(marker) = song.markers.iter().find(|marker| marker.tick == start) {
            master.marker = Some(cecilia_core::Marker {
                title: marker.name.clone(),
                color: cecilia_core::Color::default(),
                letter: String::new(),
            });
        }
        for change in song.tempo_map.changes() {
            if change.tick >= start && change.tick < start + bar {
                master.tempo_changes.push(TempoChange {
                    tick: u32::try_from(change.tick - start).unwrap_or(0),
                    bpm: change.bpm,
                });
            }
        }
        master_bars.push(master);
    }

    let mut track_bars = Vec::with_capacity(bars);
    for index in 0..bars {
        let start = index as Tick * bar;
        let stop = start + bar;
        let mut notes: Vec<&Note> = track
            .notes
            .iter()
            .filter(|note| note.start >= start && note.start < stop)
            .collect();
        notes.sort_by_key(|note| (note.start, note.pitch.as_u8()));
        let mut beats = Vec::new();
        let mut cursor = start;
        let mut at = 0;
        while at < notes.len() {
            let note_start = notes[at].start;
            if note_start > cursor {
                beats.extend(cecilia_core::rest_beats(
                    u32::try_from(note_start - cursor).unwrap_or(0),
                ));
            }
            let mut end = at;
            while end < notes.len() && notes[end].start == note_start {
                end += 1;
            }
            let available = stop - note_start;
            let written = notes[at..end]
                .iter()
                .map(|note| note.duration.min(available).max(1))
                .min()
                .unwrap_or(1);
            // Room before the next chord, so a rounded-up value never overlaps it.
            let room = notes
                .get(end)
                .map_or(available, |next| next.start - note_start);
            let Some(duration) = nearest_duration(written, room) else {
                return Err(format!(
                    "note duration {written} ticks has no Cecilia note value"
                ));
            };
            let written = Tick::from(duration.ticks());
            let chord = song
                .chords
                .iter()
                .find(|event| event.tick == note_start)
                .map(|event| cecilia_core::Chord {
                    name: event.chord.format(music_core::Spelling::Sharp),
                    ..cecilia_core::Chord::default()
                });
            let beat = Beat {
                duration,
                notes: notes[at..end]
                    .iter()
                    .map(|note| CeciliaNote {
                        string: 0,
                        fret: note.pitch.as_u8(),
                        velocity: note.velocity.as_u8(),
                        ..CeciliaNote::default()
                    })
                    .collect(),
                chord,
                ..Beat::default()
            };
            beats.push(beat);
            cursor = note_start + written;
            at = end;
        }
        if cursor < stop {
            beats.extend(cecilia_core::rest_beats(
                u32::try_from(stop - cursor).unwrap_or(0),
            ));
        }
        track_bars.push(Bar {
            voices: vec![Voice { beats }],
            ..Bar::default()
        });
    }

    let low = track
        .notes
        .iter()
        .filter(|note| note.pitch.as_u8() < 60)
        .count();
    let staves = if drums {
        Vec::new()
    } else if grand_staff {
        vec![Staff::Standard(Clef::Grand { split: 60 })]
    } else if low * 2 > track.notes.len() {
        // Mostly below middle C: a bass clef.
        vec![Staff::Standard(Clef::Bass)]
    } else {
        Vec::new()
    };
    let cecilia_track = cecilia_core::Track {
        name: track.name.clone(),
        kind: if drums {
            TrackKind::Drums
        } else {
            TrackKind::Pitched
        },
        staves,
        bars: track_bars,
        ..cecilia_core::Track::default()
    };
    Ok(CeciliaScore {
        meta: Metadata {
            title: title.to_string(),
            subtitle: key_name(key),
            ..Metadata::default()
        },
        tempo: song.tempo_map.bpm_at(0).round().max(1.0) as u32,
        master_bars,
        tracks: vec![cecilia_track],
        ..CeciliaScore::default()
    })
}

/// Engraves the selected track and returns the PDF bytes.
///
/// # Errors
///
/// Returns a message when the score cannot be built.
pub fn render_pdf(
    song: &Song,
    track_index: usize,
    title: &str,
    grand_staff: bool,
) -> Result<Vec<u8>, String> {
    let score = build_score(song, track_index, title, grand_staff)?;
    let layout = engrave(
        &score,
        0,
        &Options {
            mode: Mode::Page {
                width: 595.0,
                height: 842.0,
            },
            sp: 8.0,
            margin: 48.0,
            standard: true,
            tab: false,
            chant: false,
            ..Options::default()
        },
    );
    Ok(cecilia_engrave::output::to_pdf(&layout.pages))
}

/// One track's score as PDF: Cecilia's engraving, with a grand staff when the
/// notes reach both sides of middle C. Falls back to the classic layout when
/// Cecilia cannot write the track.
#[must_use]
pub fn track_pdf(song: &Song, track: project::TrackId, title: &str) -> Vec<u8> {
    let index = song.tracks.iter().position(|entry| entry.id == track);
    let grand = index
        .and_then(|index| song.tracks.get(index))
        .is_some_and(|entry| {
            entry.notes.iter().any(|note| note.pitch.as_u8() < 60)
                && entry.notes.iter().any(|note| note.pitch.as_u8() >= 60)
                && entry
                    .notes
                    .iter()
                    .all(|note| note.channel.zero_based() != 9)
        });
    index
        .and_then(|index| render_pdf(song, index, title, grand).ok())
        .unwrap_or_else(|| {
            let settings = crate::Settings::default();
            crate::pdf::to_pdf(&crate::engrave(song, track, settings), settings, title)
        })
}

/// Engraves the selected track and returns one PNG per page.
///
/// # Errors
///
/// Returns a message when the score cannot be built or a page cannot be rendered.
pub fn render_pages(
    song: &Song,
    track_index: usize,
    title: &str,
    grand_staff: bool,
    scale: f64,
) -> Result<Vec<Vec<u8>>, String> {
    let score = build_score(song, track_index, title, grand_staff)?;
    let layout = engrave(
        &score,
        0,
        &Options {
            mode: Mode::Page {
                width: 595.0,
                height: 842.0,
            },
            sp: 8.0,
            margin: 48.0,
            standard: true,
            tab: false,
            chant: false,
            ..Options::default()
        },
    );
    layout
        .pages
        .iter()
        .map(|page| cecilia_engrave::output::to_png(page, scale))
        .collect()
}

/// One decoded page image.
#[derive(Debug)]
pub struct PageImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight-alpha RGBA bytes.
    pub rgba: Vec<u8>,
}

/// Engraves the selected track and decodes one PNG per page.
///
/// # Errors
///
/// Returns a message when the score cannot be built or a page cannot be decoded.
pub fn render_page_images(
    song: &Song,
    track_index: usize,
    title: &str,
    grand_staff: bool,
    scale: f64,
) -> Result<Vec<PageImage>, String> {
    render_pages(song, track_index, title, grand_staff, scale)?
        .into_iter()
        .map(|png| {
            let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
                .map_err(|error| format!("Cecilia page PNG: {error}"))?
                .to_rgba8();
            Ok(PageImage {
                width: image.width(),
                height: image.height(),
                rgba: image.into_raw(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Pitch, PitchClass};

    #[test]
    fn key_signatures_follow_the_circle_of_fifths() {
        assert_eq!(
            fifths(spelling::Key {
                tonic: PitchClass::new(0),
                minor: false
            }),
            0
        );
        assert_eq!(
            fifths(spelling::Key {
                tonic: PitchClass::new(9),
                minor: true
            }),
            0
        );
        assert_eq!(
            fifths(spelling::Key {
                tonic: PitchClass::new(0),
                minor: true
            }),
            -3
        );
        assert_eq!(
            fifths(spelling::Key {
                tonic: PitchClass::new(7),
                minor: false
            }),
            1
        );
    }

    fn test_song() -> Song {
        use music_core::{Channel, Pitch, Velocity};
        use project::NoteId;
        use project::TrackId;
        let mut song = Song::new("Demo", 120.0);
        let mut track = project::song::Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, name) in ["C4", "E4", "G4", "C5"].into_iter().enumerate() {
            track.notes.push(Note {
                id: NoteId::new(index as u32 + 1).expect("valid"),
                start: index as Tick * music_core::PPQ,
                duration: music_core::PPQ,
                pitch: Pitch::from_name(name).expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        song
    }

    #[test]
    fn a_demo_track_builds_and_engraves() {
        let song = test_song();
        let score = build_score(&song, 0, "Demo", true).expect("build");
        assert_eq!(score.tracks.len(), 1);
        assert_eq!(score.master_bars.len(), 1);
        assert_eq!(score.tracks[0].bars.len(), 1);
        assert!(!score.tracks[0].bars[0].voices[0].beats.is_empty());
        let pages = render_pages(&song, 0, "Demo", true, 1.0).expect("pages");
        assert!(!pages.is_empty());
        assert!(pages[0].starts_with(&[0x89, b'P', b'N', b'G']));
        let pdf = render_pdf(&song, 0, "Demo", true).expect("pdf");
        assert!(pdf.starts_with(b"%PDF-"));
    }

    #[test]
    fn played_lengths_round_to_the_nearest_note_value() {
        // A quarter played a little short, and one a little long that
        // would overlap the next note.
        let mut song = test_song();
        song.tracks[0].notes[0].duration = music_core::PPQ - 20;
        song.tracks[0].notes[1].duration = music_core::PPQ + 40;
        let score = build_score(&song, 0, "Demo", true).expect("build");
        let beats = &score.tracks[0].bars[0].voices[0].beats;
        let total: u32 = beats.iter().map(|beat| beat.duration.ticks()).sum();
        assert_eq!(Tick::from(total), song.time_signature.bar_ticks());
        assert_eq!(
            beats[0].duration.ticks(),
            u32::try_from(music_core::PPQ).expect("ppq")
        );
        assert_eq!(
            beats[1].duration.ticks(),
            u32::try_from(music_core::PPQ).expect("ppq")
        );
    }

    #[test]
    fn low_tracks_get_a_bass_clef_and_a_pdf() {
        let mut song = test_song();
        for note in &mut song.tracks[0].notes {
            note.pitch = Pitch::new(note.pitch.as_u8() - 24).expect("in range");
        }
        let score = build_score(&song, 0, "Low", false).expect("build");
        assert_eq!(score.tracks[0].staves, vec![Staff::Standard(Clef::Bass)]);
        let pdf = track_pdf(&song, song.tracks[0].id, "Low");
        assert!(pdf.starts_with(b"%PDF-"));
    }
}
