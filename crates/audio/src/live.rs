//! Live mixer playback.
//!
//! Playback copies from pre-rendered buffers, so a fader used to need a full
//! render before it was heard. Here most tracks are rendered once, without
//! their fader, pan, bus and master gain, into a buffer of their own (a
//! lane). The player sums the lanes with gains it reads while it plays, so a
//! fader, pan, mute or solo change is heard at the next audio buffer.
//!
//! A track stays in the fixed part of the render when a gain stage after it
//! is not linear or changes over time: volume, pan or send automation, a bus
//! with plug-ins, or master plug-ins. Changes that touch the fixed part still
//! need a new render.
//!
//! A live track with notes only (no recorded audio, no plug-ins) is not
//! rendered at all: its lane carries a [`SynthLane`] that plays the notes in
//! the audio callback, and a note edit swaps in a new lane.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use project::TrackId;
use project::song::{AutomationTarget, Bus, PluginSlot, Song, Track};

use crate::master::{self, Master};
use crate::synth::SynthLane;
use crate::{RenderedAudio, gains, soundfont};

/// A song rendered for live mixing.
#[derive(Clone, Debug, Default)]
pub struct LiveRender {
    /// Everything that is not live, already through its own faders.
    pub fixed: RenderedAudio,
    /// One buffer per live track, before its fader, at `fixed`'s length.
    /// Empty for a track played by a synth lane.
    pub lanes: Vec<Vec<f32>>,
    /// The synth lane of each live track that has one, in lane order.
    pub synths: Vec<Option<SynthLane>>,
    /// Frames a synth lane is delayed by; needed to build a replacement.
    pub synth_delay: usize,
    /// SoundFonts the synth lanes play, by path, to build replacements.
    pub fonts: Vec<(String, Arc<soundfont::SoundFont>)>,
    /// The track of each lane.
    pub tracks: Vec<TrackId>,
    /// Left and right gain of each lane when it was rendered.
    pub gains: Vec<(f32, f32)>,
    /// Linear master gain applied after the sum.
    pub master_gain: f32,
    /// Whether the soft limiter runs after the master gain.
    pub limiter: bool,
    /// Master chorus wet amount, 0..=1, run on the sum while playing.
    pub chorus: f32,
    /// Master reverb wet amount, 0..=1, run on the sum while playing.
    pub reverb: f32,
    /// True for a finished mix that plays back exactly as it is, with no
    /// master stage.
    pub finished: bool,
}

impl LiveRender {
    /// Wraps a finished mix that needs no live stages.
    #[must_use]
    pub fn from_audio(audio: RenderedAudio) -> Self {
        Self {
            fixed: audio,
            lanes: Vec::new(),
            synths: Vec::new(),
            synth_delay: 0,
            fonts: Vec::new(),
            tracks: Vec::new(),
            gains: Vec::new(),
            master_gain: 1.0,
            limiter: false,
            chorus: 0.0,
            reverb: 0.0,
            finished: true,
        }
    }

    /// Sample rate in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.fixed.sample_rate
    }

    /// Keeps the master settings for playback, which runs them on the sum
    /// of every part.
    pub fn apply_master(&mut self, settings: Master) {
        self.master_gain = master::gain(settings.gain_db);
        self.limiter = settings.limiter;
        self.chorus = settings.chorus.clamp(0.0, 1.0);
        self.reverb = settings.reverb.clamp(0.0, 1.0);
    }

    /// Renders every synth lane into its lane buffer.
    pub fn bake_synths(&mut self) {
        let len = self.fixed.samples.len();
        for (lane, synth) in self.lanes.iter_mut().zip(&mut self.synths) {
            if let Some(synth) = synth.take() {
                *lane = synth.render_all(len);
            }
        }
    }

    /// The finished mix with the gains it was rendered with.
    #[must_use]
    pub fn mixdown(&self) -> RenderedAudio {
        let mut samples = self.fixed.samples.clone();
        let mut baked = self.clone();
        baked.bake_synths();
        for (lane, (left, right)) in baked.lanes.iter().zip(&self.gains) {
            for (target, pair) in samples
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip(lane.as_chunks::<2>().0)
            {
                target[0] += pair[0] * left;
                target[1] += pair[1] * right;
            }
        }
        let mut audio = RenderedAudio {
            samples,
            sample_rate: self.fixed.sample_rate,
        };
        master::apply_effects(
            &mut audio,
            Master {
                gain_db: 0.0,
                limiter: false,
                reverb: self.reverb,
                chorus: self.chorus,
            },
        );
        let mut samples = audio.samples;
        for sample in &mut samples {
            *sample = master::finish(*sample * self.master_gain, self.limiter);
        }
        RenderedAudio {
            samples,
            sample_rate: self.fixed.sample_rate,
        }
    }
}

fn active(slots: &[PluginSlot]) -> bool {
    slots
        .iter()
        .any(|slot| !slot.bypassed && !slot.identifier.is_empty())
}

fn bus_index(song: &Song, id: Option<u8>) -> Option<usize> {
    id.and_then(|id| song.buses.iter().position(|bus| bus.id == id))
}

/// True when a bus is heard, following its mute and the bus solos.
#[must_use]
pub fn bus_audible(song: &Song, bus: &Bus) -> bool {
    let any_solo = song.buses.iter().any(|entry| entry.soloed);
    !(bus.muted || (any_solo && !bus.soloed))
}

/// Most tracks mixed live; the playback callback keeps their gain ramps on
/// the stack. Tracks past this count stay in the fixed part.
pub const MAX_LANES: usize = 256;

/// Which tracks can be mixed live, in track order.
#[must_use]
pub fn live_tracks(song: &Song) -> Vec<bool> {
    if active(&song.master_plugins) {
        return vec![false; song.tracks.len()];
    }
    let plain_bus = |id: Option<u8>| {
        bus_index(song, id).is_none_or(|index| !active(&song.buses[index].plugins))
    };
    let mut count = 0;
    song.tracks
        .iter()
        .map(|track| {
            let live = count < MAX_LANES
                && !track.automation.iter().any(|lane| {
                    matches!(
                        lane.target,
                        AutomationTarget::Volume | AutomationTarget::Pan | AutomationTarget::Send
                    )
                })
                && plain_bus(track.bus)
                && plain_bus(track.send);
            count += usize::from(live);
            live
        })
        .collect()
}

/// Buses whose sound is in the fixed part: they have plug-ins, or a fixed
/// track feeds them.
fn fixed_buses(song: &Song, live: &[bool]) -> Vec<bool> {
    let mut fixed: Vec<bool> = song.buses.iter().map(|bus| active(&bus.plugins)).collect();
    for (track, live) in song.tracks.iter().zip(live) {
        if *live {
            continue;
        }
        for index in [bus_index(song, track.bus), bus_index(song, track.send)]
            .into_iter()
            .flatten()
        {
            fixed[index] = true;
        }
    }
    fixed
}

/// Left and right gain of a live track's lane in the current song: its
/// fader and pan, then its bus or the master, plus its send.
#[must_use]
pub fn lane_gains(song: &Song, track: &Track) -> (f32, f32) {
    if !song.is_audible(track) {
        return (0.0, 0.0);
    }
    let (track_left, track_right) = gains(f32::from(track.volume), track.pan);
    let bus_gains = |index: usize| {
        let bus = &song.buses[index];
        if bus_audible(song, bus) {
            gains(f32::from(bus.volume), bus.pan)
        } else {
            (0.0, 0.0)
        }
    };
    let (mut left, mut right) = bus_index(song, track.bus).map_or((1.0, 1.0), bus_gains);
    if let Some(index) = bus_index(song, track.send) {
        let level = f32::from(track.send_level.min(127)) / 100.0;
        let (send_left, send_right) = bus_gains(index);
        left += send_left * level;
        right += send_right * level;
    }
    (track_left * left, track_right * right)
}

/// Hashes everything the fixed part of a live render depends on in the
/// mixer. When it is unchanged, a mixer change only needs new lane gains.
pub fn hash_fixed(song: &Song, hasher: &mut impl Hasher) {
    let live = live_tracks(song);
    let fixed = fixed_buses(song, &live);
    live.hash(hasher);
    for (track, live) in song.tracks.iter().zip(&live) {
        track.id.hash(hasher);
        track.bus.hash(hasher);
        track.send.hash(hasher);
        for slot in &track.plugins {
            slot.bypassed.hash(hasher);
        }
        if !live {
            track.volume.hash(hasher);
            track.pan.hash(hasher);
            track.send_level.hash(hasher);
            song.is_audible(track).hash(hasher);
        }
    }
    for (bus, fixed) in song.buses.iter().zip(&fixed) {
        bus.id.hash(hasher);
        for slot in &bus.plugins {
            slot.bypassed.hash(hasher);
        }
        if *fixed {
            bus.volume.hash(hasher);
            bus.pan.hash(hasher);
            bus_audible(song, bus).hash(hasher);
        }
    }
    for slot in &song.master_plugins {
        slot.bypassed.hash(hasher);
    }
}

/// Renders a song for live mixing, before the master stage.
#[must_use]
pub fn render(song: &Song, sample_rate: u32) -> LiveRender {
    render_with_soundfont(song, sample_rate, &mut |path| {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| soundfont::parse(&bytes).ok())
    })
}

/// Renders a song for live mixing, loading SoundFonts through `load`.
#[must_use]
pub fn render_with_soundfont(
    song: &Song,
    sample_rate: u32,
    load: &mut dyn FnMut(&str) -> Option<soundfont::SoundFont>,
) -> LiveRender {
    let live = live_tracks(song);
    let skip: Vec<bool> = song.tracks.iter().map(crate::synth::can_play).collect();
    let parts = crate::render_parts(song, sample_rate, load, &live, &skip);
    let live_tracks: Vec<&Track> = song
        .tracks
        .iter()
        .zip(&live)
        .filter(|(_, live)| **live)
        .map(|(track, _)| track)
        .collect();
    let mut fonts: HashMap<String, Option<Arc<soundfont::SoundFont>>> = HashMap::new();
    let synths = live_tracks
        .iter()
        .map(|track| {
            crate::synth::can_play(track).then(|| {
                let font = track.soundfont.as_ref().and_then(|reference| {
                    fonts
                        .entry(reference.path.clone())
                        .or_insert_with(|| load(&reference.path).map(Arc::new))
                        .clone()
                });
                SynthLane::build(song, track, font, sample_rate, parts.delay)
            })
        })
        .collect();
    LiveRender {
        fixed: parts.fixed,
        lanes: parts.lanes,
        synths,
        synth_delay: parts.delay,
        fonts: fonts
            .into_iter()
            .filter_map(|(path, font)| font.map(|font| (path, font)))
            .collect(),
        tracks: live_tracks.iter().map(|track| track.id).collect(),
        gains: live_tracks
            .iter()
            .map(|track| lane_gains(song, track))
            .collect(),
        master_gain: 1.0,
        limiter: false,
        chorus: 0.0,
        reverb: 0.0,
        finished: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::{Pitch, Velocity};
    use project::song::Note;

    fn song() -> Song {
        let mut song = Song::default();
        for (id, pitch) in [(1, 60), (2, 67)] {
            let mut track = Track::new(TrackId::new(id).expect("track"), "Keys");
            track.notes.push(Note {
                id: project::NoteId::new(id).expect("note"),
                start: 0,
                duration: music_core::PPQ,
                pitch: Pitch::new(pitch).expect("pitch"),
                velocity: Velocity::new(100).expect("velocity"),
                channel: music_core::Channel::first(),
            });
            song.tracks.push(track);
        }
        song.buses.push(Bus::new(1, "Bus"));
        song.tracks[1].bus = Some(1);
        song
    }

    fn close(left: &[f32], right: &[f32]) -> bool {
        left.len() == right.len() && left.iter().zip(right).all(|(a, b)| (a - b).abs() < 1e-4)
    }

    #[test]
    fn a_live_mix_sounds_like_the_full_render() {
        let mut song = song();
        song.tracks[0].volume = 80;
        song.tracks[0].pan = 20;
        song.buses[0].volume = 70;
        song.buses[0].pan = 100;
        let settings = Master::default();
        let mut full = crate::render(&song, 8_000);
        master::apply(&mut full, settings);
        let mut live = render(&song, 8_000);
        assert_eq!(live.lanes.len(), 2, "both tracks are live");
        live.apply_master(settings);
        assert!(close(&live.mixdown().samples, &full.samples));
    }

    #[test]
    fn new_lane_gains_match_a_new_render() {
        let song = song();
        let mut live = render(&song, 8_000);
        let mut changed = song.clone();
        changed.tracks[0].volume = 40;
        changed.tracks[1].muted = true;
        changed.buses[0].pan = 0;
        let mut signature = std::collections::hash_map::DefaultHasher::new();
        hash_fixed(&song, &mut signature);
        let mut after = std::collections::hash_map::DefaultHasher::new();
        hash_fixed(&changed, &mut after);
        assert_eq!(signature.finish(), after.finish(), "no new render needed");
        live.gains = changed
            .tracks
            .iter()
            .map(|track| lane_gains(&changed, track))
            .collect();
        let full = crate::render(&changed, 8_000);
        assert!(close(&live.mixdown().samples, &full.samples));
    }

    #[test]
    fn automation_and_bus_plugins_keep_a_track_fixed() {
        let mut song = song();
        song.tracks[0].automation.push(project::song::Automation {
            target: AutomationTarget::Volume,
            points: Vec::new(),
        });
        song.buses[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("Effect"),
            format: project::song::PluginFormat::Vst3,
            path: String::new(),
            identifier: String::from("effect"),
            bypassed: false,
            state: String::new(),
        });
        assert_eq!(live_tracks(&song), vec![false, false]);
        let mut before = std::collections::hash_map::DefaultHasher::new();
        hash_fixed(&song, &mut before);
        song.tracks[0].volume = 10;
        let mut after = std::collections::hash_map::DefaultHasher::new();
        hash_fixed(&song, &mut after);
        assert_ne!(
            before.finish(),
            after.finish(),
            "a fixed fader renders again"
        );
    }
}
