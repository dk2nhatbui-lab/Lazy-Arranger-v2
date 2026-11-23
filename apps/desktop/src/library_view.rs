//! Music Finder and Playlist browser.

use std::path::Path;

use library::{Book, Entry, List};

use crate::theme;

/// One row shown in the browser.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Title.
    pub title: String,
    /// Secondary line: style, tempo, meter or file path.
    pub detail: String,
    /// Tempo to apply, when the source has one.
    pub tempo: Option<f64>,
    /// Style name hint.
    pub style: Option<String>,
    /// Style number, when the source has one.
    pub style_number: Option<usize>,
}

/// A loaded book or playlist.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Browser {
    /// Rows in source order.
    pub rows: Vec<Row>,
    /// Source kind, for the heading.
    pub kind: String,
}

impl Browser {
    fn from_book(book: &Book) -> Self {
        Self {
            rows: book.entries.iter().map(row_from_entry).collect(),
            kind: String::from("Music Finder"),
        }
    }

    fn from_list(list: &List) -> Self {
        Self {
            rows: list
                .items
                .iter()
                .map(|item| Row {
                    title: item.title.clone(),
                    detail: format!("{} · regist {}", item.file_path, item.regist),
                    tempo: None,
                    style: None,
                    style_number: None,
                })
                .collect(),
            kind: String::from("Playlist"),
        }
    }

    /// Rows whose title or detail contains the query, case-insensitively.
    #[must_use]
    pub fn filtered(&self, query: &str) -> Vec<usize> {
        let needle = query.trim().to_ascii_lowercase();
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                needle.is_empty()
                    || row.title.to_ascii_lowercase().contains(&needle)
                    || row.detail.to_ascii_lowercase().contains(&needle)
            })
            .map(|(index, _)| index)
            .collect()
    }
}

fn row_from_entry(entry: &Entry) -> Row {
    Row {
        title: entry.name.clone(),
        detail: format!(
            "{} · {} bpm · {}/{}",
            entry.style, entry.tempo, entry.time_numerator, entry.time_denominator
        ),
        tempo: Some(entry.tempo),
        style: if entry.style.is_empty() {
            None
        } else {
            Some(entry.style.clone())
        },
        style_number: (entry.style_number > 0).then_some(entry.style_number as usize),
    }
}

/// Loads a book or playlist from a file by extension.
///
/// # Errors
///
/// Returns a message for an unsupported extension or a parse failure.
pub fn load(path: &Path) -> Result<Browser, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "json" => {
            let text = String::from_utf8_lossy(&bytes);
            Book::from_json(&text)
                .map(|book| Browser::from_book(&book))
                .map_err(|error| error.to_string())
        }
        "csv" => {
            let text = String::from_utf8_lossy(&bytes);
            Book::from_csv(&text)
                .map(|book| Browser::from_book(&book))
                .map_err(|error| error.to_string())
        }
        "mfd" => Book::from_mfd(&bytes)
            .map(|book| Browser::from_book(&book))
            .map_err(|error| error.to_string()),
        "tsv" => {
            let text = String::from_utf8_lossy(&bytes);
            List::from_tsv(&text)
                .map(|list| Browser::from_list(&list))
                .map_err(|error| error.to_string())
        }
        other => Err(format!("unsupported library extension {other:?}")),
    }
}

/// What the user asked the browser to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Apply the row's tempo.
    Apply(usize),
    /// Import the row's style.
    Import(usize),
}

/// Draws the browser; returns the requested action.
pub fn show(ui: &mut egui::Ui, browser: &Browser, query: &mut String) -> Option<Action> {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&browser.kind)
                .size(10.0)
                .color(theme::TEXT_DIM),
        );
        ui.add(
            egui::TextEdit::singleline(query)
                .hint_text("search")
                .desired_width(180.0),
        );
    });
    let matches = browser.filtered(query);
    let mut apply = None;
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            if matches.is_empty() {
                ui.label(
                    egui::RichText::new("no matches")
                        .size(10.0)
                        .color(theme::TEXT_DIM),
                );
            }
            for index in matches {
                let row = &browser.rows[index];
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new(&row.title).size(11.0),
                        ))
                        .clicked()
                    {
                        apply = Some(Action::Apply(index));
                    }
                    if row.style.is_some() && ui.small_button("Import").clicked() {
                        apply = Some(Action::Import(index));
                    }
                    ui.label(
                        egui::RichText::new(&row.detail)
                            .size(9.0)
                            .color(theme::TEXT_MUTED),
                    );
                });
            }
        });
    apply
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_books_load_and_filter() {
        let text = r#"{"format":"arranger-music-finder","entries":[
            {"name":"My Way","style":"Ballad.sty","tempo":68.0},
            {"name":"Superstition","style":"Funk.sty","tempo":100.0}
        ]}"#;
        let book = Book::from_json(text).expect("book");
        let browser = Browser::from_book(&book);
        assert_eq!(browser.kind, "Music Finder");
        assert_eq!(browser.filtered("my").len(), 1);
        assert_eq!(browser.filtered("funk").len(), 1);
        assert_eq!(browser.filtered("").len(), 2);
        assert!(browser.filtered("nothing").is_empty());
        assert_eq!(browser.rows[0].tempo, Some(68.0));
        assert_eq!(browser.rows[0].style.as_deref(), Some("Ballad.sty"));
    }

    #[test]
    fn unknown_extensions_are_rejected() {
        let path = std::env::temp_dir().join("lazy-arranger-library-test.txt");
        std::fs::write(&path, b"x").expect("write");
        assert!(load(&path).is_err());
        let _ = std::fs::remove_file(path);
    }
}
