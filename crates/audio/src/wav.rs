//! WAV decoding for analysis, import and mixing.
//!
//! Supported: PCM at 16, 24 and 32 bits and IEEE float at 32 bits, any channel
//! count. The Zoom L6max records 32-bit float audio, so that path is required.

use crate::RenderedAudio;

struct WavData<'a> {
    format_tag: u16,
    channels: u16,
    sample_rate: u32,
    bits: u16,
    data: &'a [u8],
}

fn parse(bytes: &[u8]) -> Result<WavData<'_>, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(String::from("not a RIFF/WAVE file"));
    }
    let mut offset = 12usize;
    let mut format_tag = 1u16;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits = 0u16;
    let mut data: Option<&[u8]> = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let body_start = offset
            .checked_add(8)
            .ok_or_else(|| String::from("WAV chunk offset overflow"))?;
        let body_end = body_start
            .checked_add(size)
            .ok_or_else(|| String::from("WAV chunk length overflow"))?;
        let body = bytes
            .get(body_start..body_end)
            .ok_or("truncated WAV chunk")?;
        match id {
            b"fmt " if body.len() >= 16 => {
                format_tag = u16::from_le_bytes([body[0], body[1]]);
                channels = u16::from_le_bytes([body[2], body[3]]);
                sample_rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                bits = u16::from_le_bytes([body[14], body[15]]);
            }
            b"data" => data = Some(body),
            _ => {}
        }
        offset = body_end
            .checked_add(size % 2)
            .ok_or_else(|| String::from("WAV chunk offset overflow"))?;
    }
    if channels == 0 || sample_rate == 0 || bits == 0 {
        return Err(String::from("unsupported WAV format"));
    }
    if !matches!((format_tag, bits), (1, 16) | (1, 24) | (1, 32) | (3, 32)) {
        return Err(format!(
            "unsupported WAV format: tag {format_tag}, {bits} bits"
        ));
    }
    Ok(WavData {
        format_tag,
        channels,
        sample_rate,
        bits,
        data: data.ok_or("no data chunk")?,
    })
}

fn sample(chunk: &[u8], start: usize, format_tag: u16, bits: u16) -> f32 {
    match (format_tag, bits) {
        (3, 32) => f32::from_le_bytes([
            chunk[start],
            chunk[start + 1],
            chunk[start + 2],
            chunk[start + 3],
        ]),
        (1, 16) => {
            f32::from(i16::from_le_bytes([chunk[start], chunk[start + 1]])) / f32::from(i16::MAX)
        }
        (1, 24) => {
            let raw =
                i32::from_le_bytes([0, chunk[start], chunk[start + 1], chunk[start + 2]]) >> 8;
            raw as f32 / 8_388_607.0
        }
        (1, 32) => {
            let raw = i32::from_le_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
            raw as f32 / 2_147_483_647.0
        }
        _ => 0.0,
    }
}

/// Decodes WAV into a stereo buffer, preserving the first two channels.
///
/// A mono file is duplicated to both sides; channels beyond the first two are
/// ignored.
///
/// # Errors
///
/// Returns a message for another format or a truncated file.
pub fn decode_stereo(bytes: &[u8]) -> Result<RenderedAudio, String> {
    let wav = parse(bytes)?;
    let bytes_per_sample = usize::from(wav.bits / 8);
    let frame = usize::from(wav.channels) * bytes_per_sample;
    let mut samples = Vec::with_capacity(wav.data.len() / frame * 2);
    for chunk in wav.data.chunks_exact(frame) {
        let left = sample(chunk, 0, wav.format_tag, wav.bits);
        let right = if wav.channels > 1 {
            sample(chunk, bytes_per_sample, wav.format_tag, wav.bits)
        } else {
            left
        };
        samples.push(crate::spsc::sanitize(left));
        samples.push(crate::spsc::sanitize(right));
    }
    Ok(RenderedAudio {
        samples,
        sample_rate: wav.sample_rate,
    })
}

/// Decodes WAV into mono samples by averaging every channel.
///
/// # Errors
///
/// Returns a message for another format or a truncated file.
pub fn decode_mono(bytes: &[u8]) -> Result<RenderedAudio, String> {
    let wav = parse(bytes)?;
    let bytes_per_sample = usize::from(wav.bits / 8);
    let frame = usize::from(wav.channels) * bytes_per_sample;
    let mut samples = Vec::with_capacity(wav.data.len() / frame);
    for chunk in wav.data.chunks_exact(frame) {
        let mut sum = 0.0_f32;
        for channel in 0..usize::from(wav.channels) {
            sum += sample(chunk, channel * bytes_per_sample, wav.format_tag, wav.bits);
        }
        samples.push(crate::spsc::sanitize(sum / f32::from(wav.channels)));
    }
    Ok(RenderedAudio {
        samples,
        sample_rate: wav.sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_own_wav_round_trips() {
        let audio = RenderedAudio {
            samples: vec![0.0, 0.0, 0.5, 0.5, -0.5, -0.5, 1.0, 1.0],
            sample_rate: 48_000,
        };
        let decoded = decode_mono(&audio.to_wav()).expect("decode");
        assert_eq!(decoded.sample_rate, 48_000);
        assert_eq!(decoded.samples.len(), 4);
        assert!((decoded.samples[1] - 0.5).abs() < 0.001);
        assert!((decoded.samples[2] + 0.5).abs() < 0.001);
        assert!((decoded.samples[3] - 1.0).abs() < 0.001);
    }

    fn wav_header(format_tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&format_tag.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * u32::from(channels) * u32::from(bits / 8)).to_le_bytes());
        out.extend_from_slice(&(channels * (bits / 8)).to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn stereo_decode_preserves_both_channels() {
        let audio = RenderedAudio {
            samples: vec![0.25, -0.25, 0.5, -0.5, 0.1, -0.1, 0.2, -0.2],
            sample_rate: 48_000,
        };
        let bytes = audio.to_wav();
        let decoded = decode_stereo(&bytes).expect("decode");
        assert_eq!(decoded.samples.len(), 8);
        assert_eq!(decoded.frames(), 4);
        assert!((decoded.duration_seconds() - 4.0 / 48_000.0).abs() < 1.0e-9);
        assert!((decoded.samples[0] - 0.25).abs() < 0.001);
        assert!((decoded.samples[1] + 0.25).abs() < 0.001);
    }

    #[test]
    fn stereo_pcm_keeps_left_and_right_separate() {
        let mut data = Vec::new();
        for (left, right) in [(0.5_f32, -0.5_f32), (0.25, -0.25)] {
            data.extend_from_slice(&((left * 32767.0) as i16).to_le_bytes());
            data.extend_from_slice(&((right * 32767.0) as i16).to_le_bytes());
        }
        let bytes = wav_header(1, 2, 48_000, 16, &data);
        let decoded = decode_stereo(&bytes).expect("stereo wav");
        assert!((decoded.samples[0] - 0.5).abs() < 0.001);
        assert!((decoded.samples[1] + 0.5).abs() < 0.001);
        assert!((decoded.samples[2] - 0.25).abs() < 0.001);
        assert!((decoded.samples[3] + 0.25).abs() < 0.001);
    }

    #[test]
    fn thirty_two_bit_float_decodes() {
        let mut data = Vec::new();
        for value in [0.0_f32, 0.5, -0.5, 1.0] {
            data.extend_from_slice(&value.to_le_bytes());
            data.extend_from_slice(&value.to_le_bytes());
        }
        let bytes = wav_header(3, 2, 48_000, 32, &data);
        let decoded = decode_mono(&bytes).expect("float wav");
        assert_eq!(decoded.samples.len(), 4);
        assert!((decoded.samples[1] - 0.5).abs() < 1.0e-6);
        assert!((decoded.samples[2] + 0.5).abs() < 1.0e-6);
        let info = info(&bytes).expect("info");
        assert_eq!((info.format_tag, info.bits, info.frames), (3, 32, 4));
    }

    #[test]
    fn twenty_four_bit_pcm_decodes() {
        let mut data = Vec::new();
        for value in [0.5_f32, -0.25] {
            let raw = (value * 8_388_607.0) as i32;
            let bytes = raw.to_le_bytes();
            data.extend_from_slice(&bytes[0..3]);
        }
        let bytes = wav_header(1, 1, 48_000, 24, &data);
        let decoded = decode_mono(&bytes).expect("24-bit wav");
        assert_eq!(decoded.samples.len(), 2);
        assert!((decoded.samples[0] - 0.5).abs() < 1.0e-5);
        assert!((decoded.samples[1] + 0.25).abs() < 1.0e-5);
    }

    #[test]
    fn other_files_are_rejected() {
        assert!(decode_mono(b"nope").is_err());
    }
}

/// What a WAV header declares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WavInfo {
    /// WAV format tag: 1 for PCM, 3 for IEEE float.
    pub format_tag: u16,
    /// Channel count.
    pub channels: u16,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Bits per sample.
    pub bits: u16,
    /// Frames in the data chunk.
    pub frames: u64,
    /// Duration in seconds.
    pub seconds: f64,
}

/// Reads the declared format and length without decoding samples.
///
/// # Errors
///
/// Returns a message for a non-WAV or truncated file.
pub fn info(bytes: &[u8]) -> Result<WavInfo, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(String::from("not a RIFF/WAVE file"));
    }
    let mut offset = 12;
    let mut format_tag = 1u16;
    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits = 0u16;
    let mut data_len = 0usize;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let end = (offset + 8)
            .checked_add(size)
            .ok_or("truncated WAV chunk")?;
        let body = bytes.get(offset + 8..end).ok_or("truncated WAV chunk")?;
        match id {
            b"fmt " if body.len() >= 16 => {
                format_tag = u16::from_le_bytes([body[0], body[1]]);
                channels = u16::from_le_bytes([body[2], body[3]]);
                sample_rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                bits = u16::from_le_bytes([body[14], body[15]]);
            }
            b"data" => data_len = body.len(),
            _ => {}
        }
        offset = end.saturating_add(size % 2);
    }
    if channels == 0 || sample_rate == 0 || bits == 0 {
        return Err(String::from("WAV has no usable format chunk"));
    }
    let frame_bytes = usize::from(channels) * usize::from(bits / 8).max(1);
    let frames = u64::try_from(data_len)
        .ok()
        .and_then(|len| len.checked_div(u64::try_from(frame_bytes).ok()?))
        .unwrap_or(0);
    Ok(WavInfo {
        format_tag,
        channels,
        sample_rate,
        bits,
        frames,
        seconds: frames as f64 / f64::from(sample_rate),
    })
}

#[cfg(test)]
mod info_tests {
    use super::*;

    #[test]
    fn an_exported_file_reports_its_format() {
        let audio = RenderedAudio {
            samples: vec![0.0; 48_000 * 2],
            sample_rate: 48_000,
        };
        let info = info(&audio.to_wav()).expect("info");
        assert_eq!(info.format_tag, 1);
        assert_eq!(info.channels, 2);
        assert_eq!(info.sample_rate, 48_000);
        assert_eq!(info.bits, 16);
        assert_eq!(info.frames, 48_000);
        assert!((info.seconds - 1.0).abs() < 1.0e-9);
        assert_eq!(super::info(&audio.to_wav24()).expect("24").bits, 24);
    }

    #[test]
    fn other_data_is_rejected() {
        assert!(info(b"nope").is_err());
        assert!(info(b"RIFF____WAVE").is_err());
    }

    #[test]
    fn malformed_and_mutated_wav_inputs_never_panic() {
        let valid = RenderedAudio {
            samples: vec![0.25, -0.25, 0.5, -0.5],
            sample_rate: 48_000,
        }
        .to_wav();
        for cut in 0..valid.len() {
            let _ = info(&valid[..cut]);
            let _ = decode_stereo(&valid[..cut]);
            let _ = decode_mono(&valid[..cut]);
        }
        let mut seed = 0xDA7A_1234_5678_9ABC_u64;
        for case in 0..128_usize {
            let len = (case * 13) % 160;
            let mut buf = vec![0_u8; len];
            for byte in &mut buf {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                *byte = (seed >> 56) as u8;
            }
            if buf.len() >= 12 {
                buf[..4].copy_from_slice(b"RIFF");
                buf[8..12].copy_from_slice(b"WAVE");
            }
            let _ = info(&buf);
            let _ = decode_stereo(&buf);
            let _ = decode_mono(&buf);
        }
    }
}
