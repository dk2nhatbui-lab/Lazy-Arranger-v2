//! Style sections and chord-following compilation.
//!
//! A style holds named sections (the C++ application calls them Main A, Fill
//! In AA, and so on). Each section holds parts; a part is a set of notes plus
//! the [`ChordRule`] that says how those notes follow the chord.
//!
//! Compilation splits a section at chord changes, gates each part with
//! [`crate::plays`], converts every note with [`crate::transpose::transpose`], and cuts a
//! note that would cross a chord change or the section end. Cutting matches the
//! oracle, which re-renders a clip on a chord change and stops the notes that
//! were sounding (`docs/audit/notes/cpp-style-arranger.md` §5).
//!
//! This module has no parser. Yamaha container scanning is in `style-formats`;
//! turning CASM bytes into these rules is the next Stage 4 step.

use music_core::{Channel, Tick};
use project::Song;
use thiserror::Error;

use crate::chord_type::ArrangerChord;
use crate::rules::ChordRule;
use crate::transpose::{plays, transpose};

/// A note inside a part, in ticks relative to the section start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleNote {
    /// Offset from the section start.
    pub tick: Tick,
    /// Length in ticks; always greater than zero.
    pub duration: Tick,
    /// Source pitch, before chord conversion.
    pub pitch: u8,
    /// Velocity.
    pub velocity: u8,
}

/// One part of a section: its notes and its chord rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StylePart {
    /// Display name, for example `Chord 1`.
    pub name: String,
    /// Chord rules for this part's source channel.
    pub rule: ChordRule,
    /// Notes in source form.
    pub notes: Vec<StyleNote>,
}

impl StylePart {
    /// Creates a part with a name and rule.
    #[must_use]
    pub fn new(name: impl Into<String>, rule: ChordRule) -> Self {
        Self {
            name: name.into(),
            rule,
            notes: Vec::new(),
        }
    }
}

/// A named section of a style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleSection {
    /// Display name, for example `Main A`.
    pub name: String,
    /// Length in ticks.
    pub length_ticks: Tick,
    /// Parts in source-channel order.
    pub parts: Vec<StylePart>,
}

impl StyleSection {
    /// Creates an empty section of a given length.
    #[must_use]
    pub fn new(name: impl Into<String>, length_ticks: Tick) -> Self {
        Self {
            name: name.into(),
            length_ticks,
            parts: Vec::new(),
        }
    }
}

/// A native style: sections, a source chord and no manufacturer bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Style {
    /// Display name.
    pub name: String,
    /// The chord the style's notes were written in.
    pub source_chord: ArrangerChord,
    /// Sections in source order.
    pub sections: Vec<StyleSection>,
}

impl Style {
    /// Creates an empty style.
    #[must_use]
    pub fn new(name: impl Into<String>, source_chord: ArrangerChord) -> Self {
        Self {
            name: name.into(),
            source_chord,
            sections: Vec::new(),
        }
    }

    /// Finds a section by exact name.
    #[must_use]
    pub fn section(&self, name: &str) -> Option<&StyleSection> {
        self.sections.iter().find(|section| section.name == name)
    }
}

/// Builds a one-section style from a song's current tracks.
///
/// Notes are clipped to `length_ticks` and stored relative to zero. Each track
/// becomes one part with the oracle's default rule for its channel; a drum track
/// keeps its recorded pitches.
#[must_use]
pub fn from_song(
    song: &Song,
    name: &str,
    source_chord: ArrangerChord,
    length_ticks: Tick,
) -> Style {
    let length = length_ticks.max(1);
    let mut section = StyleSection::new("Main A", length);
    for track in &song.tracks {
        if track.notes.is_empty() {
            continue;
        }
        let channel = track.notes[0].channel.one_based();
        let mut part = StylePart::new(
            if track.name.is_empty() {
                format!("Part {channel}")
            } else {
                track.name.clone()
            },
            ChordRule::default_for_channel(channel),
        );
        for note in &track.notes {
            if note.start >= length {
                continue;
            }
            let duration = note.duration.max(1).min(length.saturating_sub(note.start));
            if duration == 0 {
                continue;
            }
            part.notes.push(StyleNote {
                tick: note.start,
                duration,
                pitch: note.pitch.as_u8(),
                velocity: note.velocity.as_u8(),
            });
        }
        if !part.notes.is_empty() {
            section.parts.push(part);
        }
    }
    let mut style = Style::new(name, source_chord);
    style.sections.push(section);
    style
}

/// A chord change on the song timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChordAt {
    /// Position in ticks.
    pub tick: Tick,
    /// The chord in force from this tick.
    pub chord: ArrangerChord,
}

/// One converted note on the song timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompiledNote {
    /// Absolute start tick.
    pub tick: Tick,
    /// Length in ticks, already cut at a chord change or section end.
    pub duration: Tick,
    /// Pitch after chord conversion.
    pub pitch: u8,
    /// Velocity, unchanged.
    pub velocity: u8,
    /// Destination channel from the part rule.
    pub channel: Channel,
}

/// Errors raised while compiling a section.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StyleError {
    /// The requested section does not exist.
    #[error("style has no section named {0:?}")]
    UnknownSection(String),
    /// A part rule named an invalid destination channel.
    #[error("part rule for channel {0} has an invalid destination channel")]
    Channel(u8),
    /// A section had a non-positive length.
    #[error("section {name:?} has length {length}, which cannot be played")]
    SectionLength {
        /// Section name.
        name: String,
        /// The offending length.
        length: Tick,
    },
    /// A style note had a non-positive length.
    #[error("note at {tick} in part {part:?} has length {duration}")]
    NoteLength {
        /// Part name.
        part: String,
        /// Note offset.
        tick: Tick,
        /// The offending length.
        duration: Tick,
    },
}

impl Style {
    /// Compiles one section onto the song timeline.
    ///
    /// Chord changes split the section. A part whose rule rejects a chord is
    /// silent for that segment. A note that would cross a segment boundary is
    /// cut there.
    ///
    /// # Errors
    ///
    /// Returns [`StyleError`] for an unknown section, an invalid channel, a
    /// non-positive section length or a non-positive note length.
    pub fn compile(
        &self,
        section_name: &str,
        start_tick: Tick,
        chords: &[ChordAt],
    ) -> Result<Vec<CompiledNote>, StyleError> {
        let section = self
            .section(section_name)
            .ok_or_else(|| StyleError::UnknownSection(String::from(section_name)))?;
        if section.length_ticks <= 0 {
            return Err(StyleError::SectionLength {
                name: section.name.clone(),
                length: section.length_ticks,
            });
        }
        let end_tick = start_tick + section.length_ticks;
        let segments = segments_for(start_tick, end_tick, &self.source_chord, chords);

        let mut out = Vec::new();
        for segment in segments {
            for part in &section.parts {
                let channel = Channel::from_one_based(part.rule.dest_channel)
                    .map_err(|_| StyleError::Channel(part.rule.dest_channel))?;
                if !plays(&part.rule, &segment.chord) {
                    continue;
                }
                for note in &part.notes {
                    if note.duration <= 0 {
                        return Err(StyleError::NoteLength {
                            part: part.name.clone(),
                            tick: note.tick,
                            duration: note.duration,
                        });
                    }
                    let note_start = start_tick + note.tick;
                    if note_start < segment.start || note_start >= segment.end {
                        continue;
                    }
                    let Some(pitch) = transpose(note.pitch, &part.rule, &segment.chord) else {
                        continue;
                    };
                    let cut = segment.end.min(note_start + note.duration);
                    out.push(CompiledNote {
                        tick: note_start,
                        duration: cut - note_start,
                        pitch,
                        velocity: note.velocity,
                        channel,
                    });
                }
            }
        }
        out.sort_by_key(|note| (note.tick, note.channel.zero_based(), note.pitch));
        Ok(out)
    }
}

/// One stretch of the section where a single chord is in force.
#[derive(Clone, Copy, Debug)]
struct Segment {
    start: Tick,
    end: Tick,
    chord: ArrangerChord,
}

fn segments_for(
    start: Tick,
    end: Tick,
    source_chord: &ArrangerChord,
    chords: &[ChordAt],
) -> Vec<Segment> {
    let mut points: Vec<(Tick, ArrangerChord)> = chords
        .iter()
        .filter(|event| event.tick > start && event.tick < end)
        .map(|event| (event.tick, event.chord))
        .collect();
    points.sort_by_key(|(tick, _)| *tick);

    let first = chords
        .iter()
        .rev()
        .find(|event| event.tick <= start)
        .map_or(*source_chord, |event| event.chord);

    let mut segments = Vec::with_capacity(points.len() + 1);
    let mut segment_start = start;
    let mut chord = first;
    for (tick, next) in points {
        segments.push(Segment {
            start: segment_start,
            end: tick,
            chord,
        });
        segment_start = tick;
        chord = next;
    }
    segments.push(Segment {
        start: segment_start,
        end,
        chord,
    });
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord_type::ChordType;
    use crate::rules::{Ntr, Ntt};
    use music_core::PitchClass;

    fn chord(root: i32, chord_type: ChordType) -> ArrangerChord {
        ArrangerChord::new(PitchClass::new(root), chord_type)
    }

    fn note(tick: Tick, duration: Tick, pitch: u8) -> StyleNote {
        StyleNote {
            tick,
            duration,
            pitch,
            velocity: 100,
        }
    }

    /// A style with one section: a rhythm part that ignores the chord and a
    /// bass part that follows it.
    fn style() -> Style {
        let mut style = Style::new("Fixture", chord(0, ChordType::Major7));
        let mut section = StyleSection::new("Main A", 3840);

        let mut drums_rule = ChordRule::new(10, 10);
        drums_rule.transpose.ntr = Ntr::RootFixed;
        drums_rule.transpose.ntt = Ntt::Bypass;
        let mut drums = StylePart::new("Drums", drums_rule);
        drums.notes.push(note(0, 240, 38));
        drums.notes.push(note(1920, 240, 38));

        let mut bass_rule = ChordRule::new(11, 11);
        bass_rule.source_chord_type = ChordType::Major7;
        bass_rule.transpose.ntt = Ntt::Melody;
        bass_rule.transpose.high_key = 6;
        bass_rule.transpose.low_limit = 0;
        bass_rule.transpose.high_limit = 127;
        let mut bass = StylePart::new("Bass", bass_rule);
        bass.notes.push(note(0, 960, 36));
        bass.notes.push(note(1920, 960, 43));

        section.parts.push(drums);
        section.parts.push(bass);
        style.sections.push(section);
        style
    }

    #[test]
    fn source_chord_is_the_identity() {
        let compiled = style().compile("Main A", 0, &[]).expect("compiles");
        assert_eq!(compiled.len(), 4);
        let bass: Vec<u8> = compiled
            .iter()
            .filter(|n| n.channel.one_based() == 11)
            .map(|n| n.pitch)
            .collect();
        assert_eq!(bass, vec![36, 43], "C and G stay put in C major");
    }

    #[test]
    fn chord_changes_split_the_section() {
        let compiled = style()
            .compile(
                "Main A",
                0,
                &[
                    ChordAt {
                        tick: 0,
                        chord: chord(0, ChordType::Major),
                    },
                    ChordAt {
                        tick: 1920,
                        chord: chord(5, ChordType::Major),
                    },
                ],
            )
            .expect("compiles");
        let bass: Vec<(Tick, u8, Tick)> = compiled
            .iter()
            .filter(|n| n.channel.one_based() == 11)
            .map(|n| (n.tick, n.pitch, n.duration))
            .collect();
        // C at 0 stays C; the G at 1920 becomes the octave C at 48, and the
        // note is cut at the chord change is not needed because it starts on it.
        assert_eq!(bass[0], (0, 36, 960));
        assert_eq!(bass[1], (1920, 48, 960));
    }

    #[test]
    fn notes_are_cut_at_a_chord_change() {
        let mut fixture = style();
        // One long bass note across the whole bar.
        fixture.sections[0].parts[1].notes = vec![note(0, 3840, 36)];
        let compiled = fixture
            .compile(
                "Main A",
                0,
                &[
                    ChordAt {
                        tick: 0,
                        chord: chord(0, ChordType::Major),
                    },
                    ChordAt {
                        tick: 1920,
                        chord: chord(5, ChordType::Major),
                    },
                ],
            )
            .expect("compiles");
        let bass: Vec<&CompiledNote> = compiled
            .iter()
            .filter(|n| n.channel.one_based() == 11)
            .collect();
        assert_eq!(bass.len(), 1, "the note starts once");
        assert_eq!(bass[0].duration, 1920, "cut at the chord change");
    }

    #[test]
    fn no_chord_stops_every_part_but_rhythm() {
        let compiled = style()
            .compile(
                "Main A",
                0,
                &[ChordAt {
                    tick: 0,
                    chord: ArrangerChord::cancel(),
                }],
            )
            .expect("compiles");
        assert_eq!(compiled.len(), 2, "only the bypassed drum part survives");
        assert!(compiled.iter().all(|n| n.channel.one_based() == 10));
    }

    #[test]
    fn mute_masks_silence_a_part_for_that_chord() {
        let mut fixture = style();
        fixture.sections[0].parts[1].rule.note_mute = 0x0fff & !(1 << 5);
        let compiled = fixture
            .compile(
                "Main A",
                0,
                &[ChordAt {
                    tick: 0,
                    chord: chord(5, ChordType::Major),
                }],
            )
            .expect("compiles");
        assert!(
            compiled.iter().all(|n| n.channel.one_based() != 11),
            "the bass part is muted on F"
        );
        assert!(compiled.iter().any(|n| n.channel.one_based() == 10));
    }

    #[test]
    fn section_start_offsets_every_note() {
        let compiled = style().compile("Main A", 7680, &[]).expect("compiles");
        assert!(compiled.iter().all(|note| note.tick >= 7680));
        assert_eq!(compiled[0].tick, 7680);
    }

    #[test]
    fn notes_are_cut_at_the_section_end() {
        let mut fixture = style();
        fixture.sections[0].parts[1].notes = vec![note(2880, 3840, 36)];
        let compiled = fixture.compile("Main A", 0, &[]).expect("compiles");
        let bass = compiled
            .iter()
            .find(|n| n.channel.one_based() == 11)
            .expect("bass note");
        assert_eq!(bass.tick, 2880);
        assert_eq!(bass.duration, 960, "cut at the end of the bar");
    }

    #[test]
    fn unknown_sections_and_bad_lengths_are_errors() {
        let fixture = style();
        assert_eq!(
            fixture.compile("Fill In AA", 0, &[]),
            Err(StyleError::UnknownSection(String::from("Fill In AA")))
        );

        let mut broken = fixture.clone();
        broken.sections[0].length_ticks = 0;
        assert!(matches!(
            broken.compile("Main A", 0, &[]),
            Err(StyleError::SectionLength { length: 0, .. })
        ));

        let mut zero_note = fixture.clone();
        zero_note.sections[0].parts[0].notes = vec![note(0, 0, 38)];
        assert!(matches!(
            zero_note.compile("Main A", 0, &[]),
            Err(StyleError::NoteLength { duration: 0, .. })
        ));

        let mut bad_channel = fixture;
        bad_channel.sections[0].parts[0].rule.dest_channel = 17;
        assert_eq!(
            bad_channel.compile("Main A", 0, &[]),
            Err(StyleError::Channel(17))
        );
    }

    #[test]
    fn output_is_sorted_and_channels_come_from_the_rules() {
        let compiled = style().compile("Main A", 0, &[]).expect("compiles");
        let mut sorted = compiled.clone();
        sorted.sort_by_key(|note| (note.tick, note.channel.zero_based(), note.pitch));
        assert_eq!(sorted, compiled);
        let channels: Vec<u8> = compiled.iter().map(|n| n.channel.one_based()).collect();
        assert!(channels.contains(&10) && channels.contains(&11));
    }

    #[test]
    fn source_chord_is_used_before_the_first_chord_event() {
        let compiled = style()
            .compile(
                "Main A",
                0,
                &[ChordAt {
                    tick: 1920,
                    chord: chord(5, ChordType::Major),
                }],
            )
            .expect("compiles");
        let first_bass = compiled
            .iter()
            .find(|n| n.channel.one_based() == 11)
            .expect("bass");
        assert_eq!(
            first_bass.pitch, 36,
            "C major from the style's source chord"
        );
    }

    #[test]
    fn a_style_can_be_built_from_song_tracks() {
        use music_core::{Channel, PPQ, Pitch, PitchClass, Velocity};
        use project::ids::{NoteId, TrackId};
        use project::song::{Note, Track};
        let mut song = Song::new("Clips", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Bass");
        track.notes.push(Note {
            id: NoteId::new(1).expect("note"),
            start: PPQ / 2,
            duration: PPQ,
            pitch: Pitch::from_name("C2").expect("pitch"),
            velocity: Velocity::new(100).expect("velocity"),
            channel: Channel::from_one_based(11).expect("channel"),
        });
        track.notes.push(Note {
            id: NoteId::new(2).expect("note"),
            start: PPQ * 2,
            duration: PPQ,
            pitch: Pitch::from_name("D2").expect("pitch"),
            velocity: Velocity::new(90).expect("velocity"),
            channel: Channel::from_one_based(11).expect("channel"),
        });
        song.tracks.push(track);
        let style = from_song(&song, "Clips", chord(0, ChordType::Major), PPQ);
        assert_eq!(style.sections.len(), 1);
        assert_eq!(style.sections[0].parts.len(), 1);
        assert_eq!(style.sections[0].parts[0].notes.len(), 1);
        assert_eq!(style.sections[0].parts[0].notes[0].tick, PPQ / 2);
        assert_eq!(style.sections[0].parts[0].notes[0].duration, PPQ / 2);
    }
}
