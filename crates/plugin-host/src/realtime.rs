//! Effects prepared for the audio callback.
//!
//! A [`RealtimeEffect`] is built, and its state loaded, on a control thread.
//! It then moves to the audio thread, which only calls
//! [`RealtimeEffect::process`]: no allocation, no locks and no parameter
//! queues. It moves back to a control thread to be dropped.
//!
//! # Safety
//! The wrapped instances hold raw plug-in pointers, so they are not `Send` by
//! default. Sending one is sound because the instance is owned by exactly one
//! thread at a time and never shared: the control thread builds it, the
//! audio thread processes it, and the control thread drops it. This is the
//! VST3 and Audio Unit threading model, where processing runs on a thread
//! other than the one that created the instance. The edit controller is
//! never called from the audio thread.

use std::path::Path;

use crate::vst3_instrument::NoteEvent;
use crate::{AuEffect, Vst3Effect, Vst3Instrument};

/// The plug-in behind a [`RealtimeEffect`].
#[derive(Debug)]
enum Kind {
    Vst3(Vst3Effect),
    #[allow(dead_code, reason = "Audio Units only open on macOS")]
    Au(AuEffect),
}

/// An effect instance prepared for the audio callback.
#[derive(Debug)]
pub struct RealtimeEffect {
    kind: Kind,
    /// Planar input and output channels, at the largest block size.
    left: Vec<f32>,
    right: Vec<f32>,
    out_left: Vec<f32>,
    out_right: Vec<f32>,
    latency: usize,
}

// SAFETY: see the module notes; one owner at a time, never shared.
unsafe impl Send for RealtimeEffect {}

impl RealtimeEffect {
    /// Opens a VST3 effect, loads its saved state and prepares it for blocks
    /// of up to `max_block` frames.
    ///
    /// # Errors
    ///
    /// Returns a message when the plug-in cannot be opened, is not stereo or
    /// rejects its state.
    pub fn vst3(
        bundle: &Path,
        class_id: &str,
        state: &str,
        sample_rate: f64,
        max_block: usize,
    ) -> Result<Self, String> {
        let mut effect = Vst3Effect::new(bundle, class_id, sample_rate, max_block)?;
        if effect.channels() != 2 {
            return Err(String::from("only stereo effects run live"));
        }
        if !state.is_empty() {
            effect.load_state_base64(state)?;
        }
        let latency = effect.latency_samples();
        let block = effect.max_block();
        Ok(Self::with(Kind::Vst3(effect), block, latency))
    }

    /// Opens an Audio Unit effect by `manufacturer:subtype` and applies its
    /// saved parameter state.
    ///
    /// # Errors
    ///
    /// Returns a message when the unit cannot be opened, or always outside
    /// macOS.
    pub fn audio_unit(identifier: &str, state: &str, sample_rate: f64) -> Result<Self, String> {
        let effect = AuEffect::new(identifier, sample_rate)?;
        if !state.is_empty() {
            effect.apply_state(state);
        }
        let latency = effect.latency_samples();
        Ok(Self::with(Kind::Au(effect), 4096, latency))
    }

    fn with(kind: Kind, block: usize, latency: usize) -> Self {
        Self {
            kind,
            left: vec![0.0; block],
            right: vec![0.0; block],
            out_left: vec![0.0; block],
            out_right: vec![0.0; block],
            latency,
        }
    }

    /// Latency the plug-in reported when it was opened, in frames.
    #[must_use]
    pub fn latency_samples(&self) -> usize {
        self.latency
    }

    /// Largest block [`RealtimeEffect::process`] takes, in frames.
    #[must_use]
    pub fn max_block(&self) -> usize {
        self.left.len()
    }

    /// Processes interleaved stereo audio in place. Never allocates.
    ///
    /// Returns false, and leaves `samples` as they were, when the block is
    /// larger than [`RealtimeEffect::max_block`] or the plug-in rejects it.
    pub fn process(&mut self, samples: &mut [f32]) -> bool {
        let frames = samples.len() / 2;
        if frames > self.left.len() {
            return false;
        }
        for (frame, pair) in samples.as_chunks::<2>().0.iter().enumerate() {
            self.left[frame] = pair[0];
            self.right[frame] = pair[1];
        }
        let done = match &mut self.kind {
            Kind::Vst3(effect) => effect.process_planar(
                [&mut self.left[..frames], &mut self.right[..frames]],
                [&mut self.out_left[..frames], &mut self.out_right[..frames]],
                frames,
            ),
            Kind::Au(effect) => {
                let done = process_au(effect, &mut self.left[..frames], &mut self.right[..frames]);
                self.out_left[..frames].copy_from_slice(&self.left[..frames]);
                self.out_right[..frames].copy_from_slice(&self.right[..frames]);
                done
            }
        };
        if !done {
            return false;
        }
        for (frame, pair) in samples.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            pair[0] = self.out_left[frame];
            pair[1] = self.out_right[frame];
        }
        true
    }
}

/// A VST3 instrument prepared for the audio callback, under the same
/// threading rules as [`RealtimeEffect`].
#[derive(Debug)]
pub struct RealtimeInstrument {
    instrument: Vst3Instrument,
    left: Vec<f32>,
    right: Vec<f32>,
}

// SAFETY: see the module notes; one owner at a time, never shared.
unsafe impl Send for RealtimeInstrument {}

impl RealtimeInstrument {
    /// Opens a VST3 instrument, loads its saved state and prepares it for
    /// blocks of up to `max_block` frames.
    ///
    /// # Errors
    ///
    /// Returns a message when the plug-in cannot be opened as an
    /// instrument or rejects its state.
    pub fn vst3(
        bundle: &Path,
        class_id: &str,
        state: &str,
        sample_rate: f64,
        max_block: usize,
    ) -> Result<Self, String> {
        let mut instrument = Vst3Instrument::new(bundle, class_id, sample_rate, max_block)?;
        if !state.is_empty() {
            instrument.load_state_base64(state)?;
        }
        let block = instrument.max_block();
        Ok(Self {
            instrument,
            left: vec![0.0; block],
            right: vec![0.0; block],
        })
    }

    /// Largest block [`RealtimeInstrument::process`] takes, in frames.
    #[must_use]
    pub fn max_block(&self) -> usize {
        self.left.len()
    }

    /// Renders one block into interleaved stereo `output`, with `events`
    /// (offsets inside the block, in time order). Never allocates. Returns
    /// false, leaving `output` silent, when the block is too long or the
    /// plug-in rejects it.
    pub fn process(&mut self, output: &mut [f32], events: &[NoteEvent]) -> bool {
        let frames = output.len() / 2;
        if frames > self.left.len() {
            output.fill(0.0);
            return false;
        }
        let left = &mut self.left[..frames];
        let right = &mut self.right[..frames];
        left.fill(0.0);
        right.fill(0.0);
        let done = self.instrument.process_planar(left, right, events);
        for (frame, pair) in output.as_chunks_mut::<2>().0.iter_mut().enumerate() {
            if done {
                pair[0] = left[frame];
                pair[1] = right[frame];
            } else {
                *pair = [0.0; 2];
            }
        }
        done
    }
}

#[cfg(target_os = "macos")]
fn process_au(effect: &mut AuEffect, left: &mut [f32], right: &mut [f32]) -> bool {
    effect.process_planar(left, right)
}

#[cfg(not(target_os = "macos"))]
fn process_au(_effect: &mut AuEffect, _left: &mut [f32], _right: &mut [f32]) -> bool {
    false
}
