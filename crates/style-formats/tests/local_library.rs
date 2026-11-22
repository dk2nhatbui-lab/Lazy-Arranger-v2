//! Optional check against a real style library.
//!
//! Set `ARRANGER_STYLE_LIBRARY` to a directory of Yamaha styles to run it:
//!
//! ```sh
//! ARRANGER_STYLE_LIBRARY="/path/to/Styles" cargo test -p style-formats --test local_library
//! ```
//!
//! The test is skipped when the variable is unset, so CI needs no style data.
//! It never copies or stores a style file; it only parses them in place.

use std::path::Path;

use style_formats::casm::read_rules;

const EXTENSIONS: [&str; 8] = ["sty", "prs", "sst", "pst", "bcs", "pcs", "scp", "fps"];

#[derive(Default)]
struct Counts {
    files: usize,
    with_rules: usize,
    without_casm: usize,
    recovered: usize,
    rules: usize,
}

fn walk(dir: &Path, counts: &mut Counts) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, counts);
            continue;
        }
        let is_style = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                EXTENSIONS
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(extension))
            });
        if !is_style {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if !bytes.starts_with(b"MThd") {
            continue;
        }
        counts.files += 1;
        let parsed = read_rules(&bytes);
        if !parsed.has_casm {
            counts.without_casm += 1;
            continue;
        }
        if !parsed.groups.is_empty() {
            counts.with_rules += 1;
            counts.rules += parsed.rule_count();
        }
        if parsed.stats.clamped_chunk_lengths > 0
            || parsed.stats.short_rules > 0
            || parsed.stats.unparsed_tail_bytes > 0
        {
            counts.recovered += 1;
        }
    }
}

#[test]
fn a_local_library_yields_rules_for_most_styles() {
    let Some(root) = std::env::var_os("ARRANGER_STYLE_LIBRARY") else {
        return;
    };
    let root = Path::new(&root);
    if !root.is_dir() {
        eprintln!(
            "ARRANGER_STYLE_LIBRARY is not a directory: {}",
            root.display()
        );
        return;
    }
    let mut counts = Counts::default();
    walk(root, &mut counts);
    println!(
        "files={} with_rules={} without_casm={} recovered={} rules={}",
        counts.files, counts.with_rules, counts.without_casm, counts.recovered, counts.rules
    );
    assert!(
        counts.files > 0,
        "no style files under the configured directory"
    );
    assert!(
        counts.with_rules * 100 >= counts.files * 95,
        "fewer than 95% of files yielded rules: {} of {}",
        counts.with_rules,
        counts.files
    );
    assert!(counts.rules > 0, "no rules were read");
}
