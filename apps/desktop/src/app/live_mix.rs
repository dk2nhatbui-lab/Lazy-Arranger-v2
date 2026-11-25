//! Live mixer changes while the song plays.
//!
//! Playback is a pre-rendered buffer, so a fader, pan, mute or solo change
//! does nothing until the song is rendered again. While the song plays, the
//! app watches a fingerprint of the mix. When it changes, the song renders
//! again in the background, and the new buffer replaces the old one at the
//! same position. This matches the C++ app, which rebuilds its playback
//! snapshot when the mixer changes.

use std::hash::{Hash, Hasher};

use super::*;

/// A background render that will replace the playing buffer.
pub(super) struct LiveRemix {
    pub(super) receiver: std::sync::mpsc::Receiver<Result<RenderedSong, String>>,
    pub(super) signature: u64,
}

/// A fingerprint of every setting the mixer controls.
///
/// Two songs with the same fingerprint sound the same through the mixer, so a
/// change to it means the playing buffer is out of date.
pub(super) fn mix_signature(song: &Song, settings: &crate::settings::Settings) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for track in &song.tracks {
        track.id.hash(&mut hasher);
        track.volume.hash(&mut hasher);
        track.pan.hash(&mut hasher);
        track.muted.hash(&mut hasher);
        track.soloed.hash(&mut hasher);
        track.bus.hash(&mut hasher);
        track.send.hash(&mut hasher);
        track.send_level.hash(&mut hasher);
        for slot in &track.plugins {
            slot.bypassed.hash(&mut hasher);
        }
    }
    for bus in &song.buses {
        bus.id.hash(&mut hasher);
        bus.volume.hash(&mut hasher);
        bus.pan.hash(&mut hasher);
        bus.muted.hash(&mut hasher);
        bus.soloed.hash(&mut hasher);
        for slot in &bus.plugins {
            slot.bypassed.hash(&mut hasher);
        }
    }
    for slot in &song.master_plugins {
        slot.bypassed.hash(&mut hasher);
    }
    settings.master_gain_db.to_bits().hash(&mut hasher);
    settings.master_limiter.hash(&mut hasher);
    settings.master_reverb.to_bits().hash(&mut hasher);
    settings.master_chorus.to_bits().hash(&mut hasher);
    hasher.finish()
}

impl ShellApp {
    /// Fingerprint of the current song and master settings.
    pub(super) fn current_mix_signature(&self) -> u64 {
        mix_signature(self.song(), &self.settings)
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
        let Some(old) = self.player.take() else {
            return;
        };
        let stem_rate = rendered.audio.sample_rate;
        let new = match audio::player::Player::new_on(
            rendered.audio,
            self.settings.audio_output.as_deref(),
            (self.settings.audio_buffer_frames > 0).then_some(self.settings.audio_buffer_frames),
        ) {
            Ok(player) => player,
            Err(error) => {
                // Keep playing the old mix.
                self.player = Some(old);
                self.mix_rendered = signature;
                self.status = Some(format!("audio: {error}"));
                return;
            }
        };
        if self.loop_enabled
            && let Some((start, end)) = self.loop_region
            && end > start
        {
            let from = self.song().tempo_map.seconds_at(start);
            let to = self.song().tempo_map.seconds_at(end);
            new.set_loop(true, from, to);
        }
        // Read the position as late as possible so the jump is not heard.
        let _ = new.seek_seconds(old.seconds());
        if old.is_playing() {
            new.play();
        }
        old.stop();
        old.release();
        self.last_loop_wraps = 0;
        self.stem_sample_rate = stem_rate;
        self.stem_levels = rendered.stems;
        self.player = Some(new);
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
    fn mixer_changes_change_the_signature() {
        let settings = crate::settings::Settings::default();
        let base = song();
        let start = mix_signature(&base, &settings);
        assert_eq!(start, mix_signature(&base.clone(), &settings));

        let mut changed = base.clone();
        changed.tracks[0].volume = changed.tracks[0].volume.wrapping_sub(10);
        assert_ne!(start, mix_signature(&changed, &settings));

        let mut changed = base.clone();
        changed.tracks[0].pan = 0;
        assert_ne!(start, mix_signature(&changed, &settings));

        let mut changed = base.clone();
        changed.tracks[0].muted = !changed.tracks[0].muted;
        assert_ne!(start, mix_signature(&changed, &settings));

        let mut changed = base.clone();
        changed.tracks[0].soloed = !changed.tracks[0].soloed;
        assert_ne!(start, mix_signature(&changed, &settings));

        let mut louder = settings.clone();
        louder.master_gain_db += 3.0;
        assert_ne!(start, mix_signature(&base, &louder));
    }

    #[test]
    fn note_edits_keep_the_signature() {
        let settings = crate::settings::Settings::default();
        let base = song();
        let mut renamed = base.clone();
        renamed.tracks[0].name = String::from("Keys");
        assert_eq!(
            mix_signature(&base, &settings),
            mix_signature(&renamed, &settings)
        );
    }
}
