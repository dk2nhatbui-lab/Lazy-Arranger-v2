//! Spoken assistant replies with the macOS `say` command.
//!
//! Speech is optional and local: text is cleaned and passed to `say`, and a
//! host without the command simply reports the failure. Speech input (Whisper,
//! Kitten) is not implemented.

/// Cleans text for speech: no control characters and a bounded length.
#[must_use]
pub fn clean(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .take(240)
        .collect()
}

/// The speech command and its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechCommand {
    /// Program name.
    pub program: String,
    /// Arguments.
    pub arguments: Vec<String>,
}

/// Builds a command for one utterance.
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

/// Speaks one utterance on macOS; does nothing elsewhere.
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
            .map(|_| ())
            .map_err(|error| format!("speech: {error}"))
    }
    #[cfg(not(target_os = "macos"))]
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
