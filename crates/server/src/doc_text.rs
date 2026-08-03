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

/// Read one entry of a zip as UTF-8 text.
fn zip_read_text(path: &str, entry_name: &str) -> Result<String, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = zip.by_name(entry_name).map_err(|e| format!("{entry_name}: {e}"))?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
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
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let result = match zip.by_name(entry_name) {
        Ok(mut entry) => {
            let mut buf = String::new();
            entry.read_to_string(&mut buf).map_err(|e| e.to_string())?;
            Ok(Some(buf))
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
    use std::io::Read;
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
        if let Ok(mut entry) = zip.by_name(n) {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                let text = strip_markup_to_text(&buf, &["a:p"]);
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
    use std::io::Read;
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
        if let Ok(mut entry) = zip.by_name(n) {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                let text = strip_markup_to_text(&buf, &["p", "h1", "h2", "h3", "h4", "div", "li", "br"]);
                if !text.trim().is_empty() {
                    out.push_str(text.trim());
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
}
