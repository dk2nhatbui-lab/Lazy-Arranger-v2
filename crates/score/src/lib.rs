//! Engraving layout, toolkit-independent (ADR-010).
//!
//! `engrave` turns a song into an immutable layout of pages, systems, measures
//! and positioned notes. Renderers (egui canvas, PDF) consume this and make no
//! musical decisions. The first version lays out one track on one staff with
//! fixed measure widths; spelling, beams, ties, collisions, lyrics and marks
//! come later.

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
}

impl Engraved {
    /// Every page's systems.
    #[must_use]
    pub fn systems(&self) -> usize {
        self.pages.iter().map(|page| page.systems.len()).sum()
    }
}

/// Lays out one track as a scored document.
#[must_use]
pub fn engrave(song: &Song, track: TrackId, settings: Settings) -> Engraved {
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

    let mut pages = Vec::new();
    let mut notes = Vec::new();
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
                    for note in &track.notes {
                        if note.start < start || note.start >= start + bar {
                            continue;
                        }
                        let offset = f32::from(i16::try_from(note.start - start).unwrap_or(0))
                            / bar as f32
                            * width;
                        notes.push(PlacedNote {
                            tick: note.start,
                            pitch: note.pitch,
                            x: x + offset,
                            y: 0.0,
                            page: page_index,
                            measure: measure_index,
                        });
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
