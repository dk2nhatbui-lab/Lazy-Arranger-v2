#![allow(clippy::unnecessary_cast)]
//! Minimal in-process VST3 effect instance: component, audio processor and a
//! block-processing loop. No editor or parameter automation yet.

use std::ffi::c_void;
use std::path::Path;

use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, BusDirections_, IAudioProcessor, IAudioProcessorTrait,
    IComponent, IComponentHandler, IComponentHandlerTrait, IComponentTrait, IConnectionPoint,
    IConnectionPointTrait, IEditController, IEditControllerTrait, IHostApplication,
    IHostApplicationTrait, IParamValueQueue, IParamValueQueueTrait, IParameterChanges,
    IParameterChangesTrait, MediaTypes_, ParamID, ParamValue, ProcessContext, ProcessData,
    ProcessSetup, String128, SymbolicSampleSizes_,
};
use vst3::Steinberg::{
    IBStream, IBStreamTrait, IPluginBaseTrait, IPluginFactory, IPluginFactoryTrait, TUID,
    kNoInterface, kResultOk,
};
use vst3::{Class, ComPtr, ComWrapper, Interface};

/// Metadata reported by a VST3 plug-in's `IPlugView` before attaching to a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vst3ViewInfo {
    /// True when the view reports support for `NSView` on macOS.
    pub supports_nsview: bool,
    /// Preferred editor width in points/pixels.
    pub width: i32,
    /// Preferred editor height in points/pixels.
    pub height: i32,
    /// True when the view permits host-driven resizing.
    pub can_resize: bool,
}

/// One active VST3 effect.
pub struct Vst3Effect {
    // Fields drop in order: the controller goes before the component it talks
    // to, and the plug-in library outlives every COM pointer.
    controller: Option<ComPtr<IEditController>>,
    /// Both ends of the component/controller link, disconnected on drop.
    connection: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
    /// True when the controller is its own object that needs `terminate()`.
    separate_controller: bool,
    processor: ComPtr<IAudioProcessor>,
    component: ComPtr<IComponent>,
    _handler: Option<ComPtr<IComponentHandler>>,
    _host: ComPtr<IHostApplication>,
    channels: usize,
    max_block: usize,
    sample_rate: f64,
    pending_parameters: Vec<(u32, f64)>,
    _library: libloading::Library,
}

/// The largest block a VST3 host may promise: positive, fits `i32`, and
/// doubles (two channels) without overflow.
pub(crate) fn clamp_max_block(max_block: usize) -> usize {
    max_block.clamp(1, (i32::MAX as usize).min(usize::MAX / 2))
}

impl std::fmt::Debug for Vst3Effect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Vst3Effect")
            .field("channels", &self.channels)
            .field("max_block", &self.max_block)
            .field("sample_rate", &self.sample_rate)
            .field("has_controller", &self.controller.is_some())
            .finish()
    }
}

impl Vst3Effect {
    /// Loads a class from a VST3 bundle and activates it for stereo processing.
    ///
    /// # Errors
    ///
    /// Returns a message when the class cannot be created, initialized or
    /// activated.
    pub fn new(
        bundle: &Path,
        class_id: &str,
        sample_rate: f64,
        max_block: usize,
    ) -> Result<Self, String> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Self::new_inner(bundle, class_id, sample_rate, max_block)
        }))
        .unwrap_or_else(|_| Err(String::from("VST3 instance panicked")))
    }

    fn new_inner(
        bundle: &Path,
        class_id: &str,
        sample_rate: f64,
        max_block: usize,
    ) -> Result<Self, String> {
        let cid = parse_cid(class_id).ok_or_else(|| String::from("invalid VST3 class id"))?;
        let binary = vst3_binary(bundle).ok_or_else(|| String::from("no VST3 binary"))?;
        let library = unsafe { libloading::Library::new(&binary) }
            .map_err(|error| format!("load {}: {error}", binary.display()))?;
        type GetFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
        let get_factory: libloading::Symbol<'_, GetFactory> =
            unsafe { library.get(b"GetPluginFactory\0") }
                .map_err(|error| format!("GetPluginFactory: {error}"))?;
        let factory = unsafe { ComPtr::from_raw(get_factory()) }
            .ok_or_else(|| String::from("GetPluginFactory returned null"))?;
        let host = ComWrapper::new(HostApplication)
            .to_com_ptr::<IHostApplication>()
            .ok_or_else(|| String::from("host application"))?;
        let component = create::<IComponent>(&factory, &cid)?;
        let init = unsafe { component.initialize(host.as_ptr().cast()) };
        if init != kResultOk {
            return Err(format!("component initialize returned {init}"));
        }
        let Some(processor) = component.cast::<IAudioProcessor>() else {
            let _ = unsafe { component.terminate() };
            return Err(String::from("component has no IAudioProcessor"));
        };
        let max_block = clamp_max_block(max_block);
        // From here on every early return drops `effect`, whose Drop
        // deactivates, disconnects and terminates what was set up.
        let mut effect = Self {
            controller: None,
            connection: None,
            separate_controller: false,
            processor,
            component,
            _handler: ComWrapper::new(HostComponentHandler).to_com_ptr::<IComponentHandler>(),
            _host: host,
            channels: 2,
            max_block,
            sample_rate,
            pending_parameters: Vec::new(),
            _library: library,
        };
        let component = effect.component.clone();
        let mut controller_id: TUID = [0; 16];
        effect.controller =
            if unsafe { component.getControllerClassId(&mut controller_id) } == kResultOk {
                match create::<IEditController>(&factory, &controller_id) {
                    Ok(controller) => {
                        let result = unsafe { controller.initialize(effect._host.as_ptr().cast()) };
                        if result == kResultOk {
                            effect.separate_controller = true;
                            Some(controller)
                        } else {
                            None
                        }
                    }
                    Err(_) => component.cast::<IEditController>(),
                }
            } else {
                component.cast::<IEditController>()
            };
        if let Some(controller) = &effect.controller {
            if let Some(handler) = &effect._handler {
                let _ = unsafe { controller.setComponentHandler(handler.as_ptr()) };
            }
            if effect.separate_controller
                && let (Some(comp_cp), Some(ctrl_cp)) = (
                    component.cast::<IConnectionPoint>(),
                    controller.cast::<IConnectionPoint>(),
                )
            {
                let _ = unsafe { comp_cp.connect(ctrl_cp.as_ptr()) };
                let _ = unsafe { ctrl_cp.connect(comp_cp.as_ptr()) };
                effect.connection = Some((comp_cp, ctrl_cp));
            }
        }
        let inputs = unsafe {
            component.getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kInput as i32)
        }
        .max(0);
        if inputs == 0 {
            return Err(String::from("plug-in has no audio input bus"));
        }
        let outputs = unsafe {
            component.getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kOutput as i32)
        }
        .max(0);
        if outputs == 0 {
            return Err(String::from("plug-in has no audio output bus"));
        }
        negotiate_stereo(&component, &effect.processor, true)?;
        unsafe {
            let _ = component.activateBus(
                MediaTypes_::kAudio as i32,
                BusDirections_::kInput as i32,
                0,
                1,
            );
            let _ = component.activateBus(
                MediaTypes_::kAudio as i32,
                BusDirections_::kOutput as i32,
                0,
                1,
            );
        }
        let mut setup = ProcessSetup {
            processMode: 0,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            maxSamplesPerBlock: max_block as i32,
            sampleRate: sample_rate,
        };
        let result = unsafe { effect.processor.setupProcessing(&mut setup) };
        if result != kResultOk {
            return Err(format!("setupProcessing returned {result}"));
        }
        let active = unsafe { component.setActive(1) };
        if active != kResultOk {
            return Err(format!("setActive returned {active}"));
        }
        let processing = unsafe { effect.processor.setProcessing(1) };
        if processing != kResultOk {
            return Err(format!("setProcessing returned {processing}"));
        }
        Ok(effect)
    }

    /// Number of channels the processor handles.
    #[must_use]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Queues a normalized parameter change for the next process block.
    pub fn set_parameter(&mut self, id: u32, value: f64) {
        self.pending_parameters.push((id, value.clamp(0.0, 1.0)));
        if let Some(controller) = &self.controller {
            let _ = unsafe { controller.setParamNormalized(id, value.clamp(0.0, 1.0)) };
        }
    }

    /// Processes interleaved stereo samples in place.
    ///
    /// # Errors
    ///
    /// Returns a message when the processor rejects a block or the buffer is not
    /// a whole number of stereo frames.
    pub fn process_stereo(&mut self, samples: &mut [f32]) -> Result<(), String> {
        if self.channels != 2 {
            return Err(String::from("only stereo processing is implemented"));
        }
        if !samples.len().is_multiple_of(2) {
            return Err(String::from("stereo buffer has an odd length"));
        }
        if samples.is_empty() {
            return Ok(());
        }
        let mut changes = if self.pending_parameters.is_empty() {
            None
        } else {
            let queues = self
                .pending_parameters
                .drain(..)
                .map(|(id, value)| {
                    ComWrapper::new(ParameterQueue {
                        id,
                        points: std::sync::Mutex::new(vec![ParameterPoint { offset: 0, value }]),
                    })
                    .to_com_ptr::<IParamValueQueue>()
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| String::from("parameter queue"))?;
            ComWrapper::new(ParameterChanges {
                queues: std::sync::Mutex::new(queues),
            })
            .to_com_ptr::<IParameterChanges>()
        };
        for block in samples.chunks_mut(self.max_block * 2) {
            let frames = block.len() / 2;
            let mut left = vec![0.0_f32; frames];
            let mut right = vec![0.0_f32; frames];
            for (frame, sample) in block.as_chunks::<2>().0.iter().enumerate() {
                left[frame] = sample[0];
                right[frame] = sample[1];
            }
            let mut input_channels = [left.as_mut_ptr(), right.as_mut_ptr()];
            let mut output_left = vec![0.0_f32; frames];
            let mut output_right = vec![0.0_f32; frames];
            let mut output_channels = [output_left.as_mut_ptr(), output_right.as_mut_ptr()];
            let mut inputs = [AudioBusBuffers {
                numChannels: 2,
                silenceFlags: 0,
                __field0: AudioBusBuffers__type0 {
                    channelBuffers32: input_channels.as_mut_ptr(),
                },
            }];
            let mut outputs = [AudioBusBuffers {
                numChannels: 2,
                silenceFlags: 0,
                __field0: AudioBusBuffers__type0 {
                    channelBuffers32: output_channels.as_mut_ptr(),
                },
            }];
            let mut context: ProcessContext = unsafe { std::mem::zeroed() };
            context.state = 2;
            context.sampleRate = self.sample_rate;
            context.tempo = 120.0;
            context.timeSigNumerator = 4;
            context.timeSigDenominator = 4;
            let mut data = ProcessData {
                processMode: 0,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
                numSamples: frames as i32,
                numInputs: 1,
                numOutputs: 1,
                inputs: inputs.as_mut_ptr(),
                outputs: outputs.as_mut_ptr(),
                inputParameterChanges: changes
                    .as_ref()
                    .map_or(std::ptr::null_mut(), |changes| changes.as_ptr()),
                outputParameterChanges: std::ptr::null_mut(),
                inputEvents: std::ptr::null_mut(),
                outputEvents: std::ptr::null_mut(),
                processContext: &mut context,
            };
            let result = unsafe { self.processor.process(&mut data) };
            if result != kResultOk {
                return Err(format!("process returned {result}"));
            }
            for (frame, sample) in block.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                sample[0] = output_left[frame];
                sample[1] = output_right[frame];
            }
            // Changes land at offset 0 of the first block only.
            changes = None;
        }
        Ok(())
    }

    /// Number of parameters exposed by the edit controller.
    #[must_use]
    pub fn parameter_count(&self) -> usize {
        self.controller.as_ref().map_or(0, |controller| {
            unsafe { controller.getParameterCount() }.max(0) as usize
        })
    }

    /// Serializes the component state into a memory stream.
    ///
    /// # Errors
    ///
    /// Returns a message when the component rejects the stream or has no state.
    pub fn save_state(&self) -> Result<Vec<u8>, String> {
        let (stream, data) = MemoryStream::new();
        let pointer = stream
            .to_com_ptr::<IBStream>()
            .ok_or_else(|| String::from("memory stream"))?;
        let result = unsafe { self.component.getState(pointer.as_ptr()) };
        if result != kResultOk {
            return Err(format!("getState returned {result}"));
        }
        let bytes = data
            .lock()
            .map_err(|error| error.to_string())?
            .bytes
            .clone();
        if bytes.is_empty() {
            Err(String::from("plug-in wrote no state"))
        } else {
            Ok(bytes)
        }
    }

    /// Restores component state from a memory stream.
    ///
    /// # Errors
    ///
    /// Returns a message when the component rejects the stream.
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), String> {
        let (stream, _) = MemoryStream::from_bytes(bytes);
        let pointer = stream
            .to_com_ptr::<IBStream>()
            .ok_or_else(|| String::from("memory stream"))?;
        let result = unsafe { self.component.setState(pointer.as_ptr()) };
        if result != kResultOk {
            return Err(format!("setState returned {result}"));
        }
        Ok(())
    }

    /// Latency reported by the audio processor, in samples at the active rate.
    #[must_use]
    pub fn latency_samples(&self) -> usize {
        let latency = unsafe { self.processor.getLatencySamples() };
        usize::try_from(latency).unwrap_or(usize::MAX)
    }

    /// Saves component state as base64 for a project slot.
    ///
    /// # Errors
    ///
    /// Returns a message when the plug-in has no state.
    pub fn save_state_base64(&self) -> Result<String, String> {
        use base64::Engine as _;
        Ok(base64::engine::general_purpose::STANDARD.encode(self.save_state()?))
    }

    /// Restores base64 component state from a project slot.
    ///
    /// # Errors
    ///
    /// Returns a message when the text is not valid base64 or the plug-in rejects it.
    pub fn load_state_base64(&mut self, state: &str) -> Result<(), String> {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(state.trim())
            .map_err(|error| format!("base64 state: {error}"))?;
        self.load_state(&bytes)?;
        self.sync_controller_state();
        Ok(())
    }

    /// Synchronizes the edit controller with the component's current state.
    pub fn sync_controller_state(&self) {
        let Some(controller) = &self.controller else {
            return;
        };
        if let Ok(bytes) = self.save_state() {
            let (stream, _) = MemoryStream::from_bytes(&bytes);
            if let Some(pointer) = stream.to_com_ptr::<IBStream>() {
                let _ = unsafe { controller.setComponentState(pointer.as_ptr()) };
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn controller(&self) -> Option<&ComPtr<IEditController>> {
        self.controller.as_ref()
    }

    /// Probes whether this VST3 instance exposes a native `IPlugView` editor
    /// without creating an OS window.
    ///
    /// # Errors
    ///
    /// Returns a message when the plug-in has no edit controller or no editor view.
    pub fn probe_view(&self) -> Result<Vst3ViewInfo, String> {
        use vst3::Steinberg::Vst::ViewType;
        use vst3::Steinberg::{IPlugView, IPlugViewTrait, ViewRect, kPlatformTypeNSView};

        let controller = self
            .controller
            .as_ref()
            .ok_or_else(|| String::from("plug-in has no IEditController"))?;
        let raw_view = unsafe { controller.createView(ViewType::kEditor) };
        let view = unsafe { ComPtr::<IPlugView>::from_raw(raw_view) }
            .ok_or_else(|| String::from("plug-in returned no editor IPlugView"))?;
        let supports_nsview =
            unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) } == kResultOk;
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let (width, height) = if unsafe { view.getSize(&mut rect) } == kResultOk {
            (
                (rect.right - rect.left).max(0),
                (rect.bottom - rect.top).max(0),
            )
        } else {
            (0, 0)
        };
        let can_resize = unsafe { view.canResize() } == vst3::Steinberg::kResultTrue;
        drop(view);
        Ok(Vst3ViewInfo {
            supports_nsview,
            width,
            height,
            can_resize,
        })
    }

    /// Parameters exposed by the edit controller: id, title, default and range.
    #[must_use]
    pub fn parameters(&self) -> Vec<(u32, String, f64)> {
        let Some(controller) = &self.controller else {
            return Vec::new();
        };
        let count = unsafe { controller.getParameterCount() }.max(0);
        let mut parameters = Vec::new();
        for index in 0..count {
            let mut info: vst3::Steinberg::Vst::ParameterInfo = unsafe { std::mem::zeroed() };
            let result = unsafe { controller.getParameterInfo(index, &mut info) };
            if result != kResultOk {
                continue;
            }
            let units: Vec<u16> = info
                .title
                .iter()
                .take_while(|value| **value != 0)
                .map(|value| *value as u16)
                .collect();
            let title = String::from_utf16_lossy(&units);
            parameters.push((info.id, title, info.defaultNormalizedValue));
        }
        parameters
    }
}

impl Drop for Vst3Effect {
    fn drop(&mut self) {
        unsafe {
            let _ = self.processor.setProcessing(0);
            let _ = self.component.setActive(0);
            if let Some((component, controller)) = self.connection.take() {
                let _ = component.disconnect(controller.as_ptr());
                let _ = controller.disconnect(component.as_ptr());
            }
            if let Some(controller) = &self.controller {
                let _ = controller.setComponentHandler(std::ptr::null_mut());
                // A single-component plug-in is its own controller: it is
                // terminated once, below, as the component.
                if self.separate_controller {
                    let _ = controller.terminate();
                }
            }
            let _ = self.component.terminate();
        }
    }
}

/// Asks the plug-in for one stereo input (effects only) and one stereo
/// output bus, the layout the process code assumes.
///
/// # Errors
///
/// Returns a message when a main bus keeps more than two channels, which the
/// two-pointer buffers cannot serve.
pub(crate) fn negotiate_stereo(
    component: &ComPtr<IComponent>,
    processor: &ComPtr<IAudioProcessor>,
    with_input: bool,
) -> Result<(), String> {
    use vst3::Steinberg::Vst::{BusInfo, SpeakerArrangement};
    const STEREO: SpeakerArrangement = 0b11; // kSpeakerL | kSpeakerR
    let count = |direction: i32| {
        unsafe { component.getBusCount(MediaTypes_::kAudio as i32, direction) }.max(0) as usize
    };
    let mut inputs: Vec<SpeakerArrangement> = vec![0; count(BusDirections_::kInput as i32)];
    let mut outputs: Vec<SpeakerArrangement> = vec![0; count(BusDirections_::kOutput as i32)];
    if with_input && let Some(first) = inputs.first_mut() {
        *first = STEREO;
    }
    if let Some(first) = outputs.first_mut() {
        *first = STEREO;
    }
    let _ = unsafe {
        processor.setBusArrangements(
            inputs.as_mut_ptr(),
            inputs.len() as i32,
            outputs.as_mut_ptr(),
            outputs.len() as i32,
        )
    };
    let main_channels = |direction: i32| {
        let mut info: BusInfo = unsafe { std::mem::zeroed() };
        let result =
            unsafe { component.getBusInfo(MediaTypes_::kAudio as i32, direction, 0, &mut info) };
        (result == kResultOk).then_some(info.channelCount)
    };
    if with_input && main_channels(BusDirections_::kInput as i32).is_some_and(|c| c > 2) {
        return Err(String::from("plug-in input has more than two channels"));
    }
    if main_channels(BusDirections_::kOutput as i32).is_some_and(|c| c > 2) {
        return Err(String::from("plug-in output has more than two channels"));
    }
    Ok(())
}

pub(crate) struct StreamState {
    pub(crate) bytes: Vec<u8>,
    position: usize,
}

pub(crate) struct MemoryStream {
    state: std::sync::Arc<std::sync::Mutex<StreamState>>,
}

impl MemoryStream {
    fn new() -> (
        ComWrapper<Self>,
        std::sync::Arc<std::sync::Mutex<StreamState>>,
    ) {
        let state = std::sync::Arc::new(std::sync::Mutex::new(StreamState {
            bytes: Vec::new(),
            position: 0,
        }));
        (
            ComWrapper::new(Self {
                state: std::sync::Arc::clone(&state),
            }),
            state,
        )
    }

    fn from_bytes(
        bytes: &[u8],
    ) -> (
        ComWrapper<Self>,
        std::sync::Arc<std::sync::Mutex<StreamState>>,
    ) {
        let state = std::sync::Arc::new(std::sync::Mutex::new(StreamState {
            bytes: bytes.to_vec(),
            position: 0,
        }));
        (
            ComWrapper::new(Self {
                state: std::sync::Arc::clone(&state),
            }),
            state,
        )
    }
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(
        &self,
        buffer: *mut std::ffi::c_void,
        num_bytes: i32,
        bytes_read: *mut i32,
    ) -> vst3::Steinberg::tresult {
        let Ok(mut state) = self.state.lock() else {
            return vst3::Steinberg::kResultFalse;
        };
        if num_bytes < 0 || (buffer.is_null() && num_bytes > 0) {
            return vst3::Steinberg::kInvalidArgument;
        }
        let available = state.bytes.len().saturating_sub(state.position);
        let count = usize::try_from(num_bytes).unwrap_or(0).min(available);
        if count > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    state.bytes[state.position..].as_ptr(),
                    buffer.cast::<u8>(),
                    count,
                );
            }
            state.position += count;
        }
        if !bytes_read.is_null() {
            unsafe { *bytes_read = i32::try_from(count).unwrap_or(0) };
        }
        kResultOk
    }

    unsafe fn write(
        &self,
        buffer: *mut std::ffi::c_void,
        num_bytes: i32,
        bytes_written: *mut i32,
    ) -> vst3::Steinberg::tresult {
        let Ok(mut state) = self.state.lock() else {
            return vst3::Steinberg::kResultFalse;
        };
        if num_bytes < 0 || (buffer.is_null() && num_bytes > 0) {
            return vst3::Steinberg::kInvalidArgument;
        }
        let count = usize::try_from(num_bytes).unwrap_or(0);
        let Some(end) = state
            .position
            .checked_add(count)
            .filter(|end| *end <= MAX_STREAM_BYTES)
        else {
            return vst3::Steinberg::kResultFalse;
        };
        let bytes = if count == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), count) }
        };
        if end > state.bytes.len() {
            state.bytes.resize(end, 0);
        }
        let position = state.position;
        state.bytes[position..end].copy_from_slice(bytes);
        state.position = end;
        if !bytes_written.is_null() {
            unsafe { *bytes_written = i32::try_from(count).unwrap_or(0) };
        }
        kResultOk
    }

    unsafe fn seek(&self, position: i64, mode: i32, result: *mut i64) -> vst3::Steinberg::tresult {
        let Ok(mut state) = self.state.lock() else {
            return vst3::Steinberg::kResultFalse;
        };
        let base = match mode {
            0 => 0,
            1 => i64::try_from(state.position).unwrap_or(0),
            2 => i64::try_from(state.bytes.len()).unwrap_or(0),
            _ => return vst3::Steinberg::kResultFalse,
        };
        let target = base.saturating_add(position).max(0);
        let Ok(target) = usize::try_from(target) else {
            return vst3::Steinberg::kResultFalse;
        };
        state.position = target;
        if !result.is_null() {
            unsafe { *result = i64::try_from(target).unwrap_or(i64::MAX) };
        }
        kResultOk
    }

    unsafe fn tell(&self, position: *mut i64) -> vst3::Steinberg::tresult {
        let Ok(state) = self.state.lock() else {
            return vst3::Steinberg::kResultFalse;
        };
        if !position.is_null() {
            unsafe { *position = i64::try_from(state.position).unwrap_or(i64::MAX) };
        }
        kResultOk
    }
}

/// The largest plug-in state a memory stream holds (64 MiB).
const MAX_STREAM_BYTES: usize = 64 * 1024 * 1024;

pub(crate) fn memory_stream_from_bytes(
    bytes: &[u8],
) -> (
    ComWrapper<MemoryStream>,
    std::sync::Arc<std::sync::Mutex<StreamState>>,
) {
    MemoryStream::from_bytes(bytes)
}

pub(crate) fn memory_stream() -> (
    ComWrapper<MemoryStream>,
    std::sync::Arc<std::sync::Mutex<StreamState>>,
) {
    MemoryStream::new()
}

struct ParameterPoint {
    offset: i32,
    value: f64,
}

struct ParameterQueue {
    id: u32,
    points: std::sync::Mutex<Vec<ParameterPoint>>,
}

impl Class for ParameterQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ParameterQueue {
    unsafe fn getParameterId(&self) -> u32 {
        self.id
    }

    unsafe fn getPointCount(&self) -> i32 {
        self.points.lock().map_or(0, |points| points.len() as i32)
    }

    unsafe fn getPoint(
        &self,
        index: i32,
        sample_offset: *mut i32,
        value: *mut f64,
    ) -> vst3::Steinberg::tresult {
        let Ok(points) = self.points.lock() else {
            return vst3::Steinberg::kResultFalse;
        };
        let Some(point) = usize::try_from(index)
            .ok()
            .and_then(|index| points.get(index))
        else {
            return vst3::Steinberg::kResultFalse;
        };
        if !sample_offset.is_null() {
            unsafe { *sample_offset = point.offset };
        }
        if !value.is_null() {
            unsafe { *value = point.value };
        }
        kResultOk
    }

    unsafe fn addPoint(
        &self,
        sample_offset: i32,
        value: f64,
        index: *mut i32,
    ) -> vst3::Steinberg::tresult {
        let Ok(mut points) = self.points.lock() else {
            return vst3::Steinberg::kResultFalse;
        };
        let position = points.len() as i32;
        points.push(ParameterPoint {
            offset: sample_offset,
            value,
        });
        if !index.is_null() {
            unsafe { *index = position };
        }
        kResultOk
    }
}

struct ParameterChanges {
    queues: std::sync::Mutex<Vec<ComPtr<IParamValueQueue>>>,
}

impl Class for ParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for ParameterChanges {
    unsafe fn getParameterCount(&self) -> i32 {
        self.queues.lock().map_or(0, |queues| queues.len() as i32)
    }

    unsafe fn getParameterData(&self, index: i32) -> *mut IParamValueQueue {
        let Ok(queues) = self.queues.lock() else {
            return std::ptr::null_mut();
        };
        let Some(queue) = usize::try_from(index)
            .ok()
            .and_then(|index| queues.get(index))
        else {
            return std::ptr::null_mut();
        };
        // The VST3 ABI lends this queue to the plug-in: it must not release
        // it, so no reference is added here.
        queue.as_ptr()
    }

    unsafe fn addParameterData(&self, _id: *const u32, _index: *mut i32) -> *mut IParamValueQueue {
        std::ptr::null_mut()
    }
}

pub(crate) struct HostComponentHandler;

impl Class for HostComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for HostComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> vst3::Steinberg::tresult {
        kResultOk
    }

    unsafe fn performEdit(&self, _id: ParamID, _value: ParamValue) -> vst3::Steinberg::tresult {
        kResultOk
    }

    unsafe fn endEdit(&self, _id: ParamID) -> vst3::Steinberg::tresult {
        kResultOk
    }

    unsafe fn restartComponent(&self, _flags: i32) -> vst3::Steinberg::tresult {
        kResultOk
    }
}

pub(crate) struct HostApplication;

impl Class for HostApplication {
    type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> vst3::Steinberg::tresult {
        if name.is_null() {
            return vst3::Steinberg::kInvalidArgument;
        }
        // Write through a raw pointer: the buffer may be uninitialised, so no
        // `&mut` reference to it is made.
        let target = name.cast::<u16>();
        let capacity = std::mem::size_of::<String128>() / std::mem::size_of::<u16>();
        let mut written = 0;
        for value in "Lazy Arranger".encode_utf16().take(capacity - 1) {
            unsafe { target.add(written).write(value) };
            written += 1;
        }
        unsafe { target.add(written).write(0) };
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        _cid: *mut TUID,
        _iid: *mut TUID,
        _obj: *mut *mut std::ffi::c_void,
    ) -> vst3::Steinberg::tresult {
        kNoInterface
    }
}

pub(crate) fn create<T: Interface>(
    factory: &ComPtr<IPluginFactory>,
    cid: &TUID,
) -> Result<ComPtr<T>, String> {
    let mut raw: *mut c_void = std::ptr::null_mut();
    let result =
        unsafe { factory.createInstance(cid.as_ptr().cast(), T::IID.as_ptr().cast(), &mut raw) };
    if result != kResultOk || raw.is_null() {
        return Err(format!("createInstance returned {result}"));
    }
    unsafe { ComPtr::from_raw(raw.cast()) }.ok_or_else(|| String::from("created instance was null"))
}

pub(crate) fn parse_cid(text: &str) -> Option<TUID> {
    if text.len() != 32 || !text.is_ascii() {
        return None;
    }
    let mut cid: TUID = [0; 16];
    for (index, slot) in cid.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()? as i8;
    }
    Some(cid)
}

pub(crate) use crate::vst3_binary;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_changes_expose_a_queue() {
        let queue = ComWrapper::new(ParameterQueue {
            id: 7,
            points: std::sync::Mutex::new(vec![ParameterPoint {
                offset: 0,
                value: 0.5,
            }]),
        })
        .to_com_ptr::<IParamValueQueue>()
        .expect("queue");
        let changes = ComWrapper::new(ParameterChanges {
            queues: std::sync::Mutex::new(vec![queue]),
        })
        .to_com_ptr::<IParameterChanges>()
        .expect("changes");
        assert_eq!(unsafe { changes.getParameterCount() }, 1);
        let raw = unsafe { changes.getParameterData(0) };
        assert!(!raw.is_null());
        let queue = unsafe { vst3::ComRef::from_raw(raw) }.expect("queue pointer");
        assert_eq!(unsafe { queue.getParameterId() }, 7);
        assert_eq!(unsafe { queue.getPointCount() }, 1);
    }

    #[test]
    fn class_ids_parse_only_32_hex_digits() {
        assert!(parse_cid("00000000000000000000000000000000").is_some());
        assert!(parse_cid("00").is_none());
        assert!(parse_cid("GGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG").is_none());
    }

    #[test]
    #[ignore = "loads an installed plug-in"]
    fn r0verb_state_round_trips() {
        let classes =
            crate::scan_vst3(Path::new("/Library/Audio/Plug-Ins/VST3/R0Verb.vst3")).expect("scan");
        let mut effect = Vst3Effect::new(
            Path::new("/Library/Audio/Plug-Ins/VST3/R0Verb.vst3"),
            &classes[0].cid,
            48_000.0,
            256,
        )
        .expect("effect");
        let state = effect.save_state_base64().expect("save state");
        effect.load_state_base64(&state).expect("load state");
    }

    #[test]
    #[ignore = "loads an installed plug-in"]
    fn r0verb_processes_audio() {
        let classes =
            crate::scan_vst3(Path::new("/Library/Audio/Plug-Ins/VST3/R0Verb.vst3")).expect("scan");
        let mut effect = Vst3Effect::new(
            Path::new("/Library/Audio/Plug-Ins/VST3/R0Verb.vst3"),
            &classes[0].cid,
            48_000.0,
            256,
        )
        .expect("effect");
        let mut samples = vec![0.0_f32; 512 * 2];
        for frame in 0..512 {
            samples[frame * 2] = (frame as f32 * 0.02).sin();
            samples[frame * 2 + 1] = samples[frame * 2];
        }
        effect.process_stereo(&mut samples).expect("process");
        assert!(samples.iter().any(|sample| *sample != 0.0));
    }
}
