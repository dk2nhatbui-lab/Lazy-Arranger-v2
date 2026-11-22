//! Minimal WAV decoding for the audio-to-MIDI path (16-bit PCM, any channels).

use crate::RenderedAudio;

/// Decodes 16-bit PCM WAV into mono samples.
///
/// # Errors
///
/// Returns a message for another format or a truncated file.
pub fn decode_mono(bytes: &[u8]) -> Result<RenderedAudio, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(String::from("not a RIFF/WAVE file"));
    }
    let mut offset = 12;
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
        let body = bytes
            .get(offset + 8..offset + 8 + size)
            .ok_or("truncated WAV chunk")?;
        match id {
            b"fmt " if body.len() >= 16 => {
                channels = u16::from_le_bytes([body[2], body[3]]);
                sample_rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                bits = u16::from_le_bytes([body[14], body[15]]);
            }
            b"data" => data = Some(body),
            _ => {}
        }
        offset += 8 + size + size % 2;
    }
    if channels == 0 || sample_rate == 0 || bits != 16 {
        return Err(String::from("unsupported WAV format"));
    }
    let data = data.ok_or("no data chunk")?;
    let frame = usize::from(channels) * 2;
    let mut samples = Vec::with_capacity(data.len() / frame);
    for chunk in data.chunks_exact(frame) {
        let mut sum = 0.0_f32;
        for channel in 0..usize::from(channels) {
            let value = i16::from_le_bytes([chunk[channel * 2], chunk[channel * 2 + 1]]);
            sum += f32::from(value) / f32::from(i16::MAX);
        }
        samples.push(sum / f32::from(channels));
    }
    Ok(RenderedAudio {
        samples,
        sample_rate,
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

    #[test]
    fn other_files_are_rejected() {
        assert!(decode_mono(b"nope").is_err());
    }
}

/// What a WAV header declares.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WavInfo {
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
        let body = bytes
            .get(offset + 8..offset + 8 + size)
            .ok_or("truncated WAV chunk")?;
        match id {
            b"fmt " if body.len() >= 16 => {
                channels = u16::from_le_bytes([body[2], body[3]]);
                sample_rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                bits = u16::from_le_bytes([body[14], body[15]]);
            }
            b"data" => data_len = body.len(),
            _ => {}
        }
        offset += 8 + size + size % 2;
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
}
