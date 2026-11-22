//! Yamaha style containers and the native style model.
//!
//! Stage 4 of the blueprint owns style import: CASM parsing, NTR/NTT chord
//! conversion and a playable arrangement. This crate holds only the part that
//! can be verified against a fixture today:
//!
//! * [`scan_chunks`] walks a Yamaha `.sty`/`.prs` container. The format is a
//!   sequence of four-byte chunk identifiers followed by a big-endian 32-bit
//!   length, which covers `MThd`, `MTrk` and the CASM/CSEG/Sdec/OTSc chunks
//!   that later work will interpret.
//! * [`NativeStyle`] is the manufacturer-neutral style shape that a future
//!   importer fills. It deliberately keeps the Yamaha source chunks so a lossy
//!   conversion can be reported instead of hidden.
//!
//! The C++ audit (`docs/audit/notes/cpp-style-arranger.md`) records that the
//! demo style in `tests/fixtures` carries only `MThd` and `MTrk`; real PSR
//! styles add CASM. Nothing here claims CASM support.

use thiserror::Error;

pub mod casm;

pub use casm::{CasmRules, CasmStats, SectionRules, SffVersion, read_rules};

/// Errors raised while reading a style container.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StyleError {
    /// The file ended in the middle of a chunk header or payload.
    #[error("style container is truncated at byte {offset}: {detail}")]
    Truncated {
        /// Offset where the truncation was found.
        offset: usize,
        /// What was expected there.
        detail: &'static str,
    },
    /// A chunk identifier was not four printable ASCII bytes.
    #[error("chunk at byte {offset} has an invalid identifier {id:?}")]
    BadChunkId {
        /// Offset of the identifier.
        offset: usize,
        /// The bytes found, rendered for display.
        id: String,
    },
    /// The container has no header chunk.
    #[error("style container has no MThd chunk")]
    MissingHeader,
    /// The container declares a format this reader does not know.
    #[error("unsupported style container format {0:?}")]
    UnsupportedFormat(String),
    /// A CASM rule record was shorter than its encoding requires.
    #[error("CASM rule at byte {offset} is {len} bytes; {needed} are needed")]
    CasmRule {
        /// Offset of the record.
        offset: usize,
        /// Length found.
        len: usize,
        /// Length required by the encoding.
        needed: usize,
    },
}

/// One chunk in a style container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    /// Four-byte identifier, for example `MThd`, `MTrk` or `CASM`.
    pub id: [u8; 4],
    /// Byte offset of the identifier inside the file.
    pub offset: usize,
    /// Payload length in bytes, excluding the eight-byte header.
    pub len: usize,
}

impl Chunk {
    /// Returns the identifier as text.
    #[must_use]
    pub fn id_str(&self) -> String {
        self.id
            .iter()
            .map(|byte| char::from(*byte))
            .collect::<String>()
    }

    /// Byte offset of the payload.
    #[must_use]
    pub fn payload_offset(&self) -> usize {
        self.offset + 8
    }

    /// Payload slice, or `None` when the slice does not fit the file.
    #[must_use]
    pub fn payload<'a>(&self, file: &'a [u8]) -> Option<&'a [u8]> {
        file.get(self.payload_offset()..self.payload_offset() + self.len)
    }
}

/// Walks every chunk in a Yamaha style container.
///
/// # Errors
///
/// Returns [`StyleError::Truncated`] when a header or payload runs past the
/// end of the file, [`StyleError::BadChunkId`] for a non-printable identifier,
/// and [`StyleError::MissingHeader`] when the first chunk is not `MThd`.
pub fn scan_chunks(bytes: &[u8]) -> Result<Vec<Chunk>, StyleError> {
    let mut chunks = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        // Some writers pad an odd-length chunk with one zero byte before the
        // next identifier. Skip up to three such bytes; a real identifier is
        // printable, so this cannot hide a corrupt chunk id.
        let mut padding = 0;
        while offset + 4 <= bytes.len()
            && bytes[offset] == 0
            && !bytes[offset..offset + 4].iter().all(u8::is_ascii_graphic)
            && padding < 3
        {
            offset += 1;
            padding += 1;
        }
        let header = bytes.get(offset..offset + 8).ok_or(StyleError::Truncated {
            offset,
            detail: "chunk header",
        })?;
        let mut id = [0u8; 4];
        id.copy_from_slice(&header[..4]);
        if !id.iter().all(|byte| byte.is_ascii_graphic()) {
            return Err(StyleError::BadChunkId {
                offset,
                id: String::from_utf8_lossy(&id).into_owned(),
            });
        }
        if chunks.is_empty() && &id != b"MThd" {
            return Err(StyleError::MissingHeader);
        }
        let len = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let end = offset
            .checked_add(8)
            .and_then(|start| start.checked_add(len))
            .ok_or(StyleError::Truncated {
                offset,
                detail: "chunk length overflow",
            })?;
        if end > bytes.len() {
            return Err(StyleError::Truncated {
                offset,
                detail: "chunk payload",
            });
        }
        chunks.push(Chunk { id, offset, len });
        offset = end;
    }
    if chunks.is_empty() {
        return Err(StyleError::MissingHeader);
    }
    Ok(chunks)
}

/// A style section as the native model sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleSection {
    /// Display name, for example `Intro A` or `Main B`.
    pub name: String,
    /// Length in ticks, when the source states it.
    pub length_ticks: Option<i64>,
}

/// Where a native style came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleSource {
    /// Authored in this application.
    Native,
    /// Imported from a Yamaha container; the chunk list is kept for reports.
    Yamaha {
        /// Chunk identifiers in source order.
        chunks: Vec<String>,
        /// True when the container carried CASM chord rules.
        has_casm: bool,
    },
    /// Imported from a Korg source; not implemented in this milestone.
    Korg,
}

/// A manufacturer-neutral style.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeStyle {
    /// Display name.
    pub name: String,
    /// Sections in source order.
    pub sections: Vec<StyleSection>,
    /// Provenance and preserved manufacturer metadata.
    pub source: StyleSource,
    /// Lossy-conversion warnings produced during import.
    pub warnings: Vec<String>,
}

impl NativeStyle {
    /// Summarises a Yamaha container without interpreting its musical data.
    ///
    /// # Errors
    ///
    /// Returns [`StyleError`] when the container cannot be scanned.
    pub fn describe_yamaha(name: impl Into<String>, bytes: &[u8]) -> Result<Self, StyleError> {
        let chunks = scan_chunks(bytes)?;
        let ids: Vec<String> = chunks.iter().map(Chunk::id_str).collect();
        let has_casm = ids.iter().any(|id| id == "CASM");
        let has_sections = ids.iter().any(|id| id == "CSEG");
        let mut warnings = Vec::new();
        if !has_casm {
            warnings.push(String::from(
                "no CASM chunk: chord rules are unavailable; notes would play as written",
            ));
        }
        if !has_sections {
            warnings.push(String::from(
                "no CSEG chunk: section boundaries are unknown in this container",
            ));
        }
        Ok(Self {
            name: name.into(),
            sections: Vec::new(),
            source: StyleSource::Yamaha {
                chunks: ids,
                has_casm,
            },
            warnings,
        })
    }

    /// True when the style came with CASM chord rules.
    #[must_use]
    pub fn has_chord_rules(&self) -> bool {
        matches!(&self.source, StyleSource::Yamaha { has_casm: true, .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO_STYLE: &[u8] = include_bytes!("../../../tests/fixtures/demo-style.sty");

    #[test]
    fn scans_the_demo_style_chunks() {
        let chunks = scan_chunks(DEMO_STYLE).expect("demo style scans");
        let ids: Vec<String> = chunks.iter().map(Chunk::id_str).collect();
        assert_eq!(ids, vec!["MThd", "MTrk"]);
        assert_eq!(chunks[0].offset, 0);
        assert_eq!(chunks[0].len, 6);
        assert_eq!(chunks[1].offset, 14);
        assert_eq!(chunks[1].len, 3918);
        assert_eq!(
            chunks[1].payload(DEMO_STYLE).expect("payload").len(),
            chunks[1].len
        );
        // The chunks tile the file with no gaps.
        let consumed: usize = chunks.iter().map(|chunk| chunk.len + 8).sum();
        assert_eq!(consumed, DEMO_STYLE.len());
    }

    #[test]
    fn header_payload_is_a_standard_midi_header() {
        let chunks = scan_chunks(DEMO_STYLE).expect("scan");
        let payload = chunks[0].payload(DEMO_STYLE).expect("payload");
        assert_eq!(payload.len(), 6);
        assert_eq!(
            u16::from_be_bytes([payload[0], payload[1]]),
            0,
            "the demo style is a single-track format 0 file"
        );
        assert_eq!(u16::from_be_bytes([payload[2], payload[3]]), 1, "one track");
        // Division is a metrical tick count.
        let division = u16::from_be_bytes([payload[4], payload[5]]);
        assert_eq!(division & 0x8000, 0, "metrical, not SMPTE");
        assert_eq!(
            division, 1920,
            "the style is authored at 1920 ticks per quarter, twice the project PPQ"
        );
    }

    #[test]
    fn describes_a_style_without_casm() {
        let style = NativeStyle::describe_yamaha("demo-style", DEMO_STYLE).expect("describe");
        assert_eq!(style.name, "demo-style");
        assert!(style.sections.is_empty());
        assert!(!style.has_chord_rules());
        assert_eq!(style.warnings.len(), 2);
        assert!(style.warnings[0].contains("no CASM"));
        match &style.source {
            StyleSource::Yamaha { chunks, has_casm } => {
                assert_eq!(chunks, &vec![String::from("MThd"), String::from("MTrk")]);
                assert!(!has_casm);
            }
            other => panic!("unexpected source: {other:?}"),
        }
    }

    #[test]
    fn handles_a_container_with_casm_and_cseg() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&[0, 1, 0, 1, 0x03, 0xC0]);
        for id in [b"MTrk", b"CASM", b"CSEG"] {
            bytes.extend_from_slice(id);
            bytes.extend_from_slice(&4u32.to_be_bytes());
            bytes.extend_from_slice(&[0, 0, 0, 0]);
        }
        let chunks = scan_chunks(&bytes).expect("scan");
        assert_eq!(chunks.len(), 4);
        let style = NativeStyle::describe_yamaha("with-casm", &bytes).expect("describe");
        assert!(style.has_chord_rules());
        assert!(style.warnings.is_empty());
    }

    #[test]
    fn rejects_truncated_and_malformed_containers() {
        assert_eq!(scan_chunks(&[]), Err(StyleError::MissingHeader));
        assert_eq!(scan_chunks(b"MTrk\0\0\0\0"), Err(StyleError::MissingHeader));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&[0, 1, 0, 1, 0x03]);
        assert!(matches!(
            scan_chunks(&bytes),
            Err(StyleError::Truncated { .. })
        ));

        let mut bad = Vec::new();
        bad.extend_from_slice(b"MThd");
        bad.extend_from_slice(&0u32.to_be_bytes());
        // A non-printable identifier that is not zero padding.
        bad.extend_from_slice(&[0x01, 0x01, 0x02, 0x03, 0, 0, 0, 0]);
        assert!(matches!(
            scan_chunks(&bad),
            Err(StyleError::BadChunkId { .. })
        ));
    }

    #[test]
    fn skips_zero_padding_before_a_chunk() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&[0, 1, 0, 1, 0x03, 0xC0]);
        bytes.push(0); // one padding byte, as some writers emit after an odd chunk
        bytes.extend_from_slice(b"CASM");
        bytes.extend_from_slice(&4u32.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        let chunks = scan_chunks(&bytes).expect("padded container scans");
        let ids: Vec<String> = chunks.iter().map(Chunk::id_str).collect();
        assert_eq!(ids, vec![String::from("MThd"), String::from("CASM")]);
        assert_eq!(chunks[1].offset, 15);
    }

    #[test]
    fn chunk_payload_is_bounded() {
        let chunk = Chunk {
            id: *b"MTrk",
            offset: 0,
            len: 100,
        };
        assert!(chunk.payload(&[0u8; 10]).is_none());
        assert_eq!(chunk.payload_offset(), 8);
    }
}
