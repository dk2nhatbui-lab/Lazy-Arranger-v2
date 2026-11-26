//! Mixing audio references into a rendered buffer.
//!
//! An audio track stores a path and a tick span. This module reads WAV files,
//! resamples them linearly to the render rate, places them at the tempo-derived
//! start and mixes them under the synth. Missing or unreadable files are
//! reported, never silent.

use std::path::Path;

use project::TrackId;
use project::song::{AudioRef, Song};

use crate::{RenderedAudio, wav};

/// One audio reference that could not be mixed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioProblem {
    /// Track holding the reference.
    pub track: TrackId,
    /// Path as stored.
    pub path: String,
    /// Why it was skipped.
    pub reason: String,
}

/// Reports unreadable audio references without changing a buffer.
#[must_use]
pub fn check_audio_references(song: &Song) -> Vec<AudioProblem> {
    let mut problems = Vec::new();
    for track in song.tracks.iter().filter(|track| song.is_audible(track)) {
        let Some(reference) = &track.audio else {
            continue;
        };
        let path = Path::new(&reference.path);
        let result: Result<(), String> = match std::fs::read(path) {
            Ok(bytes) => {
                if bytes.starts_with(b"RIFF") {
                    wav::decode_stereo(&bytes).map(|_| ())
                } else {
                    crate::decode::decode_mono(path).map(|_| ())
                }
            }
            Err(error) => Err(format!("could not read audio: {error}")),
        };
        if let Err(reason) = result {
            problems.push(AudioProblem {
                track: track.id,
                path: reference.path.clone(),
                reason,
            });
        }
    }
    problems
}

/// Standalone legacy mixer for a buffer that has no audio references yet.
///
/// `render_with_soundfont` already mixes references, so callers must not pass
/// its output here. Use [`check_audio_references`] only to report bad files.
/// Returns the problems found; the mix still contains everything that worked.
#[must_use]
pub fn mix_audio_references(
    rendered: &mut RenderedAudio,
    song: &Song,
    gain: f32,
) -> Vec<AudioProblem> {
    let mut problems = Vec::new();
    for track in song.tracks.iter().filter(|track| song.is_audible(track)) {
        let Some(reference) = &track.audio else {
            continue;
        };
        let (left_gain, right_gain) = crate::gains_at(track, reference.start);
        match mix_one(rendered, song, reference, gain, (left_gain, right_gain)) {
            Ok(()) => {}
            Err(reason) => problems.push(AudioProblem {
                track: track.id,
                path: reference.path.clone(),
                reason,
            }),
        }
    }
    for sample in &mut rendered.samples {
        *sample = crate::spsc::sanitize(*sample);
    }
    problems
}

fn mix_one(
    rendered: &mut RenderedAudio,
    song: &Song,
    reference: &AudioRef,
    gain: f32,
    (left_gain, right_gain): (f32, f32),
) -> Result<(), String> {
    let bytes = std::fs::read(Path::new(&reference.path))
        .map_err(|error| format!("could not read audio: {error}"))?;
    let decoded = if bytes.starts_with(b"RIFF") {
        wav::decode_mono(&bytes)?
    } else {
        crate::decode::decode_mono(Path::new(&reference.path))?
    };
    if decoded.sample_rate == 0 || decoded.samples.is_empty() {
        return Err(String::from("audio has no samples"));
    }
    let rate = rendered.sample_rate;
    if rate == 0 {
        return Err(String::from("render has no sample rate"));
    }
    let start_seconds = song.tempo_map.seconds_at(reference.start);
    let in_point = song.tempo_map.duration_seconds(0, reference.offset);
    let length_seconds = if reference.length > 0 {
        song.tempo_map.duration_seconds(
            reference.start,
            reference.start.saturating_add(reference.length),
        )
    } else {
        decoded.samples.len() as f64 / f64::from(decoded.sample_rate)
    };
    let start = (start_seconds * f64::from(rate)).round() as usize;
    let frames = (length_seconds * f64::from(rate)).round() as usize;
    let ratio = f64::from(decoded.sample_rate) / f64::from(rate);
    let in_frames = (in_point * f64::from(decoded.sample_rate)).round() as usize;
    let fade_in_seconds = if reference.fade_in > 0 {
        song.tempo_map.duration_seconds(
            reference.start,
            reference.start.saturating_add(reference.fade_in),
        )
    } else {
        0.0
    };
    // The fade-out sits at the end of the clip, so measure it there.
    let fade_out_seconds = if reference.fade_out > 0 && reference.length > 0 {
        let end = reference.start.saturating_add(reference.length);
        song.tempo_map.duration_seconds(
            end.saturating_sub(reference.fade_out.min(reference.length)),
            end,
        )
    } else if reference.fade_out > 0 {
        song.tempo_map.duration_seconds(
            reference.start,
            reference.start.saturating_add(reference.fade_out),
        )
    } else {
        0.0
    };
    let fade_in_frames = (fade_in_seconds * f64::from(rate)).max(0.0);
    let fade_out_frames = (fade_out_seconds * f64::from(rate)).max(0.0);
    let stretch_step = if reference.stretch && frames > 0 {
        decoded.samples.len().saturating_sub(in_frames) as f64 / frames as f64
    } else {
        ratio
    };
    let clip_gain = 10.0_f32.powf(f32::from(reference.gain_mb) / 20_000.0);
    for frame in 0..frames {
        let target = start + frame;
        if target * 2 + 1 >= rendered.samples.len() {
            break;
        }
        let source = in_frames + (frame as f64 * stretch_step) as usize;
        if source >= decoded.samples.len() {
            break;
        }
        let mut envelope = 1.0_f64;
        if fade_in_frames > 0.0 {
            envelope *= (frame as f64 / fade_in_frames).clamp(0.0, 1.0);
        }
        if fade_out_frames > 0.0 {
            envelope *= ((frames.saturating_sub(frame)) as f64 / fade_out_frames).clamp(0.0, 1.0);
        }
        let value = decoded.samples[source] * gain * clip_gain * envelope as f32;
        rendered.samples[target * 2] += value * left_gain;
        rendered.samples[target * 2 + 1] += value * right_gain;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::TrackId;
    use project::song::{AudioRef, Track};

    fn write_wav(name: &str, samples: Vec<f32>) -> String {
        let audio = RenderedAudio {
            samples,
            sample_rate: 48_000,
        };
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, audio.to_wav()).expect("write");
        path.display().to_string()
    }

    fn song_with(path: String, muted: bool) -> Song {
        let mut song = Song::new("Mix", 120.0);
        let mut track = Track::new(TrackId::new(1).expect("valid"), "Take");
        track.audio = Some(AudioRef {
            path,
            start: 0,
            length: 1920,
            offset: 0,
            fade_in: 0,
            fade_out: 0,

            stretch: false,
            gain_mb: 0,
        });
        track.muted = muted;
        song.tracks.push(track);
        song
    }

    #[test]
    fn a_missing_reference_is_reported_without_mixing() {
        let mut song = song_with(String::from("/no/such/lazy-arranger.wav"), false);
        song.tracks[0].id = TrackId::new(1).expect("track");
        let problems = check_audio_references(&song);
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].track.get(), 1);
    }

    #[test]
    fn a_reference_is_mixed_at_its_start() {
        let path = write_wav("lazy-arranger-mix-a.wav", vec![0.25; 48_000 * 2]);
        let song = song_with(path.clone(), false);
        let mut rendered = RenderedAudio {
            samples: vec![0.0; 96_000 * 2],
            sample_rate: 48_000,
        };
        let problems = mix_audio_references(&mut rendered, &song, 0.5);
        assert!(problems.is_empty());
        assert!(rendered.samples[0] > 0.1);
        let silent_later = rendered.samples[(48_000 + 10) * 2];
        assert!(silent_later.abs() < 1.0e-6);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_flac_reference_mixes_like_a_wav() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone.flac");
        let mut song = song_with(path.display().to_string(), false);
        song.tracks[0].audio = Some(AudioRef {
            path: path.display().to_string(),
            start: 0,
            length: 9600,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: false,
            gain_mb: 0,
        });
        let mut rendered = RenderedAudio {
            samples: vec![0.0; 96_000 * 2],
            sample_rate: 48_000,
        };
        let problems = mix_audio_references(&mut rendered, &song, 1.0);
        assert!(problems.is_empty(), "{problems:?}");
        assert!(rendered.samples.iter().any(|sample| sample.abs() > 0.1));
    }

    #[test]
    fn the_in_point_skips_into_the_file() {
        let mut samples = vec![0.3_f32; 48_000 * 2];
        samples.extend(vec![-0.3_f32; 48_000 * 2]);
        let path = write_wav("lazy-arranger-mix-offset.wav", samples);
        let mut song = song_with(path.clone(), false);
        song.tracks[0].audio = Some(AudioRef {
            path: path.clone(),
            start: 0,
            length: 960,
            offset: 1920,
            fade_in: 0,
            fade_out: 0,

            stretch: false,
            gain_mb: 0,
        });
        let mut rendered = RenderedAudio {
            samples: vec![0.0; 96_000 * 2],
            sample_rate: 48_000,
        };
        let problems = mix_audio_references(&mut rendered, &song, 1.0);
        assert!(problems.is_empty());
        assert!(
            rendered.samples[0] < 0.0,
            "the in-point starts in the quiet half"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn fades_ramp_the_reference_edges() {
        let path = write_wav("lazy-arranger-mix-fade.wav", vec![0.4; 48_000 * 2]);
        let mut song = song_with(path.clone(), false);
        song.tracks[0].audio = Some(AudioRef {
            path: path.clone(),
            start: 0,
            length: 1920,
            offset: 0,
            fade_in: 960,
            fade_out: 480,

            stretch: false,
            gain_mb: 0,
        });
        let mut rendered = RenderedAudio {
            samples: vec![0.0; 96_000 * 2],
            sample_rate: 48_000,
        };
        assert!(mix_audio_references(&mut rendered, &song, 1.0).is_empty());
        let at = |frame: usize| rendered.samples[frame * 2].abs();
        assert!(at(0) < 0.01, "fade-in starts near zero");
        assert!(at(24_000) > 0.2, "middle is full");
        assert!(at(47_000) < 0.05, "fade-out ends near zero");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stretching_resamples_the_source_to_the_reference_length() {
        let mut samples = vec![0.3_f32; 96_000 * 2];
        samples.extend(vec![-0.3_f32; 96_000 * 2]);
        let path = write_wav("lazy-arranger-mix-stretch.wav", samples);
        let mut song = song_with(path.clone(), false);
        song.tracks[0].audio = Some(AudioRef {
            path: path.clone(),
            start: 0,
            length: 1920,
            offset: 0,
            fade_in: 0,
            fade_out: 0,
            stretch: true,
            gain_mb: 0,
        });
        let mut rendered = RenderedAudio {
            samples: vec![0.0; 96_000 * 2],
            sample_rate: 48_000,
        };
        assert!(mix_audio_references(&mut rendered, &song, 1.0).is_empty());
        assert!(
            rendered.samples[12_000 * 2] > 0.1,
            "first quarter is positive"
        );
        assert!(
            rendered.samples[36_000 * 2] < -0.1,
            "stretching reaches the negative half by 0.75 s"
        );
        song.tracks[0].audio.as_mut().expect("audio").stretch = false;
        let mut plain = RenderedAudio {
            samples: vec![0.0; 96_000 * 2],
            sample_rate: 48_000,
        };
        assert!(mix_audio_references(&mut plain, &song, 1.0).is_empty());
        assert!(
            plain.samples[36_000 * 2] > 0.1,
            "plain playback is still positive"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn muted_tracks_and_missing_files_are_handled() {
        let path = write_wav("lazy-arranger-mix-b.wav", vec![0.2; 48_000 * 2]);
        let muted = song_with(path.clone(), true);
        let mut rendered = RenderedAudio {
            samples: vec![0.0; 96_000 * 2],
            sample_rate: 48_000,
        };
        assert!(mix_audio_references(&mut rendered, &muted, 0.5).is_empty());
        assert!(rendered.samples.iter().all(|sample| sample.abs() < 1.0e-9));

        let missing = song_with(String::from("/tmp/does-not-exist-lazy.wav"), false);
        let problems = mix_audio_references(&mut rendered, &missing, 0.5);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].reason.contains("could not read"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_different_rate_is_resampled() {
        let path = write_wav("lazy-arranger-mix-c.wav", vec![0.3; 24_000 * 2]);
        let song = song_with(path.clone(), false);
        let mut rendered = RenderedAudio {
            samples: vec![0.0; 96_000 * 2],
            sample_rate: 48_000,
        };
        let problems = mix_audio_references(&mut rendered, &song, 1.0);
        assert!(problems.is_empty());
        assert!(
            rendered.samples[20_000 * 2] > 0.1,
            "resampled audio reaches later frames"
        );
        let _ = std::fs::remove_file(path);
    }
}
