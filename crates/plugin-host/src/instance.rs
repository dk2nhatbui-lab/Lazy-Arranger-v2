//! Minimal in-process VST3 effect instance: component, audio processor and a
//! block-processing loop. No editor or parameter automation yet.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, BusDirections_, IAudioProcessor, IAudioProcessorTrait,
    IComponent, IComponentTrait, IEditController, IEditControllerTrait, IHostApplication,
    IHostApplicationTrait, MediaTypes_, ProcessContext, ProcessData, ProcessSetup, String128,
    SymbolicSampleSizes_,
};
use vst3::Steinberg::{
    IBStream, IBStreamTrait, IPluginBaseTrait, IPluginFactory, IPluginFactoryTrait, TUID,
    kNoInterface, kResultOk,
};
use vst3::{Class, ComPtr, ComWrapper, Interface};

/// One active VST3 effect.
pub struct Vst3Effect {
    component: ComPtr<IComponent>,
    processor: ComPtr<IAudioProcessor>,
    controller: Option<ComPtr<IEditController>>,
    _host: ComPtr<IHostApplication>,
    channels: usize,
    max_block: usize,
    sample_rate: f64,
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
        let processor = component
            .cast::<IAudioProcessor>()
            .ok_or_else(|| String::from("component has no IAudioProcessor"))?;
        let mut controller_id: TUID = [0; 16];
        let controller =
            if unsafe { component.getControllerClassId(&mut controller_id) } == kResultOk {
                match create::<IEditController>(&factory, &controller_id) {
                    Ok(controller) => {
                        let result = unsafe { controller.initialize(host.as_ptr().cast()) };
                        (result == kResultOk).then_some(controller)
                    }
                    Err(_) => None,
                }
            } else {
                None
            };
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
            maxSamplesPerBlock: max_block.min(i32::MAX as usize) as i32,
            sampleRate: sample_rate,
        };
        let result = unsafe { processor.setupProcessing(&mut setup) };
        if result != kResultOk {
            return Err(format!("setupProcessing returned {result}"));
        }
        let active = unsafe { component.setActive(1) };
        if active != kResultOk {
            return Err(format!("setActive returned {active}"));
        }
        let processing = unsafe { processor.setProcessing(1) };
        if processing != kResultOk {
            return Err(format!("setProcessing returned {processing}"));
        }
        Ok(Self {
            component,
            processor,
            controller,
            _host: host,
            channels: 2,
            max_block: max_block.max(1),
            sample_rate,
        })
    }

    /// Number of channels the processor handles.
    #[must_use]
    pub fn channels(&self) -> usize {
        self.channels
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
                inputParameterChanges: std::ptr::null_mut(),
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
        self.load_state(&bytes)
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
            let title: String = info
                .title
                .iter()
                .take_while(|value| **value != 0)
                .map(|value| *value as u8 as char)
                .collect();
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
            if let Some(controller) = &self.controller {
                let _ = controller.terminate();
            }
            let _ = self.component.terminate();
        }
    }
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
        let available = state.bytes.len().saturating_sub(state.position);
        let count = usize::try_from(num_bytes.max(0))
            .unwrap_or(0)
            .min(available);
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
        let count = usize::try_from(num_bytes.max(0)).unwrap_or(0);
        let bytes = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), count) };
        let end = state.position.saturating_add(count);
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
        state.position = usize::try_from(target).unwrap_or(usize::MAX);
        if !result.is_null() {
            unsafe { *result = target };
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

pub(crate) struct HostApplication;

impl Class for HostApplication {
    type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> vst3::Steinberg::tresult {
        let text: Vec<u16> = "Lazy Arranger".encode_utf16().collect();
        let target = unsafe { &mut *name };
        for (slot, value) in target.iter_mut().zip(text.iter().copied()) {
            *slot = value;
        }
        if let Some(last) = target.get_mut(text.len()) {
            *last = 0;
        }
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
    if text.len() != 32 {
        return None;
    }
    let mut cid: TUID = [0; 16];
    for (index, slot) in cid.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok()? as i8;
    }
    Some(cid)
}

pub(crate) fn vst3_binary(path: &Path) -> Option<PathBuf> {
    let macos = path.join("Contents").join("MacOS");
    let stem = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let direct = macos.join(&stem);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(&macos)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|entry| entry.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

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
