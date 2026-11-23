//! Recognition of Korg PCG containers.
//!
//! Korg's PCG format is not published. This scanner only reports whether the
//! bytes look like a Korg container and which model name appears near the
//! start; it does not decode programs or combis, and an unrecognised file is
//! reported as unknown rather than guessed.

/// What the scanner saw.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KorgScan {
    /// True when a `KORG` marker appears in the first 4096 bytes.
    pub korg_marker: bool,
    /// Model string found near the marker, when any.
    pub model: Option<String>,
    /// Offset of the `KORG` marker.
    pub offset: Option<usize>,
    /// File size in bytes.
    pub size: usize,
}

const MODELS: [&str; 4] = ["KRONOS", "KROME", "KINGKORG", "TRITON"];

/// Scans bytes for a Korg marker and model name.
#[must_use]
pub fn scan(bytes: &[u8]) -> KorgScan {
    let window = &bytes[..bytes.len().min(4096)];
    let offset = window
        .windows(4)
        .position(|window| window.eq_ignore_ascii_case(b"KORG"));
    let model = MODELS.iter().find_map(|model| {
        window
            .windows(model.len())
            .any(|window| window.eq_ignore_ascii_case(model.as_bytes()))
            .then(|| (*model).to_string())
    });
    KorgScan {
        korg_marker: offset.is_some(),
        model,
        offset,
        size: bytes.len(),
    }
}

/// One line describing the scan, for a status or CLI.
#[must_use]
pub fn describe(scan: &KorgScan) -> String {
    if !scan.korg_marker {
        return format!(
            "not a recognised Korg container ({} byte(s)); program decoding is not implemented",
            scan.size
        );
    }
    format!(
        "Korg container at byte {} with model {} ({} byte(s)); program and combi decoding is not implemented",
        scan.offset.unwrap_or(0),
        scan.model.as_deref().unwrap_or("unknown"),
        scan.size
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marker_and_model_are_recognised() {
        let mut bytes = vec![0_u8; 64];
        bytes.extend_from_slice(b"KORG");
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(b"KRONOS");
        let scan = scan(&bytes);
        assert!(scan.korg_marker);
        assert_eq!(scan.model.as_deref(), Some("KRONOS"));
        assert_eq!(scan.offset, Some(64));
        assert!(describe(&scan).contains("KRONOS"));
    }

    #[test]
    fn random_bytes_are_not_called_korg() {
        let scan = scan(b"just some bytes");
        assert!(!scan.korg_marker);
        assert!(scan.model.is_none());
        assert!(describe(&scan).contains("not a recognised"));
    }

    #[test]
    fn model_matching_is_case_insensitive() {
        let bytes = b"xxxxkorgxxxxkrome";
        let scan = scan(bytes);
        assert!(scan.korg_marker);
        assert_eq!(scan.model.as_deref(), Some("KROME"));
    }
}
