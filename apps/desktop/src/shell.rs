//! Safe substitution for user-configured shell command templates.
//!
//! Placeholder text is passed through the environment and replaced by a quoted
//! variable, so quotes or shell metacharacters in AI or speech output cannot
//! escape into the command line.

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

#[cfg(test)]
mod tests {
    use super::*;

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
