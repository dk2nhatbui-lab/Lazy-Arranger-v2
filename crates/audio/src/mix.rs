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

/// Mixes every audible track's audio reference into `rendered`.
///
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
        let (left_gain, right_gain) = crate::track_gains(track);
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
        *sample = sample.clamp(-1.0, 1.0);
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
    let decoded = wav::decode_mono(&bytes)?;
    if decoded.sample_rate == 0 || decoded.samples.is_empty() {
        return Err(String::from("audio has no samples"));
    }
    let rate = rendered.sample_rate;
    let start_seconds = song.tempo_map.seconds_at(reference.start);
    let length_seconds = if reference.length > 0 {
        song.tempo_map
            .duration_seconds(reference.start, reference.start + reference.length)
    } else {
        decoded.samples.len() as f64 / f64::from(decoded.sample_rate)
    };
    let start = (start_seconds * f64::from(rate)).round() as usize;
    let frames = (length_seconds * f64::from(rate)).round() as usize;
    let ratio = f64::from(decoded.sample_rate) / f64::from(rate);
    for frame in 0..frames {
        let target = start + frame;
        if target * 2 + 1 >= rendered.samples.len() {
            break;
        }
        let source = (frame as f64 * ratio) as usize;
        if source >= decoded.samples.len() {
            break;
        }
        let value = decoded.samples[source] * gain;
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
        });
        track.muted = muted;
        song.tracks.push(track);
        song
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
