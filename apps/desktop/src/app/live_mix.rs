//! Live mixer changes while the song plays.
//!
//! Playback mixes most tracks live (see `audio::live`): their faders, pans,
//! mutes, solos, the buses they feed and the master gain are applied by the
//! audio callback, so a change is heard at the next audio buffer. Every frame
//! the app hands the current mixer to the player.
//!
//! Some changes still need a new render: a fader on a track with volume
//! automation, a bus with plug-ins, a plug-in bypass, or a master effect.
//! The app watches a fingerprint of those settings. When it changes, the
//! song renders again in the background, and the engine swaps the new graph
//! in at its next buffer, at the same position, on the same stream.

use std::hash::Hasher;

use super::*;

/// The synth lanes of the playing graph: what each was built from, and what
/// a replacement needs.
#[derive(Debug, Default)]
pub(super) struct SynthState {
    fonts: std::collections::HashMap<String, std::sync::Arc<audio::soundfont::SoundFont>>,
    delay: usize,
    rate: u32,
    /// Each synth track and the fingerprint its lane was built from.
    tracks: Vec<(TrackId, u64)>,
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
        Self {
            fonts: render.fonts.iter().cloned().collect(),
            delay: render.synth_delay,
            rate: render.sample_rate(),
            tracks,
        }
    }

    /// Keeps the state when the player runs at the rate the lanes were
    /// built for; otherwise the player baked them and they cannot be
    /// swapped.
    pub(super) fn fit(self, player_rate: u32) -> Option<Self> {
        (self.rate == player_rate && !self.tracks.is_empty()).then_some(self)
    }

    /// Rebuilds the lane of every synth track whose notes or sound changed,
    /// and swaps it into the player, so the edit is heard while playing.
    pub(super) fn swap_edited(&mut self, song: &Song, player: &audio::player::Player) {
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
<<<<<<< HEAD
            player.apply_mix(self.song(), self.settings.master());
=======
            player.apply_mix(self.song(), self.settings.master_gain_db);
>>>>>>> 92b0496cb85b3db5315895ddecb4ace903f1c153
            if let Some(state) = &mut self.synth_state {
                state.swap_edited(self.editor.song(), player);
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
    fn baked_settings_change_the_signature() {
        let base = song();
        let start = mix_signature(&base);

        // A fader under volume automation is part of the render.
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
        assert_ne!(before, mix_signature(&automated));
    }

    #[test]
    fn note_edits_keep_the_signature() {
        let base = song();
        let mut renamed = base.clone();
        renamed.tracks[0].name = String::from("Keys");
        assert_eq!(mix_signature(&base), mix_signature(&renamed));
    }
}
