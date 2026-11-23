//! A small release check against a GitHub-style `releases/latest` endpoint.
//!
//! The check only reports; it never downloads or runs an installer.

use serde_json::Value;

/// Release endpoint used by the Check for updates button.
pub const RELEASE_URL: &str =
    "https://api.github.com/repos/nhth-mibu/Lazy-Arranger-v2/releases/latest";

/// Compares dotted versions; returns true when `candidate` is newer.
#[must_use]
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn parts(value: &str) -> Vec<u64> {
        value
            .trim()
            .trim_start_matches('v')
            .split(['.', '-', '+'])
            .take(3)
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    }
    let candidate = parts(candidate);
    let current = parts(current);
    for index in 0..3 {
        let left = candidate.get(index).copied().unwrap_or(0);
        let right = current.get(index).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    false
}

/// Text for a release response.
#[must_use]
pub fn describe_release(current: &str, tag: &str, url: &str) -> String {
    if is_newer(tag, current) {
        format!("Update {tag} available: {url}")
    } else {
        format!("Up to date (this build is v{current})")
    }
}

/// Extracts `tag_name` and `html_url` from a release response.
///
/// # Errors
///
/// Returns a message when the response is not a release object.
pub fn parse_release(body: &str) -> Result<(String, String), String> {
    let value: Value =
        serde_json::from_str(body).map_err(|error| format!("release JSON: {error}"))?;
    if value.get("message").and_then(Value::as_str) == Some("Not Found") {
        return Err(String::from("no published release"));
    }
    let tag = value
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("release has no tag_name"))?;
    let url = value
        .get("html_url")
        .and_then(Value::as_str)
        .unwrap_or(RELEASE_URL);
    Ok((tag.to_string(), url.to_string()))
}

/// Fetches the latest release and formats one status line.
#[must_use]
pub fn check_status() -> String {
    let output = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            "10",
            "-H",
            "Accept: application/vnd.github+json",
            RELEASE_URL,
        ])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => return format!("Update check failed: {error}"),
    };
    if !output.status.success() {
        return format!(
            "Update check failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    match parse_release(&String::from_utf8_lossy(&output.stdout)) {
        Ok((tag, url)) => describe_release(env!("CARGO_PKG_VERSION"), &tag, &url),
        Err(error) => format!("Update check failed: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare() {
        assert!(is_newer("v1.2.0", "1.1.9"));
        assert!(!is_newer("1.1.9", "1.2.0"));
        assert!(!is_newer("v1.2.0", "1.2.0"));
        assert!(is_newer("2.0", "1.9.9"));
    }

    #[test]
    fn release_text_reports_newer_and_current() {
        assert!(describe_release("1.0.0", "v1.1.0", "https://x").contains("Update v1.1.0"));
        assert!(describe_release("1.0.0", "v1.0.0", "").starts_with("Up to date"));
    }

    #[test]
    fn release_json_parses() {
        let body = r#"{"tag_name":"v1.1.0","html_url":"https://example.test/r"}"#;
        let (tag, url) = parse_release(body).expect("release");
        assert_eq!(tag, "v1.1.0");
        assert_eq!(url, "https://example.test/r");
        assert!(parse_release(r#"{"message":"Not Found"}"#).is_err());
    }

    #[test]
    fn a_local_release_endpoint_is_read() {
        if std::process::Command::new("curl")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer);
            let body = r#"{"tag_name":"v9.9.9","html_url":"https://example.test"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("write");
        });
        let status = std::process::Command::new("curl")
            .args([
                "--silent",
                "--max-time",
                "5",
                &format!("http://{address}/release"),
            ])
            .output()
            .expect("curl");
        let (tag, url) = parse_release(&String::from_utf8_lossy(&status.stdout)).expect("release");
        assert_eq!(tag, "v9.9.9");
        assert_eq!(url, "https://example.test");
    }
}
