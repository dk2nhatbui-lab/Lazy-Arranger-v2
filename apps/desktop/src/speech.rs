//! Spoken assistant replies using local offline TTS:
//! - System voice (macOS `say`, Linux `espeak-ng` / `spd-say`, Windows SAPI)
//! - `Kokoro-82M` via `sherpa-onnx-offline-tts` (Apache-2.0)
//! - `KittenTTS` via `sherpa-onnx-offline-tts` (Apache-2.0)
//! - `Piper` ONNX voices (MIT, supports all 8 UI languages)
//! - Custom `{text}` command template (shell-safe via `ARRANGER_TEXT`).

use std::path::{Path, PathBuf};

/// Built-in local offline TTS presets selectable in Settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtsPreset {
    /// Automatically prefer Kokoro -> Kitten -> Piper -> System voice.
    Auto,
    /// Native OS speech (`say` / `espeak-ng`).
    System,
    /// Kokoro-82M via `sherpa-onnx-offline-tts`.
    Kokoro,
    /// KittenTTS via `sherpa-onnx-offline-tts`.
    Kitten,
    /// Piper ONNX local voice (`piper`).
    Piper,
}

impl TtsPreset {
    /// Short label for Settings buttons.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::System => "System",
            Self::Kokoro => "Kokoro-82M",
            Self::Kitten => "KittenTTS",
            Self::Piper => "Piper",
        }
    }

    /// Setting value written to `speak_command`.
    #[must_use]
    pub const fn setting_value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::System => "",
            Self::Kokoro => "kokoro",
            Self::Kitten => "kitten",
            Self::Piper => "piper",
        }
    }
}

/// A detected local offline neural TTS engine ready to synthesize a WAV file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedTts {
    /// Preset family (`Kokoro`, `Kitten`, or `Piper`).
    pub preset: TtsPreset,
    /// Display label.
    pub label: String,
    /// Executable path.
    pub program: PathBuf,
    /// Model directory or `.onnx` voice path.
    pub model_path: PathBuf,
}

/// Cleans text for speech: no control characters and a bounded length.
#[must_use]
pub fn clean(text: &str) -> String {
    let text: String = text
        .chars()
        .filter(|character| !character.is_control())
        .take(240)
        .collect();
    // A leading dash would be read as an option by `say` or `espeak-ng`.
    text.trim_start_matches(|c: char| c == '-' || c.is_whitespace())
        .to_string()
}

/// Waits for a spawned speech process on a helper thread so it never stays a
/// zombie.
fn reap(child: std::process::Child) {
    let mut child = child;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

/// The speech command and its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechCommand {
    /// Program name.
    pub program: String,
    /// Arguments.
    pub arguments: Vec<String>,
}

/// Builds a system speech command for one utterance.
#[must_use]
pub fn command(text: &str, voice: Option<&str>) -> SpeechCommand {
    let mut arguments = Vec::new();
    if let Some(voice) = voice {
        arguments.push(String::from("-v"));
        arguments.push(voice.to_string());
    }
    arguments.push(clean(text));
    SpeechCommand {
        program: String::from("say"),
        arguments,
    }
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

/// Detects installed offline Kokoro-82M, KittenTTS, and Piper models in `bin_dirs`
/// and `model_dirs`.
#[must_use]
pub fn detect_tts_in_dirs(bin_dirs: &[PathBuf], model_dirs: &[PathBuf]) -> Vec<DetectedTts> {
    let mut found = Vec::new();
    if let Some(bin) = find_executable(&["sherpa-onnx-offline-tts"], bin_dirs) {
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
                let has_voices = sub.join("voices.bin").is_file();
                let has_tokens = sub.join("tokens.txt").is_file();
                let has_model = sub.join("model.onnx").is_file()
                    || sub.join("model.fp16.onnx").is_file()
                    || sub.join("model.int8.onnx").is_file();
                if name.contains("kokoro") && has_voices && has_tokens && has_model {
                    found.push(DetectedTts {
                        preset: TtsPreset::Kokoro,
                        label: format!("Kokoro-82M ({name})"),
                        program: bin.clone(),
                        model_path: sub,
                    });
                } else if name.contains("kitten") && has_voices && has_tokens && has_model {
                    found.push(DetectedTts {
                        preset: TtsPreset::Kitten,
                        label: format!("KittenTTS ({name})"),
                        program: bin.clone(),
                        model_path: sub,
                    });
                }
            }
        }
    }
    if let Some(bin) = find_executable(&["piper"], bin_dirs) {
        for dir in model_dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            let mut files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("onnx"))
                })
                .collect();
            files.sort();
            if let Some(voice) = files.into_iter().next() {
                let name = voice
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                found.push(DetectedTts {
                    preset: TtsPreset::Piper,
                    label: format!("Piper ({name})"),
                    program: bin.clone(),
                    model_path: voice,
                });
            }
        }
    }
    found
}

/// Detects installed offline TTS engines in standard system and app directories.
#[must_use]
pub fn detect_tts() -> Vec<DetectedTts> {
    let bin_dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    let model_dirs = vec![crate::dictate::models_dir()];
    detect_tts_in_dirs(&bin_dirs, &model_dirs)
}

/// Builds arguments for `sherpa-onnx-offline-tts` (Kokoro-82M or KittenTTS).
#[must_use]
pub fn sherpa_tts_args(engine: &DetectedTts, wav_out: &Path, text: &str) -> Vec<String> {
    let dir = &engine.model_path;
    let model = ["model.int8.onnx", "model.fp16.onnx", "model.onnx"]
        .iter()
        .map(|name| dir.join(name))
        .find(|p| p.is_file())
        .unwrap_or_else(|| dir.join("model.onnx"));
    let prefix = if engine.preset == TtsPreset::Kitten {
        "kitten"
    } else {
        "kokoro"
    };
    let mut args = vec![
        format!("--{prefix}-model={}", model.display()),
        format!("--{prefix}-voices={}", dir.join("voices.bin").display()),
        format!("--{prefix}-tokens={}", dir.join("tokens.txt").display()),
        format!(
            "--{prefix}-data-dir={}",
            dir.join("espeak-ng-data").display()
        ),
        format!("--output-filename={}", wav_out.display()),
        clean(text),
    ];
    if dir.join("dict").is_dir() {
        args.insert(
            4,
            format!("--{prefix}-dict-dir={}", dir.join("dict").display()),
        );
    }
    args
}

fn speak_neural(engine: DetectedTts, text: String) {
    std::thread::spawn(move || {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let wav = std::env::temp_dir().join(format!(
            "lazy-arranger-tts-{}-{stamp}.wav",
            std::process::id()
        ));
        let ok = match engine.preset {
            TtsPreset::Kokoro | TtsPreset::Kitten => {
                let args = sherpa_tts_args(&engine, &wav, &text);
                std::process::Command::new(&engine.program)
                    .args(&args)
                    .status()
                    .is_ok_and(|s| s.success())
            }
            TtsPreset::Piper => {
                use std::io::Write as _;
                use std::process::Stdio;
                std::process::Command::new(&engine.program)
                    .arg("--model")
                    .arg(&engine.model_path)
                    .arg("--output_file")
                    .arg(&wav)
                    .stdin(Stdio::piped())
                    .spawn()
                    .ok()
                    .is_some_and(|mut child| {
                        if let Some(mut stdin) = child.stdin.take() {
                            let _ = stdin.write_all(text.as_bytes());
                        }
                        child.wait().is_ok_and(|s| s.success())
                    })
            }
            TtsPreset::Auto | TtsPreset::System => false,
        };
        if ok && wav.is_file() {
            play_wav_file(&wav);
        }
        let _ = std::fs::remove_file(&wav);
    });
}

fn play_wav_file(wav: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("afplay").arg(wav).status();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("aplay")
            .arg("-q")
            .arg(wav)
            .status();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = wav;
    }
}

/// Speaks through a built-in preset (`auto`, `kokoro`, `kitten`, `piper`, `system`),
/// a user command template containing `{text}`, or the OS system voice.
///
/// # Errors
///
/// Returns a message when the requested engine is missing or fails to start.
pub fn speak_with(text: &str, template: &str) -> Result<(), String> {
    let text = clean(text);
    if text.trim().is_empty() {
        return Ok(());
    }
    let template = template.trim();
    if template.eq_ignore_ascii_case("auto") {
        if let Some(engine) = detect_tts().into_iter().next() {
            speak_neural(engine, text);
            return Ok(());
        }
        return speak(&text);
    }
    for (keyword, preset) in [
        ("kokoro", TtsPreset::Kokoro),
        ("kitten", TtsPreset::Kitten),
        ("piper", TtsPreset::Piper),
    ] {
        if template.eq_ignore_ascii_case(keyword) {
            let engine = detect_tts()
                .into_iter()
                .find(|e| e.preset == preset)
                .ok_or_else(|| {
                    format!(
                        "{} model/binary not found in {}",
                        preset.label(),
                        crate::dictate::models_dir().display()
                    )
                })?;
            speak_neural(engine, text);
            return Ok(());
        }
    }
    if template.is_empty() || template.eq_ignore_ascii_case("system") {
        return speak(&text);
    }
    #[cfg(unix)]
    {
        let rendered = crate::shell::render(template, "{text}", "ARRANGER_TEXT");
        std::process::Command::new("sh")
            .arg("-c")
            .arg(rendered)
            .env("ARRANGER_TEXT", &text)
            .spawn()
            .map(reap)
            .map_err(|error| format!("speech: {error}"))
    }
    #[cfg(not(unix))]
    {
        Err(String::from("custom speech commands need a Unix shell"))
    }
}

/// Speaks one utterance with the platform system voice (`say` on macOS,
/// `espeak-ng` on Linux).
///
/// # Errors
///
/// Returns a message when the command cannot be started.
pub fn speak(text: &str) -> Result<(), String> {
    let text = clean(text);
    if text.trim().is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let built = command(&text, None);
        std::process::Command::new(&built.program)
            .args(&built.arguments)
            .spawn()
            .map(reap)
            .map_err(|error| format!("speech: {error}"))
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("espeak-ng")
            .arg(&text)
            .spawn()
            .map(reap)
            .map_err(|error| format!("speech: {error}"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = text;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_cleaned_and_bounded() {
        assert_eq!(clean("hello\nworld\u{0007}"), "helloworld");
        let long = "a".repeat(400);
        assert_eq!(clean(&long).len(), 240);
    }

    #[cfg(unix)]
    #[test]
    fn speech_text_cannot_inject_shell_commands() {
        let marker = std::env::temp_dir().join(format!(
            "lazy-arranger-speech-injection-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&marker);
        let _ = speak_with(
            &format!("'; touch {}; echo '", marker.display()),
            "printf '%s' {text}",
        );
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!marker.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_custom_speech_command_is_used() {
        assert!(speak_with("hello", "true").is_ok());
        assert!(speak_with("", "false").is_ok());
    }

    #[test]
    fn detects_kokoro_kitten_and_piper_in_custom_dirs() {
        let root =
            std::env::temp_dir().join(format!("lazy-arranger-tts-detect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bins = root.join("bin");
        let models = root.join("models");
        let kokoro = models.join("kokoro-en-v0_19");
        let kitten = models.join("kitten-nano-en");
        std::fs::create_dir_all(&bins).expect("bins");
        std::fs::create_dir_all(&kokoro).expect("kokoro");
        std::fs::create_dir_all(&kitten).expect("kitten");
        for exe in ["sherpa-onnx-offline-tts", "piper"] {
            std::fs::write(
                bins.join(format!("{exe}{}", std::env::consts::EXE_SUFFIX)),
                b"",
            )
            .expect(exe);
        }
        for dir in [&kokoro, &kitten] {
            for f in ["model.onnx", "voices.bin", "tokens.txt"] {
                std::fs::write(dir.join(f), b"").expect(f);
            }
        }
        std::fs::write(models.join("vi_VN-vais1000-medium.onnx"), b"").expect("piper");
        let found = detect_tts_in_dirs(&[bins], &[models]);
        assert_eq!(found.len(), 3);
        assert!(found.iter().any(|e| e.preset == TtsPreset::Kokoro));
        assert!(found.iter().any(|e| e.preset == TtsPreset::Kitten));
        assert!(found.iter().any(|e| e.preset == TtsPreset::Piper));
        let kokoro_engine = found
            .iter()
            .find(|e| e.preset == TtsPreset::Kokoro)
            .expect("kokoro");
        let args = sherpa_tts_args(kokoro_engine, Path::new("/tmp/out.wav"), "hello");
        assert!(args.iter().any(|a| a.starts_with("--kokoro-model=")));
        assert!(args.iter().any(|a| a == "--output-filename=/tmp/out.wav"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn commands_carry_the_voice_and_text() {
        assert_eq!(
            command("hello", None),
            SpeechCommand {
                program: String::from("say"),
                arguments: vec![String::from("hello")]
            }
        );
        assert_eq!(
            command("hi", Some("Sara")).arguments,
            vec!["-v", "Sara", "hi"]
        );
        assert_eq!(command("a\nb", None).arguments, vec!["ab"]);
    }
}
