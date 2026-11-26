//! Engraving layout, toolkit-independent (ADR-010).
//!
//! `engrave` turns a song into an immutable layout of pages, systems, measures
//! and positioned notes. Renderers (egui canvas, PDF) consume this and make no
//! musical decisions. The first version lays out one track on one staff with
//! fixed measure widths; spelling, beams, ties, collisions, lyrics and marks
//! come later.

pub mod cecilia;
pub mod clef;
pub mod full;
pub mod guitar;
pub mod pdf;
pub mod spelling;
pub mod unicode_font;

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
    /// Lay the track out on treble and bass staves, splitting at middle C.
    pub grand_staff: bool,
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
            measure_gap: 16.0,
            grand_staff: false,
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
    /// Extra leftward shift for a crowded accidental.
    pub accidental_shift: f32,
    /// Augmentation dots: 0, 1 or 2.
    pub dots: u8,
}

/// A beam joining short notes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    /// Page holding the beam.
    pub page: usize,
    /// First notehead x.
    pub start_x: f32,
    /// Last notehead x.
    pub end_x: f32,
    /// Beam line y.
    pub y: f32,
    /// Notes joined.
    pub notes: usize,
    /// True when a sixteenth beam is needed too.
    pub sixteenth: bool,
}

/// A chord symbol above the staff.
#[derive(Clone, Debug, PartialEq)]
pub struct ChordSymbol {
    /// Tick of the chord.
    pub tick: Tick,
    /// Page holding the symbol.
    pub page: usize,
    /// Left edge.
    pub x: f32,
    /// Baseline y.
    pub y: f32,
    /// Written chord name.
    pub name: String,
}

/// A guitar chord frame above the staff.
#[derive(Clone, Debug, PartialEq)]
pub struct ChordFrame {
    /// Tick of the chord.
    pub tick: Tick,
    /// Page holding the frame.
    pub page: usize,
    /// Left edge.
    pub x: f32,
    /// Bottom edge (the staff side).
    pub y: f32,
    /// Written chord name.
    pub name: String,
    /// Fret per string, low E first.
    pub frets: [Option<u8>; 6],
    /// Base fret.
    pub base_fret: u8,
}

/// A phrase slur curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlurArc {
    /// Page holding the slur.
    pub page: usize,
    /// First notehead x.
    pub start_x: f32,
    /// Last notehead x.
    pub end_x: f32,
    /// Curve height above the staff.
    pub y: f32,
}

/// A rest glyph kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestKind {
    /// Whole rest.
    Whole,
    /// Half rest.
    Half,
    /// Quarter rest.
    Quarter,
    /// Eighth rest.
    Eighth,
}

/// One placed rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedRest {
    /// Page holding the rest.
    pub page: usize,
    /// Horizontal position.
    pub x: f32,
    /// Vertical position.
    pub y: f32,
    /// Glyph kind.
    pub kind: RestKind,
    /// Gap length this glyph covers.
    pub ticks: Tick,
    /// Augmentation dots: 0 or 1.
    pub dots: u8,
    /// Measure index, from zero.
    pub measure: usize,
}

/// A tie joining two notes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tie {
    /// Page holding the tie.
    pub page: usize,
    /// Start notehead x.
    pub start_x: f32,
    /// End notehead x.
    pub end_x: f32,
    /// Notehead y on the starting note.
    pub y: f32,
}

/// A tuplet bracket.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuplet {
    /// Page holding the bracket.
    pub page: usize,
    /// Bracket start x.
    pub start_x: f32,
    /// Bracket end x.
    pub end_x: f32,
    /// Bracket y.
    pub y: f32,
    /// Notes in the tuplet.
    pub count: usize,
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

/// Maps an x from one track's system into another track's measure geometry.
///
/// Full scores share barlines taken from the first track; this maps later
/// tracks' note positions proportionally inside the matching measure.
#[must_use]
pub fn full_score_x(system: &System, reference: &System, x: f32) -> f32 {
    let Some(measure) = system
        .measures
        .iter()
        .find(|measure| x >= measure.x && x <= measure.x_end)
    else {
        return x;
    };
    let Some(reference) = reference
        .measures
        .iter()
        .find(|entry| entry.number == measure.number)
    else {
        return x;
    };
    let width = (measure.x_end - measure.x).max(1.0);
    let ratio = ((x - measure.x) / width).clamp(0.0, 1.0);
    reference.x + ratio * (reference.x_end - reference.x)
}

/// The pitch a click at `y` means on a system whose staff starts at `system_y`.
#[must_use]
pub fn pitch_at_y_staff(system_y: f32, y: f32, settings: Settings, staves: usize) -> u8 {
    let mut y = y;
    if staves == 2 && y > system_y + 3.0 * settings.staff_space {
        y -= 6.0 * settings.staff_space;
    }
    pitch_at_y(system_y, y, settings)
}

pub fn pitch_at_y(system_y: f32, y: f32, settings: Settings) -> u8 {
    let steps = (system_y - y) / settings.staff_space.max(1.0);
    (60.0 + 2.0 * steps).round().clamp(0.0, 127.0) as u8
}

/// Vertical position for an augmentation dot: on a staff line, move the dot to
/// the space above; otherwise keep the notehead position.
#[must_use]
pub fn dot_y(y: f32, system_y: f32, staff_space: f32) -> f32 {
    if staff_space <= 0.0 {
        return y;
    }
    let relative = (y - system_y) / staff_space;
    if (relative - relative.round()).abs() < 0.2 {
        y - staff_space * 0.5
    } else {
        y
    }
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
    /// Song meter, drawn at each system start.
    pub time_signature: music_core::TimeSignature,
    /// Lyrics, one syllable per placed note.
    pub lyrics: Vec<Lyric>,
    /// Dynamics and articulations.
    pub marks: Vec<PlacedMark>,
    /// Beams for short notes.
    pub beams: Vec<Beam>,
    /// Tuplet brackets.
    pub tuplets: Vec<Tuplet>,
    /// One staff, or two for a grand staff.
    pub staves: usize,
    /// Ties joining notes of the same pitch.
    pub ties: Vec<Tie>,
    /// Rests in gaps and empty measures.
    pub rests: Vec<PlacedRest>,
    /// Phrase slurs.
    pub slurs: Vec<SlurArc>,
    /// True when the track is shown on a percussion staff.
    pub percussion: bool,
    /// Guitar chord frames.
    pub frames: Vec<ChordFrame>,
    /// Chord symbols above the staff.
    pub chord_symbols: Vec<ChordSymbol>,
}

impl Engraved {
    /// The system holding a zero-based measure on a page.
    #[must_use]
    pub fn system_for(&self, page: usize, measure: usize) -> Option<&System> {
        self.pages.get(page)?.systems.iter().find(|system| {
            system
                .measures
                .iter()
                .any(|item| item.number == measure + 1)
        })
    }

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

fn shift_accidentals(notes: &mut [PlacedNote], settings: Settings) {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (index, note) in notes.iter().enumerate() {
        if note.spelled.accidental.is_some() {
            groups
                .entry((note.page, note.measure))
                .or_default()
                .push(index);
        }
    }
    for indices in groups.values_mut() {
        indices.sort_by(|left, right| {
            notes[*left]
                .x
                .partial_cmp(&notes[*right].x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut right_edge = f32::MIN;
        let mut previous_x = f32::MIN;
        for index in indices.iter() {
            let note = &mut notes[*index];
            let base = note.x - 1.45 * settings.staff_space;
            if (note.x - previous_x).abs() > 0.5 && base < right_edge + 0.15 * settings.staff_space
            {
                note.accidental_shift = right_edge + 0.15 * settings.staff_space - base;
            }
            previous_x = note.x;
            right_edge = note.x + 0.65 * settings.staff_space;
        }
    }
}

fn placed_chord_symbols(song: &Song, pages: &[Page], settings: Settings) -> Vec<ChordSymbol> {
    use music_core::Spelling;
    let mut symbols = Vec::new();
    let mut last: std::collections::BTreeMap<(usize, i32), f32> = std::collections::BTreeMap::new();
    for event in &song.chords {
        let frame = guitar::shape(&event.chord).is_some();
        for (page_index, page) in pages.iter().enumerate() {
            let mut placed = None;
            for system in &page.systems {
                for measure in &system.measures {
                    if event.tick >= measure.start && event.tick < measure.start + measure.length {
                        let fraction = (event.tick - measure.start) as f32 / measure.length as f32;
                        let mut x = measure.x + fraction * (measure.x_end - measure.x);
                        let key = (page_index, (system.y * 10.0) as i32);
                        if let Some(previous) = last.get(&key)
                            && x < previous + 34.0
                        {
                            x = previous + 34.0;
                        }
                        last.insert(key, x);
                        placed = Some(ChordSymbol {
                            tick: event.tick,
                            page: page_index,
                            x,
                            y: if frame {
                                system.y - 8.0 * settings.staff_space
                            } else {
                                system.y - 2.4 * settings.staff_space
                            },
                            name: event.chord.format(Spelling::Sharp),
                        });
                        break;
                    }
                }
                if placed.is_some() {
                    break;
                }
            }
            if let Some(symbol) = placed {
                symbols.push(symbol);
                break;
            }
        }
    }
    symbols
}

fn placed_frames(song: &Song, pages: &[Page], settings: Settings) -> Vec<ChordFrame> {
    use music_core::Spelling;
    let mut frames = Vec::new();
    let mut last: std::collections::BTreeMap<(usize, i32), f32> = std::collections::BTreeMap::new();
    for event in &song.chords {
        let Some(shape) = guitar::shape(&event.chord) else {
            continue;
        };
        let mut placed = None;
        for (page_index, page) in pages.iter().enumerate() {
            for system in &page.systems {
                for measure in &system.measures {
                    if event.tick >= measure.start && event.tick < measure.start + measure.length {
                        let fraction = (event.tick - measure.start) as f32 / measure.length as f32;
                        let mut x = measure.x + fraction * (measure.x_end - measure.x);
                        let key = (page_index, (system.y * 10.0) as i32);
                        if let Some(previous) = last.get(&key)
                            && x < previous + 50.0
                        {
                            x = previous + 50.0;
                        }
                        last.insert(key, x);
                        placed = Some(ChordFrame {
                            tick: event.tick,
                            page: page_index,
                            x,
                            y: system.y - 5.0 * settings.staff_space,
                            name: shape.name.clone(),
                            frets: shape.frets,
                            base_fret: shape.base_fret,
                        });
                        break;
                    }
                }
                if placed.is_some() {
                    break;
                }
            }
            if placed.is_some() {
                break;
            }
        }
        if let Some(mut frame) = placed {
            frame.name = event.chord.format(Spelling::Sharp);
            frames.push(frame);
        }
    }
    frames
}

fn spread_lyrics(lyrics: &mut [Lyric]) {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<(usize, i32), Vec<usize>> = BTreeMap::new();
    for (index, lyric) in lyrics.iter().enumerate() {
        groups
            .entry((lyric.page, (lyric.y * 10.0).round() as i32))
            .or_default()
            .push(index);
    }
    for indices in groups.values_mut() {
        indices.sort_by(|left, right| {
            lyrics[*left]
                .x
                .partial_cmp(&lyrics[*right].x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut right_edge = f32::MIN;
        for index in indices.iter() {
            let lyric = &mut lyrics[*index];
            if lyric.x < right_edge {
                lyric.x = right_edge;
            }
            let width = (lyric.text.chars().count() as f32 * 4.6).max(4.0);
            right_edge = lyric.x + width + 4.0;
        }
    }
}

fn placed_slurs(notes: &[PlacedNote], slurs: &[project::song::Slur]) -> Vec<SlurArc> {
    let mut out = Vec::new();
    for slur in slurs {
        let mut inside: Vec<&PlacedNote> = notes
            .iter()
            .filter(|note| note.tick >= slur.start && note.tick <= slur.end)
            .collect();
        inside.sort_by_key(|note| note.tick);
        let (Some(first), Some(last)) = (inside.first(), inside.last()) else {
            continue;
        };
        if first.tick == last.tick || first.page != last.page {
            continue;
        }
        let highest = inside.iter().map(|note| note.y).fold(f32::MAX, f32::min);
        out.push(SlurArc {
            page: first.page,
            start_x: first.x,
            end_x: last.x,
            y: highest - 14.0,
        });
    }
    out
}

fn placed_rests(notes: &[PlacedNote], pages: &[Page], settings: Settings) -> Vec<PlacedRest> {
    let mut rests = Vec::new();
    for (page_index, page) in pages.iter().enumerate() {
        for system in &page.systems {
            for measure in &system.measures {
                let mut spans: Vec<(Tick, Tick)> = notes
                    .iter()
                    .filter(|note| note.measure + 1 == measure.number && note.page == page_index)
                    .map(|note| {
                        let end = (note.tick + note.duration).min(measure.start + measure.length);
                        (note.tick.max(measure.start), end)
                    })
                    .filter(|(start, end)| end > start)
                    .collect();
                spans.sort_by_key(|(start, _)| *start);
                let mut merged: Vec<(Tick, Tick)> = Vec::new();
                for span in spans {
                    if let Some(last) = merged.last_mut()
                        && span.0 <= last.1
                    {
                        last.1 = last.1.max(span.1);
                    } else {
                        merged.push(span);
                    }
                }
                let mut gaps = Vec::new();
                let mut cursor = measure.start;
                for (start, end) in merged {
                    if start > cursor {
                        gaps.push((cursor, start));
                    }
                    cursor = cursor.max(end);
                }
                if cursor < measure.start + measure.length {
                    gaps.push((cursor, measure.start + measure.length));
                }
                let units = [
                    (music_core::PPQ * 4, RestKind::Whole, 0),
                    (music_core::PPQ * 3, RestKind::Half, 1),
                    (music_core::PPQ * 2, RestKind::Half, 0),
                    (music_core::PPQ * 3 / 2, RestKind::Quarter, 1),
                    (music_core::PPQ, RestKind::Quarter, 0),
                    (music_core::PPQ * 3 / 4, RestKind::Eighth, 1),
                    (music_core::PPQ / 2, RestKind::Eighth, 0),
                ];
                for (start, end) in gaps {
                    let mut at = start;
                    while end - at >= music_core::PPQ / 2 {
                        let remaining = end - at;
                        let (length, kind, dots) = units
                            .iter()
                            .find(|(length, _, _)| *length <= remaining)
                            .copied()
                            .unwrap_or((music_core::PPQ / 2, RestKind::Eighth, 0));
                        let offset = (at - measure.start) as f32 / measure.length as f32
                            * (measure.x_end - measure.x);
                        rests.push(PlacedRest {
                            page: page_index,
                            x: measure.x + offset,
                            y: system.y - 2.0 * settings.staff_space,
                            kind,
                            ticks: length,
                            dots,
                            measure: measure.number - 1,
                        });
                        at += length.max(1);
                    }
                }
            }
        }
    }
    avoid_rest_collisions(&mut rests, notes, settings.staff_space);
    rests
}

/// Shifts rests vertically away from horizontally overlapping noteheads on the
/// same page and measure, snapping the displaced rest to half-space increments.
pub fn avoid_rest_collisions(rests: &mut [PlacedRest], notes: &[PlacedNote], staff_space: f32) {
    let space = staff_space.max(1.0);
    let x_margin = 1.8 * space;
    let y_margin = 1.15 * space;
    let half = 0.5 * space;
    for rest in rests {
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut collides = false;
        for note in notes {
            if note.page != rest.page || note.measure != rest.measure {
                continue;
            }
            if (note.x - rest.x).abs() <= x_margin {
                min_y = min_y.min(note.y);
                max_y = max_y.max(note.y);
                if (note.y - rest.y).abs() < y_margin {
                    collides = true;
                }
            }
        }
        if !collides || !min_y.is_finite() {
            continue;
        }
        let above = ((min_y - 1.75 * space) / half).floor() * half;
        let below = ((max_y + 1.75 * space) / half).ceil() * half;
        rest.y = if (above - rest.y).abs() <= (below - rest.y).abs() {
            above
        } else {
            below
        };
    }
}

fn tie_pairs(notes: &[PlacedNote]) -> Vec<Tie> {
    let mut ties = Vec::new();
    for (index, note) in notes.iter().enumerate() {
        if !note.tie_start {
            continue;
        }
        if let Some(next) = notes[index + 1..]
            .iter()
            .find(|next| next.tied && next.pitch == note.pitch && next.page == note.page)
        {
            ties.push(Tie {
                page: note.page,
                start_x: note.x,
                end_x: next.x,
                y: note.y,
            });
        }
    }
    ties
}

fn beams_and_tuplets(notes: &[PlacedNote]) -> (Vec<Beam>, Vec<Tuplet>) {
    let mut beams = Vec::new();
    let mut tuplets = Vec::new();
    let eighth = music_core::PPQ / 2;
    let sixteenth = music_core::PPQ / 4;
    let mut index = 0;
    while index < notes.len() {
        let note = notes[index];
        if note.tie_start || note.tied {
            index += 1;
            continue;
        }
        // Triplet run: three equal notes whose total is a quarter or eighth.
        let triplet =
            |value: Tick| value * 3 == music_core::PPQ || value * 3 == music_core::PPQ / 2;
        if triplet(note.duration) {
            let mut end = index;
            while end + 1 < notes.len()
                && notes[end + 1].duration == note.duration
                && notes[end + 1].tick == notes[end].tick + note.duration
                && notes[end + 1].measure == note.measure
            {
                end += 1;
            }
            let count = end - index + 1;
            if count == 3 {
                tuplets.push(Tuplet {
                    page: note.page,
                    start_x: note.x,
                    end_x: notes[end].x,
                    y: notes[index..=end]
                        .iter()
                        .map(|entry| entry.y)
                        .fold(f32::MAX, f32::min)
                        - 14.0,
                    count,
                });
                index = end + 1;
                continue;
            }
        }
        if note.duration <= eighth {
            let mut end = index;
            while end + 1 < notes.len()
                && notes[end + 1].duration <= eighth
                && notes[end + 1].tick == notes[end].tick + notes[end].duration
                && notes[end + 1].measure == note.measure
            {
                end += 1;
            }
            if end > index {
                let sixteenth = notes[index..=end]
                    .iter()
                    .any(|entry| entry.duration <= sixteenth);
                beams.push(Beam {
                    page: note.page,
                    start_x: note.x,
                    end_x: notes[end].x,
                    y: notes[index..=end]
                        .iter()
                        .map(|entry| entry.y)
                        .fold(f32::MAX, f32::min)
                        - 8.0,
                    notes: end - index + 1,
                    sixteenth,
                });
                index = end + 1;
                continue;
            }
        }
        index += 1;
    }
    (beams, tuplets)
}

/// The most measures a score lays out (about 5 hours of 4/4 at 120 bpm).
pub(crate) const MAX_MEASURES: i64 = 10_000;

/// Lays out one track, placing one lyric syllable per note.
#[must_use]
pub fn engrave_with(song: &Song, track: TrackId, settings: Settings, lyrics: &[&str]) -> Engraved {
    let bar = song.time_signature.bar_ticks().max(1);
    let end = song.end_tick().max(bar);
    let measure_count =
        usize::try_from((end.saturating_add(bar - 1) / bar).min(MAX_MEASURES)).unwrap_or(1);
    let content_width = (settings.page_width - 2.0 * settings.margin).max(1.0);
    let content_height = (settings.page_height - 2.0 * settings.margin).max(settings.system_gap);
    let has_frames = song
        .chords
        .iter()
        .any(|event| guitar::shape(&event.chord).is_some());
    let above_spaces = if has_frames {
        8.2
    } else if song.chords.is_empty() {
        0.0
    } else {
        3.0
    };
    let top_reserve = if content_height / settings.system_gap >= 3.0 {
        above_spaces * settings.staff_space
    } else {
        0.0
    };
    let per_page = ((content_height - top_reserve).max(settings.system_gap) / settings.system_gap)
        .floor()
        .max(1.0) as usize;

    let key = spelling::detect_key(song);
    let mut pages = Vec::new();
    let mut notes: Vec<PlacedNote> = Vec::new();
    let mut placed_lyrics: Vec<Lyric> = Vec::new();
    let mut lyric_index = 0;
    let mut open_ties: std::collections::BTreeMap<u8, (usize, Tick)> =
        std::collections::BTreeMap::new();
    let source = song.tracks.iter().find(|entry| entry.id == track);
    let percussion = source.is_some_and(|track| {
        !track.notes.is_empty()
            && track
                .notes
                .iter()
                .all(|note| note.channel.zero_based() == 9)
    });
    let widths: Vec<f32> = (0..measure_count)
        .map(|index| required_measure_width(source, index as Tick * bar, bar, &settings))
        .collect();
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
            while measure_index < measure_count {
                let width = widths[measure_index];
                if !measures.is_empty() && x + width > settings.margin + content_width + 0.01 {
                    break;
                }
                let start = (measure_index as Tick) * bar;
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
                            accidental_shift: 0.0,
                            dots: dots_for(note.duration),
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
                        if let Some(text) = text
                            && !placed_lyrics.iter().any(|lyric| lyric.tick == note.start)
                        {
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
                + top_reserve
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
    spread_lyrics(&mut placed_lyrics);
    let grand = settings.grand_staff;
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
            if percussion {
                note.y = system.y - 2.0 * settings.staff_space;
            } else {
                let step = (f64::from(note.pitch.as_u8()) - 60.0) / 2.0;
                note.y = system.y - (step as f32) * settings.staff_space;
                if grand && note.pitch.as_u8() < 60 {
                    note.y += 6.0 * settings.staff_space;
                }
            }
        }
    }
    shift_accidentals(&mut notes, settings);
    let (beams, tuplets) = beams_and_tuplets(&notes);
    let ties = tie_pairs(&notes);
    let rests = placed_rests(&notes, &pages, settings);
    let slurs = placed_slurs(
        &notes,
        source.map_or(&[][..], |track| track.slurs.as_slice()),
    );
    let frames = placed_frames(song, &pages, settings);
    let chord_symbols = placed_chord_symbols(song, &pages, settings);
    Engraved {
        pages,
        notes,
        track,
        measures: measure_count,
        key,
        time_signature: song.time_signature,
        lyrics: placed_lyrics,
        marks: placed_marks,
        beams,
        tuplets,
        staves: if settings.grand_staff { 2 } else { 1 },
        ties,
        rests,
        slurs,
        percussion,
        frames,
        chord_symbols,
    }
}

fn dots_for(duration: Tick) -> u8 {
    for base in [3840_i64, 1920, 960, 480, 240, 120, 60] {
        if duration == base * 3 / 2 {
            return 1;
        }
        if duration == base * 7 / 4 {
            return 2;
        }
    }
    0
}

fn required_measure_width(
    source: Option<&project::song::Track>,
    start: Tick,
    bar: Tick,
    settings: &Settings,
) -> f32 {
    let base = measure_width(settings, bar);
    let Some(track) = source else {
        return base;
    };
    let mut starts: Vec<Tick> = track
        .notes
        .iter()
        .filter(|note| note.start >= start && note.start < start + bar)
        .map(|note| note.start)
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let Some(min_gap) = starts.windows(2).map(|pair| pair[1] - pair[0]).min() else {
        return base;
    };
    if min_gap <= 0 {
        return base;
    }
    let slots = (bar as f32 / min_gap as f32).max(1.0);
    base.max(slots * settings.staff_space * 1.15)
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
            grand_staff: false,
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
            1,
            "four measures fit a system"
        );
        assert_eq!(engraved.pages[0].systems[0].measures.len(), 4);
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
        assert_eq!(engraved.lyrics.len(), 40, "one syllable per chord tick");
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
    fn full_score_x_maps_into_the_reference_measure() {
        use crate::{Measure, System};
        let system = System {
            measures: vec![Measure {
                start: 0,
                length: 3840,
                number: 1,
                x: 50.0,
                x_end: 150.0,
            }],
            y: 100.0,
            page: 0,
        };
        let reference = System {
            measures: vec![Measure {
                start: 0,
                length: 3840,
                number: 1,
                x: 50.0,
                x_end: 200.0,
            }],
            y: 100.0,
            page: 0,
        };
        assert!((crate::full_score_x(&system, &reference, 100.0) - 125.0).abs() < 0.01);
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

#[cfg(test)]
mod notation_tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, Song, Track};

    fn song_with(durations: &[(Tick, Tick)]) -> Song {
        let mut song = Song::new("Notation", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, (start, duration)) in durations.iter().enumerate() {
            track.notes.push(Note {
                id: project::NoteId::new(index as u32 + 1).expect("valid"),
                start: *start,
                duration: *duration,
                pitch: Pitch::from_name("C4")
                    .expect("valid")
                    .transpose(index as i8),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        song
    }

    #[test]
    fn eighth_notes_beam_in_a_group() {
        let song = song_with(&[
            (0, PPQ / 2),
            (PPQ / 2, PPQ / 2),
            (PPQ, PPQ / 2),
            (PPQ * 3 / 2, PPQ / 2),
        ]);
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.beams.len(), 1);
        assert_eq!(engraved.beams[0].notes, 4);
        assert!(!engraved.beams[0].sixteenth);
    }

    #[test]
    fn sixteenth_groups_need_a_second_beam() {
        let song = song_with(&[
            (0, PPQ / 4),
            (PPQ / 4, PPQ / 4),
            (PPQ / 2, PPQ / 4),
            (PPQ * 3 / 4, PPQ / 4),
        ]);
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.beams.len(), 1);
        assert!(engraved.beams[0].sixteenth);
    }

    #[test]
    fn triplet_runs_get_a_bracket() {
        let song = song_with(&[(0, PPQ / 3), (PPQ / 3, PPQ / 3), (PPQ * 2 / 3, PPQ / 3)]);
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.tuplets.len(), 1);
        assert_eq!(engraved.tuplets[0].count, 3);
        assert!(engraved.beams.is_empty());
    }
}

#[cfg(test)]
mod grand_staff_tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, Song, Track};

    fn piano() -> Song {
        let mut song = Song::new("Grand", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, name) in ["C4", "A3"].iter().enumerate() {
            track.notes.push(Note {
                id: project::NoteId::new(index as u32 + 1).expect("valid"),
                start: index as i64 * PPQ,
                duration: PPQ,
                pitch: Pitch::from_name(name).expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        song
    }

    #[test]
    fn clicks_map_back_to_the_right_staff_octave() {
        let settings = Settings::default();
        let system_y = 100.0;
        assert_eq!(
            pitch_at_y_staff(system_y, system_y, settings, 2),
            60,
            "treble line keeps C4"
        );
        assert_eq!(
            pitch_at_y_staff(system_y, system_y + 7.5 * settings.staff_space, settings, 2),
            57,
            "the bass offset maps back to A3"
        );
        assert!(
            pitch_at_y_staff(system_y, system_y + 8.0 * settings.staff_space, settings, 2) < 60,
            "the second staff stays below middle C"
        );
    }

    #[test]
    fn a_grand_staff_splits_at_middle_c_and_offsets_bass_notes() {
        let song = piano();
        let settings = Settings {
            grand_staff: true,
            ..Settings::default()
        };
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), settings);
        assert_eq!(engraved.staves, 2);
        let treble = engraved
            .notes
            .iter()
            .find(|note| note.pitch.as_u8() == 60)
            .expect("treble");
        let bass = engraved
            .notes
            .iter()
            .find(|note| note.pitch.as_u8() == 57)
            .expect("bass");
        assert!(
            (bass.y - treble.y - 7.5 * settings.staff_space).abs() < 0.001,
            "bass sits seven and a half staff spaces lower"
        );
        let single = engrave(
            &song,
            TrackId::new(1).expect("valid"),
            Settings {
                grand_staff: false,
                ..Settings::default()
            },
        );
        assert_eq!(single.staves, 1);
        assert!(
            single
                .notes
                .iter()
                .all(|note| (note.y - treble.y).abs() <= 2.0 * settings.staff_space),
            "single staff keeps both notes around one staff"
        );
    }
}

#[cfg(test)]
mod tie_tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, Song, Track};

    #[test]
    fn adjacent_same_pitch_notes_get_one_tie() {
        let mut song = Song::new("Ties", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, start) in [(1_u32, 0_i64), (2, PPQ)] {
            track.notes.push(Note {
                id: project::NoteId::new(index).expect("valid"),
                start,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.ties.len(), 1);
        assert!(engraved.ties[0].end_x > engraved.ties[0].start_x);
    }
}

#[cfg(test)]
mod rest_tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, Song, Track};

    #[test]
    fn dotted_rests_cover_dotted_gaps() {
        let mut song = Song::new("Rests", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, start) in [(1_u32, 0_i64), (2, 2400)] {
            track.notes.push(Note {
                id: project::NoteId::new(index).expect("valid"),
                start,
                duration: 960,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        let dotted = engraved
            .rests
            .iter()
            .find(|rest| rest.dots == 1)
            .expect("a dotted rest");
        assert_eq!(dotted.kind, RestKind::Quarter);
        assert_eq!(dotted.ticks, 1440);
    }

    #[test]
    fn gaps_and_empty_measures_get_rests() {
        let mut song = Song::new("Rests", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, start) in [(1_u32, 0_i64), (2, 8 * PPQ)] {
            track.notes.push(Note {
                id: project::NoteId::new(index).expect("valid"),
                start,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        let measure = |index: usize| -> Vec<RestKind> {
            engraved
                .rests
                .iter()
                .filter(|rest| rest.measure == index)
                .map(|rest| rest.kind)
                .collect()
        };
        assert_eq!(
            measure(0),
            vec![RestKind::Half],
            "2880 ticks is a dotted half"
        );
        assert!(
            engraved
                .rests
                .iter()
                .any(|rest| rest.measure == 0 && rest.dots == 1)
        );
        assert_eq!(measure(1), vec![RestKind::Whole]);
        assert!(engraved.rests.iter().all(|rest| rest.x > 0.0));
    }
}

#[cfg(test)]
mod slur_tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, Slur, Song, Track};

    fn song() -> Song {
        let mut song = Song::new("Slurs", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, start) in [(1_u32, 0_i64), (2, PPQ)] {
            track.notes.push(Note {
                id: project::NoteId::new(index).expect("valid"),
                start,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        song
    }

    #[test]
    fn a_slur_becomes_one_curve_between_its_notes() {
        let song = song();
        let plain = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert!(plain.slurs.is_empty());
        let mut song = song;
        song.tracks[0].slurs.push(Slur { start: 0, end: PPQ });
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.slurs.len(), 1);
        assert!(engraved.slurs[0].end_x > engraved.slurs[0].start_x);
    }
}

#[cfg(test)]
mod percussion_tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{Note, Song, Track};

    #[test]
    fn a_drum_track_uses_one_line_and_equal_note_heights() {
        let mut song = Song::new("Drums", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Drums");
        for (index, pitch) in [36_u8, 38, 42].iter().enumerate() {
            track.notes.push(Note {
                id: project::NoteId::new(index as u32 + 1).expect("valid"),
                start: index as i64 * PPQ,
                duration: PPQ,
                pitch: Pitch::new(*pitch).expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::drums(),
            });
        }
        song.tracks.push(track);
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert!(engraved.percussion);
        let first = engraved.notes[0].y;
        assert!(
            engraved
                .notes
                .iter()
                .all(|note| (note.y - first).abs() < 0.001)
        );

        let mut pitched = Song::new("Pitched", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        pitched.tracks.push(track);
        let engraved = engrave(
            &pitched,
            TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        assert!(!engraved.percussion);
    }
}

#[cfg(test)]
mod lyric_spacing_tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{LyricEvent, Note, Song, Track};

    #[test]
    fn dots_on_lines_move_to_the_space_above() {
        assert!((dot_y(100.0, 100.0, 10.0) - 95.0).abs() < 1.0e-6);
        assert!((dot_y(105.0, 100.0, 10.0) - 105.0).abs() < 1.0e-6);
        assert!((dot_y(120.0, 100.0, 10.0) - 115.0).abs() < 1.0e-6);
        assert!((dot_y(100.0, 100.0, 0.0) - 100.0).abs() < 1.0e-6);
    }

    #[test]
    fn dotted_durations_get_one_or_two_dots() {
        assert_eq!(dots_for(960), 0);
        assert_eq!(dots_for(1440), 1);
        assert_eq!(dots_for(1000), 0);
        assert_eq!(dots_for(1680), 2);
        let mut song = Song::new("Dotted", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: 1440,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.notes[0].dots, 1);
    }

    #[test]
    fn dense_rhythms_widen_their_measure() {
        let mut song = Song::new("Dense", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for index in 0..16_u32 {
            track.notes.push(Note {
                id: project::NoteId::new(index + 1).expect("valid"),
                start: Tick::from(index) * 240,
                duration: 240,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        let base = Settings::default().quarter_width * 4.0;
        let width = engraved.pages[0].systems[0].measures[0].x_end
            - engraved.pages[0].systems[0].measures[0].x;
        assert!(width > base, "sixteenths need more than quarter spacing");
        assert!(
            engraved
                .notes
                .iter()
                .all(|note| note.x >= engraved.pages[0].systems[0].measures[0].x)
        );
    }

    #[test]
    fn crowded_accidentals_shift_left() {
        let mut song = Song::new("Accidentals", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, (start, pitch)) in [(0_i64, "C#4"), (240, "D#4")].into_iter().enumerate() {
            track.notes.push(Note {
                id: project::NoteId::new(index as u32 + 1).expect("valid"),
                start,
                duration: 240,
                pitch: Pitch::from_name(pitch).expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        song.chords.push(project::song::ChordEvent {
            tick: 0,
            chord: music_core::Chord::parse("C").expect("valid"),
        });
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert!(engraved.notes.len() == 2);
        assert!(
            engraved.notes[1].accidental_shift > 0.0,
            "the second accidental moves left"
        );
    }

    #[test]
    fn a_chord_prints_one_lyric_per_tick() {
        let mut song = Song::new("Lyrics", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, pitch) in ["C4", "E4", "G4"].iter().enumerate() {
            track.notes.push(Note {
                id: project::NoteId::new(index as u32 + 1).expect("valid"),
                start: 0,
                duration: 960,
                pitch: Pitch::from_name(pitch).expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        song.lyrics.push(project::song::LyricEvent {
            tick: 0,
            text: String::from("Lazy"),
        });
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.lyrics.len(), 1);
    }

    #[test]
    fn crowded_lyrics_are_shifted_apart() {
        let mut song = Song::new("Lyrics", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, start) in [(1_u32, 0_i64), (2, 120)] {
            track.notes.push(Note {
                id: project::NoteId::new(index).expect("valid"),
                start,
                duration: PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
            song.lyrics.push(LyricEvent {
                tick: start,
                text: String::from(if index == 1 { "A" } else { "BB" }),
            });
        }
        song.tracks.push(track);
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.lyrics.len(), 2);
        let mut lyrics = engraved.lyrics.clone();
        lyrics.sort_by(|left, right| {
            left.x
                .partial_cmp(&right.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        assert!(
            lyrics[1].x >= lyrics[0].x + 4.6 + 4.0,
            "the second syllable moved"
        );
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;
    use music_core::{Channel, Chord, PPQ, Pitch, Velocity};
    use project::TrackId;
    use project::song::{ChordEvent, Note, Song, Track};

    #[test]
    fn chords_get_frames_that_do_not_overlap() {
        let mut song = Song::new("Frames", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Guitar");
        track.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: 8 * PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        for (tick, name) in [(0_i64, "C"), (120, "G")] {
            song.chords.push(ChordEvent {
                tick,
                chord: Chord::parse(name).expect("valid"),
            });
        }
        let engraved = engrave(&song, TrackId::new(1).expect("valid"), Settings::default());
        assert_eq!(engraved.frames.len(), 2);
        assert_eq!(engraved.chord_symbols.len(), 2);
        assert_eq!(engraved.chord_symbols[0].name, "C");
        assert!(
            engraved.chord_symbols[0].y < engraved.frames[0].y,
            "name above frame"
        );
        assert_eq!(engraved.frames[0].name, "C");
        assert!(engraved.frames[1].x >= engraved.frames[0].x + 50.0);
        assert!(engraved.frames[0].y < 200.0, "frames sit above the staff");
        let mut unknown = song;
        unknown.chords[0].chord = Chord::parse("C#dim").expect("valid");
        let engraved = engrave(
            &unknown,
            TrackId::new(1).expect("valid"),
            Settings::default(),
        );
        assert_eq!(engraved.frames.len(), 1, "only the listed shape remains");
    }

    #[test]
    fn rests_shift_away_from_colliding_notes() {
        let space = 8.0_f32;
        let center_y = 120.0_f32;
        let note_at = |x: f32, y: f32| PlacedNote {
            tick: 0,
            duration: PPQ,
            pitch: Pitch::new(60).expect("valid"),
            x,
            y,
            page: 0,
            measure: 0,
            spelled: spelling::Spelled {
                step: 0,
                alter: 0,
                octave: 4,
                accidental: None,
            },
            tie_start: false,
            tied: false,
            accidental_shift: 0.0,
            dots: 0,
        };
        let rest_at = |x: f32| PlacedRest {
            page: 0,
            x,
            y: center_y,
            kind: RestKind::Quarter,
            ticks: PPQ,
            dots: 0,
            measure: 0,
        };

        // Unobstructed rest stays at staff center.
        let mut clear = vec![rest_at(200.0)];
        avoid_rest_collisions(&mut clear, &[note_at(80.0, center_y)], space);
        assert_eq!(clear[0].y, center_y);

        // Rest colliding with a low note cluster shifts upward away from the noteheads.
        let mut low_collision = vec![rest_at(100.0)];
        let low_notes = [
            note_at(102.0, center_y + 2.0),
            note_at(102.0, center_y + 18.0),
        ];
        avoid_rest_collisions(&mut low_collision, &low_notes, space);
        assert!(
            low_collision[0].y < center_y - space,
            "rest should shift above low colliding notes, got {}",
            low_collision[0].y
        );

        // Rest colliding with a high note cluster shifts downward away from the noteheads.
        let mut high_collision = vec![rest_at(100.0)];
        let high_notes = [
            note_at(101.0, center_y - 2.0),
            note_at(101.0, center_y - 18.0),
        ];
        avoid_rest_collisions(&mut high_collision, &high_notes, space);
        assert!(
            high_collision[0].y > center_y + space,
            "rest should shift below high colliding notes, got {}",
            high_collision[0].y
        );
    }
}
