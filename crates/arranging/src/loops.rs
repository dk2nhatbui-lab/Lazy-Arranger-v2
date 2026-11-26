//! Loops view: each track holds one clip per scene, and each slot can be
//! launched on its own, like the clip launcher in the C++ app.
//!
//! Slot clips own their notes (starts measured from the clip start) and loop
//! over their length. Every edit here is one undoable [`Command::SetLoops`].

use std::collections::BTreeMap;

use app_commands::{Command, EditSession};
use music_core::Tick;
use project::TrackId;
use project::song::{Note, SlotClip, Song};

use crate::ArrangingError;

/// Most scenes "From Timeline" makes, as in the C++ app.
pub const MAX_SCENES: usize = 128;

/// Bar choices offered by "From Timeline".
pub const SCENE_BARS: [u32; 5] = [1, 2, 4, 8, 16];

/// Ticks in `bars` bars of the song's meter.
#[must_use]
pub fn bars_to_ticks(song: &Song, bars: u32) -> Tick {
    song.time_signature.bar_ticks().max(1) * Tick::from(bars.max(1))
}

/// Cuts the timeline into scenes of `scene_length` ticks and fills each MIDI
/// track's slots with the notes it plays there. Scene names become
/// "Bars 1-4", "Bars 5-8" and so on. Returns the number of scenes.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn fill_from_timeline(
    session: &mut EditSession,
    scene_length: Tick,
) -> Result<usize, ArrangingError> {
    if scene_length <= 0 {
        return Ok(0);
    }
    let song = session.song();
    let end = song.end_tick();
    let scenes = usize::try_from((end + scene_length - 1) / scene_length)
        .unwrap_or(MAX_SCENES)
        .min(MAX_SCENES);
    if scenes == 0 {
        return Ok(0);
    }
    let bar = song.time_signature.bar_ticks().max(1);
    let mut names = song.scene_names.clone();
    if names.len() < scenes {
        names.resize(scenes, String::new());
    }
    for (index, name) in names.iter_mut().enumerate().take(scenes) {
        let from = index as Tick * scene_length;
        let to = from + scene_length;
        *name = format!("Bars {}-{}", from / bar + 1, (to - 1) / bar + 1);
    }
    let mut tracks = Vec::new();
    for track in &song.tracks {
        if track.audio.is_some() {
            continue;
        }
        let mut parts: Vec<Vec<Note>> = vec![Vec::new(); scenes];
        for note in &track.notes {
            let Ok(scene) = usize::try_from(note.start / scene_length) else {
                continue;
            };
            let Some(part) = parts.get_mut(scene) else {
                continue;
            };
            let scene_start = scene as Tick * scene_length;
            let mut copy = note.clone();
            copy.start = note.start - scene_start;
            copy.duration = note
                .duration
                .min(scene_start + scene_length - note.start)
                .max(1);
            part.push(copy);
        }
        let mut slots = track.slots.clone();
        if slots.len() < scenes {
            slots.resize(scenes, None);
        }
        for (slot, notes) in slots.iter_mut().zip(parts) {
            *slot = (!notes.is_empty()).then(|| SlotClip {
                name: track.name.clone(),
                length: scene_length,
                notes,
            });
        }
        tracks.push((track.id, slots));
    }
    session.apply(Command::SetLoops {
        tracks,
        names: Some(names),
    })?;
    Ok(scenes)
}

/// Puts `clip` in one slot, or empties it with `None`.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn set_slot(
    session: &mut EditSession,
    track: TrackId,
    scene: usize,
    clip: Option<SlotClip>,
) -> Result<(), ArrangingError> {
    let Some(entry) = session.song().tracks.iter().find(|entry| entry.id == track) else {
        return Ok(());
    };
    let mut slots = entry.slots.clone();
    if slots.len() <= scene {
        if clip.is_none() {
            return Ok(());
        }
        slots.resize(scene + 1, None);
    }
    if slots[scene] == clip {
        return Ok(());
    }
    slots[scene] = clip;
    session.apply(Command::SetLoops {
        tracks: vec![(track, slots)],
        names: None,
    })?;
    Ok(())
}

/// Makes an empty clip of `length` ticks in one slot, named after the track.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn new_slot(
    session: &mut EditSession,
    track: TrackId,
    scene: usize,
    length: Tick,
) -> Result<(), ArrangingError> {
    let Some(entry) = session.song().tracks.iter().find(|entry| entry.id == track) else {
        return Ok(());
    };
    let clip = SlotClip {
        name: entry.name.clone(),
        length: length.max(1),
        notes: Vec::new(),
    };
    set_slot(session, track, scene, Some(clip))
}

/// Changes a slot clip's loop length. Notes past the new end are kept, so a
/// longer length brings them back.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn set_slot_length(
    session: &mut EditSession,
    track: TrackId,
    scene: usize,
    length: Tick,
) -> Result<(), ArrangingError> {
    let Some(mut clip) = slot(session.song(), track, scene).cloned() else {
        return Ok(());
    };
    clip.length = length.max(1);
    set_slot(session, track, scene, Some(clip))
}

/// Renames a slot clip.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn rename_slot(
    session: &mut EditSession,
    track: TrackId,
    scene: usize,
    name: &str,
) -> Result<(), ArrangingError> {
    let Some(mut clip) = slot(session.song(), track, scene).cloned() else {
        return Ok(());
    };
    clip.name = name.trim().to_string();
    set_slot(session, track, scene, Some(clip))
}

/// Renames a scene.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn rename_scene(
    session: &mut EditSession,
    scene: usize,
    name: &str,
) -> Result<(), ArrangingError> {
    let mut names = session.song().scene_names.clone();
    if names.len() <= scene {
        names.resize(scene + 1, String::new());
    }
    names[scene] = name.trim().to_string();
    while names.last().is_some_and(String::is_empty) {
        names.pop();
    }
    session.apply(Command::SetSceneNames { names })?;
    Ok(())
}

/// Replaces the notes of a slot clip, for the piano roll.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn set_slot_notes(
    session: &mut EditSession,
    track: TrackId,
    scene: usize,
    mut notes: Vec<Note>,
) -> Result<(), ArrangingError> {
    let Some(mut clip) = slot(session.song(), track, scene).cloned() else {
        return Ok(());
    };
    notes.sort_by_key(|note| (note.start, note.pitch.as_u8()));
    clip.notes = notes;
    set_slot(session, track, scene, Some(clip))
}

/// The clip in one slot.
#[must_use]
pub fn slot(song: &Song, track: TrackId, scene: usize) -> Option<&SlotClip> {
    song.tracks
        .iter()
        .find(|entry| entry.id == track)?
        .slot(scene)
}

/// One launch or stop in a jam. A stop returns the track to its timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    /// The track.
    pub track: TrackId,
    /// The scene launched, or `None` for back to the timeline.
    pub scene: Option<usize>,
    /// Song tick where it took effect.
    pub tick: Tick,
    /// Tick the loop counts from; usually `tick`, earlier when recording
    /// starts while a slot already plays.
    pub origin: Tick,
}

/// A track's notes with `changes` (sorted, all for this track): a launched
/// slot loops from its tick until the next change or `until`. Between
/// launches the track plays its timeline when `keep_timeline` is set, and is
/// silent otherwise.
fn jam_notes(
    timeline: &[Note],
    slots: &[Option<SlotClip>],
    changes: &[Change],
    until: Tick,
    keep_timeline: bool,
) -> (Vec<Note>, usize) {
    let mut notes = Vec::new();
    let mut looped = 0;
    let mut segments: Vec<(Tick, Tick, Tick, Option<&SlotClip>)> = Vec::new();
    let first = changes.first().map_or(Tick::MAX, |change| change.tick);
    segments.push((Tick::MIN, first, Tick::MIN, None));
    for (index, change) in changes.iter().enumerate() {
        let stop = changes.get(index + 1).map_or(Tick::MAX, |next| next.tick);
        let clip = change
            .scene
            .and_then(|scene| slots.get(scene).and_then(Option::as_ref));
        segments.push((change.tick, stop, change.origin.min(change.tick), clip));
    }
    for (from, to, origin, clip) in segments {
        if from >= to {
            continue;
        }
        match clip {
            None if !keep_timeline => {}
            None => {
                for note in timeline {
                    if note.start >= from && note.start < to {
                        let mut copy = note.clone();
                        copy.duration = copy.duration.min(to.saturating_sub(note.start)).max(1);
                        notes.push(copy);
                    }
                }
            }
            Some(clip) => {
                let length = clip.length.max(1);
                let stop = to.min(until);
                let mut base = from - (from - origin) % length;
                while base < stop {
                    for note in &clip.notes {
                        let start = base + note.start;
                        if note.start >= length || start < from || start >= stop {
                            continue;
                        }
                        let mut copy = note.clone();
                        copy.start = start;
                        copy.duration = note
                            .duration
                            .min(length - note.start)
                            .min(stop - start)
                            .max(1);
                        notes.push(copy);
                        looped += 1;
                    }
                    base += length;
                }
            }
        }
    }
    notes.sort_by_key(|note| (note.start, note.pitch.as_u8()));
    for (index, note) in notes.iter_mut().enumerate() {
        if let Ok(id) = project::NoteId::new(index as u32 + 1) {
            note.id = id;
        }
    }
    (notes, looped)
}

fn by_track(changes: &[Change]) -> BTreeMap<TrackId, Vec<Change>> {
    let mut map: BTreeMap<TrackId, Vec<Change>> = BTreeMap::new();
    for change in changes {
        map.entry(change.track).or_default().push(*change);
    }
    for list in map.values_mut() {
        list.sort_by_key(|change| change.tick);
    }
    map
}

/// The song as heard while Loops is active: launched slots loop up to
/// `until`, and the timeline is silent on every track, as in the C++ app.
#[must_use]
pub fn jam_song(song: &Song, changes: &[Change], until: Tick) -> Song {
    let mut out = song.clone();
    let lists = by_track(changes);
    for entry in &mut out.tracks {
        entry.notes = match lists.get(&entry.id) {
            Some(list) => jam_notes(&entry.notes, &entry.slots, list, until, false).0,
            None => Vec::new(),
        };
    }
    out
}

/// Writes a jam to the timeline: launched slots are copied in, looped, from
/// each launch to the next change or `end`. Returns how many notes came
/// from slots.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn record_to_timeline(
    session: &mut EditSession,
    changes: &[Change],
    end: Tick,
) -> Result<usize, ArrangingError> {
    let mut written = 0;
    let mut replaced = Vec::new();
    for (track, list) in by_track(changes) {
        let Some(entry) = session.song().tracks.iter().find(|entry| entry.id == track) else {
            continue;
        };
        let (notes, looped) = jam_notes(&entry.notes, &entry.slots, &list, end, true);
        written += looped;
        replaced.push((track, notes));
    }
    if written > 0 {
        session.apply(Command::ReplaceTrackNotesMany { tracks: replaced })?;
    }
    Ok(written)
}

/// Per-track slot launching, quantised to a grid, like the C++ engine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launcher {
    /// Scene each track plays now. A missing track is stopped.
    pub playing: BTreeMap<TrackId, usize>,
    /// Waiting changes: `Some(scene)` launches, `None` stops.
    pub pending: BTreeMap<TrackId, Option<usize>>,
    /// Tick where each playing track's clip started.
    pub since: BTreeMap<TrackId, Tick>,
    /// Launches and stops that took effect, oldest first.
    pub changes: Vec<Change>,
}

impl Launcher {
    /// True while any slot plays or waits; the timeline is silent then.
    #[must_use]
    pub fn active(&self) -> bool {
        !self.playing.is_empty() || !self.pending.is_empty()
    }

    /// Queues a launch for the next grid line.
    pub fn launch(&mut self, track: TrackId, scene: usize) {
        self.pending.insert(track, Some(scene));
    }

    /// Queues a stop for a track that plays or waits.
    pub fn stop(&mut self, track: TrackId) {
        if self.playing.contains_key(&track) {
            self.pending.insert(track, None);
        } else {
            self.pending.remove(&track);
        }
    }

    /// A press on a slot, as in the C++ app: cancels a waiting launch of
    /// that slot, stops (or un-stops) a playing one, and launches otherwise.
    pub fn toggle(&mut self, track: TrackId, scene: usize) {
        let pending = self.pending.get(&track).copied();
        if pending == Some(Some(scene)) {
            self.pending.remove(&track);
        } else if self.playing.get(&track) == Some(&scene) {
            if pending == Some(None) {
                self.pending.remove(&track);
            } else {
                self.pending.insert(track, None);
            }
        } else {
            self.launch(track, scene);
        }
    }

    /// Launches a scene: tracks with a clip there launch it, the others stop.
    pub fn launch_scene(&mut self, song: &Song, scene: usize) {
        for track in &song.tracks {
            if track.slot(scene).is_some() {
                self.launch(track.id, scene);
            } else {
                self.stop(track.id);
            }
        }
    }

    /// Stops every track at the next grid line.
    pub fn stop_all(&mut self) {
        let tracks: Vec<TrackId> = self
            .playing
            .keys()
            .chain(self.pending.keys())
            .copied()
            .collect();
        for track in tracks {
            self.stop(track);
        }
    }

    /// Commits the waiting changes at `tick`; true when anything changed.
    pub fn commit(&mut self, tick: Tick) -> bool {
        if self.pending.is_empty() {
            return false;
        }
        for (track, scene) in std::mem::take(&mut self.pending) {
            match scene {
                Some(scene) => {
                    self.playing.insert(track, scene);
                    self.since.insert(track, tick);
                }
                None => {
                    self.playing.remove(&track);
                    self.since.remove(&track);
                }
            }
            self.changes.push(Change {
                track,
                scene,
                tick,
                origin: tick,
            });
        }
        true
    }

    /// What plays now, as launches at the tick each clip started.
    #[must_use]
    pub fn now_playing(&self) -> Vec<Change> {
        self.playing
            .iter()
            .map(|(track, scene)| {
                let since = self.since.get(track).copied().unwrap_or(0);
                Change {
                    track: *track,
                    scene: Some(*scene),
                    tick: since,
                    origin: since,
                }
            })
            .collect()
    }

    /// Forgets what plays and waits, keeping the change log.
    pub fn reset(&mut self) {
        self.playing.clear();
        self.pending.clear();
        self.since.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::NoteId;
    use project::song::{ProjectFile, Track};

    fn note(start: Tick, duration: Tick) -> Note {
        Note {
            id: NoteId::new(1).expect("valid"),
            start,
            duration,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        }
    }

    fn session() -> EditSession {
        let mut song = Song::new("Loops", 120.0);
        let mut piano = Track::new(TrackId::new(1).expect("valid"), "Piano");
        piano.notes = vec![note(0, PPQ), note(3840, PPQ), note(3840 * 5, 3840 * 2)];
        for (index, note) in piano.notes.iter_mut().enumerate() {
            note.id = NoteId::new(index as u32 + 1).expect("valid");
        }
        let bass = Track::new(TrackId::new(2).expect("valid"), "Bass");
        song.tracks.push(piano);
        song.tracks.push(bass);
        EditSession::new(ProjectFile::new(song)).expect("valid")
    }

    #[test]
    fn from_timeline_cuts_each_track_into_scene_clips() {
        let mut session = session();
        let length = bars_to_ticks(session.song(), 4);
        let scenes = fill_from_timeline(&mut session, length).expect("fill");
        assert_eq!(scenes, 2, "seven bars of notes make two four-bar scenes");
        let song = session.song();
        assert_eq!(song.scene_name(0), "Bars 1-4");
        assert_eq!(song.scene_name(1), "Bars 5-8");
        let first = slot(song, TrackId::new(1).expect("valid"), 0).expect("slot");
        assert_eq!(first.name, "Piano");
        assert_eq!(first.length, length);
        assert_eq!(first.notes.len(), 2);
        assert_eq!(first.notes[1].start, 3840);
        let second = slot(song, TrackId::new(1).expect("valid"), 1).expect("slot");
        assert_eq!(second.notes[0].start, 3840);
        assert_eq!(second.notes[0].duration, 3840 * 2);
        assert!(slot(song, TrackId::new(2).expect("valid"), 0).is_none());
        session.undo().expect("undo");
        assert!(session.song().tracks[0].slots.is_empty());
        assert!(session.song().scene_names.is_empty());
    }

    #[test]
    fn slots_can_be_made_renamed_resized_and_cleared() {
        let mut session = session();
        let bass = TrackId::new(2).expect("valid");
        new_slot(&mut session, bass, 3, 3840).expect("new");
        assert_eq!(session.song().tracks[1].slots.len(), 4);
        rename_slot(&mut session, bass, 3, " Groove ").expect("rename");
        set_slot_length(&mut session, bass, 3, 7680).expect("length");
        let clip = slot(session.song(), bass, 3).expect("slot");
        assert_eq!((clip.name.as_str(), clip.length), ("Groove", 7680));
        set_slot(&mut session, bass, 3, None).expect("clear");
        assert!(
            session.song().tracks[1].slots.is_empty(),
            "trailing empties trimmed"
        );
        rename_scene(&mut session, 2, "Chorus").expect("scene");
        assert_eq!(session.song().scene_name(2), "Chorus");
        assert_eq!(session.song().scene_name(0), "Scene 1");
    }

    #[test]
    fn a_launched_slot_replaces_the_timeline_from_its_tick() {
        let mut session = session();
        let piano = TrackId::new(1).expect("valid");
        new_slot(&mut session, piano, 0, 1920).expect("new");
        set_slot_notes(&mut session, piano, 0, vec![note(0, 480)]).expect("notes");
        let mut changes = vec![Change {
            track: piano,
            scene: Some(0),
            tick: 3840,
            origin: 3840,
        }];
        let jam = jam_song(session.song(), &changes, 3840 + 1920 * 3);
        let starts: Vec<Tick> = jam.tracks[0].notes.iter().map(|note| note.start).collect();
        assert_eq!(starts, vec![3840, 5760, 7680], "the timeline is silent");
        assert!(jam.tracks[1].notes.is_empty());
        changes.push(Change {
            track: piano,
            scene: None,
            tick: 7680,
            origin: 7680,
        });
        let back = jam_song(session.song(), &changes, 30000);
        let starts: Vec<Tick> = back.tracks[0].notes.iter().map(|note| note.start).collect();
        assert_eq!(starts, vec![3840, 5760], "silent after the stop");
    }

    #[test]
    fn the_launcher_waits_for_the_grid_and_toggles_like_the_cpp_app() {
        let piano = TrackId::new(1).expect("valid");
        let bass = TrackId::new(2).expect("valid");
        let mut launcher = Launcher::default();
        launcher.toggle(piano, 0);
        assert!(launcher.active());
        assert!(launcher.playing.is_empty());
        launcher.toggle(piano, 0);
        assert!(
            !launcher.active(),
            "a second press cancels the waiting launch"
        );
        launcher.toggle(piano, 0);
        assert!(launcher.commit(3840));
        assert_eq!(launcher.playing.get(&piano), Some(&0));
        launcher.toggle(piano, 0);
        assert_eq!(launcher.pending.get(&piano), Some(&None), "stops soon");
        launcher.toggle(piano, 0);
        assert!(
            launcher.pending.is_empty(),
            "pressing again keeps it playing"
        );
        launcher.launch(bass, 1);
        launcher.stop_all();
        assert!(launcher.commit(7680));
        assert!(!launcher.active());
        assert_eq!(launcher.changes.len(), 2, "the bass launch was taken back");
        let mut session = session();
        new_slot(&mut session, bass, 2, 3840).expect("new");
        launcher.launch_scene(session.song(), 2);
        assert_eq!(launcher.pending.get(&bass), Some(&Some(2)));
        assert!(
            !launcher.pending.contains_key(&piano),
            "not playing, nothing to stop"
        );
    }

    #[test]
    fn recording_writes_the_jam_to_the_timeline() {
        let mut session = session();
        let piano = TrackId::new(1).expect("valid");
        new_slot(&mut session, piano, 0, 1920).expect("new");
        set_slot_notes(&mut session, piano, 0, vec![note(0, 480)]).expect("notes");
        let changes = [
            Change {
                track: piano,
                scene: Some(0),
                tick: 3840,
                origin: 3840,
            },
            Change {
                track: piano,
                scene: None,
                tick: 7680,
                origin: 7680,
            },
        ];
        let written = record_to_timeline(&mut session, &changes, 15360).expect("record");
        assert_eq!(written, 2);
        let starts: Vec<Tick> = session.song().tracks[0]
            .notes
            .iter()
            .map(|note| note.start)
            .collect();
        assert_eq!(starts, vec![0, 3840, 5760, 3840 * 5]);
        session.undo().expect("undo");
        assert_eq!(session.song().tracks[0].notes.len(), 3);
    }
}
