//! The all-tracks score: one staff per track, with shared barlines.
//!
//! Drawn through [`Canvas`], so the desktop canvas and the PDF export match.
//!
//! It follows the engraving rules of Cecilia (the engraver behind the
//! one-track preview), which lays out one track at a time:
//! - notes sit by letter name on a treble, bass or percussion staff;
//! - clefs, heads, stems and text scale with the staff space;
//! - each staff gets room for the notes above and below it;
//! - as many systems go on a page as their height allows.
//!
//! The first track's engraving gives the system breaks and bar widths. Every
//! track places its notes in those bars by tick, so all staves line up.

use std::collections::BTreeMap;

use music_core::{PPQ, Tick};

use crate::{Engraved, PlacedNote, Settings, System};

/// A point on the page in points, y down from the top.
pub type Point = (f32, f32);

/// The colours the score uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    /// Notes, lines and text.
    Black,
    /// Secondary text (key, bar and page numbers).
    Grey,
    /// The page colour, for hollow noteheads.
    Paper,
    /// The playback position.
    Playhead,
}

/// Where a text's anchor point sits on the text: 0 left/top, 0.5 centre, 1 right/bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    /// Horizontal fraction of the text width.
    pub x: f32,
    /// Vertical fraction of the text height.
    pub y: f32,
}

#[allow(missing_docs)]
impl Anchor {
    pub const LEFT_TOP: Self = Self { x: 0.0, y: 0.0 };
    pub const CENTER_TOP: Self = Self { x: 0.5, y: 0.0 };
    pub const LEFT_CENTER: Self = Self { x: 0.0, y: 0.5 };
    pub const CENTER_CENTER: Self = Self { x: 0.5, y: 0.5 };
    pub const RIGHT_CENTER: Self = Self { x: 1.0, y: 0.5 };
    pub const LEFT_BOTTOM: Self = Self { x: 0.0, y: 1.0 };
    pub const CENTER_BOTTOM: Self = Self { x: 0.5, y: 1.0 };
}

/// A drawing surface in page points (y down), so the screen and the PDF
/// export draw the same score.
pub trait Canvas {
    /// A straight line.
    fn line(&self, from: Point, to: Point, width: f32, ink: Ink);
    /// An open path.
    fn polyline(&self, points: &[Point], width: f32);
    /// A filled convex polygon, optionally outlined in black.
    fn polygon(&self, points: &[Point], fill: Ink, outline: Option<f32>);
    /// A filled black circle.
    fn circle(&self, centre: Point, radius: f32);
    /// A filled black rectangle.
    fn fill_rect(&self, min: Point, max: Point);
    /// A black rectangle outline.
    fn stroke_rect(&self, min: Point, max: Point, width: f32);
    /// Text of a size in points.
    fn text(&self, at: Point, anchor: Anchor, text: &str, size: f32, ink: Ink);
    /// The width of a text in points.
    fn text_width(&self, text: &str, size: f32) -> f32;
    /// A treble or bass clef image filling a rectangle.
    fn clef(&self, treble: bool, min: Point, max: Point);
}

/// Room for the title block at the top of every page, in points.
const HEADER: f32 = 66.0;
/// Room for the page number at the bottom, in points.
const FOOTER: f32 = 14.0;
/// Width of the track-name column left of the staves, in points.
const LABEL: f32 = 64.0;
/// Staff space when everything fits, in points.
const BASE_SPACE: f32 = 5.5;
/// The smallest staff space, for scores with many tracks.
const MIN_SPACE: f32 = 2.5;
/// Space between systems, in staff spaces.
const SYSTEM_GAP: f32 = 2.0;
/// The most room a staff takes above or below for its notes, in staff spaces.
const MAX_REACH: f32 = 8.0;
/// Room from a barline to the first note of the bar, in staff spaces.
const BAR_LEAD: f32 = 1.4;
/// Room from the last note of a bar to the barline, in staff spaces.
const BAR_TAIL: f32 = 0.9;
/// Semitones of the natural notes C to B.
const NATURAL: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Clef {
    Treble,
    Bass,
    Percussion,
}

impl Clef {
    /// Diatonic index (octave * 7 + letter) of the top staff line.
    fn top_line(self) -> i32 {
        match self {
            Self::Treble | Self::Percussion => 38,
            Self::Bass => 26,
        }
    }
}

/// One track's staff in a system.
#[derive(Clone, Debug, PartialEq)]
struct Staff {
    clef: Clef,
    /// Room the notes need above the top line, in staff spaces.
    notes_above: f32,
    /// Room the notes need below the bottom line, in staff spaces.
    notes_below: f32,
    /// Extra rows above: markers, chord names and chord frames.
    marker_row: f32,
    chord_row: f32,
    frame_row: f32,
    /// Extra rows below: dynamics and lyrics.
    mark_row: f32,
    lyric_row: f32,
    /// Offset of the top line from the system top, in staff spaces.
    top: f32,
}

impl Staff {
    fn above(&self) -> f32 {
        self.notes_above + self.marker_row + self.chord_row + self.frame_row
    }

    fn below(&self) -> f32 {
        self.notes_below + self.mark_row + self.lyric_row
    }
}

/// The page layout of the all-tracks score, in points.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layout {
    sp: f32,
    staves: Vec<Staff>,
    /// Height of one system, in staff spaces.
    system_height: f32,
    pub(crate) systems_per_page: usize,
    pub(crate) total_systems: usize,
}

impl Layout {
    pub(crate) fn new(
        tracks: &[(String, Engraved)],
        frames: bool,
        markers: &[project::Marker],
        settings: Settings,
    ) -> Self {
        let mut staves: Vec<Staff> = tracks
            .iter()
            .map(|(name, engraved)| {
                let clef = choose_clef(name, engraved);
                // Highest and lowest ink, stems included (a stem is 3.5 spaces).
                let (low, high) = engraved
                    .notes
                    .iter()
                    .map(|note| {
                        let position = note_position(clef, note);
                        let stemmed = note.duration < 4 * PPQ;
                        if !stemmed {
                            (position, position)
                        } else if clef == Clef::Percussion || position > 4 {
                            (position - 7, position)
                        } else {
                            (position, position + 7)
                        }
                    })
                    .fold((i32::MAX, i32::MIN), |(low, high), (top, bottom)| {
                        (low.min(top), high.max(bottom))
                    });
                let (notes_above, notes_below) = if low > high {
                    (2.2, 2.2)
                } else {
                    (
                        (-low as f32 / 2.0 + 0.8).clamp(2.2, MAX_REACH),
                        ((high - 8) as f32 / 2.0 + 0.8).clamp(2.2, MAX_REACH),
                    )
                };
                Staff {
                    clef,
                    notes_above,
                    notes_below,
                    marker_row: 0.0,
                    chord_row: 0.0,
                    frame_row: 0.0,
                    mark_row: if engraved.marks.is_empty() { 0.0 } else { 1.8 },
                    lyric_row: 0.0,
                    top: 0.0,
                }
            })
            .collect();
        if let (Some(first), Some((_, engraved))) = (staves.first_mut(), tracks.first()) {
            if !markers.is_empty() {
                first.marker_row = 2.2;
            }
            if !engraved.chord_symbols.is_empty() {
                first.chord_row = 2.0;
            }
            if frames && !engraved.frames.is_empty() {
                first.frame_row = 5.5;
            }
            if !engraved.lyrics.is_empty() {
                first.lyric_row = 2.2;
            }
        }
        let mut cursor = 0.0;
        for staff in &mut staves {
            cursor += staff.above();
            staff.top = cursor;
            cursor += 4.0 + staff.below();
        }
        let system_height = cursor.max(4.0);
        let available = settings.page_height - 2.0 * settings.margin - HEADER - FOOTER;
        let sp = (available / system_height).clamp(MIN_SPACE, BASE_SPACE);
        let systems_per_page = (((available + SYSTEM_GAP * sp)
            / ((system_height + SYSTEM_GAP) * sp))
            .floor() as usize)
            .max(1);
        let total_systems = tracks
            .first()
            .map_or(0, |(_, engraved)| reference_systems(engraved).len())
            .max(1);
        Self {
            sp,
            staves,
            system_height,
            systems_per_page,
            total_systems,
        }
    }

    pub(crate) fn page_count(&self) -> usize {
        self.total_systems.div_ceil(self.systems_per_page).max(1)
    }

    /// Top of a page slot's system, in points from the page top.
    fn system_top(&self, slot: usize, settings: Settings) -> f32 {
        settings.margin + HEADER + slot as f32 * (self.system_height + SYSTEM_GAP) * self.sp
    }
}

/// The horizontal layout of one system, in points from the page left.
struct Columns {
    staff_left: f32,
    /// Barline positions: the start of the first bar, then the end of each.
    bars: Vec<f32>,
}

impl Columns {
    fn new(
        system: &System,
        widest: f32,
        index: usize,
        last: bool,
        fifths: i8,
        sp: f32,
        settings: Settings,
    ) -> Self {
        let staff_left = settings.margin + LABEL;
        let right = settings.page_width - settings.margin;
        let mut prefix = 0.8 + 4.4 + f32::from(fifths.unsigned_abs()) * 1.0 + 1.0;
        if index == 0 {
            prefix += 2.6;
        }
        let start = staff_left + prefix * sp;
        let widths: Vec<f32> = system
            .measures
            .iter()
            .map(|measure| (measure.x_end - measure.x).max(1.0))
            .collect();
        let total: f32 = widths.iter().sum::<f32>().max(1.0);
        // A short last system keeps the bar widths of the full ones.
        let scale = if last && total < widest * 0.7 {
            (right - start) / widest.max(1.0)
        } else {
            (right - start) / total
        };
        let mut bars = vec![start];
        let mut x = start;
        for width in widths {
            x += width * scale;
            bars.push(x);
        }
        Self { staff_left, bars }
    }

    fn end(&self) -> f32 {
        self.bars.last().copied().unwrap_or(self.staff_left)
    }

    /// X of a tick in the system, if one of its bars holds it.
    fn x_at(&self, system: &System, tick: Tick, sp: f32) -> Option<f32> {
        let (index, measure) = system.measures.iter().enumerate().find(|(_, measure)| {
            tick >= measure.start && tick < measure.start.saturating_add(measure.length)
        })?;
        let left = self.bars[index] + BAR_LEAD * sp;
        let right = self.bars[index + 1] - BAR_TAIL * sp;
        let ratio = if measure.length > 0 {
            (tick - measure.start) as f32 / measure.length as f32
        } else {
            0.0
        };
        Some(left + (right - left).max(0.0) * ratio)
    }

    /// Middle of a bar, for whole-bar rests.
    fn bar_middle(&self, index: usize) -> f32 {
        (self.bars[index] + self.bars[index + 1]) / 2.0
    }

    /// The tick at an x in the system, clamped to its bars.
    fn tick_at(&self, system: &System, x: f32, sp: f32) -> Option<Tick> {
        let first = system.measures.first()?;
        if x < self.bars[0] {
            return Some(first.start);
        }
        for (index, measure) in system.measures.iter().enumerate() {
            if x <= self.bars[index + 1] {
                let left = self.bars[index] + BAR_LEAD * sp;
                let right = self.bars[index + 1] - BAR_TAIL * sp;
                let ratio = if right > left {
                    ((x - left) / (right - left)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                return Some(
                    measure.start
                        + ((measure.length as f32 * ratio) as Tick)
                            .min(measure.length.saturating_sub(1)),
                );
            }
        }
        system
            .measures
            .last()
            .map(|measure| measure.start + measure.length.saturating_sub(1))
    }
}

fn reference_systems(engraved: &Engraved) -> Vec<&System> {
    engraved
        .pages
        .iter()
        .flat_map(|page| page.systems.iter())
        .collect()
}

/// Half staff spaces of a note below the top staff line.
fn note_position(clef: Clef, note: &PlacedNote) -> i32 {
    if clef == Clef::Percussion {
        return drum_position(note.pitch.as_u8());
    }
    clef.top_line() - diatonic(note)
}

/// Octave * 7 + letter of the note as written.
fn diatonic(note: &PlacedNote) -> i32 {
    let step = note.spelled.step.min(6);
    let natural = NATURAL[step] + i32::from(note.spelled.alter);
    let octave = (i32::from(note.pitch.as_u8()) - natural).div_euclid(12) - 1;
    octave * 7 + step as i32
}

/// Where a General MIDI drum sits on the percussion staff.
fn drum_position(pitch: u8) -> i32 {
    match pitch {
        35 | 36 => 7,
        41 | 43 => 6,
        45 | 47 => 4,
        48 | 50 => 2,
        42 | 44 | 46 => -1,
        49 | 52 | 55 | 57 => -2,
        51 | 53 | 59 => 0,
        _ => 3,
    }
}

/// Cymbals and hi-hats take cross heads.
fn cross_head(pitch: u8) -> bool {
    matches!(pitch, 42 | 44 | 46 | 49 | 51 | 52 | 53 | 55 | 57 | 59)
}

/// The clef that needs the fewest ledger lines for the track.
fn choose_clef(name: &str, engraved: &Engraved) -> Clef {
    if engraved.percussion || is_percussion(name) {
        return Clef::Percussion;
    }
    let ledgers = |clef: Clef| -> i32 {
        engraved
            .notes
            .iter()
            .map(|note| {
                let position = note_position(clef, note);
                if position < -1 {
                    -position / 2
                } else if position > 9 {
                    (position - 8) / 2
                } else {
                    0
                }
            })
            .sum()
    };
    if ledgers(Clef::Bass) < ledgers(Clef::Treble) {
        Clef::Bass
    } else {
        Clef::Treble
    }
}

/// Circle-of-fifths position of the key: sharps positive, flats negative.
fn fifths(key: crate::spelling::Key) -> i8 {
    let major = if key.minor {
        key.tonic.transpose(3)
    } else {
        key.tonic
    };
    match major.as_u8() {
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

/// The written note value (without dots) and dot count closest to a played
/// length that fits in `room` ticks, as Cecilia rounds played notes.
fn written(played: Tick, room: Tick) -> (Tick, u8) {
    let room = room.max(PPQ / 4);
    let values = [4, 2, 1]
        .into_iter()
        .map(|quarters| quarters * PPQ)
        .chain([PPQ / 2, PPQ / 4]);
    let mut best = (PPQ / 4, 0, Tick::MAX);
    for base in values {
        for dots in 0..=1_u8 {
            let value = if dots == 1 { base * 3 / 2 } else { base };
            if value > room {
                continue;
            }
            let distance = (value - played).abs();
            if distance < best.2 {
                best = (base, dots, distance);
            }
        }
    }
    (best.0, best.1)
}

/// Draws on one page in page points.
struct Pen<'a, C: Canvas + ?Sized> {
    canvas: &'a C,
    sp: f32,
}

impl<C: Canvas + ?Sized> Clone for Pen<'_, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Canvas + ?Sized> Copy for Pen<'_, C> {}

impl<C: Canvas + ?Sized> Pen<'_, C> {
    fn line(&self, from: Point, to: Point, width: f32) {
        self.canvas.line(from, to, width, Ink::Black);
    }

    fn text(&self, x: f32, y: f32, anchor: Anchor, text: &str, size: f32, ink: Ink) {
        self.canvas.text((x, y), anchor, text, size, ink);
    }

    /// A tilted oval notehead.
    fn head(&self, x: f32, y: f32, hollow: bool) {
        let sp = self.sp;
        let (rx, ry) = (0.62 * sp, 0.44 * sp);
        let angle = -0.35_f32;
        let (sin, cos) = angle.sin_cos();
        let points: Vec<Point> = (0..20)
            .map(|step| {
                let t = step as f32 / 20.0 * std::f32::consts::TAU;
                let (px, py) = (rx * t.cos(), ry * t.sin());
                (x + px * cos - py * sin, y + px * sin + py * cos)
            })
            .collect();
        if hollow {
            self.canvas.polygon(&points, Ink::Paper, Some(0.17 * sp));
        } else {
            self.canvas.polygon(&points, Ink::Black, None);
        }
    }

    fn cross(&self, x: f32, y: f32) {
        let r = 0.45 * self.sp;
        self.line((x - r, y - r), (x + r, y + r), 0.16 * self.sp);
        self.line((x - r, y + r), (x + r, y - r), 0.16 * self.sp);
    }

    fn accidental(&self, x: f32, y: f32, alter: i8) {
        let sp = self.sp;
        match alter {
            1 => {
                self.line(
                    (x - 0.22 * sp, y - 1.2 * sp),
                    (x - 0.22 * sp, y + 1.3 * sp),
                    0.1 * sp,
                );
                self.line(
                    (x + 0.22 * sp, y - 1.3 * sp),
                    (x + 0.22 * sp, y + 1.2 * sp),
                    0.1 * sp,
                );
                self.line(
                    (x - 0.5 * sp, y - 0.25 * sp),
                    (x + 0.5 * sp, y - 0.55 * sp),
                    0.28 * sp,
                );
                self.line(
                    (x - 0.5 * sp, y + 0.55 * sp),
                    (x + 0.5 * sp, y + 0.25 * sp),
                    0.28 * sp,
                );
            }
            -1 => {
                self.line(
                    (x - 0.3 * sp, y - 1.8 * sp),
                    (x - 0.3 * sp, y + 0.5 * sp),
                    0.12 * sp,
                );
                let bowl: Vec<Point> = [
                    (-0.3, -0.15),
                    (0.1, -0.5),
                    (0.45, -0.45),
                    (0.45, -0.15),
                    (0.1, 0.2),
                    (-0.3, 0.5),
                ]
                .into_iter()
                .map(|(dx, dy)| (x + dx * sp, y + dy * sp))
                .collect();
                self.canvas.polyline(&bowl, 0.17 * sp);
            }
            0 => {
                self.line(
                    (x - 0.25 * sp, y - 1.3 * sp),
                    (x - 0.25 * sp, y + 0.5 * sp),
                    0.1 * sp,
                );
                self.line(
                    (x + 0.25 * sp, y - 0.5 * sp),
                    (x + 0.25 * sp, y + 1.3 * sp),
                    0.1 * sp,
                );
                self.line(
                    (x - 0.25 * sp, y - 0.3 * sp),
                    (x + 0.25 * sp, y - 0.5 * sp),
                    0.26 * sp,
                );
                self.line(
                    (x - 0.25 * sp, y + 0.5 * sp),
                    (x + 0.25 * sp, y + 0.3 * sp),
                    0.26 * sp,
                );
            }
            2 => {
                let r = 0.35 * sp;
                self.line((x - r, y - r), (x + r, y + r), 0.2 * sp);
                self.line((x - r, y + r), (x + r, y - r), 0.2 * sp);
            }
            -2 => {
                self.accidental(x - 0.4 * sp, y, -1);
                self.accidental(x + 0.4 * sp, y, -1);
            }
            _ => {}
        }
    }

    /// A curved arc for ties and slurs.
    fn arc(&self, from: f32, to: f32, y: f32, bulge: f32) {
        let points: Vec<Point> = (0..=12)
            .map(|step| {
                let t = step as f32 / 12.0;
                let lift = 4.0 * t * (1.0 - t) * bulge;
                (from + (to - from) * t, y + lift)
            })
            .collect();
        self.canvas.polyline(&points, 0.14 * self.sp);
    }
}

/// One chord (all notes of a track starting together).
struct Group<'a> {
    tick: Tick,
    /// The written value without its dots.
    duration: Tick,
    dots: u8,
    x: f32,
    notes: Vec<(&'a PlacedNote, i32)>,
}

impl Group<'_> {
    fn low(&self) -> i32 {
        self.notes.iter().map(|(_, p)| *p).max().unwrap_or(4)
    }

    fn high(&self) -> i32 {
        self.notes.iter().map(|(_, p)| *p).min().unwrap_or(4)
    }

    /// The written length, dots included.
    fn length(&self) -> Tick {
        if self.dots == 1 {
            self.duration * 3 / 2
        } else {
            self.duration
        }
    }

    fn stemmed(&self) -> bool {
        self.duration < 4 * PPQ
    }

    fn beamable(&self) -> bool {
        self.duration <= PPQ / 2
    }
}

/// The all-tracks score of a song, laid out and ready to draw page by page.
#[derive(Debug)]
pub struct FullScore<'a> {
    tracks: &'a [(String, Engraved)],
    layout: Layout,
    reference: Vec<&'a System>,
    widest: f32,
    key_fifths: i8,
    settings: Settings,
}

impl<'a> FullScore<'a> {
    /// Lays out the tracks (name and engraving each); the first track gives
    /// the bars.
    #[must_use]
    pub fn new(
        tracks: &'a [(String, Engraved)],
        frames: bool,
        markers: &[project::Marker],
        settings: Settings,
    ) -> Self {
        let layout = Layout::new(tracks, frames, markers, settings);
        let reference: Vec<&System> = tracks
            .first()
            .map(|(_, engraved)| reference_systems(engraved))
            .unwrap_or_default();
        let key_fifths = tracks
            .first()
            .map_or(0, |(_, engraved)| fifths(engraved.key));
        let widest = reference
            .iter()
            .map(|system| {
                system
                    .measures
                    .iter()
                    .map(|measure| (measure.x_end - measure.x).max(1.0))
                    .sum::<f32>()
            })
            .fold(1.0_f32, f32::max);
        Self {
            tracks,
            layout,
            reference,
            widest,
            key_fifths,
            settings,
        }
    }

    /// Number of pages.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.layout.page_count()
    }

    /// Systems on each page.
    #[must_use]
    pub fn systems_per_page(&self) -> usize {
        self.layout.systems_per_page
    }

    /// The page holding a tick, when a system contains it.
    #[must_use]
    pub fn page_for_tick(&self, tick: Tick) -> Option<usize> {
        self.reference
            .iter()
            .position(|system| {
                system.measures.iter().any(|measure| {
                    tick >= measure.start && tick < measure.start.saturating_add(measure.length)
                })
            })
            .map(|index| index / self.layout.systems_per_page)
    }

    fn slots(&self, page: usize) -> impl Iterator<Item = (usize, &'a System, f32, Columns)> + '_ {
        let first = page * self.layout.systems_per_page;
        (0..self.layout.systems_per_page).filter_map(move |slot| {
            let index = first + slot;
            let system = *self.reference.get(index)?;
            let columns = Columns::new(
                system,
                self.widest,
                index,
                index + 1 == self.reference.len(),
                self.key_fifths,
                self.layout.sp,
                self.settings,
            );
            Some((
                index,
                system,
                self.layout.system_top(slot, self.settings),
                columns,
            ))
        })
    }

    /// The tick under a page point, when it falls on a system.
    #[must_use]
    pub fn tick_at(&self, page: usize, x: f32, y: f32) -> Option<Tick> {
        let sp = self.layout.sp;
        let half_gap = SYSTEM_GAP * sp / 2.0;
        self.slots(page)
            .find(|(_, _, top, _)| {
                y >= top - half_gap && y <= top + self.layout.system_height * sp + half_gap
            })
            .and_then(|(_, system, _, columns)| columns.tick_at(system, x, sp))
    }

    /// Draws one page (not its paper), with the playhead at `position`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_page<C: Canvas + ?Sized>(
        &self,
        canvas: &C,
        page: usize,
        title: &str,
        bpm: f64,
        position: Option<Tick>,
        frames: bool,
        markers: &[project::Marker],
    ) {
        let settings = self.settings;
        let layout = &self.layout;
        let sp = layout.sp;
        let page_count = self.page_count();
        let page_index = page.min(page_count.saturating_sub(1));
        let pen = Pen { canvas, sp };

        // Title block.
        let centre = settings.page_width / 2.0;
        if page_index == 0 {
            pen.text(
                centre,
                settings.margin - 8.0,
                Anchor::CENTER_TOP,
                title,
                16.0,
                Ink::Black,
            );
            if let Some((_, engraved)) = self.tracks.first() {
                pen.text(
                    centre,
                    settings.margin + 13.0,
                    Anchor::CENTER_TOP,
                    &format!(
                        "{} {}",
                        engraved.key.tonic.name_sharp(),
                        if engraved.key.minor { "minor" } else { "major" }
                    ),
                    7.5,
                    Ink::Grey,
                );
            }
            // Tempo mark: a quarter note, then "= bpm".
            let x = settings.margin + 6.0;
            let y = settings.margin + HEADER - 22.0;
            let tempo = Pen { canvas, sp: 3.6 };
            tempo.head(x, y, false);
            tempo.line((x + 0.55 * 3.6, y), (x + 0.55 * 3.6, y - 12.0), 0.7);
            pen.text(
                x + 5.0,
                y,
                Anchor::LEFT_CENTER,
                &format!("= {bpm:.0}"),
                8.0,
                Ink::Black,
            );
        } else {
            pen.text(
                settings.margin,
                settings.margin - 8.0,
                Anchor::LEFT_TOP,
                title,
                8.0,
                Ink::Grey,
            );
        }

        for (index, system, system_top, columns) in self.slots(page_index) {
            draw_system(
                pen,
                layout,
                self.tracks,
                system,
                &columns,
                index,
                index + 1 == self.reference.len(),
                system_top,
                frames,
                markers,
            );
            if let Some(x) = position.and_then(|tick| columns.x_at(system, tick, sp)) {
                let first_top =
                    system_top + layout.staves.first().map_or(0.0, |staff| staff.top) * sp;
                let last_bottom =
                    system_top + layout.staves.last().map_or(4.0, |staff| staff.top + 4.0) * sp;
                canvas.line(
                    (x, first_top - sp),
                    (x, last_bottom + sp),
                    1.0,
                    Ink::Playhead,
                );
            }
        }
        pen.text(
            centre,
            settings.page_height - 8.0,
            Anchor::CENTER_BOTTOM,
            &format!("{} / {page_count}", page_index + 1),
            8.0,
            Ink::Grey,
        );
    }
}

/// Whether a track name reads as drums or percussion.
#[must_use]
pub fn is_percussion(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("drum") || lower.contains("kit") || lower.contains("perc")
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn draw_system<C: Canvas + ?Sized>(
    pen: Pen<'_, C>,
    layout: &Layout,
    tracks: &[(String, Engraved)],
    system: &System,
    columns: &Columns,
    index: usize,
    last: bool,
    system_top: f32,
    frames: bool,
    markers: &[project::Marker],
) {
    let sp = layout.sp;
    let staff_right = columns.end();
    let first_tick = system.measures.first().map_or(0, |measure| measure.start);
    let last_tick = system
        .measures
        .last()
        .map_or(0, |measure| measure.start.saturating_add(measure.length));
    let in_system = |tick: Tick| tick >= first_tick && tick < last_tick;
    let key_fifths = tracks
        .first()
        .map_or(0, |(_, engraved)| fifths(engraved.key));

    // The line joining the staves, and the measure number.
    if let (Some(first), Some(end)) = (layout.staves.first(), layout.staves.last()) {
        let top = system_top + first.top * sp;
        let bottom = system_top + (end.top + 4.0) * sp;
        pen.line(
            (columns.staff_left, top),
            (columns.staff_left, bottom),
            0.16 * sp,
        );
        if tracks.len() > 1 {
            pen.line(
                (columns.staff_left - 0.6 * sp, top - 0.4 * sp),
                (columns.staff_left - 0.6 * sp, bottom + 0.4 * sp),
                0.5 * sp,
            );
        }
        if let Some(measure) = system.measures.first() {
            pen.text(
                columns.staff_left,
                top - (first.notes_above - 0.6).max(1.0) * sp,
                Anchor::LEFT_BOTTOM,
                &measure.number.to_string(),
                1.5 * sp,
                Ink::Grey,
            );
        }
    }

    for (staff_index, ((name, engraved), staff)) in tracks.iter().zip(&layout.staves).enumerate() {
        let top = system_top + staff.top * sp;
        let bottom = top + 4.0 * sp;
        let y_of = |position: i32| top + position as f32 * sp / 2.0;

        // Staff lines, barlines, name.
        for line in 0..5 {
            let y = top + line as f32 * sp;
            pen.line((columns.staff_left, y), (staff_right, y), 0.1 * sp);
        }
        for (bar, x) in columns.bars.iter().enumerate().skip(1) {
            if last && bar + 1 == columns.bars.len() {
                pen.line((*x - 0.6 * sp, top), (*x - 0.6 * sp, bottom), 0.12 * sp);
                pen.line((*x - 0.2 * sp, top), (*x - 0.2 * sp, bottom), 0.45 * sp);
            } else {
                pen.line((*x, top), (*x, bottom), 0.12 * sp);
            }
        }
        let label: String = if index == 0 || name.chars().count() <= 10 {
            name.chars().take(16).collect()
        } else {
            let mut short: String = name.chars().take(6).collect();
            short.push('.');
            short
        };
        pen.text(
            columns.staff_left - 1.4 * sp,
            top + 2.0 * sp,
            Anchor::RIGHT_CENTER,
            &label,
            (1.6 * sp).clamp(5.0, 8.0),
            Ink::Black,
        );

        // Clef, key and time signature.
        let mut x = columns.staff_left + 0.8 * sp;
        match staff.clef {
            Clef::Treble | Clef::Bass => {
                let treble = staff.clef == Clef::Treble;
                let (image, height, offset) = if treble {
                    (crate::clef::treble(), 7.0, -1.5)
                } else {
                    (crate::clef::bass(), 3.4, 0.0)
                };
                let height = height * sp;
                let width = height * image.width as f32 / image.height as f32;
                pen.canvas.clef(
                    treble,
                    (x, top + offset * sp),
                    (x + width, top + offset * sp + height),
                );
            }
            Clef::Percussion => {
                for dx in [0.6, 1.4] {
                    pen.line(
                        (x + dx * sp, top + sp),
                        (x + dx * sp, top + 3.0 * sp),
                        0.45 * sp,
                    );
                }
            }
        }
        x += 4.4 * sp;
        if staff.clef != Clef::Percussion {
            let shift = if staff.clef == Clef::Bass { 14 } else { 0 };
            let (alter, letters): (i8, [i32; 7]) = if key_fifths >= 0 {
                (1, [38, 35, 39, 36, 33, 37, 34])
            } else {
                (-1, [34, 37, 33, 36, 32, 35, 31])
            };
            for (step, letter) in letters
                .iter()
                .take(usize::from(key_fifths.unsigned_abs()))
                .enumerate()
            {
                let position = staff.clef.top_line() - (letter - shift);
                pen.accidental(x + (step as f32 + 0.5) * sp, y_of(position), alter);
            }
        }
        x += f32::from(key_fifths.unsigned_abs()) * sp;
        if index == 0 {
            let numerator = engraved.time_signature.numerator.to_string();
            let denominator = engraved.time_signature.denominator.to_string();
            for (text, y) in [(numerator, top + sp), (denominator, top + 3.0 * sp)] {
                pen.text(
                    x + 1.3 * sp,
                    y,
                    Anchor::CENTER_CENTER,
                    &text,
                    2.6 * sp,
                    Ink::Black,
                );
            }
        }

        // Rows above the first staff: markers, chord frames, chord names.
        if staff_index == 0 {
            let chord_y = top - (staff.notes_above + 1.0) * sp;
            for chord in engraved
                .chord_symbols
                .iter()
                .filter(|chord| in_system(chord.tick))
            {
                if let Some(x) = columns.x_at(system, chord.tick, sp) {
                    pen.text(
                        x - 0.6 * sp,
                        chord_y,
                        Anchor::LEFT_CENTER,
                        &chord.name,
                        (2.0 * sp).clamp(6.0, 9.0),
                        Ink::Black,
                    );
                }
            }
            if frames {
                let frame_bottom = top - (staff.notes_above + staff.chord_row + 0.5) * sp;
                for frame in engraved.frames.iter().filter(|frame| in_system(frame.tick)) {
                    let Some(x) = columns.x_at(system, frame.tick, sp) else {
                        continue;
                    };
                    let (width, height) = (3.0 * sp, 3.6 * sp);
                    pen.canvas.stroke_rect(
                        (x - 0.6 * sp, frame_bottom - height),
                        (x - 0.6 * sp + width, frame_bottom),
                        0.08 * sp,
                    );
                    for (string, fret) in frame.frets.iter().enumerate() {
                        if let Some(fret) = fret {
                            pen.text(
                                x - 0.6 * sp + (string as f32 + 0.5) * width / 6.0,
                                frame_bottom - height - 0.2 * sp,
                                Anchor::CENTER_BOTTOM,
                                &fret.to_string(),
                                sp,
                                Ink::Black,
                            );
                        }
                    }
                }
            }
            let marker_y = top - (staff.notes_above + staff.chord_row + staff.frame_row + 1.1) * sp;
            for marker in markers.iter().filter(|marker| in_system(marker.tick)) {
                if let Some(x) = columns.x_at(system, marker.tick, sp) {
                    let x = x - BAR_LEAD * sp;
                    let size = (1.8 * sp).clamp(6.0, 8.5);
                    let width = pen.canvas.text_width(&marker.name, size);
                    pen.canvas.stroke_rect(
                        (x - 2.0, marker_y - size * 0.6 - 2.0),
                        (x + width + 2.0, marker_y + size * 0.6 + 2.0),
                        0.1 * sp,
                    );
                    pen.text(
                        x,
                        marker_y,
                        Anchor::LEFT_CENTER,
                        &marker.name,
                        (1.8 * sp).clamp(6.0, 8.5),
                        Ink::Black,
                    );
                }
            }
        }

        // Rows below: dynamics, then (first staff) lyrics.
        let mark_y = bottom + (staff.notes_below + 1.0) * sp;
        for mark in engraved.marks.iter().filter(|mark| in_system(mark.tick)) {
            let Some(x) = columns.x_at(system, mark.tick, sp) else {
                continue;
            };
            let text = match mark.kind {
                project::song::MarkKind::Dynamic(dynamic) => dynamic.text().to_string(),
                project::song::MarkKind::Articulation(articulation) => {
                    format!("{articulation:?}").to_lowercase()
                }
            };
            pen.text(
                x - 0.6 * sp,
                mark_y,
                Anchor::LEFT_CENTER,
                &text,
                (1.7 * sp).clamp(5.0, 8.0),
                Ink::Black,
            );
        }
        if staff_index == 0 {
            let lyric_y = bottom + (staff.notes_below + staff.mark_row + 1.0) * sp;
            for lyric in engraved.lyrics.iter().filter(|lyric| in_system(lyric.tick)) {
                if let Some(x) = columns.x_at(system, lyric.tick, sp) {
                    pen.text(
                        x,
                        lyric_y,
                        Anchor::CENTER_CENTER,
                        &lyric.text,
                        (1.6 * sp).clamp(5.0, 8.0),
                        Ink::Black,
                    );
                }
            }
        }

        // Notes, grouped into chords.
        let mut chords: BTreeMap<Tick, Vec<&PlacedNote>> = BTreeMap::new();
        for note in engraved.notes.iter().filter(|note| in_system(note.tick)) {
            chords.entry(note.tick).or_default().push(note);
        }
        let chords: Vec<(Tick, Vec<&PlacedNote>)> = chords.into_iter().collect();
        let groups: Vec<Group<'_>> = chords
            .iter()
            .enumerate()
            .filter_map(|(index, (tick, notes))| {
                let x = columns.x_at(system, *tick, sp)?;
                let bar_end = system
                    .measures
                    .iter()
                    .find(|measure| {
                        *tick >= measure.start && *tick < measure.start + measure.length
                    })
                    .map_or(*tick + PPQ, |measure| measure.start + measure.length);
                let next = chords
                    .get(index + 1)
                    .map_or(bar_end, |(next, _)| (*next).min(bar_end));
                let mut played = notes.iter().map(|note| note.duration).min().unwrap_or(PPQ);
                // Drum hits and notes shorter than a sixteenth read as
                // lasting to the next note, within the beat.
                if staff.clef == Clef::Percussion || played < PPQ / 4 {
                    let beat_end = (tick.div_euclid(PPQ) + 1) * PPQ;
                    played = (next.min(beat_end) - tick).max(PPQ / 4);
                }
                let (duration, dots) = written(played.min(bar_end - tick), next - tick);
                Some(Group {
                    tick: *tick,
                    duration,
                    dots,
                    x,
                    notes: notes
                        .iter()
                        .map(|note| (*note, note_position(staff.clef, note)))
                        .collect(),
                })
            })
            .collect();
        // Slurs, as arcs over the staff between their first and last notes.
        let tick_near = |page: usize, x: f32| {
            engraved
                .notes
                .iter()
                .filter(|note| note.page == page)
                .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
                .map(|note| note.tick)
        };
        for slur in &engraved.slurs {
            let (Some(start), Some(end)) = (
                tick_near(slur.page, slur.start_x),
                tick_near(slur.page, slur.end_x),
            ) else {
                continue;
            };
            let Some(from) = columns.x_at(system, start, sp) else {
                continue;
            };
            let to = columns
                .x_at(system, end, sp)
                .unwrap_or(staff_right)
                .max(from + 2.0 * sp);
            pen.arc(from, to, top - 0.8 * sp, -1.2 * sp);
        }
        // Rests fill the gaps between the written notes.
        for (bar, measure) in system.measures.iter().enumerate() {
            let end = measure.start + measure.length;
            let in_bar: Vec<&Group<'_>> = groups
                .iter()
                .filter(|group| group.tick >= measure.start && group.tick < end)
                .collect();
            if in_bar.is_empty() {
                draw_rest(pen, columns.bar_middle(bar), top, 4 * PPQ);
                continue;
            }
            let mut cursor = measure.start;
            for group in in_bar
                .iter()
                .map(|group| (group.tick, group.length()))
                .chain(std::iter::once((end, 0)))
            {
                for (tick, value) in rest_values(cursor - measure.start, group.0 - cursor) {
                    if let Some(x) = columns.x_at(system, measure.start + tick, sp) {
                        draw_rest(pen, x, top, value);
                    }
                }
                cursor = cursor.max(group.0 + group.1);
            }
        }
        let percussion = staff.clef == Clef::Percussion;

        // Beam runs: short notes in the same beat.
        let mut runs: Vec<Vec<usize>> = Vec::new();
        for (position, group) in groups.iter().enumerate() {
            let joins = runs
                .last()
                .and_then(|run| run.last())
                .is_some_and(|previous| {
                    let previous = &groups[*previous];
                    previous.beamable()
                        && group.beamable()
                        && previous.tick.div_euclid(PPQ) == group.tick.div_euclid(PPQ)
                });
            if joins {
                if let Some(run) = runs.last_mut() {
                    run.push(position);
                }
            } else {
                runs.push(vec![position]);
            }
        }
        let up_for = |run: &[usize]| -> bool {
            if percussion {
                return true;
            }
            let total: i32 = run
                .iter()
                .map(|index| groups[*index].low() + groups[*index].high())
                .sum();
            total as f32 / (2.0 * run.len() as f32) > 4.0
        };
        let head_width = 1.2 * sp;
        for run in &runs {
            let up = up_for(run);
            let stem_x = |group: &Group<'_>| {
                if up {
                    group.x + head_width / 2.0 - 0.06 * sp
                } else {
                    group.x - head_width / 2.0 + 0.06 * sp
                }
            };
            let stem_end = |group: &Group<'_>| {
                if up {
                    y_of(group.high()) - 3.5 * sp
                } else {
                    y_of(group.low()) + 3.5 * sp
                }
            };
            // Stems and beams.
            if run.len() >= 2 {
                let first = &groups[run[0]];
                let last_group = &groups[run[run.len() - 1]];
                let (x0, x1) = (stem_x(first), stem_x(last_group));
                let slope = if x1 > x0 {
                    ((stem_end(last_group) - stem_end(first)) / (x1 - x0)).clamp(-0.15, 0.15)
                } else {
                    0.0
                };
                let mut base = stem_end(first);
                for index in run {
                    let group = &groups[*index];
                    let beam_y = base + slope * (stem_x(group) - x0);
                    let limit = if up {
                        y_of(group.high()) - 2.8 * sp
                    } else {
                        y_of(group.low()) + 2.8 * sp
                    };
                    if up && beam_y > limit {
                        base -= beam_y - limit;
                    } else if !up && beam_y < limit {
                        base += limit - beam_y;
                    }
                }
                let beam_at = |x: f32| base + slope * (x - x0);
                for index in run {
                    let group = &groups[*index];
                    let from = if up {
                        y_of(group.low())
                    } else {
                        y_of(group.high())
                    };
                    pen.line(
                        (stem_x(group), from),
                        (stem_x(group), beam_at(stem_x(group))),
                        0.12 * sp,
                    );
                }
                let thickness = 0.5 * sp;
                let toward = if up { 1.0 } else { -1.0 };
                let beam = |from: f32, to: f32, offset: f32| {
                    let (a, b) = (beam_at(from) + offset, beam_at(to) + offset);
                    pen.canvas.polygon(
                        &[
                            (from, a),
                            (to, b),
                            (to, b + thickness * toward),
                            (from, a + thickness * toward),
                        ],
                        Ink::Black,
                        None,
                    );
                };
                beam(x0, x1, 0.0);
                for pair in run.windows(2) {
                    let (a, b) = (&groups[pair[0]], &groups[pair[1]]);
                    if a.duration <= PPQ / 4 && b.duration <= PPQ / 4 {
                        beam(stem_x(a), stem_x(b), 0.75 * sp * toward);
                    }
                }
            } else {
                let group = &groups[run[0]];
                if group.stemmed() {
                    let from = if up {
                        y_of(group.low())
                    } else {
                        y_of(group.high())
                    };
                    let end = stem_end(group);
                    pen.line((stem_x(group), from), (stem_x(group), end), 0.12 * sp);
                    let flags = if group.duration <= PPQ / 4 {
                        2
                    } else {
                        usize::from(group.duration <= PPQ / 2)
                    };
                    for flag in 0..flags {
                        let direction = if up { 1.0 } else { -1.0 };
                        let start = end + flag as f32 * 0.8 * sp * direction;
                        let x = stem_x(group);
                        let points: Vec<Point> = [(0.0, 0.0), (0.5, 0.9), (1.0, 1.7), (0.8, 2.6)]
                            .into_iter()
                            .map(|(dx, dy)| (x + dx * sp, start + dy * sp * direction))
                            .collect();
                        pen.canvas.polyline(&points, 0.16 * sp);
                    }
                }
            }
            // Heads, ledger lines, accidentals, dots.
            for index in run {
                let group = &groups[*index];
                let mut notes = group.notes.clone();
                notes.sort_by_key(|(_, position)| std::cmp::Reverse(*position));
                let mut previous: Option<(i32, bool)> = None;
                let mut accidentals = 0;
                for (note, position) in notes {
                    let shifted =
                        previous.is_some_and(|(last, moved)| last - position == 1 && !moved);
                    previous = Some((position, shifted));
                    let x = if shifted {
                        group.x + if up { head_width } else { -head_width } * 0.95
                    } else {
                        group.x
                    };
                    let y = y_of(position);
                    let ledger = |line: i32| {
                        pen.line(
                            (x - 0.95 * sp, y_of(line)),
                            (x + 0.95 * sp, y_of(line)),
                            0.12 * sp,
                        );
                    };
                    let mut line = -2;
                    while line >= position {
                        ledger(line);
                        line -= 2;
                    }
                    let mut line = 10;
                    while line <= position {
                        ledger(line);
                        line += 2;
                    }
                    if percussion && cross_head(note.pitch.as_u8()) {
                        pen.cross(x, y);
                    } else {
                        pen.head(x, y, group.duration >= 2 * PPQ);
                    }
                    if !percussion && let Some(alter) = note.spelled.accidental {
                        let offset = 1.3 + accidentals as f32 % 3.0 * 1.0;
                        pen.accidental(group.x - offset * sp, y, alter);
                        accidentals += 1;
                    }
                    let dot_y = if position % 2 == 0 { y - 0.5 * sp } else { y };
                    for dot in 0..group.dots {
                        pen.canvas
                            .circle((x + (1.0 + f32::from(dot) * 0.5) * sp, dot_y), 0.2 * sp);
                    }
                    // A tie over the barline to the same pitch. Repeated
                    // notes inside a bar stay separate notes.
                    if note.tie_start
                        && let Some(next) = engraved
                            .notes
                            .iter()
                            .filter(|next| next.pitch == note.pitch && next.tick > note.tick)
                            .min_by_key(|next| next.tick)
                        && next.tied
                        && next.measure != note.measure
                    {
                        let end = columns
                            .x_at(system, next.tick, sp)
                            .unwrap_or(staff_right + 0.5 * sp);
                        let (bend, y) = if up {
                            (0.8 * sp, y + 0.6 * sp)
                        } else {
                            (-0.8 * sp, y - 0.6 * sp)
                        };
                        pen.arc(x + 0.7 * sp, end - 0.7 * sp, y, bend);
                    }
                }
            }
        }
    }
}

/// Rests for a gap of `length` ticks starting `offset` ticks into a bar:
/// each the longest value that fits and starts on a multiple of itself.
fn rest_values(offset: Tick, length: Tick) -> Vec<(Tick, Tick)> {
    let mut rests = Vec::new();
    let (mut at, end) = (offset, offset + length.max(0));
    while end - at >= PPQ / 4 {
        let value = [2 * PPQ, PPQ, PPQ / 2, PPQ / 4]
            .into_iter()
            .find(|value| at % value == 0 && at + value <= end)
            .unwrap_or(PPQ / 4);
        rests.push((at, value));
        at += value;
    }
    rests
}

/// Draws a rest of a written value, centred on `x`.
fn draw_rest<C: Canvas + ?Sized>(pen: Pen<'_, C>, x: f32, top: f32, value: Tick) {
    let sp = pen.sp;
    if value >= 4 * PPQ {
        pen.canvas
            .fill_rect((x - 0.6 * sp, top + sp), (x + 0.6 * sp, top + 1.5 * sp));
    } else if value >= 2 * PPQ {
        pen.canvas.fill_rect(
            (x - 0.6 * sp, top + 1.5 * sp),
            (x + 0.6 * sp, top + 2.0 * sp),
        );
    } else if value >= PPQ {
        let points: Vec<Point> = [
            (-0.1, 0.6),
            (0.4, 1.3),
            (-0.15, 2.0),
            (0.35, 2.7),
            (-0.25, 2.8),
            (0.1, 3.4),
        ]
        .into_iter()
        .map(|(dx, dy)| (x + dx * sp, top + dy * sp))
        .collect();
        pen.canvas.polyline(&points, 0.28 * sp);
    } else {
        // Eighth: one hook; sixteenth: two.
        let hooks = if value >= PPQ / 2 { 1 } else { 2 };
        let foot = (x - 0.15 * sp, top + 3.0 * sp + (hooks - 1) as f32 * sp);
        let head = (x + 0.45 * sp, top + 1.45 * sp);
        pen.line(head, foot, 0.12 * sp);
        for hook in 0..hooks {
            let lift = hook as f32 * sp;
            let (hx, hy) = (head.0 - lift * 0.3, head.1 + lift);
            pen.canvas.circle((hx - 0.6 * sp, hy + 0.1 * sp), 0.22 * sp);
            pen.line((hx - 0.6 * sp, hy + 0.25 * sp), (hx, hy), 0.12 * sp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, Pitch, Velocity};
    use project::song::{Note, Song, Track};
    use project::{NoteId, TrackId};

    fn song_with(names: &[&str]) -> Song {
        let mut song = Song::new("Full", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        for (index, name) in names.iter().enumerate() {
            track.notes.push(Note {
                id: NoteId::new(index as u32 + 1).expect("valid"),
                start: index as Tick * PPQ,
                duration: PPQ,
                pitch: Pitch::from_name(name).expect("valid"),
                velocity: Velocity::new(100).expect("valid"),
                channel: Channel::first(),
            });
        }
        song.tracks.push(track);
        song
    }

    fn engraved(song: &Song) -> Engraved {
        crate::engrave(song, TrackId::new(1).expect("valid"), Settings::default())
    }

    #[test]
    fn notes_sit_by_letter_name() {
        let song = song_with(&["E4", "F4", "B4", "F5", "C4", "A3"]);
        let engraved = engraved(&song);
        let positions: Vec<i32> = engraved
            .notes
            .iter()
            .map(|note| note_position(Clef::Treble, note))
            .collect();
        // Bottom line, first space, middle line, top line, one ledger below.
        assert_eq!(positions[..5], [8, 7, 4, 0, 10]);
        assert_eq!(
            note_position(Clef::Bass, &engraved.notes[5]),
            0,
            "A3 on the bass top line"
        );
    }

    #[test]
    fn low_tracks_get_a_bass_clef() {
        let low = engraved(&song_with(&["E2", "A2", "C3", "G2"]));
        assert_eq!(choose_clef("Bass", &low), Clef::Bass);
        let high = engraved(&song_with(&["E4", "A4", "C5", "G4"]));
        assert_eq!(choose_clef("Lead", &high), Clef::Treble);
        assert_eq!(choose_clef("Drums", &high), Clef::Percussion);
    }

    #[test]
    fn staves_never_overlap_and_fill_the_page() {
        let song = song_with(&["C2", "C7", "E4", "G4"]);
        let one = engraved(&song);
        let tracks: Vec<(String, Engraved)> = (0..4)
            .map(|index| (format!("Track {index}"), one.clone()))
            .collect();
        let layout = Layout::new(&tracks, false, &[], Settings::default());
        for pair in layout.staves.windows(2) {
            assert!(
                pair[1].top - pair[1].above() >= pair[0].top + 4.0 + pair[0].below() - 0.001,
                "staves keep their room"
            );
        }
        let settings = Settings::default();
        let bottom = layout.system_top(layout.systems_per_page - 1, settings)
            + layout.system_height * layout.sp;
        assert!(
            bottom <= settings.page_height - settings.margin,
            "fits the page"
        );
    }

    #[test]
    fn a_tick_maps_to_x_and_back() {
        let song = song_with(&["C4", "D4", "E4", "F4"]);
        let engraved = engraved(&song);
        let systems = reference_systems(&engraved);
        let columns = Columns::new(systems[0], 100.0, 0, true, 0, 5.0, Settings::default());
        let x = columns.x_at(systems[0], 2 * PPQ, 5.0).expect("x");
        assert_eq!(columns.tick_at(systems[0], x, 5.0), Some(2 * PPQ));
    }

    #[test]
    fn gaps_fill_with_beat_aligned_rests() {
        assert_eq!(rest_values(0, PPQ), vec![(0, PPQ)]);
        assert_eq!(
            rest_values(PPQ / 2, PPQ * 3 / 2),
            vec![(PPQ / 2, PPQ / 2), (PPQ, PPQ)]
        );
        assert_eq!(
            rest_values(0, 4 * PPQ),
            vec![(0, 2 * PPQ), (2 * PPQ, 2 * PPQ)]
        );
        assert!(rest_values(0, PPQ / 8).is_empty(), "too short to write");
    }

    #[test]
    fn played_lengths_round_to_written_values() {
        assert_eq!(written(PPQ - 40, PPQ), (PPQ, 0), "a short quarter");
        assert_eq!(
            written(PPQ * 3 / 8, PPQ / 4),
            (PPQ / 4, 0),
            "cut at the next note"
        );
        assert_eq!(written(PPQ * 3 / 2, 2 * PPQ), (PPQ, 1), "a dotted quarter");
        assert_eq!(written(4 * PPQ - 30, 4 * PPQ), (4 * PPQ, 0), "a whole note");
    }
}
