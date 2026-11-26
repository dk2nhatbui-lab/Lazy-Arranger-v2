#![allow(warnings)]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::{CString, c_char, c_void};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{ptr, slice};

use vst3::{Class, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*, uid};

fn copy_cstring(src: &str, dst: &mut [c_char]) {
    let c_string = CString::new(src).unwrap_or_else(|_| CString::default());
    let bytes = c_string.as_bytes_with_nul();

    for (src, dst) in bytes.iter().zip(dst.iter_mut()) {
        *dst = *src as c_char;
    }

    if bytes.len() > dst.len() {
        if let Some(last) = dst.last_mut() {
            *last = 0;
        }
    }
}

fn copy_wstring(src: &str, dst: &mut [TChar]) {
    let mut len = 0;
    for (src, dst) in src.encode_utf16().zip(dst.iter_mut()) {
        *dst = src as TChar;
        len += 1;
    }

    if len < dst.len() {
        dst[len] = 0;
    } else if let Some(last) = dst.last_mut() {
        *last = 0;
    }
}

unsafe fn len_wstring(string: *const TChar) -> usize {
    let mut len = 0;

    while *string.offset(len) != 0 {
        len += 1;
    }

    len as usize
}

// Adapted from the vst3-rs gain example (MIT OR Apache-2.0).
const PLUGIN_NAME: &'static str = "Lazy Arranger Test Gain";

struct DelayLine {
    left: [f32; 32],
    right: [f32; 32],
    index: usize,
}

impl DelayLine {
    fn new() -> Self {
        Self {
            left: [0.0; 32],
            right: [0.0; 32],
            index: 0,
        }
    }
}

struct GainProcessor {
    gain: AtomicU64,
    delay: std::sync::Mutex<DelayLine>,
}

impl Class for GainProcessor {
    type Interfaces = (IComponent, IAudioProcessor, IProcessContextRequirements);
}

impl GainProcessor {
    const CID: TUID = uid(0x4C41_5A59, 0x5445_5354, 0x4741_494E, 0x0000_0001);

    fn new() -> GainProcessor {
        GainProcessor {
            gain: AtomicU64::new(1.0f64.to_bits()),
            delay: std::sync::Mutex::new(DelayLine::new()),
        }
    }
}

impl IPluginBaseTrait for GainProcessor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for GainProcessor {
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        if class_id.is_null() {
            return kInvalidArgument;
        }
        *class_id = GainController::CID;
        kResultOk
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, mediaType: MediaType, dir: BusDirection) -> i32 {
        match mediaType as BusDirections {
            MediaTypes_::kAudio => match dir as BusDirections {
                BusDirections_::kInput => 1,
                BusDirections_::kOutput => 1,
                _ => 0,
            },
            MediaTypes_::kEvent => 0,
            _ => 0,
        }
    }

    unsafe fn getBusInfo(
        &self,
        mediaType: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        if bus.is_null() {
            return kInvalidArgument;
        }
        match mediaType as MediaTypes {
            MediaTypes_::kAudio => match dir as BusDirections {
                BusDirections_::kInput => match index {
                    0 => {
                        let bus = &mut *bus;

                        bus.mediaType = MediaTypes_::kAudio as MediaType;
                        bus.direction = BusDirections_::kInput as BusDirection;
                        bus.channelCount = 2;
                        copy_wstring("Input", &mut bus.name);
                        bus.busType = BusTypes_::kMain as BusType;
                        bus.flags = BusInfo_::BusFlags_::kDefaultActive as u32;

                        kResultOk
                    }
                    _ => kInvalidArgument,
                },
                BusDirections_::kOutput => match index {
                    0 => {
                        let bus = &mut *bus;

                        bus.mediaType = MediaTypes_::kAudio as MediaType;
                        bus.direction = BusDirections_::kOutput as BusDirection;
                        bus.channelCount = 2;
                        copy_wstring("Output", &mut bus.name);
                        bus.busType = BusTypes_::kMain as BusType;
                        bus.flags = BusInfo_::BusFlags_::kDefaultActive as u32 as uint32;

                        kResultOk
                    }
                    _ => kInvalidArgument,
                },
                _ => kInvalidArgument,
            },
            MediaTypes_::kEvent => kInvalidArgument,
            _ => kInvalidArgument,
        }
    }

    unsafe fn getRoutingInfo(
        &self,
        _in_info: *mut RoutingInfo,
        _out_info: *mut RoutingInfo,
    ) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(
        &self,
        _media_type: MediaType,
        _dir: BusDirection,
        _index: i32,
        _state: TBool,
    ) -> tresult {
        kResultOk
    }

    unsafe fn setActive(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        let Some(stream) = ComRef::from_raw(state) else {
            return kInvalidArgument;
        };
        let mut bytes = [0u8; 8];
        let mut read = 0i32;
        if stream.read(bytes.as_mut_ptr().cast(), 8, &mut read) == kResultOk && read == 8 {
            let gain = f64::from_le_bytes(bytes);
            self.gain.store(gain.to_bits(), Ordering::Relaxed);
        }
        kResultOk
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        let Some(stream) = ComRef::from_raw(state) else {
            return kInvalidArgument;
        };
        let bytes = f64::from_bits(self.gain.load(Ordering::Relaxed)).to_le_bytes();
        let mut written = 0i32;
        stream.write(bytes.as_ptr() as *mut c_void, 8, &mut written)
    }
}

impl IAudioProcessorTrait for GainProcessor {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if inputs.is_null() || outputs.is_null() {
            return kInvalidArgument;
        }
        if num_ins != 1 || num_outs != 1 {
            return kResultFalse;
        }

        if *inputs != SpeakerArr::kStereo || *outputs != SpeakerArr::kStereo {
            return kResultFalse;
        }

        kResultTrue
    }

    unsafe fn getBusArrangement(
        &self,
        dir: BusDirection,
        index: i32,
        arr: *mut SpeakerArrangement,
    ) -> tresult {
        if arr.is_null() {
            return kInvalidArgument;
        }
        match dir as BusDirections {
            BusDirections_::kInput => {
                if index == 0 {
                    *arr = SpeakerArr::kStereo;
                    kResultOk
                } else {
                    kInvalidArgument
                }
            }
            BusDirections_::kOutput => {
                if index == 0 {
                    *arr = SpeakerArr::kStereo;
                    kResultOk
                } else {
                    kInvalidArgument
                }
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn canProcessSampleSize(&self, symbolic_sample_size: i32) -> tresult {
        match symbolic_sample_size as SymbolicSampleSizes {
            SymbolicSampleSizes_::kSample32 => kResultOk as i32,
            SymbolicSampleSizes_::kSample64 => kNotImplemented as i32,
            _ => kInvalidArgument,
        }
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        32
    }

    unsafe fn setupProcessing(&self, _setup: *mut ProcessSetup) -> tresult {
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        if data.is_null() {
            return kInvalidArgument;
        }
        let process_data = &*data;

        if let Some(param_changes) = ComRef::from_raw(process_data.inputParameterChanges) {
            let param_count = param_changes.getParameterCount();
            for param_index in 0..param_count {
                if let Some(param_queue) =
                    ComRef::from_raw(param_changes.getParameterData(param_index))
                {
                    let param_id = param_queue.getParameterId();
                    let point_count = param_queue.getPointCount();
                    if point_count <= 0 {
                        continue;
                    }

                    match param_id {
                        0 => {
                            let mut sample_offset = 0;
                            let mut value = 0.0;
                            let result = param_queue.getPoint(
                                point_count - 1,
                                &mut sample_offset,
                                &mut value,
                            );

                            if result == kResultTrue {
                                self.gain.store(value.to_bits(), Ordering::Relaxed);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let gain = f64::from_bits(self.gain.load(Ordering::Relaxed)) as f32;

        if process_data.numSamples < 0
            || process_data.inputs.is_null()
            || process_data.outputs.is_null()
            || process_data.numInputs != 1
            || process_data.numOutputs != 1
        {
            return kResultOk;
        }
        let num_samples = process_data.numSamples as usize;
        if num_samples == 0 {
            return kResultOk;
        }

        let input_buses = &*process_data.inputs;
        let output_buses = &*process_data.outputs;
        if input_buses.numChannels != 2
            || output_buses.numChannels != 2
            || input_buses.__field0.channelBuffers32.is_null()
            || output_buses.__field0.channelBuffers32.is_null()
        {
            return kResultOk;
        }
        let input_channels = input_buses.__field0.channelBuffers32;
        let output_channels = output_buses.__field0.channelBuffers32;
        if input_channels.is_null()
            || output_channels.is_null()
            || (*input_channels.add(0)).is_null()
            || (*input_channels.add(1)).is_null()
            || (*output_channels.add(0)).is_null()
            || (*output_channels.add(1)).is_null()
        {
            return kResultOk;
        }

        // Input and output may alias for in-place processing. Read one frame
        // before writing it instead of forming overlapping slices.
        let mut delay = self.delay.lock().unwrap_or_else(|e| e.into_inner());
        for i in 0..num_samples {
            let left = *(*input_channels.add(0)).add(i);
            let right = *(*input_channels.add(1)).add(i);
            let idx = delay.index;
            let out_left = delay.left[idx];
            let out_right = delay.right[idx];
            delay.left[idx] = gain * left;
            delay.right[idx] = gain * right;
            delay.index = (idx + 1) % 32;
            *(*output_channels.add(0)).add(i) = out_left;
            *(*output_channels.add(1)).add(i) = out_right;
        }

        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}

impl IProcessContextRequirementsTrait for GainProcessor {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        0
    }
}

struct GainController {
    gain: AtomicU64,
}

impl Class for GainController {
    type Interfaces = (IEditController,);
}

impl GainController {
    const CID: TUID = uid(0x1BA8A477, 0xEE0A4A2D, 0x80F50D14, 0x13D2EAA0);

    fn new() -> GainController {
        GainController {
            gain: AtomicU64::new(1.0f64.to_bits()),
        }
    }
}

impl IPluginBaseTrait for GainController {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IEditControllerTrait for GainController {
    unsafe fn setComponentState(&self, _state: *mut IBStream) -> tresult {
        kNotImplemented
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        let Some(stream) = ComRef::from_raw(state) else {
            return kInvalidArgument;
        };
        let mut bytes = [0u8; 8];
        let mut read = 0i32;
        if stream.read(bytes.as_mut_ptr().cast(), 8, &mut read) == kResultOk && read == 8 {
            let gain = f64::from_le_bytes(bytes);
            self.gain.store(gain.to_bits(), Ordering::Relaxed);
        }
        kResultOk
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        let Some(stream) = ComRef::from_raw(state) else {
            return kInvalidArgument;
        };
        let bytes = f64::from_bits(self.gain.load(Ordering::Relaxed)).to_le_bytes();
        let mut written = 0i32;
        stream.write(bytes.as_ptr() as *mut c_void, 8, &mut written)
    }

    unsafe fn getParameterCount(&self) -> i32 {
        1
    }

    unsafe fn getParameterInfo(&self, param_index: i32, info: *mut ParameterInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        match param_index {
            0 => {
                let info = &mut *info;

                info.id = 0;
                copy_wstring("Gain", &mut info.title);
                copy_wstring("Gain", &mut info.shortTitle);
                copy_wstring("", &mut info.units);
                info.stepCount = 0;
                info.defaultNormalizedValue = 1.0;
                info.unitId = 0;
                info.flags = ParameterInfo_::ParameterFlags_::kCanAutomate as i32;

                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn getParamStringByValue(
        &self,
        id: u32,
        value_normalized: f64,
        string: *mut String128,
    ) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let slice = unsafe { &mut *string };

        match id {
            0 => {
                let display = value_normalized.to_string();
                copy_wstring(&display, slice);
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn getParamValueByString(
        &self,
        id: u32,
        string: *mut TChar,
        value_normalized: *mut f64,
    ) -> tresult {
        if string.is_null() || value_normalized.is_null() {
            return kInvalidArgument;
        }
        match id {
            0 => {
                let len = len_wstring(string as *const TChar);
                if let Ok(string) =
                    String::from_utf16(slice::from_raw_parts(string as *const u16, len))
                {
                    if let Ok(value) = f64::from_str(&string) {
                        *value_normalized = value;
                        return kResultOk;
                    }
                }
                kInvalidArgument
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value_normalized: f64) -> f64 {
        match id {
            0 => value_normalized,
            _ => 0.0,
        }
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain_value: f64) -> f64 {
        match id {
            0 => plain_value,
            _ => 0.0,
        }
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        match id {
            0 => f64::from_bits(self.gain.load(Ordering::Relaxed)),
            _ => 0.0,
        }
    }

    unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
        match id {
            0 => {
                self.gain.store(value.to_bits(), Ordering::Relaxed);
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn setComponentHandler(&self, _handler: *mut IComponentHandler) -> tresult {
        kResultOk
    }

    unsafe fn createView(&self, name: *const c_char) -> *mut IPlugView {
        if name.is_null() {
            return ptr::null_mut();
        }
        let requested = std::ffi::CStr::from_ptr(name).to_str().unwrap_or("");
        if requested != "editor" {
            return ptr::null_mut();
        }
        ComWrapper::new(GainPlugView::new())
            .to_com_ptr::<IPlugView>()
            .map_or(ptr::null_mut(), |ptr| ptr.into_raw())
    }
}

struct GainPlugView {
    width: std::sync::atomic::AtomicI32,
    height: std::sync::atomic::AtomicI32,
}

impl GainPlugView {
    fn new() -> Self {
        Self {
            width: std::sync::atomic::AtomicI32::new(360),
            height: std::sync::atomic::AtomicI32::new(220),
        }
    }
}

impl Class for GainPlugView {
    type Interfaces = (IPlugView,);
}

impl IPlugViewTrait for GainPlugView {
    unsafe fn isPlatformTypeSupported(&self, kind: FIDString) -> tresult {
        if kind.is_null() {
            return kInvalidArgument;
        }
        let name = std::ffi::CStr::from_ptr(kind).to_str().unwrap_or("");
        if name == "NSView" || name == "HWND" || name == "X11EmbedWindowID" {
            kResultOk
        } else {
            kNotImplemented
        }
    }

    unsafe fn attached(&self, parent: *mut c_void, kind: FIDString) -> tresult {
        if parent.is_null() || self.isPlatformTypeSupported(kind) != kResultOk {
            return kInvalidArgument;
        }
        kResultOk
    }

    unsafe fn removed(&self) -> tresult {
        kResultOk
    }

    unsafe fn onWheel(&self, _distance: f32) -> tresult {
        kResultFalse
    }

    unsafe fn onKeyDown(&self, _key: char16, _key_code: int16, _modifiers: int16) -> tresult {
        kResultFalse
    }

    unsafe fn onKeyUp(&self, _key: char16, _key_code: int16, _modifiers: int16) -> tresult {
        kResultFalse
    }

    unsafe fn getSize(&self, size: *mut ViewRect) -> tresult {
        if size.is_null() {
            return kInvalidArgument;
        }
        let rect = &mut *size;
        rect.left = 0;
        rect.top = 0;
        rect.right = self.width.load(Ordering::Relaxed);
        rect.bottom = self.height.load(Ordering::Relaxed);
        kResultOk
    }

    unsafe fn onSize(&self, new_size: *mut ViewRect) -> tresult {
        if new_size.is_null() {
            return kInvalidArgument;
        }
        let rect = &*new_size;
        let w = rect.right.saturating_sub(rect.left).max(120);
        let h = rect.bottom.saturating_sub(rect.top).max(80);
        self.width.store(w, Ordering::Relaxed);
        self.height.store(h, Ordering::Relaxed);
        kResultOk
    }

    unsafe fn onFocus(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setFrame(&self, _frame: *mut IPlugFrame) -> tresult {
        kResultOk
    }

    unsafe fn canResize(&self) -> tresult {
        kResultTrue
    }

    unsafe fn checkSizeConstraint(&self, rect: *mut ViewRect) -> tresult {
        if rect.is_null() {
            return kInvalidArgument;
        }
        let rect = &mut *rect;
        if rect.right.saturating_sub(rect.left) < 120 {
            rect.right = rect.left.saturating_add(120);
        }
        if rect.bottom.saturating_sub(rect.top) < 80 {
            rect.bottom = rect.top.saturating_add(80);
        }
        kResultOk
    }
}

struct SynthProcessor {
    phase: std::sync::atomic::AtomicU64,
    frequency: std::sync::atomic::AtomicU64,
    active: std::sync::atomic::AtomicBool,
}

impl SynthProcessor {
    const CID: TUID = uid(0x4C41_5A59, 0x5445_5354, 0x5359_4E54, 0x0000_0003);

    fn new() -> Self {
        Self {
            phase: AtomicU64::new(0.0f64.to_bits()),
            frequency: AtomicU64::new(440.0f64.to_bits()),
            active: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl Class for SynthProcessor {
    type Interfaces = (IComponent, IAudioProcessor);
}

impl IPluginBaseTrait for SynthProcessor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for SynthProcessor {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        kNotImplemented
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        match (media_type as MediaTypes, dir as BusDirections) {
            (MediaTypes_::kAudio, BusDirections_::kOutput) => 1,
            (MediaTypes_::kAudio, BusDirections_::kInput) => 0,
            (MediaTypes_::kEvent, BusDirections_::kInput) => 1,
            _ => 0,
        }
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        if bus.is_null() {
            return kInvalidArgument;
        }
        if index != 0 {
            return kInvalidArgument;
        }
        let bus = &mut *bus;
        match (media_type as MediaTypes, dir as BusDirections) {
            (MediaTypes_::kAudio, BusDirections_::kOutput) => {
                bus.mediaType = MediaTypes_::kAudio as MediaType;
                bus.direction = BusDirections_::kOutput as BusDirection;
                bus.channelCount = 2;
                copy_wstring("Output", &mut bus.name);
                bus.busType = BusTypes_::kMain as BusType;
                bus.flags = BusInfo_::BusFlags_::kDefaultActive as u32;
                kResultOk
            }
            (MediaTypes_::kEvent, BusDirections_::kInput) => {
                bus.mediaType = MediaTypes_::kEvent as MediaType;
                bus.direction = BusDirections_::kInput as BusDirection;
                bus.channelCount = 16;
                copy_wstring("MIDI In", &mut bus.name);
                bus.busType = BusTypes_::kMain as BusType;
                bus.flags = BusInfo_::BusFlags_::kDefaultActive as u32;
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn getRoutingInfo(
        &self,
        _in_info: *mut RoutingInfo,
        _out_info: *mut RoutingInfo,
    ) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(
        &self,
        _media_type: MediaType,
        _dir: BusDirection,
        _index: i32,
        _state: TBool,
    ) -> tresult {
        kResultOk
    }

    unsafe fn setActive(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }
}

impl IAudioProcessorTrait for SynthProcessor {
    unsafe fn setBusArrangements(
        &self,
        _inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if outputs.is_null() {
            return kInvalidArgument;
        }
        if num_ins != 0 || num_outs != 1 || *outputs != SpeakerArr::kStereo {
            return kResultFalse;
        }
        kResultTrue
    }

    unsafe fn getBusArrangement(
        &self,
        dir: BusDirection,
        index: i32,
        arr: *mut SpeakerArrangement,
    ) -> tresult {
        if arr.is_null() {
            return kInvalidArgument;
        }
        if index != 0 || dir as BusDirections != BusDirections_::kOutput {
            return kInvalidArgument;
        }
        *arr = SpeakerArr::kStereo;
        kResultOk
    }

    unsafe fn canProcessSampleSize(&self, size: i32) -> tresult {
        if size as SymbolicSampleSizes == SymbolicSampleSizes_::kSample32 {
            kResultOk
        } else {
            kNotImplemented
        }
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        0
    }

    unsafe fn setupProcessing(&self, _setup: *mut ProcessSetup) -> tresult {
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        if data.is_null() {
            return kInvalidArgument;
        }
        let process_data = &*data;
        // Notes take effect at their sample offset, so the output does not
        // depend on the block size.
        let mut changes = [(0_i32, false, 0.0_f64); 64];
        let mut count = 0;
        if let Some(events) = ComRef::from_raw(process_data.inputEvents) {
            for index in 0..events.getEventCount() {
                let mut event: Event = std::mem::zeroed();
                if events.getEvent(index, &mut event) != kResultOk || count == changes.len() {
                    continue;
                }
                if event.r#type == Event_::EventTypes_::kNoteOnEvent as u16 {
                    let note = event.__field0.noteOn;
                    let frequency = 440.0 * 2.0_f64.powf((f64::from(note.pitch) - 69.0) / 12.0);
                    changes[count] = (event.sampleOffset, note.velocity > 0.0, frequency);
                    count += 1;
                } else if event.r#type == Event_::EventTypes_::kNoteOffEvent as u16 {
                    changes[count] = (event.sampleOffset, false, 0.0);
                    count += 1;
                }
            }
        }
        let changes = &mut changes[..count];
        changes.sort_by_key(|change| change.0);
        let mut frequency = f64::from_bits(self.frequency.load(Ordering::Relaxed));
        let mut active = self.active.load(Ordering::Relaxed);
        let mut apply = |index: i32, frequency: &mut f64, active: &mut bool, next: &mut usize| {
            while *next < changes.len() && changes[*next].0 <= index {
                let (_, on, value) = changes[*next];
                if on {
                    *frequency = value;
                }
                *active = on;
                *next += 1;
            }
        };
        let mut next = 0;
        if process_data.numOutputs != 1
            || process_data.numSamples < 0
            || process_data.outputs.is_null()
        {
            apply(i32::MAX, &mut frequency, &mut active, &mut next);
            self.frequency.store(frequency.to_bits(), Ordering::Relaxed);
            self.active.store(active, Ordering::Relaxed);
            return kResultOk;
        }
        let outputs = &*process_data.outputs;
        let channels = outputs.__field0.channelBuffers32;
        let num_samples = process_data.numSamples as usize;
        if outputs.numChannels != 2
            || channels.is_null()
            || (*channels.add(0)).is_null()
            || (*channels.add(1)).is_null()
        {
            apply(i32::MAX, &mut frequency, &mut active, &mut next);
            self.frequency.store(frequency.to_bits(), Ordering::Relaxed);
            self.active.store(active, Ordering::Relaxed);
            return kResultOk;
        }
        let left = *channels.add(0);
        let right = *channels.add(1);
        let mut phase = f64::from_bits(self.phase.load(Ordering::Relaxed));
        for index in 0..num_samples {
            apply(index as i32, &mut frequency, &mut active, &mut next);
            let value = if active {
                let sample = (std::f64::consts::TAU * phase).sin() * 0.25;
                phase = (phase + frequency / 48_000.0).fract();
                sample as f32
            } else {
                0.0
            };
            *left.add(index) = value;
            *right.add(index) = value;
        }
        apply(i32::MAX, &mut frequency, &mut active, &mut next);
        self.phase.store(phase.to_bits(), Ordering::Relaxed);
        self.frequency.store(frequency.to_bits(), Ordering::Relaxed);
        self.active.store(active, Ordering::Relaxed);
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        64
    }
}

struct Factory {}

impl Class for Factory {
    type Interfaces = (IPluginFactory,);
}

impl IPluginFactoryTrait for Factory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        let info = &mut *info;

        copy_cstring("Vendor", &mut info.vendor);
        copy_cstring("https://example.com", &mut info.url);
        copy_cstring("someone@example.com", &mut info.email);
        info.flags = PFactoryInfo_::FactoryFlags_::kUnicode as int32;

        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        3
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        if info.is_null() {
            return kInvalidArgument;
        }
        match index {
            0 => {
                let info = &mut *info;
                info.cid = GainProcessor::CID;
                info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
                copy_cstring("Audio Module Class", &mut info.category);
                copy_cstring(PLUGIN_NAME, &mut info.name);

                kResultOk
            }
            1 => {
                let info = &mut *info;
                info.cid = GainController::CID;
                info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
                copy_cstring("Component Controller Class", &mut info.category);
                copy_cstring(PLUGIN_NAME, &mut info.name);

                kResultOk
            }
            2 => {
                let info = &mut *info;
                info.cid = SynthProcessor::CID;
                info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
                copy_cstring("Audio Module Class", &mut info.category);
                copy_cstring("Lazy Arranger Test Synth", &mut info.name);
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn createInstance(
        &self,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult {
        if cid.is_null() || iid.is_null() || obj.is_null() {
            return kInvalidArgument;
        }
        let instance = match *(cid as *const TUID) {
            GainProcessor::CID => Some(
                ComWrapper::new(GainProcessor::new())
                    .to_com_ptr::<FUnknown>()
                    .unwrap(),
            ),
            GainController::CID => Some(
                ComWrapper::new(GainController::new())
                    .to_com_ptr::<FUnknown>()
                    .unwrap(),
            ),
            SynthProcessor::CID => Some(
                ComWrapper::new(SynthProcessor::new())
                    .to_com_ptr::<FUnknown>()
                    .unwrap(),
            ),
            _ => None,
        };

        if let Some(instance) = instance {
            let ptr = instance.as_ptr();
            ((*(*ptr).vtbl).queryInterface)(ptr, iid as *mut TUID, obj)
        } else {
            kInvalidArgument
        }
    }
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
extern "system" fn InitDll() -> bool {
    true
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
extern "system" fn ExitDll() -> bool {
    true
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
extern "system" fn BundleEntry(_bundle_ref: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
extern "system" fn BundleExit() -> bool {
    true
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
extern "system" fn ModuleEntry(_library_handle: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
extern "system" fn ModuleExit() -> bool {
    true
}

#[unsafe(no_mangle)]
extern "system" fn GetPluginFactory() -> *mut IPluginFactory {
    ComWrapper::new(Factory {})
        .to_com_ptr::<IPluginFactory>()
        .unwrap()
        .into_raw()
}
