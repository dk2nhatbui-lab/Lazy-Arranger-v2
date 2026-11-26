//! Live mixer changes while the song plays.
//!
//! Playback mixes most tracks live (see `audio::live`): their faders, pans,
//! mutes, solos, the buses they feed and the master gain are applied by the
//! audio callback, so a change is heard at the next audio buffer. Every frame
//! the app hands the current mixer to the player.
//!
//! A track's insert plug-ins run live too: a preset, knob or bypass change
//! builds a new chain for that track and swaps it in. Some changes still
//! need a new render: adding the first or removing the last gain automation
//! lane of a track, or a plug-in change that alters a track's or bus's
//! latency. Bus and master plug-ins run live and swap like track inserts,
//! and a volume, pan or send automation edit swaps in a new envelope.
//! The app watches a fingerprint of those settings. When it changes, the
//! song renders again in the background, and the engine swaps the new graph
//! in at its next buffer, at the same position, on the same stream.

use std::hash::Hasher;

use audio::inserts::ChainId;

use super::*;

/// The synth lanes and live insert chains of the playing graph: what each
/// was built from, and what a replacement needs.
#[derive(Debug, Default)]
pub(super) struct SynthState {
    fonts: std::collections::HashMap<String, std::sync::Arc<audio::soundfont::SoundFont>>,
    delay: usize,
    rate: u32,
    /// Each synth track and the fingerprint its lane was built from.
    tracks: Vec<(TrackId, u64)>,
    /// Each plug-in chain that could run live: track inserts, live buses
    /// and the master.
    inserts: Vec<InsertLane>,
    /// Each track with gain automation and the fingerprint its envelope
    /// was built from.
    envelopes: Vec<(TrackId, u64)>,
    /// Length of the playing graph in frames.
    frames: usize,
    /// Each rendered lane and the fingerprint it was rendered from.
    lanes: Vec<(TrackId, u64)>,
    /// The direct route's bus delay in the playing graph.
    bus_delay: usize,
    /// A lane being rendered again in the background.
    pending: Option<(
        TrackId,
        std::sync::mpsc::Receiver<Option<audio::live::LaneRender>>,
    )>,
}

/// A plug-in chain that could run in the audio callback.
#[derive(Debug)]
struct InsertLane {
    chain: ChainId,
    /// Fingerprint of the slots the chain was built from.
    built: u64,
    /// Latency of the chain; a new chain must match it to swap in.
    latency: usize,
    /// False when the render kept the inserts (a plug-in instrument), so an
    /// edit needs a new render.
    live: bool,
}

impl SynthState {
    /// Records the synth lanes of a render of `song`.
    pub(super) fn new(song: &Song, render: &audio::live::LiveRender) -> Self {
        let tracks = render
            .tracks
            .iter()
            .zip(&render.synths)
            .filter(|(_, synth)| synth.is_some())
            .filter_map(|(id, _)| {
                let track = song.tracks.iter().find(|track| track.id == *id)?;
                Some((*id, audio::synth::signature(song, track)))
            })
            .collect();
        let inserts = render
            .tracks
            .iter()
            .zip(&render.inserts)
            .filter_map(|(id, chain)| {
                let track = song.tracks.iter().find(|track| track.id == *id)?;
                audio::inserts::can_run_live(track).then(|| InsertLane {
                    chain: ChainId::Track(*id),
                    built: audio::inserts::signature(track),
                    latency: chain.as_ref().map_or(0, audio::inserts::Inserts::latency),
                    live: chain.is_some(),
                })
            })
            .chain(render.buses.iter().map(|bus| InsertLane {
                chain: ChainId::Bus(bus.id),
                built: audio::inserts::slots_signature(&bus.slots),
                latency: bus.inserts.latency(),
                live: true,
            }))
            .chain(render.master_inserts.as_ref().map(|chain| InsertLane {
                chain: ChainId::Master,
                built: audio::inserts::slots_signature(&render.master_slots),
                latency: chain.latency(),
                live: true,
            }))
            .collect();
        let envelopes = render
            .tracks
            .iter()
            .zip(&render.envelopes)
            .filter(|(_, envelope)| envelope.is_some())
            .filter_map(|(id, _)| {
                let track = song.tracks.iter().find(|track| track.id == *id)?;
                Some((*id, audio::automation::signature(song, track)))
            })
            .collect();
        let lanes = render
            .tracks
            .iter()
            .zip(&render.synths)
            .filter(|(_, synth)| synth.is_none())
            .filter_map(|(id, _)| {
                let track = song.tracks.iter().find(|track| track.id == *id)?;
                Some((*id, audio::live::lane_signature(song, track)))
            })
            .collect();
        Self {
            fonts: render.fonts.iter().cloned().collect(),
            delay: render.synth_delay,
            rate: render.sample_rate(),
            tracks,
            inserts,
            envelopes,
            frames: render.fixed.samples.len() / 2,
            lanes,
            bus_delay: render.bus_delay,
            pending: None,
        }
    }

    /// Keeps the state when the player runs at the rate the lanes were
    /// built for; otherwise the player baked them and they cannot be
    /// swapped.
    pub(super) fn fit(self, player_rate: u32) -> Option<Self> {
        (self.rate == player_rate
            && !(self.tracks.is_empty()
                && self.inserts.is_empty()
                && self.envelopes.is_empty()
                && self.lanes.is_empty()))
        .then_some(self)
    }

    /// Rebuilds the lane of every synth track whose notes or sound changed,
    /// and the chain of every track whose plug-ins changed, and swaps them
    /// into the player, so the edit is heard while playing. Returns true
    /// when a change cannot be swapped in and needs a new render.
    pub(super) fn swap_edited(&mut self, song: &Song, player: &audio::player::Player) -> bool {
        let mut rerender = self.swap_inserts(song, player);
        self.swap_synths(song, player);
        self.swap_envelopes(song, player);
        rerender |= self.swap_lanes(song, player);
        rerender
    }

    /// Renders the lane of one edited track again in the background (a
    /// clip move, a take, a plug-in instrument) and swaps it in when done.
    /// Returns true when the new lane does not fit the playing graph.
    fn swap_lanes(&mut self, song: &Song, player: &audio::player::Player) -> bool {
        if let Some((id, receiver)) = &self.pending {
            let id = *id;
            match receiver.try_recv() {
                Err(std::sync::mpsc::TryRecvError::Empty) => return false,
                Ok(Some(lane)) => {
                    self.pending = None;
                    if lane.delay != self.delay
                        || lane.bus_delay != self.bus_delay
                        || lane.samples.len() > self.frames * 2
                    {
                        // The latency or the song length changed.
                        return true;
                    }
                    if player.swap_lane(id, lane.samples).is_err()
                        && let Some(entry) = self.lanes.iter_mut().find(|entry| entry.0 == id)
                    {
                        // Try again later.
                        entry.1 = 0;
                    }
                }
                Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                }
            }
        }
        for (id, built) in &mut self.lanes {
            let Some(track) = song.tracks.iter().find(|track| track.id == *id) else {
                continue;
            };
            let signature = audio::live::lane_signature(song, track);
            if signature == *built {
                continue;
            }
            *built = signature;
            let (sender, receiver) = std::sync::mpsc::channel();
            let (song, rate, track) = (song.clone(), self.rate, *id);
            std::thread::spawn(move || {
                let _ = sender.send(audio::live::render_lane(&song, rate, track));
            });
            self.pending = Some((*id, receiver));
            break;
        }
        false
    }

    fn swap_envelopes(&mut self, song: &Song, player: &audio::player::Player) {
        for (id, built) in &mut self.envelopes {
            let Some(track) = song.tracks.iter().find(|track| track.id == *id) else {
                continue;
            };
            // Adding or removing all gain automation renders again.
            if !audio::automation::automated(track) {
                continue;
            }
            let signature = audio::automation::signature(song, track);
            if signature == *built {
                continue;
            }
            let envelope = audio::automation::Envelope::build(song, track, self.rate, self.frames);
            if player.swap_envelope(*id, envelope).is_ok() {
                *built = signature;
            }
        }
    }

    fn swap_inserts(&mut self, song: &Song, player: &audio::player::Player) -> bool {
        let mut rerender = false;
        let live_buses = audio::live::live_buses(song);
        for lane in &mut self.inserts {
            // A chain that stopped running live changes the render
            // signature instead.
            let slots = match lane.chain {
                ChainId::Track(id) => song
                    .tracks
                    .iter()
                    .find(|track| track.id == id)
                    .filter(|track| audio::inserts::can_run_live(track))
                    .map(|track| track.plugins.as_slice()),
                ChainId::Bus(id) => song
                    .buses
                    .iter()
                    .zip(&live_buses)
                    .find(|(bus, live)| bus.id == id && **live)
                    .map(|(bus, _)| bus.plugins.as_slice()),
                ChainId::Master => Some(song.master_plugins.as_slice()),
            };
            let Some(slots) = slots else {
                continue;
            };
            let signature = audio::inserts::slots_signature(slots);
            if signature == lane.built {
                continue;
            }
            lane.built = signature;
            if !lane.live {
                rerender = true;
                continue;
            }
            // Opening plug-ins can take a moment; it happens here, off the
            // audio thread.
            let chain = audio::inserts::Inserts::from_slots(slots, self.rate);
            // Other tracks' and buses' delays depend on a track or bus
            // chain's latency; the master's delays everything alike.
            if chain.latency() != lane.latency && lane.chain != ChainId::Master {
                rerender = true;
                continue;
            }
            if player.swap_inserts(lane.chain, chain).is_err() {
                // Try again next frame.
                lane.built = 0;
            }
        }
        rerender
    }

    fn swap_synths(&mut self, song: &Song, player: &audio::player::Player) {
        for (id, built) in &mut self.tracks {
            let Some(track) = song.tracks.iter().find(|track| track.id == *id) else {
                continue;
            };
            if !audio::synth::can_play(track) {
                continue;
            }
            let signature = audio::synth::signature(song, track);
            if signature == *built {
                continue;
            }
            let font = match &track.soundfont {
                Some(reference) => match self.fonts.get(&reference.path) {
                    Some(font) => Some(std::sync::Arc::clone(font)),
                    // A new font needs loading: leave it to a full render.
                    None => continue,
                },
                None => None,
            };
            let lane = audio::synth::SynthLane::build(song, track, font, self.rate, self.delay);
            if player.swap_synth(*id, lane).is_ok() {
                *built = signature;
            }
        }
    }
}

/// A background render that will replace the playing buffer.
pub(super) struct LiveRemix {
    pub(super) receiver: std::sync::mpsc::Receiver<Result<RenderedSong, String>>,
    pub(super) signature: u64,
}

/// A fingerprint of every mixer setting that the playing buffer bakes in.
///
/// Two songs with the same fingerprint need the same render; their other
/// mixer differences are applied live. A change to it means the playing
/// buffer is out of date.
///
/// The master gain, limiter, reverb and chorus run live in the engine, so
/// they are not part of it.
pub(super) fn mix_signature(song: &Song) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    audio::live::hash_fixed(song, &mut hasher);
    hasher.finish()
}

impl ShellApp {
    /// Fingerprint of the current song's mixer.
    pub(super) fn current_mix_signature(&self) -> u64 {
        mix_signature(self.song())
    }

    /// Renders again when the mix changed while playing, and swaps the
    /// finished buffer in at the current position.
    pub(super) fn poll_live_remix(&mut self, context: &Context) {
        let playing = self
            .player
            .as_ref()
            .is_some_and(audio::player::Player::is_playing);
        if !playing || self.launcher.active() {
            self.live_remix = None;
            return;
        }
        if let Some(player) = &self.player {
            player.apply_mix(self.song(), self.settings.master());
            if let Some(state) = &mut self.synth_state
                && state.swap_edited(self.editor.song(), player)
            {
                // Render again; the new graph brings a fresh state.
                self.mix_rendered = 0;
            }
        }
        if let Some(remix) = &self.live_remix {
            let result = match remix.receiver.try_recv() {
                Ok(result) => result,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    context.request_repaint_after(std::time::Duration::from_millis(30));
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Err(String::from("audio renderer stopped"))
                }
            };
            let signature = remix.signature;
            self.live_remix = None;
            match result {
                Ok(rendered) => self.swap_in_render(rendered, signature),
                Err(error) => {
                    // Do not retry the same mix in a loop.
                    self.mix_rendered = signature;
                    self.status = Some(format!("audio: {error}"));
                }
            }
        }
        let signature = self.current_mix_signature();
        if self.live_remix.is_none() && signature != self.mix_rendered {
            let receiver = self.spawn_render(self.song().clone(), self.view == View::Mix);
            self.live_remix = Some(LiveRemix {
                receiver,
                signature,
            });
            context.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }

    /// Replaces the playing buffer with a new render, keeping the position.
    fn swap_in_render(&mut self, rendered: RenderedSong, signature: u64) {
        let Some(player) = &self.player else {
            return;
        };
        let stem_rate = rendered.audio.sample_rate();
        // The engine installs the new graph at its next buffer and keeps the
        // position, the stream and the loop; the old graph is freed here.
        if let Err(error) = player.load(rendered.audio) {
            // Keep playing the old mix; the next change tries again.
            self.status = Some(format!("audio: {error}"));
            return;
        }
        self.stem_sample_rate = stem_rate;
        self.stem_levels = rendered.stems;
        self.synth_state = rendered.synths.fit(player.sample_rate());
        self.mix_rendered = signature;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> Song {
        let mut song = Song::default();
        song.tracks.push(project::song::Track::new(
            TrackId::new(1).expect("track"),
            "Piano",
        ));
        song
    }

    #[test]
    fn plain_fader_moves_keep_the_signature() {
        // These are applied live, so they must not start a new render.
        let base = song();
        let start = mix_signature(&base);
        assert_eq!(start, mix_signature(&base.clone()));

        let mut changed = base.clone();
        changed.tracks[0].volume = changed.tracks[0].volume.wrapping_sub(10);
        changed.tracks[0].pan = 0;
        changed.tracks[0].muted = !changed.tracks[0].muted;
        changed.tracks[0].soloed = !changed.tracks[0].soloed;
        assert_eq!(start, mix_signature(&changed));
    }

    #[test]
    fn automation_lanes_change_the_signature_but_not_their_edits() {
        let base = song();
        let start = mix_signature(&base);

        // The first gain automation lane renders again; later edits and the
        // fader under it are swapped in live.
        let mut automated = base.clone();
        automated.tracks[0]
            .automation
            .push(project::song::Automation {
                target: project::song::AutomationTarget::Volume,
                points: Vec::new(),
            });
        let before = mix_signature(&automated);
        assert_ne!(start, before);
        automated.tracks[0].volume = 10;
        automated.tracks[0].automation[0]
            .points
            .push(project::song::AutomationPoint {
                tick: 0,
                value: 50.0,
            });
        assert_eq!(before, mix_signature(&automated));
    }

    #[test]
    fn note_edits_keep_the_signature() {
        let base = song();
        let mut renamed = base.clone();
        renamed.tracks[0].name = String::from("Keys");
        assert_eq!(mix_signature(&base), mix_signature(&renamed));
    }
}
