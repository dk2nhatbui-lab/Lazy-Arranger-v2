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
        if track.program.is_some_and(|program| program > 127)
            || track
                .bank
                .is_some_and(|bank| bank.msb > 127 || bank.lsb > 127)
        {
            issues.push(issue(
                Severity::Error,
                format!("{track_path}.program"),
                String::from("program and bank values must be 0..=127"),
            ));
        }
        let mut plugin_ids = HashSet::new();
        for plugin in &track.plugins {
            if !plugin_ids.insert(plugin.id) {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.plugins"),
                    format!("duplicate plug-in id {}", plugin.id),
                ));
            }
        }
        if track.volume > 127 || track.pan > 127 || track.send_level > 127 {
            issues.push(issue(
                Severity::Error,
                format!("{track_path}.mix"),
                String::from("volume, pan and send level must be 0..=127"),
            ));
        }
        for (lane_index, lane) in track.automation.iter().enumerate() {
            let mut previous = None;
            for point in &lane.points {
                if point.tick < 0
                    || !point.value.is_finite()
                    || !(0.0..=127.0).contains(&point.value)
                {
                    issues.push(issue(
                        Severity::Error,
                        format!("{track_path}.automation[{lane_index}]"),
                        String::from(
                            "automation points need a non-negative tick and value 0..=127",
                        ),
                    ));
                    break;
                }
                if previous.is_some_and(|tick| point.tick < tick) {
                    issues.push(issue(
                        Severity::Warning,
                        format!("{track_path}.automation[{lane_index}]"),
                        String::from("automation points are not sorted"),
                    ));
                }
                previous = Some(point.tick);
            }
        }
        for (slur_index, slur) in track.slurs.iter().enumerate() {
            if slur.start >= slur.end {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.slurs[{slur_index}]"),
                    String::from("a slur must end after it starts"),
                ));
            }
        }
        if let Some(reference) = &track.audio
            && (reference.path.trim().is_empty()
                || reference.start < 0
                || reference.length <= 0
                || reference.offset < 0
                || reference.fade_in < 0
                || reference.fade_out < 0
                || (reference.length > 0
                    && reference.fade_in.saturating_add(reference.fade_out) > reference.length))
        {
            issues.push(issue(
                Severity::Error,
                format!("{track_path}.audio"),
                String::from(
                    "path is empty, start/offset negative, length non-positive, or fades do not fit",
                ),
            ));
        }
        for (take_index, take) in track.takes.iter().enumerate() {
            if take.path.trim().is_empty() {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.takes[{take_index}].path"),
                    String::from("audio take path is empty"),
                ));
            }
            if take.fade_in < 0
                || take.fade_out < 0
                || (take.length > 0 && take.fade_in.saturating_add(take.fade_out) > take.length)
            {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.takes[{take_index}]"),
                    String::from("take fades must be non-negative and fit inside its length"),
                ));
            }
        }
        let mut comps = track.comps.clone();
        comps.sort_by_key(|comp| comp.start);
        for (comp_index, comp) in track.comps.iter().enumerate() {
            if comp.take >= track.takes.len() {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.comps[{comp_index}].take"),
                    String::from("comp refers to an unknown take"),
                ));
            }
            if comp.start < 0 || comp.length <= 0 {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.comps[{comp_index}]"),
                    String::from("a comp needs a non-negative start and positive length"),
                ));
            }
        }
        for pair in comps.windows(2) {
            if pair[0].start.saturating_add(pair[0].length) > pair[1].start {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.comps"),
                    format!("comp spans overlap at tick {}", pair[1].start),
                ));
            }
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
            if note.duration > 0
                && let Some(end) = note.start.checked_add(note.duration)
            {
                sounding.insert(key, end);
            }
        }
    }

    let mut bus_ids = HashSet::new();
    for (bus_index, bus) in song.buses.iter().enumerate() {
        if !bus_ids.insert(bus.id) {
            issues.push(issue(
                Severity::Error,
                format!("song.buses[{bus_index}].id"),
                format!("duplicate bus id {}", bus.id),
            ));
        }
        if bus.volume > 127 || bus.pan > 127 {
            issues.push(issue(
                Severity::Error,
                format!("song.buses[{bus_index}]"),
                String::from("bus volume and pan must be 0..=127"),
            ));
        }
        let mut plugin_ids = HashSet::new();
        for plugin in &bus.plugins {
            if !plugin_ids.insert(plugin.id) {
                issues.push(issue(
                    Severity::Error,
                    format!("song.buses[{bus_index}].plugins"),
                    format!("duplicate plug-in id {}", plugin.id),
                ));
            }
        }
    }
    let mut master_plugins = HashSet::new();
    for plugin in &song.master_plugins {
        if !master_plugins.insert(plugin.id) {
            issues.push(issue(
                Severity::Error,
                "song.master_plugins",
                format!("duplicate plug-in id {}", plugin.id),
            ));
        }
    }
    for (track_index, track) in song.tracks.iter().enumerate() {
        for (label, bus) in [("bus", track.bus), ("send", track.send)] {
            if let Some(id) = bus
                && !bus_ids.contains(&id)
            {
                issues.push(issue(
                    Severity::Error,
                    format!("song.tracks[{track_index}].{label}"),
                    format!("unknown bus {id}"),
                ));
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

    for (index, mark) in song.marks.iter().enumerate() {
        if mark.tick < 0 {
            issues.push(issue(
                Severity::Error,
                format!("song.marks[{index}].tick"),
                String::from("negative mark tick"),
            ));
        }
    }
    for (index, lyric) in song.lyrics.iter().enumerate() {
        if lyric.tick < 0 {
            issues.push(issue(
                Severity::Error,
                format!("song.lyrics[{index}].tick"),
                String::from("negative lyric tick"),
            ));
        }
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
    fn catches_duplicate_buses_and_dangling_routing() {
        use crate::song::Bus;
        let mut song = Song::new("x", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "t");
        track.bus = Some(9);
        song.tracks.push(track);
        song.buses.push(Bus::new(1, "A"));
        song.buses.push(Bus::new(1, "B"));
        let issues = validate(&ProjectFile::new(song));
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("duplicate bus id"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("unknown bus 9"))
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
