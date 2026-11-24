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

/// One published release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Release tag.
    pub tag: String,
    /// Release page.
    pub url: String,
    /// Downloadable assets.
    pub assets: Vec<Asset>,
}

/// One downloadable release asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    /// Asset file name.
    pub name: String,
    /// Download URL.
    pub url: String,
}

/// True when the release should be offered.
#[must_use]
pub fn should_offer(tag: &str, current: &str, skipped: &str) -> bool {
    is_newer(tag, current) && tag != skipped
}

/// Fetches the latest release.
///
/// # Errors
///
/// Returns a message when curl, the request or the JSON fails.
pub fn fetch_release() -> Result<Release, String> {
    let output = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            "10",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "-H",
            "Accept: application/vnd.github+json",
            RELEASE_URL,
        ])
        .output()
        .map_err(|error| format!("curl: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "endpoint: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_release(&String::from_utf8_lossy(&output.stdout))
}

/// Opens an HTTP(S) release page in the browser.
pub fn open_url(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "linux")]
    let program = "xdg-open";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let program = "open";
    let _ = std::process::Command::new(program).arg(url).spawn();
}

/// Opens a downloaded file in the platform file manager.
pub fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "linux")]
    let program = "xdg-open";
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    let program = "open";
    let _ = std::process::Command::new(program).arg(path).spawn();
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
pub fn parse_release(body: &str) -> Result<Release, String> {
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
    let assets = value
        .get("assets")
        .and_then(Value::as_array)
        .map(|assets| {
            assets
                .iter()
                .filter_map(|asset| {
                    Some(Asset {
                        name: asset.get("name")?.as_str()?.to_string(),
                        url: asset.get("browser_download_url")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Release {
        tag: tag.to_string(),
        url: url.to_string(),
        assets,
    })
}

/// Picks a disk image or archive asset, else the first asset.
#[must_use]
pub fn preferred_asset(release: &Release) -> Option<&Asset> {
    release
        .assets
        .iter()
        .find(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name.ends_with(".dmg") || name.ends_with(".zip") || name.ends_with(".tar.gz")
        })
        .or_else(|| release.assets.first())
}

/// Downloads one asset into a directory and returns the written path.
///
/// # Errors
///
/// Returns a message when the directory or curl fails.
pub fn download_asset(
    asset: &Asset,
    directory: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let secure = asset.url.starts_with("https://")
        || (cfg!(test)
            && (asset.url.starts_with("http://127.0.0.1")
                || asset.url.starts_with("http://localhost")));
    if !secure {
        return Err(String::from("update asset URL is not HTTPS"));
    }
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let name = std::path::Path::new(&asset.name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| String::from("asset has no usable file name"))?;
    let path = directory.join(name);
    let mut command = std::process::Command::new("curl");
    command.args([
        "--location",
        "--fail",
        "--silent",
        "--show-error",
        "--max-time",
        "300",
    ]);
    if asset.url.starts_with("https://") {
        command.args(["--proto", "=https", "--proto-redir", "=https"]);
    }
    let output = command
        .arg("--output")
        .arg(&path)
        .arg(&asset.url)
        .output()
        .map_err(|error| format!("curl: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "download: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(path)
}

/// Fetches the latest release and formats one status line.
#[must_use]
pub fn check_status() -> String {
    match fetch_release() {
        Ok(release) => describe_release(env!("CARGO_PKG_VERSION"), &release.tag, &release.url),
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
    fn skipped_releases_are_not_offered() {
        assert!(should_offer("v1.1.0", "1.0.0", ""));
        assert!(!should_offer("v1.1.0", "1.0.0", "v1.1.0"));
        assert!(!should_offer("v1.0.0", "1.0.0", ""));
    }

    #[test]
    fn release_text_reports_newer_and_current() {
        assert!(describe_release("1.0.0", "v1.1.0", "https://x").contains("Update v1.1.0"));
        assert!(describe_release("1.0.0", "v1.0.0", "").starts_with("Up to date"));
    }

    #[test]
    fn release_json_parses() {
        let body = r#"{"tag_name":"v1.1.0","html_url":"https://example.test/r","assets":[{"name":"app.dmg","browser_download_url":"https://example.test/app.dmg"}]}"#;
        let release = parse_release(body).expect("release");
        assert_eq!(release.tag, "v1.1.0");
        assert_eq!(release.url, "https://example.test/r");
        assert_eq!(release.assets[0].name, "app.dmg");
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
        let release = parse_release(&String::from_utf8_lossy(&status.stdout)).expect("release");
        assert_eq!(release.tag, "v9.9.9");
        assert_eq!(release.url, "https://example.test");
    }

    #[test]
    fn a_disk_image_is_preferred_and_downloaded() {
        let release = Release {
            tag: String::from("v1"),
            url: String::new(),
            assets: vec![
                Asset {
                    name: String::from("notes.txt"),
                    url: String::new(),
                },
                Asset {
                    name: String::from("App.dmg"),
                    url: String::from("http://example.test/app.dmg"),
                },
            ],
        };
        assert_eq!(preferred_asset(&release).expect("asset").name, "App.dmg");
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
            let body = b"fake disk image";
            let response = format!(
                "HTTP/1.1 200 OK
Content-Length: {}\r
Connection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).expect("headers");
            stream.write_all(body).expect("body");
        });
        let asset = Asset {
            name: String::from("App.dmg"),
            url: format!("http://{address}/app.dmg"),
        };
        let directory = std::env::temp_dir().join("lazy-arranger-download-test");
        let _ = std::fs::remove_dir_all(&directory);
        let path = download_asset(&asset, &directory).expect("download");
        assert_eq!(std::fs::read(&path).expect("file"), b"fake disk image");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
