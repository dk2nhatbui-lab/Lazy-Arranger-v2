//! Live mixer playback.
//!
//! Playback copies from pre-rendered buffers, so a fader used to need a full
//! render before it was heard. Here most tracks are rendered once, without
//! their fader, pan, bus and master gain, into a buffer of their own (a
//! lane). The player sums the lanes with gains it reads while it plays, so a
//! fader, pan, mute or solo change is heard at the next audio buffer.
//!
//! A track stays in the fixed part of the render when its gain changes over
//! time: volume, pan or send automation. A bus such a track feeds stays
//! fixed too, so its plug-ins run once over everything on it. Changes that
//! touch the fixed part still need a new render.
//!
//! Every other bus with plug-ins is a [`LiveBus`]: the engine sums its feeds,
//! runs its plug-ins and applies its fader live, and delays the direct
//! route to the master to line up with bus plug-in latency. The master
//! plug-ins always run live.
//!
//! A live track with notes only (no recorded audio, no plug-ins) is not
//! rendered at all: its lane carries a [`SynthLane`] that plays the notes in
//! the audio callback, and a note edit swaps in a new lane.
//!
//! A live track's insert plug-ins run in the callback as well, from an
//! [`Inserts`] chain built with the render; its lane buffer is then the
//! sound before the inserts. A plug-in edit swaps in a new chain.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use project::TrackId;
use project::song::{AutomationTarget, Bus, PluginSlot, Song, Track};

use crate::inserts::Inserts;
use crate::master::{self, Master};
use crate::synth::SynthLane;
use crate::{RenderedAudio, gains, soundfont};

/// A song rendered for live mixing.
#[derive(Debug, Default)]
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
    /// The insert chain of each lane whose plug-ins run live, in lane
    /// order; its buffer is then before the inserts.
    pub inserts: Vec<Option<Inserts>>,
    /// The track of each lane.
    pub tracks: Vec<TrackId>,
    /// Left and right gain of each lane's direct route to the master (its
    /// fader, pan, and plain buses) when it was rendered.
    pub gains: Vec<(f32, f32)>,
    /// Each lane's routes into live buses: bus id, left and right gain.
    pub feeds: Vec<Vec<(u8, f32, f32)>>,
    /// Buses whose plug-ins run live.
    pub buses: Vec<LiveBus>,
    /// Frames the direct route is delayed by, to line up with bus plug-in
    /// latency.
    pub bus_delay: usize,
    /// The master plug-ins, run live on the sum before the master effects.
    pub master_inserts: Option<Inserts>,
    /// The master slots the chain was built from, to build it again.
    pub master_slots: Vec<PluginSlot>,
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
            inserts: Vec::new(),
            tracks: Vec::new(),
            gains: Vec::new(),
            feeds: Vec::new(),
            buses: Vec::new(),
            bus_delay: 0,
            master_inserts: None,
            master_slots: Vec::new(),
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

    /// Runs every live insert chain over its lane buffer, as playback
    /// would, and drops the chains.
    pub fn bake_inserts(&mut self) {
        self.bake_synths();
        for (lane, inserts) in self.lanes.iter_mut().zip(&mut self.inserts) {
            if let Some(mut inserts) = inserts.take() {
                inserts.run_all(lane);
            }
        }
    }

    /// The finished mix with the gains it was rendered with, played
    /// through the engine from start to end.
    #[must_use]
    pub fn mixdown(self) -> RenderedAudio {
        let sample_rate = self.fixed.sample_rate;
        RenderedAudio {
            samples: crate::engine::Graph::from_render(self).render_all(),
            sample_rate,
        }
    }
}

/// A bus whose plug-ins and gains run in the engine.
#[derive(Debug)]
pub struct LiveBus {
    /// The bus id tracks route to.
    pub id: u8,
    /// The slots the chain was built from, to build it again.
    pub slots: Vec<PluginSlot>,
    /// The bus's plug-ins.
    pub inserts: Inserts,
    /// Left and right gain of the bus fader, pan and mute.
    pub gains: (f32, f32),
    /// Frames the bus output is delayed by, so every bus lines up with the
    /// slowest one.
    pub delay: usize,
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

fn automated(track: &Track) -> bool {
    track.automation.iter().any(|lane| {
        matches!(
            lane.target,
            AutomationTarget::Volume | AutomationTarget::Pan | AutomationTarget::Send
        )
    })
}

/// Tracks that could be live before bus routing is considered: no gain
/// automation, and within [`MAX_LANES`].
fn candidates(song: &Song) -> Vec<bool> {
    let mut count = 0;
    song.tracks
        .iter()
        .map(|track| {
            let candidate = count < MAX_LANES && !automated(track);
            count += usize::from(candidate);
            candidate
        })
        .collect()
}

/// Buses whose plug-ins run in the engine: they have active plug-ins and
/// every track feeding them is live. A bus fed by a rendered track keeps
/// its plug-ins in the render, so both run through the same instance.
#[must_use]
pub fn live_buses(song: &Song) -> Vec<bool> {
    let mut live: Vec<bool> = song.buses.iter().map(|bus| active(&bus.plugins)).collect();
    for (track, candidate) in song.tracks.iter().zip(candidates(song)) {
        if candidate {
            continue;
        }
        for index in [bus_index(song, track.bus), bus_index(song, track.send)]
            .into_iter()
            .flatten()
        {
            live[index] = false;
        }
    }
    live
}

/// Which tracks can be mixed live, in track order.
#[must_use]
pub fn live_tracks(song: &Song) -> Vec<bool> {
    let buses = live_buses(song);
    let route_ok = |id: Option<u8>| {
        bus_index(song, id).is_none_or(|index| buses[index] || !active(&song.buses[index].plugins))
    };
    song.tracks
        .iter()
        .zip(candidates(song))
        .map(|(track, candidate)| candidate && route_ok(track.bus) && route_ok(track.send))
        .collect()
}

/// Buses whose sound is in the fixed part: they have plug-ins that do not
/// run live, or a fixed track feeds them.
fn fixed_buses(song: &Song, live: &[bool], buses: &[bool]) -> Vec<bool> {
    let mut fixed: Vec<bool> = song
        .buses
        .iter()
        .zip(buses)
        .map(|(bus, live)| !live && active(&bus.plugins))
        .collect();
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

/// Left and right gain of a bus fader, pan and mute.
fn bus_gain(song: &Song, bus: &Bus) -> (f32, f32) {
    if bus_audible(song, bus) {
        gains(f32::from(bus.volume), bus.pan)
    } else {
        (0.0, 0.0)
    }
}

/// The gains of one live track in the current song.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaneGains {
    /// Straight to the master: the fader and pan, then a plain bus or the
    /// master, plus a send to a plain bus.
    pub direct: (f32, f32),
    /// Into live buses: bus id, left and right gain.
    pub feeds: Vec<(u8, f32, f32)>,
}

/// The gains of a live track, given which buses run live.
#[must_use]
pub fn lane_routes(song: &Song, buses: &[bool], track: &Track) -> LaneGains {
    let audible = song.is_audible(track);
    let (track_left, track_right) = if audible {
        gains(f32::from(track.volume), track.pan)
    } else {
        (0.0, 0.0)
    };
    let mut routes = LaneGains::default();
    let (mut left, mut right) = (0.0, 0.0);
    match bus_index(song, track.bus) {
        Some(index) if buses[index] => {
            routes
                .feeds
                .push((song.buses[index].id, track_left, track_right));
        }
        Some(index) => (left, right) = bus_gain(song, &song.buses[index]),
        None => (left, right) = (1.0, 1.0),
    }
    if let Some(index) = bus_index(song, track.send) {
        let level = f32::from(track.send_level.min(127)) / 100.0;
        if buses[index] {
            routes.feeds.push((
                song.buses[index].id,
                track_left * level,
                track_right * level,
            ));
        } else {
            let (send_left, send_right) = bus_gain(song, &song.buses[index]);
            left += send_left * level;
            right += send_right * level;
        }
    }
    routes.direct = (track_left * left, track_right * right);
    routes
}

/// Left and right gain of a live track's direct route in the current song:
/// its fader and pan, then a plain bus or the master, plus its send.
#[must_use]
pub fn lane_gains(song: &Song, track: &Track) -> (f32, f32) {
    lane_routes(song, &live_buses(song), track).direct
}

/// Left and right gain of each live bus, in bus order: bus id and gains.
#[must_use]
pub fn bus_gains(song: &Song, buses: &[bool]) -> Vec<(u8, f32, f32)> {
    song.buses
        .iter()
        .zip(buses)
        .filter(|(_, live)| **live)
        .map(|(bus, _)| {
            let (left, right) = bus_gain(song, bus);
            (bus.id, left, right)
        })
        .collect()
}

/// Hashes everything the fixed part of a live render depends on in the
/// mixer. When it is unchanged, a mixer change only needs new lane gains.
pub fn hash_fixed(song: &Song, hasher: &mut impl Hasher) {
    let live = live_tracks(song);
    let buses = live_buses(song);
    let fixed = fixed_buses(song, &live, &buses);
    live.hash(hasher);
    buses.hash(hasher);
    for (track, live) in song.tracks.iter().zip(&live) {
        track.id.hash(hasher);
        track.bus.hash(hasher);
        track.send.hash(hasher);
        let live_inserts = *live && crate::inserts::can_run_live(track);
        live_inserts.hash(hasher);
        // Live insert chains are swapped in on their own.
        if !live_inserts {
            for slot in &track.plugins {
                slot.bypassed.hash(hasher);
            }
        }
        if !live {
            track.volume.hash(hasher);
            track.pan.hash(hasher);
            track.send_level.hash(hasher);
            song.is_audible(track).hash(hasher);
        }
    }
    for ((bus, fixed), live) in song.buses.iter().zip(&fixed).zip(&buses) {
        bus.id.hash(hasher);
        // Live bus chains are swapped in on their own.
        if !live {
            for slot in &bus.plugins {
                slot.bypassed.hash(hasher);
            }
        }
        if *fixed {
            bus.volume.hash(hasher);
            bus.pan.hash(hasher);
            bus_audible(song, bus).hash(hasher);
        }
    }
    // The master plug-ins always run live.
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
    let dry: Vec<bool> = song
        .tracks
        .iter()
        .map(crate::inserts::can_run_live)
        .collect();
    let buses = live_buses(song);
    let parts = crate::render_parts(
        song,
        sample_rate,
        load,
        &crate::LivePlan {
            live: &live,
            skip: &skip,
            dry: &dry,
            buses: &buses,
            master: true,
        },
    );
    let live_tracks: Vec<&Track> = song
        .tracks
        .iter()
        .zip(&live)
        .filter(|(_, live)| **live)
        .map(|(track, _)| track)
        .collect();
    let routes: Vec<LaneGains> = live_tracks
        .iter()
        .map(|track| lane_routes(song, &buses, track))
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
        inserts: live_tracks
            .iter()
            .zip(&parts.dry)
            .map(|(track, dry)| dry.then(|| Inserts::build(track, sample_rate)))
            .collect(),
        tracks: live_tracks.iter().map(|track| track.id).collect(),
        gains: routes.iter().map(|routes| routes.direct).collect(),
        feeds: routes.into_iter().map(|routes| routes.feeds).collect(),
        buses: song
            .buses
            .iter()
            .zip(&buses)
            .zip(&parts.bus_latencies)
            .filter(|((_, live), _)| **live)
            .map(|((bus, _), latency)| LiveBus {
                id: bus.id,
                slots: bus.plugins.clone(),
                inserts: Inserts::from_slots(&bus.plugins, sample_rate),
                gains: bus_gain(song, bus),
                delay: parts.bus_delay.saturating_sub(*latency),
            })
            .collect(),
        bus_delay: parts.bus_delay,
        master_inserts: Some(Inserts::from_slots(&song.master_plugins, sample_rate)),
        master_slots: song.master_plugins.clone(),
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
    fn automation_keeps_a_track_and_the_bus_it_feeds_fixed() {
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
        // The bus plug-ins run live, so the track on the bus stays live.
        assert_eq!(live_tracks(&song), vec![false, true]);
        assert_eq!(live_buses(&song), vec![true]);
        // Once a fixed track feeds the bus too, the bus and both tracks
        // are rendered.
        song.tracks[0].send = Some(1);
        assert_eq!(live_buses(&song), vec![false]);
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
