//! Headless synthesis and WAV export.
//!
//! This is the renderer the desktop shell will play through once a realtime
//! backend exists: one deterministic function from song to samples, so tests
//! can check timing and level without opening an audio device.

pub mod analysis;
pub mod chords;
pub mod decode;
pub mod drums;
pub mod edit;
pub mod l6max;
pub mod link;
pub mod master;
pub mod metronome;
pub mod mix;
pub mod monitor;
pub mod player;
pub mod record;
pub mod soundfont;
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
            let value = (sample.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
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
            let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
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
#[must_use]
pub fn render(song: &Song, sample_rate: u32) -> RenderedAudio {
    render_with_soundfont(song, sample_rate, &mut |path| {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| soundfont::parse(&bytes).ok())
    })
}

/// Renders every track, loading SoundFonts through `load`.
#[must_use]
pub fn render_with_soundfont(
    song: &Song,
    sample_rate: u32,
    load: &mut dyn FnMut(&str) -> Option<soundfont::SoundFont>,
) -> RenderedAudio {
    let mut end_tick = song.end_tick();
    for track in &song.tracks {
        if let Some(reference) = &track.audio {
            end_tick = end_tick.max(reference.start.saturating_add(reference.length));
        }
    }
    let duration = song.tempo_map.duration_seconds(0, end_tick.max(1)) + 0.75;
    let frames = (duration * f64::from(sample_rate)).ceil() as usize;
    let mut samples = vec![0.0_f32; frames * usize::from(CHANNELS)];
    let mut bus_buffers: Vec<Vec<f32>> = song
        .buses
        .iter()
        .map(|_| vec![0.0_f32; frames * usize::from(CHANNELS)])
        .collect();

    let track_latencies: Vec<usize> = song
        .tracks
        .iter()
        .map(|track| {
            if song.is_audible(track) {
                chain_latency(&track.plugins, sample_rate)
            } else {
                0
            }
        })
        .collect();
    let max_track_latency = track_latencies.iter().copied().max().unwrap_or(0);
    for (track_index, track) in song
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| song.is_audible(track))
    {
        let timbre = timbre_for(track);
        let sampled = track.soundfont.as_ref().and_then(|reference| {
            load(&reference.path).map(|font| (font, reference.bank, reference.program))
        });
        let mut track_buffer = vec![0.0_f32; frames * usize::from(CHANNELS)];
        if let Some(mut instrument) = instrument_for(track, sample_rate) {
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
                let (left_gain, right_gain) = gains_at(track, note.start);
                let patch = track.patch.map(|mut patch| {
                    if let Some(brightness) = track.brightness_at(note.start) {
                        patch.brightness = brightness;
                    }
                    patch
                });
                let send_level = track.send_at(note.start).min(127.0) / 100.0;
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
                            let left = left_audio[offset] * left_gain;
                            let right = right_audio[offset] * right_gain;
                            track_buffer[frame * 2] += left;
                            track_buffer[frame * 2 + 1] += right;
                            if let Some(index) = track
                                .send
                                .and_then(|id| song.buses.iter().position(|bus| bus.id == id))
                            {
                                bus_buffers[index][frame * 2] += left * send_level;
                                bus_buffers[index][frame * 2 + 1] += right * send_level;
                            }
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
                    let left = value * left_gain;
                    let right = value * right_gain;
                    track_buffer[frame * 2] += left;
                    track_buffer[frame * 2 + 1] += right;
                    if let Some(index) = track
                        .send
                        .and_then(|id| song.buses.iter().position(|bus| bus.id == id))
                    {
                        bus_buffers[index][frame * 2] += left * send_level;
                        bus_buffers[index][frame * 2 + 1] += right * send_level;
                    }
                }
            }
        }
        apply_track_plugins(song, track, &mut track_buffer, sample_rate);
        delay_buffer(
            &mut track_buffer,
            max_track_latency.saturating_sub(track_latencies[track_index]),
        );
        if let Some(index) = track
            .bus
            .and_then(|id| song.buses.iter().position(|bus| bus.id == id))
        {
            for (target, value) in bus_buffers[index].iter_mut().zip(&track_buffer) {
                *target += *value;
            }
        } else {
            for (target, value) in samples.iter_mut().zip(&track_buffer) {
                *target += *value;
            }
        }
    }
    let bus_latencies: Vec<usize> = song
        .buses
        .iter()
        .map(|bus| chain_latency(&bus.plugins, sample_rate))
        .collect();
    let max_bus_latency = bus_latencies.iter().copied().max().unwrap_or(0);
    for (index, bus) in song.buses.iter().enumerate() {
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
    apply_plugins(&song.master_plugins, &mut samples, sample_rate);
    for sample in &mut samples {
        *sample = sample.clamp(-1.0, 1.0);
    }
    RenderedAudio {
        samples,
        sample_rate,
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
    let (left_gain, right_gain) = track_gains(track);
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
            buffer[(position + index) * 2] += left[index] * left_gain;
            buffer[(position + index) * 2 + 1] += right[index] * right_gain;
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

    #[cfg(target_os = "macos")]
    fn test_gain_bundle() -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let debug = exe.parent()?.parent()?;
        let library = debug.join("liblazy_arranger_test_plugin.dylib");
        if !library.is_file() {
            return None;
        }
        let root =
            std::env::temp_dir().join(format!("lazy-arranger-pdc-plugin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let macos = root.join("Test.vst3").join("Contents").join("MacOS");
        std::fs::create_dir_all(&macos).ok()?;
        std::fs::copy(&library, macos.join("Test")).ok()?;
        Some(root.join("Test.vst3"))
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn latency_compensation_aligns_a_delayed_plugin_track() {
        use project::song::{PluginFormat, PluginSlot};
        let Some(bundle) = test_gain_bundle() else {
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
        let Some(bundle) = test_gain_bundle() else {
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
        let Some(bundle) = test_gain_bundle() else {
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
        let sampled = render_with_soundfont(&song, SAMPLE_RATE, &mut |_| Some(font.clone()));
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
}
