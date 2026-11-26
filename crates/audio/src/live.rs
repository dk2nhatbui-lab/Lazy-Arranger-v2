//! Live mixer playback.
//!
//! Playback copies from pre-rendered buffers, so a fader used to need a full
//! render before it was heard. Here most tracks are rendered once, without
//! their fader, pan, bus and master gain, into a buffer of their own (a
//! lane). The player sums the lanes with gains it reads while it plays, so a
//! fader, pan, mute or solo change is heard at the next audio buffer.
//!
//! A track with volume, pan or send automation is live too: its lane
//! carries an [`Envelope`] the engine follows as it plays. Only tracks past
//! [`MAX_LANES`] stay in the fixed part of the render, and a bus such a
//! track feeds stays fixed, so its plug-ins run once over everything on it.
//!
//! An edit on one rendered lane (a clip move, a take) renders that lane
//! alone again with [`render_lane`] and swaps it in.
//!
//! Every bus with plug-ins and only live feeds is a [`LiveBus`]: the engine sums its feeds,
//! runs its plug-ins and applies its fader live, and delays the direct
//! route to the master to line up with bus plug-in latency. The master
//! plug-ins always run live.
//!
//! A live track with notes only (no recorded audio, no plug-ins) is not
//! rendered at all: its lane carries a [`SynthLane`] that plays the notes in
//! the audio callback, and a note edit swaps in a new lane.
//!
//! A live track with recorded audio only streams its clips in the callback
//! from decoded files ([`ClipLane`]), and a track whose first plug-in is a
//! VST3 instrument plays it in the callback ([`InstrumentLane`]). Neither
//! is rendered: a clip move or a note edit swaps in a new lane or note
//! list.
//!
//! A live track's insert plug-ins run in the callback as well, from an
//! [`Inserts`] chain built with the render; its lane buffer is then the
//! sound before the inserts. A plug-in edit swaps in a new chain.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use project::TrackId;
use project::song::{Bus, PluginSlot, Song, Track};

use crate::automation::Envelope;
use crate::clips::ClipLane;
use crate::inserts::Inserts;
use crate::instruments::InstrumentLane;
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
    /// The clip lane of each live track that streams its audio, in lane
    /// order.
    pub clips: Vec<Option<ClipLane>>,
    /// Decoded audio files the clip lanes read, to build replacements.
    pub sources: crate::clips::Sources,
    /// The plug-in instrument of each live track that plays one in the
    /// callback, in lane order.
    pub instruments: Vec<Option<InstrumentLane>>,
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
    /// Each lane's routes into live buses: bus id, whether it is the send,
    /// left and right gain.
    pub feeds: Vec<Vec<(u8, bool, f32, f32)>>,
    /// Each lane's plain send route at unity, for an automated send.
    pub sends: Vec<(f32, f32)>,
    /// Each lane's gain automation, when its fader, pan or send has any.
    pub envelopes: Vec<Option<Envelope>>,
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
            clips: Vec::new(),
            sources: crate::clips::Sources::new(),
            instruments: Vec::new(),
            synth_delay: 0,
            fonts: Vec::new(),
            inserts: Vec::new(),
            tracks: Vec::new(),
            gains: Vec::new(),
            feeds: Vec::new(),
            sends: Vec::new(),
            envelopes: Vec::new(),
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

    /// Renders every synth and clip lane into its lane buffer.
    pub fn bake_synths(&mut self) {
        let len = self.fixed.samples.len();
        for (lane, synth) in self.lanes.iter_mut().zip(&mut self.synths) {
            if let Some(synth) = synth.take() {
                *lane = synth.render_all(len);
            }
        }
        for (lane, clips) in self.lanes.iter_mut().zip(&mut self.clips) {
            if let Some(clips) = clips.take() {
                *lane = clips.render_all(len);
            }
        }
        for (lane, instrument) in self.lanes.iter_mut().zip(&mut self.instruments) {
            if let Some(instrument) = instrument.take() {
                *lane = instrument.render_all(len);
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

/// Tracks that could be live before bus routing is considered: the first
/// [`MAX_LANES`].
fn candidates(song: &Song) -> Vec<bool> {
    (0..song.tracks.len())
        .map(|index| index < MAX_LANES)
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
    /// The send to a plain bus at unity level, for a track whose send
    /// level follows automation; zero otherwise, as `direct` holds it.
    pub send: (f32, f32),
    /// Into live buses: bus id, whether it is the send, left and right gain.
    pub feeds: Vec<(u8, bool, f32, f32)>,
}

/// The gains of a live track, given which buses run live.
#[must_use]
///
/// For a track with gain automation the engine applies the fader, pan and
/// send level from its [`crate::automation::Envelope`], so these gains are
/// the routes only, and `send` carries the plain send route at unity.
pub fn lane_routes(song: &Song, buses: &[bool], track: &Track) -> LaneGains {
    let audible = song.is_audible(track);
    let enveloped = crate::automation::automated(track);
    let (track_left, track_right) = if !audible {
        (0.0, 0.0)
    } else if enveloped {
        (1.0, 1.0)
    } else {
        gains(f32::from(track.volume), track.pan)
    };
    let mut routes = LaneGains::default();
    let (mut left, mut right) = (0.0, 0.0);
    match bus_index(song, track.bus) {
        Some(index) if buses[index] => {
            routes
                .feeds
                .push((song.buses[index].id, false, track_left, track_right));
        }
        Some(index) => (left, right) = bus_gain(song, &song.buses[index]),
        None => (left, right) = (1.0, 1.0),
    }
    if let Some(index) = bus_index(song, track.send) {
        let level = if enveloped {
            1.0
        } else {
            f32::from(track.send_level.min(127)) / 100.0
        };
        if buses[index] {
            routes.feeds.push((
                song.buses[index].id,
                true,
                track_left * level,
                track_right * level,
            ));
        } else if enveloped {
            let (send_left, send_right) = bus_gain(song, &song.buses[index]);
            routes.send = (track_left * send_left, track_right * send_right);
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
        // A lane that changes kind (notes, clips, rendered) needs a new
        // graph.
        if *live {
            crate::synth::can_play(track).hash(hasher);
            crate::clips::can_stream(track).hash(hasher);
            crate::instruments::candidate(track).hash(hasher);
        }
        // Automation edits swap in a new envelope on their own.
        crate::automation::automated(track).hash(hasher);
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

/// True when a live track plays in the callback without a rendered lane:
/// notes on a synth lane, or recorded audio on a clip lane.
#[must_use]
pub fn streams(track: &Track) -> bool {
    crate::synth::can_play(track) || crate::clips::can_stream(track)
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
    // Open live plug-in instruments first: a track whose instrument opens
    // is not rendered.
    let mut instruments: Vec<Option<(InstrumentLane, usize)>> = song
        .tracks
        .iter()
        .zip(&live)
        .map(|(track, live)| {
            if *live {
                InstrumentLane::build(song, track, sample_rate)
            } else {
                None
            }
        })
        .collect();
    let skip: Vec<bool> = song
        .tracks
        .iter()
        .zip(&instruments)
        .map(|(track, instrument)| streams(track) || instrument.is_some())
        .collect();
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
    let mut lane_instruments: Vec<Option<(InstrumentLane, usize)>> = instruments
        .iter_mut()
        .zip(&live)
        .filter(|(_, live)| **live)
        .map(|(instrument, _)| instrument.take())
        .collect();
    let frames = parts.fixed.samples.len() / 2;
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
    // An instrument's insert effects are the slots after it.
    let inserts: Vec<Option<Inserts>> = live_tracks
        .iter()
        .zip(&parts.dry)
        .zip(&lane_instruments)
        .map(|((track, dry), instrument)| match instrument {
            Some((_, index)) => Some(Inserts::from_slots(
                &track.plugins[index + 1..],
                sample_rate,
            )),
            None => dry.then(|| Inserts::build(track, sample_rate)),
        })
        .collect();
    let instruments: Vec<Option<InstrumentLane>> = lane_instruments
        .iter_mut()
        .zip(&inserts)
        .map(|(instrument, chain)| {
            let (mut lane, _) = instrument.take()?;
            let latency = chain.as_ref().map_or(0, Inserts::latency);
            lane.set_delay(parts.delay.saturating_sub(latency));
            Some(lane)
        })
        .collect();
    let mut sources = crate::clips::Sources::new();
    let clips = live_tracks
        .iter()
        .zip(&inserts)
        .map(|(track, chain)| {
            if crate::synth::can_play(track) || !crate::clips::can_stream(track) {
                return None;
            }
            // The live inserts add their own latency after the lane.
            let latency = chain.as_ref().map_or(0, Inserts::latency);
            ClipLane::build(
                song,
                track,
                sample_rate,
                parts.delay.saturating_sub(latency),
                &mut sources,
                true,
            )
        })
        .collect();
    LiveRender {
        fixed: parts.fixed,
        lanes: parts.lanes,
        synths,
        clips,
        sources,
        instruments,
        synth_delay: parts.delay,
        fonts: fonts
            .into_iter()
            .filter_map(|(path, font)| font.map(|font| (path, font)))
            .collect(),
        inserts,
        tracks: live_tracks.iter().map(|track| track.id).collect(),
        gains: routes.iter().map(|routes| routes.direct).collect(),
        sends: routes.iter().map(|routes| routes.send).collect(),
        feeds: routes.into_iter().map(|routes| routes.feeds).collect(),
        envelopes: live_tracks
            .iter()
            .map(|track| {
                crate::automation::automated(track)
                    .then(|| Envelope::build(song, track, sample_rate, frames))
            })
            .collect(),
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

/// A fingerprint of everything a rendered lane is built from: recorded
/// audio, takes and comps, notes, sound, plug-ins, the automation the
/// render bakes in, and the tempo. When it changes, the lane is out of date.
#[must_use]
pub fn lane_signature(song: &Song, track: &Track) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // Small values without `Hash`: their debug text is enough.
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
        track.audio,
        track.takes,
        track.comps,
        track.notes,
        track.patch,
        track.soundfont,
        song.tempo_map
    )
    .hash(&mut hasher);
    // Live inserts are left out of the lane and swapped on their own.
    if !crate::inserts::can_run_live(track) {
        format!("{:?}", track.plugins).hash(&mut hasher);
    }
    for lane in &track.automation {
        if !matches!(
            lane.target,
            project::song::AutomationTarget::Volume
                | project::song::AutomationTarget::Pan
                | project::song::AutomationTarget::Send
        ) {
            format!("{:?}{:?}", lane.target, lane.points).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// One track's lane rendered again, to swap into a playing graph.
#[derive(Debug)]
pub struct LaneRender {
    /// The lane buffer, before the track's fader.
    pub samples: Vec<f32>,
    /// Frames lanes are delayed by for track plug-in latency. A lane fits a
    /// playing graph only when this matches the graph's.
    pub delay: usize,
    /// Frames the direct route is delayed by for bus plug-in latency; must
    /// match too.
    pub bus_delay: usize,
}

/// Renders only one live track's lane, as [`render`] would, so an edit on
/// that track (a clip move, a take, a plug-in instrument) is heard without
/// rendering the whole song. None when the track is not live or plays as
/// a synth lane.
#[must_use]
pub fn render_lane(song: &Song, sample_rate: u32, track: TrackId) -> Option<LaneRender> {
    let live = live_tracks(song);
    let index = song.tracks.iter().position(|entry| entry.id == track)?;
    if !live[index] || streams(&song.tracks[index]) {
        return None;
    }
    // Every other track is skipped, as if a synth lane played it.
    let skip: Vec<bool> = (0..song.tracks.len()).map(|other| other != index).collect();
    let dry: Vec<bool> = song
        .tracks
        .iter()
        .map(crate::inserts::can_run_live)
        .collect();
    let buses = live_buses(song);
    let parts = crate::render_parts(
        song,
        sample_rate,
        &mut |path| {
            std::fs::read(path)
                .ok()
                .and_then(|bytes| soundfont::parse(&bytes).ok())
        },
        &crate::LivePlan {
            live: &live,
            skip: &skip,
            dry: &dry,
            buses: &buses,
            master: true,
        },
    );
    let lane = live[..index].iter().filter(|live| **live).count();
    let samples = parts.lanes.into_iter().nth(lane)?;
    Some(LaneRender {
        samples,
        delay: parts.delay,
        bus_delay: parts.bus_delay,
    })
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
    fn automated_tracks_and_their_buses_stay_live() {
        let mut song = song();
        song.tracks[0].automation.push(project::song::Automation {
            target: project::song::AutomationTarget::Volume,
            points: Vec::new(),
        });
        song.tracks[0].send = Some(1);
        song.buses[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("Effect"),
            format: project::song::PluginFormat::Vst3,
            path: String::new(),
            identifier: String::from("effect"),
            bypassed: false,
            state: String::new(),
        });
        assert_eq!(live_tracks(&song), vec![true, true]);
        assert_eq!(live_buses(&song), vec![true]);
        let mut before = std::collections::hash_map::DefaultHasher::new();
        hash_fixed(&song, &mut before);
        song.tracks[0].volume = 10;
        song.tracks[0].automation[0]
            .points
            .push(project::song::AutomationPoint {
                tick: 0,
                value: 20.0,
            });
        let mut after = std::collections::hash_map::DefaultHasher::new();
        hash_fixed(&song, &mut after);
        assert_eq!(
            before.finish(),
            after.finish(),
            "an automation edit swaps in a new envelope"
        );
        song.tracks[0].automation.clear();
        let mut cleared = std::collections::hash_map::DefaultHasher::new();
        hash_fixed(&song, &mut cleared);
        assert_ne!(
            after.finish(),
            cleared.finish(),
            "removing it renders again"
        );
    }
}
