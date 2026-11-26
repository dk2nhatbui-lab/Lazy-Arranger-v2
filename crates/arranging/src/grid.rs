//! Clip grid: scenes as rows, tracks as columns.
//!
//! The grid is derived from the timeline: cutting a song into scenes of N bars
//! gives one row per scene, one column per track, and a slot that names the
//! clip covering that scene's start. It is a view over the existing clip
//! regions, not a second copy of the notes.

use app_commands::{Command, EditSession};
use music_core::Tick;
use project::song::Note;
use project::song::Song;
use project::{ClipId, TrackId};

use crate::ArrangingError;

/// One scene, a fixed span of bars.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    /// Display name.
    pub name: String,
    /// First tick.
    pub start: Tick,
    /// Length in ticks.
    pub length: Tick,
}

/// One cell: the clip that covers the scene start on a track, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    /// Track column.
    pub track: TrackId,
    /// Clip covering the scene start.
    pub clip: Option<ClipId>,
}

/// One scene row with a slot per track.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The scene.
    pub scene: Scene,
    /// One slot per track, in the same order as [`Grid::tracks`].
    pub slots: Vec<Slot>,
}

/// The clip grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    /// Track columns, in song order.
    pub tracks: Vec<TrackId>,
    /// Scene rows.
    pub rows: Vec<Row>,
}

/// Cuts the song into a grid of `bars_per_scene` bars.
#[must_use]
pub fn from_timeline(song: &Song, bars_per_scene: usize) -> Grid {
    let bar = song.time_signature.bar_ticks().max(1);
    let scene_length = bar * (bars_per_scene.max(1) as Tick);
    let tracks: Vec<TrackId> = song.tracks.iter().map(|track| track.id).collect();
    let clip_end = song
        .tracks
        .iter()
        .flat_map(|track| track.clips.iter())
        .map(|clip| clip.end())
        .max()
        .unwrap_or(0);
    let end = song.end_tick().max(scene_length).max(clip_end);
    let scenes = usize::try_from((end + scene_length - 1) / scene_length).unwrap_or(1);
    let mut rows = Vec::with_capacity(scenes);
    for index in 0..scenes {
        let start = (index as Tick) * scene_length;
        let name = song
            .scene_names
            .get(index)
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("Scene {}", index + 1));
        let scene = Scene {
            name,
            start,
            length: scene_length,
        };
        let slots = song
            .tracks
            .iter()
            .map(|track| Slot {
                track: track.id,
                clip: track
                    .clips
                    .iter()
                    .find(|clip| clip.contains(start))
                    .map(|clip| clip.id),
            })
            .collect();
        rows.push(Row { scene, slots });
    }
    Grid { tracks, rows }
}

/// The scene index that contains a tick.
#[must_use]
pub fn scene_at(grid: &Grid, tick: Tick) -> Option<usize> {
    grid.rows
        .iter()
        .position(|row| tick >= row.scene.start && tick < row.scene.start + row.scene.length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::PPQ;
    use project::NoteId;
    use project::song::{Clip, Note, Track};

    fn song() -> Song {
        let mut song = Song::new("Grid", 120.0);
        let mut first = Track::new(TrackId::new(1).expect("valid"), "Piano");
        first.clips.push(Clip {
            id: ClipId::new(1).expect("valid"),
            name: String::from("A"),
            start: 0,
            length: 7680,
        });
        first.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: music_core::Pitch::from_name("C4").expect("valid"),
            velocity: music_core::Velocity::new(100).expect("valid"),
            channel: music_core::Channel::first(),
        });
        let mut second = Track::new(TrackId::new(2).expect("valid"), "Bass");
        second.clips.push(Clip {
            id: ClipId::new(1).expect("valid"),
            name: String::from("B"),
            start: 3840,
            length: 3840,
        });
        song.tracks.push(first);
        song.tracks.push(second);
        song
    }

    #[test]
    fn a_song_cuts_into_scenes_of_n_bars() {
        let grid = from_timeline(&song(), 1);
        assert_eq!(grid.tracks.len(), 2);
        assert_eq!(grid.rows.len(), 2, "two bars of clips at one bar per scene");
        assert_eq!(grid.rows[0].scene.start, 0);
        assert_eq!(grid.rows[1].scene.start, 3840);
        assert_eq!(grid.rows[0].slots.len(), 2);
        assert_eq!(
            grid.rows[0].slots[0].clip,
            Some(ClipId::new(1).expect("valid")),
            "piano clip covers scene one"
        );
        assert_eq!(grid.rows[0].slots[1].clip, None, "bass clip starts later");
        assert_eq!(
            grid.rows[1].slots[1].clip,
            Some(ClipId::new(1).expect("valid"))
        );
    }

    #[test]
    fn scenes_are_found_by_tick() {
        let grid = from_timeline(&song(), 1);
        assert_eq!(scene_at(&grid, 0), Some(0));
        assert_eq!(scene_at(&grid, 3839), Some(0));
        assert_eq!(scene_at(&grid, 3840), Some(1));
        assert_eq!(scene_at(&grid, 7679), Some(1));
        assert_eq!(scene_at(&grid, 7680), None);
    }

    #[test]
    fn an_empty_song_still_has_one_scene() {
        let song = Song::new("Empty", 120.0);
        let grid = from_timeline(&song, 4);
        assert_eq!(grid.rows.len(), 1);
        assert!(grid.rows[0].slots.is_empty());
    }
}

/// Launch state: what plays, what waits for the next bar, and what has played.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launcher {
    /// Scene currently playing.
    pub current: Option<usize>,
    /// Scene that starts at the next bar boundary.
    pub waiting: Option<usize>,
    /// True between start and stop.
    pub playing: bool,
    /// Scenes that started, with the tick where each began.
    pub log: Vec<(usize, Tick)>,
}

impl Launcher {
    /// Starts or queues a scene. While playing, the change waits for a bar.
    pub fn launch(&mut self, scene: usize) {
        if self.playing {
            self.waiting = Some(scene);
        } else {
            self.playing = true;
            self.current = Some(scene);
            self.waiting = None;
            self.log.push((scene, 0));
        }
    }

    /// Commits a waiting scene at a bar boundary; returns it when it changed.
    pub fn advance(&mut self, bar_tick: Tick) -> Option<usize> {
        if !self.playing {
            return None;
        }
        let scene = self.waiting.take()?;
        self.current = Some(scene);
        self.log.push((scene, bar_tick));
        Some(scene)
    }

    /// Cancels the waiting scene.
    pub fn cancel_waiting(&mut self) {
        self.waiting = None;
    }

    /// Stops playback and clears the queue, keeping the log.
    pub fn stop(&mut self) {
        self.playing = false;
        self.waiting = None;
        self.current = None;
    }
}

/// Writes the scenes a launcher played to the timeline.
///
/// Each logged scene copies its slotted clips (notes and clip regions) to the
/// tick where it started. Notes append; clip identifiers are fresh per track.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when an edit is rejected.
pub fn record_to_timeline(
    session: &mut EditSession,
    grid: &Grid,
    log: &[(usize, Tick)],
) -> Result<usize, ArrangingError> {
    // Every edit below lands as one undo step, or none if any fails.
    let mut batch = Vec::new();
    use std::collections::BTreeMap;

    let mut additions: BTreeMap<TrackId, Vec<Note>> = BTreeMap::new();
    let mut clips: BTreeMap<TrackId, Vec<project::song::Clip>> = BTreeMap::new();
    let mut copied = 0;
    for (scene_index, target) in log {
        let Some(row) = grid.rows.get(*scene_index) else {
            continue;
        };
        for slot in &row.slots {
            let Some(clip_id) = slot.clip else {
                continue;
            };
            let Some(track) = session
                .song()
                .tracks
                .iter()
                .find(|track| track.id == slot.track)
            else {
                continue;
            };
            let Some(clip) = track.clips.iter().find(|clip| clip.id == clip_id) else {
                continue;
            };
            let offset = *target - row.scene.start;
            for note in &track.notes {
                if note.start < clip.start || note.start >= clip.end() {
                    continue;
                }
                let mut copy = note.clone();
                copy.start = note.start + offset;
                additions.entry(track.id).or_default().push(copy);
                copied += 1;
            }
            let next = track
                .clips
                .iter()
                .map(|entry| entry.id.get())
                .max()
                .unwrap_or(0)
                .saturating_add(1)
                + u32::try_from(clips.get(&track.id).map_or(0, Vec::len)).unwrap_or(0);
            if let Ok(id) = ClipId::new(next) {
                clips
                    .entry(track.id)
                    .or_default()
                    .push(project::song::Clip {
                        id,
                        name: format!("{} take", clip.name),
                        start: clip.start + offset,
                        length: clip.length,
                    });
            }
        }
    }

    for (track, notes) in &additions {
        let Some(entry) = session
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == *track)
        else {
            continue;
        };
        let mut merged = entry.notes.clone();
        merged.extend(notes.iter().cloned());
        merged.sort_by_key(|note| note.start);
        for (index, note) in merged.iter_mut().enumerate() {
            if let Ok(id) = project::NoteId::new(u32::try_from(index + 1).unwrap_or(u32::MAX)) {
                note.id = id;
            }
        }
        batch.push(Command::ReplaceTrackNotes {
            track: *track,
            notes: merged,
        });
    }
    for (track, regions) in &clips {
        let Some(entry) = session
            .song()
            .tracks
            .iter()
            .find(|entry| entry.id == *track)
        else {
            continue;
        };
        let mut merged = entry.clips.clone();
        merged.extend(regions.iter().cloned());
        merged.sort_by_key(|clip| clip.start);
        batch.push(Command::SetClips {
            track: *track,
            clips: merged,
        });
    }
    session.apply(Command::Batch {
        label: String::from("record to timeline"),
        commands: batch,
    })?;
    Ok(copied)
}

#[cfg(test)]
mod launcher_tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::NoteId;
    use project::song::{Clip, ProjectFile, Song, Track};

    fn launcher() -> Launcher {
        Launcher::default()
    }

    #[test]
    fn launching_while_stopped_starts_and_logs_at_zero() {
        let mut state = launcher();
        state.launch(2);
        assert!(state.playing);
        assert_eq!(state.current, Some(2));
        assert_eq!(state.log, vec![(2, 0)]);
    }

    #[test]
    fn launching_while_playing_waits_for_the_next_bar() {
        let mut state = launcher();
        state.launch(0);
        state.launch(1);
        assert_eq!(state.current, Some(0));
        assert_eq!(state.waiting, Some(1));
        assert_eq!(state.advance(3840), Some(1));
        assert_eq!(state.current, Some(1));
        assert_eq!(state.log, vec![(0, 0), (1, 3840)]);
        assert_eq!(state.advance(7680), None, "nothing waiting");
        state.launch(3);
        state.cancel_waiting();
        assert_eq!(state.waiting, None);
        state.stop();
        assert!(!state.playing);
        assert_eq!(state.log.len(), 2);
    }

    #[test]
    fn recording_writes_the_played_scene_to_the_timeline() {
        let mut song = Song::new("Record", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.clips.push(Clip {
            id: ClipId::new(1).expect("valid"),
            name: String::from("A"),
            start: 0,
            length: 3840,
        });
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        let grid = from_timeline(&song, 1);
        let mut session = EditSession::new(ProjectFile::new(song)).expect("valid");
        let copied = record_to_timeline(&mut session, &grid, &[(0, 7680)]).expect("record");
        assert_eq!(copied, 1);
        let track = &session.song().tracks[0];
        assert_eq!(track.notes.len(), 2);
        assert_eq!(track.notes[1].start, 7680);
        assert_eq!(track.clips.len(), 2);
        assert_eq!(track.clips[1].start, 7680);
        // Notes and clips come back in one undo step.
        session.undo().expect("undo record");
        assert_eq!(session.song().tracks[0].notes.len(), 1);
        assert_eq!(session.song().tracks[0].clips.len(), 1);
    }
}

/// Changes the length of the clip in one slot.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn set_slot_length(
    session: &mut EditSession,
    grid: &Grid,
    scene_index: usize,
    track: TrackId,
    length: Tick,
) -> Result<(), ArrangingError> {
    let Some(clip) = slot_clip(grid, scene_index, track) else {
        return Ok(());
    };
    session.resize_clip(track, clip, length)?;
    Ok(())
}

/// Removes the clip from one slot, keeping its notes.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn clear_slot(
    session: &mut EditSession,
    grid: &Grid,
    scene_index: usize,
    track: TrackId,
) -> Result<(), ArrangingError> {
    let Some(clip) = slot_clip(grid, scene_index, track) else {
        return Ok(());
    };
    session.remove_clip(track, clip)?;
    Ok(())
}

/// Copies one slot's clip and notes to `target`, as a new clip.
///
/// Returns how many notes were copied.
///
/// # Errors
///
/// Returns [`ArrangingError::Command`] when the edit is rejected.
pub fn copy_slot(
    session: &mut EditSession,
    grid: &Grid,
    scene_index: usize,
    track: TrackId,
    target: Tick,
) -> Result<usize, ArrangingError> {
    let Some(clip_id) = slot_clip(grid, scene_index, track) else {
        return Ok(0);
    };
    let Some(entry) = session.song().tracks.iter().find(|entry| entry.id == track) else {
        return Ok(0);
    };
    let Some(clip) = entry.clips.iter().find(|clip| clip.id == clip_id) else {
        return Ok(0);
    };
    let offset = target - clip.start;
    let copies: Vec<Note> = entry
        .notes
        .iter()
        .filter(|note| note.start >= clip.start && note.start < clip.end())
        .map(|note| {
            let mut copy = note.clone();
            copy.start = note.start + offset;
            copy
        })
        .collect();
    let mut notes = entry.notes.clone();
    notes.extend(copies.iter().cloned());
    let existing_clips = entry.clips.clone();
    let next = existing_clips
        .iter()
        .map(|clip| clip.id.get())
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let clip_name = clip.name.clone();
    let clip_length = clip.length;
    notes.sort_by_key(|note| note.start);
    for (index, note) in notes.iter_mut().enumerate() {
        if let Ok(id) = project::NoteId::new(index as u32 + 1) {
            note.id = id;
        }
    }
    session.apply(Command::ReplaceTrackNotes { track, notes })?;
    if let Ok(id) = ClipId::new(next) {
        let mut clips = existing_clips;
        clips.push(project::song::Clip {
            id,
            name: format!("{clip_name} copy"),
            start: target,
            length: clip_length,
        });
        clips.sort_by_key(|clip| clip.start);
        session.apply(Command::SetClips { track, clips })?;
    }
    Ok(copies.len())
}

/// The clip identifier in one slot, when the slot is filled.
#[must_use]
pub fn slot_clip(grid: &Grid, scene_index: usize, track: TrackId) -> Option<ClipId> {
    grid.rows
        .get(scene_index)?
        .slots
        .iter()
        .find(|slot| slot.track == track)?
        .clip
}

#[cfg(test)]
mod slot_tests {
    use super::*;
    use music_core::{Channel, PPQ, Pitch, Velocity};
    use project::NoteId;
    use project::song::{Clip, ProjectFile, Song, Track};

    fn session_with_clip() -> (EditSession, Grid) {
        let mut song = Song::new("Slots", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.clips.push(Clip {
            id: ClipId::new(1).expect("valid"),
            name: String::from("A"),
            start: 0,
            length: 3840,
        });
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: PPQ,
            pitch: Pitch::from_name("C4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        let grid = from_timeline(&song, 1);
        let session = EditSession::new(ProjectFile::new(song)).expect("valid");
        (session, grid)
    }

    #[test]
    fn a_slot_length_can_change_and_undo() {
        let (mut session, grid) = session_with_clip();
        let track = TrackId::new(1).expect("valid");
        set_slot_length(&mut session, &grid, 0, track, 1920).expect("resize");
        assert_eq!(session.song().tracks[0].clips[0].length, 1920);
        session.undo().expect("undo");
        assert_eq!(session.song().tracks[0].clips[0].length, 3840);
    }

    #[test]
    fn clearing_a_slot_keeps_its_notes() {
        let (mut session, grid) = session_with_clip();
        let track = TrackId::new(1).expect("valid");
        clear_slot(&mut session, &grid, 0, track).expect("clear");
        assert!(session.song().tracks[0].clips.is_empty());
        assert_eq!(session.song().tracks[0].notes.len(), 1);
    }

    #[test]
    fn copying_a_slot_duplicates_notes_and_clip() {
        let (mut session, grid) = session_with_clip();
        let track = TrackId::new(1).expect("valid");
        let copied = copy_slot(&mut session, &grid, 0, track, 3840).expect("copy");
        assert_eq!(copied, 1);
        let entry = &session.song().tracks[0];
        assert_eq!(entry.notes.len(), 2);
        assert_eq!(entry.notes[1].start, 3840);
        assert_eq!(entry.clips.len(), 2);
        assert_eq!(entry.clips[1].start, 3840);
        assert_eq!(entry.clips[1].name, "A copy");
        session.undo().expect("undo clips");
        session.undo().expect("undo notes");
        assert_eq!(session.song().tracks[0].notes.len(), 1);
    }

    #[test]
    fn empty_slots_are_no_ops() {
        let (mut session, grid) = session_with_clip();
        let empty = TrackId::new(9).expect("valid");
        assert!(slot_clip(&grid, 0, empty).is_none());
        assert!(set_slot_length(&mut session, &grid, 0, empty, 960).is_ok());
        assert!(clear_slot(&mut session, &grid, 0, empty).is_ok());
        assert_eq!(
            copy_slot(&mut session, &grid, 0, empty, 0).expect("copy"),
            0
        );
    }
}
