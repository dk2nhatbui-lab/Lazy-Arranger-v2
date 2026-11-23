//! Builds a native style from monophonic audio analysis.

use arranger::chord_type::ArrangerChord;
use arranger::style::{Style, StyleNote};
use audio::RenderedAudio;

/// Options for turning analysed audio into a style.
#[derive(Clone, Copy, Debug)]
pub struct AudioStyleOptions<'a> {
    /// Tempo used to convert seconds to ticks.
    pub bpm: f64,
    /// Single-section name when `section_bars` is zero.
    pub section: &'a str,
    /// Source chord, or `None` to detect one from chroma.
    pub source_chord: Option<ArrangerChord>,
    /// Split notes below C3 into Bass and the rest into Chord 1.
    pub split_parts: bool,
    /// Uniform section length in 4/4 bars; zero means one section.
    pub section_bars: u32,
    /// Derive a Drums part from onsets and band energy.
    pub include_drums: bool,
}

/// A built audio style and what was detected.
#[derive(Clone, Debug)]
pub struct BuiltAudioStyle {
    /// The style.
    pub style: Style,
    /// Source chord used.
    pub source_chord: ArrangerChord,
    /// Number of detected notes placed in the style.
    pub notes: usize,
}

/// Builds a style from monophonic notes in `audio`.
///
/// # Errors
///
/// Returns a message when chord detection or section compilation fails.
pub fn style_from_audio(
    audio: &RenderedAudio,
    options: AudioStyleOptions<'_>,
) -> Result<BuiltAudioStyle, String> {
    let bpm = options.bpm.max(1.0);
    let detected = audio::analysis::to_midi(&audio.samples, audio.sample_rate);
    let seconds = audio.samples.len() as f64 / f64::from(audio.sample_rate);
    let length = audio::chords::to_ticks(seconds, bpm);
    let source = options
        .source_chord
        .or_else(|| auto_chord(&audio.samples, audio.sample_rate, bpm))
        .unwrap_or_else(|| {
            ArrangerChord::new(
                music_core::PitchClass::new(0),
                arranger::chord_type::ChordType::Major,
            )
        });
    let mut notes = Vec::new();
    for note in &detected {
        let (tick, duration) = audio::analysis::as_ticks(note, bpm);
        if duration <= 0 {
            continue;
        }
        notes.push(StyleNote {
            tick,
            duration,
            pitch: note.pitch,
            velocity: (note.level * 127.0).clamp(1.0, 127.0) as u8,
        });
    }
    let name = "Audio style";
    let mut style = if options.section_bars == 0 {
        let mut style = if options.split_parts {
            arranger::style::from_notes_split(name, source, length, notes)
        } else {
            arranger::style::from_notes(name, source, length, notes)
        };
        if let Some(first) = style.sections.first_mut() {
            first.name = options.section.to_string();
        }
        style
    } else {
        uniform_sections(
            name,
            source,
            length,
            notes,
            options.split_parts,
            options.section_bars,
        )
    };
    if options.include_drums {
        let drum_notes = drum_notes(audio, bpm);
        attach_drums(&mut style, source, drum_notes);
    }
    let notes = style
        .sections
        .iter()
        .flat_map(|section| &section.parts)
        .map(|part| part.notes.len())
        .sum();
    Ok(BuiltAudioStyle {
        style,
        source_chord: source,
        notes,
    })
}

fn drum_notes(audio: &RenderedAudio, bpm: f64) -> Vec<StyleNote> {
    let onsets = audio::drums::onsets(&audio.samples, audio.sample_rate);
    let mut notes = Vec::new();
    let window = (0.08 * f64::from(audio.sample_rate)) as usize;
    for (index, onset) in onsets.iter().enumerate() {
        let start = *onset;
        let end = (start + window).min(audio.samples.len());
        if end <= start {
            continue;
        }
        let slice = &audio.samples[start..end];
        let level =
            (slice.iter().map(|sample| sample * sample).sum::<f32>() / slice.len() as f32).sqrt();
        if level < 1.0e-4 {
            continue;
        }
        let pitch = classify_drum(slice, audio.sample_rate);
        let next = onsets
            .get(index + 1)
            .copied()
            .unwrap_or(audio.samples.len());
        let seconds = start as f64 / f64::from(audio.sample_rate);
        let duration_seconds = (next.saturating_sub(start)) as f64 / f64::from(audio.sample_rate);
        notes.push(StyleNote {
            tick: audio::chords::to_ticks(seconds, bpm),
            duration: audio::chords::to_ticks(duration_seconds.max(0.04), bpm).max(1),
            pitch,
            velocity: (level * 250.0).clamp(1.0, 127.0) as u8,
        });
    }
    notes
}

fn classify_drum(samples: &[f32], sample_rate: u32) -> u8 {
    let alpha = 1.0 - (-std::f64::consts::TAU * 200.0 / f64::from(sample_rate)).exp();
    let mut low = 0.0_f32;
    let mut low_energy = 0.0_f32;
    let mut high_energy = 0.0_f32;
    for sample in samples {
        low += alpha as f32 * (*sample - low);
        low_energy += low * low;
        high_energy += (*sample - low) * (*sample - low);
    }
    if low_energy > high_energy * 1.4 {
        36
    } else if high_energy > low_energy * 1.4 {
        42
    } else {
        38
    }
}

fn attach_drums(style: &mut Style, source: ArrangerChord, drums: Vec<StyleNote>) {
    let mut offset = 0;
    let root = source.root.as_u8();
    for section in &mut style.sections {
        let end = offset + section.length_ticks;
        let local: Vec<StyleNote> = drums
            .iter()
            .filter(|note| note.tick >= offset && note.tick < end)
            .map(|note| StyleNote {
                tick: note.tick - offset,
                duration: note.duration.min(end - note.tick),
                ..*note
            })
            .collect();
        if !local.is_empty() {
            let mut part = arranger::style::StylePart::new(
                "Drums",
                arranger::rules::ChordRule::default_for_channel(10),
            );
            part.rule.source_root = root;
            part.rule.source_chord_type = source.chord_type;
            part.notes = local;
            section.parts.push(part);
        }
        offset = end;
    }
}

fn auto_chord(samples: &[f32], sample_rate: u32, bpm: f64) -> Option<ArrangerChord> {
    let segment = (4.0 * 60.0 / bpm.max(1.0)).max(0.5);
    let chords = audio::chords::recognize(samples, sample_rate, segment);
    let mut best: Option<(music_core::Chord, f64)> = None;
    for entry in chords {
        let weight = entry.duration * entry.score;
        if best.as_ref().is_none_or(|(_, score)| weight > *score) {
            best = Some((entry.chord, weight));
        }
    }
    best.and_then(|(chord, _)| ArrangerChord::try_from(&chord).ok())
}

fn uniform_sections(
    name: &str,
    source: ArrangerChord,
    length: i64,
    notes: Vec<StyleNote>,
    split_parts: bool,
    section_bars: u32,
) -> Style {
    let section_length = i64::from(section_bars) * 4 * music_core::PPQ;
    let names = ["Main A", "Main B", "Main C", "Main D"];
    let mut merged = Style::new(name, source);
    let count = ((length + section_length - 1) / section_length).max(1);
    for index in 0..count {
        let start = index * section_length;
        let stop = (start + section_length).min(length);
        let local: Vec<StyleNote> = notes
            .iter()
            .filter(|note| note.tick >= start && note.tick < stop)
            .map(|note| StyleNote {
                tick: note.tick - start,
                duration: note.duration.min(stop - note.tick),
                ..*note
            })
            .collect();
        if local.is_empty() {
            continue;
        }
        let mut part_style = if split_parts {
            arranger::style::from_notes_split(name, source, stop - start, local)
        } else {
            arranger::style::from_notes(name, source, stop - start, local)
        };
        if let Some(section) = part_style.sections.first_mut() {
            section.name = names
                .get(index as usize)
                .copied()
                .unwrap_or("Section")
                .to_string();
            merged.sections.push(section.clone());
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_low_hit_becomes_a_drum_part() {
        let mut samples = vec![0.0_f32; 48_000];
        for (index, sample) in samples.iter_mut().skip(9_600).take(4_800).enumerate() {
            let envelope = 1.0 - index as f32 / 4_800.0;
            *sample =
                (std::f32::consts::TAU * 80.0 * index as f32 / 48_000.0).sin() * envelope * 0.8;
        }
        let audio = RenderedAudio {
            samples,
            sample_rate: 48_000,
        };
        let built = style_from_audio(
            &audio,
            AudioStyleOptions {
                bpm: 120.0,
                section: "Main A",
                source_chord: None,
                split_parts: false,
                section_bars: 0,
                include_drums: true,
            },
        )
        .expect("style");
        let drums = built
            .style
            .sections
            .first()
            .and_then(|section| section.parts.iter().find(|part| part.name == "Drums"))
            .expect("drum part");
        assert_eq!(drums.notes[0].pitch, 36);
    }

    #[test]
    fn a_tone_becomes_a_one_part_style() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../audio/tests/fixtures/tone.flac");
        let decoded = audio::decode::decode_mono(&path).expect("decode");
        let built = style_from_audio(
            &decoded,
            AudioStyleOptions {
                bpm: 120.0,
                section: "Main A",
                source_chord: None,
                split_parts: false,
                section_bars: 0,
                include_drums: false,
            },
        )
        .expect("style");
        assert_eq!(built.style.sections.len(), 1);
        assert_eq!(built.notes, 1);
    }
}
