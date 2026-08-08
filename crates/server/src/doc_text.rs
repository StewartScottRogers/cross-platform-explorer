//! Document text extraction (CPE-070/071/072/077): pull readable plain text out of RTF, DOCX, ODT, and
//! EPUB for the preview pane. Small, dependency-light (reuses the `zip` reader already in the Server for
//! the office/ebook containers; RTF is a hand-rolled reader) — not full renderers, just enough for a
//! text preview. Pure and Tauri-free (CPE-815); the Tauri `read_preview_info` command dispatches here.
//!
//! [`xlsx_text`]/[`pptx_text`] (CPE-1274, epic CPE-976) extend the same approach to the other two OOXML
//! container formats, reusing [`zip_read_text`]/[`strip_markup_to_text`] — no new dependency, just two
//! more extension→ZIP-part mappings, same as [`docx_text`]. [`crate::content_text`] is their consumer
//! (content search's text-extraction dispatch); the preview pane doesn't currently call them.

use std::fs;
use std::io::Read;

/// Value of a single ASCII hex digit, or `None`. Used to decode RTF `\'XX` byte escapes without slicing
/// the source `str` (which would panic if `\'` were followed by a multi-byte UTF-8 char in malformed RTF).
fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decode the five predefined XML entities. Applied after tag stripping.
fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Strip XML/HTML tags to plain text, turning the given block/paragraph tags' closing tags into
/// newlines first. Good enough for a readable text preview of office and ebook markup — not a full
/// renderer.
fn strip_markup_to_text(markup: &str, para_tags: &[&str]) -> String {
    let mut s = markup.to_string();
    for t in para_tags {
        s = s.replace(&format!("</{t}>"), "\n");
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    collapse_blank_lines(&decode_xml_entities(&out))
}

/// Collapse runs of 3+ newlines into 2 and trim, so stripped markup reads cleanly.
fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0;
    for c in s.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push('\n');
            }
        } else if c == '\r' {
            // ignore
        } else {
            newlines = 0;
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Hard cap on bytes pulled from a single zip entry's DECOMPRESSING reader before we stop reading
/// (CPE-1446, deflate-bomb OOM). `zip`'s entry reader inflates lazily as bytes are read from it, so
/// wrapping it in `Read::take` bounds how much we ever inflate into memory *regardless* of what the
/// entry's `size()`/`compressed_size()` header metadata claims — a crafted zip can lie about both, so
/// a pre-check against those numbers (e.g. `archive_safety::expansion_ratio`) is only ever a cheap
/// early-warning, never a substitute for capping the actual read. 8 MiB is far more than any
/// legitimate single office/ebook document *part* needs for a text preview (a real `word/document.xml`
/// — the whole document's markup, not just what's on screen — is typically well under 1 MiB even for a
/// long report); it's generous headroom for real content while making a bomb entry (which can claim to
/// inflate to gigabytes) cheap to cut off well before memory pressure.
const MAX_DECOMPRESSED_PART_BYTES: u64 = 8 * 1024 * 1024;

/// Read a zip entry up to [`MAX_DECOMPRESSED_PART_BYTES`], returning the text and whether the cap was
/// hit. Reads into a byte buffer and lossy-decodes rather than `read_to_string` so a cap landing
/// mid-codepoint (or genuinely non-UTF-8 bytes) degrades to replacement characters instead of an
/// `Err` — this is the one place that actually stops the deflate-bomb OOM (CPE-1446); every entry read
/// in this module goes through it instead of a raw `read_to_string`/`read_to_end`.
fn read_entry_capped<R: Read>(entry: R) -> Result<(String, bool), String> {
    let mut limited = entry.take(MAX_DECOMPRESSED_PART_BYTES);
    let mut buf: Vec<u8> = Vec::new();
    limited.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let truncated = buf.len() as u64 >= MAX_DECOMPRESSED_PART_BYTES;
    Ok((String::from_utf8_lossy(&buf).into_owned(), truncated))
}

/// Append the truncation marker to text that was cut off by [`MAX_DECOMPRESSED_PART_BYTES`], matching
/// the "… (truncated)" idiom [`epub_text`] already uses for its whole-document output cap.
fn mark_truncated(mut text: String, truncated: bool) -> String {
    if truncated {
        text.push_str("\n… (truncated)");
    }
    text
}

/// Read one entry of a zip as UTF-8 text, capped at [`MAX_DECOMPRESSED_PART_BYTES`] (CPE-1446).
fn zip_read_text(path: &str, entry_name: &str) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let entry = zip.by_name(entry_name).map_err(|e| format!("{entry_name}: {e}"))?;
    let (text, truncated) = read_entry_capped(entry)?;
    Ok(mark_truncated(text, truncated))
}

/// Extract the body text of a DOCX (word/document.xml) (CPE-071).
pub fn docx_text(path: &str) -> Result<String, String> {
    let xml = zip_read_text(path, "word/document.xml")?;
    Ok(strip_markup_to_text(&xml, &["w:p"]))
}

/// Read one entry of a zip as UTF-8 text if present, or `Ok(None)` if the archive simply doesn't have
/// that part (a normal, non-error shape some documents legitimately lack — e.g. an all-numeric XLSX
/// workbook has no `sharedStrings.xml` at all). A file that isn't a valid zip at all is still `Err`.
fn zip_read_text_optional(path: &str, entry_name: &str) -> Result<Option<String>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let result = match zip.by_name(entry_name) {
        Ok(entry) => {
            let (text, truncated) = read_entry_capped(entry)?;
            Ok(Some(mark_truncated(text, truncated)))
        }
        Err(_) => Ok(None),
    };
    result
}

/// Extract cell text from an XLSX workbook's shared-strings table (`xl/sharedStrings.xml`) (CPE-1274):
/// the OOXML spreadsheet format stores every distinct text string used by any cell once, in this one
/// part, referenced by index from the sheets — so it's the whole workbook's textual content without
/// needing to parse the (much larger, mostly numeric) per-sheet XML at all. A workbook with no shared
/// strings (e.g. an all-numeric sheet) is a normal, valid `Ok("")`, not an error — only an unreadable /
/// non-ZIP file is `Err`.
pub fn xlsx_text(path: &str) -> Result<String, String> {
    let xml = zip_read_text_optional(path, "xl/sharedStrings.xml")?;
    Ok(xml.map(|x| strip_markup_to_text(&x, &["si"])).unwrap_or_default())
}

/// Extract slide text from a PPTX presentation's `ppt/slides/slideN.xml` parts (CPE-1274), concatenated
/// in (string) filename order with a blank line between slides — mirrors [`epub_text`]'s
/// enumerate-then-concatenate shape (list entries under a prefix, sort, strip each, join). A
/// presentation with no slides yields an empty string, not an error.
pub fn pptx_text(path: &str) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut names: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        if let Ok(entry) = zip.by_index(i) {
            let n = entry.name().to_string();
            let low = n.to_lowercase();
            if low.starts_with("ppt/slides/slide") && low.ends_with(".xml") {
                names.push(n);
            }
        }
    }
    names.sort();

    let mut out = String::new();
    for n in &names {
        if let Ok(entry) = zip.by_name(n) {
            // Capped BEFORE strip_markup_to_text runs on it (CPE-1446): a bomb slide part can't
            // inflate past MAX_DECOMPRESSED_PART_BYTES no matter what it claims to decompress to.
            if let Ok((buf, entry_truncated)) = read_entry_capped(entry) {
                let text = mark_truncated(strip_markup_to_text(&buf, &["a:p"]), entry_truncated);
                if !text.trim().is_empty() {
                    out.push_str(text.trim());
                    out.push_str("\n\n");
                }
            }
        }
    }
    Ok(out)
}

/// Extract the body text of an ODT (content.xml) (CPE-072).
pub fn odt_text(path: &str) -> Result<String, String> {
    let xml = zip_read_text(path, "content.xml")?;
    Ok(strip_markup_to_text(&xml, &["text:p", "text:h"]))
}

/// Extract readable text from an EPUB's content documents in name order, capped so a whole book can't
/// flood the pane (CPE-077).
pub fn epub_text(path: &str) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut names: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        if let Ok(entry) = zip.by_index(i) {
            let n = entry.name().to_string();
            let low = n.to_lowercase();
            if low.ends_with(".xhtml") || low.ends_with(".html") || low.ends_with(".htm") {
                names.push(n);
            }
        }
    }
    names.sort();

    let mut out = format!("EPUB — {} content document(s)\n\n", names.len());
    for n in &names {
        if out.len() > 128 * 1024 {
            out.push_str("\n… (truncated)\n");
            break;
        }
        if let Ok(entry) = zip.by_name(n) {
            // Cap the PER-ENTRY inflate BEFORE accumulating into `out` (CPE-1446): the 128KiB check
            // above only runs between entries, so without this a single bomb content-document could
            // OOM on its own `read_to_end` before that check is ever reached again.
            if let Ok((buf, entry_truncated)) = read_entry_capped(entry) {
                let text = strip_markup_to_text(&buf, &["p", "h1", "h2", "h3", "h4", "div", "li", "br"]);
                if !text.trim().is_empty() {
                    out.push_str(text.trim());
                    if entry_truncated {
                        out.push_str("\n… (entry truncated)");
                    }
                    out.push_str("\n\n");
                }
            }
        }
    }
    Ok(out)
}

/// Extract readable text from RTF: a small, dependency-free reader that drops control words and the
/// font/colour/style/info destinations, turning `\par` and friends into newlines. Not a full RTF engine
/// — enough for a text preview (CPE-070).
pub fn rtf_text(path: &str) -> Result<String, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let bytes = raw.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    let mut depth: i32 = 0;
    let mut skip_depth: i32 = -1; // depth of a destination group being skipped

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                if skip_depth >= 0 && depth == skip_depth {
                    skip_depth = -1;
                }
                depth -= 1;
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                let n = bytes[i];
                if n == b'\'' && i + 2 < bytes.len() {
                    // `\'XX` = one byte in the current code page. Decode from the two raw bytes directly —
                    // slicing `raw` here would panic if `\'` were followed by a multi-byte UTF-8 char.
                    if skip_depth < 0 {
                        if let (Some(h), Some(l)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2])) {
                            out.push((h * 16 + l) as char);
                        }
                    }
                    i += 3;
                } else if n.is_ascii_alphabetic() {
                    let start = i;
                    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = &raw[start..i];
                    // optional numeric parameter
                    if i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                        if bytes[i] == b'-' {
                            i += 1;
                        }
                        while i < bytes.len() && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    // a single trailing space is part of the control word
                    if i < bytes.len() && bytes[i] == b' ' {
                        i += 1;
                    }
                    if skip_depth < 0 {
                        match word {
                            "par" | "line" | "sect" => out.push('\n'),
                            "tab" => out.push('\t'),
                            "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "object"
                            | "header" | "footer" | "generator" => skip_depth = depth,
                            _ => {}
                        }
                    }
                } else {
                    if skip_depth < 0 {
                        match n {
                            b'\\' | b'{' | b'}' => out.push(n as char),
                            b'~' => out.push(' '),
                            _ => {}
                        }
                    }
                    i += 1;
                }
            }
            b'\r' | b'\n' => i += 1,
            c => {
                if skip_depth < 0 && depth > 0 {
                    out.push(c as char);
                }
                i += 1;
            }
        }
    }
    Ok(collapse_blank_lines(&out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-doctext-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn rtf_text_extracts_body_and_drops_control_words() {
        let d = scratch("rtf");
        let f = d.join("doc.rtf");
        let rtf = r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}\f0\fs24 Hello \b world\b0.\par Second line.}";
        fs::write(&f, rtf).unwrap();
        let text = rtf_text(&f.to_string_lossy()).unwrap();
        assert!(text.contains("Hello world."), "body text extracted: {text:?}");
        assert!(text.contains("Second line."), "second paragraph present");
        assert!(!text.contains("fonttbl"), "font table dropped");
        assert!(!text.contains("Arial"), "font table contents dropped");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rtf_text_decodes_hex_escapes_and_survives_malformed_ones() {
        let d = scratch("rtf_hex");
        let f = d.join("hex.rtf");
        // Valid `\'41` = 'A'. Then a MALFORMED `\'` immediately followed by a 3-byte UTF-8 char (€):
        // the old code sliced `raw[i+1..i+3]`, splitting the '€' mid-byte and panicking. It must instead
        // skip the bad escape and keep going.
        let rtf = "{\\rtf1 start\\'41\\'\u{20AC}end}";
        fs::write(&f, rtf).unwrap();
        let text = rtf_text(&f.to_string_lossy()).unwrap(); // must not panic
        assert!(text.contains("startA"), "valid \\'41 decodes to A: {text:?}");
        assert!(text.contains("end"), "parsing continues past the malformed escape: {text:?}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn docx_text_extracts_paragraph_text() {
        let d = scratch("docx");
        let f = d.join("doc.docx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("word/document.xml", opts).unwrap();
            let xml = r#"<?xml version="1.0"?><w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t> world</w:t></w:r></w:p><w:p><w:r><w:t>Next &amp; last</w:t></w:r></w:p></w:body></w:document>"#;
            zip.write_all(xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let text = docx_text(&f.to_string_lossy()).unwrap();
        assert!(text.contains("Hello world"), "runs joined within a paragraph: {text:?}");
        assert!(text.contains("Next & last"), "entities decoded");
        assert!(!text.contains("(truncated)"), "a small legitimate document must not be truncated: {text:?}");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1446 regression: a `.docx` whose `word/document.xml` entry is a deflate-bomb (a small
    /// on-disk zip that decompresses to far more than [`MAX_DECOMPRESSED_PART_BYTES`]) must be read
    /// through the cap — proving the fix actually stops the OOM — rather than fully inflated. Before
    /// the fix, `zip_read_text` called `entry.read_to_string` with no bound at all, so a real-world
    /// ~1000:1 deflate bomb (a few KiB on disk expanding to gigabytes) would allocate gigabytes right
    /// here. This fixture only needs to prove the *mechanism* works, so it uses a much smaller bomb
    /// (64 MiB decompressed, 8x the cap) that still exercises the same code path in well under a
    /// second, rather than actually constructing/inflating a multi-GB payload.
    #[test]
    fn docx_text_caps_a_deflate_bomb_entry_instead_of_inflating_it_fully() {
        let d = scratch("docx-bomb");
        let f = d.join("bomb.docx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("word/document.xml", opts).unwrap();
            // Stream a single repeated byte in 1 MiB chunks so building the fixture itself never holds
            // the whole uncompressed payload in memory — 64 MiB total, well over MAX_DECOMPRESSED_PART_BYTES
            // (8 MiB), and highly compressible (the classic zip-bomb shape: tiny on disk, huge inflated).
            let chunk = vec![b'A'; 1024 * 1024];
            for _ in 0..64 {
                zip.write_all(&chunk).unwrap();
            }
            zip.finish().unwrap();
        }

        // Sanity-check the fixture is actually bomb-shaped: tiny on disk vs. its 64 MiB payload.
        let on_disk = fs::metadata(&f).unwrap().len();
        assert!(on_disk < 1024 * 1024, "fixture should compress to well under 1 MiB on disk, got {on_disk}");

        let text = docx_text(&f.to_string_lossy()).unwrap();
        // Must be capped at MAX_DECOMPRESSED_PART_BYTES (plus the short truncation marker) — NOT the
        // full 64 MiB the entry would otherwise inflate to. This is the assertion that proves the cap
        // actually bounded the read rather than allocating the whole decompressed stream.
        const MAX_DECOMPRESSED_PART_BYTES: usize = 8 * 1024 * 1024;
        assert!(
            text.len() <= MAX_DECOMPRESSED_PART_BYTES + 64,
            "capped read must stay near the {MAX_DECOMPRESSED_PART_BYTES}-byte cap, got {} bytes \
             (would be ~64MiB uncapped)",
            text.len()
        );
        assert!(text.contains("(truncated)"), "truncation marker present for a capped entry: len={}", text.len());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn xlsx_text_extracts_shared_strings() {
        let d = scratch("xlsx");
        let f = d.join("book.xlsx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("xl/sharedStrings.xml", opts).unwrap();
            let xml = r#"<?xml version="1.0"?><sst><si><t>Budget</t></si><si><t>Q1 &amp; Q2</t></si></sst>"#;
            zip.write_all(xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let text = xlsx_text(&f.to_string_lossy()).unwrap();
        assert!(text.contains("Budget"), "first shared string present: {text:?}");
        assert!(text.contains("Q1 & Q2"), "entities decoded, second shared string present: {text:?}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn xlsx_text_with_no_shared_strings_part_is_an_empty_ok_not_an_error() {
        let d = scratch("xlsx-empty");
        let f = d.join("numbers.xlsx");
        {
            // A valid zip, but with no xl/sharedStrings.xml part at all — an all-numeric workbook.
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("xl/workbook.xml", opts).unwrap();
            zip.write_all(b"<workbook/>").unwrap();
            zip.finish().unwrap();
        }
        let text = xlsx_text(&f.to_string_lossy()).unwrap();
        assert!(text.is_empty(), "no shared-strings part -> empty text, not an error: {text:?}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn xlsx_text_on_a_non_zip_file_is_an_err_not_a_panic() {
        let d = scratch("xlsx-notzip");
        let f = d.join("fake.xlsx");
        fs::write(&f, b"not a zip at all").unwrap();
        assert!(xlsx_text(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn pptx_text_extracts_and_concatenates_slide_text() {
        let d = scratch("pptx");
        let f = d.join("deck.pptx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("ppt/slides/slide1.xml", opts).unwrap();
            zip.write_all(b"<p:sld><a:p><a:r><a:t>Title Slide</a:t></a:r></a:p></p:sld>").unwrap();
            zip.start_file("ppt/slides/slide2.xml", opts).unwrap();
            zip.write_all(b"<p:sld><a:p><a:r><a:t>Second &amp; last</a:t></a:r></a:p></p:sld>").unwrap();
            // A non-slide part under ppt/slides/ (e.g. a rels sidecar) must not be picked up.
            zip.start_file("ppt/slides/_rels/slide1.xml.rels", opts).unwrap();
            zip.write_all(b"<Relationships/>").unwrap();
            zip.finish().unwrap();
        }
        let text = pptx_text(&f.to_string_lossy()).unwrap();
        assert!(text.contains("Title Slide"), "first slide's text present: {text:?}");
        assert!(text.contains("Second & last"), "second slide's text present, entities decoded: {text:?}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn pptx_text_with_no_slides_is_an_empty_ok_not_an_error() {
        let d = scratch("pptx-empty");
        let f = d.join("blank.pptx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("[Content_Types].xml", opts).unwrap();
            zip.write_all(b"<Types/>").unwrap();
            zip.finish().unwrap();
        }
        let text = pptx_text(&f.to_string_lossy()).unwrap();
        assert!(text.is_empty(), "no slide parts -> empty text, not an error: {text:?}");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1446 regression, the epub-specific half of the bug: `epub_text`'s 128KiB *output* cap
    /// (`out.len() > 128 * 1024`) is only checked between content documents, so before the fix a
    /// single bomb `.xhtml` entry would fully inflate via `read_to_string` and OOM before that
    /// between-entries check was ever reached again. This proves the PER-ENTRY cap now applied inside
    /// the loop (via [`read_entry_capped`]) stops the bomb entry's own read regardless — using a
    /// 64 MiB (8x [`MAX_DECOMPRESSED_PART_BYTES`]) bomb, not an actual multi-GB one, to keep the test
    /// fast while still exercising the same code path.
    #[test]
    fn epub_text_caps_a_deflate_bomb_content_document_before_accumulating() {
        let d = scratch("epub-bomb");
        let f = d.join("bomb.epub");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("OEBPS/bomb.xhtml", opts).unwrap();
            let chunk = vec![b'A'; 1024 * 1024];
            for _ in 0..64 {
                zip.write_all(&chunk).unwrap();
            }
            zip.finish().unwrap();
        }

        let on_disk = fs::metadata(&f).unwrap().len();
        assert!(on_disk < 1024 * 1024, "fixture should compress to well under 1 MiB on disk, got {on_disk}");

        let text = epub_text(&f.to_string_lossy()).unwrap();
        // The single bomb entry's own contribution must be capped near MAX_DECOMPRESSED_PART_BYTES
        // (8 MiB), not the 64 MiB it would otherwise inflate to — this bounds the whole function's
        // output far below what an uncapped read would have allocated.
        const MAX_DECOMPRESSED_PART_BYTES: usize = 8 * 1024 * 1024;
        assert!(
            text.len() <= MAX_DECOMPRESSED_PART_BYTES + 256,
            "single bomb entry must be capped near {MAX_DECOMPRESSED_PART_BYTES} bytes, got {} \
             (would be ~64MiB uncapped)",
            text.len()
        );
        assert!(text.contains("(entry truncated)"), "per-entry truncation marker present: len={}", text.len());
        let _ = fs::remove_dir_all(&d);
    }
}
