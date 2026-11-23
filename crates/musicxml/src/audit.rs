//! Round-trip audit: what MusicXML export and import preserve.

use music_core::Tick;
use project::song::Song;

/// Sorted drum (pitch, velocity) pairs.
pub type DrumPairs = Vec<(u8, u8)>;
/// A pitched note not involved in a tie: start, pitch, velocity, duration.
pub type NoteSignature = (Tick, u8, u8, Tick);

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
    /// Phrase slurs before and after.
    pub slurs: (usize, usize),
    /// Notes the import is expected to merge through ties.
    pub ties_merged: usize,
    /// Drum (pitch, velocity) pairs before and after.
    pub drums: (DrumPairs, DrumPairs),
    /// Untied pitched notes before and after.
    pub detail: (Vec<NoteSignature>, Vec<NoteSignature>),
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
        slurs: (
            original.tracks.iter().map(|track| track.slurs.len()).sum(),
            imported.tracks.iter().map(|track| track.slurs.len()).sum(),
        ),
        ties_merged: tie_merges(original),
        drums: (drum_notes(original), drum_notes(imported)),
        detail: (untied_notes(original), untied_notes(imported)),
        changes: Vec::new(),
        info: Vec::new(),
    };
    for (name, pair) in [
        ("tracks", delta.tracks),
        ("chords", delta.chords),
        ("lyrics", delta.lyrics),
        ("marks", delta.marks),
        ("slurs", delta.slurs),
    ] {
        if pair.0 != pair.1 {
            delta
                .changes
                .push(format!("{name}: {} -> {}", pair.0, pair.1));
        }
    }
    if delta.detail.0 != delta.detail.1 {
        let mut example = String::new();
        if let Some((left, right)) = delta
            .detail
            .0
            .iter()
            .zip(&delta.detail.1)
            .find(|(left, right)| left != right)
        {
            example = format!(" (e.g. {left:?} vs {right:?})");
        }
        delta.changes.push(format!(
            "pitched notes: {} -> {}{example}",
            delta.detail.0.len(),
            delta.detail.1.len()
        ));
    }
    if delta.drums.0 != delta.drums.1 {
        delta.changes.push(format!(
            "drum pitches/velocities: {} -> {} pairs",
            delta.drums.0.len(),
            delta.drums.1.len()
        ));
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
        let left_detail = merged_track_notes(left);
        let right_detail = merged_track_notes(right);
        if left_detail != right_detail {
            delta.changes.push(format!(
                "track {index} ({}): {} -> {} untied pitched note(s)",
                left.name,
                left_detail.len(),
                right_detail.len()
            ));
        }
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

fn untied_notes(song: &Song) -> Vec<NoteSignature> {
    song.tracks.iter().flat_map(merged_track_notes).collect()
}

fn merged_track_notes(track: &project::song::Track) -> Vec<NoteSignature> {
    let mut notes: Vec<&project::song::Note> = track.notes.iter().collect();
    notes.sort_by_key(|note| note.start);
    let mut merged: Vec<NoteSignature> = Vec::new();
    let mut open: std::collections::BTreeMap<u8, usize> = std::collections::BTreeMap::new();
    for note in notes {
        if note.channel.zero_based() == 9 {
            continue;
        }
        let pitch = note.pitch.as_u8();
        if let Some(index) = open.get(&pitch).copied() {
            let (start, _, velocity, duration) = merged[index];
            if start.saturating_add(duration) == note.start {
                merged[index] = (
                    start,
                    pitch,
                    velocity,
                    duration.saturating_add(note.duration.max(1)),
                );
                continue;
            }
        }
        merged.push((note.start, pitch, note.velocity.as_u8(), note.duration));
        open.insert(pitch, merged.len() - 1);
    }
    merged.sort_unstable();
    merged
}

fn drum_notes(song: &Song) -> Vec<(u8, u8)> {
    let mut pairs: Vec<(u8, u8)> = song
        .tracks
        .iter()
        .flat_map(|track| &track.notes)
        .filter(|note| note.channel.zero_based() == 9)
        .map(|note| (note.pitch.as_u8(), note.velocity.as_u8()))
        .collect();
    pairs.sort_unstable();
    pairs
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
    fn drum_changes_are_named() {
        let original = song();
        let mut changed = song();
        changed.tracks[0].notes.push(Note {
            id: NoteId::new(2).expect("valid"),
            start: PPQ,
            duration: PPQ,
            pitch: Pitch::new(38).expect("valid"),
            velocity: Velocity::new(90).expect("valid"),
            channel: Channel::drums(),
        });
        let delta = audit(&original, &changed);
        assert!(delta.changes.iter().any(|change| change.contains("drum")));
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
