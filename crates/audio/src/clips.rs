//! Audio clips played in the audio callback.
//!
//! A track's recorded audio (its active take, and the comp spans of other
//! takes) is decoded once on a control thread and shared through an
//! [`Arc`]. A [`ClipLane`] holds each placed clip: where it starts, how it
//! reads its source, its fades and gain. The callback reads the samples it
//! needs for each block, so moving a clip, trimming it or changing a fade
//! only builds a new lane from the decoded audio already in hand.
//!
//! The offline render mixes clips through the same [`Clip::mix`], so a
//! streamed clip sounds exactly like a rendered one.

use std::collections::HashMap;
use std::sync::Arc;

use project::song::{AudioRef, Song, Track};

use crate::RenderedAudio;

/// Decoded audio files by path, shared by every clip that reads them.
pub type Sources = HashMap<String, Arc<RenderedAudio>>;

/// Where a clip's samples come from.
#[derive(Clone, Debug)]
enum Data {
    /// The decoded file at its own rate, read with a rate ratio.
    Source(Arc<RenderedAudio>),
    /// A time-stretched copy at the output rate, one frame per output frame.
    Stretched(Arc<Vec<f32>>),
}

/// One placed clip, ready to mix.
#[derive(Clone, Debug)]
pub struct Clip {
    data: Data,
    /// First output frame.
    target_start: usize,
    /// Output frames it covers.
    target_frames: usize,
    /// First source frame, for unstretched audio.
    source_start: usize,
    /// Source frames per output frame.
    ratio: f64,
    fade_in: f64,
    fade_out: f64,
    gain: f32,
}

impl Clip {
    /// Places one audio reference at `sample_rate`, reading its file
    /// through `load`. None when the file is missing or the clip is empty.
    #[must_use]
    pub fn prepare(
        song: &Song,
        reference: &AudioRef,
        sample_rate: u32,
        load: &mut dyn FnMut(&AudioRef) -> Option<Arc<RenderedAudio>>,
    ) -> Option<Self> {
        if reference.length == 0 {
            return None;
        }
        let source = load(reference)?;
        if source.samples.is_empty() {
            return None;
        }
        let rate = f64::from(sample_rate.max(1));
        let start_seconds = song.tempo_map.duration_seconds(0, reference.start);
        let end_seconds = song
            .tempo_map
            .duration_seconds(0, reference.start.saturating_add(reference.length));
        let offset_seconds = song.tempo_map.duration_seconds(0, reference.offset);
        let target_start = (start_seconds * rate).round() as usize;
        let target_frames = ((end_seconds - start_seconds).max(0.0) * rate).round() as usize;
        if target_frames == 0 {
            return None;
        }
        let source_start = (offset_seconds * f64::from(source.sample_rate.max(1))).round() as usize;
        if source.frames().saturating_sub(source_start) == 0 {
            return None;
        }
        let ratio = f64::from(source.sample_rate.max(1)) / rate;
        let fade_in = if reference.fade_in > 0 {
            song.tempo_map.duration_seconds(
                reference.start,
                reference.start.saturating_add(reference.fade_in),
            ) * rate
        } else {
            0.0
        };
        let fade_out = if reference.fade_out > 0 {
            let from = reference
                .start
                .saturating_add(reference.length.saturating_sub(reference.fade_out));
            song.tempo_map
                .duration_seconds(from, reference.start.saturating_add(reference.length))
                * rate
        } else {
            0.0
        };
        // Stretch at the output rate, or a different file rate would shift
        // pitch.
        let data = if reference.stretch {
            let at_output_rate = crate::player::resample_stereo(
                RenderedAudio {
                    samples: source.samples[source_start * 2..].to_vec(),
                    sample_rate: source.sample_rate,
                },
                sample_rate,
            );
            Data::Stretched(Arc::new(crate::stretch_interleaved(
                &at_output_rate.samples,
                target_frames,
            )))
        } else {
            Data::Source(source)
        };
        Some(Self {
            data,
            target_start,
            target_frames,
            source_start,
            ratio,
            fade_in,
            fade_out,
            gain: 10.0_f32.powf(f32::from(reference.gain_mb) / 20_000.0),
        })
    }

    /// Adds the clip's part of output frames `first..first + out.len() / 2`
    /// into `out`. Never allocates.
    pub fn mix(&self, out: &mut [f32], first: usize) {
        let frames = out.len() / 2;
        let begin = first.saturating_sub(self.target_start);
        let end = (first + frames)
            .saturating_sub(self.target_start)
            .min(self.target_frames);
        for frame in begin..end {
            let source_position = self.source_start + (frame as f64 * self.ratio).round() as usize;
            let (left, right) = match &self.data {
                Data::Stretched(samples) => {
                    // An empty stretch (size overflow) just ends the clip.
                    match (samples.get(frame * 2), samples.get(frame * 2 + 1)) {
                        (Some(left), Some(right)) => (*left, *right),
                        _ => break,
                    }
                }
                Data::Source(source) => {
                    match (
                        source.samples.get(source_position * 2),
                        source.samples.get(source_position * 2 + 1),
                    ) {
                        (Some(left), Some(right)) => (*left, *right),
                        _ => break,
                    }
                }
            };
            let mut gain = 1.0_f64;
            if self.fade_in > 0.0 {
                gain = gain.min((frame as f64 / self.fade_in).min(1.0));
            }
            if self.fade_out > 0.0 {
                gain = gain.min(
                    ((self.target_frames.saturating_sub(frame)) as f64 / self.fade_out).min(1.0),
                );
            }
            let gain = gain as f32 * self.gain;
            let at = (self.target_start + frame - first) * 2;
            out[at] += left * gain;
            out[at + 1] += right * gain;
        }
    }
}

/// The references a track plays: its active take, cut around comp spans,
/// and each comp span of another take, with short crossfades at the cuts.
#[must_use]
pub fn placed(song: &Song, track: &Track) -> Vec<AudioRef> {
    let mut out = Vec::new();
    if track.comps.is_empty() {
        out.extend(track.audio.clone());
        return out;
    }
    let mut comps: Vec<&project::song::AudioComp> = track.comps.iter().collect();
    comps.sort_by_key(|comp| comp.start);
    if let Some(active) = &track.audio {
        let active_end = active.start.saturating_add(active.length);
        let mut cursor = active.start;
        let mut gaps = Vec::new();
        for comp in &comps {
            if comp.start > cursor {
                gaps.push((cursor, comp.start.min(active_end)));
            }
            cursor = cursor.max(comp.start.saturating_add(comp.length));
        }
        if cursor < active_end {
            gaps.push((cursor, active_end));
        }
        for (start, end) in gaps {
            if end <= start {
                continue;
            }
            let cross = crate::chords::to_ticks(0.005, song.tempo_map.bpm_at(start))
                .max(active.fade_in.min(end.saturating_sub(start)))
                .max(active.fade_out.min(end.saturating_sub(start)))
                .max(1);
            let mut placed = active.clone();
            placed.offset = active
                .offset
                .saturating_add(start.saturating_sub(active.start));
            placed.start = start;
            placed.length = end.saturating_sub(start).saturating_add(cross);
            placed.fade_in = placed.fade_in.max(cross);
            placed.fade_out = placed.fade_out.max(cross);
            out.push(placed);
        }
    }
    for comp in &comps {
        if let Some(take) = track.takes.get(comp.take) {
            let cross = crate::chords::to_ticks(0.005, song.tempo_map.bpm_at(comp.start))
                .max(take.fade_in.min(comp.length))
                .max(take.fade_out.min(comp.length))
                .max(1);
            let mut placed = take.clone();
            placed.start = comp.start;
            // Overlap the next span by the crossfade length so the two
            // edges cross rather than both fading to silence.
            placed.length = comp.length.saturating_add(cross);
            placed.offset = placed
                .offset
                .saturating_add(comp.start.saturating_sub(take.start).max(0));
            placed.fade_in = placed.fade_in.max(cross);
            placed.fade_out = placed.fade_out.max(cross);
            out.push(placed);
        }
    }
    out
}

/// True when a track's sound is recorded audio only (no notes), so it can
/// stream its clips in the callback. Its insert plug-ins must run live too,
/// or the lane is rendered with them.
#[must_use]
pub fn can_stream(track: &Track) -> bool {
    (track.audio.is_some() || !track.comps.is_empty())
        && track.notes.is_empty()
        && (!track
            .plugins
            .iter()
            .any(|slot| !slot.bypassed && !slot.identifier.is_empty())
            || crate::inserts::can_run_live(track))
}

/// A fingerprint of everything a track's clip lane is built from. When it
/// changes, the lane is out of date.
#[must_use]
pub fn signature(song: &Song, track: &Track) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!(
        "{:?}{:?}{:?}{:?}",
        track.audio, track.takes, track.comps, song.tempo_map
    )
    .hash(&mut hasher);
    hasher.finish()
}

/// Loads a reference's file once into `sources`.
pub fn load_into(sources: &mut Sources, reference: &AudioRef) -> Option<Arc<RenderedAudio>> {
    if let Some(found) = sources.get(&reference.path) {
        return Some(Arc::clone(found));
    }
    let decoded = Arc::new(crate::load_reference_audio(reference)?);
    sources.insert(reference.path.clone(), Arc::clone(&decoded));
    Some(decoded)
}

/// A track's clips, played in the audio callback.
#[derive(Clone, Debug)]
pub struct ClipLane {
    clips: Vec<Clip>,
    /// Frames the lane is delayed by, to line up with plug-in latency
    /// elsewhere in the song.
    delay: usize,
}

impl ClipLane {
    /// Places every clip of a track, reading files from `sources` and
    /// adding the ones it decodes. With `decode` false, a file not in
    /// `sources` makes it return None, so a control thread can build a lane
    /// without touching the disk.
    #[must_use]
    pub fn build(
        song: &Song,
        track: &Track,
        sample_rate: u32,
        delay: usize,
        sources: &mut Sources,
        decode: bool,
    ) -> Option<Self> {
        let mut clips = Vec::new();
        for reference in placed(song, track) {
            if !decode && !sources.contains_key(&reference.path) {
                return None;
            }
            if let Some(clip) = Clip::prepare(song, &reference, sample_rate, &mut |reference| {
                load_into(sources, reference)
            }) {
                clips.push(clip);
            }
        }
        Some(Self { clips, delay })
    }

    /// Writes the lane's interleaved stereo signal for output samples
    /// `pos..pos + output.len()` into `output`. Never allocates.
    pub fn render(&self, output: &mut [f32], pos: usize) {
        output.fill(0.0);
        let first = pos / 2;
        let frames = output.len() / 2;
        // Source frames start `delay` later than output frames.
        let lead = self.delay.saturating_sub(first).min(frames);
        if lead == frames {
            return;
        }
        let source = (first + lead).saturating_sub(self.delay);
        let out = &mut output[lead * 2..];
        for clip in &self.clips {
            clip.mix(out, source);
        }
    }

    /// The whole lane rendered into a new buffer of `len` samples.
    #[must_use]
    pub fn render_all(&self, len: usize) -> Vec<f32> {
        let mut out = vec![0.0; len & !1];
        for (index, chunk) in out.chunks_mut(1024).enumerate() {
            self.render(chunk, index * 1024);
        }
        out
    }
}
