//! Headless synthesis and WAV export.
//!
//! This is the renderer the desktop shell will play through once a realtime
//! backend exists: one deterministic function from song to samples, so tests
//! can check timing and level without opening an audio device.

pub mod analysis;
pub mod automation;
pub mod chords;
pub mod decode;
pub mod drums;
pub mod edit;
pub mod engine;
pub mod inserts;
pub mod l6max;
pub mod link;
pub mod live;
pub mod master;
pub mod metronome;
pub mod mix;
pub mod monitor;
pub mod player;
pub mod record;
pub mod soundfont;
pub mod spsc;
pub mod synth;
pub mod watchdog;
pub mod wav;
pub mod waveform;

use std::f64::consts::TAU;

use music_core::{Pitch, Velocity};
use project::song::Song;

/// Sample rate used by export and playback.
pub const SAMPLE_RATE: u32 = 48_000;
/// Stereo channel count.
pub const CHANNELS: u16 = 2;

/// A rendered stereo buffer.
#[derive(Clone, Debug, Default)]
pub struct RenderedAudio {
    /// Interleaved stereo samples in `-1.0..=1.0`.
    pub samples: Vec<f32>,
    /// Sample rate the buffer was produced at.
    pub sample_rate: u32,
}

impl RenderedAudio {
    /// A copy of the buffer between two times, clamped to its length.
    #[must_use]
    pub fn slice(&self, from_seconds: f64, to_seconds: f64) -> Self {
        let frames = self.frames();
        let rate = f64::from(self.sample_rate.max(1));
        let start = ((from_seconds.max(0.0)) * rate).round() as usize;
        let end = ((to_seconds.max(0.0)) * rate).round() as usize;
        let start = start.min(frames);
        let end = end.clamp(start, frames);
        Self {
            samples: self.samples[start * 2..end * 2].to_vec(),
            sample_rate: self.sample_rate,
        }
    }

    /// Number of frames (one frame holds both channels).
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(CHANNELS)
    }

    /// Duration in seconds.
    #[must_use]
    pub fn duration_seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    /// Encodes the buffer as a 32-bit IEEE float WAV file.
    #[must_use]
    pub fn to_wav32f(&self) -> Vec<u8> {
        let data_len = self.samples.len() * 4;
        let mut out = Vec::with_capacity(44 + data_len);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&3u16.to_le_bytes());
        out.extend_from_slice(&CHANNELS.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&(self.sample_rate * u32::from(CHANNELS) * 4).to_le_bytes());
        out.extend_from_slice(&(CHANNELS * 4).to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data_len as u32).to_le_bytes());
        for sample in &self.samples {
            // NaN or infinity would poison every player that reads the file.
            let sample = if sample.is_finite() { *sample } else { 0.0 };
            out.extend_from_slice(&sample.to_le_bytes());
        }
        out
    }

    /// Encodes the buffer as a 24-bit PCM WAV file.
    #[must_use]
    pub fn to_wav24(&self) -> Vec<u8> {
        let data_len = self.samples.len() * 3;
        let mut out = Vec::with_capacity(44 + data_len);
        let byte_rate = self.sample_rate * u32::from(CHANNELS) * 3;
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&CHANNELS.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&(CHANNELS * 3).to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data_len as u32).to_le_bytes());
        for sample in &self.samples {
            let value = (crate::spsc::sanitize(*sample) * 8_388_607.0).round() as i32;
            let bytes = value.to_le_bytes();
            out.extend_from_slice(&bytes[0..3]);
        }
        out
    }

    /// Encodes the buffer as a 16-bit PCM WAV file.
    #[must_use]
    pub fn to_wav(&self) -> Vec<u8> {
        let data_len = self.samples.len() * 2;
        let mut out = Vec::with_capacity(44 + data_len);
        let byte_rate = self.sample_rate * u32::from(CHANNELS) * 2;
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&CHANNELS.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&(CHANNELS * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data_len as u32).to_le_bytes());
        for sample in &self.samples {
            let value = (crate::spsc::sanitize(*sample) * f32::from(i16::MAX)).round() as i16;
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }
}

/// One track rendered alone.
#[derive(Clone, Debug)]
pub struct Stem {
    /// Track rendered.
    pub track: project::TrackId,
    /// Track name.
    pub name: String,
    /// Its audio.
    pub audio: RenderedAudio,
}

/// Renders each audible track alone, without buses.
#[must_use]
pub fn render_stems(song: &Song, sample_rate: u32) -> Vec<Stem> {
    song.tracks
        .iter()
        .filter(|track| song.is_audible(track))
        .map(|track| {
            let mut solo = song.clone();
            solo.tracks = vec![track.clone()];
            solo.buses.clear();
            Stem {
                track: track.id,
                name: track.name.clone(),
                audio: render(&solo, sample_rate),
            }
        })
        .collect()
}

/// Renders every track, using SoundFont presets where a track names one.
///
/// The song plays through the same engine as live playback, faster than
/// real time, so an export sounds exactly like what the user heard.
#[must_use]
pub fn render(song: &Song, sample_rate: u32) -> RenderedAudio {
    live::render(song, sample_rate).mixdown()
}

/// Renders a song through the engine with the metronome and master stage,
/// as playback hears it.
#[must_use]
pub fn render_mastered(
    song: &Song,
    sample_rate: u32,
    settings: master::Master,
    metronome: Option<f32>,
) -> RenderedAudio {
    let mut live = live::render(song, sample_rate);
    if let Some(accent) = metronome {
        metronome::mix_accent(&mut live.fixed, song, 1.0, accent);
    }
    live.apply_master(settings);
    live.mixdown()
}

/// The reference render the engine is tested against: every stage run
/// over the whole song, one after the other.
#[cfg(test)]
pub(crate) fn render_offline(song: &Song, sample_rate: u32) -> RenderedAudio {
    render_offline_with_soundfont(song, sample_rate, &mut |path| {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| soundfont::parse(&bytes).ok())
    })
}

/// Decodes one audio reference; WAV keeps stereo, other formats are mono.
fn load_reference_audio(reference: &project::song::AudioRef) -> Option<RenderedAudio> {
    let path = std::path::Path::new(&reference.path);
    let bytes = std::fs::read(path).ok()?;
    if bytes.starts_with(b"RIFF") {
        wav::decode_stereo(&bytes).ok()
    } else {
        decode::decode_stereo(path).ok()
    }
}

/// Mixes one audio reference into a track buffer with offset, stretch and fades.
fn stretch_interleaved(input: &[f32], target_frames: usize) -> Vec<f32> {
    const FRAME: usize = 1_024;
    const HOP: usize = 512;
    const SEARCH: usize = 128;
    let source_frames = input.len() / 2;
    let Some(output_len) = target_frames.checked_mul(2) else {
        return Vec::new();
    };
    if source_frames < 16 || target_frames == 0 {
        return vec![0.0; output_len];
    }
    let ratio = source_frames as f64 / target_frames as f64;
    let mut output = vec![0.0_f32; output_len];
    let mut previous = vec![0.0_f32; HOP.min(source_frames) * 2];
    let mut has_previous = false;
    let mut out_frame = 0_usize;
    while out_frame < target_frames {
        let nominal = (out_frame as f64 * ratio).round() as usize;
        let mut best_start = nominal.min(source_frames.saturating_sub(1));
        if has_previous {
            let mut best = f64::MIN;
            let span = SEARCH.min(nominal);
            for offset in (0..=span * 2).step_by(4) {
                let candidate = nominal.saturating_sub(span).saturating_add(offset);
                let available = source_frames.saturating_sub(candidate);
                let length = HOP.min(available).min(previous.len() / 2);
                if length == 0 {
                    continue;
                }
                let mut score = 0.0_f64;
                for index in 0..length {
                    score +=
                        f64::from(previous[index * 2]) * f64::from(input[(candidate + index) * 2]);
                }
                if score > best {
                    best = score;
                    best_start = candidate;
                }
            }
        }
        let available = source_frames.saturating_sub(best_start);
        let length = FRAME
            .min(available)
            .min(target_frames.saturating_sub(out_frame));
        if length == 0 {
            break;
        }
        let current = &input[best_start * 2..(best_start + length) * 2];
        // The previous tail can be shorter than a hop near the end.
        let overlap = if has_previous {
            HOP.min(length).min(previous.len() / 2)
        } else {
            0
        };
        for index in 0..overlap {
            let weight = index as f32 / overlap as f32;
            output[(out_frame + index) * 2] =
                previous[index * 2] * (1.0 - weight) + current[index * 2] * weight;
            output[(out_frame + index) * 2 + 1] =
                previous[index * 2 + 1] * (1.0 - weight) + current[index * 2 + 1] * weight;
        }
        for index in overlap..length {
            output[(out_frame + index) * 2] = current[index * 2];
            output[(out_frame + index) * 2 + 1] = current[index * 2 + 1];
        }
        let tail_start = length.saturating_sub(HOP);
        previous.clear();
        previous.extend_from_slice(&current[tail_start * 2..]);

        has_previous = true;
        // A short window (tiny source) moves on by its own length, so no
        // silent gap is left behind it.
        out_frame += HOP.min(length);
    }
    output
}

fn mix_audio_reference(
    song: &Song,
    track_buffer: &mut [f32],
    reference: &project::song::AudioRef,
    sample_rate: u32,
) {
    let Some(source) = load_reference_audio(reference) else {
        return;
    };
    if source.samples.is_empty() || reference.length == 0 {
        return;
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
        return;
    }
    let source_start = (offset_seconds * f64::from(source.sample_rate.max(1))).round() as usize;
    let source_frames = source.frames().saturating_sub(source_start);
    if source_frames == 0 {
        return;
    }
    // Stretch at the output rate, or a different file rate would shift pitch.
    let stretched = reference.stretch.then(|| {
        let at_output_rate = player::resample_stereo(
            RenderedAudio {
                samples: source.samples[source_start * 2..].to_vec(),
                sample_rate: source.sample_rate,
            },
            sample_rate,
        );
        stretch_interleaved(&at_output_rate.samples, target_frames)
    });
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
    let clip_gain = 10.0_f32.powf(f32::from(reference.gain_mb) / 20_000.0);
    for frame in 0..target_frames {
        let target = target_start + frame;
        if target >= track_buffer.len() / usize::from(CHANNELS) {
            break;
        }
        let source_position = source_start + (frame as f64 * ratio).round() as usize;
        if stretched.is_none() && source_position >= source.frames() {
            break;
        }
        let mut gain = 1.0_f64;
        if fade_in > 0.0 {
            gain = gain.min((frame as f64 / fade_in).min(1.0));
        }
        if fade_out > 0.0 {
            gain = gain.min(((target_frames.saturating_sub(frame)) as f64 / fade_out).min(1.0));
        }
        let gain = gain as f32 * clip_gain;
        let (sample_left, sample_right) = if let Some(stretched) = &stretched {
            // An empty stretch (size overflow) just ends the clip.
            match (stretched.get(frame * 2), stretched.get(frame * 2 + 1)) {
                (Some(left), Some(right)) => (*left, *right),
                _ => break,
            }
        } else {
            (
                source.samples[source_position * 2],
                source.samples[source_position * 2 + 1],
            )
        };
        track_buffer[target * 2] += sample_left * gain;
        track_buffer[target * 2 + 1] += sample_right * gain;
    }
}

/// Renders every track through the engine, loading SoundFonts through `load`.
#[must_use]
pub fn render_with_soundfont(
    song: &Song,
    sample_rate: u32,
    load: &mut dyn FnMut(&str) -> Option<soundfont::SoundFont>,
) -> RenderedAudio {
    live::render_with_soundfont(song, sample_rate, load).mixdown()
}

/// The offline reference render, loading SoundFonts through `load`.
#[cfg(test)]
pub(crate) fn render_offline_with_soundfont(
    song: &Song,
    sample_rate: u32,
    load: &mut dyn FnMut(&str) -> Option<soundfont::SoundFont>,
) -> RenderedAudio {
    render_parts(song, sample_rate, load, &LivePlan::default()).fixed
}

/// Renders the song, keeping the tracks marked in `live` out of the mix.
///
/// Each live track comes back as its own buffer before the fader, pan,
/// bus and master stages, so playback can apply those gains while it plays.
/// A live track is rendered even when it is muted, so unmuting it is
/// instant. The other tracks are mixed as usual into the first buffer.
///
/// A live track also marked in `skip` is played by a live synth lane, so it
/// is not rendered here and its lane buffer stays empty.
pub(crate) fn render_parts(
    song: &Song,
    sample_rate: u32,
    load: &mut dyn FnMut(&str) -> Option<soundfont::SoundFont>,
    plan: &LivePlan<'_>,
) -> Parts {
    let LivePlan {
        live,
        skip,
        dry,
        buses: live_buses,
        master: live_master,
    } = *plan;
    let bus_is_live = |index: usize| live_buses.get(index).copied().unwrap_or(false);
    let is_live = |index: usize| live.get(index).copied().unwrap_or(false);
    let wants_dry = |index: usize| is_live(index) && dry.get(index).copied().unwrap_or(false);
    let is_skipped = |index: usize| is_live(index) && skip.get(index).copied().unwrap_or(false);
    let mut end_tick = song.end_tick();
    for track in &song.tracks {
        if let Some(reference) = &track.audio {
            end_tick = end_tick.max(reference.start.saturating_add(reference.length));
        }
    }
    let track_latencies: Vec<usize> = song
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            if is_live(index) || song.is_audible(track) {
                chain_latency(&track.plugins, sample_rate)
            } else {
                0
            }
        })
        .collect();
    let max_track_latency = track_latencies.iter().copied().max().unwrap_or(0);
    let bus_latencies: Vec<usize> = song
        .buses
        .iter()
        .map(|bus| chain_latency(&bus.plugins, sample_rate))
        .collect();
    let max_bus_latency = bus_latencies.iter().copied().max().unwrap_or(0);
    let total_latency = max_track_latency.saturating_add(max_bus_latency);
    let latency_seconds = total_latency as f64 / f64::from(sample_rate);
    let duration = song.tempo_map.duration_seconds(0, end_tick.max(1)) + latency_seconds + 0.75;
    let frames = (duration * f64::from(sample_rate)).ceil() as usize;
    let mut samples = vec![0.0_f32; frames * usize::from(CHANNELS)];
    let mut bus_buffers: Vec<Vec<f32>> = song
        .buses
        .iter()
        .map(|_| vec![0.0_f32; frames * usize::from(CHANNELS)])
        .collect();
    let mut lanes = Vec::new();
    let mut dry_lanes = Vec::new();
    for (track_index, track) in song
        .tracks
        .iter()
        .enumerate()
        .filter(|(index, track)| is_live(*index) || song.is_audible(track))
    {
        if is_skipped(track_index) {
            lanes.push(Vec::new());
            dry_lanes.push(false);
            continue;
        }
        let timbre = timbre_for(track);
        let sampled = track.soundfont.as_ref().and_then(|reference| {
            load(&reference.path).map(|font| (font, reference.bank, reference.program))
        });
        let mut track_buffer = vec![0.0_f32; frames * usize::from(CHANNELS)];
        if track.comps.is_empty() {
            if let Some(reference) = &track.audio {
                mix_audio_reference(song, &mut track_buffer, reference, sample_rate);
            }
        } else {
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
                    let cross = chords::to_ticks(0.005, song.tempo_map.bpm_at(start))
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
                    mix_audio_reference(song, &mut track_buffer, &placed, sample_rate);
                }
            }
            for comp in &comps {
                if let Some(take) = track.takes.get(comp.take) {
                    let cross = chords::to_ticks(0.005, song.tempo_map.bpm_at(comp.start))
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
                    mix_audio_reference(song, &mut track_buffer, &placed, sample_rate);
                }
            }
        }
        let mut plugin_instrument = false;
        if let Some(mut instrument) = instrument_for(track, sample_rate) {
            plugin_instrument = true;
            render_instrument(
                song,
                track,
                &mut instrument,
                &mut track_buffer,
                frames,
                sample_rate,
            );
        } else {
            for note in &track.notes {
                let patch = track.patch.map(|mut patch| {
                    if let Some(brightness) = track.brightness_at(note.start) {
                        patch.brightness = brightness;
                    }
                    patch
                });
                let start_seconds = song.tempo_map.seconds_at(note.start);
                let end_seconds = song
                    .tempo_map
                    .seconds_at(note.start.saturating_add(note.duration.max(1)));
                let start = (start_seconds * f64::from(sample_rate)).round() as usize;
                let stop = (end_seconds * f64::from(sample_rate)).round() as usize;
                let frequency = frequency(note.pitch);
                let amplitude = f64::from(note.velocity.as_u8()) / 127.0 * 0.18;
                let release = (0.08 * f64::from(sample_rate)) as usize;
                let note_seconds = (stop - start) as f64 / f64::from(sample_rate);
                if let Some((font, bank, program)) = &sampled {
                    let channels = soundfont::render_stereo(
                        font,
                        *bank,
                        *program,
                        note.pitch.as_u8(),
                        note.velocity.as_u8(),
                        note_seconds as f32,
                        sample_rate,
                    )
                    .or_else(|| {
                        soundfont::render_note(
                            font,
                            *bank,
                            *program,
                            note.pitch.as_u8(),
                            note.velocity.as_u8(),
                            note_seconds as f32,
                            sample_rate,
                        )
                        .map(|mono| (mono.clone(), mono))
                    });
                    if let Some((left_audio, right_audio)) = channels {
                        for offset in 0..left_audio.len().min(right_audio.len()) {
                            let frame = start + offset;
                            if frame >= frames {
                                break;
                            }
                            track_buffer[frame * 2] += left_audio[offset];
                            track_buffer[frame * 2 + 1] += right_audio[offset];
                        }
                        continue;
                    }
                }
                for frame in start..stop.min(frames) {
                    let t = (frame - start) as f64 / f64::from(sample_rate);
                    let remaining = stop.saturating_sub(frame);
                    let envelope = if remaining < release {
                        remaining as f64 / release as f64
                    } else {
                        1.0
                    };
                    let voice = match patch {
                        Some(patch) => patch.sample(frequency, t as f32, note_seconds as f32),
                        None => sample(timbre, frequency, t, note_seconds, note.pitch.as_u8()),
                    };
                    let value = (voice * amplitude * envelope) as f32;
                    track_buffer[frame * 2] += value;
                    track_buffer[frame * 2 + 1] += value;
                }
            }
        }
        // A track whose sound comes from a plug-in instrument keeps its
        // inserts here, so the live chain never meets the instrument slot.
        let leave_dry = wants_dry(track_index) && !plugin_instrument;
        if !leave_dry {
            apply_track_plugins(song, track, &mut track_buffer, sample_rate);
        }
        delay_buffer(
            &mut track_buffer,
            max_track_latency.saturating_sub(track_latencies[track_index]),
        );
        if is_live(track_index) {
            // The engine delays the direct route by the bus compensation,
            // and live buses by their own share of it.
            lanes.push(track_buffer);
            dry_lanes.push(leave_dry);
            continue;
        }
        // The fader and pan come after the inserts, as on a console.
        apply_fader(song, track, &mut track_buffer, sample_rate);
        if let Some(index) = track
            .send
            .and_then(|id| song.buses.iter().position(|bus| bus.id == id))
        {
            let mut frame = 0_usize;
            let mut level = 1.0_f32;
            while frame < frames {
                if frame.is_multiple_of(64) {
                    let tick = song.tempo_map.tick_at_samples(frame as i64, sample_rate);
                    level = track.send_at(tick).clamp(0.0, 127.0) / 100.0;
                }
                bus_buffers[index][frame * 2] += track_buffer[frame * 2] * level;
                bus_buffers[index][frame * 2 + 1] += track_buffer[frame * 2 + 1] * level;
                frame += 1;
            }
        }
        if let Some(index) = track
            .bus
            .and_then(|id| song.buses.iter().position(|bus| bus.id == id))
        {
            for (target, value) in bus_buffers[index].iter_mut().zip(&track_buffer) {
                *target += *value;
            }
        } else if max_bus_latency > 0 {
            // Direct tracks bypass the bus chain, so delay them by the bus
            // compensation their bus-routed peers receive.
            let delay = max_bus_latency.saturating_mul(2);
            for (target, value) in samples.iter_mut().skip(delay).zip(&track_buffer) {
                *target += *value;
            }
        } else {
            for (target, value) in samples.iter_mut().zip(&track_buffer) {
                *target += *value;
            }
        }
    }
    let any_bus_solo = song.buses.iter().any(|bus| bus.soloed);
    for (index, bus) in song.buses.iter().enumerate() {
        if bus.muted || (any_bus_solo && !bus.soloed) || bus_is_live(index) {
            continue;
        }
        let (left_gain, right_gain) = gains(f32::from(bus.volume), bus.pan);
        if let Some(buffer) = bus_buffers.get_mut(index) {
            apply_plugins(&bus.plugins, buffer, sample_rate);
            delay_buffer(buffer, max_bus_latency.saturating_sub(bus_latencies[index]));
            for (frame, pair) in buffer.chunks(2).enumerate() {
                samples[frame * 2] += pair[0] * left_gain;
                samples[frame * 2 + 1] += pair[1] * right_gain;
            }
        }
    }
    if !live_master {
        apply_plugins(&song.master_plugins, &mut samples, sample_rate);
    }
    if live_master {
        // The fixed part is summed with the live tracks later, so only
        // clean it here; clamping it alone would clip a different mix.
        for sample in samples.iter_mut().chain(lanes.iter_mut().flatten()) {
            if !sample.is_finite() {
                *sample = 0.0;
            }
        }
    } else {
        for sample in &mut samples {
            *sample = crate::spsc::sanitize(*sample);
        }
    }
    Parts {
        fixed: RenderedAudio {
            samples,
            sample_rate,
        },
        lanes,
        dry: dry_lanes,
        delay: max_track_latency,
        bus_delay: max_bus_latency,
        bus_latencies,
    }
}

/// What `render_parts` leaves to the engine. Every slice is indexed like
/// the song's tracks or buses; a missing entry means false.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LivePlan<'a> {
    /// Tracks mixed live, rendered into lanes of their own.
    pub(crate) live: &'a [bool],
    /// Live tracks played by a synth lane, not rendered.
    pub(crate) skip: &'a [bool],
    /// Live tracks whose insert plug-ins run in the engine.
    pub(crate) dry: &'a [bool],
    /// Buses whose plug-ins and gains run in the engine.
    pub(crate) buses: &'a [bool],
    /// Whether the master plug-ins run in the engine.
    pub(crate) master: bool,
}

/// The result of `render_parts`.
pub(crate) struct Parts {
    /// Every track that is not live, mixed.
    pub(crate) fixed: RenderedAudio,
    /// One buffer per live track; empty for a skipped one.
    pub(crate) lanes: Vec<Vec<f32>>,
    /// Per lane: true when its insert plug-ins were left out, to run live.
    pub(crate) dry: Vec<bool>,
    /// Frames a live track without plug-ins is delayed by, to line up with
    /// the latency of track plug-ins elsewhere.
    pub(crate) delay: usize,
    /// Frames the engine delays the direct route to the master by, to line
    /// up with the latency of bus plug-ins.
    pub(crate) bus_delay: usize,
    /// Latency of each bus's plug-ins, in frames.
    pub(crate) bus_latencies: Vec<usize>,
}

/// Applies a track's fader and pan, following automation, to its buffer.
fn apply_fader(song: &Song, track: &project::song::Track, buffer: &mut [f32], sample_rate: u32) {
    let mut gains = track_gains(track);
    for (frame, pair) in buffer.as_chunks_mut::<2>().0.iter_mut().enumerate() {
        if frame.is_multiple_of(64) {
            let tick = song.tempo_map.tick_at_samples(frame as i64, sample_rate);
            gains = gains_at(track, tick);
        }
        pair[0] *= gains.0;
        pair[1] *= gains.1;
    }
}

enum TrackInstrument {
    Vst3(plugin_host::Vst3Instrument),
    #[cfg(target_os = "macos")]
    Au(plugin_host::AuInstrument),
}

impl TrackInstrument {
    fn note_on_at(&mut self, key: u8, velocity: u8, offset: u32) {
        match self {
            Self::Vst3(instrument) => instrument.note_on_at(key, velocity, offset),
            #[cfg(target_os = "macos")]
            Self::Au(instrument) => instrument.note_on_at(key, velocity, offset),
        }
    }

    fn note_off_at(&mut self, key: u8, offset: u32) {
        match self {
            Self::Vst3(instrument) => instrument.note_off_at(key, offset),
            #[cfg(target_os = "macos")]
            Self::Au(instrument) => instrument.note_off_at(key, offset),
        }
    }

    fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<(), String> {
        match self {
            Self::Vst3(instrument) => instrument.process_block(left, right),
            #[cfg(target_os = "macos")]
            Self::Au(instrument) => instrument.process_block(left, right),
        }
    }
}

fn instrument_for(track: &project::song::Track, sample_rate: u32) -> Option<TrackInstrument> {
    for slot in &track.plugins {
        if slot.bypassed || slot.identifier.is_empty() {
            continue;
        }
        match slot.format {
            project::song::PluginFormat::Vst3 => {
                if let Ok(mut instrument) = plugin_host::Vst3Instrument::new(
                    std::path::Path::new(&slot.path),
                    &slot.identifier,
                    f64::from(sample_rate),
                    512,
                ) {
                    if !slot.state.is_empty() {
                        let _ = instrument.load_state_base64(&slot.state);
                    }
                    return Some(TrackInstrument::Vst3(instrument));
                }
            }
            #[cfg(target_os = "macos")]
            project::song::PluginFormat::AudioUnit => {
                if let Ok(instrument) =
                    plugin_host::AuInstrument::new(&slot.identifier, f64::from(sample_rate))
                {
                    instrument.apply_state(&slot.state);
                    return Some(TrackInstrument::Au(instrument));
                }
            }
            #[cfg(not(target_os = "macos"))]
            project::song::PluginFormat::AudioUnit => {}
        }
    }
    None
}

fn render_instrument(
    song: &Song,
    track: &project::song::Track,
    instrument: &mut TrackInstrument,
    buffer: &mut [f32],
    frames: usize,
    sample_rate: u32,
) {
    struct Event {
        frame: usize,
        on: bool,
        key: u8,
        velocity: u8,
    }
    let mut events = Vec::new();
    for note in &track.notes {
        let start =
            (song.tempo_map.seconds_at(note.start) * f64::from(sample_rate)).round() as usize;
        let stop = (song
            .tempo_map
            .seconds_at(note.start.saturating_add(note.duration.max(1)))
            * f64::from(sample_rate))
        .round() as usize;
        if start < frames {
            events.push(Event {
                frame: start,
                on: true,
                key: note.pitch.as_u8(),
                velocity: note.velocity.as_u8(),
            });
        }
        if stop < frames {
            events.push(Event {
                frame: stop,
                on: false,
                key: note.pitch.as_u8(),
                velocity: 0,
            });
        }
    }
    events.sort_by_key(|event| (event.frame, event.on));
    let mut position = 0;
    let mut next = 0;
    while position < frames {
        let block = (frames - position).min(512);
        while next < events.len() && events[next].frame < position + block {
            let event = &events[next];
            let offset = u32::try_from(event.frame.saturating_sub(position)).unwrap_or(0);
            if event.on {
                instrument.note_on_at(event.key, event.velocity, offset);
            } else {
                instrument.note_off_at(event.key, offset);
            }
            next += 1;
        }
        let mut left = vec![0.0_f32; block];
        let mut right = vec![0.0_f32; block];
        if instrument.process_block(&mut left, &mut right).is_err() {
            return;
        }
        for index in 0..block {
            buffer[(position + index) * 2] += left[index];
            buffer[(position + index) * 2 + 1] += right[index];
        }
        position += block;
    }
    for note in &track.notes {
        instrument.note_off_at(note.pitch.as_u8(), 0);
    }
}

fn chain_latency(slots: &[project::song::PluginSlot], sample_rate: u32) -> usize {
    let mut latency = 0_usize;
    for slot in slots {
        if slot.bypassed || slot.identifier.is_empty() {
            continue;
        }
        match slot.format {
            project::song::PluginFormat::Vst3 => {
                if let Ok(effect) = plugin_host::Vst3Effect::new(
                    std::path::Path::new(&slot.path),
                    &slot.identifier,
                    f64::from(sample_rate),
                    512,
                ) {
                    latency = latency.saturating_add(effect.latency_samples());
                }
            }
            #[cfg(target_os = "macos")]
            project::song::PluginFormat::AudioUnit => {
                if let Ok(effect) =
                    plugin_host::AuEffect::new(&slot.identifier, f64::from(sample_rate))
                {
                    latency = latency.saturating_add(effect.latency_samples());
                }
            }
            #[cfg(not(target_os = "macos"))]
            project::song::PluginFormat::AudioUnit => {}
        }
    }
    latency
}

fn delay_buffer(buffer: &mut [f32], frames: usize) {
    if frames == 0 || buffer.is_empty() {
        return;
    }
    let samples = frames.saturating_mul(2).min(buffer.len());
    if samples == buffer.len() {
        buffer.fill(0.0);
        return;
    }
    buffer.copy_within(0..buffer.len() - samples, samples);
    buffer[..samples].fill(0.0);
}

fn apply_track_plugins(
    song: &Song,
    track: &project::song::Track,
    buffer: &mut [f32],
    sample_rate: u32,
) {
    for slot in &track.plugins {
        if slot.bypassed || slot.identifier.is_empty() {
            continue;
        }
        let lanes: Vec<&project::song::Automation> = track
            .automation
            .iter()
            .filter(|lane| {
                matches!(
                    lane.target,
                    project::song::AutomationTarget::PluginParam { slot: id, .. } if id == slot.id
                )
            })
            .collect();
        match slot.format {
            project::song::PluginFormat::Vst3 => {
                let Ok(mut effect) = plugin_host::Vst3Effect::new(
                    std::path::Path::new(&slot.path),
                    &slot.identifier,
                    f64::from(sample_rate),
                    512,
                ) else {
                    continue;
                };
                if !slot.state.is_empty() {
                    let _ = effect.load_state_base64(&slot.state);
                }
                if lanes.is_empty() {
                    let _ = effect.process_stereo(buffer);
                    continue;
                }
                let mut frame = 0_usize;
                for block in buffer.chunks_mut(512 * 2) {
                    let tick = song.tempo_map.tick_at_samples(frame as i64, sample_rate);
                    for lane in &lanes {
                        if let project::song::AutomationTarget::PluginParam { parameter, .. } =
                            lane.target
                            && let Some(value) = lane.value_at(tick)
                        {
                            effect.set_parameter(parameter, f64::from(value / 127.0));
                        }
                    }
                    let _ = effect.process_stereo(block);
                    frame += block.len() / 2;
                }
            }
            #[cfg(target_os = "macos")]
            project::song::PluginFormat::AudioUnit => {
                let Ok(mut effect) =
                    plugin_host::AuEffect::new(&slot.identifier, f64::from(sample_rate))
                else {
                    continue;
                };
                if lanes.is_empty() {
                    let _ = effect.process_stereo(buffer);
                    continue;
                }
                let ranges = effect.parameters();
                let mut frame = 0_usize;
                for block in buffer.chunks_mut(512 * 2) {
                    let tick = song.tempo_map.tick_at_samples(frame as i64, sample_rate);
                    for lane in &lanes {
                        if let project::song::AutomationTarget::PluginParam { parameter, .. } =
                            lane.target
                            && let Some(value) = lane.value_at(tick)
                            && let Some((_, _, min, max, _, _)) =
                                ranges.iter().find(|entry| entry.0 == parameter)
                        {
                            let raw = min + (max - min) * value / 127.0;
                            let _ = effect.set_parameter(parameter, raw);
                        }
                    }
                    let _ = effect.process_stereo(block);
                    frame += block.len() / 2;
                }
            }
            #[cfg(not(target_os = "macos"))]
            project::song::PluginFormat::AudioUnit => {}
        }
    }
}

fn apply_plugins(slots: &[project::song::PluginSlot], buffer: &mut [f32], sample_rate: u32) {
    for slot in slots {
        if slot.bypassed || slot.identifier.is_empty() {
            continue;
        }
        match slot.format {
            project::song::PluginFormat::Vst3 => {
                let Ok(mut effect) = plugin_host::Vst3Effect::new(
                    std::path::Path::new(&slot.path),
                    &slot.identifier,
                    f64::from(sample_rate),
                    512,
                ) else {
                    continue;
                };
                if !slot.state.is_empty() {
                    let _ = effect.load_state_base64(&slot.state);
                }
                if effect.channels() == 2 {
                    let _ = effect.process_stereo(buffer);
                }
            }
            #[cfg(target_os = "macos")]
            project::song::PluginFormat::AudioUnit => {
                let Ok(mut effect) =
                    plugin_host::AuEffect::new(&slot.identifier, f64::from(sample_rate))
                else {
                    continue;
                };
                let _ = effect.process_stereo(buffer);
            }
            #[cfg(not(target_os = "macos"))]
            project::song::PluginFormat::AudioUnit => {}
        }
    }
}

/// Equal-tempered frequency for a MIDI pitch.
#[must_use]
pub fn frequency(pitch: Pitch) -> f64 {
    440.0 * 2.0_f64.powf((f64::from(pitch.as_u8()) - 69.0) / 12.0)
}

/// Left and right gains for a track's fader and pan at a tick.
#[must_use]
pub fn gains_at(track: &project::song::Track, tick: music_core::Tick) -> (f32, f32) {
    gains(
        track.volume_at(tick),
        track.pan_at(tick).clamp(0.0, 127.0) as u8,
    )
}

/// Left and right gains for a volume and pan.
#[must_use]
pub fn gains(volume: f32, pan: u8) -> (f32, f32) {
    let volume = volume.min(127.0) / 100.0;
    let pan = pan.min(127);
    let left = if pan <= 64 {
        1.0
    } else {
        f32::from(127 - pan) / 63.0
    };
    let right = if pan >= 64 {
        1.0
    } else {
        f32::from(pan) / 64.0
    };
    (volume * left, volume * right)
}

/// Left and right gains with the static fader and pan.
#[must_use]
pub fn track_gains(track: &project::song::Track) -> (f32, f32) {
    gains_at(track, 0)
}

/// The synth voice family for a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timbre {
    /// Bright, decaying harmonics.
    Piano,
    /// Warm, sustained low harmonics.
    Bass,
    /// Steady drawbar-like harmonics.
    Organ,
    /// Soft, slow-attack bowed tone.
    Strings,
    /// Slow, sustained pad.
    Pad,
    /// Pitched percussion on channel 10.
    Drums,
    /// Fallback sine with two harmonics.
    Default,
}

/// Chooses a timbre from the track's channel, program and name.
#[must_use]
pub fn timbre_for(track: &project::song::Track) -> Timbre {
    let name = track.name.to_ascii_lowercase();
    let drums = track
        .notes
        .first()
        .is_some_and(|note| note.channel.zero_based() == 9)
        || name.contains("drum")
        || name.contains("kit")
        || name.contains("perc");
    if drums {
        return Timbre::Drums;
    }
    match track.program {
        Some(0..=7) => Timbre::Piano,
        Some(16..=23) => Timbre::Organ,
        Some(32..=39) => Timbre::Bass,
        Some(40..=43) => Timbre::Strings,
        Some(48..=55) => Timbre::Strings,
        Some(88..=95) => Timbre::Pad,
        _ => {
            if name.contains("bass") {
                Timbre::Bass
            } else if name.contains("pad") {
                Timbre::Pad
            } else if name.contains("string") {
                Timbre::Strings
            } else if name.contains("organ") {
                Timbre::Organ
            } else if name.contains("piano") || name.contains("ep") {
                Timbre::Piano
            } else {
                Timbre::Default
            }
        }
    }
}

fn sample(timbre: Timbre, frequency: f64, seconds: f64, duration: f64, pitch: u8) -> f64 {
    let phase = TAU * frequency * seconds;
    let attack = |time: f64| (seconds / time).min(1.0);
    match timbre {
        Timbre::Piano => {
            let decay = (-2.5 * seconds).exp();
            attack(0.004)
                * decay
                * ((phase).sin() + 0.42 * (2.0 * phase).sin() + 0.18 * (3.0 * phase).sin())
        }
        Timbre::Bass => {
            let decay = (-1.2 * seconds).exp();
            attack(0.006)
                * decay
                * ((phase).sin() + 0.5 * (2.0 * phase).sin() + 0.22 * (3.0 * phase).sin())
        }
        Timbre::Organ => {
            attack(0.01)
                * ((phase).sin()
                    + 0.55 * (2.0 * phase).sin()
                    + 0.35 * (3.0 * phase).sin()
                    + 0.2 * (4.0 * phase).sin())
        }
        Timbre::Strings => {
            let swell = attack(0.12) * (1.0 - (-1.5 * seconds).exp() * 0.2);
            swell
                * ((phase).sin()
                    + 0.3 * (2.0 * phase).sin()
                    + 0.12 * (3.0 * phase).sin()
                    + 0.06 * (5.0 * phase).sin())
        }
        Timbre::Pad => {
            attack(0.2)
                * ((phase).sin()
                    + 0.18 * (2.0 * phase).sin()
                    + 0.08 * (3.0 * phase).sin()
                    + 0.04 * (4.0 * phase).sin())
                * (-0.3 * seconds / duration.max(0.1)).exp()
        }
        Timbre::Drums => drum(seconds, pitch),
        Timbre::Default => attack(0.005) * (phase.sin() + 0.25 * (2.0 * phase).sin()),
    }
}

fn drum(seconds: f64, pitch: u8) -> f64 {
    match pitch {
        35 | 36 => {
            // Kick: a short low sine sweep with a click.
            let frequency = 120.0 * (-30.0 * seconds).exp() + 45.0;
            let phase = TAU * frequency * seconds;
            phase.sin() * (-9.0 * seconds).exp()
        }
        38 | 40 => {
            // Snare: noise plus a little body.
            noise(seconds) * (-18.0 * seconds).exp()
                + 0.25 * (TAU * 190.0 * seconds).sin() * (-14.0 * seconds).exp()
        }
        _ => noise(seconds) * (-28.0 * seconds).exp(),
    }
}

fn noise(seconds: f64) -> f64 {
    let mut state = (seconds * 48_000.0) as u64 ^ 0x9E37_79B9_7F4A_7C15;
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    (state as f64 / u64::MAX as f64) * 2.0 - 1.0
}

/// Converts a pitch and velocity into an approximate peak amplitude.
#[must_use]
pub fn peak_for(velocity: Velocity) -> f32 {
    f32::from(velocity.as_u8()) / 127.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use music_core::Channel;
    use project::song::{Note, Track};
    use project::{NoteId, TrackId};

    fn song_with_note() -> Song {
        let mut song = Song::new("Audio", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Piano");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: music_core::PPQ,
            pitch: Pitch::from_name("A4").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(track);
        song
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn find_test_plugin_dylib() -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let name = if cfg!(target_os = "macos") {
            "liblazy_arranger_test_plugin.dylib"
        } else {
            "liblazy_arranger_test_plugin.so"
        };
        let profile_dir = exe.ancestors().skip(1).find(|dir| {
            dir.join("deps").is_dir()
                || dir.join("build").join("lazy-arranger-test-plugin").is_dir()
        })?;
        let mut candidates = Vec::new();
        for direct in [profile_dir.join("deps").join(name), profile_dir.join(name)] {
            if direct.is_file() {
                candidates.push(direct);
            }
        }
        if let Ok(entries) =
            std::fs::read_dir(profile_dir.join("build").join("lazy-arranger-test-plugin"))
        {
            for entry in entries.flatten() {
                let candidate = entry.path().join("out").join(name);
                if candidate.is_file() {
                    candidates.push(candidate);
                }
            }
        }
        candidates.into_iter().max_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok()
        })
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn test_gain_bundle(tag: &str) -> Option<std::path::PathBuf> {
        let library = find_test_plugin_dylib()?;
        let root = std::env::temp_dir().join(format!(
            "lazy-arranger-pdc-plugin-{}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let (folder, name) = if cfg!(target_os = "macos") {
            ("MacOS", "Test")
        } else if cfg!(target_arch = "aarch64") {
            ("aarch64-linux", "Test.so")
        } else {
            ("x86_64-linux", "Test.so")
        };
        let binary = root.join("Test.vst3").join("Contents").join(folder);
        std::fs::create_dir_all(&binary).ok()?;
        std::fs::copy(&library, binary.join(name)).ok()?;
        plugin_host::approve_vst3_bundle(&root.join("Test.vst3"));
        Some(root.join("Test.vst3"))
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn latency_compensation_aligns_a_delayed_plugin_track() {
        use project::song::{PluginFormat, PluginSlot};
        let Some(bundle) = test_gain_bundle("pdc") else {
            return;
        };
        let classes = plugin_host::scan_vst3(&bundle).expect("scan");
        assert!(!classes.is_empty());
        let mut song = song_with_note();
        song.tracks[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("Test Gain"),
            format: PluginFormat::Vst3,
            path: bundle.display().to_string(),
            identifier: classes[0].cid.clone(),
            bypassed: false,
            state: String::new(),
        });
        let mut dry = song.tracks[0].clone();
        dry.id = TrackId::new(2).expect("track");
        dry.plugins.clear();
        for note in &mut dry.notes {
            note.id = project::NoteId::new(note.id.get() + 10).expect("note");
        }
        song.tracks.push(dry);
        let rendered = render(&song, SAMPLE_RATE);
        let early = rendered.samples[..32 * 2]
            .iter()
            .map(|sample| sample.abs())
            .fold(0.0_f32, f32::max);
        assert!(
            early < 1.0e-6,
            "compensation should delay both tracks: {early}"
        );
        assert!(rendered.samples.iter().any(|sample| sample.abs() > 0.01));
        let _ = std::fs::remove_dir_all(bundle.parent().expect("root"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn plugin_parameter_automation_changes_gain_over_time() {
        use project::song::{
            Automation, AutomationPoint, AutomationTarget, PluginFormat, PluginSlot,
        };
        let Some(bundle) = test_gain_bundle("automation") else {
            return;
        };
        let classes = plugin_host::scan_vst3(&bundle).expect("scan");
        let mut song = song_with_note();
        song.tracks[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("Test Gain"),
            format: PluginFormat::Vst3,
            path: bundle.display().to_string(),
            identifier: classes[0].cid.clone(),
            bypassed: false,
            state: String::new(),
        });
        song.tracks[0].automation.push(Automation {
            target: AutomationTarget::PluginParam {
                slot: 1,
                parameter: 0,
            },
            points: vec![
                AutomationPoint {
                    tick: 0,
                    value: 0.0,
                },
                AutomationPoint {
                    tick: 960,
                    value: 127.0,
                },
            ],
        });
        let rendered = render(&song, SAMPLE_RATE);
        let energy = |range: std::ops::Range<usize>| {
            rendered.samples[range]
                .iter()
                .map(|sample| sample.abs())
                .sum::<f32>()
        };
        let early = energy(0..512);
        let late = energy(1_000 * 2..2_000 * 2);
        assert!(late > early * 4.0, "early {early}, late {late}");
        let _ = std::fs::remove_dir_all(bundle.parent().expect("root"));
    }

    #[test]
    fn latency_delay_shifts_stereo_frames() {
        let mut buffer = vec![1.0, 2.0, 3.0, 4.0];
        delay_buffer(&mut buffer, 1);
        assert_eq!(buffer, vec![0.0, 0.0, 1.0, 2.0]);
        let mut buffer = vec![1.0, 2.0];
        delay_buffer(&mut buffer, 1);
        assert_eq!(buffer, vec![0.0, 0.0]);
    }

    #[test]
    fn a_note_produces_sound_where_it_sounds() {
        let rendered = render(&song_with_note(), SAMPLE_RATE);
        assert!(rendered.samples.iter().any(|sample| sample.abs() > 0.01));
        assert_eq!(rendered.samples[rendered.samples.len() - 1], 0.0);
        // One beat at 120 bpm is 0.5 s, plus the release tail.
        assert!(rendered.duration_seconds() > 0.5);
        assert!(rendered.duration_seconds() < 2.0);
    }

    #[test]
    #[ignore = "loads an installed VST3 plug-in"]
    fn a_vst3_slot_changes_the_track_render() {
        use project::song::{PluginFormat, PluginSlot};
        let bundle = std::path::Path::new("/Library/Audio/Plug-Ins/VST3/R0Verb.vst3");
        let Ok(classes) = plugin_host::scan_vst3(bundle) else {
            return;
        };
        let mut song = song_with_note();
        song.tracks[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("R0Verb"),
            format: PluginFormat::Vst3,
            path: bundle.display().to_string(),
            identifier: classes[0].cid.clone(),
            bypassed: false,
            state: String::new(),
        });
        let wet = render(&song, SAMPLE_RATE);
        song.tracks[0].plugins.clear();
        let dry = render(&song, SAMPLE_RATE);
        assert_eq!(wet.samples.len(), dry.samples.len());
        let difference: f32 = wet
            .samples
            .iter()
            .zip(&dry.samples)
            .map(|(wet, dry)| (wet - dry).abs())
            .sum();
        assert!(difference > 0.01, "plugin made no audible change");
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "loads the Apple dynamics Audio Unit"]
    fn a_bus_au_processes_its_sum() {
        use project::song::{Bus, PluginFormat, PluginSlot};
        let mut song = song_with_note();
        song.buses.push(Bus {
            id: 1,
            name: String::from("Drums"),
            volume: 100,
            pan: 64,
            muted: false,
            soloed: false,
            plugins: Vec::new(),
        });
        song.tracks[0].bus = Some(1);
        let dry = render(&song, SAMPLE_RATE);
        song.buses[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("AUDynamicsProcessor"),
            format: PluginFormat::AudioUnit,
            path: String::new(),
            identifier: String::from("appl:dcmp"),
            bypassed: false,
            state: String::new(),
        });
        let wet = render(&song, SAMPLE_RATE);
        let difference: f32 = wet
            .samples
            .iter()
            .zip(&dry.samples)
            .map(|(wet, dry)| (wet - dry).abs())
            .sum();
        assert!(difference > 0.01, "bus AU made no change");
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "loads the Apple dynamics Audio Unit"]
    fn a_master_au_processes_the_final_mix() {
        use project::song::{PluginFormat, PluginSlot};
        let mut song = song_with_note();
        let dry = render(&song, SAMPLE_RATE);
        song.master_plugins.push(PluginSlot {
            id: 1,
            name: String::from("AUDynamicsProcessor"),
            format: PluginFormat::AudioUnit,
            path: String::new(),
            identifier: String::from("appl:dcmp"),
            bypassed: false,
            state: String::new(),
        });
        let wet = render(&song, SAMPLE_RATE);
        let difference: f32 = wet
            .samples
            .iter()
            .zip(&dry.samples)
            .map(|(wet, dry)| (wet - dry).abs())
            .sum();
        assert!(difference > 0.01, "master AU made no change");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_test_vst3_synth_renders_track_notes() {
        use project::song::{PluginFormat, PluginSlot};
        let Some(bundle) = test_gain_bundle("synth") else {
            return;
        };
        let classes = plugin_host::scan_vst3(&bundle).expect("scan");
        let class = classes
            .iter()
            .find(|class| class.name.contains("Synth"))
            .expect("synth class");
        let mut song = song_with_note();
        song.tracks[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("Test Synth"),
            format: PluginFormat::Vst3,
            path: bundle.display().to_string(),
            identifier: class.cid.clone(),
            bypassed: false,
            state: String::new(),
        });
        let rendered = render(&song, SAMPLE_RATE);
        let energy: f32 = rendered.samples.iter().map(|sample| sample.abs()).sum();
        assert!(energy > 0.1, "VST3 synth produced no audio");
        let _ = std::fs::remove_dir_all(bundle.parent().expect("root"));
    }

    #[test]
    #[ignore = "loads the installed LABS VST3 instrument"]
    fn a_vst3_instrument_slot_replaces_the_builtin_synth() {
        use project::song::{PluginFormat, PluginSlot};
        let bundle = std::path::Path::new("/Library/Audio/Plug-Ins/VST3/LABS.vst3");
        let Ok(classes) = plugin_host::scan_vst3(bundle) else {
            return;
        };
        let mut song = song_with_note();
        song.tracks[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("LABS"),
            format: PluginFormat::Vst3,
            path: bundle.display().to_string(),
            identifier: classes[0].cid.clone(),
            bypassed: false,
            state: String::new(),
        });
        let instrument = render(&song, SAMPLE_RATE);
        song.tracks[0].plugins.clear();
        let builtin = render(&song, SAMPLE_RATE);
        let difference: f32 = instrument
            .samples
            .iter()
            .zip(&builtin.samples)
            .map(|(instrument, builtin)| (instrument - builtin).abs())
            .sum();
        assert!(difference > 0.01, "VST3 instrument path was not used");
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "loads the Apple DLSMusicDevice Audio Unit"]
    fn an_au_instrument_renders_track_notes() {
        use project::song::{PluginFormat, PluginSlot};
        let mut song = song_with_note();
        song.tracks[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("DLSMusicDevice"),
            format: PluginFormat::AudioUnit,
            path: String::new(),
            identifier: String::from("appl:dls"),
            bypassed: false,
            state: String::new(),
        });
        let instrument = render(&song, SAMPLE_RATE);
        song.tracks[0].plugins.clear();
        let builtin = render(&song, SAMPLE_RATE);
        let energy: f32 = instrument.samples.iter().map(|sample| sample.abs()).sum();
        assert!(energy > 0.1, "instrument produced no audio");
        let difference: f32 = instrument
            .samples
            .iter()
            .zip(&builtin.samples)
            .map(|(instrument, builtin)| (instrument - builtin).abs())
            .sum();
        assert!(
            difference > 0.1,
            "instrument did not replace the built-in synth"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "loads the Apple dynamics Audio Unit"]
    fn an_au_slot_changes_the_track_render() {
        use project::song::{PluginFormat, PluginSlot};
        let mut song = song_with_note();
        song.tracks[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("AUDynamicsProcessor"),
            format: PluginFormat::AudioUnit,
            path: String::new(),
            identifier: String::from("appl:dcmp"),
            bypassed: false,
            state: String::new(),
        });
        let wet = render(&song, SAMPLE_RATE);
        song.tracks[0].plugins.clear();
        let dry = render(&song, SAMPLE_RATE);
        assert_eq!(wet.samples.len(), dry.samples.len());
        let difference: f32 = wet
            .samples
            .iter()
            .zip(&dry.samples)
            .map(|(wet, dry)| (wet - dry).abs())
            .sum();
        assert!(difference > 0.01, "AU made no audible change");
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "loads the Apple dynamics Audio Unit"]
    fn an_au_parameter_lane_changes_the_render() {
        use project::song::{
            Automation, AutomationPoint, AutomationTarget, PluginFormat, PluginSlot,
        };
        let mut song = song_with_note();
        song.tracks[0].plugins.push(PluginSlot {
            id: 1,
            name: String::from("AUDynamicsProcessor"),
            format: PluginFormat::AudioUnit,
            path: String::new(),
            identifier: String::from("appl:dcmp"),
            bypassed: false,
            state: String::new(),
        });
        song.tracks[0].automation.push(Automation {
            target: AutomationTarget::PluginParam {
                slot: 1,
                parameter: 0,
            },
            points: vec![AutomationPoint {
                tick: 0,
                value: 0.0,
            }],
        });
        let low = render(&song, SAMPLE_RATE);
        song.tracks[0].automation[0].points[0].value = 127.0;
        let high = render(&song, SAMPLE_RATE);
        let difference: f32 = low
            .samples
            .iter()
            .zip(&high.samples)
            .map(|(left, right)| (left - right).abs())
            .sum();
        assert!(difference > 0.01, "AU parameter lane made no change");
    }

    #[test]
    fn an_audio_reference_extends_the_buffer() {
        let mut song = song_with_note();
        let mut take = Track::new(TrackId::new(2).expect("valid"), "Take");
        take.audio = Some(project::song::AudioRef {
            path: String::from("/tmp/unused.wav"),
            start: 0,
            length: 7680,
            offset: 0,

            fade_in: 0,
            fade_out: 0,

            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(take);
        let rendered = render(&song, SAMPLE_RATE);
        assert!(
            rendered.duration_seconds() > 4.0,
            "four seconds of audio at 120 bpm must fit"
        );
    }

    #[test]
    fn muted_and_soloed_tracks_follow_the_mix_rules() {
        let mut song = song_with_note();
        let mut second =
            project::song::Track::new(project::TrackId::new(2).expect("valid"), "Second");
        second.notes.push(Note {
            id: project::NoteId::new(1).expect("valid"),
            start: 0,
            duration: music_core::PPQ,
            pitch: Pitch::from_name("A5").expect("valid"),
            velocity: Velocity::new(100).expect("valid"),
            channel: Channel::first(),
        });
        song.tracks.push(second);
        let both = render(&song, SAMPLE_RATE);
        song.tracks[1].muted = true;
        let muted = render(&song, SAMPLE_RATE);
        assert_ne!(both.samples, muted.samples);
        song.tracks[1].muted = false;
        song.tracks[1].soloed = true;
        let soloed = render(&song, SAMPLE_RATE);
        assert_ne!(
            both.samples, soloed.samples,
            "solo silences the first track"
        );
        song.tracks[1].soloed = false;
        song.tracks[0].muted = true;
        song.tracks[1].muted = true;
        let silent = render(&song, SAMPLE_RATE);
        assert!(silent.samples.iter().all(|sample| sample.abs() < 1.0e-6));
    }

    #[test]
    fn a_track_soundfont_replaces_the_builtin_timbre() {
        let mut song = song_with_note();
        song.tracks[0].soundfont = Some(project::song::SoundFontRef {
            path: String::from("/missing/bank.sf2"),
            bank: 0,
            program: 0,
        });
        let font = soundfont::SoundFont {
            name: String::from("Test"),
            presets: vec![soundfont::Preset {
                name: String::from("Test"),
                bank: 0,
                program: 0,
                zones: vec![soundfont::PresetZone {
                    instrument: Some(0),
                    key_low: 0,
                    key_high: 127,
                    vel_low: 0,
                    vel_high: 127,
                }],
            }],
            instruments: vec![soundfont::Instrument {
                name: String::from("Test"),
                zones: vec![soundfont::InstrumentZone {
                    sample: Some(0),
                    key_low: 0,
                    key_high: 127,
                    vel_low: 0,
                    vel_high: 127,
                    root_override: Some(69),
                    coarse_tune: 0,
                    fine_tune: 0,
                    sample_modes: 0,
                    delay: -12_000,
                    attack: -12_000,
                    hold: -12_000,
                    decay: -12_000,
                    sustain: 0,
                    release: -12_000,
                    filter_cutoff: 13_500,
                    filter_q: 0,
                }],
            }],
            samples: vec![soundfont::Sample {
                name: String::from("Ramp"),
                start: 0,
                end: 64,
                loop_start: 0,
                loop_end: 64,
                rate: 44_100,
                root: 69,
                correction: 0,
                kind: 1,
                link: 0,
            }],
            pcm: (0..64).map(|index| index * 400).collect(),
        };
        let sampled =
            render_offline_with_soundfont(&song, SAMPLE_RATE, &mut |_| Some(font.clone()));
        let builtin = render(&song, SAMPLE_RATE);
        assert_ne!(sampled.samples, builtin.samples);
        assert!(sampled.samples.iter().any(|value| value.abs() > 0.01));
    }

    #[test]
    fn send_and_brightness_lanes_change_the_render() {
        use project::song::{Automation, AutomationPoint, AutomationTarget};
        let mut song = song_with_note();
        song.buses.push(project::song::Bus {
            id: 1,
            name: String::from("Reverb"),
            volume: 100,
            pan: 64,
            muted: false,
            soloed: false,
            plugins: Vec::new(),
        });
        song.tracks[0].send = Some(1);
        song.tracks[0].send_level = 0;
        let quiet = render(&song, SAMPLE_RATE);
        song.tracks[0].automation.push(Automation {
            target: AutomationTarget::Send,
            points: vec![AutomationPoint {
                tick: 0,
                value: 127.0,
            }],
        });
        let loud = render(&song, SAMPLE_RATE);
        assert_ne!(quiet.samples, loud.samples, "send lane is followed");

        song.tracks[0].automation.clear();
        song.tracks[0].patch = Some(project::song::Patch {
            waveform: project::song::Waveform::Sine,
            attack: 0.01,
            decay: 0.1,
            sustain: 0.8,
            release: 0.1,
            brightness: 0.0,
        });
        let dark = render(&song, SAMPLE_RATE);
        song.tracks[0].automation.push(Automation {
            target: AutomationTarget::Brightness,
            points: vec![AutomationPoint {
                tick: 0,
                value: 127.0,
            }],
        });
        let bright = render(&song, SAMPLE_RATE);
        assert_ne!(dark.samples, bright.samples, "brightness lane is followed");
    }

    #[test]
    fn stems_include_audio_references() {
        use project::song::{AudioRef, Track};
        let mut song = Song::new("Stem", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-stem-audio-{}.wav",
            std::process::id()
        ));
        let source = RenderedAudio {
            samples: vec![0.4; SAMPLE_RATE as usize * 2],
            sample_rate: SAMPLE_RATE,
        };
        std::fs::write(&path, source.to_wav32f()).expect("wav");
        track.audio = Some(AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: music_core::PPQ * 2,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let stems = render_stems(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        assert_eq!(stems.len(), 1);
        assert!(stems[0].audio.samples.iter().any(|sample| *sample > 0.3));
    }

    #[test]
    fn stems_render_one_audible_track_each() {
        let mut song = song_with_note();
        let mut second = song.tracks[0].clone();
        second.id = project::TrackId::new(2).expect("valid");
        second.name = String::from("Bass");
        second.notes[0].id = project::NoteId::new(2).expect("valid");
        second.notes[0].pitch = music_core::Pitch::from_name("C2").expect("valid");
        song.tracks.push(second);
        let stems = render_stems(&song, SAMPLE_RATE);
        assert_eq!(stems.len(), 2);
        assert_eq!(stems[1].name, "Bass");
        assert!(stems.iter().all(|stem| !stem.audio.samples.is_empty()));
        song.tracks[1].muted = true;
        assert_eq!(render_stems(&song, SAMPLE_RATE).len(), 1);
    }

    #[test]
    fn a_volume_lane_is_followed_per_note() {
        use project::song::{Automation, AutomationPoint, AutomationTarget};
        let mut song = song_with_note();
        let mut second = song.tracks[0].notes[0].clone();
        second.id = project::NoteId::new(2).expect("valid");
        second.start = music_core::PPQ;
        song.tracks[0].notes.push(second);
        song.tracks[0].automation.push(Automation {
            target: AutomationTarget::Volume,
            points: vec![
                AutomationPoint {
                    tick: 0,
                    value: 100.0,
                },
                AutomationPoint {
                    tick: music_core::PPQ,
                    value: 0.0,
                },
            ],
        });
        let rendered = render(&song, SAMPLE_RATE);
        let first = rendered.samples[1000 * 2].abs();
        let second_frame = rendered.samples[(24_000 + 1000) * 2].abs();
        assert!(first > 0.001, "first note sounds");
        assert!(second_frame < first * 0.5, "fade reached the second note");
    }

    #[test]
    fn a_patch_overrides_waveform_and_attack() {
        use project::song::{Patch, Waveform};
        let mut square = song_with_note();
        square.tracks[0].patch = Some(Patch {
            waveform: Waveform::Square,
            attack: 0.005,
            ..Patch::default()
        });
        let square = render(&square, SAMPLE_RATE);
        let sine = render(&song_with_note(), SAMPLE_RATE);
        assert_ne!(square.samples, sine.samples);

        let mut slow = song_with_note();
        slow.tracks[0].patch = Some(Patch {
            attack: 0.25,
            ..Patch::default()
        });
        let padded = render(&slow, SAMPLE_RATE);
        let peak = |audio: &RenderedAudio, start: usize, frames: usize| {
            audio.samples[start * 2..(start + frames) * 2]
                .iter()
                .step_by(2)
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
        };
        assert!(peak(&padded, 100, 200) < peak(&padded, 10_000, 200));
    }

    #[test]
    fn buses_and_sends_change_the_mix() {
        use project::song::Bus;
        let mut song = song_with_note();
        song.buses.push(Bus::new(1, "Reverb"));
        song.tracks[0].bus = Some(1);
        let routed = render(&song, SAMPLE_RATE);
        assert!(routed.samples.iter().any(|sample| sample.abs() > 0.001));
        song.buses[0].volume = 0;
        let silent = render(&song, SAMPLE_RATE);
        assert!(
            silent.samples.iter().all(|sample| sample.abs() < 1.0e-6),
            "zero bus fader silences the track"
        );

        let mut sent = song_with_note();
        sent.buses.push(Bus::new(1, "Reverb"));
        sent.tracks[0].send = Some(1);
        sent.tracks[0].send_level = 100;
        let with_send = render(&sent, SAMPLE_RATE);
        let dry = render(&song_with_note(), SAMPLE_RATE);
        let peak = |audio: &RenderedAudio| {
            audio
                .samples
                .iter()
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
        };
        assert!(peak(&with_send) > peak(&dry), "a send adds the wet copy");
    }

    #[test]
    fn comped_spans_replace_the_active_reference() {
        use project::song::{AudioComp, AudioRef, Track};
        let mut song = Song::new("Comp", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        let path =
            std::env::temp_dir().join(format!("lazy-arranger-comp-{}.wav", std::process::id()));
        let source = RenderedAudio {
            samples: vec![0.5; SAMPLE_RATE as usize * 2],
            sample_rate: SAMPLE_RATE,
        };
        std::fs::write(&path, source.to_wav32f()).expect("wav");
        let reference = AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: music_core::PPQ * 2,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        };
        track.audio = Some(reference.clone());
        track.takes.push(reference);
        track.comps.push(AudioComp {
            take: 0,
            start: 0,
            length: music_core::PPQ / 2,
        });
        song.tracks.push(track);
        let rendered = render(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        let inside = (SAMPLE_RATE as f64 * 0.02).round() as usize;
        assert!(
            rendered.samples[inside * 2] > 0.3,
            "comp starts with the take"
        );
        let after = (SAMPLE_RATE as f64 * 0.6).round() as usize;
        assert!(
            rendered.samples[after * 2].abs() > 0.3,
            "the active reference fills the gap outside the comp"
        );
    }

    #[test]
    fn adjacent_comp_spans_crossfade_instead_of_dropping_to_silence() {
        use project::song::{AudioComp, AudioRef, Track};
        let mut song = Song::new("Comp crossfade", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-comp-cross-{}.wav",
            std::process::id()
        ));
        let source = RenderedAudio {
            samples: vec![0.5; SAMPLE_RATE as usize * 2],
            sample_rate: SAMPLE_RATE,
        };
        std::fs::write(&path, source.to_wav32f()).expect("wav");
        let reference = AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: music_core::PPQ * 2,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        };
        track.audio = Some(reference.clone());
        track.takes.push(reference);
        track.comps.push(AudioComp {
            take: 0,
            start: 0,
            length: music_core::PPQ,
        });
        track.comps.push(AudioComp {
            take: 0,
            start: music_core::PPQ,
            length: music_core::PPQ,
        });
        song.tracks.push(track);
        let rendered = render(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        let boundary = (SAMPLE_RATE as f64 * 0.5).round() as usize;
        assert!(
            rendered.samples[boundary * 2].abs() > 0.2,
            "comp boundary should not fade to silence"
        );
    }

    #[test]
    fn comp_crossfades_follow_take_fades() {
        use project::song::{AudioComp, AudioRef, Track};
        let mut song = Song::new("Comp fades", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Audio");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-comp-fades-{}.wav",
            std::process::id()
        ));
        let source = RenderedAudio {
            samples: vec![0.5; SAMPLE_RATE as usize * 2],
            sample_rate: SAMPLE_RATE,
        };
        std::fs::write(&path, source.to_wav32f()).expect("wav");
        let reference = AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: music_core::PPQ * 2,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        };
        track.audio = Some(reference.clone());
        let mut left = reference.clone();
        left.fade_out = music_core::PPQ / 2;
        let mut right = reference;
        right.fade_in = music_core::PPQ / 2;
        track.takes.push(left);
        track.takes.push(right);
        track.comps.push(AudioComp {
            take: 0,
            start: 0,
            length: music_core::PPQ,
        });
        track.comps.push(AudioComp {
            take: 1,
            start: music_core::PPQ,
            length: music_core::PPQ,
        });
        song.tracks.push(track);
        let rendered = render(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        let boundary = (SAMPLE_RATE as f64 * 0.5).round() as usize;
        assert!(
            rendered.samples[boundary * 2].abs() > 0.2,
            "take fades should lengthen the overlap"
        );
    }

    #[test]
    fn audio_references_are_mixed() {
        use project::song::{AudioRef, Track};
        let mut song = Song::new("Audio", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Take");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-audio-mix-{}.wav",
            std::process::id()
        ));
        let source = RenderedAudio {
            samples: vec![0.5; SAMPLE_RATE as usize * 2],
            sample_rate: SAMPLE_RATE,
        };
        std::fs::write(&path, source.to_wav32f()).expect("write wav");
        track.audio = Some(AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: music_core::PPQ * 2,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let rendered = render(&song, SAMPLE_RATE);
        let unity_peak = rendered
            .samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(unity_peak > 0.3, "audio reference reaches the mix");
        if let Some(reference) = song.tracks[0].audio.as_mut() {
            reference.gain_mb = -6_000;
        }
        let quieter = render(&song, SAMPLE_RATE);
        let quieter_peak = quieter
            .samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            quieter_peak < unity_peak * 0.6,
            "clip gain lowers the mixed level"
        );
        song.buses.push(project::song::Bus::new(1, "FX"));
        song.tracks[0].bus = Some(1);
        song.buses[0].volume = 0;
        let silent = render(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        assert!(
            silent.samples.iter().all(|sample| sample.abs() < 1.0e-6),
            "the audio reference follows bus routing"
        );
    }

    #[test]
    fn audio_reference_start_and_fade_are_applied() {
        use project::song::{AudioRef, Track};
        let mut song = Song::new("Audio", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Take");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-audio-fade-{}.wav",
            std::process::id()
        ));
        let source = RenderedAudio {
            samples: vec![0.5; SAMPLE_RATE as usize * 2],
            sample_rate: SAMPLE_RATE,
        };
        std::fs::write(&path, source.to_wav32f()).expect("write wav");
        track.audio = Some(AudioRef {
            path: path.display().to_string(),
            start: music_core::PPQ,
            length: music_core::PPQ * 2,
            offset: 0,
            fade_in: music_core::PPQ,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let rendered = render(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        let at = |seconds: f64| {
            let frame = (seconds * f64::from(SAMPLE_RATE)).round() as usize;
            rendered.samples[frame * 2]
        };
        assert!(at(0.1).abs() < 1.0e-6, "silent before the clip start");
        assert!(at(0.6) < 0.12, "fade-in starts quietly");
        assert!(at(1.1) > 0.3, "full gain after the fade");
    }

    #[test]
    fn audio_reference_stretch_fills_the_clip() {
        use project::song::{AudioRef, Track};
        let mut song = Song::new("Audio", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Take");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-audio-stretch-{}.wav",
            std::process::id()
        ));
        let mut samples = Vec::with_capacity(SAMPLE_RATE as usize * 2);
        for frame in 0..SAMPLE_RATE as usize {
            let value = frame as f32 / SAMPLE_RATE as f32;
            samples.extend_from_slice(&[value, value]);
        }
        let source = RenderedAudio {
            samples,
            sample_rate: SAMPLE_RATE,
        };
        std::fs::write(&path, source.to_wav32f()).expect("write wav");
        track.audio = Some(AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: music_core::PPQ / 2,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: true,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let rendered = render(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        let frame = (SAMPLE_RATE as f64 * 0.2).round() as usize;
        assert!(
            rendered.samples[frame * 2] > 0.6,
            "the source ramp is stretched over the clip"
        );
    }

    #[test]
    fn time_stretch_keeps_the_pitch() {
        let rate = 48_000_u32;
        let mut input = Vec::with_capacity(rate as usize * 2);
        for frame in 0..rate as usize {
            let time = frame as f64 / f64::from(rate);
            let value = (std::f64::consts::TAU * 440.0 * time).sin() as f32 * 0.5;
            input.extend_from_slice(&[value, value]);
        }
        let target = rate as usize * 2;
        let stretched = stretch_interleaved(&input, target);
        assert_eq!(stretched.len(), target * 2);
        let middle = rate as usize / 2;
        let mono: Vec<f32> = stretched[middle * 2..(middle + 2_048) * 2]
            .iter()
            .step_by(2)
            .copied()
            .collect();
        let pitch = crate::analysis::detect_pitch(&mono, rate).expect("pitch");
        assert!((pitch - 440.0).abs() < 5.0, "pitch was {pitch}");
    }

    #[test]
    fn a_stretched_audio_reference_keeps_pitch() {
        use project::song::{AudioRef, Track};
        let mut song = Song::new("Stretch", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Take");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-audio-stretch-pitch-{}.wav",
            std::process::id()
        ));
        let mut samples = Vec::with_capacity(SAMPLE_RATE as usize * 2);
        for frame in 0..SAMPLE_RATE as usize {
            let time = frame as f64 / f64::from(SAMPLE_RATE);
            let value = (std::f64::consts::TAU * 440.0 * time).sin() as f32 * 0.5;
            samples.extend_from_slice(&[value, value]);
        }
        std::fs::write(
            &path,
            RenderedAudio {
                samples,
                sample_rate: SAMPLE_RATE,
            }
            .to_wav32f(),
        )
        .expect("write wav");
        track.audio = Some(AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: 4 * music_core::PPQ,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: true,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let rendered = render(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        let frame = SAMPLE_RATE as usize;
        let mono: Vec<f32> = rendered.samples[frame * 2..(frame + 2_048) * 2]
            .iter()
            .step_by(2)
            .copied()
            .collect();
        let pitch = crate::analysis::detect_pitch(&mono, SAMPLE_RATE).expect("pitch");
        assert!((pitch - 440.0).abs() < 5.0, "pitch was {pitch}");
    }

    #[test]
    fn time_stretch_compresses_without_changing_pitch() {
        let rate = 48_000_u32;
        let mut input = Vec::with_capacity(rate as usize * 4);
        for frame in 0..rate as usize * 2 {
            let time = frame as f64 / f64::from(rate);
            let value = (std::f64::consts::TAU * 440.0 * time).sin() as f32 * 0.5;
            input.extend_from_slice(&[value, value]);
        }
        let target = rate as usize;
        let stretched = stretch_interleaved(&input, target);
        assert_eq!(stretched.len(), target * 2);
        let mono: Vec<f32> = stretched[10_000 * 2..(10_000 + 2_048) * 2]
            .iter()
            .step_by(2)
            .copied()
            .collect();
        let pitch = crate::analysis::detect_pitch(&mono, rate).expect("pitch");
        assert!((pitch - 440.0).abs() < 5.0, "pitch was {pitch}");
    }

    #[test]
    fn audio_reference_offset_skips_the_source_start() {
        use project::song::{AudioRef, Track};
        let mut song = Song::new("Audio", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Take");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-audio-offset-{}.wav",
            std::process::id()
        ));
        let mut samples = vec![0.0; SAMPLE_RATE as usize * 2];
        for sample in samples.iter_mut().skip(SAMPLE_RATE as usize) {
            *sample = 0.5;
        }
        let source = RenderedAudio {
            samples,
            sample_rate: SAMPLE_RATE,
        };
        std::fs::write(&path, source.to_wav32f()).expect("write wav");
        track.audio = Some(AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: music_core::PPQ,
            offset: music_core::PPQ,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        song.tracks.push(track);
        let rendered = render(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        assert!(rendered.samples[0] > 0.4, "the offset source is used");
    }

    #[test]
    fn audio_references_feed_sends() {
        use project::song::{AudioRef, Bus, Track};
        let mut song = Song::new("Audio", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Take");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-audio-send-{}.wav",
            std::process::id()
        ));
        let source = RenderedAudio {
            samples: vec![0.5; SAMPLE_RATE as usize * 2],
            sample_rate: SAMPLE_RATE,
        };
        std::fs::write(&path, source.to_wav32f()).expect("write wav");
        track.audio = Some(AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: music_core::PPQ * 2,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        track.bus = Some(2);
        track.send = Some(1);
        track.send_level = 100;
        song.tracks.push(track);
        let mut dry = Bus::new(2, "Dry");
        dry.volume = 0;
        song.buses.push(dry);
        song.buses.push(Bus::new(1, "Wet"));
        let wet = render(&song, SAMPLE_RATE);
        assert!(wet.samples.iter().any(|sample| sample.abs() > 0.3));
        song.tracks[0].send_level = 0;
        let silent = render(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        assert!(silent.samples.iter().all(|sample| sample.abs() < 1.0e-6));
    }

    #[test]
    fn audio_references_follow_volume_automation() {
        use project::song::{AudioRef, Automation, AutomationPoint, AutomationTarget, Track};
        let mut song = Song::new("Audio", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("track"), "Take");
        let path = std::env::temp_dir().join(format!(
            "lazy-arranger-audio-automation-{}.wav",
            std::process::id()
        ));
        let source = RenderedAudio {
            samples: vec![0.5; SAMPLE_RATE as usize * 2],
            sample_rate: SAMPLE_RATE,
        };
        std::fs::write(&path, source.to_wav32f()).expect("write wav");
        track.audio = Some(AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: music_core::PPQ * 2,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        track.automation.push(Automation {
            target: AutomationTarget::Volume,
            points: vec![AutomationPoint {
                tick: 0,
                value: 0.0,
            }],
        });
        song.tracks.push(track);
        let rendered = render(&song, SAMPLE_RATE);
        let _ = std::fs::remove_file(&path);
        assert!(
            rendered.samples.iter().all(|sample| sample.abs() < 1.0e-6),
            "audio follows a volume lane"
        );
    }

    #[test]
    fn muted_and_non_soloed_buses_are_silent() {
        use project::song::Bus;
        let mut song = song_with_note();
        song.buses.push(Bus::new(1, "Reverb"));
        song.buses.push(Bus::new(2, "Delay"));
        song.tracks[0].bus = Some(1);
        let audible = render(&song, SAMPLE_RATE);
        assert!(audible.samples.iter().any(|sample| sample.abs() > 0.001));
        song.buses[0].muted = true;
        let muted = render(&song, SAMPLE_RATE);
        assert!(muted.samples.iter().all(|sample| sample.abs() < 1.0e-6));
        song.buses[0].muted = false;
        song.buses[1].soloed = true;
        let excluded = render(&song, SAMPLE_RATE);
        assert!(excluded.samples.iter().all(|sample| sample.abs() < 1.0e-6));
        song.buses[0].soloed = true;
        let soloed = render(&song, SAMPLE_RATE);
        assert!(soloed.samples.iter().any(|sample| sample.abs() > 0.001));
    }

    #[test]
    fn a_bus_pan_places_its_tracks() {
        use project::song::Bus;
        let mut song = song_with_note();
        let mut bus = Bus::new(1, "Left");
        bus.pan = 127;
        song.buses.push(bus);
        song.tracks[0].bus = Some(1);
        let rendered = render(&song, SAMPLE_RATE);
        assert!(
            rendered
                .samples
                .chunks(2)
                .all(|frame| frame[0].abs() < 1.0e-6)
        );
        assert!(
            rendered
                .samples
                .chunks(2)
                .any(|frame| frame[1].abs() > 0.001)
        );
    }

    #[test]
    fn faders_and_pans_change_the_mix() {
        let mut song = song_with_note();
        let centred = render(&song, SAMPLE_RATE);
        song.tracks[0].volume = 0;
        let silent = render(&song, SAMPLE_RATE);
        assert!(silent.samples.iter().all(|sample| sample.abs() < 1.0e-6));
        song.tracks[0].volume = 100;
        song.tracks[0].pan = 127;
        let right = render(&song, SAMPLE_RATE);
        assert!(right.samples.chunks(2).all(|frame| frame[0].abs() < 1.0e-6));
        assert!(right.samples.chunks(2).any(|frame| frame[1].abs() > 0.001));
        assert_ne!(centred.samples, right.samples);
    }

    #[test]
    fn a4_sits_at_440_hz() {
        let pitch = Pitch::from_name("A4").expect("valid");
        assert!((frequency(pitch) - 440.0).abs() < 0.01);
        assert!((frequency(Pitch::new(60).expect("valid")) - 261.63).abs() < 0.01);
    }

    #[test]
    fn output_never_clips() {
        let mut song = song_with_note();
        let track = &mut song.tracks[0];
        for index in 2..12 {
            track.notes.push(Note {
                id: NoteId::new(index).expect("valid"),
                start: 0,
                duration: music_core::PPQ,
                pitch: Pitch::from_name("C4").expect("valid"),
                velocity: Velocity::new(127).expect("valid"),
                channel: Channel::first(),
            });
        }
        let rendered = render(&song, SAMPLE_RATE);
        assert!(rendered.samples.iter().all(|sample| sample.abs() <= 1.0));
    }

    #[test]
    fn where_the_note_ends_the_tail_decays() {
        let rendered = render(&song_with_note(), SAMPLE_RATE);
        let frames = rendered.frames();
        let release_start = (0.5 * f64::from(SAMPLE_RATE)) as usize;
        let at_release = rendered.samples[release_start * 2].abs();
        let tail_end = rendered.samples[(frames - 1) * 2].abs();
        assert!(tail_end <= at_release);
    }

    #[test]
    fn a_24_bit_wav_has_three_byte_samples() {
        let rendered = RenderedAudio {
            samples: vec![0.0, 1.0, -1.0, 0.5],
            sample_rate: 48_000,
        };
        let wav = rendered.to_wav24();
        assert_eq!(wav.len(), 44 + 4 * 3);
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 24);
        assert_eq!(
            u16::from_le_bytes([wav[32], wav[33]]),
            6,
            "2 channels x 3 bytes"
        );
        assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 12);
        assert_eq!(&wav[44..47], &[0x00, 0x00, 0x00]);
        assert_eq!(&wav[47..50], &[0xff, 0xff, 0x7f]);
        assert_eq!(&wav[50..53], &[0x01, 0x00, 0x80]);
    }

    #[test]
    fn timbres_follow_channel_program_and_name() {
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Track");
        track.program = Some(32);
        assert_eq!(timbre_for(&track), Timbre::Bass);
        track.program = Some(89);
        assert_eq!(timbre_for(&track), Timbre::Pad);
        track.program = None;
        track.name = String::from("Drums");
        assert_eq!(timbre_for(&track), Timbre::Drums);
    }

    #[test]
    fn a_pad_swells_while_piano_strikes() {
        let mut song = song_with_note();
        song.tracks[0].program = Some(89);
        song.tracks[0].notes[0].duration = music_core::PPQ;
        let pad = render(&song, SAMPLE_RATE);
        let at = |seconds: f64| {
            let frame = (seconds * f64::from(SAMPLE_RATE)) as usize;
            pad.samples[frame * 2..(frame + 200) * 2]
                .iter()
                .step_by(2)
                .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
        };
        assert!(at(0.02) < at(0.3) * 0.5, "pad swells after its slow attack");

        let mut piano_song = song_with_note();
        piano_song.tracks[0].program = Some(1);
        let piano = render(&piano_song, SAMPLE_RATE);
        assert_ne!(pad.samples, piano.samples);
    }

    #[test]
    fn drums_produce_noise_without_a_pitch() {
        let mut song = Song::new("Drums", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Drums");
        track.notes.push(Note {
            id: NoteId::new(1).expect("valid"),
            start: 0,
            duration: 480,
            pitch: Pitch::new(38).expect("valid"),
            velocity: Velocity::new(120).expect("valid"),
            channel: Channel::drums(),
        });
        song.tracks.push(track);
        let rendered = render(&song, SAMPLE_RATE);
        assert!(rendered.samples.iter().any(|sample| sample.abs() > 0.01));
    }

    #[test]
    fn slicing_takes_the_requested_time_window() {
        let audio = RenderedAudio {
            samples: (0..8).map(|value| value as f32).collect(),
            sample_rate: 4,
        };
        let sliced = audio.slice(0.5, 1.5);
        assert_eq!(sliced.samples, vec![4.0, 5.0, 6.0, 7.0]);
        assert_eq!(audio.slice(0.75, 9.0).samples.len(), 2);
        assert!(audio.slice(-1.0, 0.0).samples.is_empty());
    }

    #[test]
    fn a_float_wav_reports_and_decodes_32_bit_float() {
        let audio = RenderedAudio {
            samples: vec![0.5, -0.25, 0.0, 1.0],
            sample_rate: 48_000,
        };
        let bytes = audio.to_wav32f();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 3);
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 32);
        assert_eq!(
            u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]),
            16
        );
        let decoded = wav::decode_mono(&bytes).expect("decode");
        assert_eq!(decoded.samples, vec![0.125, 0.5]);
    }

    #[test]
    fn wav_header_is_standard() {
        let rendered = render(&song_with_note(), SAMPLE_RATE);
        let wav = rendered.to_wav();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + rendered.samples.len() * 2);
        let data_len = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_len as usize, rendered.samples.len() * 2);
    }

    #[test]
    fn peak_tracks_velocity() {
        assert!(peak_for(Velocity::new(127).expect("valid")) > 0.99);
        assert!(peak_for(Velocity::new(64).expect("valid")) < 0.6);
    }

    /// Plays a live render through the engine to its end.
    fn play_live(render: crate::live::LiveRender) -> Vec<f32> {
        let len = render.fixed.samples.len();
        let mut playback = crate::player::Playback::new_live(render);
        playback.play();
        let mut output = vec![0.0_f32; len];
        for chunk in output.chunks_mut(200) {
            playback.fill(chunk);
        }
        output
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn gain_slot(bundle: &std::path::Path, state: &str) -> project::song::PluginSlot {
        let classes = plugin_host::scan_vst3(bundle).expect("scan");
        project::song::PluginSlot {
            id: 1,
            name: String::from("Test Gain"),
            format: project::song::PluginFormat::Vst3,
            path: bundle.display().to_string(),
            identifier: classes[0].cid.clone(),
            bypassed: false,
            state: String::from(state),
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_live_insert_matches_the_offline_render() {
        let Some(bundle) = test_gain_bundle("live-insert") else {
            return;
        };
        let mut song = song_with_note();
        // Half gain.
        song.tracks[0]
            .plugins
            .push(gain_slot(&bundle, "AAAAAAAA4D8="));
        let offline = render_offline(&song, SAMPLE_RATE);
        let live = crate::live::render(&song, SAMPLE_RATE);
        assert!(
            live.inserts[0]
                .as_ref()
                .is_some_and(|chain| chain.len() == 1),
            "the gain runs live"
        );
        let output = play_live(live);
        assert_eq!(output.len(), offline.samples.len());
        let worst = output
            .iter()
            .zip(&offline.samples)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(worst < 1e-5, "live and offline differ by {worst}");
        assert!(output.iter().any(|sample| sample.abs() > 0.01));
        let _ = std::fs::remove_dir_all(bundle.parent().expect("root"));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_plugin_edit_swaps_in_while_playing() {
        let Some(bundle) = test_gain_bundle("insert-swap") else {
            return;
        };
        let mut song = song_with_note();
        song.tracks[0].plugins.push(gain_slot(&bundle, ""));
        let live = crate::live::render(&song, SAMPLE_RATE);
        let mut playback = crate::player::Playback::new_live(live);
        playback.play();
        let mut output = vec![0.0_f32; 4096];
        playback.fill(&mut output);
        // Silence: gain 0.
        song.tracks[0].plugins[0].state = String::from("AAAAAAAAAAA=");
        let chain = crate::inserts::Inserts::build(&song.tracks[0], SAMPLE_RATE);
        playback
            .swap_inserts(crate::inserts::ChainId::Track(song.tracks[0].id), chain)
            .expect("swap");
        playback.fill(&mut output);
        assert!(
            output.iter().all(|sample| sample.abs() < 1e-6),
            "the new chain is heard from the next buffer"
        );
        assert_eq!(playback.collect_garbage(), 1, "the old chain comes back");
        let _ = std::fs::remove_dir_all(bundle.parent().expect("root"));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_live_insert_stays_aligned_with_a_dry_track() {
        let Some(bundle) = test_gain_bundle("live-pdc") else {
            return;
        };
        let mut song = song_with_note();
        song.tracks[0].plugins.push(gain_slot(&bundle, ""));
        let mut dry = song.tracks[0].clone();
        dry.id = TrackId::new(2).expect("track");
        dry.plugins.clear();
        for note in &mut dry.notes {
            note.id = project::NoteId::new(note.id.get() + 10).expect("note");
        }
        song.tracks.push(dry);
        let offline = render_offline(&song, SAMPLE_RATE);
        let live = crate::live::render(&song, SAMPLE_RATE);
        assert!(
            live.inserts[0]
                .as_ref()
                .is_some_and(|chain| chain.latency() > 0)
        );
        let output = play_live(live);
        let worst = output
            .iter()
            .zip(&offline.samples)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(worst < 1e-5, "live and offline differ by {worst}");
        let _ = std::fs::remove_dir_all(bundle.parent().expect("root"));
    }

    #[test]
    fn a_missing_insert_plugin_plays_dry_live() {
        let mut song = song_with_note();
        song.tracks[0].plugins.push(project::song::PluginSlot {
            id: 1,
            name: String::from("Gone"),
            format: project::song::PluginFormat::Vst3,
            path: String::from("/nonexistent/Gone.vst3"),
            identifier: String::from("00000000000000000000000000000000"),
            bypassed: false,
            state: String::new(),
        });
        assert!(crate::inserts::can_run_live(&song.tracks[0]));
        let offline = render_offline(&song, SAMPLE_RATE);
        let live = crate::live::render(&song, SAMPLE_RATE);
        assert!(
            live.inserts[0]
                .as_ref()
                .is_some_and(crate::inserts::Inserts::is_empty),
            "the slot opens nothing, so the chain passes audio through"
        );
        let output = play_live(live);
        assert_eq!(output, offline.samples);
    }

    #[test]
    fn live_insert_edits_need_no_new_render() {
        use std::hash::Hasher;
        let mut song = song_with_note();
        song.tracks[0].plugins.push(project::song::PluginSlot {
            id: 1,
            name: String::from("Fx"),
            format: project::song::PluginFormat::Vst3,
            path: String::from("/nonexistent/Fx.vst3"),
            identifier: String::from("00000000000000000000000000000000"),
            bypassed: false,
            state: String::new(),
        });
        let fixed = |song: &Song| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            crate::live::hash_fixed(song, &mut hasher);
            hasher.finish()
        };
        let before = fixed(&song);
        let chain = crate::inserts::signature(&song.tracks[0]);
        song.tracks[0].plugins[0].state = String::from("AAAAAAAA4D8=");
        assert_eq!(before, fixed(&song), "a state change swaps the chain");
        assert_ne!(chain, crate::inserts::signature(&song.tracks[0]));

        // With plug-in automation the chain is rendered, so it is fixed.
        song.tracks[0].automation.push(project::song::Automation {
            target: project::song::AutomationTarget::PluginParam {
                slot: 1,
                parameter: 0,
            },
            points: Vec::new(),
        });
        assert!(!crate::inserts::can_run_live(&song.tracks[0]));
        assert_ne!(before, fixed(&song));
    }

    fn worst_difference(left: &[f32], right: &[f32]) -> f32 {
        assert_eq!(left.len(), right.len());
        left.iter()
            .zip(right)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max)
    }

    /// A song with a track on a bus and a dry track straight to the master.
    fn bus_song(bus_slot: project::song::PluginSlot) -> Song {
        let mut song = song_with_note();
        let mut bus = project::song::Bus::new(1, "Bus");
        bus.volume = 90;
        bus.pan = 30;
        bus.plugins.push(bus_slot);
        song.buses.push(bus);
        song.tracks[0].bus = Some(1);
        let mut dry = song.tracks[0].clone();
        dry.id = TrackId::new(2).expect("track");
        dry.bus = None;
        dry.send = Some(1);
        dry.send_level = 60;
        for note in &mut dry.notes {
            note.id = project::NoteId::new(note.id.get() + 10).expect("note");
        }
        song.tracks.push(dry);
        song
    }

    fn missing_slot() -> project::song::PluginSlot {
        project::song::PluginSlot {
            id: 1,
            name: String::from("Gone"),
            format: project::song::PluginFormat::Vst3,
            path: String::from("/nonexistent/Gone.vst3"),
            identifier: String::from("00000000000000000000000000000000"),
            bypassed: false,
            state: String::new(),
        }
    }

    #[test]
    fn a_blocked_plugin_is_left_out_of_live_chains() {
        let Some(bundle) = test_gain_bundle("watchdog") else {
            return;
        };
        let slot = gain_slot(&bundle, "");
        let slots = [slot.clone()];
        assert_eq!(
            crate::inserts::Inserts::from_slots(&slots, SAMPLE_RATE).len(),
            1
        );
        crate::watchdog::block(&crate::watchdog::key(&slot));
        assert!(crate::inserts::Inserts::from_slots(&slots, SAMPLE_RATE).is_empty());
    }

    #[test]
    fn export_plays_through_the_engine_and_matches_the_offline_render() {
        for song in [bus_song(missing_slot()), automated_song(true)] {
            let offline = render_offline(&song, SAMPLE_RATE);
            let exported = render(&song, SAMPLE_RATE);
            assert_eq!(exported.samples.len(), offline.samples.len());
            let worst = worst_difference(&exported.samples, &offline.samples);
            assert!(worst < 1e-5, "export and offline differ by {worst}");

            let settings = master::Master {
                gain_db: -3.0,
                limiter: true,
                reverb: 0.3,
                chorus: 0.2,
            };
            let mut expected = offline.clone();
            metronome::mix(&mut expected, &song, 1.0);
            master::apply(&mut expected, settings);
            let mastered = render_mastered(&song, SAMPLE_RATE, settings, Some(1.0));
            let worst = worst_difference(&mastered.samples, &expected.samples);
            assert!(worst < 1e-4, "mastered export differs by {worst}");
        }
    }

    #[test]
    fn a_live_bus_follows_its_fader_and_sends() {
        let mut song = bus_song(missing_slot());
        assert_eq!(crate::live::live_buses(&song), vec![true]);
        assert_eq!(crate::live::live_tracks(&song), vec![true, true]);
        let live = crate::live::render(&song, SAMPLE_RATE);
        assert_eq!(live.buses.len(), 1);
        let offline = render_offline(&song, SAMPLE_RATE);
        let worst = worst_difference(&play_live(live), &offline.samples);
        assert!(worst < 1e-5, "live and offline differ by {worst}");

        // A bus fader, pan and send move need no new render.
        let mut moved = song.clone();
        moved.buses[0].volume = 40;
        moved.buses[0].pan = 100;
        moved.tracks[1].send_level = 120;
        let fixed = |song: &Song| {
            use std::hash::Hasher;
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            crate::live::hash_fixed(song, &mut hasher);
            hasher.finish()
        };
        assert_eq!(fixed(&song), fixed(&moved));
        let live = crate::live::render(&song, SAMPLE_RATE);
        let len = live.fixed.samples.len();
        let mut playback = crate::player::Playback::new_live(live);
        playback.apply_mix(
            &moved,
            crate::master::Master {
                gain_db: 0.0,
                limiter: false,
                reverb: 0.0,
                chorus: 0.0,
            },
        );
        playback.play();
        let mut output = vec![0.0_f32; len];
        // One quantum ramps from the old gains to the new ones.
        playback.fill(&mut output[..64]);
        playback.fill(&mut output[64..]);
        song = moved;
        let offline = render_offline(&song, SAMPLE_RATE);
        let worst = worst_difference(&output[64..], &offline.samples[64..]);
        assert!(worst < 1e-5, "live and offline differ by {worst}");
        song.buses[0].plugins.clear();
        assert_ne!(fixed(&song), fixed(&bus_song(missing_slot())));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_live_bus_plugin_matches_the_offline_render() {
        let Some(bundle) = test_gain_bundle("live-bus") else {
            return;
        };
        // Half gain, with the plug-in's latency.
        let song = bus_song(gain_slot(&bundle, "AAAAAAAA4D8="));
        let offline = render_offline(&song, SAMPLE_RATE);
        let live = crate::live::render(&song, SAMPLE_RATE);
        assert!(live.buses[0].inserts.len() == 1 && live.bus_delay > 0);
        let output = play_live(live);
        let worst = worst_difference(&output, &offline.samples);
        assert!(worst < 1e-5, "live and offline differ by {worst}");
        assert!(output.iter().any(|sample| sample.abs() > 0.01));
        let _ = std::fs::remove_dir_all(bundle.parent().expect("root"));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn live_master_plugins_match_and_swap() {
        use crate::inserts::{ChainId, Inserts};
        let Some(bundle) = test_gain_bundle("live-master") else {
            return;
        };
        let mut song = song_with_note();
        song.master_plugins.push(gain_slot(&bundle, "AAAAAAAA4D8="));
        assert_eq!(crate::live::live_tracks(&song), vec![true]);
        let offline = render_offline(&song, SAMPLE_RATE);
        let live = crate::live::render(&song, SAMPLE_RATE);
        let output = play_live(live);
        let worst = worst_difference(&output, &offline.samples);
        assert!(worst < 1e-5, "live and offline differ by {worst}");

        let live = crate::live::render(&song, SAMPLE_RATE);
        let mut playback = crate::player::Playback::new_live(live);
        playback.play();
        let mut output = vec![0.0_f32; 4096];
        playback.fill(&mut output);
        song.master_plugins[0].state = String::from("AAAAAAAAAAA=");
        playback
            .swap_inserts(
                ChainId::Master,
                Inserts::from_slots(&song.master_plugins, SAMPLE_RATE),
            )
            .expect("swap");
        playback.fill(&mut output);
        assert!(output.iter().all(|sample| sample.abs() < 1e-6));
        assert_eq!(playback.collect_garbage(), 1);
        let _ = std::fs::remove_dir_all(bundle.parent().expect("root"));
    }

    /// A song whose first track's fader, pan and send follow automation,
    /// sending to a bus with or without (missing) plug-ins.
    fn automated_song(bus_plugins: bool) -> Song {
        use project::song::{Automation, AutomationPoint, AutomationTarget};
        let mut song = bus_song(missing_slot());
        if !bus_plugins {
            song.buses[0].plugins.clear();
        }
        let ramp = |target, from: f32, to: f32| Automation {
            target,
            points: vec![
                AutomationPoint {
                    tick: 0,
                    value: from,
                },
                AutomationPoint {
                    tick: music_core::PPQ,
                    value: to,
                },
            ],
        };
        song.tracks[1].automation = vec![
            ramp(AutomationTarget::Volume, 20.0, 120.0),
            ramp(AutomationTarget::Pan, 0.0, 127.0),
            ramp(AutomationTarget::Send, 127.0, 0.0),
        ];
        song
    }

    #[test]
    fn automated_tracks_play_live_as_they_render() {
        for bus_plugins in [false, true] {
            let song = automated_song(bus_plugins);
            assert_eq!(crate::live::live_tracks(&song), vec![true, true]);
            let live = crate::live::render(&song, SAMPLE_RATE);
            assert!(live.envelopes[1].is_some(), "the automation runs live");
            assert_eq!(live.buses.len(), usize::from(bus_plugins));
            let offline = render_offline(&song, SAMPLE_RATE);
            let worst = worst_difference(&play_live(live), &offline.samples);
            assert!(
                worst < 1e-5,
                "bus plug-ins {bus_plugins}: live and offline differ by {worst}"
            );
        }
    }

    #[test]
    fn an_automation_edit_swaps_in_while_playing() {
        use project::song::AutomationPoint;
        let mut song = automated_song(false);
        let live = crate::live::render(&song, SAMPLE_RATE);
        let len = live.fixed.samples.len();
        let mut playback = crate::player::Playback::new_live(live);
        playback.play();
        // Fader down to silence for the whole song.
        song.tracks[1].automation[0].points = vec![AutomationPoint {
            tick: 0,
            value: 0.0,
        }];
        song.tracks[1].automation[2].points = vec![AutomationPoint {
            tick: 0,
            value: 0.0,
        }];
        let envelope =
            crate::automation::Envelope::build(&song, &song.tracks[1], SAMPLE_RATE, len / 2);
        playback
            .swap_envelope(song.tracks[1].id, envelope)
            .expect("swap");
        let mut output = vec![0.0_f32; len];
        playback.fill(&mut output);
        assert_eq!(playback.collect_garbage(), 1, "the old envelope comes back");
        song.tracks[0].muted = true;
        let only_automated = render(&song, SAMPLE_RATE);
        assert!(
            only_automated
                .samples
                .iter()
                .all(|sample| sample.abs() < 1e-6)
        );
        song.tracks[0].muted = false;
        let offline = render_offline(&song, SAMPLE_RATE);
        let worst = worst_difference(&output, &offline.samples);
        assert!(worst < 1e-5, "live and offline differ by {worst}");
    }

    #[test]
    fn an_edit_on_a_rendered_track_swaps_in_one_lane() {
        let song = automated_song(true);
        let mut moved = song.clone();
        for note in &mut moved.tracks[0].notes {
            note.start += music_core::PPQ / 2;
        }
        // A track with an effect slot is rendered, not a synth lane, like a
        // track with recorded audio.
        let mut with_slot = song.clone();
        with_slot.tracks[0].plugins.push(missing_slot());
        let mut moved_slot = moved.clone();
        moved_slot.tracks[0].plugins.push(missing_slot());
        assert_ne!(
            crate::live::lane_signature(&with_slot, &with_slot.tracks[0]),
            crate::live::lane_signature(&moved_slot, &moved_slot.tracks[0])
        );
        let live = crate::live::render(&with_slot, SAMPLE_RATE);
        let len = live.fixed.samples.len();
        let mut playback = crate::player::Playback::new_live(live);
        playback.play();
        let lane = crate::live::render_lane(&moved_slot, SAMPLE_RATE, moved_slot.tracks[0].id)
            .expect("a rendered lane");
        assert_eq!(lane.delay, 0);
        playback
            .swap_lane(moved_slot.tracks[0].id, lane.samples)
            .expect("swap");
        let mut output = vec![0.0_f32; len];
        playback.fill(&mut output);
        assert_eq!(playback.collect_garbage(), 1, "the old buffer comes back");
        let offline = render_offline(&moved_slot, SAMPLE_RATE);
        let worst = worst_difference(&output, &offline.samples[..len]);
        assert!(worst < 1e-5, "live and offline differ by {worst}");
        // A synth lane is not rendered.
        assert!(crate::live::render_lane(&song, SAMPLE_RATE, song.tracks[0].id).is_none());
    }
}
