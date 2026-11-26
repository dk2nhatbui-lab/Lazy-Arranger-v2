//! Volume, pan and send automation, followed live by the engine.
//!
//! A track with gain automation is still mixed live. Its fader and pan
//! curve, and its send curve, are worked out on a control thread into an
//! [`Envelope`]: one value per 64 frames, as the offline render steps them.
//! The engine multiplies the lane by it as it plays, and an automation edit
//! swaps in a new envelope.

use std::hash::{Hash, Hasher};

use project::song::{AutomationTarget, Song, Track};

/// Frames per envelope step; the offline render steps gains the same way.
pub const STEP: usize = 64;

/// True when a track's fader, pan or send follows automation.
#[must_use]
pub fn automated(track: &Track) -> bool {
    track.automation.iter().any(|lane| {
        matches!(
            lane.target,
            AutomationTarget::Volume | AutomationTarget::Pan | AutomationTarget::Send
        )
    })
}

/// A fingerprint of everything a track's envelope is built from: its gain
/// lanes, static fader, pan and send, and the tempo.
#[must_use]
pub fn signature(song: &Song, track: &Track) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    track.volume.hash(&mut hasher);
    track.pan.hash(&mut hasher);
    track.send_level.hash(&mut hasher);
    for lane in &track.automation {
        if matches!(
            lane.target,
            AutomationTarget::Volume | AutomationTarget::Pan | AutomationTarget::Send
        ) {
            format!("{:?}{:?}", lane.target, lane.points).hash(&mut hasher);
        }
    }
    format!("{:?}", song.tempo_map).hash(&mut hasher);
    hasher.finish()
}

/// A track's automated gains, one step per [`STEP`] frames.
#[derive(Debug, Default)]
pub struct Envelope {
    /// Left and right fader-and-pan gain.
    gains: Vec<[f32; 2]>,
    /// Send level, 1.0 at unity.
    sends: Vec<f32>,
}

impl Envelope {
    /// Works out a track's envelope for `frames` frames at `sample_rate`.
    #[must_use]
    pub fn build(song: &Song, track: &Track, sample_rate: u32, frames: usize) -> Self {
        let steps = frames.div_ceil(STEP).max(1);
        let mut gains = Vec::with_capacity(steps);
        let mut sends = Vec::with_capacity(steps);
        for step in 0..steps {
            let tick = song
                .tempo_map
                .tick_at_samples((step * STEP) as i64, sample_rate);
            let (left, right) = crate::gains_at(track, tick);
            gains.push([left, right]);
            sends.push(track.send_at(tick).clamp(0.0, 127.0) / 100.0);
        }
        Self { gains, sends }
    }

    /// The same envelope for a stream at another sample rate.
    #[must_use]
    pub fn resample(&self, from_rate: u32, to_rate: u32) -> Self {
        if from_rate == to_rate || from_rate == 0 || to_rate == 0 {
            return Self {
                gains: self.gains.clone(),
                sends: self.sends.clone(),
            };
        }
        let steps = (self.gains.len() as u64 * u64::from(to_rate))
            .div_ceil(u64::from(from_rate))
            .max(1) as usize;
        let source =
            |step: usize| (step as u64 * u64::from(from_rate) / u64::from(to_rate)) as usize * STEP;
        Self {
            gains: (0..steps).map(|step| self.gains(source(step))).collect(),
            sends: (0..steps).map(|step| self.send(source(step))).collect(),
        }
    }

    /// Fader-and-pan gains at a frame; the last step holds past the end.
    #[inline]
    #[must_use]
    pub fn gains(&self, frame: usize) -> [f32; 2] {
        let step = (frame / STEP).min(self.gains.len().saturating_sub(1));
        self.gains.get(step).copied().unwrap_or([0.0; 2])
    }

    /// Send level at a frame.
    #[inline]
    #[must_use]
    pub fn send(&self, frame: usize) -> f32 {
        let step = (frame / STEP).min(self.sends.len().saturating_sub(1));
        self.sends.get(step).copied().unwrap_or(0.0)
    }
}
