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

use arranger::{ArrangerChord, Ntr, Ntt, transpose};
use music_core::PitchClass;
use style_formats::casm::read_rules;

const EXTENSIONS: [&str; 8] = ["sty", "prs", "sst", "pst", "bcs", "pcs", "scp", "fps"];

#[derive(Default)]
struct Counts {
    files: usize,
    with_rules: usize,
    without_casm: usize,
    recovered: usize,
    rules: usize,
    identity_checked: usize,
    identity_failures: usize,
    limit_failures: usize,
    imported: usize,
    sections: usize,
    notes: usize,
    samples: Vec<String>,
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
        check_rules(&parsed, counts);
        if let Ok(imported) = style_formats::import_style(&bytes) {
            counts.imported += 1;
            counts.sections += imported.sections.len();
            counts.notes += imported
                .sections
                .iter()
                .map(|section| section.notes)
                .sum::<usize>();
        }
    }
}

/// At a rule's own source chord, conversion must not move a note unless the
/// selected range folds it or the rule is guitar fingering.
///
/// Pitches 12..=115 are used: the nearest-note search walks past the MIDI
/// range at the edges and clamps, which is the oracle's behaviour, not a
/// conversion difference.
fn check_rules(rules: &style_formats::CasmRules, counts: &mut Counts) {
    for group in &rules.groups {
        for rule in &group.rules {
            let chord = ArrangerChord::new(
                PitchClass::new(i32::from(rule.source_root)),
                rule.source_chord_type,
            );
            for pitch in 12u8..=115 {
                let range = rule.range_for(pitch);
                let Some(result) = transpose(pitch, rule, &chord) else {
                    counts.identity_failures += 1;
                    counts.identity_checked += 1;
                    continue;
                };
                if range.ntr == Ntr::Guitar {
                    continue;
                }
                counts.identity_checked += 1;
                // Limits above the MIDI range cannot be honoured: the final
                // clamp wins. Compare against the reachable window.
                let low = range.low_limit.min(127);
                let high = range.high_limit.min(127);
                let folds = low <= high && high - low >= 11;
                // The oracle returns a bypassed note before it applies limits,
                // so a bypass range may not honour them.
                if folds && range.ntt != Ntt::Bypass && (result < low || result > high) {
                    counts.limit_failures += 1;
                    if counts.samples.len() < 5 {
                        counts.samples.push(format!(
                            "pitch={pitch} result={result} ntr={:?} ntt={:?} limits={}..{} hk={} src={} type={}",
                            range.ntr,
                            range.ntt,
                            range.low_limit,
                            range.high_limit,
                            range.high_key,
                            rule.source_root,
                            rule.source_chord_type.suffix(),
                        ));
                    }
                }
                // Identity is only required where nothing may legitimately
                // move the note: full limits, no guitar snapping.
                if result != pitch && range.low_limit == 0 && range.high_limit == 127 {
                    counts.identity_failures += 1;
                }
            }
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
    println!(
        "imported={} sections={} notes={} identity_checked={} identity_failures={} limit_failures={}",
        counts.imported,
        counts.sections,
        counts.notes,
        counts.identity_checked,
        counts.identity_failures,
        counts.limit_failures
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
    assert_eq!(
        counts.identity_failures, 0,
        "conversion moved notes at their own source chord"
    );
    assert_eq!(
        counts.limit_failures, 0,
        "conversion left notes outside the rule's limits: {:?}",
        counts.samples
    );
    assert!(
        counts.imported * 100 >= counts.with_rules * 90,
        "fewer than 90% of rule-bearing styles imported: {} of {}",
        counts.imported,
        counts.with_rules
    );
}
