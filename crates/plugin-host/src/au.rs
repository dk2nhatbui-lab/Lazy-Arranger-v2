//! Minimal Audio Unit v2 effect host (macOS).
//!
//! Uses `AudioUnitProcess` with non-interleaved 32-bit float stereo buffers.
//! No parameter, state or editor support yet.

use std::ffi::c_void;

type OsStatus = i32;
type AudioComponent = *mut c_void;
type AudioUnit = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct AudioComponentDescription {
    component_type: u32,
    component_sub_type: u32,
    component_manufacturer: u32,
    component_flags: u32,
    component_flags_mask: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AudioBuffer {
    number_channels: u32,
    data_byte_size: u32,
    data: *mut c_void,
}

#[repr(C)]
struct AudioBufferList2 {
    number_buffers: u32,
    buffers: [AudioBuffer; 2],
}

#[repr(C)]
struct AudioUnitParameterInfo {
    name: [std::ffi::c_char; 52],
    unit_name: *const c_void,
    clump_id: u32,
    cf_name: *const c_void,
    unit: u32,
    min: f32,
    max: f32,
    default: f32,
    flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AudioTimeStamp {
    sample_time: f64,
    host_time: u64,
    rate_scalar: f64,
    word_clock_time: u64,
    smpte_time: [u32; 4],
    flags: u32,
    reserved: u32,
}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioComponentFindNext(
        component: AudioComponent,
        description: *const AudioComponentDescription,
    ) -> AudioComponent;
    fn AudioComponentInstanceNew(component: AudioComponent, instance: *mut AudioUnit) -> OsStatus;
    fn AudioComponentInstanceDispose(instance: AudioUnit) -> OsStatus;
    fn AudioUnitSetProperty(
        unit: AudioUnit,
        property: u32,
        scope: u32,
        element: u32,
        data: *const c_void,
        size: u32,
    ) -> OsStatus;
    fn AudioUnitGetProperty(
        unit: AudioUnit,
        property: u32,
        scope: u32,
        element: u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> OsStatus;
    fn AudioUnitGetParameter(
        unit: AudioUnit,
        id: u32,
        scope: u32,
        element: u32,
        value: *mut f32,
    ) -> OsStatus;
    fn AudioUnitSetParameter(
        unit: AudioUnit,
        id: u32,
        scope: u32,
        element: u32,
        value: f32,
        offset: u32,
    ) -> OsStatus;
    fn AudioUnitInitialize(unit: AudioUnit) -> OsStatus;
    fn AudioUnitUninitialize(unit: AudioUnit) -> OsStatus;
    fn AudioUnitProcess(
        unit: AudioUnit,
        flags: *mut u32,
        timestamp: *const AudioTimeStamp,
        frames: u32,
        data: *mut AudioBufferList2,
    ) -> OsStatus;
}

const AU_EFFECT: u32 = 0x6175_6678; // 'aufx'
const STREAM_FORMAT: u32 = 8;
const PARAMETER_LIST: u32 = 3;
const PARAMETER_INFO: u32 = 4;
const SCOPE_GLOBAL: u32 = 0;
const MAX_FRAMES: u32 = 14;
const SCOPE_INPUT: u32 = 1;
const SCOPE_OUTPUT: u32 = 2;
const LINEAR_PCM: u32 = 0x6c70_636d; // 'lpcm'
const FLOAT: u32 = 1;
const PACKED: u32 = 1 << 3;
const NON_INTERLEAVED: u32 = 1 << 5;

/// One active Audio Unit effect.
pub struct AuEffect {
    unit: AudioUnit,
    sample_time: f64,
}

impl std::fmt::Debug for AuEffect {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuEffect")
            .field("sample_time", &self.sample_time)
            .finish()
    }
}

impl AuEffect {
    /// Opens an installed AU effect by `manufacturer:subtype`.
    ///
    /// # Errors
    ///
    /// Returns a message when the component is absent, the format is rejected
    /// or initialization fails.
    pub fn new(identifier: &str, sample_rate: f64) -> Result<Self, String> {
        let (manufacturer, subtype) = identifier
            .split_once(':')
            .ok_or_else(|| String::from("AU id must be manufacturer:subtype"))?;
        let description = AudioComponentDescription {
            component_type: AU_EFFECT,
            component_sub_type: fourcc(subtype),
            component_manufacturer: fourcc(manufacturer),
            ..AudioComponentDescription::default()
        };
        let component = unsafe { AudioComponentFindNext(std::ptr::null_mut(), &description) };
        if component.is_null() {
            return Err(format!("AU effect {identifier} not found"));
        }
        let mut unit: AudioUnit = std::ptr::null_mut();
        let result = unsafe { AudioComponentInstanceNew(component, &mut unit) };
        if result != 0 || unit.is_null() {
            return Err(format!("AudioComponentInstanceNew returned {result}"));
        }
        let effect = Self {
            unit,
            sample_time: 0.0,
        };
        effect.configure(sample_rate)?;
        Ok(effect)
    }

    fn configure(&self, sample_rate: f64) -> Result<(), String> {
        let format = AudioStreamBasicDescription {
            sample_rate,
            format_id: LINEAR_PCM,
            format_flags: FLOAT | PACKED | NON_INTERLEAVED,
            bytes_per_packet: 4,
            frames_per_packet: 1,
            bytes_per_frame: 4,
            channels_per_frame: 2,
            bits_per_channel: 32,
            reserved: 0,
        };
        for (scope, name) in [(SCOPE_INPUT, "input"), (SCOPE_OUTPUT, "output")] {
            let result = unsafe {
                AudioUnitSetProperty(
                    self.unit,
                    STREAM_FORMAT,
                    scope,
                    0,
                    (&raw const format).cast(),
                    u32::try_from(std::mem::size_of::<AudioStreamBasicDescription>()).unwrap_or(0),
                )
            };
            if result != 0 {
                return Err(format!("set {name} format returned {result}"));
            }
        }
        let frames = 4096_u32;
        let result = unsafe {
            AudioUnitSetProperty(
                self.unit,
                MAX_FRAMES,
                0,
                0,
                (&raw const frames).cast(),
                u32::try_from(std::mem::size_of::<u32>()).unwrap_or(0),
            )
        };
        if result != 0 {
            return Err(format!("set maximum frames returned {result}"));
        }
        let result = unsafe { AudioUnitInitialize(self.unit) };
        if result != 0 {
            return Err(format!("AudioUnitInitialize returned {result}"));
        }
        Ok(())
    }

    /// Parameters exposed by this unit: id, name, current, min, max, default.
    #[must_use]
    pub fn parameters(&self) -> Vec<(u32, String, f32, f32, f32, f32)> {
        let mut size = 0_u32;
        let result = unsafe {
            AudioUnitGetProperty(
                self.unit,
                PARAMETER_LIST,
                SCOPE_GLOBAL,
                0,
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if result != 0 || size == 0 {
            return Vec::new();
        }
        let count = size as usize / std::mem::size_of::<u32>();
        let mut ids = vec![0_u32; count];
        let result = unsafe {
            AudioUnitGetProperty(
                self.unit,
                PARAMETER_LIST,
                SCOPE_GLOBAL,
                0,
                ids.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if result != 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        for id in ids {
            let mut info: AudioUnitParameterInfo = unsafe { std::mem::zeroed() };
            let mut info_size =
                u32::try_from(std::mem::size_of::<AudioUnitParameterInfo>()).unwrap_or(0);
            let result = unsafe {
                AudioUnitGetProperty(
                    self.unit,
                    PARAMETER_INFO,
                    SCOPE_GLOBAL,
                    id,
                    (&raw mut info).cast(),
                    &mut info_size,
                )
            };
            if result != 0 {
                continue;
            }
            let mut current = info.default;
            let _ = unsafe { AudioUnitGetParameter(self.unit, id, SCOPE_GLOBAL, 0, &mut current) };
            let name: Vec<u8> = info
                .name
                .iter()
                .take_while(|value| **value != 0)
                .map(|value| *value as u8)
                .collect();
            out.push((
                id,
                String::from_utf8_lossy(&name).into_owned(),
                current,
                info.min,
                info.max,
                info.default,
            ));
        }
        out
    }

    /// Sets one parameter by id.
    ///
    /// # Errors
    ///
    /// Returns a message when the unit rejects the value.
    pub fn set_parameter(&self, id: u32, value: f32) -> Result<(), String> {
        let result = unsafe { AudioUnitSetParameter(self.unit, id, SCOPE_GLOBAL, 0, value, 0) };
        if result == 0 {
            Ok(())
        } else {
            Err(format!("AudioUnitSetParameter returned {result}"))
        }
    }

    /// Processes interleaved stereo samples in place.
    ///
    /// # Errors
    ///
    /// Returns a message when the buffer is not stereo or the unit rejects a block.
    pub fn process_stereo(&mut self, samples: &mut [f32]) -> Result<(), String> {
        if !samples.len().is_multiple_of(2) {
            return Err(String::from("stereo buffer has an odd length"));
        }
        for block in samples.chunks_mut(4096 * 2) {
            let frames = block.len() / 2;
            let mut left = vec![0.0_f32; frames];
            let mut right = vec![0.0_f32; frames];
            for (frame, sample) in block.as_chunks::<2>().0.iter().enumerate() {
                left[frame] = sample[0];
                right[frame] = sample[1];
            }
            let mut buffers = AudioBufferList2 {
                number_buffers: 2,
                buffers: [
                    AudioBuffer {
                        number_channels: 1,
                        data_byte_size: u32::try_from(frames * 4).unwrap_or(0),
                        data: left.as_mut_ptr().cast(),
                    },
                    AudioBuffer {
                        number_channels: 1,
                        data_byte_size: u32::try_from(frames * 4).unwrap_or(0),
                        data: right.as_mut_ptr().cast(),
                    },
                ],
            };
            let timestamp = AudioTimeStamp {
                sample_time: self.sample_time,
                flags: 2,
                ..AudioTimeStamp::default()
            };
            let result = unsafe {
                AudioUnitProcess(
                    self.unit,
                    std::ptr::null_mut(),
                    &timestamp,
                    u32::try_from(frames).unwrap_or(0),
                    &mut buffers,
                )
            };
            if result != 0 {
                return Err(format!("AudioUnitProcess returned {result}"));
            }
            self.sample_time += frames as f64;
            for (frame, sample) in block.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                sample[0] = left[frame];
                sample[1] = right[frame];
            }
        }
        Ok(())
    }
}

impl Drop for AuEffect {
    fn drop(&mut self) {
        unsafe {
            let _ = AudioUnitUninitialize(self.unit);
            let _ = AudioComponentInstanceDispose(self.unit);
        }
    }
}

fn fourcc(text: &str) -> u32 {
    text.as_bytes()
        .iter()
        .take(4)
        .fold(0_u32, |value, byte| (value << 8) | u32::from(*byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_character_codes_are_big_endian() {
        assert_eq!(fourcc("aufx"), AU_EFFECT);
        assert_eq!(fourcc("rvrb"), 0x7276_7262);
    }

    #[test]
    #[ignore = "loads the Apple dynamics Audio Unit"]
    fn apple_dynamics_exposes_parameters() {
        let effect = AuEffect::new("appl:dcmp", 48_000.0).expect("au");
        let parameters = effect.parameters();
        assert!(!parameters.is_empty(), "no parameters");
        let (id, _, current, min, max, _) = parameters[0];
        if min < max {
            let target = (min + max) / 2.0;
            effect.set_parameter(id, target).expect("set");
            let readback = effect
                .parameters()
                .into_iter()
                .find(|parameter| parameter.0 == id)
                .map(|parameter| parameter.2)
                .unwrap_or(current);
            assert!(
                (readback - target).abs() < 1.0e-3,
                "parameter did not retain the value: {readback} vs {target}"
            );
        }
    }

    #[test]
    #[ignore = "loads the installed R0Verb Audio Unit"]
    fn apple_dynamics_au_processes_audio() {
        let mut effect = AuEffect::new("appl:dcmp", 48_000.0).expect("au");
        let mut samples = vec![0.0_f32; 512 * 2];
        for frame in 0..512 {
            samples[frame * 2] = (frame as f32 * 0.02).sin();
            samples[frame * 2 + 1] = samples[frame * 2];
        }
        effect.process_stereo(&mut samples).expect("process");
        assert!(samples.iter().any(|sample| *sample != 0.0));
    }
}
