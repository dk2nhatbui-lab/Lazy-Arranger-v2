//! Music Finder books and Yamaha playlists.
//!
//! Formats are pinned to the C++ implementation so files move between the two
//! applications unchanged (parity with `core/src/MusicFinder.cpp` and
//! `core/src/Playlist.cpp`).

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors from Music Finder or playlist parsing.
#[derive(Debug, Error)]
pub enum LibraryError {
    /// The document is not the expected kind.
    #[error("{0}")]
    Format(String),
    /// A number could not be parsed.
    #[error("{0}")]
    Number(String),
}

/// One Music Finder entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Song name.
    pub name: String,
    /// Style file name or style name.
    #[serde(default)]
    pub style: String,
    /// Tempo in BPM.
    #[serde(default = "default_tempo")]
    pub tempo: f64,
    /// Yamaha style number, or -1.
    #[serde(default = "default_number")]
    pub style_number: i32,
    /// Meter numerator.
    #[serde(default = "default_four")]
    pub time_numerator: u32,
    /// Meter denominator.
    #[serde(default = "default_four")]
    pub time_denominator: u32,
    /// One voice name per part.
    #[serde(default)]
    pub voices: Vec<String>,
    /// Free notes.
    #[serde(default)]
    pub notes: String,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            name: String::new(),
            style: String::new(),
            tempo: 120.0,
            style_number: -1,
            time_numerator: 4,
            time_denominator: 4,
            voices: Vec::new(),
            notes: String::new(),
        }
    }
}

fn default_tempo() -> f64 {
    120.0
}
fn default_number() -> i32 {
    -1
}
fn default_four() -> u32 {
    4
}

/// A Music Finder book.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Book {
    /// Entries, in display order.
    #[serde(default)]
    pub entries: Vec<Entry>,
}

impl Book {
    /// Parses the stable JSON form (`arranger-music-finder`).
    ///
    /// # Errors
    ///
    /// Returns [`LibraryError::Format`] for another document kind.
    pub fn from_json(text: &str) -> Result<Self, LibraryError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|error| LibraryError::Format(error.to_string()))?;
        if value.get("format").and_then(|value| value.as_str()) != Some("arranger-music-finder") {
            return Err(LibraryError::Format(String::from(
                "not a Music Finder file",
            )));
        }
        let entries = value
            .get("entries")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
        let mut parsed: Vec<Entry> = serde_json::from_value(entries)
            .map_err(|error| LibraryError::Format(error.to_string()))?;
        for entry in &mut parsed {
            if entry.time_numerator == 0 {
                entry.time_numerator = 4;
            }
            if entry.time_denominator == 0 {
                entry.time_denominator = 4;
            }
        }
        Ok(Self { entries: parsed })
    }

    /// Writes the stable JSON form with the C++ key names.
    #[must_use]
    pub fn to_json(&self) -> String {
        let entries: Vec<serde_json::Value> = self
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "name": entry.name,
                    "style": entry.style,
                    "tempo": entry.tempo,
                    "styleNumber": entry.style_number,
                    "timeNumerator": entry.time_numerator,
                    "timeDenominator": entry.time_denominator,
                    "voices": entry.voices,
                    "notes": entry.notes,
                })
            })
            .collect();
        serde_json::json!({
            "format": "arranger-music-finder",
            "entries": entries,
        })
        .to_string()
    }

    /// Parses a Yamaha Music Finder file (`MDB-100-100-3000`).
    ///
    /// # Errors
    ///
    /// Returns [`LibraryError::Format`] for another file or a truncated one.
    pub fn from_mfd(bytes: &[u8]) -> Result<Self, LibraryError> {
        const HEADER: usize = 36;
        const RECORD: usize = 92;
        let magic = b"MDB-100-100-3000";
        if bytes.len() < HEADER || !bytes.starts_with(magic) {
            return Err(LibraryError::Format(String::from(
                "not a Yamaha Music Finder file",
            )));
        }
        let count = usize::from(u16::from_be_bytes([bytes[34], bytes[35]]));
        let records = if count > 0 {
            count
        } else {
            (bytes.len() - HEADER) / RECORD
        };
        let field = |at: usize, offset: usize, length: usize| -> String {
            let start = at + offset;
            let end = (start + length).min(bytes.len());
            let slice = &bytes[start.min(end)..end];
            let stop = slice
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(slice.len());
            String::from_utf8_lossy(&slice[..stop]).trim().to_string()
        };
        let mut entries = Vec::new();
        for index in 0..records {
            let at = HEADER + index * RECORD;
            if at + RECORD > bytes.len() {
                break;
            }
            let name = field(at, 11, 32);
            if name.is_empty() {
                continue;
            }
            let mut notes = field(at, 43, 16);
            let keywords = field(at, 59, 32);
            if !keywords.is_empty() {
                if notes.is_empty() {
                    notes = keywords;
                } else {
                    notes = format!("{notes}, {keywords}");
                }
            }
            entries.push(Entry {
                name,
                style: String::new(),
                tempo: f64::from(u16::from_be_bytes([bytes[at + 8], bytes[at + 9]])),
                style_number: i32::from(u16::from_be_bytes([bytes[at + 4], bytes[at + 5]])),
                time_numerator: u32::from(bytes[at + 6]),
                time_denominator: u32::from(bytes[at + 7]),
                voices: Vec::new(),
                notes,
            });
        }
        Ok(Self { entries })
    }

    /// Writes `name,style,tempo,voices,notes` CSV, quoting as the C++ code does.
    #[must_use]
    pub fn to_csv(&self) -> String {
        let mut out = String::from("name,style,tempo,voices,notes\n");
        for entry in &self.entries {
            let voices = entry.voices.join("|");
            out.push_str(&format!(
                "{},{},{:.1},{},{}\n",
                quote(&entry.name),
                quote(&entry.style),
                entry.tempo,
                quote(&voices),
                quote(&entry.notes)
            ));
        }
        out
    }

    /// Parses that CSV; a missing header is an error.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing header, a short row or a bad tempo.
    pub fn from_csv(text: &str) -> Result<Self, LibraryError> {
        let mut lines = text.lines();
        let header = lines.next().unwrap_or_default();
        if !header.starts_with("name,style,tempo,voices,notes") {
            return Err(LibraryError::Format(String::from("not a Music Finder CSV")));
        }
        let mut entries = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let fields = parse_csv_line(line);
            if fields.len() < 5 {
                return Err(LibraryError::Format(String::from(
                    "a Music Finder CSV row needs name, style and tempo",
                )));
            }
            let tempo: f64 = fields[2].parse().map_err(|_| {
                LibraryError::Number(String::from("a Music Finder tempo is not a number"))
            })?;
            if !tempo.is_finite() || tempo <= 0.0 {
                return Err(LibraryError::Number(String::from(
                    "a Music Finder tempo must be finite and positive",
                )));
            }
            entries.push(Entry {
                name: fields[0].clone(),
                style: fields[1].clone(),
                tempo,
                voices: fields[3]
                    .split('|')
                    .filter(|voice| !voice.is_empty())
                    .map(String::from)
                    .collect(),
                notes: fields[4].clone(),
                ..Entry::default()
            });
        }
        Ok(Self { entries })
    }
}

fn quote(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!(
            "\"{}\"",
            field.replace('"', "\"\"").replace(['\n', '\r'], " ")
        )
    } else {
        field.to_string()
    }
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.peek() == Some(&'"') => {
                field.push('"');
                characters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            other => field.push(other),
        }
    }
    fields.push(field);
    fields
}

/// One Yamaha playlist row.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Title shown by the keyboard.
    pub title: String,
    /// Path stored by the keyboard, such as `I:/Playlist/Song.rgt`.
    pub file_path: String,
    /// Registration number.
    pub regist: i32,
    /// Whether the keyboard opens the viewer.
    pub open_viewer: bool,
}

/// A playlist.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct List {
    /// Rows in order.
    #[serde(default)]
    pub items: Vec<Item>,
}

impl List {
    /// Builds a playlist from a Music Finder book, pointing at a folder.
    ///
    /// Entries keep their order, REGIST is the entry number, and the viewer
    /// flag stays off.
    #[must_use]
    pub fn from_book(book: &Book, folder: &str) -> Self {
        let folder = folder.trim_end_matches('/');
        Self {
            items: book
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| Item {
                    title: entry.name.clone(),
                    file_path: if entry.style.is_empty() {
                        String::new()
                    } else {
                        format!("{folder}/{}", entry.style)
                    },
                    regist: i32::try_from(index + 1).unwrap_or(1),
                    open_viewer: false,
                })
                .collect(),
        }
    }

    /// Parses the Yamaha tab-separated playlist.
    ///
    /// # Errors
    ///
    /// Returns an error for a missing header, a short row or a bad REGIST.
    pub fn from_tsv(text: &str) -> Result<Self, LibraryError> {
        let mut lines = text.lines();
        let header = lines.next().unwrap_or_default().trim_end_matches('\r');
        if !header.starts_with("TITLE\tFILEPATH\tREGIST\tOPENVIEWER") {
            return Err(LibraryError::Format(String::from(
                "not a Yamaha Playlist file",
            )));
        }
        let mut items = Vec::new();
        for line in lines {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() < 4 {
                return Err(LibraryError::Format(String::from(
                    "a Playlist row needs four tab-separated fields",
                )));
            }
            let regist = fields[2].parse().map_err(|_| {
                LibraryError::Number(String::from("a Playlist REGIST value is not a number"))
            })?;
            items.push(Item {
                title: fields[0].to_string(),
                file_path: fields[1].to_string(),
                regist,
                open_viewer: fields[3] == "ON",
            });
        }
        Ok(Self { items })
    }

    /// Writes the Yamaha format with CRLF line ends.
    #[must_use]
    pub fn to_tsv(&self) -> String {
        let mut out = String::from("TITLE\tFILEPATH\tREGIST\tOPENVIEWER\r\n");
        let clean = |value: &str| value.replace(['\t', '\r', '\n'], " ");
        for item in &self.items {
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\r\n",
                clean(&item.title),
                clean(&item.file_path),
                item.regist,
                if item.open_viewer { "ON" } else { "OFF" }
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        Entry {
            name: String::from("My Way"),
            style: String::from("PopBallad.T161"),
            tempo: 72.0,
            style_number: 42,
            time_numerator: 4,
            time_denominator: 4,
            voices: vec![String::from("Piano"), String::from("Strings")],
            notes: String::from("encore"),
        }
    }

    #[test]
    fn a_default_entry_has_a_real_tempo_and_meter() {
        let entry = Entry::default();
        assert_eq!(entry.tempo, 120.0);
        assert_eq!(entry.style_number, -1);
        assert_eq!((entry.time_numerator, entry.time_denominator), (4, 4));
        assert!(Book::from_csv("name,style,tempo,voices,notes\nSong,style,NaN,,notes\n",).is_err());
    }

    #[test]
    fn json_round_trips_with_the_cpp_keys() {
        let book = Book {
            entries: vec![entry()],
        };
        let json = book.to_json();
        assert!(json.contains("\"format\":\"arranger-music-finder\""));
        assert!(json.contains("\"styleNumber\":42"));
        assert_eq!(Book::from_json(&json).expect("parse"), book);
        assert!(Book::from_json("{\"format\":\"other\"}").is_err());
    }

    #[test]
    fn csv_matches_the_cpp_header_and_escapes() {
        let mut special = entry();
        special.notes = String::from("a, \"b\"");
        let book = Book {
            entries: vec![special],
        };
        let csv = book.to_csv();
        assert!(csv.starts_with("name,style,tempo,voices,notes\n"));
        assert!(csv.contains("\"a, \"\"b\"\"\""));
        let parsed = Book::from_csv(&csv).expect("parse");
        assert_eq!(parsed.entries[0].notes, "a, \"b\"");
        assert_eq!(parsed.entries[0].voices, vec!["Piano", "Strings"]);
        assert!(Book::from_csv("wrong\n").is_err());
    }

    #[test]
    fn mfd_reads_records_and_skips_empty_names() {
        let mut bytes = vec![0_u8; 36];
        bytes[..16].copy_from_slice(b"MDB-100-100-3000");
        bytes[34] = 0;
        bytes[35] = 3;
        let mut record = |name: &str, number: u16, meter: (u8, u8), tempo: u16, genre: &str| {
            let mut entry = vec![0_u8; 92];
            entry[4..6].copy_from_slice(&number.to_be_bytes());
            entry[6] = meter.0;
            entry[7] = meter.1;
            entry[8..10].copy_from_slice(&tempo.to_be_bytes());
            entry[11..11 + name.len()].copy_from_slice(name.as_bytes());
            entry[43..43 + genre.len()].copy_from_slice(genre.as_bytes());
            bytes.extend_from_slice(&entry);
        };
        record("My Way", 42, (4, 4), 72, "Pop");
        record("", 7, (3, 4), 90, "Jazz");
        record("Autumn Leaves", 9, (4, 4), 120, "Jazz");
        let book = Book::from_mfd(&bytes).expect("parse");
        assert_eq!(book.entries.len(), 2);
        assert_eq!(book.entries[0].name, "My Way");
        assert_eq!(book.entries[0].style_number, 42);
        assert_eq!(book.entries[0].tempo, 72.0);
        assert_eq!(book.entries[0].time_numerator, 4);
        assert_eq!(book.entries[0].notes, "Pop");
        assert_eq!(book.entries[1].name, "Autumn Leaves");
        assert!(Book::from_mfd(b"not an mfd").is_err());
    }

    #[test]
    fn playlist_round_trips_with_crlf_and_flags() {
        let list = List {
            items: vec![Item {
                title: String::from("Opening"),
                file_path: String::from("I:/Playlist/A.rgt"),
                regist: 1,
                open_viewer: true,
            }],
        };
        let tsv = list.to_tsv();
        assert!(tsv.starts_with("TITLE\tFILEPATH\tREGIST\tOPENVIEWER\r\n"));
        assert!(tsv.contains("\tON\r\n"));
        assert_eq!(List::from_tsv(&tsv).expect("parse"), list);
        assert!(List::from_tsv("nope\n").is_err());
    }

    #[test]
    fn playlist_rejects_short_rows_and_bad_numbers() {
        let header = "TITLE\tFILEPATH\tREGIST\tOPENVIEWER\r\n";
        assert!(List::from_tsv(&format!("{header}only\ttwo\r\n")).is_err());
        assert!(List::from_tsv(&format!("{header}a\tb\tx\tON\r\n")).is_err());
    }
}

#[cfg(test)]
mod playlist_from_book_tests {
    use super::*;

    #[test]
    fn a_book_becomes_a_playlist_in_order() {
        let text = "name,style,tempo,voices,notes\nMy Way,Ballad.sty,68,,\nFunk,Funk.sty,100,,\n";
        let book = Book::from_csv(text).expect("book");
        let list = List::from_book(&book, "/styles/");
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[0].title, "My Way");
        assert_eq!(list.items[0].file_path, "/styles/Ballad.sty");
        assert_eq!(list.items[0].regist, 1);
        assert_eq!(list.items[1].regist, 2);
        let tsv = list.to_tsv();
        assert!(tsv.contains("My Way"));
        assert!(tsv.contains("/styles/Ballad.sty"));
        assert_eq!(List::from_tsv(&tsv).expect("round trip").items, list.items);
    }

    #[test]
    fn entries_without_a_style_get_an_empty_path() {
        let text = "name,style,tempo,voices,notes\nOnly name,,120,,\n";
        let book = Book::from_csv(text).expect("book");
        let list = List::from_book(&book, "/styles");
        assert!(list.items[0].file_path.is_empty());
    }
}
