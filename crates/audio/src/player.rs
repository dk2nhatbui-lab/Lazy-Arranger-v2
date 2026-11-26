//! Device playback through the real-time engine.
//!
//! The device callback owns an [`Engine`] and runs its graph; it does no
//! locking, allocation or freeing. See [`crate::engine`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::RenderedAudio;
use crate::engine::{Engine, EngineControl, Graph, Param, Transport, engine};
use crate::live::LiveRender;
use crate::master::{Master, Meter};

/// Remembers the last parameters sent, so only changes go to the audio
/// thread.
#[derive(Debug, Default)]
struct Sent {
    tracks: Vec<(project::TrackId, f32, f32)>,
    feeds: Vec<(project::TrackId, u8, f32, f32)>,
    buses: Vec<(u8, f32, f32)>,
    master: Option<f32>,
    effects: Option<(f32, f32)>,
    limiter: Option<bool>,
}

/// Sends the song's mixer to the engine: lane gains and the master stage.
fn send_mix(
    control: &mut EngineControl,
    sent: &mut Sent,
    song: &project::song::Song,
    master: Master,
) {
    control.collect_garbage();
    let buses = crate::live::live_buses(song);
    for track in &song.tracks {
        let routes = crate::live::lane_routes(song, &buses, track);
        for (bus, left, right) in routes.feeds {
            let known = sent
                .feeds
                .iter_mut()
                .find(|entry| entry.0 == track.id && entry.1 == bus);
            if known
                .as_ref()
                .is_some_and(|entry| entry.2 == left && entry.3 == right)
            {
                continue;
            }
            if !control.send(Param::BusFeed {
                track: track.id,
                bus,
                left,
                right,
            }) {
                continue;
            }
            match known {
                Some(entry) => *entry = (track.id, bus, left, right),
                None => sent.feeds.push((track.id, bus, left, right)),
            }
        }
        let (left, right) = routes.direct;
        let known = sent.tracks.iter_mut().find(|entry| entry.0 == track.id);
        if known
            .as_ref()
            .is_some_and(|entry| entry.1 == left && entry.2 == right)
        {
            continue;
        }
        if !control.send(Param::TrackGain {
            track: track.id,
            left,
            right,
        }) {
            // Full: try again next time.
            continue;
        }
        match known {
            Some(entry) => *entry = (track.id, left, right),
            None => sent.tracks.push((track.id, left, right)),
        }
    }
    for (bus, left, right) in crate::live::bus_gains(song, &buses) {
        let known = sent.buses.iter_mut().find(|entry| entry.0 == bus);
        if known
            .as_ref()
            .is_some_and(|entry| entry.1 == left && entry.2 == right)
        {
            continue;
        }
        if !control.send(Param::BusGain { bus, left, right }) {
            continue;
        }
        match known {
            Some(entry) => *entry = (bus, left, right),
            None => sent.buses.push((bus, left, right)),
        }
    }
    let gain = crate::master::gain(master.gain_db);
    if sent.master != Some(gain) && control.send(Param::MasterGain(gain)) {
        sent.master = Some(gain);
    }
    let effects = (master.chorus.clamp(0.0, 1.0), master.reverb.clamp(0.0, 1.0));
    if sent.effects != Some(effects)
        && control.send(Param::MasterEffects {
            chorus: effects.0,
            reverb: effects.1,
        })
    {
        sent.effects = Some(effects);
    }
    if sent.limiter != Some(master.limiter) && control.send(Param::Limiter(master.limiter)) {
        sent.limiter = Some(master.limiter);
    }
}

/// An engine driven by hand instead of by a device: for tests, offline
/// bounces and tools.
#[derive(Debug)]
pub struct Playback {
    engine: Engine,
    control: EngineControl,
    transport: Arc<Transport>,
    sent: Sent,
}

impl Playback {
    /// Wraps a finished mix.
    #[must_use]
    pub fn new(audio: RenderedAudio) -> Self {
        Self::new_live(LiveRender::from_audio(audio))
    }

    /// Wraps a live render; its lane gains and master gain follow
    /// [`Playback::apply_mix`].
    #[must_use]
    pub fn new_live(render: LiveRender) -> Self {
        let (engine, control, transport) = engine(Graph::from_render(render));
        Self {
            engine,
            control,
            transport,
            sent: Sent::default(),
        }
    }

    /// The shared transport.
    #[must_use]
    pub fn transport(&self) -> &Arc<Transport> {
        &self.transport
    }

    /// Sample rate in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.transport.sample_rate()
    }

    /// Starts or resumes playback.
    pub fn play(&self) {
        self.transport.play();
    }

    /// Pauses without losing the position.
    pub fn pause(&self) {
        self.transport.pause();
    }

    /// Stops and rewinds.
    pub fn stop(&self) {
        self.transport.stop();
    }

    /// Moves to a frame without changing the play state.
    pub fn seek_frame(&self, frame: usize) {
        self.transport.seek_frame(frame);
    }

    /// Moves to a time in seconds without changing the play state.
    pub fn seek_seconds(&self, seconds: f64) {
        self.transport.seek_seconds(seconds);
    }

    /// Enables or disables looping over a time range.
    pub fn set_loop(&self, enabled: bool, start_seconds: f64, end_seconds: f64) {
        self.transport.set_loop(enabled, start_seconds, end_seconds);
    }

    /// Times playback wrapped from the loop end to its start.
    #[must_use]
    pub fn loop_wraps(&self) -> u64 {
        self.transport.loop_wraps()
    }

    /// Current position in frames.
    #[must_use]
    pub fn frame(&self) -> usize {
        self.transport.frame()
    }

    /// Current position in seconds.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.transport.seconds()
    }

    /// True while samples are being produced.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.transport.is_playing()
    }

    /// Level of what was played since the last read.
    #[must_use]
    pub fn meter(&self) -> Meter {
        self.transport.meter()
    }

    /// Sends the song's mixer: heard from the next quantum, ramped.
    pub fn apply_mix(&mut self, song: &project::song::Song, master: Master) {
        send_mix(&mut self.control, &mut self.sent, song, master);
    }

    /// Replaces the graph at the next buffer, keeping the position.
    ///
    /// # Errors
    ///
    /// Returns a message when earlier graphs are still waiting.
    pub fn load(&mut self, render: LiveRender) -> Result<(), String> {
        let render = resample_live(render, self.sample_rate());
        self.control
            .send_graph(Graph::from_render(render))
            .map_err(|_| String::from("the audio engine is busy"))?;
        self.sent = Sent::default();
        Ok(())
    }

    /// Replaces one track's synth lane, for a note edit while playing.
    ///
    /// # Errors
    ///
    /// Returns a message when earlier lanes are still waiting.
    pub fn swap_synth(
        &mut self,
        track: project::TrackId,
        synth: crate::synth::SynthLane,
    ) -> Result<(), String> {
        self.control
            .send_synth(track, synth)
            .map_err(|_| String::from("the audio engine is busy"))
    }

    /// Replaces one plug-in chain, for a plug-in edit while playing.
    ///
    /// # Errors
    ///
    /// Returns a message when earlier chains are still waiting.
    pub fn swap_inserts(
        &mut self,
        chain: crate::inserts::ChainId,
        inserts: crate::inserts::Inserts,
    ) -> Result<(), String> {
        self.control
            .send_inserts(chain, inserts)
            .map_err(|_| String::from("the audio engine is busy"))
    }

    /// Frees graphs the engine has handed back.
    pub fn collect_garbage(&mut self) -> usize {
        self.control.collect_garbage()
    }

    /// Runs the engine for one output buffer, as the device callback does.
    pub fn fill(&mut self, output: &mut [f32]) {
        self.engine.fill(output);
    }
}

/// Picks a supported stereo output configuration at the nearest rate.
fn stereo_config(
    device: &cpal::Device,
    source_rate: u32,
) -> Result<cpal::SupportedStreamConfig, String> {
    let configs = device
        .supported_output_configs()
        .map_err(|error| format!("no output configurations: {error}"))?;
    let mut best: Option<cpal::SupportedStreamConfig> = None;
    let mut best_distance = u32::MAX;
    for range in configs {
        if range.channels() != 2
            || !matches!(
                range.sample_format(),
                cpal::SampleFormat::F32 | cpal::SampleFormat::I16
            )
        {
            continue;
        }
        let min = range.min_sample_rate().0;
        let max = range.max_sample_rate().0;
        let rate = source_rate.clamp(min, max);
        let distance = source_rate.abs_diff(rate);
        if distance < best_distance {
            best_distance = distance;
            best = Some(range.with_sample_rate(cpal::SampleRate(rate)));
        }
    }
    best.ok_or_else(|| String::from("the output device has no stereo F32/I16 configuration"))
}

/// Resamples every part of a live render.
fn resample_live(mut render: LiveRender, target_rate: u32) -> LiveRender {
    let source_rate = render.fixed.sample_rate;
    if source_rate == target_rate || source_rate == 0 || target_rate == 0 {
        return render;
    }
    // Synth lanes and insert chains run at the rate they were built for:
    // lanes are baked, and bus and master chains open again at the new rate.
    render.bake_inserts();
    let scale = |frames: usize| {
        (frames as f64 * f64::from(target_rate) / f64::from(source_rate)).round() as usize
    };
    render.bus_delay = scale(render.bus_delay);
    for bus in &mut render.buses {
        bus.inserts = crate::inserts::Inserts::from_slots(&bus.slots, target_rate);
        bus.delay = scale(bus.delay);
    }
    if render.master_inserts.is_some() {
        render.master_inserts = Some(crate::inserts::Inserts::from_slots(
            &render.master_slots,
            target_rate,
        ));
    }
    render.fixed = resample_stereo(std::mem::take(&mut render.fixed), target_rate);
    for lane in &mut render.lanes {
        *lane = resample_stereo(
            RenderedAudio {
                samples: std::mem::take(lane),
                sample_rate: source_rate,
            },
            target_rate,
        )
        .samples;
    }
    render
}

/// Resamples interleaved stereo audio with linear interpolation.
#[must_use]
pub(crate) fn resample_stereo(audio: RenderedAudio, target_rate: u32) -> RenderedAudio {
    if audio.sample_rate == target_rate || audio.sample_rate == 0 || target_rate == 0 {
        return audio;
    }
    let source_frames = audio.samples.len() / 2;
    let ratio = f64::from(target_rate) / f64::from(audio.sample_rate);
    let target_frames = ((source_frames as f64) * ratio).ceil() as usize;
    let mut samples = Vec::with_capacity(target_frames.saturating_mul(2));
    for frame in 0..target_frames {
        let source = frame as f64 / ratio;
        let index = source.floor() as usize;
        let fraction = (source - index as f64) as f32;
        let next = (index + 1).min(source_frames.saturating_sub(1));
        for channel in 0..2 {
            let a = audio
                .samples
                .get(index * 2 + channel)
                .copied()
                .unwrap_or(0.0);
            let b = audio.samples.get(next * 2 + channel).copied().unwrap_or(a);
            samples.push(a + (b - a) * fraction);
        }
    }
    RenderedAudio {
        samples,
        sample_rate: target_rate,
    }
}

/// The name of the audio system in use, such as "CoreAudio", "ALSA" or "WASAPI".
#[must_use]
pub fn host_name() -> String {
    cpal::default_host().id().name().to_string()
}

/// Names of available output devices.
#[must_use]
pub fn output_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.output_devices()
        .map(|devices| {
            devices
                .filter_map(|device| device.name().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn find_output_device(name: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    match name {
        Some(name) => {
            let mut available = Vec::new();
            let mut found = None;
            if let Ok(devices) = host.output_devices() {
                for device in devices {
                    let label = device.name().unwrap_or_else(|_| String::from("unknown"));
                    if label == name {
                        found = Some(device);
                        break;
                    }
                    available.push(label);
                }
            }
            found.ok_or_else(|| {
                format!(
                    "no output named {name:?}; available: {}",
                    available.join(", ")
                )
            })
        }
        None => host
            .default_output_device()
            .ok_or_else(|| String::from("no default output device")),
    }
}

/// Preferred stereo sample rate for an output device, or `fallback` if unavailable.
///
/// Callers can render directly at this rate on a background thread so opening
/// [`Player::new_on`] needs no resampling on the UI thread.
#[must_use]
pub fn preferred_sample_rate(name: Option<&str>, fallback: u32) -> u32 {
    find_output_device(name)
        .and_then(|device| stereo_config(&device, fallback))
        .map(|config| config.sample_rate().0)
        .unwrap_or(fallback)
}

/// A live output stream.
pub struct Player {
    stream: cpal::Stream,
    transport: Arc<Transport>,
    /// Control side of the engine. Locked only on the control thread; the
    /// audio thread never sees this lock.
    control: Mutex<(EngineControl, Sent)>,
    sample_rate: u32,
    buffer_frames: Option<u32>,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Player").finish_non_exhaustive()
    }
}

impl Player {
    /// Opens the default output device and starts paused.
    ///
    /// # Errors
    ///
    /// Returns a message when no device or supported configuration exists.
    pub fn new(audio: RenderedAudio) -> Result<Self, String> {
        Self::new_on(audio, None, None)
    }

    /// Opens a named output device with an optional fixed buffer size.
    ///
    /// # Errors
    ///
    /// Returns a message when the device is missing or has no configuration.
    pub fn new_on(
        audio: RenderedAudio,
        name: Option<&str>,
        buffer_frames: Option<u32>,
    ) -> Result<Self, String> {
        Self::new_live_on(LiveRender::from_audio(audio), name, buffer_frames)
    }

    /// Opens a named output device for a live render, whose track gains and
    /// master gain follow [`Player::apply_mix`] while it plays.
    ///
    /// # Errors
    ///
    /// Returns a message when the device is missing or has no configuration.
    pub fn new_live_on(
        render: LiveRender,
        name: Option<&str>,
        buffer_frames: Option<u32>,
    ) -> Result<Self, String> {
        let device = find_output_device(name)?;
        let config = stereo_config(&device, render.sample_rate())?;
        let sample_format = config.sample_format();
        let mut stream_config: cpal::StreamConfig = config.into();
        if let Some(frames) = buffer_frames {
            stream_config.buffer_size = cpal::BufferSize::Fixed(frames.max(32));
        }
        let sample_rate = stream_config.sample_rate.0;
        let (mut engine, control, transport) =
            engine(Graph::from_render(resample_live(render, sample_rate)));
        // Report the first stream error only, so a failing device cannot
        // flood stderr from the audio thread.
        let reported = Arc::new(AtomicBool::new(false));
        let error_callback = move |error| {
            if !reported.swap(true, Ordering::Relaxed) {
                eprintln!("audio stream error: {error}");
            }
        };
        let actual_buffer = match stream_config.buffer_size {
            cpal::BufferSize::Fixed(frames) => Some(frames),
            cpal::BufferSize::Default => None,
        };
        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &stream_config,
                move |output: &mut [f32], _| {
                    engine.fill(output);
                },
                error_callback,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                &stream_config,
                move |output: &mut [i16], _| {
                    let mut chunk_buf = [0.0_f32; 512];
                    for dst in output.chunks_mut(chunk_buf.len()) {
                        let slice = &mut chunk_buf[..dst.len()];
                        engine.fill(slice);
                        for (slot, value) in dst.iter_mut().zip(slice.iter().copied()) {
                            *slot = (crate::spsc::sanitize(value) * f32::from(i16::MAX)) as i16;
                        }
                    }
                },
                error_callback,
                None,
            ),
            other => return Err(format!("unsupported sample format {other:?}")),
        }
        .map_err(|error| format!("could not open the output stream: {error}"))?;
        stream
            .play()
            .map_err(|error| format!("could not start the stream: {error}"))?;
        Ok(Self {
            stream,
            transport,
            control: Mutex::new((control, Sent::default())),
            sample_rate,
            buffer_frames: actual_buffer,
        })
    }

    /// Output sample rate.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Actual fixed output buffer size, when known.
    #[must_use]
    pub fn buffer_frames(&self) -> Option<u32> {
        self.buffer_frames
    }

    /// Buffer duration in milliseconds, when known.
    #[must_use]
    pub fn buffer_latency_ms(&self) -> Option<f64> {
        self.buffer_frames
            .map(|frames| f64::from(frames) / f64::from(self.sample_rate.max(1)) * 1000.0)
    }

    /// Starts playback from the current position.
    pub fn play(&self) {
        self.transport.play();
    }

    /// Moves the live playback position, keeping the play state.
    ///
    /// # Errors
    ///
    /// Always succeeds; retained as `Result` for caller compatibility.
    pub fn seek_seconds(&self, seconds: f64) -> Result<(), String> {
        self.transport.seek_seconds(seconds);
        Ok(())
    }

    /// Enables or disables seamless in-callback looping over a time range.
    pub fn set_loop(&self, enabled: bool, start_seconds: f64, end_seconds: f64) {
        self.transport.set_loop(enabled, start_seconds, end_seconds);
    }

    /// Number of times playback wrapped from loop end back to loop start.
    #[must_use]
    pub fn loop_wraps(&self) -> u64 {
        self.transport.loop_wraps()
    }

    /// Pauses playback.
    pub fn pause(&self) {
        self.transport.pause();
    }

    /// Stops and rewinds.
    pub fn stop(&self) {
        self.transport.stop();
    }

    /// Current frame position.
    #[must_use]
    pub fn frame(&self) -> usize {
        self.transport.frame()
    }

    /// Current playback time in seconds using the output device's sample rate.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.transport.seconds()
    }

    /// True while playing.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.transport.is_playing()
    }

    /// Sends the song's mixer to the engine: track gains and the master
    /// gain, heard from the next 32-frame quantum.
    pub fn apply_mix(&self, song: &project::song::Song, master: Master) {
        if let Ok(mut guard) = self.control.lock() {
            let (control, sent) = &mut *guard;
            send_mix(control, sent, song, master);
        }
    }

    /// Replaces what plays with a new render at the next buffer, keeping the
    /// position and the stream. The old graph is freed here, not on the
    /// audio thread.
    ///
    /// # Errors
    ///
    /// Returns a message when earlier graphs are still waiting.
    pub fn load(&self, render: LiveRender) -> Result<(), String> {
        let graph = Graph::from_render(resample_live(render, self.sample_rate));
        let mut guard = self
            .control
            .lock()
            .map_err(|_| String::from("the audio engine is unavailable"))?;
        let (control, sent) = &mut *guard;
        control
            .send_graph(graph)
            .map_err(|_| String::from("the audio engine is busy"))?;
        // Send the whole mixer again for the new graph.
        *sent = Sent::default();
        Ok(())
    }

    /// Replaces one track's synth lane at the next buffer, for a note edit
    /// while playing. The lane must be built at [`Player::sample_rate`].
    ///
    /// # Errors
    ///
    /// Returns a message when earlier lanes are still waiting.
    pub fn swap_synth(
        &self,
        track: project::TrackId,
        synth: crate::synth::SynthLane,
    ) -> Result<(), String> {
        let mut guard = self
            .control
            .lock()
            .map_err(|_| String::from("the audio engine is unavailable"))?;
        guard
            .0
            .send_synth(track, synth)
            .map_err(|_| String::from("the audio engine is busy"))
    }

    /// Replaces one plug-in chain (a track's, a bus's or the master's) at
    /// the next buffer, for a plug-in edit while playing. The chain must be
    /// built at [`Player::sample_rate`]. The old chain is freed here, not on
    /// the audio thread.
    ///
    /// # Errors
    ///
    /// Returns a message when earlier chains are still waiting.
    pub fn swap_inserts(
        &self,
        chain: crate::inserts::ChainId,
        inserts: crate::inserts::Inserts,
    ) -> Result<(), String> {
        let mut guard = self
            .control
            .lock()
            .map_err(|_| String::from("the audio engine is unavailable"))?;
        guard
            .0
            .send_inserts(chain, inserts)
            .map_err(|_| String::from("the audio engine is busy"))
    }

    /// Level of what was played since the last read.
    #[must_use]
    pub fn meter(&self) -> Meter {
        self.transport.meter()
    }

    /// Drops the stream; the device is released.
    pub fn release(self) {
        drop(self.stream);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playback_with(frames: usize) -> Playback {
        playback_with_rate(frames, 48_000)
    }

    fn playback_with_rate(frames: usize, sample_rate: u32) -> Playback {
        let samples = (0..frames * 2).map(|index| index as f32).collect();
        Playback::new(RenderedAudio {
            samples,
            sample_rate,
        })
    }

    #[test]
    fn resampling_preserves_duration_and_channels() {
        let audio = RenderedAudio {
            samples: (0..16).map(|value| value as f32).collect(),
            sample_rate: 48_000,
        };
        let half = resample_stereo(audio.clone(), 24_000);
        assert_eq!(half.sample_rate, 24_000);
        assert_eq!(half.samples.len(), 8);
        assert_eq!(half.samples[0], 0.0);
        assert_eq!(half.samples[1], 1.0);
        assert_eq!(half.samples[6], 12.0);
        let same = resample_stereo(audio, 48_000);
        assert_eq!(same.samples.len(), 16);
    }

    #[test]
    fn seconds_uses_device_sample_rate() {
        for rate in [44_100_u32, 48_000, 96_000] {
            let state = playback_with_rate(rate as usize * 2, rate);
            state.seek_frame(rate as usize);
            assert!(
                (state.seconds() - 1.0).abs() < 1.0e-9,
                "1s at {rate} Hz should report 1.0s, got {}",
                state.seconds()
            );
            state.seek_seconds(1.5);
            assert_eq!(state.frame(), (rate as usize * 3) / 2);
            assert!(
                (state.seconds() - 1.5).abs() < 1.0e-6,
                "1.5s at {rate} Hz should report 1.5s, got {}",
                state.seconds()
            );
        }
    }

    #[test]
    fn looping_wraps_seamlessly_across_buffer_boundaries() {
        // 44.1 kHz playback looping from 0.25s..0.75s (half-second region = 22,050 frames).
        let mut state = playback_with_rate(44_100, 44_100);
        state.set_loop(true, 0.25, 0.75);
        state.seek_seconds(0.25);
        state.play();

        let start_frame = 44_100 / 4;
        let end_frame = (44_100 * 3) / 4;
        let loop_frames = end_frame - start_frame;
        assert_eq!(state.frame(), start_frame);

        // Drain 2.5 loop iterations in odd-sized chunks to cross loop boundaries mid-buffer.
        let mut out = vec![0.0_f32; 512];
        let mut expected_frame = start_frame;
        let total_chunks = (loop_frames * 5) / (2 * 256);
        for _ in 0..total_chunks {
            state.fill(&mut out);
            for frame_slice in out.as_chunks::<2>().0 {
                let expected_left = (expected_frame * 2) as f32;
                let expected_right = (expected_frame * 2 + 1) as f32;
                assert_eq!(frame_slice[0], expected_left);
                assert_eq!(frame_slice[1], expected_right);
                expected_frame += 1;
                if expected_frame >= end_frame {
                    expected_frame = start_frame;
                }
            }
        }
        assert!(state.is_playing());
        assert_eq!(state.loop_wraps(), 2);
        assert!(state.seconds() >= 0.25 && state.seconds() < 0.75);
    }

    #[test]
    fn concurrent_meter_and_frame_reads_never_drop_samples() {
        let frames = 44_100;
        let samples = vec![0.5_f32; frames * 2];
        let mut playback = Playback::new(RenderedAudio {
            samples,
            sample_rate: 44_100,
        });
        playback.set_loop(true, 0.0, 1.0);
        playback.play();

        let reader = Arc::clone(playback.transport());
        let done = Arc::new(AtomicBool::new(false));
        let done_reader = Arc::clone(&done);
        let ui_thread = std::thread::spawn(move || {
            while !done_reader.load(Ordering::Relaxed) {
                let meter = reader.meter();
                assert!(meter.peak == 0.0 || (meter.peak - 0.5).abs() < 1.0e-6);
                let _ = reader.seconds();
            }
        });

        let mut buffer = [0.0_f32; 256];
        for _ in 0..200 {
            playback.fill(&mut buffer);
            assert!(
                buffer.iter().all(|&sample| (sample - 0.5).abs() < 1.0e-6),
                "lock-free callback must never output silence dropouts"
            );
        }
        done.store(true, Ordering::Relaxed);
        ui_thread.join().expect("ui reader thread");
    }

    #[test]
    fn a_missing_output_device_is_reported() {
        let error = Player::new_on(
            RenderedAudio {
                samples: vec![0.0; 2],
                sample_rate: 48_000,
            },
            Some("lazy-arranger-no-such-device"),
            None,
        )
        .expect_err("missing device");
        assert!(error.contains("no output named"), "{error}");
    }

    #[test]
    fn the_meter_reports_what_was_played() {
        let mut playback = Playback::new(RenderedAudio {
            samples: vec![0.25, -0.25, 1.0, -1.0, 0.0, 0.0],
            sample_rate: 48_000,
        });
        assert_eq!(playback.meter(), Meter::default());
        playback.play();
        let mut output = [0.0_f32; 2];
        playback.fill(&mut output);
        assert!((playback.meter().peak - 0.25).abs() < 1.0e-6);
        playback.fill(&mut output);
        assert!((playback.meter().peak - 1.0).abs() < 1.0e-6);
        // The peak is the largest since the last read.
        assert_eq!(playback.meter().peak, 0.0);
    }

    #[test]
    fn fill_copies_samples_only_while_playing() {
        let mut state = playback_with(4);
        let mut output = [1.0_f32; 4];
        state.fill(&mut output);
        assert_eq!(output, [0.0; 4], "paused means silence");

        state.play();
        let mut output = [0.0_f32; 4];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 1.0, 2.0, 3.0]);
        assert_eq!(state.frame(), 2);
        assert!(state.is_playing());
    }

    #[test]
    fn seeking_moves_without_changing_the_play_state() {
        let state = playback_with(96_000);
        state.play();
        state.seek_seconds(0.5);
        assert_eq!(state.frame(), 24_000);
        assert!(state.is_playing());
        state.pause();
        state.seek_frame(10);
        assert_eq!(state.frame(), 10);
        assert!(!state.is_playing());
        state.seek_frame(usize::MAX);
        assert_eq!(state.frame(), 96_000);
    }

    #[test]
    fn reaching_the_end_stops_and_rewinds() {
        let mut state = playback_with(2);
        state.play();
        let mut output = [0.0_f32; 4];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 1.0, 2.0, 3.0]);
        assert!(!state.is_playing());
        assert_eq!(state.frame(), 0);
    }

    #[test]
    fn pause_keeps_the_position_and_stop_rewinds() {
        let mut state = playback_with(8);
        state.play();
        let mut output = [0.0_f32; 4];
        state.fill(&mut output);
        state.pause();
        assert_eq!(state.frame(), 2);
        state.play();
        let mut output = [0.0_f32; 4];
        state.fill(&mut output);
        assert_eq!(output, [4.0, 5.0, 6.0, 7.0]);
        state.stop();
        assert_eq!(state.frame(), 0);
        assert!(!state.is_playing());
    }

    #[test]
    fn an_odd_output_slot_gets_silence_and_frames_stay_aligned() {
        let mut state = playback_with(4);
        state.play();
        let mut output = [0.5_f32; 3];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 1.0, 0.0]);
        let mut output = [0.5_f32; 2];
        state.fill(&mut output);
        assert_eq!(output, [2.0, 3.0]);
    }

    #[test]
    fn non_finite_samples_play_as_silence() {
        let mut state = Playback::new(RenderedAudio {
            sample_rate: 48_000,
            samples: vec![f32::NAN, f32::INFINITY, 0.25],
        });
        state.play();
        let mut output = [1.0_f32; 2];
        state.fill(&mut output);
        assert_eq!(output, [0.0, 0.0]);
    }

    /// Master settings with no limiter or effects.
    fn dry(gain_db: f32) -> Master {
        Master {
            gain_db,
            limiter: false,
            reverb: 0.0,
            chorus: 0.0,
        }
    }

    fn live_song(volume: u8) -> project::song::Song {
        let mut song = project::song::Song::default();
        let mut track = project::song::Track::new(project::TrackId::new(1).expect("track"), "Keys");
        track.volume = volume;
        song.tracks.push(track);
        song
    }

    fn live_playback() -> Playback {
        let song = live_song(100);
        Playback::new_live(LiveRender {
            fixed: RenderedAudio {
                samples: vec![0.0; 2048],
                sample_rate: 48_000,
            },
            lanes: vec![vec![0.5; 2048]],
            tracks: vec![song.tracks[0].id],
            gains: vec![(1.0, 1.0)],
            master_gain: 1.0,
            limiter: false,
            ..LiveRender::default()
        })
    }

    #[test]
    fn a_fader_change_is_heard_in_the_next_buffer() {
        let mut playback = live_playback();
        playback.play();
        let mut output = [0.0_f32; 64];
        playback.fill(&mut output);
        assert!(output.iter().all(|sample| (sample - 0.5).abs() < 1e-6));

        playback.apply_mix(&live_song(50), dry(0.0));
        playback.fill(&mut output);
        // One buffer ramps from the old gain to the new one, without a click.
        assert!((output[0] - 0.5).abs() < 1e-6);
        assert!((output[62] - 0.25).abs() < 0.01);
        assert!(output.windows(4).all(|pair| pair[2] <= pair[0] + 1e-6));
        playback.fill(&mut output);
        assert!(output.iter().all(|sample| (sample - 0.25).abs() < 1e-6));
    }

    #[test]
    fn mute_and_master_gain_apply_live() {
        let mut playback = live_playback();
        playback.play();
        let mut song = live_song(100);
        song.tracks[0].muted = true;
        playback.apply_mix(&song, dry(0.0));
        let mut output = [0.0_f32; 64];
        playback.fill(&mut output);
        playback.fill(&mut output);
        assert!(output.iter().all(|sample| sample.abs() < 1e-6));

        playback.apply_mix(&live_song(100), dry(-6.0));
        playback.fill(&mut output);
        let _ = playback.meter();
        playback.fill(&mut output);
        let expected = 0.5 * crate::master::gain(-6.0);
        assert!(output.iter().all(|sample| (sample - expected).abs() < 1e-5));
        assert!((playback.meter().peak - expected).abs() < 1e-5);
    }

    #[test]
    fn the_engine_matches_the_rendered_mix_exactly() {
        let song = live_song(80);
        let render = LiveRender {
            fixed: RenderedAudio {
                samples: (0..512)
                    .map(|index| (index as f32 * 0.01).sin() * 0.3)
                    .collect(),
                sample_rate: 48_000,
            },
            lanes: vec![
                (0..512)
                    .map(|index| (index as f32 * 0.03).cos() * 0.4)
                    .collect(),
            ],
            tracks: vec![song.tracks[0].id],
            gains: vec![(0.8, 0.6)],
            master_gain: crate::master::gain(-3.0),
            limiter: true,
            ..LiveRender::default()
        };
        let expected = plain_mix(&render);
        let mut playback = Playback::new_live(render);
        playback.play();
        let mut output = vec![0.0_f32; 512];
        // Odd buffer sizes cross quantum edges.
        for chunk in output.chunks_mut(94) {
            playback.fill(chunk);
        }
        assert_eq!(output, expected, "bit-exact with the offline mix");
    }

    /// The mix of a render with one lane and no buses, worked out by hand:
    /// the lane at its gains plus the fixed part, the master effects, then
    /// the master gain and limiter.
    fn plain_mix(render: &LiveRender) -> Vec<f32> {
        let (left, right) = render.gains[0];
        let mut audio = RenderedAudio {
            samples: render
                .fixed
                .samples
                .iter()
                .zip(&render.lanes[0])
                .enumerate()
                .map(|(index, (fixed, lane))| {
                    fixed + lane * if index % 2 == 0 { left } else { right }
                })
                .collect(),
            sample_rate: render.fixed.sample_rate,
        };
        crate::master::apply_effects(
            &mut audio,
            Master {
                gain_db: 0.0,
                limiter: false,
                reverb: render.reverb,
                chorus: render.chorus,
            },
        );
        audio
            .samples
            .iter()
            .map(|sample| crate::master::finish(sample * render.master_gain, render.limiter))
            .collect()
    }

    #[test]
    fn a_new_graph_keeps_the_position_and_old_graphs_are_freed_off_the_audio_thread() {
        let mut playback = Playback::new(RenderedAudio {
            samples: vec![0.25; 4096],
            sample_rate: 48_000,
        });
        playback.play();
        let mut output = [0.0_f32; 64];
        playback.fill(&mut output);
        assert_eq!(playback.frame(), 32);

        playback
            .load(LiveRender::from_audio(RenderedAudio {
                samples: vec![0.5; 4096],
                sample_rate: 48_000,
            }))
            .expect("load");
        playback.fill(&mut output);
        assert!(output.iter().all(|sample| (sample - 0.5).abs() < 1e-6));
        assert_eq!(playback.frame(), 64, "the swap keeps the position");
        // The callback handed the old graph back; the control side frees it.
        assert_eq!(playback.collect_garbage(), 1);
    }

    #[test]
    fn graphs_wait_when_the_engine_is_busy() {
        let mut playback = Playback::new(RenderedAudio {
            samples: vec![0.0; 64],
            sample_rate: 48_000,
        });
        let graph = || {
            LiveRender::from_audio(RenderedAudio {
                samples: vec![0.0; 64],
                sample_rate: 48_000,
            })
        };
        // The callback has not run, so graphs pile up until the ring is full.
        let mut accepted = 0;
        while playback.load(graph()).is_ok() {
            accepted += 1;
            assert!(accepted < 100, "the graph ring is bounded");
        }
        let mut output = [0.0_f32; 4];
        playback.fill(&mut output);
        assert!(
            playback.load(graph()).is_ok(),
            "room again after a callback"
        );
    }

    #[test]
    fn a_swap_keeps_a_fader_moved_during_the_render() {
        let song = live_song(100);
        let lane = |gain: f32| LiveRender {
            fixed: RenderedAudio {
                samples: vec![0.0; 256],
                sample_rate: 48_000,
            },
            lanes: vec![vec![0.5; 256]],
            tracks: vec![song.tracks[0].id],
            gains: vec![(gain, gain)],
            master_gain: 1.0,
            limiter: false,
            ..LiveRender::default()
        };
        let mut playback = Playback::new_live(lane(1.0));
        playback.play();
        playback.apply_mix(&live_song(50), dry(0.0));
        let mut output = [0.0_f32; 64];
        playback.fill(&mut output);
        // A render that started before the move still bakes the old gain.
        playback.load(lane(1.0)).expect("load");
        playback.fill(&mut output);
        assert!(output.iter().all(|sample| (sample - 0.25).abs() < 1e-6));
    }

    fn note_song(pitch: u8) -> project::song::Song {
        let mut song = live_song(100);
        song.tracks[0].notes.push(project::song::Note {
            id: project::NoteId::new(1).expect("note"),
            start: 0,
            duration: music_core::PPQ * 4,
            pitch: music_core::Pitch::new(pitch).expect("pitch"),
            velocity: music_core::Velocity::new(100).expect("velocity"),
            channel: music_core::Channel::first(),
        });
        song
    }

    #[test]
    fn note_tracks_play_live_and_match_the_offline_render() {
        let song = note_song(60);
        let render = crate::live::render(&song, 8_000);
        assert!(
            render.synths[0].is_some(),
            "a notes-only track is a synth lane"
        );
        assert!(render.lanes[0].is_empty(), "and is not pre-rendered");
        let expected = crate::live::render(&song, 8_000).mixdown();
        let mut playback = Playback::new_live(render);
        playback.play();
        let mut output = vec![0.0_f32; expected.samples.len()];
        for chunk in output.chunks_mut(256) {
            playback.fill(chunk);
        }
        let worst = output
            .iter()
            .zip(&expected.samples)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(worst < 1e-5, "engine and offline differ by {worst}");
    }

    #[test]
    fn a_note_edit_swaps_in_while_playing() {
        let song = note_song(60);
        let render = crate::live::render(&song, 8_000);
        let delay = render.synth_delay;
        let mut playback = Playback::new_live(render);
        playback.play();
        let mut before = [0.0_f32; 512];
        playback.fill(&mut before);

        let edited = note_song(72);
        let lane = crate::synth::SynthLane::build(&edited, &edited.tracks[0], None, 8_000, delay);
        let expected = lane.render_all(2048);
        playback
            .swap_synth(edited.tracks[0].id, lane)
            .expect("swap");
        let mut after = [0.0_f32; 512];
        playback.fill(&mut after);
        // The new notes play from the current position on.
        for (got, want) in after.iter().zip(&expected[512..1024]) {
            assert!((got - want).abs() < 1e-5);
        }
        assert_eq!(playback.collect_garbage(), 1, "the old lane comes back");
    }

    fn wet_render(
        song: &project::song::Song,
        lane: Vec<f32>,
        chorus: f32,
        reverb: f32,
    ) -> LiveRender {
        let len = lane.len();
        LiveRender {
            fixed: RenderedAudio {
                samples: vec![0.0; len],
                sample_rate: 48_000,
            },
            lanes: vec![lane],
            tracks: vec![song.tracks[0].id],
            gains: vec![(0.9, 0.7)],
            master_gain: crate::master::gain(-2.0),
            limiter: true,
            chorus,
            reverb,
            ..LiveRender::default()
        }
    }

    fn impulse(len: usize) -> Vec<f32> {
        let mut lane = vec![0.0; len];
        lane[0] = 0.8;
        lane[1] = 0.8;
        lane
    }

    #[test]
    fn live_reverb_and_chorus_match_the_rendered_mix() {
        let song = live_song(100);
        let lane: Vec<f32> = (0..8192)
            .map(|index| (index as f32 * 0.02).sin() * 0.5)
            .collect();
        let render = wet_render(&song, lane, 0.3, 0.4);
        let expected = plain_mix(&render);
        let mut playback = Playback::new_live(render);
        playback.play();
        let mut output = vec![0.0_f32; 8192];
        for chunk in output.chunks_mut(94) {
            playback.fill(chunk);
        }
        assert_eq!(output, expected, "bit-exact with the offline mix");
    }

    #[test]
    fn a_reverb_change_is_heard_without_a_new_render() {
        let song = live_song(100);
        let mut playback = Playback::new_live(wet_render(&song, impulse(48_000), 0.0, 0.0));
        playback.play();
        let mut first = [0.0_f32; 64];
        playback.fill(&mut first);
        let mut dry_tail = [0.0_f32; 4096];
        playback.fill(&mut dry_tail);
        assert!(dry_tail.iter().all(|sample| sample.abs() < 1e-9));

        let mut wet = dry(-2.0);
        wet.limiter = true;
        wet.reverb = 0.8;
        playback.apply_mix(&song, wet);
        let mut wet_tail = [0.0_f32; 4096];
        playback.fill(&mut wet_tail);
        assert!(
            wet_tail.iter().any(|sample| sample.abs() > 1e-4),
            "the reverb already holds the impulse and rings out"
        );
    }

    #[test]
    fn a_new_graph_keeps_the_reverb_tail() {
        let song = live_song(100);
        let mut playback = Playback::new_live(wet_render(&song, impulse(48_000), 0.0, 0.8));
        playback.play();
        let mut output = [0.0_f32; 256];
        playback.fill(&mut output);
        playback
            .load(wet_render(&song, vec![0.0; 48_000], 0.0, 0.8))
            .expect("load");
        let mut tail = [0.0_f32; 8192];
        playback.fill(&mut tail);
        assert!(
            tail.iter().any(|sample| sample.abs() > 1e-4),
            "the tail rings on across the swap"
        );
        assert_eq!(playback.collect_garbage(), 1);
    }
}
