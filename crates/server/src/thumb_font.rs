//! Font glyph-sheet thumbnails (CPE-1236, epic CPE-718): render a small "Aa" specimen for
//! `.ttf`/`.otf`/`.woff` files so the thumbnail grid shows what a font actually looks like instead of
//! a generic icon. Integrates into the same [`crate::thumb_source::decode_thumb_image`] dispatch the
//! raster/SVG paths use.
//!
//! Uses `ab_glyph` for outline rasterization — an already-minimal, pure-Rust, no-system-libs crate
//! (it pulls in only `ab_glyph_rasterizer` and `owned_ttf_parser`), rather than a full text-shaping
//! stack. `.ttf`/`.otf` are raw SFNT already, so they're handed to `ab_glyph` unchanged. `.woff` wraps
//! each SFNT table in an (optionally zlib-compressed) container; [`unwrap_woff`] reassembles a plain
//! SFNT buffer from it, reusing the already-vendored `flate2` (used elsewhere for tar.gz/zip) rather
//! than adding a dedicated WOFF crate. `.woff2` uses Brotli compression instead of zlib — adding a
//! Brotli decoder would break the "pure light stack" budget this ticket set for itself, so `.woff2`
//! is explicitly unsupported for now (returns a clear `Err`, never a panic); a follow-up ticket can
//! add it if a suitably small pure-Rust Brotli decoder is worth the extra dependency.
//!
//! Malformed/truncated font data at any stage (bad SFNT, bad WOFF header, corrupt zlib stream) returns
//! `Err`, which the thumbnail pipeline already treats as "no thumbnail, show the type icon" — never a
//! panic.

use std::io::Read;

use ab_glyph::{Font, FontRef, ScaleFont, point, PxScale};
use image::{DynamicImage, Rgba, RgbaImage};

/// A single decompressed WOFF table cannot exceed this — WOFF tables are ordinarily a few KB to a few
/// hundred KB; 32 MiB is generous headroom while still refusing a crafted "tiny input, huge output"
/// zlib bomb. Mirrors the spirit of `thumb_source`'s PNG/PSD decompression-bomb guard (CPE-1087).
const MAX_WOFF_TABLE_BYTES: u64 = 32 * 1024 * 1024;
/// The whole reconstructed SFNT (all tables combined) cannot exceed this either — belt-and-braces
/// against many small-but-numerous inflated tables adding up.
const MAX_SFNT_BYTES: u64 = 64 * 1024 * 1024;
/// A WOFF/SFNT declaring more tables than this is implausible for a real font and is rejected outright
/// rather than trusted to drive a loop.
const MAX_TABLES: usize = 256;

/// The specimen glyphs rendered into the glyph sheet — a short "Aa" sample per the ticket, enough to
/// convey the font's character without needing full-alphabet coverage or a heavier text-shaping stack.
const SPECIMEN: &str = "Aa";

fn read_u16(b: &[u8], off: usize) -> Result<u16, String> {
    b.get(off..off + 2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
        .ok_or_else(|| "truncated font data".to_string())
}

fn read_u32(b: &[u8], off: usize) -> Result<u32, String> {
    b.get(off..off + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| "truncated font data".to_string())
}

/// Reassemble a plain SFNT (TrueType/OpenType) buffer from a WOFF 1.0 container: parses the WOFF
/// header + table directory, decompresses (or copies, if already stored raw) each table's data, then
/// rebuilds a standard `sfnt` offset table + table directory + table data blob that `ab_glyph`
/// (via `ttf-parser`) can parse like any other TTF/OTF file. Table checksums are copied through as-is
/// (`ttf-parser` doesn't validate them, and recomputing per the exact SFNT checksum algorithm buys
/// nothing here).
fn unwrap_woff(bytes: &[u8]) -> Result<Vec<u8>, String> {
    const HEADER_LEN: usize = 44;
    const DIR_ENTRY_LEN: usize = 20;

    if bytes.len() < HEADER_LEN {
        return Err("WOFF header too short".to_string());
    }
    if read_u32(bytes, 0)? != 0x774F_4646 {
        return Err("not a WOFF file (bad signature)".to_string());
    }
    let flavor = read_u32(bytes, 4)?;
    let num_tables = read_u16(bytes, 12)? as usize;
    if num_tables == 0 || num_tables > MAX_TABLES {
        return Err(format!("implausible WOFF table count: {num_tables}"));
    }

    let dir_start = HEADER_LEN;
    let dir_end = dir_start
        .checked_add(num_tables * DIR_ENTRY_LEN)
        .ok_or("WOFF table directory overflow")?;
    if bytes.len() < dir_end {
        return Err("WOFF table directory truncated".to_string());
    }

    struct TableRec {
        tag: [u8; 4],
        offset: u32,
        comp_len: u32,
        orig_len: u32,
        orig_checksum: u32,
    }

    let mut tables = Vec::with_capacity(num_tables);
    let mut total_orig: u64 = 0;
    for i in 0..num_tables {
        let rec = dir_start + i * DIR_ENTRY_LEN;
        let tag = [bytes[rec], bytes[rec + 1], bytes[rec + 2], bytes[rec + 3]];
        let offset = read_u32(bytes, rec + 4)?;
        let comp_len = read_u32(bytes, rec + 8)?;
        let orig_len = read_u32(bytes, rec + 12)?;
        let orig_checksum = read_u32(bytes, rec + 16)?;
        if orig_len as u64 > MAX_WOFF_TABLE_BYTES {
            return Err(format!(
                "WOFF table '{}' declares an oversized decompressed length",
                String::from_utf8_lossy(&tag)
            ));
        }
        total_orig += orig_len as u64;
        if total_orig > MAX_SFNT_BYTES {
            return Err("WOFF tables exceed the total reconstructed-SFNT size guard".to_string());
        }
        tables.push(TableRec { tag, offset, comp_len, orig_len, orig_checksum });
    }

    let mut table_data: Vec<Vec<u8>> = Vec::with_capacity(num_tables);
    for t in &tables {
        let start = t.offset as usize;
        let end = start.checked_add(t.comp_len as usize).ok_or("WOFF table data overflow")?;
        let raw = bytes.get(start..end).ok_or("WOFF table data out of bounds")?;
        let data = if t.comp_len == t.orig_len {
            raw.to_vec()
        } else {
            let mut decoder = flate2::read::ZlibDecoder::new(raw).take(MAX_WOFF_TABLE_BYTES);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out).map_err(|e| format!("WOFF table decompression failed: {e}"))?;
            if out.len() as u32 != t.orig_len {
                return Err("WOFF table decompressed to an unexpected size".to_string());
            }
            out
        };
        table_data.push(data);
    }

    // Rebuild the SFNT `searchRange`/`entrySelector`/`rangeShift` per the spec's binary-search layout.
    let n = num_tables as u32;
    let mut entry_selector = 0u32;
    while (1u32 << (entry_selector + 1)) <= n {
        entry_selector += 1;
    }
    let search_range = (1u32 << entry_selector) * 16;
    let range_shift = n * 16 - search_range;

    let header_len = 12 + num_tables * 16;
    let mut offsets = Vec::with_capacity(num_tables);
    let mut cursor = header_len as u32;
    for data in &table_data {
        offsets.push(cursor);
        cursor += (data.len() as u32 + 3) & !3; // 4-byte aligned
    }

    let mut out = Vec::with_capacity(cursor as usize);
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    out.extend_from_slice(&(search_range as u16).to_be_bytes());
    out.extend_from_slice(&(entry_selector as u16).to_be_bytes());
    out.extend_from_slice(&(range_shift as u16).to_be_bytes());
    for (i, t) in tables.iter().enumerate() {
        out.extend_from_slice(&t.tag);
        out.extend_from_slice(&t.orig_checksum.to_be_bytes());
        out.extend_from_slice(&offsets[i].to_be_bytes());
        out.extend_from_slice(&t.orig_len.to_be_bytes());
    }
    for data in &table_data {
        out.extend_from_slice(data);
        let pad = (4 - (data.len() % 4)) % 4;
        out.extend(std::iter::repeat(0u8).take(pad));
    }

    Ok(out)
}

/// Convert `bytes` (the contents of a font file with the given lowercased extension) to a raw SFNT
/// buffer `ab_glyph` can parse. `.ttf`/`.otf` pass through unchanged; `.woff` is unwrapped via
/// [`unwrap_woff`]; anything else (notably `.woff2`) is rejected with a clear message.
fn to_sfnt_bytes(bytes: &[u8], ext: &str) -> Result<Vec<u8>, String> {
    match ext {
        "woff" => unwrap_woff(bytes),
        "woff2" => Err("woff2 fonts are not yet supported (Brotli decompression; follow-up)".to_string()),
        _ => Ok(bytes.to_vec()),
    }
}

/// Render a font file's contents to an "Aa" glyph-sheet RGBA image, `edge` x `edge` pixels (`edge` is
/// `max_edge` clamped to at least 1). Returns `Err` — never panics — for unparseable font data or an
/// unsupported container (`.woff2`).
pub fn render_glyph_sheet(bytes: &[u8], ext: &str, max_edge: u32) -> Result<DynamicImage, String> {
    let sfnt = to_sfnt_bytes(bytes, ext)?;
    let font = FontRef::try_from_slice(&sfnt).map_err(|e| e.to_string())?;
    Ok(DynamicImage::ImageRgba8(compose_glyph_sheet(&font, max_edge.max(1))))
}

/// Compose the specimen glyphs into an `edge` x `edge` canvas: measure at a reference scale, derive
/// the pixel scale that fits the specimen's line height within the canvas (minus a small margin), lay
/// glyphs out left-to-right using the font's own advance widths, then rasterize each outline and alpha-
/// blend dark ink over a white background. Glyphs the font doesn't map (missing cmap coverage) are
/// silently skipped — same "don't panic on the unusual" discipline as the rest of the pipeline.
fn compose_glyph_sheet<F: Font>(font: &F, edge: u32) -> RgbaImage {
    let mut canvas = RgbaImage::from_pixel(edge, edge, Rgba([255, 255, 255, 255]));

    let margin_frac = 0.15_f32;
    let target_h = (edge as f32 * (1.0 - 2.0 * margin_frac)).max(1.0);

    // Measure at an arbitrary reference scale, then linearly correct to hit target_h (font metrics
    // scale proportionally with PxScale, so one correction pass is exact — no iteration needed).
    let probe_scale = PxScale::from(100.0);
    let probe = font.as_scaled(probe_scale);
    let probe_h = (probe.ascent() - probe.descent()).max(1.0);
    let final_px = (100.0 * target_h / probe_h).max(1.0);

    let scale = PxScale::from(final_px);
    let sf = font.as_scaled(scale);

    let mut cursor_x = 0f32;
    let mut layout: Vec<(ab_glyph::GlyphId, f32)> = Vec::with_capacity(SPECIMEN.chars().count());
    for c in SPECIMEN.chars() {
        let id = sf.glyph_id(c);
        layout.push((id, cursor_x));
        cursor_x += sf.h_advance(id);
    }
    let text_w = cursor_x.max(1.0);
    let text_h = (sf.ascent() - sf.descent()).max(1.0);

    let origin_x = ((edge as f32 - text_w) / 2.0).max(0.0);
    let baseline_y = ((edge as f32 - text_h) / 2.0).max(0.0) + sf.ascent();

    for (id, x_off) in layout {
        let glyph = id.with_scale_and_position(scale, point(origin_x + x_off, baseline_y));
        let Some(outlined) = font.outline_glyph(glyph) else { continue };
        let bounds = outlined.px_bounds();
        outlined.draw(|x, y, coverage| {
            let px = bounds.min.x as i32 + x as i32;
            let py = bounds.min.y as i32 + y as i32;
            if px < 0 || py < 0 || px as u32 >= edge || py as u32 >= edge {
                return;
            }
            let coverage = coverage.clamp(0.0, 1.0);
            let existing = *canvas.get_pixel(px as u32, py as u32);
            // Blend black ink over the existing pixel by coverage (straight alpha, opaque canvas).
            let blend = |bg: u8| (bg as f32 * (1.0 - coverage)) as u8;
            canvas.put_pixel(
                px as u32,
                py as u32,
                Rgba([blend(existing.0[0]), blend(existing.0[1]), blend(existing.0[2]), 255]),
            );
        });
    }

    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal, valid, real SFNT TrueType font: `ttf-parser`'s own doc-test fixture
    /// (`tests/fonts/demo.ttf` in the `ttf-parser` crate, MIT-licensed by its author Yevhenii
    /// Reizner, same license as the crate itself). 400 bytes, 7 tables (cmap/glyf/head/hhea/hmtx/
    /// loca/maxp), a single mapped character `'A'` -> glyph 1 with a real outline (bbox 6,0..541,656
    /// in font units) — the smallest realistic "hello world" TrueType fixture, which is exactly what a
    /// hand-built glyf/loca table would otherwise take many hours to construct byte-correct.
    /// `include_bytes!` from `src/testdata/demo.ttf` (see `src/testdata/NOTICE.md` for provenance +
    /// license) rather than a hand-transcribed literal — a binary format is exactly the case where a
    /// byte-for-byte file beats manual transcription risk.
    const DEMO_TTF: &[u8] = include_bytes!("testdata/demo.ttf");

    #[test]
    fn render_glyph_sheet_produces_a_non_empty_png_from_a_real_ttf() {
        let img = render_glyph_sheet(DEMO_TTF, "ttf", 64).unwrap();
        assert_eq!((img.width(), img.height()), (64, 64), "canvas sized to max_edge (square)");
        let rgba = img.to_rgba8();
        let has_ink = rgba.pixels().any(|p| p.0[0] < 250 || p.0[1] < 250 || p.0[2] < 250);
        assert!(has_ink, "the 'A' glyph should paint at least one non-white pixel");
    }

    #[test]
    fn render_glyph_sheet_clamps_a_zero_max_edge() {
        let img = render_glyph_sheet(DEMO_TTF, "ttf", 0).unwrap();
        assert_eq!((img.width(), img.height()), (1, 1));
    }

    #[test]
    fn render_glyph_sheet_rejects_a_malformed_ttf_without_panicking() {
        let err = render_glyph_sheet(b"not a font at all", "ttf", 32);
        assert!(err.is_err(), "malformed font data must fall back gracefully, not panic");
    }

    #[test]
    fn render_glyph_sheet_rejects_woff2_with_a_clear_message() {
        let err = render_glyph_sheet(b"wOF2 anything", "woff2", 32);
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("woff2"), "should explain woff2 specifically, not a generic parse error");
    }

    /// Build a real WOFF 1.0 container wrapping `DEMO_TTF`'s tables, zlib-compressing every table (even
    /// ones compression won't shrink) so the test exercises `unwrap_woff`'s decompression branch, not
    /// just its raw-copy branch.
    fn woff_wrapping_demo_ttf() -> Vec<u8> {
        // Parse DEMO_TTF's own table directory to recover each table's (tag, bytes) pairs.
        let num_tables = u16::from_be_bytes([DEMO_TTF[4], DEMO_TTF[5]]) as usize;
        struct T {
            tag: [u8; 4],
            data: Vec<u8>,
            checksum: u32,
        }
        let mut tables = Vec::new();
        for i in 0..num_tables {
            let rec = 12 + i * 16;
            let tag = [DEMO_TTF[rec], DEMO_TTF[rec + 1], DEMO_TTF[rec + 2], DEMO_TTF[rec + 3]];
            let checksum = u32::from_be_bytes([
                DEMO_TTF[rec + 4],
                DEMO_TTF[rec + 5],
                DEMO_TTF[rec + 6],
                DEMO_TTF[rec + 7],
            ]);
            let offset = u32::from_be_bytes([
                DEMO_TTF[rec + 8],
                DEMO_TTF[rec + 9],
                DEMO_TTF[rec + 10],
                DEMO_TTF[rec + 11],
            ]) as usize;
            let len = u32::from_be_bytes([
                DEMO_TTF[rec + 12],
                DEMO_TTF[rec + 13],
                DEMO_TTF[rec + 14],
                DEMO_TTF[rec + 15],
            ]) as usize;
            tables.push(T { tag, data: DEMO_TTF[offset..offset + len].to_vec(), checksum });
        }

        use std::io::Write;
        let mut compressed: Vec<Vec<u8>> = Vec::new();
        for t in &tables {
            let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            enc.write_all(&t.data).unwrap();
            compressed.push(enc.finish().unwrap());
        }

        let n = tables.len() as u32;
        let dir_len = 20 * tables.len();
        let mut data_offset = 44 + dir_len as u32;
        let mut dir = Vec::new();
        let mut data_blob = Vec::new();
        for (t, comp) in tables.iter().zip(compressed.iter()) {
            dir.extend_from_slice(&t.tag);
            dir.extend_from_slice(&data_offset.to_be_bytes());
            dir.extend_from_slice(&(comp.len() as u32).to_be_bytes()); // compLength
            dir.extend_from_slice(&(t.data.len() as u32).to_be_bytes()); // origLength
            dir.extend_from_slice(&t.checksum.to_be_bytes());
            data_blob.extend_from_slice(comp);
            data_offset += comp.len() as u32;
        }

        let mut out = Vec::new();
        out.extend_from_slice(&0x774F_4646u32.to_be_bytes()); // "wOFF"
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // flavor (TrueType)
        let total_len = 44 + dir.len() as u32 + data_blob.len() as u32;
        out.extend_from_slice(&total_len.to_be_bytes()); // length
        out.extend_from_slice(&(n as u16).to_be_bytes()); // numTables
        out.extend_from_slice(&0u16.to_be_bytes()); // reserved
        out.extend_from_slice(&0u32.to_be_bytes()); // totalSfntSize (unused by our unwrapper)
        out.extend_from_slice(&1u16.to_be_bytes()); // majorVersion
        out.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
        out.extend_from_slice(&0u32.to_be_bytes()); // metaOffset
        out.extend_from_slice(&0u32.to_be_bytes()); // metaLength
        out.extend_from_slice(&0u32.to_be_bytes()); // metaOrigLength
        out.extend_from_slice(&0u32.to_be_bytes()); // privOffset
        out.extend_from_slice(&0u32.to_be_bytes()); // privLength
        out.extend_from_slice(&dir);
        out.extend_from_slice(&data_blob);
        out
    }

    #[test]
    fn render_glyph_sheet_decodes_a_real_woff_via_zlib_decompression() {
        let woff = woff_wrapping_demo_ttf();
        assert_eq!(&woff[0..4], b"wOFF", "sanity: fixture really is WOFF-signed");
        let img = render_glyph_sheet(&woff, "woff", 48).unwrap();
        let rgba = img.to_rgba8();
        let has_ink = rgba.pixels().any(|p| p.0[0] < 250);
        assert!(has_ink, "WOFF must decompress+render the same 'A' glyph as the raw TTF");
    }

    #[test]
    fn render_glyph_sheet_rejects_a_malformed_woff_without_panicking() {
        let err = render_glyph_sheet(b"not a woff file", "woff", 32);
        assert!(err.is_err(), "malformed WOFF must fall back gracefully, not panic");
    }

    #[test]
    fn unwrap_woff_rejects_a_table_declaring_an_oversized_decompressed_length() {
        // A well-formed-looking WOFF header/directory for one table whose declared origLength blows
        // past MAX_WOFF_TABLE_BYTES — must be rejected before any decompression is attempted.
        let mut out = Vec::new();
        out.extend_from_slice(&0x774F_4646u32.to_be_bytes());
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&(44 + 20 + 4u32).to_be_bytes()); // length (approx, unchecked by us)
        out.extend_from_slice(&1u16.to_be_bytes()); // numTables = 1
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        // Table directory entry:
        out.extend_from_slice(b"cmap");
        out.extend_from_slice(&64u32.to_be_bytes()); // offset
        out.extend_from_slice(&4u32.to_be_bytes()); // compLength
        out.extend_from_slice(&(u32::MAX).to_be_bytes()); // origLength: absurd
        out.extend_from_slice(&0u32.to_be_bytes()); // checksum
        out.extend_from_slice(&[0u8; 4]); // 4 bytes of "table data"
        let err = render_glyph_sheet(&out, "woff", 32);
        assert!(err.is_err(), "an oversized declared WOFF table length must be rejected, not OOM");
    }
}
