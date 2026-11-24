//! Local offline microphone dictation via `whisper.cpp`, `sherpa-onnx`
//! (Whisper / Moonshine), or a user-supplied `{file}` command template.

use std::path::{Path, PathBuf};

/// Target sample rate required by `whisper.cpp` and `sherpa-onnx` models.
pub const STT_SAMPLE_RATE: u32 = 16_000;

/// Built-in local STT preset templates for the Settings UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SttPreset {
    /// Auto-detect an installed `whisper-cli` or `sherpa-onnx-offline` model.
    Auto,
    /// `whisper.cpp` CLI (`whisper-cli`, MIT).
    WhisperCpp,
    /// `sherpa-onnx-offline` with Moonshine Tiny/Base ONNX (MIT / Apache-2.0).
    SherpaMoonshine,
}

impl SttPreset {
    /// Short label for Settings buttons.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::WhisperCpp => "whisper.cpp",
            Self::SherpaMoonshine => "Moonshine",
        }
    }

    /// Suggested command template for this preset.
    #[must_use]
    pub fn template(self, models_dir: &Path) -> String {
        match self {
            Self::Auto => String::from("auto"),
            Self::WhisperCpp => format!(
                "whisper-cli -m {}/ggml-base.bin -f {{file}} -nt",
                models_dir.display()
            ),
            Self::SherpaMoonshine => format!(
                "sherpa-onnx-offline --moonshine-preprocessor={d}/moonshine-tiny/preprocess.onnx \
                 --moonshine-encoder={d}/moonshine-tiny/encode.int8.onnx \
                 --moonshine-uncached-decoder={d}/moonshine-tiny/uncached_decode.int8.onnx \
                 --moonshine-cached-decoder={d}/moonshine-tiny/cached_decode.int8.onnx \
                 --tokens={d}/moonshine-tiny/tokens.txt {{file}}",
                d = models_dir.display()
            ),
        }
    }
}

/// A detected local offline STT binary + model ready to run without a shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedStt {
    /// Human-readable name (e.g. `"whisper.cpp (ggml-base.bin)"`).
    pub label: String,
    /// Executable path.
    pub program: PathBuf,
    /// Arguments before the WAV file path.
    pub args_before: Vec<String>,
    /// Arguments after the WAV file path.
    pub args_after: Vec<String>,
}

/// Default directory for optional user-installed offline speech models.
#[must_use]
pub fn models_dir() -> PathBuf {
    crate::settings::Settings::data_dir().join("models")
}

/// Converts interleaved stereo `f32` audio at `source_rate` into a 16 kHz mono
/// 16-bit PCM WAV byte vector suitable for `whisper.cpp` and `sherpa-onnx`.
#[must_use]
pub fn encode_16khz_mono_wav(interleaved_stereo: &[f32], source_rate: u32) -> Vec<u8> {
    let source_rate = source_rate.max(1);
    let source_frames = interleaved_stereo.len() / 2;
    let out_frames = if source_frames == 0 {
        0
    } else {
        ((source_frames as u64 * u64::from(STT_SAMPLE_RATE)) / u64::from(source_rate)).max(1)
            as usize
    };
    let mut pcm = Vec::with_capacity(out_frames * 2);
    let ratio = f64::from(source_rate) / f64::from(STT_SAMPLE_RATE);
    for idx in 0..out_frames {
        let pos = idx as f64 * ratio;
        let f0 = (pos.floor() as usize).min(source_frames.saturating_sub(1));
        let f1 = (f0 + 1).min(source_frames.saturating_sub(1));
        let frac = (pos - f0 as f64) as f32;
        let m0 = 0.5 * (interleaved_stereo[f0 * 2] + interleaved_stereo[f0 * 2 + 1]);
        let m1 = 0.5 * (interleaved_stereo[f1 * 2] + interleaved_stereo[f1 * 2 + 1]);
        let sample = (m0 + (m1 - m0) * frac).clamp(-1.0, 1.0);
        let scaled = (sample * f32::from(i16::MAX)).round() as i16;
        pcm.extend_from_slice(&scaled.to_le_bytes());
    }
    let data_len = pcm.len() as u32;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1_u16.to_le_bytes()); // Mono
    wav.extend_from_slice(&STT_SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(STT_SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    wav.extend_from_slice(&2_u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16_u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&pcm);
    wav
}

/// Records a short microphone take as a temporary 16 kHz mono WAV file.
///
/// # Errors
///
/// Returns a message when the input cannot be recorded or written.
pub fn record_to_wav(seconds: f64) -> Result<PathBuf, String> {
    let audio = audio::record::record_seconds(seconds)?;
    let wav = encode_16khz_mono_wav(&audio.samples, audio.sample_rate);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "lazy-arranger-dictate-{}-{stamp}.wav",
        std::process::id()
    ));
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        file.write_all(&wav)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, wav).map_err(|error| error.to_string())?;
    Ok(path)
}

fn find_executable(names: &[&str], extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = extra_dirs.to_vec();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    for dir in dirs {
        for name in names {
            let candidate = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn find_first_matching_file(dirs: &[PathBuf], prefixes: &[&str], ext: &str) -> Option<PathBuf> {
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        for prefix in prefixes {
            if let Some(found) = files.iter().find(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(prefix) && n.ends_with(ext))
            }) {
                return Some(found.clone());
            }
        }
    }
    None
}

/// Detects installed offline STT binaries and models inside `bin_dirs` and `model_dirs`.
#[must_use]
pub fn detect_stt_in_dirs(bin_dirs: &[PathBuf], model_dirs: &[PathBuf]) -> Vec<DetectedStt> {
    let mut found = Vec::new();
    if let Some(bin) = find_executable(&["whisper-cli", "whisper-cpp"], bin_dirs)
        && let Some(model) = find_first_matching_file(
            model_dirs,
            &[
                "ggml-base",
                "ggml-small",
                "ggml-tiny",
                "ggml-large-v3-turbo",
                "ggml-",
            ],
            ".bin",
        )
    {
        let model_name = model
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        found.push(DetectedStt {
            label: format!("whisper.cpp ({model_name})"),
            program: bin,
            args_before: vec![
                String::from("-m"),
                model.display().to_string(),
                String::from("-nt"),
                String::from("-f"),
            ],
            args_after: Vec::new(),
        });
    }
    if let Some(bin) = find_executable(&["sherpa-onnx-offline"], bin_dirs) {
        for dir in model_dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            let mut subdirs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            subdirs.sort();
            for sub in subdirs {
                let name = sub
                    .file_name()
                    .map(|n| n.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                let tokens = sub.join("tokens.txt");
                if name.contains("moonshine") && tokens.is_file() {
                    let pre = sub.join("preprocess.onnx");
                    let enc = pick_onnx(&sub, &["encode.int8.onnx", "encode.onnx"]);
                    let unc =
                        pick_onnx(&sub, &["uncached_decode.int8.onnx", "uncached_decode.onnx"]);
                    let cac = pick_onnx(&sub, &["cached_decode.int8.onnx", "cached_decode.onnx"]);
                    if pre.is_file()
                        && let (Some(enc), Some(unc), Some(cac)) = (enc, unc, cac)
                    {
                        found.push(DetectedStt {
                            label: format!("sherpa-onnx Moonshine ({name})"),
                            program: bin.clone(),
                            args_before: vec![
                                format!("--moonshine-preprocessor={}", pre.display()),
                                format!("--moonshine-encoder={}", enc.display()),
                                format!("--moonshine-uncached-decoder={}", unc.display()),
                                format!("--moonshine-cached-decoder={}", cac.display()),
                                format!("--tokens={}", tokens.display()),
                            ],
                            args_after: Vec::new(),
                        });
                    }
                }
            }
        }
    }
    found
}

fn pick_onnx(dir: &Path, names: &[&str]) -> Option<PathBuf> {
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
}

/// Detects installed offline STT engines in standard system and app directories.
#[must_use]
pub fn detect_stt() -> Vec<DetectedStt> {
    let bin_dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    let model_dirs = vec![
        models_dir(),
        PathBuf::from("/opt/homebrew/share/whisper-cpp"),
        PathBuf::from("/usr/local/share/whisper-cpp"),
    ];
    detect_stt_in_dirs(&bin_dirs, &model_dirs)
}

/// Runs either an auto-detected local offline STT engine (when `command` is empty
/// or `"auto"`) or a user-supplied `{file}` command template.
///
/// # Errors
///
/// Returns a message when no engine is configured/installed, the command fails,
/// or stdout contains no text.
pub fn transcribe(wav: &Path, command: &str) -> Result<String, String> {
    let command = command.trim();
    if command.is_empty() || command.eq_ignore_ascii_case("auto") {
        let Some(engine) = detect_stt().into_iter().next() else {
            return Err(format!(
                "no offline STT engine found; install whisper-cpp or sherpa-onnx and place models in {}",
                models_dir().display()
            ));
        };
        let mut cmd = std::process::Command::new(&engine.program);
        cmd.args(&engine.args_before)
            .arg(wav)
            .args(&engine.args_after);
        let output = cmd
            .output()
            .map_err(|error| format!("{}: {error}", engine.label))?;
        if !output.status.success() {
            return Err(format!(
                "{}: {}",
                engine.label,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            return Err(format!("{} produced no text", engine.label));
        }
        return Ok(text);
    }
    let rendered = crate::shell::render(command, "{file}", "ARRANGER_FILE");
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&rendered)
        .env("ARRANGER_FILE", wav)
        .output()
        .map_err(|error| format!("speech command: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "speech command: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(String::from("speech command printed no text"));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_16khz_mono_wav_from_48khz_stereo() {
        // 48000 stereo frames = 1.0 s -> 16000 mono frames = 32000 data bytes + 44 header
        let mut stereo = Vec::with_capacity(48_000 * 2);
        for _ in 0..48_000 {
            stereo.push(0.5);
            stereo.push(0.5);
        }
        let wav = encode_16khz_mono_wav(&stereo, 48_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1); // mono
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            16_000
        );
        assert_eq!(wav.len(), 44 + 16_000 * 2);
        let first_sample = i16::from_le_bytes([wav[44], wav[45]]);
        assert!((first_sample - 16384).abs() <= 2);
    }

    #[test]
    fn detects_whisper_cpp_and_moonshine_models_in_custom_dirs() {
        let root =
            std::env::temp_dir().join(format!("lazy-arranger-stt-detect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bins = root.join("bin");
        let models = root.join("models");
        let moon = models.join("moonshine-tiny");
        std::fs::create_dir_all(&bins).expect("bins");
        std::fs::create_dir_all(&moon).expect("moon");
        std::fs::write(
            bins.join(format!("whisper-cli{}", std::env::consts::EXE_SUFFIX)),
            b"",
        )
        .expect("whisper");
        std::fs::write(
            bins.join(format!(
                "sherpa-onnx-offline{}",
                std::env::consts::EXE_SUFFIX
            )),
            b"",
        )
        .expect("sherpa");
        std::fs::write(models.join("ggml-base.bin"), b"").expect("model");
        for name in [
            "preprocess.onnx",
            "encode.int8.onnx",
            "uncached_decode.int8.onnx",
            "cached_decode.int8.onnx",
            "tokens.txt",
        ] {
            std::fs::write(moon.join(name), b"").expect(name);
        }
        let detected = detect_stt_in_dirs(&[bins], &[models]);
        assert_eq!(detected.len(), 2);
        assert!(detected[0].label.contains("whisper.cpp"));
        assert!(detected[1].label.contains("Moonshine"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_command_template_returns_stdout() {
        let wav = std::env::temp_dir().join("lazy-arranger-dictate-command.wav");
        std::fs::write(&wav, b"RIFF").expect("wav");
        let text = transcribe(&wav, "printf '  hello world  '").expect("text");
        assert_eq!(text, "hello world");
        assert!(transcribe(&wav, "printf ''").is_err());
        let _ = std::fs::remove_file(wav);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_placeholder_is_quoted() {
        let wav = std::env::temp_dir().join("lazy-arranger-dictate-q.wav");
        std::fs::write(&wav, b"RIFF").expect("wav");
        let text = transcribe(&wav, "test -f {file} && echo present").expect("present");
        assert_eq!(text, "present");
        let _ = std::fs::remove_file(wav);
    }
}
