//! Safe substitution for user-configured shell command templates.
//!
//! On Unix the template runs through `sh -c`. Placeholder text is passed
//! through the environment and replaced by a quoted variable, so quotes or
//! shell metacharacters in AI or speech output cannot escape into the command
//! line. Windows has no `sh`: there the template is split into words and run
//! directly, with the text as one literal argument, so no shell parses it.

/// Replaces one placeholder with a quoted environment-variable expansion.
///
/// A template without the placeholder gets the expansion as an extra argument.
#[must_use]
pub fn render(template: &str, placeholder: &str, variable: &str) -> String {
    let replacement = format!("\"${{{variable}}}\"");
    if template.contains(placeholder) {
        template.replace(placeholder, &replacement)
    } else {
        format!("{template} {replacement}")
    }
}

/// Quotes `text` as one single-quoted `sh` word.
#[must_use]
pub fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The process that runs `template` with `value` in place of `placeholder`.
///
/// # Errors
///
/// Returns a message when the template names no program.
pub fn command(
    template: &str,
    placeholder: &str,
    variable: &str,
    value: impl AsRef<std::ffi::OsStr>,
) -> Result<std::process::Command, String> {
    if cfg!(windows) {
        let value = value.as_ref().to_string_lossy().into_owned();
        let words = arguments(template, placeholder, &value);
        let (program, rest) = words
            .split_first()
            .ok_or_else(|| String::from("the command names no program"))?;
        let mut command = std::process::Command::new(program);
        command.args(rest);
        Ok(command)
    } else {
        let mut command = std::process::Command::new("sh");
        command
            .arg("-c")
            .arg(render(template, placeholder, variable))
            .env(variable, value);
        Ok(command)
    }
}

/// Splits `template` into words (double or single quotes group a word) and
/// puts `value` where `placeholder` is, or adds it as the last word.
#[must_use]
pub fn arguments(template: &str, placeholder: &str, value: &str) -> Vec<String> {
    let mut words = split_words(template);
    let mut used = false;
    for word in &mut words {
        if word.contains(placeholder) {
            *word = word.replace(placeholder, value);
            used = true;
        }
    }
    if !used {
        words.push(value.to_string());
    }
    words
}

fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    for character in text.chars() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => word.push(character),
            None if character == '"' || character == '\'' => {
                quote = Some(character);
                started = true;
            }
            None if character.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            None => {
                word.push(character);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_keeps_spaces_and_quotes_in_one_word() {
        assert_eq!(quote("/a b/c"), "'/a b/c'");
        assert_eq!(quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn windows_commands_take_the_text_as_one_literal_argument() {
        assert_eq!(
            arguments(
                r#""C:\Program Files\llm\llm.exe" -m 'my model' {prompt}"#,
                "{prompt}",
                "a & b \"quoted\" | c"
            ),
            [
                r"C:\Program Files\llm\llm.exe",
                "-m",
                "my model",
                "a & b \"quoted\" | c"
            ]
        );
        assert_eq!(
            arguments("ollama run llama3", "{prompt}", "hi"),
            ["ollama", "run", "llama3", "hi"]
        );
        assert_eq!(
            arguments("tool --text={prompt}", "{prompt}", "x y"),
            ["tool", "--text=x y"]
        );
        assert!(arguments("", "{prompt}", "hi") == ["hi"]);
    }

    #[test]
    fn placeholders_become_quoted_variables() {
        assert_eq!(
            render("tool {text}", "{text}", "ARRANGER_TEXT"),
            "tool \"${ARRANGER_TEXT}\""
        );
        assert_eq!(
            render("tool '{text}'", "{text}", "ARRANGER_TEXT"),
            "tool '\"${ARRANGER_TEXT}\"'"
        );
        assert_eq!(
            render("tool", "{text}", "ARRANGER_TEXT"),
            "tool \"${ARRANGER_TEXT}\""
        );
    }
}
