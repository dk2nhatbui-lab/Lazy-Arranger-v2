//! Developer tool: parse every style under a directory and report the result.
//!
//! ```sh
//! cargo run -p style-formats --example casm-scan -- "/Volumes/filer/_Yamaha PSR/Styles"
//! ```
//!
//! It reads files, prints one summary plus the first few failures, and exits
//! non-zero when anything failed. It is not part of the product; it exists so a
//! parser change can be checked against a real library in one command.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use style_formats::SffVersion;
use style_formats::casm::{CasmStats, read_rules};

const STYLE_EXTENSIONS: [&str; 8] = ["sty", "prs", "sst", "pst", "bcs", "pcs", "scp", "fps"];

#[derive(Default)]
struct Totals {
    files: usize,
    parsed: usize,
    no_casm: usize,
    failed: usize,
    groups: usize,
    rules: usize,
    stats: CasmStats,
    partial: usize,
    empty_casm: usize,
    versions: BTreeMap<&'static str, usize>,
    failures: Vec<String>,
    failure_dirs: BTreeMap<String, usize>,
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(target) = args.next() else {
        eprintln!("usage: casm-scan <directory> | casm-scan <file> --detail");
        return ExitCode::FAILURE;
    };
    let root = PathBuf::from(target);
    if args.next().as_deref() == Some("--detail") {
        return detail(&root);
    }
    if !root.is_dir() {
        eprintln!("not a directory: {}", root.display());
        return ExitCode::FAILURE;
    }
    let mut totals = Totals::default();
    walk(&root, &root, &mut totals);
    report(&totals);
    if totals.failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Prints every rule of one file, for spot checks against other tools.
fn detail(path: &Path) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let rules = read_rules(&bytes);
    println!(
        "{}: {:?}, {} group(s)",
        path.display(),
        rules.version,
        rules.groups.len()
    );
    for (index, group) in rules.groups.iter().enumerate() {
        println!("  group {index}: {:?}", group.sections);
        for rule in &group.rules {
            println!(
                "    ch{:>2}->{:>2} {:<8} root={} type={} ntr={:?} ntt={:?} bass={} hk={} limits={}..{} rtr={:?} tail={}",
                rule.source_channel,
                rule.dest_channel,
                rule.name,
                rule.source_root,
                rule.source_chord_type.suffix(),
                rule.transpose.ntr,
                rule.transpose.ntt,
                rule.transpose.bass,
                rule.transpose.high_key,
                rule.transpose.low_limit,
                rule.transpose.high_limit,
                rule.transpose.rtr,
                rule.special.len()
            );
        }
    }
    println!("stats: {:?}", rules.stats);
    ExitCode::SUCCESS
}

fn walk(dir: &Path, root: &Path, totals: &mut Totals) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            totals.failures.push(format!("{}: {error}", dir.display()));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, root, totals);
        } else if is_style(&path) {
            inspect(&path, root, totals);
        }
    }
}

fn is_style(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            STYLE_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

fn inspect(path: &Path, root: &Path, totals: &mut Totals) {
    totals.files += 1;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            fail(totals, root, path, &format!("{error}"));
            return;
        }
    };
    if !bytes.starts_with(b"MThd") {
        fail(totals, root, path, "not a style container");
        return;
    }
    let rules = read_rules(&bytes);
    if rules.stats.unparsed_tail_bytes > 0
        || rules.stats.clamped_chunk_lengths > 0
        || rules.stats.short_rules > 0
    {
        totals.partial += 1;
    }
    if rules.groups.is_empty() {
        if rules.has_casm {
            totals.empty_casm += 1;
        } else {
            totals.no_casm += 1;
        }
        return;
    }
    totals.parsed += 1;
    totals.groups += rules.groups.len();
    totals.rules += rules.rule_count();
    let key = match rules.version {
        SffVersion::Sff1 => "SFF1",
        SffVersion::Sff2 => "SFF2",
        SffVersion::Mixed => "mixed",
        SffVersion::Unknown => "unknown",
    };
    *totals.versions.entry(key).or_insert(0) += 1;
    totals.stats.ntr_clamped += rules.stats.ntr_clamped;
    totals.stats.ntt_clamped += rules.stats.ntt_clamped;
    totals.stats.rtr_clamped += rules.stats.rtr_clamped;
    totals.stats.chord_type_clamped += rules.stats.chord_type_clamped;
    totals.stats.groups_without_sections += rules.stats.groups_without_sections;
    totals.stats.empty_section_names += rules.stats.empty_section_names;
    totals.stats.unparsed_tail_bytes += rules.stats.unparsed_tail_bytes;
    totals.stats.clamped_chunk_lengths += rules.stats.clamped_chunk_lengths;
    totals.stats.short_rules += rules.stats.short_rules;
}

/// Records a failure under its first two directory levels below the root.
fn fail(totals: &mut Totals, root: &Path, path: &Path, message: &str) {
    totals.failed += 1;
    let relative = path.strip_prefix(root).unwrap_or(path);
    let key: Vec<String> = relative
        .iter()
        .take(2)
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    *totals.failure_dirs.entry(key.join("/")).or_insert(0) += 1;
    if totals.failures.len() < 20 {
        totals
            .failures
            .push(format!("{}: {message}", path.display()));
    }
}

fn report(totals: &Totals) {
    println!("style files:      {}", totals.files);
    println!("with CASM:        {}", totals.parsed);
    println!("without CASM:     {}", totals.no_casm);
    println!("CASM but no rules: {}", totals.empty_casm);
    println!("failed to read:   {}", totals.failed);
    println!("recovered damage: {}", totals.partial);
    println!("section groups:   {}", totals.groups);
    println!("rules:            {}", totals.rules);
    for (version, count) in &totals.versions {
        println!("  {version}: {count}");
    }
    let stats = totals.stats;
    println!(
        "normalised: ntr={} ntt={} rtr={} source-chord-type={}",
        stats.ntr_clamped, stats.ntt_clamped, stats.rtr_clamped, stats.chord_type_clamped
    );
    println!(
        "recovered: clamped-lengths={} short-rules={}",
        stats.clamped_chunk_lengths, stats.short_rules
    );
    println!(
        "structure: groups without sections={} empty names={}",
        stats.groups_without_sections, stats.empty_section_names
    );
    println!("tail bytes unparsed: {}", totals.stats.unparsed_tail_bytes);
    if !totals.failure_dirs.is_empty() {
        println!("failure groups:");
        for (dir, count) in &totals.failure_dirs {
            println!("  {dir}: {count}");
        }
    }
    if !totals.failures.is_empty() {
        println!("first failures:");
        for failure in &totals.failures {
            println!("  {failure}");
        }
    }
}
