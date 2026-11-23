//! Engraving layout, toolkit-independent (ADR-010).
//!
//! `engrave` turns a song into an immutable layout of pages, systems, measures
//! and positioned notes. Renderers (egui canvas, PDF) consume this and make no
//! musical decisions. The first version lays out one track on one staff with
//! fixed measure widths; spelling, beams, ties, collisions, lyrics and marks
//! come later.

pub mod pdf;
pub mod spelling;

use music_core::{Pitch, Tick};
use project::TrackId;
use project::song::Song;

/// Page geometry in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Page width.
    pub page_width: f32,
    /// Page height.
    pub page_height: f32,
    /// Margin on every side.
    pub margin: f32,
    /// Distance between staff lines.
    pub staff_space: f32,
    /// Distance from a system's staff to the next system.
    pub system_gap: f32,
    /// Horizontal room for one quarter note.
    pub quarter_width: f32,
    /// Gap between measures.
    pub measure_gap: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            page_width: 595.0,
            page_height: 842.0,
            margin: 48.0,
            staff_space: 8.0,
            system_gap: 110.0,
            quarter_width: 28.0,
            measure_gap: 18.0,
        }
    }
}

/// One positioned note.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedNote {
    /// Tick of the note.
    pub tick: Tick,
    /// Duration in ticks.
    pub duration: Tick,
    /// MIDI pitch.
    pub pitch: Pitch,
    /// Horizontal position.
    pub x: f32,
    /// Vertical position of the notehead.
    pub y: f32,
    /// Page holding the note.
    pub page: usize,
    /// Measure index, from zero.
    pub measure: usize,
    /// Letter, alteration, octave and any accidental to print.
    pub spelled: spelling::Spelled,
    /// True when this note starts a tie into the next one.
    pub tie_start: bool,
    /// True when this note continues a tie: no notehead is drawn.
    pub tied: bool,
}

/// One score mark placed on the page.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedMark {
    /// Tick of the mark.
    pub tick: Tick,
    /// What kind of mark.
    pub kind: project::song::MarkKind,
    /// Horizontal position.
    pub x: f32,
    /// Vertical position.
    pub y: f32,
    /// Page holding it.
    pub page: usize,
}

/// One lyric syllable placed under a note.
#[derive(Clone, Debug, PartialEq)]
pub struct Lyric {
    /// Tick of the note it belongs to.
    pub tick: Tick,
    /// The syllable.
    pub text: String,
    /// Horizontal position.
    pub x: f32,
    /// Vertical position (below the staff).
    pub y: f32,
    /// Page holding it.
    pub page: usize,
}

/// One measure.
#[derive(Clone, Debug, PartialEq)]
pub struct Measure {
    /// First tick.
    pub start: Tick,
    /// Length in ticks.
    pub length: Tick,
    /// Bar number, from one.
    pub number: usize,
    /// Left edge.
    pub x: f32,
    /// Right edge.
    pub x_end: f32,
}

/// One system: the measures on one staff line.
#[derive(Clone, Debug, PartialEq)]
pub struct System {
    /// Measures in order.
    pub measures: Vec<Measure>,
    /// Baseline of the top staff line.
    pub y: f32,
    /// Page holding the system.
    pub page: usize,
}

/// One page.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    /// Systems in reading order.
    pub systems: Vec<System>,
}

/// A click position turned into a note position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoreHit {
    /// Quantised tick.
    pub tick: Tick,
    /// MIDI pitch from the vertical position.
    pub pitch: u8,
}

/// The pitch a click at `y` means on a system whose staff starts at `system_y`.
#[must_use]
pub fn pitch_at_y(system_y: f32, y: f32, settings: Settings) -> u8 {
    let step = ((system_y - y) / settings.staff_space.max(1.0)).round();
    (60.0 + 2.0 * step).clamp(0.0, 127.0) as u8
}

/// The tick a click at `x` means inside a measure, quantised.
#[must_use]
pub fn tick_at_x(measure: &Measure, x: f32, quantum: Tick) -> Tick {
    let width = (measure.x_end - measure.x).max(1.0);
    let ratio = ((x - measure.x) / width).clamp(0.0, 1.0);
    let tick = measure.start + (ratio * measure.length as f32).round() as Tick;
    let quantum = quantum.max(1);
    (tick / quantum) * quantum
}

/// The engraved layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Engraved {
    /// Pages in order.
    pub pages: Vec<Page>,
    /// Positioned notes, in tick order.
    pub notes: Vec<PlacedNote>,
    /// Track laid out.
    pub track: TrackId,
    /// Total measures.
    pub measures: usize,
    /// Detected key, from the chords or the notes.
    pub key: spelling::Key,
    /// Lyrics, one syllable per placed note.
    pub lyrics: Vec<Lyric>,
    /// Dynamics and articulations.
    pub marks: Vec<PlacedMark>,
}

impl Engraved {
    /// Every page's systems.
    #[must_use]
    pub fn systems(&self) -> usize {
        self.pages.iter().map(|page| page.systems.len()).sum()
    }
}

/// Lays out one track as a scored document without lyrics.
#[must_use]
pub fn engrave(song: &Song, track: TrackId, settings: Settings) -> Engraved {
    engrave_with(song, track, settings, &[])
}

/// Lays out one track, placing one lyric syllable per note.
#[must_use]
pub fn engrave_with(song: &Song, track: TrackId, settings: Settings, lyrics: &[&str]) -> Engraved {
    let bar = song.time_signature.bar_ticks().max(1);
    let end = song.end_tick().max(bar);
    let measure_count = usize::try_from((end + bar - 1) / bar).unwrap_or(1);
    let content_width = (settings.page_width - 2.0 * settings.margin).max(1.0);
    let content_height = (settings.page_height - 2.0 * settings.margin).max(settings.system_gap);
    let per_system = ((content_width + settings.measure_gap)
        / (measure_width(&settings, bar) + settings.measure_gap))
        .floor()
        .max(1.0) as usize;
    let per_page = (content_height / settings.system_gap).floor().max(1.0) as usize;

    let key = spelling::detect_key(song);
    let mut pages = Vec::new();
    let mut notes: Vec<PlacedNote> = Vec::new();
    let mut placed_lyrics: Vec<Lyric> = Vec::new();
    let mut lyric_index = 0;
    let mut open_ties: std::collections::BTreeMap<u8, (usize, Tick)> =
        std::collections::BTreeMap::new();
    let source = song.tracks.iter().find(|entry| entry.id == track);
    let mut measure_index = 0;
    while measure_index < measure_count {
        let mut page = Page {
            systems: Vec::new(),
        };
        for _ in 0..per_page {
            if measure_index >= measure_count {
                break;
            }
            let page_index = pages.len();
            let mut measures = Vec::new();
            let start_x = settings.margin;
            let mut x = start_x;
            for _ in 0..per_system {
                if measure_index >= measure_count {
                    break;
                }
                let start = (measure_index as Tick) * bar;
                let width = measure_width(&settings, bar);
                let measure = Measure {
                    start,
                    length: bar,
                    number: measure_index + 1,
                    x,
                    x_end: x + width,
                };
                if let Some(track) = source {
                    let mut state = key.signature();
                    let mut in_measure: Vec<&project::song::Note> = track
                        .notes
                        .iter()
                        .filter(|note| note.start >= start && note.start < start + bar)
                        .collect();
                    in_measure.sort_by_key(|note| note.start);
                    for note in in_measure {
                        let offset = f32::from(i16::try_from(note.start - start).unwrap_or(0))
                            / bar as f32
                            * width;
                        let tie_start = false;
                        let mut tied = false;
                        if let Some((index, end)) = open_ties.get(&note.pitch.as_u8())
                            && *end == note.start
                        {
                            notes[*index].tie_start = true;
                            tied = true;
                        }
                        let note_x = x + offset;
                        notes.push(PlacedNote {
                            tick: note.start,
                            duration: note.duration,
                            pitch: note.pitch,
                            x: note_x,
                            y: 0.0,
                            page: page_index,
                            measure: measure_index,
                            spelled: spelling::spell(note.pitch, key, &mut state),
                            tie_start,
                            tied,
                        });
                        let pushed = notes.len() - 1;
                        open_ties.insert(
                            note.pitch.as_u8(),
                            (pushed, note.start.saturating_add(note.duration)),
                        );
                        let explicit = lyrics.get(lyric_index).copied();
                        if explicit.is_some() {
                            lyric_index += 1;
                        }
                        let text = explicit.or_else(|| {
                            song.lyrics
                                .iter()
                                .find(|event| event.tick == note.start)
                                .map(|event| event.text.as_str())
                        });
                        if let Some(text) = text {
                            placed_lyrics.push(Lyric {
                                tick: note.start,
                                text: text.to_string(),
                                x: note_x,
                                y: 0.0,
                                page: page_index,
                            });
                        }
                    }
                }
                x = measure.x_end + settings.measure_gap;
                measures.push(measure);
                measure_index += 1;
            }
            page.systems.push(System {
                measures,
                y: 0.0,
                page: page_index,
            });
        }
        pages.push(page);
    }

    // Vertical placement: page top + margin, then one gap per system, and each
    // note seven staff spaces above the bottom line for its pitch class.
    for (page_index, page) in pages.iter_mut().enumerate() {
        for (system_index, system) in page.systems.iter_mut().enumerate() {
            system.y = settings.margin
                + 4.0 * settings.staff_space
                + (system_index as f32) * settings.system_gap;
            system.page = page_index;
        }
    }
    let mut placed_marks = Vec::new();
    for mark in &song.marks {
        let Some((page_index, system)) = pages.iter().enumerate().find_map(|(page_index, page)| {
            page.systems
                .iter()
                .find(|system| {
                    system.measures.iter().any(|measure| {
                        mark.tick >= measure.start && mark.tick < measure.start + measure.length
                    })
                })
                .map(|system| (page_index, system))
        }) else {
            continue;
        };
        let Some(measure) = system.measures.iter().find(|measure| {
            mark.tick >= measure.start && mark.tick < measure.start + measure.length
        }) else {
            continue;
        };
        let offset = (mark.tick - measure.start) as f32 / measure.length.max(1) as f32
            * (measure.x_end - measure.x);
        let below = matches!(mark.kind, project::song::MarkKind::Dynamic(_));
        placed_marks.push(PlacedMark {
            tick: mark.tick,
            kind: mark.kind,
            x: measure.x + offset,
            y: if below {
                system.y + 4.0 * settings.staff_space + 30.0
            } else {
                system.y - 16.0
            },
            page: page_index,
        });
    }
    for lyric in &mut placed_lyrics {
        if let Some(system) = pages.get(lyric.page).and_then(|page| {
            page.systems.iter().find(|system| {
                system.measures.iter().any(|measure| {
                    lyric.tick >= measure.start && lyric.tick < measure.start + measure.length
                })
            })
        }) {
            lyric.y = system.y + 4.0 * settings.staff_space + 12.0;
        }
    }
    for note in &mut notes {
        let system = pages.get(note.page).and_then(|page| {
            page.systems.iter().find(|system| {
                system
                    .measures
                    .iter()
                    .any(|measure| measure.number == note.measure + 1)
            })
        });
        if let Some(system) = system {
            let step = (f64::from(note.pitch.as_u8()) - 60.0) / 2.0;
            note.y = system.y - (step as f32) * settings.staff_space;
        }
    }
    Engraved {
        pages,
        notes,
        track,
        measures: measure_count,
        key,
        lyrics: placed_lyrics,
        marks: placed_marks,
    }
}

fn measure_width(settings: &Settings, bar: Tick) -> f32 {
    let quarters = (bar as f32 / music_core::PPQ as f32).max(0.25);
    settings.quarter_width * quarters
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch as MCNotePitch, Velocity};
    use project::NoteId;
    use project::song::{Note, Track};

    fn song(measures: usize) -> Song {
        let mut song = Song::new("Score", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for index in 0..measures {
            track.notes.push(Note {
                id: NoteId::new(index as u32 + 1).expect("valid"),
                start: (index as Tick) * 3840 + 120,
                duration: PPQ,
                pitch: MCNotePitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        song
    }

    fn small_page() -> Settings {
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
    fn measures_break_into_systems_and_pages() {
        let engraved = engrave(&song(12), TrackId::new(1).expect("valid"), small_page());
        assert_eq!(engraved.measures, 12);
        assert!(engraved.pages.len() >= 2, "small page must break");
        assert_eq!(
            engraved
                .pages
                .iter()
                .map(|page| page.systems.len())
                .sum::<usize>(),
            engraved.systems()
        );
        let first = &engraved.pages[0].systems[0].measures[0];
        assert_eq!(first.number, 1);
        assert!(
            engraved
                .notes
                .iter()
                .all(|note| note.page < engraved.pages.len())
        );
    }

    #[test]
    fn a_wide_page_holds_many_measures_on_one_system() {
        let engraved = engrave(
            &song(4),
            TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        assert_eq!(engraved.pages.len(), 1);
        assert_eq!(
            engraved.pages[0].systems.len(),
            2,
            "three measures fit a system"
        );
        assert_eq!(engraved.pages[0].systems[0].measures.len(), 3);
    }

    #[test]
    fn notes_are_ordered_and_inside_their_measures() {
        let engraved = engrave(
            &song(4),
            TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        assert_eq!(engraved.notes.len(), 4);
        let ticks: Vec<Tick> = engraved.notes.iter().map(|note| note.tick).collect();
        let mut sorted = ticks.clone();
        sorted.sort_unstable();
        assert_eq!(ticks, sorted, "notes come out in tick order");
        for note in &engraved.notes {
            let measure = engraved
                .pages
                .iter()
                .flat_map(|page| &page.systems)
                .flat_map(|system| &system.measures)
                .find(|measure| measure.number == note.measure + 1)
                .expect("measure");
            assert!(note.x >= measure.x && note.x <= measure.x_end);
        }
    }

    #[test]
    fn touching_notes_of_one_pitch_are_tied() {
        let mut score = song(1);
        score.tracks[0].notes[0].start = 0;
        score.tracks[0].notes[0].duration = 960;
        let mut second = score.tracks[0].notes[0].clone();
        second.id = NoteId::new(99).expect("valid");
        second.start = 960;
        score.tracks[0].notes.push(second);
        let engraved = engrave(&score, TrackId::new(1).expect("valid"), Settings::default());
        assert!(engraved.notes[0].tie_start);
        assert!(engraved.notes[1].tied);
        assert!(!engraved.notes[0].tied);
    }

    #[test]
    fn marks_get_positions_above_and_below_the_staff() {
        use project::song::{Articulation, Dynamic, Mark, MarkKind};
        let mut score = song(2);
        score.marks.push(Mark {
            tick: 120,
            kind: MarkKind::Dynamic(Dynamic::F),
        });
        score.marks.push(Mark {
            tick: 3840 + 120,
            kind: MarkKind::Articulation(Articulation::Staccato),
        });
        let engraved = engrave(&score, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.marks.len(), 2);
        let staff_bottom = engraved.pages[0].systems[0].y + 4.0 * 8.0;
        assert!(engraved.marks[0].y > staff_bottom, "dynamics sit below");
        assert!(
            engraved.marks[1].y < engraved.pages[0].systems[0].y,
            "articulations above"
        );
    }

    #[test]
    fn song_lyrics_are_placed_without_an_explicit_list() {
        let mut score = song(2);
        score.lyrics.push(project::song::LyricEvent {
            tick: 120,
            text: String::from("hel"),
        });
        score.lyrics.push(project::song::LyricEvent {
            tick: 3840 + 120,
            text: String::from("lo"),
        });
        let engraved = engrave(&score, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.lyrics.len(), 2);
        assert_eq!(engraved.lyrics[0].text, "hel");
        assert_eq!(engraved.lyrics[1].text, "lo");
    }

    #[test]
    fn lyrics_sit_under_their_notes() {
        let score = song(2);
        let engraved = engrave_with(
            &score,
            TrackId::new(1).expect("valid"),
            Settings::default(),
            &["la", "la"],
        );
        assert_eq!(engraved.lyrics.len(), 2);
        assert_eq!(engraved.lyrics[0].x, engraved.notes[0].x);
        assert!(engraved.lyrics[0].y > engraved.notes[0].y);
        assert_eq!(engraved.lyrics[1].page, engraved.notes[1].page);
    }

    #[test]
    fn a_forty_bar_piece_spans_two_pages() {
        let mut score = Song::new("Acceptance", 120.0);
        score.chords.push(project::song::ChordEvent {
            tick: 0,
            chord: music_core::Chord::parse("C").expect("valid"),
        });
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for bar in 0..40_i64 {
            for (offset, name) in [(0_i64, "C4"), (0, "E4"), (0, "G4")] {
                let mut entry = Note {
                    id: NoteId::new(track.notes.len() as u32 + 1).expect("valid"),
                    start: bar * 3840 + offset,
                    duration: 3840,
                    pitch: MCNotePitch::from_name(name).expect("valid"),
                    velocity: Velocity::new(100).expect("valid"),
                    channel: Channel::first(),
                };
                if offset == 0 {
                    entry.duration = 3840;
                }
                track.notes.push(entry);
            }
        }
        score.tracks.push(track);
        let lyrics: Vec<&str> = vec!["la"; 120];
        let engraved = engrave_with(
            &score,
            TrackId::new(1).expect("valid"),
            Settings::default(),
            &lyrics,
        );
        assert!(engraved.pages.len() >= 2, "40 bars need more than one page");
        assert_eq!(engraved.lyrics.len(), 120);
        let tied = engraved.notes.iter().filter(|note| note.tied).count();
        assert!(tied > 0, "bar-to-bar notes are tied");
        let pdf = pdf::to_pdf(&engraved, Settings::default(), "Acceptance");
        assert_eq!(pdf::page_count(&pdf), Some(engraved.pages.len()));
    }

    #[test]
    fn entry_mapping_inverts_placement() {
        let settings = Settings::default();
        let score = song(1);
        let engraved = engrave(&score, TrackId::new(1).expect("valid"), settings);
        let system = &engraved.pages[0].systems[0];
        let note = &engraved.notes[0];
        assert_eq!(
            pitch_at_y(system.y, note.y, settings),
            60,
            "C4 sits seven steps above the bottom line"
        );
        let measure = &system.measures[0];
        let middle = (measure.x + measure.x_end) / 2.0;
        let tick = tick_at_x(measure, middle, music_core::PPQ / 4);
        assert!(tick > measure.start && tick < measure.start + measure.length);
        assert_eq!(tick % (music_core::PPQ / 4), 0);
    }

    #[test]
    fn chromatic_notes_carry_accidentals_and_the_key_is_reported() {
        let mut score = song(1);
        score.chords.push(project::song::ChordEvent {
            tick: 0,
            chord: music_core::Chord::parse("C").expect("valid"),
        });
        score.tracks[0].notes[0].pitch = MCNotePitch::from_name("C#4").expect("valid");
        let engraved = engrave(&score, TrackId::new(1).expect("valid"), Settings::default());
        let note = &engraved.notes[0];
        assert_eq!(note.spelled.step, 0);
        assert_eq!(note.spelled.alter, 1);
        assert_eq!(note.spelled.accidental, Some(1));
        assert_eq!(engraved.key.tonic.as_u8(), 0);
    }

    #[test]
    fn high_pitches_sit_above_low_ones() {
        let mut score = song(1);
        score.tracks[0].notes.push(Note {
            id: NoteId::new(9).expect("valid"),
            start: 120,
            duration: PPQ,
            pitch: MCNotePitch::from_name("C6").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        let engraved = engrave(&score, TrackId::new(1).expect("valid"), Settings::default());
        let c4 = engraved
            .notes
            .iter()
            .find(|note| note.pitch.as_u8() == 60)
            .expect("C4");
        let c6 = engraved
            .notes
            .iter()
            .find(|note| note.pitch.as_u8() == 84)
            .expect("C6");
        assert!(c6.y < c4.y, "higher pitch is higher on the page");
    }
}
