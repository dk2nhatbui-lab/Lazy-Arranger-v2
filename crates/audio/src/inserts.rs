//! A track's insert plug-ins, run in the audio callback.
//!
//! [`Inserts`] holds the opened effects of one track, built on a control
//! thread with their saved state. The engine runs them on the track's lane
//! before its fader, one quantum at a time, so a plug-in edit is heard by
//! swapping in a new chain instead of rendering the song again.
//!
//! A slot that does not open as a stereo effect (an instrument, a missing
//! plug-in) is skipped, as the offline render skips it.

use std::hash::{Hash, Hasher};

use plugin_host::RealtimeEffect;
use project::song::{AutomationTarget, PluginFormat, PluginSlot, Track};

/// Frames per block when a chain runs over a whole buffer.
const BLOCK: usize = 512;

/// The opened effects of one track, in slot order.
#[derive(Debug, Default)]
pub struct Inserts {
    effects: Vec<RealtimeEffect>,
    latency: usize,
}

fn active(slot: &PluginSlot) -> bool {
    !slot.bypassed && !slot.identifier.is_empty()
}

/// True when a live track's inserts can run in the callback: it has an
/// active slot and no plug-in parameter automation, which is still rendered.
#[must_use]
pub fn can_run_live(track: &Track) -> bool {
    track.plugins.iter().any(active)
        && !track
            .automation
            .iter()
            .any(|lane| matches!(lane.target, AutomationTarget::PluginParam { .. }))
}

/// A fingerprint of everything a track's chain is built from. When it
/// changes, the chain is out of date.
#[must_use]
pub fn signature(track: &Track) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for slot in &track.plugins {
        slot.id.hash(&mut hasher);
        slot.path.hash(&mut hasher);
        slot.identifier.hash(&mut hasher);
        slot.bypassed.hash(&mut hasher);
        slot.state.hash(&mut hasher);
        matches!(slot.format, PluginFormat::Vst3).hash(&mut hasher);
    }
    hasher.finish()
}

impl Inserts {
    /// Opens every active effect slot of a track at `sample_rate`.
    #[must_use]
    pub fn build(track: &Track, sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate);
        let effects: Vec<RealtimeEffect> = track
            .plugins
            .iter()
            .filter(|slot| active(slot))
            .filter_map(|slot| match slot.format {
                PluginFormat::Vst3 => RealtimeEffect::vst3(
                    std::path::Path::new(&slot.path),
                    &slot.identifier,
                    &slot.state,
                    rate,
                    BLOCK,
                )
                .ok(),
                PluginFormat::AudioUnit => {
                    RealtimeEffect::audio_unit(&slot.identifier, &slot.state, rate).ok()
                }
            })
            .collect();
        let latency = effects
            .iter()
            .map(RealtimeEffect::latency_samples)
            .fold(0_usize, usize::saturating_add);
        Self { effects, latency }
    }

    /// Total latency of the chain in frames.
    #[must_use]
    pub fn latency(&self) -> usize {
        self.latency
    }

    /// Number of effects that opened.
    #[must_use]
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// True when no effect opened; the chain passes audio through.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Runs interleaved stereo audio through the chain in place. Never
    /// allocates. An effect that rejects a block leaves it dry.
    pub fn process(&mut self, samples: &mut [f32]) {
        for effect in &mut self.effects {
            let block = effect.max_block().max(1) * 2;
            for chunk in samples.chunks_mut(block) {
                let _ = effect.process(chunk);
            }
        }
    }

    /// Runs a whole buffer through the chain, block by block, as playback
    /// would.
    pub fn run_all(&mut self, samples: &mut [f32]) {
        for chunk in samples.chunks_mut(crate::engine::QUANTUM * 2) {
            self.process(chunk);
        }
    }
}
