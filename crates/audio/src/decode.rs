//! Compressed audio decoding through symphonia.
//!
//! WAV keeps its dedicated decoder so 24-bit and 32-bit float takes remain
//! bit-exact; this module covers FLAC, MP3, OGG/Vorbis, AAC/M4A and AIFF. All
//! channels are averaged to mono for mixing and analysis.

use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::RenderedAudio;

/// The most samples a decode keeps: three hours of 48 kHz stereo.
const MAX_DECODED_SAMPLES: usize = 3 * 3600 * 48_000 * 2;

/// Decodes a supported audio file into mono samples at its native rate.
///
/// # Errors
///
/// Returns a message for an unsupported container/codec, a missing audio track
/// or an unreadable sample rate.
pub fn decode_mono(path: &Path) -> Result<RenderedAudio, String> {
    decode_internal(path, false)
}

/// Decodes a supported audio file into interleaved stereo at its native rate.
///
/// # Errors
///
/// Returns a message for an unsupported container/codec, a missing audio track
/// or an unreadable sample rate.
pub fn decode_stereo(path: &Path) -> Result<RenderedAudio, String> {
    decode_internal(path, true)
}

fn decode_internal(path: &Path, stereo: bool) -> Result<RenderedAudio, String> {
    let file = File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    let mut reader = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| format!("probe {}: {error}", path.display()))?;
    let track = reader
        .default_track(TrackType::Audio)
        .ok_or_else(|| format!("{}: no audio track", path.display()))?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .cloned()
        .ok_or_else(|| format!("{}: no audio codec parameters", path.display()))?;
    let track_id = track.id;
    let declared = track.num_frames;
    let sample_rate = params
        .sample_rate
        .ok_or_else(|| format!("{}: no sample rate", path.display()))?;
    if sample_rate == 0 {
        return Err(format!("{}: zero sample rate", path.display()));
    }
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let mut samples = Vec::new();
    let mut planes: Vec<Vec<f32>> = Vec::new();
    loop {
        let packet = match reader.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::ResetRequired) => break,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buffer) => {
                // A tiny compressed file can expand to gigabytes of silence.
                if samples.len() > MAX_DECODED_SAMPLES {
                    return Err(format!("{}: audio is too long to load", path.display()));
                }
                buffer.copy_to_vecs_planar::<f32>(&mut planes);
                let frames = planes.first().map_or(0, Vec::len);
                if stereo {
                    for frame in 0..frames {
                        let left = planes
                            .first()
                            .map_or(0.0, |plane| plane.get(frame).copied().unwrap_or(0.0));
                        let right = planes
                            .get(1)
                            .map_or(left, |plane| plane.get(frame).copied().unwrap_or(left));
                        samples.push(crate::spsc::sanitize(left));
                        samples.push(crate::spsc::sanitize(right));
                    }
                } else {
                    for frame in 0..frames {
                        let mut sum = 0.0_f32;
                        for plane in &planes {
                            sum += plane.get(frame).copied().unwrap_or(0.0);
                        }
                        samples.push(crate::spsc::sanitize(sum / planes.len().max(1) as f32));
                    }
                }
            }
            Err(SymphoniaError::DecodeError(_)) | Err(SymphoniaError::IoError(_)) => continue,
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
    }
    if let Some(declared) = declared {
        let channels = if stereo { 2 } else { 1 };
        samples.truncate(
            usize::try_from(declared)
                .unwrap_or(usize::MAX)
                .saturating_mul(channels),
        );
    }
    if samples.is_empty() {
        return Err(format!("{}: decoded no samples", path.display()));
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
    fn compressed_fixtures_decode_to_stereo_samples() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone.flac");
        let audio = decode_stereo(&path).expect("stereo decode");
        assert_eq!(audio.sample_rate, 48_000);
        assert!(audio.samples.len() > 16_000);
        assert_eq!(audio.samples.len() % 2, 0);
        assert!(audio.samples.iter().any(|sample| sample.abs() > 0.1));
    }

    #[test]
    fn compressed_fixtures_decode_to_mono_samples() {
        for name in ["tone.flac", "tone.mp3", "tone.m4a"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name);
            let audio = decode_mono(&path).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(audio.sample_rate, 48_000, "{name}");
            assert!(audio.samples.len() > 8_000, "{name}");
            assert!(
                audio.samples.iter().any(|sample| sample.abs() > 0.1),
                "{name}"
            );
        }
    }
}
