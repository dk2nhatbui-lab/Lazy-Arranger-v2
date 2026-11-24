#![allow(clippy::unnecessary_cast)]
//! Minimal in-process VST3 instrument: no audio input, MIDI event input and a
//! stereo output block loop.

use std::path::Path;
use std::sync::Mutex;

use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, BusDirections_, Event,
    Event_::EventTypes_::{kNoteOffEvent, kNoteOnEvent},
    Event__type0, IAudioProcessor, IAudioProcessorTrait, IComponent, IComponentTrait, IEventList,
    IEventListTrait, MediaTypes_, NoteOffEvent, NoteOnEvent, ProcessContext, ProcessData,
    ProcessSetup, SymbolicSampleSizes_,
};
use vst3::Steinberg::{IPluginBaseTrait, IPluginFactory, kResultOk};
use vst3::{Class, ComPtr, ComWrapper};

use crate::instance::{HostApplication, create, parse_cid, vst3_binary};

struct PendingEvent {
    offset: i32,
    on: bool,
    key: u8,
    velocity: u8,
}

struct EventList {
    events: Mutex<Vec<Event>>,
}

impl Class for EventList {
    type Interfaces = (IEventList,);
}

impl IEventListTrait for EventList {
    unsafe fn getEventCount(&self) -> i32 {
        self.events.lock().map_or(0, |events| events.len() as i32)
    }

    unsafe fn getEvent(&self, index: i32, event: *mut Event) -> vst3::Steinberg::tresult {
        let Ok(events) = self.events.lock() else {
            return vst3::Steinberg::kResultFalse;
        };
        let Some(source) = usize::try_from(index)
            .ok()
            .and_then(|index| events.get(index))
        else {
            return vst3::Steinberg::kResultFalse;
        };
        if !event.is_null() {
            unsafe { *event = *source };
        }
        kResultOk
    }

    unsafe fn addEvent(&self, event: *mut Event) -> vst3::Steinberg::tresult {
        let Ok(mut events) = self.events.lock() else {
            return vst3::Steinberg::kResultFalse;
        };
        if !event.is_null() {
            events.push(unsafe { *event });
        }
        kResultOk
    }
}

/// One active VST3 instrument.
pub struct Vst3Instrument {
    component: ComPtr<IComponent>,
    processor: ComPtr<IAudioProcessor>,
    sample_time: f64,
    sample_rate: f64,
    pending: Vec<PendingEvent>,
    // Keep the host and plug-in module alive while COM objects can call them.
    _host: ComPtr<vst3::Steinberg::Vst::IHostApplication>,
    _library: libloading::Library,
}

impl std::fmt::Debug for Vst3Instrument {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Vst3Instrument")
            .field("sample_time", &self.sample_time)
            .field("pending", &self.pending.len())
            .finish()
    }
}

impl Vst3Instrument {
    /// Loads a class and activates it as an instrument with a stereo output.
    ///
    /// # Errors
    ///
    /// Returns a message when the class cannot be created, initialized or
    /// activated, or when it has no audio output bus.
    pub fn new(
        bundle: &Path,
        class_id: &str,
        sample_rate: f64,
        max_block: usize,
    ) -> Result<Self, String> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Self::new_inner(bundle, class_id, sample_rate, max_block)
        }))
        .unwrap_or_else(|_| Err(String::from("VST3 instrument panicked")))
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
            .to_com_ptr::<vst3::Steinberg::Vst::IHostApplication>()
            .ok_or_else(|| String::from("host application"))?;
        let component = create::<IComponent>(&factory, &cid)?;
        let init = unsafe { component.initialize(host.as_ptr().cast()) };
        if init != kResultOk {
            return Err(format!("component initialize returned {init}"));
        }
        let processor = component
            .cast::<IAudioProcessor>()
            .ok_or_else(|| String::from("component has no IAudioProcessor"))?;
        let inputs = unsafe {
            component.getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kInput as i32)
        }
        .max(0);
        if inputs > 0 {
            return Err(String::from(
                "plug-in has audio inputs; it is not a pure instrument",
            ));
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
                BusDirections_::kOutput as i32,
                0,
                1,
            );
        }
        let mut setup = ProcessSetup {
            processMode: 0,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            maxSamplesPerBlock: i32::try_from(max_block.min(i32::MAX as usize)).unwrap_or(i32::MAX),
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
            sample_time: 0.0,
            sample_rate,
            pending: Vec::new(),
            _host: host,
            _library: library,
        })
    }

    /// Queues a note-on at a sample offset inside the next block.
    pub fn note_on_at(&mut self, key: u8, velocity: u8, offset: u32) {
        self.pending.push(PendingEvent {
            offset: i32::try_from(offset).unwrap_or(0),
            on: true,
            key,
            velocity,
        });
    }

    /// Queues a note-off at a sample offset inside the next block.
    pub fn note_off_at(&mut self, key: u8, offset: u32) {
        self.pending.push(PendingEvent {
            offset: i32::try_from(offset).unwrap_or(0),
            on: false,
            key,
            velocity: 0,
        });
    }

    /// Renders one stereo block into `left`/`right`.
    ///
    /// # Errors
    ///
    /// Returns a message when the plug-in rejects the block.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<(), String> {
        let frames = left.len().min(right.len());
        if frames == 0 {
            return Ok(());
        }
        let mut events = Vec::with_capacity(self.pending.len());
        for pending in self.pending.drain(..) {
            let mut event: Event = unsafe { std::mem::zeroed() };
            event.busIndex = 0;
            event.sampleOffset = pending.offset.min(frames as i32 - 1).max(0);
            event.ppqPosition = 0.0;
            event.flags = 0;
            if pending.on {
                event.r#type = kNoteOnEvent as u16;
                event.__field0 = Event__type0 {
                    noteOn: NoteOnEvent {
                        channel: 0,
                        pitch: i16::from(pending.key),
                        tuning: 0.0,
                        velocity: f32::from(pending.velocity) / 127.0,
                        length: 0,
                        noteId: -1,
                    },
                };
            } else {
                event.r#type = kNoteOffEvent as u16;
                event.__field0 = Event__type0 {
                    noteOff: NoteOffEvent {
                        channel: 0,
                        pitch: i16::from(pending.key),
                        velocity: 0.0,
                        noteId: -1,
                        tuning: 0.0,
                    },
                };
            }
            events.push(event);
        }
        let list = ComWrapper::new(EventList {
            events: Mutex::new(events),
        })
        .to_com_ptr::<IEventList>()
        .ok_or_else(|| String::from("event list"))?;
        let mut input_channels = [left.as_mut_ptr(), right.as_mut_ptr()];
        let mut outputs = [AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: input_channels.as_mut_ptr(),
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
            numInputs: 0,
            numOutputs: 1,
            inputs: std::ptr::null_mut(),
            outputs: outputs.as_mut_ptr(),
            inputParameterChanges: std::ptr::null_mut(),
            outputParameterChanges: std::ptr::null_mut(),
            inputEvents: list.as_ptr(),
            outputEvents: std::ptr::null_mut(),
            processContext: &mut context,
        };
        let result = unsafe { self.processor.process(&mut data) };
        if result != kResultOk {
            return Err(format!("process returned {result}"));
        }
        self.sample_time += frames as f64;
        Ok(())
    }

    /// Restores component state from base64.
    ///
    /// # Errors
    ///
    /// Returns a message when the text is not base64 or the plug-in rejects it.
    pub fn load_state_base64(&mut self, state: &str) -> Result<(), String> {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(state.trim())
            .map_err(|error| format!("base64 state: {error}"))?;
        let (stream, _) = crate::instance::memory_stream_from_bytes(&bytes);
        let pointer = stream
            .to_com_ptr::<vst3::Steinberg::IBStream>()
            .ok_or_else(|| String::from("memory stream"))?;
        let result = unsafe { self.component.setState(pointer.as_ptr()) };
        if result != kResultOk {
            return Err(format!("setState returned {result}"));
        }
        Ok(())
    }

    /// Saves the component state as base64.
    ///
    /// # Errors
    ///
    /// Returns a message when the component has no state.
    pub fn save_state_base64(&self) -> Result<String, String> {
        use base64::Engine as _;
        let (stream, data) = crate::instance::memory_stream();
        let pointer = stream
            .to_com_ptr::<vst3::Steinberg::IBStream>()
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
            return Err(String::from("plug-in wrote no state"));
        }
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    }
}

impl Drop for Vst3Instrument {
    fn drop(&mut self) {
        unsafe {
            let _ = self.processor.setProcessing(0);
            let _ = self.component.setActive(0);
            let _ = self.component.terminate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "loads an installed VST3 instrument"]
    fn labs_accepts_events_and_renders_a_block() {
        let bundle = Path::new("/Library/Audio/Plug-Ins/VST3/LABS.vst3");
        let classes = crate::scan_vst3(bundle).expect("scan");
        let mut instrument =
            Vst3Instrument::new(bundle, &classes[0].cid, 48_000.0, 512).expect("instrument");
        instrument.note_on_at(60, 100, 0);
        let mut left = vec![0.0_f32; 512];
        let mut right = vec![0.0_f32; 512];
        instrument
            .process_block(&mut left, &mut right)
            .expect("process");
        instrument.note_off_at(60, 0);
        let mut left = vec![0.0_f32; 512];
        let mut right = vec![0.0_f32; 512];
        instrument
            .process_block(&mut left, &mut right)
            .expect("tail");
    }
}
