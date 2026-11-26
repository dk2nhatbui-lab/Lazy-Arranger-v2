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
        if track.volume > 127 || track.pan > 127 || track.send_level > 127 {
            issues.push(issue(
                Severity::Error,
                format!("{track_path}.mix"),
                String::from("volume, pan and send level must be 0..=127"),
            ));
        }
        for (slot_index, clip) in track.slots.iter().enumerate() {
            let Some(clip) = clip else { continue };
            let path = format!("{track_path}.slots[{slot_index}]");
            if clip.length <= 0 {
                issues.push(issue(
                    Severity::Error,
                    path.clone(),
                    String::from("a loop clip needs a positive length"),
                ));
            }
            let mut ids = std::collections::BTreeSet::new();
            for note in &clip.notes {
                if note.start < 0
                    || note.duration <= 0
                    || note.start.checked_add(note.duration).is_none()
                {
                    issues.push(issue(
                        Severity::Error,
                        path.clone(),
                        String::from(
                            "loop clip notes need a start of zero or more and a positive length",
                        ),
                    ));
                    break;
                }
                if !ids.insert(note.id) {
                    issues.push(issue(
                        Severity::Error,
                        path.clone(),
                        format!("loop clip note id {} is used twice", note.id.get()),
                    ));
                    break;
                }
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
            if slur.start < 0 {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.slurs[{slur_index}]"),
                    String::from("a slur cannot start before zero"),
                ));
            }
        }
        if let Some(patch) = &track.patch {
            // NaN or infinity would poison the synth and save as JSON null.
            let values = [
                patch.attack,
                patch.decay,
                patch.sustain,
                patch.release,
                patch.brightness,
            ];
            if values
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
            {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.patch"),
                    String::from("patch values must be finite and not negative"),
                ));
            }
        }
        for lane in &track.automation {
            if let crate::song::AutomationTarget::PluginParam { slot, .. } = lane.target
                && !track.plugins.iter().any(|plugin| plugin.id == slot)
            {
                issues.push(issue(
                    Severity::Warning,
                    format!("{track_path}.automation"),
                    format!("automation targets plug-in slot {slot}, which the track lacks"),
                ));
            }
        }
        let spans = track
            .clips
            .iter()
            .map(|clip| (clip.start, clip.length))
            .chain(
                track
                    .audio
                    .iter()
                    .chain(track.takes.iter())
                    .map(|reference| (reference.start, reference.length)),
            )
            .chain(track.comps.iter().map(|comp| (comp.start, comp.length)));
        for (start, length) in spans {
            if start.checked_add(length).is_none() {
                issues.push(issue(
                    Severity::Error,
                    &track_path,
                    format!("a clip or audio span at {start} runs past the end of time"),
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
            if take.start < 0
                || take.length <= 0
                || take.offset < 0
                || take.fade_in < 0
                || take.fade_out < 0
                || (take.length > 0 && take.fade_in.saturating_add(take.fade_out) > take.length)
            {
                issues.push(issue(
                    Severity::Error,
                    format!("{track_path}.takes[{take_index}]"),
                    String::from(
                        "a take needs a non-negative start/offset, positive length and fitting fades",
                    ),
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

        let mut automation_targets = HashSet::new();
        for (lane_index, lane) in track.automation.iter().enumerate() {
            let lane_path = format!("{track_path}.automation[{lane_index}]");
            if !automation_targets.insert(format!("{:?}", lane.target)) {
                issues.push(issue(
                    Severity::Error,
                    &lane_path,
                    String::from("duplicate automation target"),
                ));
            }
            let mut previous: Option<Tick> = None;
            for (point_index, point) in lane.points.iter().enumerate() {
                if point.tick < 0 {
                    issues.push(issue(
                        Severity::Error,
                        format!("{lane_path}.points[{point_index}].tick"),
                        format!("negative tick {}", point.tick),
                    ));
                }
                if !point.value.is_finite() || !(0.0..=127.0).contains(&point.value) {
                    issues.push(issue(
                        Severity::Error,
                        format!("{lane_path}.points[{point_index}].value"),
                        format!("value {} is outside 0..=127", point.value),
                    ));
                }
                if let Some(previous) = previous
                    && point.tick < previous
                {
                    issues.push(issue(
                        Severity::Error,
                        format!("{lane_path}.points[{point_index}].tick"),
                        format!("points are not sorted: {previous} then {}", point.tick),
                    ));
                }
                previous = Some(point.tick);
            }
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

    let mut previous_mark_tick = None;
    for (index, mark) in song.marks.iter().enumerate() {
        if mark.tick < 0 {
            issues.push(issue(
                Severity::Error,
                format!("song.marks[{index}].tick"),
                String::from("negative mark tick"),
            ));
        }
        if let Some(previous) = previous_mark_tick
            && mark.tick < previous
        {
            issues.push(issue(
                Severity::Error,
                format!("song.marks[{index}].tick"),
                String::from("marks are not sorted by tick"),
            ));
        }
        previous_mark_tick = Some(mark.tick);
    }
    let mut previous_lyric_tick = None;
    for (index, lyric) in song.lyrics.iter().enumerate() {
        if lyric.tick < 0 {
            issues.push(issue(
                Severity::Error,
                format!("song.lyrics[{index}].tick"),
                String::from("negative lyric tick"),
            ));
        }
        if let Some(previous) = previous_lyric_tick {
            if lyric.tick == previous {
                issues.push(issue(
                    Severity::Error,
                    format!("song.lyrics[{index}].tick"),
                    String::from("two lyrics at the same tick"),
                ));
            } else if lyric.tick < previous {
                issues.push(issue(
                    Severity::Error,
                    format!("song.lyrics[{index}].tick"),
                    String::from("lyrics are not sorted by tick"),
                ));
            }
        }
        previous_lyric_tick = Some(lyric.tick);
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
    fn rejects_bad_loop_clips() {
        use crate::song::SlotClip;
        let mut song = Song::new("x", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "t");
        track.slots.push(Some(SlotClip {
            name: String::from("empty"),
            length: 0,
            notes: Vec::new(),
        }));
        track.slots.push(None);
        track.slots.push(Some(SlotClip {
            name: String::from("bad note"),
            length: 960,
            notes: vec![note(1, i64::MIN, 10, "C4")],
        }));
        song.tracks.push(track);
        let errors: Vec<String> = validate(&ProjectFile::new(song))
            .into_iter()
            .filter(|issue| issue.severity == Severity::Error)
            .map(|issue| issue.path)
            .collect();
        assert!(errors.contains(&String::from("song.tracks[0].slots[0]")));
        assert!(errors.contains(&String::from("song.tracks[0].slots[2]")));
        assert!(!errors.contains(&String::from("song.tracks[0].slots[1]")));
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
    fn catches_unsorted_marks_and_duplicate_lyrics() {
        use crate::song::{Articulation, LyricEvent, Mark, MarkKind};
        let mut song = Song::new("x", 120.0);
        song.marks.push(Mark {
            tick: 960,
            kind: MarkKind::Articulation(Articulation::Accent),
        });
        song.marks.push(Mark {
            tick: 0,
            kind: MarkKind::Articulation(Articulation::Fermata),
        });
        song.lyrics.push(LyricEvent {
            tick: 0,
            text: String::from("a"),
        });
        song.lyrics.push(LyricEvent {
            tick: 0,
            text: String::from("b"),
        });
        let issues = validate(&ProjectFile::new(song));
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("marks are not sorted"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("two lyrics"))
        );
    }

    #[test]
    fn catches_invalid_audio_take_geometry() {
        use crate::song::AudioRef;
        let mut song = Song::new("x", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "t");
        track.takes.push(AudioRef {
            path: String::from("take.wav"),
            start: -1,
            length: 0,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let issues = validate(&ProjectFile::new(song));
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("a take needs")),
            "{issues:?}"
        );
    }

    #[test]
    fn catches_invalid_automation_points() {
        use crate::song::{Automation, AutomationPoint, AutomationTarget};
        let mut song = Song::new("x", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "t");
        track.automation.push(Automation {
            target: AutomationTarget::Volume,
            points: vec![
                AutomationPoint {
                    tick: 960,
                    value: 100.0,
                },
                AutomationPoint {
                    tick: 0,
                    value: 200.0,
                },
            ],
        });
        song.tracks.push(track);
        let issues = validate(&ProjectFile::new(song));
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("not sorted"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("outside 0..=127"))
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
