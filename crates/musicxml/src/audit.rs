//! Round-trip audit: what MusicXML export and import preserve.

use music_core::Tick;
use project::song::Song;

/// Counts on both sides of a round trip, with the differences named.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Delta {
    /// Tracks before and after.
    pub tracks: (usize, usize),
    /// Notes before and after.
    pub notes: (usize, usize),
    /// Chords before and after.
    pub chords: (usize, usize),
    /// Lyrics before and after.
    pub lyrics: (usize, usize),
    /// Score marks before and after.
    pub marks: (usize, usize),
    /// Notes the import is expected to merge through ties.
    pub ties_merged: usize,
    /// Human-readable differences that are losses.
    pub changes: Vec<String>,
    /// Differences explained by tie merging.
    pub info: Vec<String>,
}

impl Delta {
    /// True when every count matches.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Compares an original song with its re-imported copy.
#[must_use]
pub fn audit(original: &Song, imported: &Song) -> Delta {
    let mut delta = Delta {
        tracks: (original.tracks.len(), imported.tracks.len()),
        notes: (count_notes(original), count_notes(imported)),
        chords: (original.chords.len(), imported.chords.len()),
        lyrics: (original.lyrics.len(), imported.lyrics.len()),
        marks: (original.marks.len(), imported.marks.len()),
        ties_merged: tie_merges(original),
        changes: Vec::new(),
        info: Vec::new(),
    };
    for (name, pair) in [
        ("tracks", delta.tracks),
        ("chords", delta.chords),
        ("lyrics", delta.lyrics),
        ("marks", delta.marks),
    ] {
        if pair.0 != pair.1 {
            delta
                .changes
                .push(format!("{name}: {} -> {}", pair.0, pair.1));
        }
    }
    let merged = delta.notes.0.saturating_sub(delta.notes.1);
    if delta.notes.1 < delta.notes.0.saturating_sub(delta.ties_merged) {
        delta
            .changes
            .push(format!("notes: {} -> {}", delta.notes.0, delta.notes.1));
    } else if merged > 0 {
        delta.info.push(format!(
            "{merged} note(s) merged by ties (up to {} expected)",
            delta.ties_merged
        ));
    }
    for (index, (left, right)) in original.tracks.iter().zip(&imported.tracks).enumerate() {
        let allowed = track_tie_merges(left);
        let lost = left.notes.len().saturating_sub(right.notes.len());
        if right.notes.len() > left.notes.len() || lost > allowed {
            delta.changes.push(format!(
                "track {index} ({}): {} -> {} notes",
                left.name,
                left.notes.len(),
                right.notes.len()
            ));
        } else if lost > 0 {
            delta.info.push(format!(
                "track {index} ({}): {lost} note(s) merged by ties",
                left.name
            ));
        }
    }
    delta
}

fn tie_merges(song: &Song) -> usize {
    song.tracks.iter().map(track_tie_merges).sum()
}

fn track_tie_merges(track: &project::song::Track) -> usize {
    let mut merges = 0;
    let mut notes: Vec<&project::song::Note> = track.notes.iter().collect();
    notes.sort_by_key(|note| note.start);
    let mut open: std::collections::BTreeMap<u8, Tick> = std::collections::BTreeMap::new();
    for note in notes {
        let pitch = note.pitch.as_u8();
        if let Some(end) = open.get(&pitch)
            && *end == note.start
        {
            merges += 1;
        }
        open.insert(pitch, note.start.saturating_add(note.duration.max(1)));
    }
    merges
}

fn count_notes(song: &Song) -> usize {
    song.tracks.iter().map(|track| track.notes.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::song::{ChordEvent, LyricEvent, Note, Track};
    use project::{NoteId, TrackId};

    fn song() -> Song {
        let mut song = Song::new("Audit", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        song.chords.push(ChordEvent {
            tick: 0,
            chord: music_core::Chord::parse("C").expect("valid"),
        });
        song.lyrics.push(LyricEvent {
            tick: 0,
            text: String::from("la"),
        });
        song
    }

    #[test]
    fn identical_songs_are_clean() {
        let song = song();
        let delta = audit(&song, &song);
        assert!(delta.is_clean());
        assert_eq!(delta.notes, (1, 1));
    }

    #[test]
    fn differences_are_named() {
        let original = song();
        let mut changed = song();
        changed.tracks[0].notes.clear();
        changed.chords.clear();
        let delta = audit(&original, &changed);
        assert!(!delta.is_clean());
        assert!(
            delta
                .changes
                .iter()
                .any(|change| change.starts_with("notes"))
        );
        assert!(delta.changes.iter().any(|change| change.contains("Piano")));
        assert!(
            delta
                .changes
                .iter()
                .any(|change| change.starts_with("chords"))
        );
    }
}
