//! Yamaha CASM rule parsing.
//!
//! A style keeps its chord rules in the `CASM` chunk. Inside it, each `CSEG`
//! names the sections it applies to (`Sdec`) and carries one rule chunk per
//! source channel: `Ctab` in SFF1 (26-byte record) or `Ctb2` in SFF2 (40-byte
//! record). Bytes after a record are vendor data and are kept verbatim.
//!
//! Two encodings are easy to miss and are handled here:
//!
//! * **SFF2 packs `bass` into the NTT byte's high bit** (`ntt | 0x80`); the
//!   oracle writes it that way (`StyleFile.cpp:178-179`).
//! * **SFF1 has no bass flag**; it uses a separate NTT value 3 for
//!   "melody, follow the slash bass" (`StyleFile.cpp:182-186`).
//!
//! Field values that the oracle would cast blindly are normalised here, and
//! every normalisation is counted in [`CasmStats`] so a caller can report it.
//! On 3,977 files (206,734 rules) the only outliers seen were `ntr = 28`,
//! `rtt = 110/128` and `ntt = 127`, two rules each.

use arranger::ChordType;
use arranger::chord_type::ALL_TYPES;
use arranger::rules::{ChordRule, Ntr, Ntt, Rtr, TransposeRange};

use crate::Chunk;

/// Which SFF generation supplied the rule tables seen so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SffVersion {
    /// SFF1: 26-byte `Ctab` records, NTT 3 means bass.
    Sff1,
    /// SFF2: 40-byte `Ctb2` records, the NTT high bit means bass.
    Sff2,
    /// Both kinds appeared in one file; each rule keeps its own encoding.
    Mixed,
    /// The container had no rule chunks at all.
    Unknown,
}

/// One `CSEG`: the section names it applies to and the rules for them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionRules {
    /// Section names as written in `Sdec`, trimmed.
    pub sections: Vec<String>,
    /// Rules in source order, one per source channel.
    pub rules: Vec<ChordRule>,
}

impl SectionRules {
    /// True when this group applies to `section`.
    #[must_use]
    pub fn applies_to(&self, section: &str) -> bool {
        self.sections.iter().any(|name| name == section)
    }
}

/// Counters for values the parser had to normalise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CasmStats {
    /// Rules with an `ntr` value the oracle does not define.
    pub ntr_clamped: usize,
    /// Rules with an `ntt` value the oracle does not define.
    pub ntt_clamped: usize,
    /// Rules with an `rtr` value the oracle does not define.
    pub rtr_clamped: usize,
    /// Rules whose source chord type was outside 0..34 and was read as a
    /// major triad, which is what the oracle's `info()` clamp produces.
    pub chord_type_clamped: usize,
    /// `CSEG` chunks without a `Sdec` section list.
    pub groups_without_sections: usize,
    /// Section names that were empty after trimming.
    pub empty_section_names: usize,
    /// Chunk lengths that reached past the end of their container and were
    /// clamped to the bytes available, as the oracle does.
    pub clamped_chunk_lengths: usize,
    /// Bytes after the last chunk header this reader could trust.
    pub unparsed_tail_bytes: usize,
    /// Rule records too short for their encoding; they were skipped.
    pub short_rules: usize,
}

/// Every rule set found in a style container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CasmRules {
    /// Rule sets in source order.
    pub groups: Vec<SectionRules>,
    /// Encoding seen across the file's rule chunks.
    pub version: SffVersion,
    /// True when the container carried a `CASM` chunk at all.
    pub has_casm: bool,
    /// Normalisations applied while reading.
    pub stats: CasmStats,
}

impl CasmRules {
    /// Rules that apply to a section, in source order.
    pub fn rules_for(&self, section: &str) -> Vec<&ChordRule> {
        self.groups
            .iter()
            .filter(|group| group.applies_to(section))
            .flat_map(|group| group.rules.iter())
            .collect()
    }

    /// All section names in first-seen order.
    #[must_use]
    pub fn section_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for group in &self.groups {
            for name in &group.sections {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
    }

    /// Total rule count across groups.
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.groups.iter().map(|group| group.rules.len()).sum()
    }
}

/// Reads every rule set from a style container.
///
/// Never fails. A damaged file yields the rules that could still be read, and
/// every recovery is counted in [`CasmStats`]: overlong chunk lengths are
/// clamped (as the oracle does), a trailing fragment is counted in
/// `unparsed_tail_bytes`, and short rules are skipped.
pub fn read_rules(bytes: &[u8]) -> CasmRules {
    let mut stats = CasmStats::default();
    let chunks = walk_container(bytes, &mut stats);
    let mut groups = Vec::new();
    let mut saw_sff1 = false;
    let mut saw_sff2 = false;
    let has_casm = chunks.iter().any(|chunk| &chunk.id == b"CASM");

    for chunk in chunks.iter().filter(|chunk| &chunk.id == b"CASM") {
        let payload = chunk.payload(bytes).unwrap_or(&[]);
        for cseg in scan_children(payload, chunk.payload_offset(), &mut stats) {
            if &cseg.id != b"CSEG" {
                continue;
            }
            let body = cseg.payload(bytes).unwrap_or(&[]);
            let mut group = SectionRules {
                sections: Vec::new(),
                rules: Vec::new(),
            };
            for child in scan_children(body, cseg.payload_offset(), &mut stats) {
                let data = child.payload(bytes).unwrap_or(&[]);
                match &child.id {
                    b"Sdec" => {
                        for name in data.split(|byte| *byte == b',') {
                            let trimmed = trim_name(name);
                            if trimmed.is_empty() {
                                stats.empty_section_names += 1;
                            } else {
                                group.sections.push(trimmed);
                            }
                        }
                    }
                    b"Ctab" => {
                        saw_sff1 = true;
                        if let Some(rule) = decode_rule(data, false, &mut stats) {
                            group.rules.push(rule);
                        }
                    }
                    b"Ctb2" => {
                        saw_sff2 = true;
                        if let Some(rule) = decode_rule(data, true, &mut stats) {
                            group.rules.push(rule);
                        }
                    }
                    _ => {}
                }
            }
            if group.sections.is_empty() {
                stats.groups_without_sections += 1;
            }
            if !group.rules.is_empty() || !group.sections.is_empty() {
                groups.push(group);
            }
        }
    }

    let version = match (saw_sff1, saw_sff2) {
        (true, true) => SffVersion::Mixed,
        (true, false) => SffVersion::Sff1,
        (false, true) => SffVersion::Sff2,
        (false, false) => SffVersion::Unknown,
    };
    CasmRules {
        groups,
        version,
        has_casm,
        stats,
    }
}

/// Walks top-level chunks, clamping overlong lengths and stopping at the first
/// header that cannot be trusted.
fn walk_container(bytes: &[u8], stats: &mut CasmStats) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        // Some writers pad an odd-length chunk with one zero byte before the
        // next identifier.
        let mut padding = 0;
        while offset + 4 <= bytes.len()
            && bytes[offset] == 0
            && !bytes[offset..offset + 4].iter().all(u8::is_ascii_graphic)
            && padding < 3
        {
            offset += 1;
            padding += 1;
        }
        if offset + 8 > bytes.len() {
            stats.unparsed_tail_bytes += bytes.len() - offset;
            break;
        }
        let mut id = [0u8; 4];
        id.copy_from_slice(&bytes[offset..offset + 4]);
        if !id.iter().all(|byte| byte.is_ascii_graphic()) {
            stats.unparsed_tail_bytes += bytes.len() - offset;
            break;
        }
        let declared = u32::from_be_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let available = bytes.len() - (offset + 8);
        let clamped = declared > available;
        if clamped {
            stats.clamped_chunk_lengths += 1;
        }
        let len = declared.min(available);
        out.push(Chunk { id, offset, len });
        offset += 8 + len;
        if clamped {
            // Nothing can be trusted after an overrun.
            break;
        }
    }
    out
}

/// Walks a chunk payload, clamping overlong lengths and stopping at the first
/// header that cannot be trusted. Offsets are reported as file offsets.
fn scan_children(payload: &[u8], payload_offset: usize, stats: &mut CasmStats) -> Vec<Chunk> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    while offset < payload.len() {
        let mut padding = 0;
        while offset + 4 <= payload.len()
            && payload[offset] == 0
            && !payload[offset..offset + 4].iter().all(u8::is_ascii_graphic)
            && padding < 3
        {
            offset += 1;
            padding += 1;
        }
        if offset + 8 > payload.len() {
            break;
        }
        let mut id = [0u8; 4];
        id.copy_from_slice(&payload[offset..offset + 4]);
        if !id.iter().all(|byte| byte.is_ascii_graphic()) {
            break;
        }
        let declared = u32::from_be_bytes([
            payload[offset + 4],
            payload[offset + 5],
            payload[offset + 6],
            payload[offset + 7],
        ]) as usize;
        let available = payload.len() - (offset + 8);
        let clamped = declared > available;
        if clamped {
            stats.clamped_chunk_lengths += 1;
        }
        let len = declared.min(available);
        out.push(Chunk {
            id,
            offset: payload_offset + offset,
            len,
        });
        offset += 8 + len;
        if clamped {
            break;
        }
    }
    out
}

/// Trims spaces and NULs, decoding as UTF-8 when possible.
fn trim_name(bytes: &[u8]) -> String {
    let start = bytes
        .iter()
        .position(|byte| *byte != b' ' && *byte != 0)
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| *byte != b' ' && *byte != 0)
        .map_or(start, |index| index + 1);
    String::from_utf8_lossy(&bytes[start..end]).into_owned()
}

/// Decodes one `Ctab` (SFF1) or `Ctb2` (SFF2) record.
fn decode_rule(body: &[u8], sff2: bool, stats: &mut CasmStats) -> Option<ChordRule> {
    let needed = if sff2 { 40 } else { 26 };
    if body.len() < needed {
        stats.short_rules += 1;
        return None;
    }
    let mut rule = ChordRule {
        source_channel: (body[0] & 0x0f) + 1,
        name: trim_name(&body[1..9]),
        dest_channel: (body[9] & 0x0f) + 1,
        editable: body[10] == 0,
        note_mute: u16::from_be_bytes([body[11], body[12]]) & 0x0fff,
        chord_mute: u64::from_be_bytes([0, 0, 0, body[13], body[14], body[15], body[16], body[17]])
            & 0x00ff_ffff_ffff,
        source_root: body[18] % 12,
        source_chord_type: chord_type(body[19], stats),
        ..ChordRule::default()
    };
    if sff2 {
        rule.middle_low = body[20];
        rule.middle_high = body[21];
        rule.low_range = decode_range(&body[22..28], stats);
        rule.transpose = decode_range(&body[28..34], stats);
        rule.high_range = decode_range(&body[34..40], stats);
    } else {
        rule.transpose = decode_range_sff1(&body[20..26], stats);
    }
    rule.special = body[needed..].to_vec();
    Some(rule)
}

fn chord_type(value: u8, stats: &mut CasmStats) -> ChordType {
    ChordType::from_index(usize::from(value)).unwrap_or_else(|_| {
        // The oracle clamps an out-of-range source type to 34, whose table row
        // is identical to the major triad's. One file in the local library
        // (seven rules) stores garbage here.
        stats.chord_type_clamped += 1;
        ChordType::Major
    })
}

/// Decodes one 6-byte SFF2 range: `ntr, ntt|bass, highKey, low, high, rtr`.
fn decode_range(bytes: &[u8], stats: &mut CasmStats) -> TransposeRange {
    let ntr = decode_ntr(bytes[0], stats);
    let packed = bytes[1];
    let bass = packed & 0x80 != 0;
    let ntt = decode_ntt_sff2(packed & 0x7f, stats);
    TransposeRange {
        ntr,
        ntt,
        bass,
        high_key: bytes[2],
        low_limit: bytes[3],
        high_limit: bytes[4],
        rtr: decode_rtr(bytes[5], stats),
    }
}

/// Decodes the single 6-byte SFF1 range; NTT 3 means melody with bass.
fn decode_range_sff1(bytes: &[u8], stats: &mut CasmStats) -> TransposeRange {
    let ntr = decode_ntr(bytes[0], stats);
    let (ntt, bass) = match bytes[1] {
        0 => (Ntt::Bypass, false),
        1 => (Ntt::Melody, false),
        3 => (Ntt::Melody, true),
        4 => (Ntt::MelodicMinor, false),
        5 => (Ntt::HarmonicMinor, false),
        // 2 and anything unknown behave as chord in the oracle.
        _ => (Ntt::Chord, false),
    };
    TransposeRange {
        ntr,
        ntt,
        bass,
        high_key: bytes[2],
        low_limit: bytes[3],
        high_limit: bytes[4],
        rtr: decode_rtr(bytes[5], stats),
    }
}

fn decode_ntr(value: u8, stats: &mut CasmStats) -> Ntr {
    match value {
        0 => Ntr::RootTransposition,
        1 => Ntr::RootFixed,
        2 => Ntr::Guitar,
        // The oracle treats any other value as "not root transposition".
        _ => {
            stats.ntr_clamped += 1;
            Ntr::RootFixed
        }
    }
}

fn decode_ntt_sff2(value: u8, stats: &mut CasmStats) -> Ntt {
    match value {
        0 => Ntt::Bypass,
        1 => Ntt::Melody,
        2 => Ntt::Chord,
        3 => Ntt::MelodicMinor,
        4 => Ntt::MelodicMinor5th,
        5 => Ntt::HarmonicMinor,
        6 => Ntt::HarmonicMinor5th,
        7 => Ntt::NaturalMinor,
        8 => Ntt::NaturalMinor5th,
        9 => Ntt::Dorian,
        10 => Ntt::Dorian5th,
        // The oracle's melody path catches every other value.
        _ => {
            stats.ntt_clamped += 1;
            Ntt::Melody
        }
    }
}

fn decode_rtr(value: u8, stats: &mut CasmStats) -> Rtr {
    match value {
        0 => Rtr::Stop,
        1 => Rtr::PitchShift,
        2 => Rtr::PitchShiftToRoot,
        3 => Rtr::Retrigger,
        4 => Rtr::RetriggerToRoot,
        5 => Rtr::NoteGenerator,
        _ => {
            stats.rtr_clamped += 1;
            // The oracle clamps to the last variant when it reads the file.
            Rtr::NoteGenerator
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a container: MThd + CASM(CSEG(Sdec, rule chunks)).
    fn container(rule_chunks: &[(&[u8; 4], Vec<u8>)], sections: &str) -> Vec<u8> {
        let mut cseg = Vec::new();
        let mut sdec = Vec::new();
        sdec.extend_from_slice(b"Sdec");
        sdec.extend_from_slice(&u32::try_from(sections.len()).expect("small").to_be_bytes());
        sdec.extend_from_slice(sections.as_bytes());
        cseg.extend_from_slice(&sdec);
        for (id, body) in rule_chunks {
            cseg.extend_from_slice(*id);
            cseg.extend_from_slice(&u32::try_from(body.len()).expect("small").to_be_bytes());
            cseg.extend_from_slice(body);
        }
        let mut casm = Vec::new();
        casm.extend_from_slice(b"CSEG");
        casm.extend_from_slice(&u32::try_from(cseg.len()).expect("small").to_be_bytes());
        casm.extend_from_slice(&cseg);

        let mut out = Vec::new();
        out.extend_from_slice(b"MThd");
        out.extend_from_slice(&6u32.to_be_bytes());
        out.extend_from_slice(&[0, 1, 0, 1, 0x03, 0xC0]);
        out.extend_from_slice(b"CASM");
        out.extend_from_slice(&u32::try_from(casm.len()).expect("small").to_be_bytes());
        out.extend_from_slice(&casm);
        out
    }

    fn sff1_rule() -> Vec<u8> {
        let mut rule = vec![0u8; 26];
        rule[0] = 10; // source channel 11
        rule[1..7].copy_from_slice(b"Aco.Ba");
        rule[7] = b's';
        rule[8] = b's';
        rule[9] = 10; // dest channel 11
        rule[10] = 0; // editable
        rule[11..13].copy_from_slice(&0x0fffu16.to_be_bytes());
        rule[13..18].copy_from_slice(&[0x03, 0xff, 0xff, 0xff, 0xff]);
        rule[18] = 0; // root C
        rule[19] = 2; // major 7
        rule[20] = 0; // ntr root transposition
        rule[21] = 3; // melody with bass
        rule[22] = 6; // highKey
        rule[23] = 28;
        rule[24] = 60;
        rule[25] = 2; // pitch shift to root
        rule.push(0xAA); // vendor tail
        rule
    }

    fn sff2_rule() -> Vec<u8> {
        let mut rule = vec![0u8; 40];
        rule[0] = 0; // source channel 1
        rule[1..4].copy_from_slice(b"Pad");
        rule[9] = 12; // dest channel 13
        rule[10] = 1; // not editable
        rule[11..13].copy_from_slice(&0x0fffu16.to_be_bytes());
        rule[13..18].copy_from_slice(&[0x00, 0x3f, 0xf8, 0x00, 0x00]);
        rule[18] = 11; // root B
        rule[19] = 19; // dominant seventh
        rule[20] = 48;
        rule[21] = 60;
        // low range: ntr guitar, ntt melody + bass, highKey 4, limits 20..80, rtr retrigger
        rule[22..28].copy_from_slice(&[2, 0x81, 4, 20, 80, 3]);
        // middle: ntr root fixed, ntt chord, highKey 6, limits 0..127, rtr pitch shift
        rule[28..34].copy_from_slice(&[1, 2, 6, 0, 127, 1]);
        // high: ntr root transposition, ntt bypass, highKey 6, limits 0..127, rtr stop
        rule[34..40].copy_from_slice(&[0, 0, 6, 0, 127, 0]);
        rule.push(0); // vendor tail byte
        rule
    }

    #[test]
    fn reads_an_sff1_rule_with_bass_and_tail() {
        let bytes = container(&[(b"Ctab", sff1_rule())], "Main A, Fill In AA");
        let rules = read_rules(&bytes);
        assert!(rules.has_casm);
        assert_eq!(rules.version, SffVersion::Sff1);
        assert_eq!(rules.groups.len(), 1);
        assert_eq!(
            rules.groups[0].sections,
            vec![String::from("Main A"), String::from("Fill In AA")]
        );
        let rule = &rules.groups[0].rules[0];
        assert_eq!(rule.source_channel, 11);
        assert_eq!(rule.dest_channel, 11);
        assert_eq!(rule.name, "Aco.Bass");
        assert!(rule.editable);
        assert_eq!(rule.note_mute, 0x0fff);
        assert_eq!(rule.chord_mute, 0x0003_ffff_ffff);
        assert_eq!(rule.source_root, 0);
        assert_eq!(rule.source_chord_type, ChordType::Major7);
        assert_eq!(rule.transpose.ntr, Ntr::RootTransposition);
        assert_eq!(rule.transpose.ntt, Ntt::Melody);
        assert!(rule.transpose.bass, "SFF1 NTT 3 means bass");
        assert_eq!(rule.transpose.high_key, 6);
        assert_eq!(rule.transpose.low_limit, 28);
        assert_eq!(rule.transpose.high_limit, 60);
        assert_eq!(rule.transpose.rtr, Rtr::PitchShiftToRoot);
        assert_eq!(rule.special, vec![0xAA]);
        assert_eq!(rules.stats, CasmStats::default());
    }

    #[test]
    fn reads_an_sff2_rule_with_three_ranges() {
        let bytes = container(&[(b"Ctb2", sff2_rule())], "Main B, Fill In BB");
        let rules = read_rules(&bytes);
        assert_eq!(rules.version, SffVersion::Sff2);
        let rule = &rules.groups[0].rules[0];
        assert_eq!(rule.source_channel, 1);
        assert_eq!(rule.dest_channel, 13);
        assert_eq!(rule.name, "Pad");
        assert!(!rule.editable);
        assert_eq!(
            rule.chord_mute,
            u64::from_be_bytes([0, 0, 0, 0x00, 0x3f, 0xf8, 0x00, 0x00])
        );
        assert_eq!(rule.middle_low, 48);
        assert_eq!(rule.middle_high, 60);
        assert_eq!(rule.low_range.ntr, Ntr::Guitar);
        assert_eq!(rule.low_range.ntt, Ntt::Melody);
        assert!(
            rule.low_range.bass,
            "the high bit of the NTT byte means bass"
        );
        assert_eq!(rule.low_range.high_key, 4);
        assert_eq!(rule.low_range.low_limit, 20);
        assert_eq!(rule.low_range.high_limit, 80);
        assert_eq!(rule.low_range.rtr, Rtr::Retrigger);
        assert_eq!(rule.transpose.ntr, Ntr::RootFixed);
        assert_eq!(rule.transpose.ntt, Ntt::Chord);
        assert!(!rule.transpose.bass);
        assert_eq!(rule.high_range.ntt, Ntt::Bypass);
        assert_eq!(rule.high_range.rtr, Rtr::Stop);
        assert_eq!(rule.special, vec![0x00]);
    }

    #[test]
    fn groups_map_sections_to_rules() {
        let bytes = container(&[(b"Ctb2", sff2_rule())], "Main A");
        let rules = read_rules(&bytes);
        assert_eq!(rules.rules_for("Main A").len(), 1);
        assert!(rules.rules_for("Main B").is_empty());
        assert_eq!(rules.section_names(), vec![String::from("Main A")]);
        assert_eq!(rules.rule_count(), 1);
    }

    #[test]
    fn unknown_enum_values_are_normalised_and_counted() {
        let mut body = sff2_rule();
        body[22] = 28; // unknown ntr
        body[23] = 127; // unknown ntt
        body[27] = 110; // unknown rtr
        let bytes = container(&[(b"Ctb2", body)], "Main A");
        let rules = read_rules(&bytes);
        let range = &rules.groups[0].rules[0].low_range;
        assert_eq!(range.ntr, Ntr::RootFixed);
        assert_eq!(range.ntt, Ntt::Melody);
        assert_eq!(range.rtr, Rtr::NoteGenerator);
        assert_eq!(rules.stats.ntr_clamped, 1);
        assert_eq!(rules.stats.ntt_clamped, 1);
        assert_eq!(rules.stats.rtr_clamped, 1);
    }

    #[test]
    fn sff1_ntt_values_map_as_the_oracle_does() {
        let cases = [
            (0u8, Ntt::Bypass, false),
            (1, Ntt::Melody, false),
            (2, Ntt::Chord, false),
            (3, Ntt::Melody, true),
            (4, Ntt::MelodicMinor, false),
            (5, Ntt::HarmonicMinor, false),
            (9, Ntt::Chord, false),
        ];
        for (value, ntt, bass) in cases {
            let mut body = sff1_rule();
            body[21] = value;
            let bytes = container(&[(b"Ctab", body)], "Main A");
            let rules = read_rules(&bytes);
            let range = &rules.groups[0].rules[0].transpose;
            assert_eq!(range.ntt, ntt, "SFF1 NTT {value}");
            assert_eq!(range.bass, bass, "SFF1 NTT {value} bass");
        }
    }

    #[test]
    fn section_names_are_trimmed_and_split() {
        let bytes = container(&[(b"Ctb2", sff2_rule())], " Main A ,Fill In AA, ");
        let rules = read_rules(&bytes);
        assert_eq!(
            rules.groups[0].sections,
            vec![String::from("Main A"), String::from("Fill In AA")]
        );
        assert_eq!(rules.stats.empty_section_names, 1);
    }

    #[test]
    fn a_short_rule_is_skipped_and_counted() {
        let mut body = sff2_rule();
        body.truncate(39);
        let bytes = container(&[(b"Ctb2", body)], "Main A");
        let rules = read_rules(&bytes);
        assert!(rules.groups[0].rules.is_empty());
        assert_eq!(rules.stats.short_rules, 1);
    }

    #[test]
    fn trailing_garbage_is_counted_and_earlier_rules_survive() {
        let mut bytes = container(&[(b"Ctb2", sff2_rule())], "Main A");
        bytes.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00, 0x20]);
        let rules = read_rules(&bytes);
        assert_eq!(rules.groups[0].rules.len(), 1);
        assert!(rules.stats.unparsed_tail_bytes > 0);
    }

    #[test]
    fn an_overlong_chunk_length_is_clamped() {
        let mut bytes = container(&[(b"Ctb2", sff2_rule())], "Main A");
        // Make the CASM chunk claim far more bytes than the file holds.
        let casm = bytes
            .windows(4)
            .position(|window| window == b"CASM")
            .expect("CASM present");
        let declared = u32::try_from(bytes.len()).expect("small file");
        bytes[casm + 4..casm + 8].copy_from_slice(&declared.to_be_bytes());
        let rules = read_rules(&bytes);
        assert_eq!(rules.stats.clamped_chunk_lengths, 1);
        assert_eq!(rules.groups[0].rules.len(), 1);
    }

    #[test]
    fn an_unknown_source_chord_type_is_clamped_and_counted() {
        let mut body = sff2_rule();
        body[19] = 200;
        let bytes = container(&[(b"Ctb2", body)], "Main A");
        let rules = read_rules(&bytes);
        assert_eq!(rules.groups[0].rules[0].source_chord_type, ChordType::Major);
        assert_eq!(rules.stats.chord_type_clamped, 1);
    }

    #[test]
    fn a_container_without_casm_yields_no_groups() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MThd");
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(&[0, 1, 0, 1, 0x03, 0xC0]);
        bytes.extend_from_slice(b"MTrk");
        bytes.extend_from_slice(&4u32.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        let rules = read_rules(&bytes);
        assert!(rules.groups.is_empty());
        assert_eq!(rules.rule_count(), 0);
        assert!(!rules.has_casm);
        assert_eq!(rules.version, SffVersion::Unknown);
    }

    #[test]
    fn mixed_records_report_mixed_versions() {
        let bytes = container(&[(b"Ctab", sff1_rule()), (b"Ctb2", sff2_rule())], "Main A");
        let rules = read_rules(&bytes);
        assert_eq!(rules.version, SffVersion::Mixed);
        assert_eq!(rules.groups[0].rules.len(), 2);
    }
}

/// Encodes rules as an SFF1 `CASM` chunk (`CSEG` with `Sdec` and `Ctab`).
///
/// Only values representable in SFF1 are encoded exactly: NTT types other than
/// bypass, melody, chord, melodic minor and harmonic minor become melody, and
/// names are truncated to eight bytes. Rules keep their vendor tail.
#[must_use]
pub fn write_casm_sff1(groups: &[SectionRules]) -> Vec<u8> {
    let mut cseg_children = Vec::new();
    let mut rules = Vec::new();
    for group in groups {
        let names = group.sections.join(",");
        cseg_children.extend(chunk(b"Sdec", names.as_bytes()));
        for rule in &group.rules {
            rules.extend(encode_rule_sff1(rule));
        }
    }
    if !rules.is_empty() {
        cseg_children.extend(chunk(b"Ctab", &rules));
    }
    chunk(b"CASM", &chunk(b"CSEG", &cseg_children))
}

fn chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len() + 1);
    out.extend_from_slice(id);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn encode_rule_sff1(rule: &ChordRule) -> Vec<u8> {
    let mut out = vec![0_u8; 26];
    out[0] = (rule.source_channel.saturating_sub(1)) & 0x0f;
    for (index, byte) in rule.name.as_bytes().iter().take(8).enumerate() {
        out[1 + index] = *byte;
    }
    out[9] = (rule.dest_channel.saturating_sub(1)) & 0x0f;
    out[10] = u8::from(!rule.editable);
    out[11..13].copy_from_slice(&(rule.note_mute & 0x0fff).to_be_bytes());
    let mute = (rule.chord_mute & 0x00ff_ffff_ffff).to_be_bytes();
    out[13..18].copy_from_slice(&mute[3..8]);
    out[18] = rule.source_root % 12;
    out[19] = u8::try_from(
        ALL_TYPES
            .iter()
            .position(|kind| *kind == rule.source_chord_type)
            .unwrap_or(0),
    )
    .unwrap_or(0);
    let range = &rule.transpose;
    out[20] = match range.ntr {
        Ntr::RootTransposition => 0,
        Ntr::RootFixed => 1,
        Ntr::Guitar => 2,
    };
    out[21] = match (range.ntt, range.bass) {
        (Ntt::Bypass, _) => 0,
        (Ntt::Melody, true) => 3,
        (Ntt::Melody, false) => 1,
        (Ntt::MelodicMinor, _) => 4,
        (Ntt::HarmonicMinor, _) => 5,
        _ => 1,
    };
    out[22] = range.high_key;
    out[23] = range.low_limit;
    out[24] = range.high_limit;
    out[25] = match range.rtr {
        Rtr::Stop => 0,
        Rtr::PitchShift => 1,
        Rtr::PitchShiftToRoot => 2,
        Rtr::Retrigger => 3,
        Rtr::RetriggerToRoot => 4,
        Rtr::NoteGenerator => 5,
    };
    out.extend_from_slice(&rule.special);
    out
}

#[cfg(test)]
mod writer_tests {
    use super::*;

    #[test]
    fn an_sff1_group_round_trips_through_the_reader() {
        let mut rule = ChordRule::new(11, 11);
        rule.name = String::from("Bass");
        rule.editable = true;
        rule.note_mute = 0x0f0f;
        rule.chord_mute = 0x0000_00ff_ffff;
        rule.source_root = 0;
        rule.source_chord_type = ChordType::Major7;
        rule.transpose.ntr = Ntr::RootFixed;
        rule.transpose.ntt = Ntt::Melody;
        rule.transpose.bass = true;
        rule.transpose.high_key = 6;
        rule.transpose.low_limit = 0;
        rule.transpose.high_limit = 127;
        rule.transpose.rtr = Rtr::PitchShiftToRoot;
        let group = SectionRules {
            sections: vec![String::from("Main A"), String::from("Fill A")],
            rules: vec![rule.clone()],
        };
        let bytes = write_casm_sff1(&[group]);
        let parsed = read_rules(&bytes);
        assert!(parsed.has_casm);
        assert_eq!(parsed.version, SffVersion::Sff1);
        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(parsed.groups[0].sections, vec!["Main A", "Fill A"]);
        let read = &parsed.groups[0].rules[0];
        assert_eq!(read.source_channel, rule.source_channel);
        assert_eq!(read.dest_channel, rule.dest_channel);
        assert_eq!(read.name, rule.name);
        assert_eq!(read.source_chord_type, rule.source_chord_type);
        assert_eq!(read.transpose.ntr, rule.transpose.ntr);
        assert_eq!(read.transpose.ntt, rule.transpose.ntt);
        assert!(read.transpose.bass);
        assert_eq!(read.transpose.rtr, rule.transpose.rtr);
        assert_eq!(read.special, rule.special);
    }
}
