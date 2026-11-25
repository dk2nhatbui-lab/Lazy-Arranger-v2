//! Loops playback: launched slots play by rendering the jam ahead of time.
//!
//! Audio here is rendered offline and then played. While a launch or stop
//! waits for its bar line, the jam as it will sound after that bar is
//! rendered in the background and loaded into a paused player. At the bar
//! line the new player starts and the old one stops.

use super::*;
use arranging::loops::{Change, Launcher};

/// Bars a jam render covers past its last change; it renders again sooner
/// when the playhead gets near the end.
const JAM_BARS: Tick = 64;

/// A jam render for the next bar line.
pub(super) struct JamNext {
    /// Bar line where it takes over.
    tick: Tick,
    /// The waiting changes it was rendered for.
    pending: std::collections::BTreeMap<TrackId, Option<usize>>,
    /// The render, while it runs.
    receiver: Option<std::sync::mpsc::Receiver<Result<RenderedSong, String>>>,
    /// The loaded, paused player.
    player: Option<audio::player::Player>,
}

impl ShellApp {
    /// The song as heard with `changes`, looping up to `JAM_BARS` past `from`.
    pub(super) fn jam_song(&self, changes: &[Change], from: Tick) -> Song {
        let bar = self.song().time_signature.bar_ticks().max(1);
        let last = changes.iter().map(|change| change.tick).max().unwrap_or(0);
        arranging::loops::jam_song(self.song(), changes, from.max(last) + bar * JAM_BARS)
    }

    /// The launch grid: the bar times the "Loops start" choice.
    fn bar(&self) -> Tick {
        self.song().time_signature.bar_ticks().max(1) * Tick::from(self.loops_quantise_bars.max(1))
    }

    /// Turns jam recording on or off. Turning it off writes what played to
    /// the timeline, as one undo step.
    pub(super) fn set_jam_recording(&mut self, on: bool) {
        if on == self.jam_recording {
            return;
        }
        if !on {
            self.finish_jam();
            return;
        }
        self.jam_recording = true;
        self.launcher.changes.clear();
        let now = self.position.max(0);
        let playing: Vec<(TrackId, usize)> = self
            .launcher
            .playing
            .iter()
            .map(|(track, scene)| (*track, *scene))
            .collect();
        for (track, scene) in playing {
            let origin = self.launcher.since.get(&track).copied().unwrap_or(now);
            self.launcher.changes.push(Change {
                track,
                scene: Some(scene),
                tick: now,
                origin,
            });
        }
        self.status = Some(String::from("recording the loops"));
    }

    /// Writes a recorded jam to the timeline.
    pub(super) fn finish_jam(&mut self) {
        if !self.jam_recording {
            return;
        }
        self.jam_recording = false;
        let changes = std::mem::take(&mut self.launcher.changes);
        let end = self.position.max(0);
        match self.editor.record_loops(&changes, end) {
            Ok(0) => self.status = Some(String::from("nothing played to record")),
            Ok(written) => {
                self.status = Some(format!("recorded {written} note(s) to the timeline"))
            }
            Err(error) => self.status = Some(error),
        }
    }

    /// Called after a press in the Loops view changed the launcher.
    pub(super) fn loops_changed(&mut self) {
        let running = self
            .player
            .as_ref()
            .is_some_and(|player| player.is_playing());
        if !running {
            // Stopped: start on the bar line at or before the playhead, so
            // later launches stay in time with this one.
            self.jam_next = None;
            if !self.launcher.active() {
                return;
            }
            let bar = self.bar();
            let at = self.position.max(0).div_euclid(bar) * bar;
            self.launcher.commit(at);
            if !self.launcher.active() {
                return;
            }
            if let Some(player) = self.player.take() {
                player.stop();
                player.release();
            }
            self.position = at;
            self.toggle_playback();
            return;
        }
        self.prepare_jam();
    }

    /// Starts rendering the jam for the next bar line, unless one for the
    /// same waiting changes already runs.
    fn prepare_jam(&mut self) {
        if self.launcher.pending.is_empty() {
            self.jam_next = None;
            return;
        }
        let bar = self.bar();
        let tick = (self.position.max(0).div_euclid(bar) + 1) * bar;
        if self
            .jam_next
            .as_ref()
            .is_some_and(|next| next.tick == tick && next.pending == self.launcher.pending)
        {
            return;
        }
        let mut after: Launcher = self.launcher.clone();
        after.commit(tick);
        if !after.active() {
            // Everything stops at the bar line; nothing to render.
            self.jam_next = Some(JamNext {
                tick,
                pending: self.launcher.pending.clone(),
                receiver: None,
                player: None,
            });
            return;
        }
        let song = self.jam_song(&after.now_playing(), tick);
        let receiver = self.spawn_render(song, false);
        self.jam_next = Some(JamNext {
            tick,
            pending: self.launcher.pending.clone(),
            receiver: Some(receiver),
            player: None,
        });
    }

    /// Each frame while playing: load a finished jam render, and switch to
    /// it at its bar line.
    pub(super) fn advance_launcher(&mut self) {
        if !self.launcher.active() {
            self.jam_next = None;
            return;
        }
        if !self.launcher.pending.is_empty() {
            let stale = self.jam_next.as_ref().is_none_or(|next| {
                next.pending != self.launcher.pending || next.tick <= self.position - self.bar()
            });
            if stale {
                self.prepare_jam();
            }
        }
        // Load a finished render into a paused player.
        if let Some(next) = &mut self.jam_next
            && let Some(receiver) = &next.receiver
        {
            match receiver.try_recv() {
                Ok(Ok(rendered)) => {
                    next.receiver = None;
                    match audio::player::Player::new_on(
                        rendered.audio,
                        self.settings.audio_output.as_deref(),
                        (self.settings.audio_buffer_frames > 0)
                            .then_some(self.settings.audio_buffer_frames),
                    ) {
                        Ok(player) => {
                            let _ = player
                                .seek_seconds(self.editor.song().tempo_map.seconds_at(next.tick));
                            next.player = Some(player);
                        }
                        Err(error) => self.status = Some(format!("audio: {error}")),
                    }
                }
                Ok(Err(error)) => {
                    next.receiver = None;
                    self.status = Some(error);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => next.receiver = None,
            }
        }
        // Switch at the bar line.
        let Some(next) = &self.jam_next else {
            return;
        };
        if self.position < next.tick {
            return;
        }
        let ready = next.receiver.is_none();
        if !ready {
            return; // the render is late; switch as soon as it lands
        }
        let Some(next) = self.jam_next.take() else {
            return;
        };
        self.launcher.commit(next.tick);
        if let Some(old) = self.player.take() {
            let now = old.seconds();
            old.stop();
            old.release();
            if let Some(player) = &next.player {
                let at = self.editor.song().tempo_map.seconds_at(next.tick);
                let _ = player.seek_seconds(now.max(at));
            }
        }
        match next.player {
            Some(player) => {
                player.play();
                self.player = Some(player);
                self.playing = true;
            }
            None => {
                self.playing = false;
                self.status = Some(String::from("loops stopped"));
            }
        }
    }

    /// Stops the loops and forgets the waiting render.
    pub(super) fn reset_loops(&mut self) {
        self.launcher.reset();
        self.jam_next = None;
    }
}
