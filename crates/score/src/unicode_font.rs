//! TrueType Unicode / Vietnamese / CJK font subsetting and PDF Type0/CIDFontType2
//! embedding for [`crate::pdf`].
//!
//! Resolves an explicit font path (or `ARRANGER_PDF_FONT`, or an installed
//! system/OFL TrueType font covering the needed Unicode characters), subsets
//! only the referenced glyphs while preserving original GIDs, and emits valid
//! PDF 1.4 `/Type0`, `/CIDFontType2`, `/FontDescriptor`, `/FontFile2`
//! (`FlateDecode`), and `/ToUnicode` CMap objects.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ttf_parser::Face;

/// Candidate system and open-source OFL Unicode TrueType font paths.
const CANDIDATE_FONT_PATHS: &[&str] = &[
    // macOS Unicode/CJK and Latin-Extended TrueType fonts
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/Library/Fonts/Arial Unicode.ttf",
    "/Library/Fonts/NotoSansCJK-Regular.ttf",
    "/Library/Fonts/NotoSans-Regular.ttf",
    "/System/Library/Fonts/Supplemental/NISC18030.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    // Linux OFL Noto / DejaVu / WenQuanYi
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    // Windows Unicode/CJK TrueType fonts
    "C:\\Windows\\Fonts\\arialuni.ttf",
    "C:\\Windows\\Fonts\\malgun.ttf",
    "C:\\Windows\\Fonts\\msyh.ttc",
    "C:\\Windows\\Fonts\\segoeui.ttf",
];

/// A loaded TrueType font prepared for incremental glyph usage tracking and
/// PDF `/Type0` subset embedding.
#[derive(Debug)]
pub struct UnicodePdfFont {
    ttf_bytes: Vec<u8>,
    postscript_name: String,
    units_per_em: u16,
    ascender: i16,
    descender: i16,
    bbox: [i16; 4],
    used_gids: BTreeSet<u16>,
    gid_to_char: BTreeMap<u16, char>,
    gid_widths: BTreeMap<u16, u16>,
}

impl UnicodePdfFont {
    /// Resolves a TrueType font covering `needed_chars`, checking `override_path`,
    /// `ARRANGER_PDF_FONT`, and standard system/OFL font paths.
    #[must_use]
    pub fn resolve(override_path: Option<&Path>, needed_chars: &[char]) -> Option<Self> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(path) = override_path {
            candidates.push(path.to_path_buf());
        }
        if let Some(env_path) = std::env::var_os("ARRANGER_PDF_FONT") {
            candidates.push(PathBuf::from(env_path));
        }
        for path in CANDIDATE_FONT_PATHS {
            candidates.push(PathBuf::from(path));
        }
        let mut best: Option<(usize, Self)> = None;
        for path in candidates {
            if !path.is_file() {
                continue;
            }
            let Ok(raw) = std::fs::read(&path) else {
                continue;
            };
            let Some(font) = Self::from_bytes(&raw) else {
                continue;
            };
            let covered = font.coverage_count(needed_chars);
            if covered == needed_chars.len() && !needed_chars.is_empty() {
                return Some(font);
            }
            if covered > best.as_ref().map_or(0, |(c, _)| *c) {
                best = Some((covered, font));
            }
        }
        best.map(|(_, font)| font)
    }

    /// Loads a TrueType font (`.ttf`, or face 0 extracted from a TrueType `.ttc`)
    /// from raw bytes. Returns `None` for CFF-only OpenType fonts lacking `glyf`.
    #[must_use]
    pub fn from_bytes(raw: &[u8]) -> Option<Self> {
        let ttf_bytes = extract_standalone_ttf(raw)?;
        let face = Face::parse(&ttf_bytes, 0).ok()?;
        let tables = parse_sfnt_tables(&ttf_bytes)?;
        if !tables.contains_key(b"glyf") || !tables.contains_key(b"loca") {
            return None;
        }
        let ps_raw = face
            .names()
            .into_iter()
            .find(|name| name.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
            .and_then(|name| name.to_string())
            .unwrap_or_else(|| String::from("UnicodeFont"));
        let postscript_name: String = ps_raw
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
            .take(32)
            .collect();
        let postscript_name = if postscript_name.is_empty() {
            String::from("UnicodeFont")
        } else {
            postscript_name
        };
        let units_per_em = face.units_per_em().max(1);
        let ascender = face.ascender();
        let descender = face.descender();
        let bbox = face.global_bounding_box();
        let mut used_gids = BTreeSet::new();
        used_gids.insert(0);
        Some(Self {
            ttf_bytes,
            postscript_name,
            units_per_em,
            ascender,
            descender,
            bbox: [bbox.x_min, bbox.y_min, bbox.x_max, bbox.y_max],
            used_gids,
            gid_to_char: BTreeMap::new(),
            gid_widths: BTreeMap::new(),
        })
    }

    /// Counts how many characters in `chars` have a non-zero glyph in this font.
    #[must_use]
    pub fn coverage_count(&self, chars: &[char]) -> usize {
        let Ok(face) = Face::parse(&self.ttf_bytes, 0) else {
            return 0;
        };
        chars
            .iter()
            .filter(|&&ch| face.glyph_index(ch).is_some_and(|gid| gid.0 != 0))
            .count()
    }

    /// Encodes `text` as an `<HEX>` CID/GID string for `/Identity-H` PDF text
    /// operators and records every referenced glyph for subsetting and `/ToUnicode`.
    pub fn encode_hex(&mut self, text: &str) -> String {
        let Ok(face) = Face::parse(&self.ttf_bytes, 0) else {
            return String::from("<>");
        };
        let upem = f64::from(self.units_per_em);
        let mut out = String::with_capacity(text.len() * 4 + 2);
        out.push('<');
        for ch in text.chars() {
            let gid = face.glyph_index(ch).map_or(0, |id| id.0);
            self.used_gids.insert(gid);
            if gid != 0 {
                self.gid_to_char.entry(gid).or_insert(ch);
            }
            self.gid_widths.entry(gid).or_insert_with(|| {
                let raw_w = face
                    .glyph_hor_advance(ttf_parser::GlyphId(gid))
                    .unwrap_or(self.units_per_em / 2);
                ((f64::from(raw_w) * 1000.0) / upem)
                    .round()
                    .clamp(0.0, 2000.0) as u16
            });
            let _ = write!(out, "{gid:04X}");
        }
        out.push('>');
        out
    }

    /// True if at least one non-zero glyph was encoded.
    #[must_use]
    pub fn has_used_glyphs(&self) -> bool {
        self.used_gids.len() > 1
    }

    /// Emits the five PDF objects for `/F2` starting at `base_object_id`:
    /// 0: Type0 font (`base_id`)
    /// 1: CIDFontType2 (`base_id + 1`)
    /// 2: FontDescriptor (`base_id + 2`)
    /// 3: Compressed subsetted TrueType `/FontFile2` stream (`base_id + 3`)
    /// 4: `/ToUnicode` CMap stream (`base_id + 4`)
    #[must_use]
    pub fn build_pdf_objects(&self, base_id: usize) -> Vec<Vec<u8>> {
        let cid_id = base_id + 1;
        let desc_id = base_id + 2;
        let file_id = base_id + 3;
        let cmap_id = base_id + 4;
        let base_font = format!("LAZSUB+{}", self.postscript_name);

        let subset = subset_ttf_preserving_gids(&self.ttf_bytes, &self.used_gids)
            .unwrap_or_else(|| self.ttf_bytes.clone());
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&subset, 6);

        let upem = f64::from(self.units_per_em);
        let scale = |v: i16| ((f64::from(v) * 1000.0) / upem).round() as i32;
        let asc = scale(self.ascender);
        let desc = scale(self.descender);
        let bbox = [
            scale(self.bbox[0]),
            scale(self.bbox[1]),
            scale(self.bbox[2]),
            scale(self.bbox[3]),
        ];

        let mut widths = String::new();
        for (&gid, &w) in &self.gid_widths {
            let _ = write!(widths, "{gid} [{w}] ");
        }

        let type0 = format!(
            "<< /Type /Font /Subtype /Type0 /BaseFont /{base_font} /Encoding /Identity-H /DescendantFonts [{cid_id} 0 R] /ToUnicode {cmap_id} 0 R >>"
        )
        .into_bytes();

        let cid_font = format!(
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{base_font} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {desc_id} 0 R /DW 1000 /W [{widths}] /CIDToGIDMap /Identity >>"
        )
        .into_bytes();

        let descriptor = format!(
            "<< /Type /FontDescriptor /FontName /{base_font} /Flags 32 /FontBBox [{} {} {} {}] /ItalicAngle 0 /Ascent {asc} /Descent {desc} /CapHeight {asc} /StemV 80 /FontFile2 {file_id} 0 R >>",
            bbox[0], bbox[1], bbox[2], bbox[3]
        )
        .into_bytes();

        let mut font_file = format!(
            "<< /Length {} /Length1 {} /Filter /FlateDecode >>\nstream\n",
            compressed.len(),
            subset.len()
        )
        .into_bytes();
        font_file.extend_from_slice(&compressed);
        font_file.extend_from_slice(b"\nendstream");

        let cmap = build_tounicode_cmap(&self.gid_to_char);
        let cmap_stream =
            format!("<< /Length {} >>\nstream\n{cmap}\nendstream", cmap.len()).into_bytes();

        vec![type0, cid_font, descriptor, font_file, cmap_stream]
    }
}

/// Encodes a PDF info string (such as `/Title`) as UTF-16BE with BOM `<FEFF...>`
/// when non-ASCII/WinAnsi characters are present.
#[must_use]
pub fn pdf_utf16_hex_string(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        let _ = write!(out, "{unit:04X}");
    }
    out.push('>');
    out
}

fn build_tounicode_cmap(gid_to_char: &BTreeMap<u16, char>) -> String {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n\
         /CMapType 2 def\n\
         1 begincodespacerange\n\
         <0000> <FFFF>\n\
         endcodespacerange\n",
    );
    let entries: Vec<(u16, char)> = gid_to_char.iter().map(|(&g, &c)| (g, c)).collect();
    for chunk in entries.chunks(100) {
        let _ = writeln!(out, "{} beginbfchar", chunk.len());
        for &(gid, ch) in chunk {
            let mut utf16 = [0_u16; 2];
            let encoded = ch.encode_utf16(&mut utf16);
            let _ = write!(out, "<{gid:04X}> <");
            for unit in encoded {
                let _ = write!(out, "{unit:04X}");
            }
            out.push_str(">\n");
        }
        out.push_str("endbfchar\n");
    }
    out.push_str(
        "endcmap\n\
         CMapName currentdict /CMap defineresource pop\n\
         end\n\
         end",
    );
    out
}

fn be_u16(bytes: &[u8], off: usize) -> Option<u16> {
    let slice = bytes.get(off..off + 2)?;
    Some(u16::from_be_bytes([slice[0], slice[1]]))
}

fn be_i16(bytes: &[u8], off: usize) -> Option<i16> {
    let slice = bytes.get(off..off + 2)?;
    Some(i16::from_be_bytes([slice[0], slice[1]]))
}

fn be_u32(bytes: &[u8], off: usize) -> Option<u32> {
    let slice = bytes.get(off..off + 4)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn sfnt_checksum(data: &[u8]) -> u32 {
    let mut sum = 0_u32;
    let mut i = 0;
    while i < data.len() {
        let b0 = u32::from(*data.get(i).unwrap_or(&0));
        let b1 = u32::from(*data.get(i + 1).unwrap_or(&0));
        let b2 = u32::from(*data.get(i + 2).unwrap_or(&0));
        let b3 = u32::from(*data.get(i + 3).unwrap_or(&0));
        sum = sum.wrapping_add((b0 << 24) | (b1 << 16) | (b2 << 8) | b3);
        i += 4;
    }
    sum
}

fn parse_sfnt_tables(data: &[u8]) -> Option<BTreeMap<[u8; 4], (usize, usize)>> {
    let sfnt = be_u32(data, 0)?;
    if sfnt != 0x0001_0000 && sfnt != u32::from_be_bytes(*b"true") {
        return None;
    }
    let num_tables = usize::from(be_u16(data, 4)?);
    let mut map = BTreeMap::new();
    for i in 0..num_tables {
        let rec = 12 + i * 16;
        let tag_slice = data.get(rec..rec + 4)?;
        let tag = [tag_slice[0], tag_slice[1], tag_slice[2], tag_slice[3]];
        let offset = be_u32(data, rec + 8)? as usize;
        let length = be_u32(data, rec + 12)? as usize;
        if offset.checked_add(length)? <= data.len() {
            map.insert(tag, (offset, length));
        }
    }
    Some(map)
}

/// If `raw` is a standalone TrueType font, returns a copy. If `raw` is a `.ttc`
/// TrueType Collection, extracts face 0 into a standalone TrueType font buffer.
fn extract_standalone_ttf(raw: &[u8]) -> Option<Vec<u8>> {
    let magic = be_u32(raw, 0)?;
    if magic == 0x0001_0000 || magic == u32::from_be_bytes(*b"true") {
        return Some(raw.to_vec());
    }
    if magic != u32::from_be_bytes(*b"ttcf") {
        return None;
    }
    let num_fonts = be_u32(raw, 8)?;
    if num_fonts == 0 {
        return None;
    }
    let face0_offset = be_u32(raw, 12)? as usize;
    let face0_header = raw.get(face0_offset..)?;
    let sfnt = be_u32(face0_header, 0)?;
    if sfnt != 0x0001_0000 && sfnt != u32::from_be_bytes(*b"true") {
        return None;
    }
    let num_tables = usize::from(be_u16(face0_header, 4)?);
    if num_tables == 0 {
        return None;
    }
    let mut tables = BTreeMap::new();
    for i in 0..num_tables {
        let rec = face0_offset
            .checked_add(12)?
            .checked_add(i.checked_mul(16)?)?;
        let tag_slice = raw.get(rec..rec.checked_add(4)?)?;
        let tag = [tag_slice[0], tag_slice[1], tag_slice[2], tag_slice[3]];
        let offset = be_u32(raw, rec.checked_add(8)?)? as usize;
        let length = be_u32(raw, rec.checked_add(12)?)? as usize;
        let slice = raw.get(offset..offset.checked_add(length)?)?;
        tables.insert(tag, slice.to_vec());
    }
    if tables.is_empty() {
        return None;
    }
    Some(assemble_sfnt(&tables))
}

fn assemble_sfnt(tables: &BTreeMap<[u8; 4], Vec<u8>>) -> Vec<u8> {
    if tables.is_empty() {
        return Vec::new();
    }
    let num_tables = u16::try_from(tables.len()).unwrap_or(u16::MAX);
    let mut entry_selector = 0_u16;
    let mut max_pow2 = 1_u16;
    while max_pow2.saturating_mul(2) <= num_tables {
        max_pow2 = max_pow2.saturating_mul(2);
        entry_selector = entry_selector.saturating_add(1);
    }
    let search_range = max_pow2.saturating_mul(16);
    let range_shift = num_tables.saturating_mul(16).saturating_sub(search_range);

    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
    out.extend_from_slice(&num_tables.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    let mut data_offset = 12 + tables.len() * 16;
    let mut head_offset = None;
    for (&tag, data) in tables {
        if &tag == b"head" {
            head_offset = Some(data_offset);
        }
        out.extend_from_slice(&tag);
        out.extend_from_slice(&sfnt_checksum(data).to_be_bytes());
        out.extend_from_slice(&(data_offset as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        data_offset += (data.len() + 3) & !3;
    }
    for data in tables.values() {
        out.extend_from_slice(data);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
    if let Some(off) = head_offset
        && off + 12 <= out.len()
    {
        let total = sfnt_checksum(&out);
        let adj = 0xB1B0_AFBA_u32.wrapping_sub(total);
        out[off + 8..off + 12].copy_from_slice(&adj.to_be_bytes());
    }
    out
}

/// Builds a compact standalone TrueType font containing only `used_gids` (plus
/// any composite glyph components), keeping original GIDs intact so `hmtx`,
/// composite references, and PDF `/CIDToGIDMap /Identity` stay exact.
fn subset_ttf_preserving_gids(ttf: &[u8], initial_gids: &BTreeSet<u16>) -> Option<Vec<u8>> {
    let tables = parse_sfnt_tables(ttf)?;
    let (head_off, head_len) = *tables.get(b"head")?;
    let (maxp_off, maxp_len) = *tables.get(b"maxp")?;
    let (loca_off, loca_len) = *tables.get(b"loca")?;
    let (glyf_off, glyf_len) = *tables.get(b"glyf")?;
    let head_orig = ttf.get(head_off..head_off + head_len)?;
    let maxp_orig = ttf.get(maxp_off..maxp_off + maxp_len)?;
    let loca_orig = ttf.get(loca_off..loca_off + loca_len)?;
    let glyf_orig = ttf.get(glyf_off..glyf_off + glyf_len)?;

    let index_to_loc = be_i16(head_orig, 50)?;
    let num_glyphs = usize::from(be_u16(maxp_orig, 4)?);
    if num_glyphs == 0 {
        return None;
    }

    let glyph_range = |gid: usize| -> Option<(usize, usize)> {
        if index_to_loc == 0 {
            let s = usize::from(be_u16(loca_orig, gid * 2)?) * 2;
            let e = usize::from(be_u16(loca_orig, (gid + 1) * 2)?) * 2;
            Some((s, e))
        } else {
            let s = be_u32(loca_orig, gid * 4)? as usize;
            let e = be_u32(loca_orig, (gid + 1) * 4)? as usize;
            Some((s, e))
        }
    };

    let mut used = initial_gids.clone();
    used.insert(0);
    let mut queue: VecDeque<u16> = used.iter().copied().collect();
    while let Some(gid) = queue.pop_front() {
        let Some((s, e)) = glyph_range(usize::from(gid)) else {
            continue;
        };
        if e <= s || e > glyf_orig.len() || e - s < 10 {
            continue;
        }
        let glyph = &glyf_orig[s..e];
        if be_i16(glyph, 0)? >= 0 {
            continue;
        }
        let mut pos = 10;
        while pos + 4 <= glyph.len() {
            let flags = be_u16(glyph, pos)?;
            let comp_gid = be_u16(glyph, pos + 2)?;
            if used.insert(comp_gid) {
                queue.push_back(comp_gid);
            }
            pos += 4;
            pos += if flags & 0x0001 != 0 { 4 } else { 2 };
            if flags & 0x0008 != 0 {
                pos += 2;
            } else if flags & 0x0040 != 0 {
                pos += 4;
            } else if flags & 0x0080 != 0 {
                pos += 8;
            }
            if flags & 0x0020 == 0 {
                break;
            }
        }
    }

    let mut new_glyf = Vec::new();
    let mut new_loca = Vec::with_capacity((num_glyphs + 1) * 4);
    for gid in 0..num_glyphs {
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        if used.contains(&(gid as u16))
            && let Some((s, e)) = glyph_range(gid)
            && e > s
            && e <= glyf_orig.len()
        {
            new_glyf.extend_from_slice(&glyf_orig[s..e]);
            while !new_glyf.len().is_multiple_of(4) {
                new_glyf.push(0);
            }
        }
    }
    new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
    if new_glyf.is_empty() {
        new_glyf.extend_from_slice(&[0; 4]);
    }

    let mut new_head = head_orig.to_vec();
    if new_head.len() < 54 {
        return None;
    }
    new_head[8..12].fill(0);
    new_head[50..52].copy_from_slice(&1_u16.to_be_bytes());

    let mut out_tables: BTreeMap<[u8; 4], Vec<u8>> = BTreeMap::new();
    out_tables.insert(*b"head", new_head);
    out_tables.insert(*b"loca", new_loca);
    out_tables.insert(*b"glyf", new_glyf);
    // Minimal PostScript format 3.0 table (no glyph names stored).
    let mut post = vec![0_u8; 32];
    post[1] = 3;
    out_tables.insert(*b"post", post);

    for tag in [
        b"hhea", b"maxp", b"hmtx", b"OS/2", b"name", b"cvt ", b"fpgm", b"prep",
    ] {
        if let Some(&(off, len)) = tables.get(tag)
            && let Some(slice) = ttf.get(off..off + len)
        {
            out_tables.insert(*tag, slice.to_vec());
        }
    }
    Some(assemble_sfnt(&out_tables))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tounicode_cmap_and_utf16_title_encode_vietnamese_and_cjk() {
        let mut map = BTreeMap::new();
        map.insert(0x0123, 'ế');
        map.insert(0x0456, '夜');
        let cmap = build_tounicode_cmap(&map);
        assert!(cmap.contains("<0123> <1EBF>"));
        assert!(cmap.contains("<0456> <591C>"));
        assert_eq!(pdf_utf16_hex_string("夜"), "<FEFF591C>");
    }

    #[test]
    fn system_unicode_font_subsets_vietnamese_and_cjk_compactly() {
        let sample = "Tiếng Việt — Đêm khuya 夜の星 한국어 中文";
        let chars: Vec<char> = sample.chars().collect();
        let Some(mut font) = UnicodePdfFont::resolve(None, &chars) else {
            // Minimal Linux CI containers may have no system TrueType fonts installed.
            return;
        };
        let hex = font.encode_hex(sample);
        assert!(hex.starts_with('<') && hex.ends_with('>'));
        assert!(font.has_used_glyphs());
        let objects = font.build_pdf_objects(10);
        assert_eq!(objects.len(), 5);
        assert!(
            std::str::from_utf8(&objects[0])
                .expect("utf8")
                .contains("/Subtype /Type0")
        );
        assert!(
            std::str::from_utf8(&objects[1])
                .expect("utf8")
                .contains("/Subtype /CIDFontType2")
        );
        assert!(
            std::str::from_utf8(&objects[4])
                .expect("utf8")
                .contains("beginbfchar")
        );
        // Subsetted font stream should be dramatically smaller than a full 22 MB CJK font.
        assert!(
            objects[3].len() < 512 * 1024,
            "subsetted font stream too large: {} bytes",
            objects[3].len()
        );
    }

    #[test]
    fn malformed_ttf_and_ttc_inputs_never_panic() {
        // Empty, truncated, and crafted TTC headers (including 0 tables and huge offsets).
        let mut ttc_zero_tables = Vec::new();
        ttc_zero_tables.extend_from_slice(b"ttcf");
        ttc_zero_tables.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
        ttc_zero_tables.extend_from_slice(&1_u32.to_be_bytes());
        ttc_zero_tables.extend_from_slice(&16_u32.to_be_bytes());
        ttc_zero_tables.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
        ttc_zero_tables.extend_from_slice(&0_u16.to_be_bytes()); // num_tables = 0
        ttc_zero_tables.extend_from_slice(&[0; 6]);

        let mut ttf_huge_table = Vec::new();
        ttf_huge_table.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
        ttf_huge_table.extend_from_slice(&1_u16.to_be_bytes());
        ttf_huge_table.extend_from_slice(&[0; 6]);
        ttf_huge_table.extend_from_slice(b"glyf");
        ttf_huge_table.extend_from_slice(&0_u32.to_be_bytes());
        ttf_huge_table.extend_from_slice(&u32::MAX.to_be_bytes());
        ttf_huge_table.extend_from_slice(&u32::MAX.to_be_bytes());

        for sample in [
            &[][..],
            b"ttcf",
            &[0x00, 0x01, 0x00, 0x00, 0xFF, 0xFF],
            &ttc_zero_tables,
            &ttf_huge_table,
        ] {
            assert!(UnicodePdfFont::from_bytes(sample).is_none());
        }

        // Deterministic pseudorandom byte corpus with TrueType/TTC magic prefixes.
        let mut seed = 0x5EED_CAFE_1234_5678_u64;
        for case in 0..256_usize {
            let len = (case * 17) % 192;
            let mut buf = vec![0_u8; len];
            for byte in &mut buf {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                *byte = (seed >> 56) as u8;
            }
            if buf.len() >= 4 {
                let magic: &[u8; 4] = match case % 3 {
                    0 => &[0x00, 0x01, 0x00, 0x00],
                    1 => b"ttcf",
                    _ => b"true",
                };
                buf[..4].copy_from_slice(magic);
            }
            let _ = UnicodePdfFont::from_bytes(&buf);
        }
    }
}
