//! Clip grid: scenes as rows, tracks as columns.
//!
//! The grid is derived from the timeline: cutting a song into scenes of N bars
//! gives one row per scene, one column per track, and a slot that names the
//! clip covering that scene's start. It is a view over the existing clip
//! regions, not a second copy of the notes.

use music_core::Tick;
use project::song::Song;
use project::{ClipId, TrackId};

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
        let scene = Scene {
            name: format!("Scene {}", index + 1),
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
