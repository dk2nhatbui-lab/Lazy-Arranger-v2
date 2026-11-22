//! Provenance for generated tracks, and the digests that detect change.
//!
//! ADR-005 asks for one rule above all: **regeneration must never overwrite
//! work silently**. A track that the arranger generated therefore records
//!
//! * the style and section it came from,
//! * the timeline range it covers,
//! * a digest of the musical intent (chords, arranger events, tempo, meter)
//!   that was in force when it was generated, and
//! * a digest of the notes as generated.
//!
//! Comparing those digests answers two separate questions: has the intent
//! changed since (stale), and have the notes been edited by hand (edited).
//! A track with no provenance is manual and is never touched.
//!
//! Digests are FNV-1a over a canonical byte encoding. They are for change
//! detection inside one document, not for security.

use music_core::{Chord, Spelling, Tick};

use crate::ids::{StyleId, TrackId};
use crate::song::{Note, Song, Track};

/// Where a generated track came from and what it was generated against.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Provenance {
    /// Style that generated the track.
    pub style: StyleId,
    /// Section of that style.
    pub section: String,
    /// First tick of the generated range.
    pub start: Tick,
    /// One past the last tick of the generated range.
    pub end: Tick,
    /// Digest of the intent in force over `start..end` at generation time.
    pub intent: u64,
    /// Digest of the notes as generated.
    pub notes: u64,
}

impl Provenance {
    /// Builds provenance for a track from the song's current intent.
    #[must_use]
    pub fn capture(
        song: &Song,
        style: StyleId,
        section: impl Into<String>,
        start: Tick,
        end: Tick,
        notes: &[Note],
    ) -> Self {
        Self {
            style,
            section: section.into(),
            start,
            end,
            intent: intent_digest(song, start, end),
            notes: notes_digest(notes),
        }
    }

    /// True when the range is sane.
    #[must_use]
    pub fn is_well_formed(&self) -> bool {
        self.start >= 0 && self.end > self.start && !self.section.trim().is_empty()
    }
}

/// How a track relates to the current document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackStatus {
    /// No provenance: a hand-made or frozen track.
    Manual,
    /// Generated from the current intent and notes untouched since.
    Fresh,
    /// Intent changed since generation; the notes are as generated.
    Stale,
    /// Intent unchanged, notes edited by hand.
    Edited,
    /// Both changed.
    StaleAndEdited,
}

impl TrackStatus {
    /// True when the track was generated at all.
    #[must_use]
    pub fn is_generated(self) -> bool {
        !matches!(self, Self::Manual)
    }

    /// True when regenerating would discard a manual edit.
    #[must_use]
    pub fn has_edits(self) -> bool {
        matches!(self, Self::Edited | Self::StaleAndEdited)
    }

    /// True when the intent moved on.
    #[must_use]
    pub fn is_stale(self) -> bool {
        matches!(self, Self::Stale | Self::StaleAndEdited)
    }
}

/// Classifies a track against the current song.
#[must_use]
pub fn track_status(song: &Song, track: &Track) -> TrackStatus {
    let Some(provenance) = track.provenance.as_ref() else {
        return TrackStatus::Manual;
    };
    let stale = provenance.intent != intent_digest(song, provenance.start, provenance.end);
    let edited = provenance.notes != notes_digest(&track.notes);
    match (stale, edited) {
        (false, false) => TrackStatus::Fresh,
        (true, false) => TrackStatus::Stale,
        (false, true) => TrackStatus::Edited,
        (true, true) => TrackStatus::StaleAndEdited,
    }
}

/// Classifies every track, in song order.
#[must_use]
pub fn track_statuses(song: &Song) -> Vec<(TrackId, TrackStatus)> {
    song.tracks
        .iter()
        .map(|track| (track.id, track_status(song, track)))
        .collect()
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

struct Hasher(u64);

impl Hasher {
    fn new(label: &str) -> Self {
        let mut hasher = Self(FNV_OFFSET);
        hasher.bytes(label.as_bytes());
        hasher
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_be_bytes());
    }

    fn i64(&mut self, value: i64) {
        self.u64(u64::from_ne_bytes(value.to_ne_bytes()));
    }

    fn str(&mut self, value: &str) {
        self.u64(u64::try_from(value.len()).unwrap_or(u64::MAX));
        self.bytes(value.as_bytes());
    }

    fn finish(self) -> u64 {
        self.0
    }
}

/// Digest of the musical intent that shapes a generated range.
///
/// Covers the meter, tempo in force and every tempo change up to `end`, the
/// chords that start in the range, and the arranger events in the range.
/// Changes outside the range do not affect it, so regenerating one section
/// does not make another section look stale.
#[must_use]
pub fn intent_digest(song: &Song, start: Tick, end: Tick) -> u64 {
    let mut hasher = Hasher::new("intent-v2");
    hasher.i64(start);
    hasher.i64(end);
    hasher.u64(u64::from(song.time_signature.numerator));
    hasher.u64(u64::from(song.time_signature.denominator));
    hasher.u64(bpm_bits(song.tempo_map.bpm_at(start)));
    for change in song.tempo_map.changes() {
        if change.tick > end {
            break;
        }
        hasher.i64(change.tick);
        hasher.u64(bpm_bits(change.bpm));
    }
    for event in &song.chords {
        if event.tick < start || event.tick >= end {
            continue;
        }
        hasher.i64(event.tick);
        hasher.bytes(chord_text(event.chord).as_bytes());
    }
    for event in &song.arranger {
        let tick = event.tick();
        if tick < start || tick >= end {
            continue;
        }
        hasher.i64(tick);
        match event {
            crate::song::ArrangerEvent::SelectStyle { style_id, .. } => {
                hasher.str("select-style");
                hasher.u64(u64::from(style_id.get()));
            }
            crate::song::ArrangerEvent::SelectVariation { variation, .. } => {
                hasher.str("select-variation");
                hasher.u64(u64::from(*variation));
            }
            crate::song::ArrangerEvent::TriggerFill { fill, .. } => {
                hasher.str("trigger-fill");
                hasher.u64(u64::from(*fill));
            }
            crate::song::ArrangerEvent::Start { .. } => hasher.str("start"),
            crate::song::ArrangerEvent::Stop { .. } => hasher.str("stop"),
        }
    }
    hasher.finish()
}

/// Digest of notes as musical content, independent of order and identifiers.
///
/// Moving a note, changing its pitch, length, velocity or channel changes the
/// digest. Reordering the list or renumbering ids does not.
#[must_use]
pub fn notes_digest(notes: &[Note]) -> u64 {
    let mut canonical: Vec<(Tick, u8, u8, Tick, u8)> = notes
        .iter()
        .map(|note| {
            (
                note.start,
                note.channel.zero_based(),
                note.pitch.as_u8(),
                note.duration,
                note.velocity.as_u8(),
            )
        })
        .collect();
    canonical.sort_unstable();
    let mut hasher = Hasher::new("notes-v2");
    hasher.u64(u64::try_from(canonical.len()).unwrap_or(u64::MAX));
    for (start, channel, pitch, duration, velocity) in canonical {
        hasher.i64(start);
        hasher.i64(duration);
        hasher.u64(u64::from(pitch));
        hasher.u64(u64::from(velocity));
        hasher.u64(u64::from(channel));
    }
    hasher.finish()
}

fn chord_text(chord: Chord) -> String {
    chord.format(Spelling::Sharp)
}

fn bpm_bits(bpm: f64) -> u64 {
    bpm.to_bits()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Track;
    use crate::ids::{NoteId, StyleId};
    use crate::song::{ArrangerEvent, ChordEvent};
    use music_core::{Channel, Pitch, Velocity};

    fn note(id: u32, start: Tick, duration: Tick, name: &str) -> Note {
        Note {
            id: NoteId::new(id).expect("valid"),
            start,
            duration,
            pitch: Pitch::from_name(name).expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        }
    }

    fn song() -> Song {
        let mut song = Song::new("Provenance", 120.0);
        song.chords.push(ChordEvent {
            tick: 0,
            chord: Chord::parse("C").expect("valid"),
        });
        song
    }

    #[test]
    fn a_manual_track_has_no_provenance_and_is_manual() {
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Hand made");
        track.notes.push(note(1, 0, 960, "C4"));
        assert_eq!(track_status(&song(), &track), TrackStatus::Manual);
    }

    #[test]
    fn fresh_edited_stale_and_both_are_distinguished() {
        let mut song = song();
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Generated");
        track.notes.push(note(1, 0, 960, "C4"));
        track.provenance = Some(Provenance::capture(
            &song,
            StyleId::new(5).expect("valid"),
            "Main A",
            0,
            3840,
            &track.notes,
        ));
        assert_eq!(track_status(&song, &track), TrackStatus::Fresh);

        // Changing the notes means a hand edit.
        let mut edited = track.clone();
        edited.notes[0].velocity = Velocity::new(60).expect("valid");
        assert_eq!(track_status(&song, &edited), TrackStatus::Edited);

        // Changing the intent in the range makes it stale.
        song.chords[0].chord = Chord::parse("Am").expect("valid");
        assert_eq!(track_status(&song, &track), TrackStatus::Stale);
        assert_eq!(track_status(&song, &edited), TrackStatus::StaleAndEdited);
        assert!(track_status(&song, &edited).has_edits());
        assert!(track_status(&song, &edited).is_stale());
    }

    #[test]
    fn intent_outside_the_range_does_not_make_a_track_stale() {
        let mut song = song();
        song.chords.push(ChordEvent {
            tick: 7680,
            chord: Chord::parse("G").expect("valid"),
        });
        let _track = Track::new(TrackId::new(1).expect("valid"), "A");
        let provenance = Provenance::capture(
            &song,
            StyleId::new(1).expect("valid"),
            "Main A",
            0,
            3840,
            &[],
        );
        assert_eq!(provenance.intent, intent_digest(&song, 0, 3840));

        song.chords[1].chord = Chord::parse("F").expect("valid");
        assert_eq!(
            intent_digest(&song, 0, 3840),
            provenance.intent,
            "a chord at 7680 is outside the generated range"
        );
        assert_ne!(intent_digest(&song, 0, 7680), provenance.intent);
    }

    #[test]
    fn arranger_events_are_part_of_the_intent() {
        let mut song = song();
        let baseline = intent_digest(&song, 0, 3840);
        let earlier_range = intent_digest(&song, 0, 1920);
        song.arranger.push(ArrangerEvent::TriggerFill {
            tick: 1920,
            fill: 1,
        });
        assert_ne!(baseline, intent_digest(&song, 0, 3840));
        assert_eq!(
            earlier_range,
            intent_digest(&song, 0, 1920),
            "a fill at 1920 is outside 0..1920"
        );
    }

    #[test]
    fn tempo_changes_affect_the_digest() {
        let mut song = song();
        let baseline = intent_digest(&song, 0, 3840);
        song.tempo_map = song
            .tempo_map
            .clone()
            .with_change(1920, 90.0)
            .expect("valid change");
        assert_ne!(intent_digest(&song, 0, 3840), baseline);
        assert_ne!(intent_digest(&song, 0, 1920), baseline);
    }

    #[test]
    fn note_digest_ignores_order_and_ids() {
        let first = note(1, 0, 960, "C4");
        let second = note(2, 960, 960, "E4");
        let a = notes_digest(&[first.clone(), second.clone()]);
        let renamed = [
            Note {
                id: NoteId::new(9).expect("valid"),
                ..second
            },
            Note {
                id: NoteId::new(7).expect("valid"),
                ..first
            },
        ];
        assert_eq!(notes_digest(&renamed), a);

        let moved = Note {
            start: 480,
            ..first
        };
        assert_ne!(notes_digest(&[moved, second]), a);
        assert_ne!(
            notes_digest(&[Note {
                duration: 480,
                ..first
            }]),
            notes_digest(&[first])
        );
    }

    #[test]
    fn provenance_validation_flags_bad_ranges() {
        let song = song();
        let mut provenance =
            Provenance::capture(&song, StyleId::new(1).expect("valid"), "", -1, 0, &[]);
        assert!(!provenance.is_well_formed());
        provenance.start = 0;
        provenance.end = 1;
        provenance.section = String::from("Main A");
        assert!(provenance.is_well_formed());
    }
}
