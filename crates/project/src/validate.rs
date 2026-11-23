//! Structural validation of a project document.

use std::collections::{HashMap, HashSet};

use music_core::{Tick, TimeSignature};

use crate::song::ProjectFile;

/// How serious a validation finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The document must not be played or saved.
    Error,
    /// The document is playable but likely not what the author meant.
    Warning,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Error => "error",
            Self::Warning => "warning",
        })
    }
}

/// One validation finding, with a JSON-ish path to the offending value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    /// Error or warning.
    pub severity: Severity,
    /// Location such as `song.tracks[0].notes[2].start`.
    pub path: String,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} ({})", self.path, self.message, self.severity)
    }
}

/// Validates a project document and returns all findings in document order.
#[must_use]
pub fn validate(project: &ProjectFile) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let song = &project.song;

    if project.version == 0 {
        issues.push(issue(
            Severity::Error,
            "version",
            String::from("schema versions start at 1"),
        ));
    }

    if let Err(err) = song.tempo_map.validate() {
        issues.push(issue(Severity::Error, "song.tempo_map", err.to_string()));
    }
    if let Err(err) = TimeSignature::new(
        song.time_signature.numerator,
        song.time_signature.denominator,
    ) {
        issues.push(issue(
            Severity::Error,
            "song.time_signature",
            err.to_string(),
        ));
    }

    let mut track_ids: HashSet<u32> = HashSet::new();
    for (track_index, track) in song.tracks.iter().enumerate() {
        let track_path = format!("song.tracks[{track_index}]");
        if let Some(reference) = &track.audio
            && (reference.fade_in < 0
                || reference.fade_out < 0
                || (reference.length > 0
                    && reference.fade_in.saturating_add(reference.fade_out) > reference.length))
        {
            issues.push(issue(
                Severity::Error,
                format!("{track_path}.audio"),
                String::from("fades must be non-negative and fit inside the audio length"),
            ));
        }
        if let Some(font) = &track.soundfont
            && font.path.trim().is_empty()
        {
            issues.push(issue(
                Severity::Error,
                format!("{track_path}.soundfont.path"),
                String::from("soundfont path is empty"),
            ));
        }
        if !track_ids.insert(track.id.get()) {
            issues.push(issue(
                Severity::Error,
                format!("{track_path}.id"),
                format!("duplicate track id {}", track.id),
            ));
        }
        if track.name.trim().is_empty() {
            issues.push(issue(
                Severity::Warning,
                format!("{track_path}.name"),
                String::from("track has no name"),
            ));
        }

        let mut plugin_ids = HashSet::new();
        for plugin in &track.plugins {
            if !plugin_ids.insert(plugin.id) {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.plugins"),
                    format!("duplicate plugin id {}", plugin.id),
                ));
            }
            if plugin.name.trim().is_empty() {
                issues.push(issue(
                    Severity::Warning,
                    format!("{track_path}.plugins"),
                    format!("plugin {} has no name", plugin.id),
                ));
            }
        }

        let mut clips = track.clips.clone();
        clips.sort_by_key(|clip| clip.start);
        for pair in clips.windows(2) {
            if pair[0].end() > pair[1].start {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.clips"),
                    format!("clips overlap at tick {}", pair[1].start),
                ));
            }
        }
        let mut clip_ids = HashSet::new();
        for clip in &track.clips {
            if clip.length <= 0 {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.clips"),
                    format!("clip {} has a non-positive length", clip.id),
                ));
            }
            if clip.start < 0 {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.clips"),
                    format!("clip {} starts before zero", clip.id),
                ));
            }
            if !clip_ids.insert(clip.id) {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.clips"),
                    format!("duplicate clip id {}", clip.id),
                ));
            }
        }

        if let Some(provenance) = &track.provenance {
            if !provenance.is_well_formed() {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.provenance"),
                    format!(
                        "range {}..{} or section {:?} is not usable",
                        provenance.start, provenance.end, provenance.section
                    ),
                ));
            }
            if track.notes.is_empty() {
                issues.push(issue(
                    Severity::Warning,
                    format!("{track_path}.notes"),
                    String::from("generated track has no notes"),
                ));
            }
        }

        let mut note_ids: HashSet<u32> = HashSet::new();
        let mut previous_start: Option<Tick> = None;
        let mut sounding: HashMap<(u8, u8), Tick> = HashMap::new();
        for (note_index, note) in track.notes.iter().enumerate() {
            let note_path = format!("{track_path}.notes[{note_index}]");
            if !note_ids.insert(note.id.get()) {
                issues.push(issue(
                    Severity::Error,
                    format!("{note_path}.id"),
                    format!("duplicate note id {}", note.id),
                ));
            }
            if note.start < 0 {
                issues.push(issue(
                    Severity::Error,
                    format!("{note_path}.start"),
                    format!("negative start tick {}", note.start),
                ));
            }
            if note.duration <= 0 {
                issues.push(issue(
                    Severity::Error,
                    format!("{note_path}.duration"),
                    format!("duration must be positive, found {}", note.duration),
                ));
            }
            if note.start.checked_add(note.duration).is_none() {
                issues.push(issue(
                    Severity::Error,
                    format!("{note_path}.duration"),
                    String::from("note end overflows the tick range"),
                ));
            }
            if let Some(previous) = previous_start
                && note.start < previous
            {
                issues.push(issue(
                    Severity::Warning,
                    format!("{note_path}.start"),
                    String::from("notes are not sorted by start tick"),
                ));
            }
            previous_start = Some(note.start);

            let key = (note.channel.zero_based(), note.pitch.as_u8());
            if let Some(previous_end) = sounding.get(&key)
                && note.start < *previous_end
                && note.duration > 0
            {
                issues.push(issue(
                    Severity::Warning,
                    format!("{note_path}.start"),
                    format!(
                        "overlaps another note on channel {} pitch {} ending at {previous_end}",
                        note.channel, note.pitch
                    ),
                ));
            }
            if note.duration > 0 {
                sounding.insert(key, note.end());
            }
        }
    }

    let mut previous_chord_tick: Option<Tick> = None;
    for (index, event) in song.chords.iter().enumerate() {
        let path = format!("song.chords[{index}].tick");
        if event.tick < 0 {
            issues.push(issue(
                Severity::Error,
                &path,
                format!("negative tick {}", event.tick),
            ));
        }
        if let Some(previous) = previous_chord_tick {
            if event.tick == previous {
                issues.push(issue(
                    Severity::Error,
                    &path,
                    format!("two chords at tick {previous}"),
                ));
            } else if event.tick < previous {
                issues.push(issue(
                    Severity::Error,
                    &path,
                    format!(
                        "chord events are not sorted: {previous} then {}",
                        event.tick
                    ),
                ));
            }
        }
        previous_chord_tick = Some(event.tick);
    }

    let mut previous_arranger_tick: Option<Tick> = None;
    for (index, event) in song.arranger.iter().enumerate() {
        let path = format!("song.arranger[{index}].tick");
        if event.tick() < 0 {
            issues.push(issue(
                Severity::Error,
                &path,
                format!("negative tick {}", event.tick()),
            ));
        }
        if let Some(previous) = previous_arranger_tick
            && event.tick() < previous
        {
            issues.push(issue(
                Severity::Error,
                &path,
                format!(
                    "arranger events are not sorted: {previous} then {}",
                    event.tick()
                ),
            ));
        }
        previous_arranger_tick = Some(event.tick());
    }

    let mut previous_marker_tick: Option<Tick> = None;
    for (index, marker) in song.markers.iter().enumerate() {
        let path = format!("song.markers[{index}].tick");
        if marker.tick < 0 {
            issues.push(issue(
                Severity::Error,
                &path,
                format!("negative tick {}", marker.tick),
            ));
        }
        if let Some(previous) = previous_marker_tick
            && marker.tick < previous
        {
            issues.push(issue(
                Severity::Warning,
                &path,
                String::from("markers are not sorted by tick"),
            ));
        }
        previous_marker_tick = Some(marker.tick);
    }

    if song.tracks.is_empty() {
        issues.push(issue(
            Severity::Warning,
            "song.tracks",
            String::from("song has no tracks"),
        ));
    }

    issues
}

fn issue(
    severity: Severity,
    path: impl Into<String>,
    message: impl Into<String>,
) -> ValidationIssue {
    ValidationIssue {
        severity,
        path: path.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{NoteId, TrackId};
    use crate::song::{Note, Song, Track};
    use music_core::{Channel, Pitch, TempoMap, Velocity};

    fn note(id: u32, start: Tick, duration: Tick, name: &str) -> Note {
        Note {
            id: NoteId::new(id).expect("valid"),
            start,
            duration,
            pitch: Pitch::from_name(name).expect("valid"),
            velocity: Velocity::new(80).expect("valid"),
            channel: Channel::first(),
        }
    }

    #[test]
    fn empty_song_warns_but_is_playable() {
        let project = ProjectFile::new(Song::default());
        let issues = validate(&project);
        assert!(
            issues
                .iter()
                .all(|issue| issue.severity == Severity::Warning)
        );
        assert!(issues.iter().any(|issue| issue.path == "song.tracks"));
    }

    #[test]
    fn catches_geometry_errors() {
        let mut song = Song::new("x", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "t");
        track.notes.push(note(1, 0, 0, "C4"));
        track.notes.push(note(2, -10, 5, "D4"));
        song.tracks.push(track);
        let project = ProjectFile::new(song);
        let errors: Vec<String> = validate(&project)
            .into_iter()
            .filter(|issue| issue.severity == Severity::Error)
            .map(|issue| issue.path)
            .collect();
        assert!(errors.contains(&String::from("song.tracks[0].notes[0].duration")));
        assert!(errors.contains(&String::from("song.tracks[0].notes[1].start")));
    }

    #[test]
    fn warns_on_overlapping_same_pitch_notes() {
        let mut song = Song::new("x", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "t");
        track.notes.push(note(1, 0, 960, "C4"));
        track.notes.push(note(2, 480, 960, "C4"));
        song.tracks.push(track);
        let issues = validate(&ProjectFile::new(song));
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("overlaps another note")),
            "{issues:?}"
        );
    }

    #[test]
    fn catches_invalid_tempo_map_from_json() {
        let json = r#"{
            "format": "lazy-arranger-project",
            "version": 1,
            "song": {
                "tempo_map": {"changes": []},
                "time_signature": {"numerator": 4, "denominator": 4},
                "tracks": []
            }
        }"#;
        // Parsing succeeds because validation is explicit; from_json reports it.
        let raw: ProjectFile = serde_json::from_str(json).expect("shape is valid JSON");
        assert_eq!(raw.version, 1);
        let issues = validate(&raw);
        assert!(
            issues
                .iter()
                .any(|issue| issue.path == "song.tempo_map" && issue.severity == Severity::Error),
            "{issues:?}"
        );
        assert!(ProjectFile::from_json(json).is_err());
        assert_eq!(TempoMap::default().bpm_at(0), 120.0);
    }
}
