//! Document text extraction for content search (CPE-1274, epic CPE-976): turn a file's already-read
//! bytes into indexable plain text, dispatched by extension — the plumbing [`crate::content_index`]'s
//! walk (and its snippet re-read) both call through [`content_text_of`], the one entry point.
//!
//! - Plain-text/code/unknown files use the pre-existing NUL-sniff + `from_utf8_lossy` path
//!   (byte-identical to pre-CPE-1274 behavior for these — see [`looks_binary`]).
//! - `.pdf` text comes from pdfium via [`crate::thumb_pdf::extract_text`] — the exact same lazily-bound
//!   pdfium instance [`crate::thumb_pdf::render_first_page`] (CPE-1256) already uses for thumbnails,
//!   gated behind the same `pdf-thumb` feature so the plain build compiles zero PDF code.
//! - `.docx`/`.xlsx`/`.pptx` (OOXML: ZIPs of XML) go through [`crate::doc_text`]'s extractors, which
//!   reuse the crate's existing (non-optional) `zip` dependency and hand-rolled tag-stripper — no new
//!   dependency either way.
//!
//! Never panics: unsupported, corrupt, encrypted, empty, or oversized input all map to `None` (skip),
//! matching the content-index walk's existing skip-on-error discipline — one bad document must never
//! fail the whole build.

use std::path::Path;

/// Hard cap on extracted text length (**chars**) for any one document. The [`crate::content_index`]
/// walk already caps the *source* bytes read from disk (`MAX_FILE_BYTES`) before this module ever sees
/// them, but a compressed container (a ZIP-based Office doc, or — in principle — a PDF with heavily
/// repeated text) can expand into much more text than its on-disk size after decoding, so this is a
/// second, independent cap on the *output* rather than trusting the input cap to bound it transitively.
/// ~4M chars is generous for any single real-world document while still keeping one huge/pathological
/// file's index+embed cost bounded.
const MAX_EXTRACTED_CHARS: usize = 4 * 1024 * 1024;

/// Truncation cue appended by [`cap`] when IT is the thing doing the cutting, matching the
/// "… (truncated)" idiom [`crate::doc_text`] already uses for its own (larger, 8 MiB) inner cap.
///
/// [`crate::doc_text`]'s docx/xlsx/pptx extractors already append this same marker when THEIR cap
/// (8 MiB of raw document bytes, comfortably more text than 4M chars for real documents) is hit. But
/// this module's 4M-char cap is independent and can be reached first — e.g. a heavily-repeated-text PDF,
/// or simply a document whose extracted text happens to exceed 4M chars well under 8 MiB of source
/// bytes — in which case [`cap`] cuts the string *before* reaching any marker doc_text may have
/// appended, silently dropping it (CPE-1448). [`cap`] always re-appends its own cue when it actually
/// truncates, so the search path never shows a silently-cut document with no visible cue.
const TRUNCATION_MARKER: &str = "\n… (truncated)";

/// True if a byte slice looks binary (contains a NUL in the sniffed prefix) — the same heuristic
/// `content_search`/(the former) `content_index` have long used to skip files not worth reading as
/// text. Only consulted on the plain-text/unknown-extension fallback branch of [`content_text_of`]:
/// PDF/Office documents are routed to their own extractors purely by extension, regardless of what
/// their raw bytes look like (a `.docx`'s raw ZIP bytes may or may not contain an early NUL — that's
/// irrelevant once the extension routes it elsewhere).
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

/// Truncate `s` to at most [`MAX_EXTRACTED_CHARS`] chars — always on a char boundary (`String`'s
/// `Chars` iterator never yields a partial code point), so this can never panic or produce invalid
/// UTF-8, unlike a raw byte-index slice.
///
/// When it actually cuts, it reserves room for and appends [`TRUNCATION_MARKER`] (CPE-1448) so the cut
/// is always visible — whether or not the input already carried its own inner-cap marker (e.g. from
/// [`crate::doc_text`]'s 8 MiB cap): if this cap lands before that marker, its own text is dropped along
/// with the rest of the tail and this cap's marker takes over as the visible cue; if this cap doesn't
/// bite at all, an existing inner marker (if any) simply passes through unchanged. The result is always
/// at most [`MAX_EXTRACTED_CHARS`] chars, so this can't push the output back over the cap it enforces.
fn cap(s: String) -> String {
    let char_count = s.chars().count();
    if char_count <= MAX_EXTRACTED_CHARS {
        return s;
    }
    let marker_chars = TRUNCATION_MARKER.chars().count();
    let keep = MAX_EXTRACTED_CHARS.saturating_sub(marker_chars);
    let mut truncated: String = s.chars().take(keep).collect();
    truncated.push_str(TRUNCATION_MARKER);
    truncated
}

/// pdfium page-text extraction, feature-gated behind `pdf-thumb` exactly like [`crate::thumb_pdf`]
/// itself — with the feature off, this is a zero-cost `None` and no PDF/pdfium code compiles at all.
#[cfg(feature = "pdf-thumb")]
fn pdf_text(bytes: &[u8]) -> Option<String> {
    crate::thumb_pdf::extract_text(bytes, MAX_EXTRACTED_CHARS).ok()
}

#[cfg(not(feature = "pdf-thumb"))]
fn pdf_text(_bytes: &[u8]) -> Option<String> {
    None
}

/// The indexable text for `path`'s content (`bytes` is that same file's already-read content — passed
/// in rather than re-read here since most callers already have it in hand), or `None` to skip the file
/// entirely. Dispatched by `path`'s lowercased extension:
///
/// - `pdf` — pdfium page text ([`pdf_text`]). Without the `pdf-thumb` feature, or if pdfium can't be
///   bound / the document is encrypted / malformed / has no extractable text object, `None`.
/// - `docx` / `xlsx` / `pptx` — the relevant OOXML ZIP part(s) via [`crate::doc_text`], tags stripped.
///   A same-extension file that isn't actually a valid ZIP is `None`, never a panic.
/// - anything else — the pre-existing plain-text path: NUL-sniffed via [`looks_binary`], then
///   `from_utf8_lossy`. This is the byte-identical pre-CPE-1274 behavior for ordinary text/code files.
///
/// Every branch is capped to [`MAX_EXTRACTED_CHARS`] before returning.
pub fn content_text_of(path: &Path, bytes: &[u8]) -> Option<String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => pdf_text(bytes).map(cap),
        "docx" => crate::doc_text::docx_text(&path.to_string_lossy()).ok().map(cap),
        "xlsx" => crate::doc_text::xlsx_text(&path.to_string_lossy()).ok().map(cap),
        "pptx" => crate::doc_text::pptx_text(&path.to_string_lossy()).ok().map(cap),
        _ => {
            if looks_binary(bytes) {
                None
            } else {
                Some(cap(String::from_utf8_lossy(bytes).into_owned()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-content-text-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn plain_text_file_is_unchanged_from_the_pre_cpe_1274_behavior() {
        let d = scratch("plain");
        let f = d.join("notes.txt");
        fs::write(&f, b"the quick brown fox").unwrap();
        let bytes = fs::read(&f).unwrap();
        let text = content_text_of(&f, &bytes).unwrap();
        assert_eq!(text, "the quick brown fox");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn binary_file_with_unknown_extension_is_none() {
        let d = scratch("binary");
        let f = d.join("blob.bin");
        fs::write(&f, b"quick fox\x00binary junk").unwrap();
        let bytes = fs::read(&f).unwrap();
        assert!(content_text_of(&f, &bytes).is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn docx_extension_dispatches_to_the_docx_extractor() {
        let d = scratch("docx");
        let f = d.join("report.docx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("word/document.xml", opts).unwrap();
            let xml = r#"<?xml version="1.0"?><w:document><w:body><w:p><w:r><w:t>Quarterly numbers</w:t></w:r></w:p></w:body></w:document>"#;
            zip.write_all(xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let bytes = fs::read(&f).unwrap();
        let text = content_text_of(&f, &bytes).unwrap();
        assert!(text.contains("Quarterly numbers"), "docx text extracted via dispatch: {text:?}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn xlsx_extension_dispatches_to_the_xlsx_extractor() {
        let d = scratch("xlsx");
        let f = d.join("book.xlsx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("xl/sharedStrings.xml", opts).unwrap();
            zip.write_all(b"<sst><si><t>Revenue Forecast</t></si></sst>").unwrap();
            zip.finish().unwrap();
        }
        let bytes = fs::read(&f).unwrap();
        let text = content_text_of(&f, &bytes).unwrap();
        assert!(text.contains("Revenue Forecast"), "xlsx text extracted via dispatch: {text:?}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn pptx_extension_dispatches_to_the_pptx_extractor() {
        let d = scratch("pptx");
        let f = d.join("deck.pptx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("ppt/slides/slide1.xml", opts).unwrap();
            zip.write_all(b"<p:sld><a:p><a:r><a:t>Roadmap</a:t></a:r></a:p></p:sld>").unwrap();
            zip.finish().unwrap();
        }
        let bytes = fs::read(&f).unwrap();
        let text = content_text_of(&f, &bytes).unwrap();
        assert!(text.contains("Roadmap"), "pptx text extracted via dispatch: {text:?}");
        let _ = fs::remove_dir_all(&d);
    }

    /// A malformed/non-ZIP file with an office extension must degrade to `None` (skip), never panic or
    /// bubble an error out of the content-index walk.
    #[test]
    fn malformed_office_document_is_none_not_a_panic() {
        let d = scratch("malformed");
        for name in ["broken.docx", "broken.xlsx", "broken.pptx"] {
            let f = d.join(name);
            fs::write(&f, b"this is not a zip file at all").unwrap();
            let bytes = fs::read(&f).unwrap();
            assert!(content_text_of(&f, &bytes).is_none(), "{name} must degrade to None, not panic");
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// Unconditional (no pdfium needed): a `.pdf`-extensioned file whose bytes are garbage must degrade
    /// to `None`. If pdfium isn't bound at all in this environment, [`pdf_text`] already returns `None`
    /// before touching the bytes — which still satisfies "malformed PDF -> None".
    #[test]
    fn malformed_pdf_is_none_not_a_panic() {
        let d = scratch("pdf-malformed");
        let f = d.join("broken.pdf");
        fs::write(&f, b"not a real pdf").unwrap();
        let bytes = fs::read(&f).unwrap();
        assert!(content_text_of(&f, &bytes).is_none());
        let _ = fs::remove_dir_all(&d);
    }

    /// Real PDF extraction through the dispatch layer, gated on pdfium being loadable in this
    /// environment (same discipline `thumb_pdf`'s own tests use — pdfium isn't bundled in the sandbox
    /// this ticket was built in; ship-time acquisition is CPE-1258). Skips (no fail) if unavailable.
    #[cfg(feature = "pdf-thumb")]
    #[test]
    fn real_pdf_extracts_text_through_dispatch_when_pdfium_is_available() {
        // A minimal single-page PDF that actually draws text ("Hello World"), built the same way
        // thumb_pdf's own fixture is, duplicated here to keep this test self-contained (that fixture
        // is private to thumb_pdf's test module).
        fn minimal_text_pdf() -> Vec<u8> {
            let mut buf = Vec::new();
            buf.extend_from_slice(b"%PDF-1.4\n");
            let content = b"BT /F1 24 Tf 72 200 Td (Hello World) Tj ET";
            let content_obj = format!(
                "4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
                content.len(),
                std::str::from_utf8(content).unwrap()
            );
            let objects: Vec<String> = vec![
                "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".to_string(),
                "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n".to_string(),
                "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] \
                 /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n"
                    .to_string(),
                content_obj,
                "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n".to_string(),
            ];
            let mut offsets = Vec::with_capacity(objects.len());
            for obj in &objects {
                offsets.push(buf.len());
                buf.extend_from_slice(obj.as_bytes());
            }
            let xref_offset = buf.len();
            buf.extend_from_slice(b"xref\n");
            buf.extend_from_slice(format!("0 {}\n", objects.len() + 1).as_bytes());
            buf.extend_from_slice(b"0000000000 65535 f \n");
            for off in &offsets {
                buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
            }
            buf.extend_from_slice(b"trailer\n");
            buf.extend_from_slice(format!("<< /Size {} /Root 1 0 R >>\n", objects.len() + 1).as_bytes());
            buf.extend_from_slice(b"startxref\n");
            buf.extend_from_slice(format!("{xref_offset}\n").as_bytes());
            buf.extend_from_slice(b"%%EOF");
            buf
        }

        let d = scratch("pdf-real");
        let f = d.join("hello.pdf");
        fs::write(&f, minimal_text_pdf()).unwrap();
        let bytes = fs::read(&f).unwrap();

        match content_text_of(&f, &bytes) {
            Some(text) => assert!(text.contains("Hello World"), "extracted text: {text:?}"),
            None => crate::skip_notice!(
                "skipping real_pdf_extracts_text_through_dispatch: no pdfium library available in \
                 this environment (expected until CPE-1258 provisions one for CI/dev)"
            ),
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn size_cap_truncates_extracted_text() {
        // Exercise `cap` indirectly via the plain-text path with a synthetic MAX_EXTRACTED_CHARS-sized
        // input would be too slow/large for a unit test; instead verify `cap`'s own contract directly.
        let long = "x".repeat(MAX_EXTRACTED_CHARS + 500);
        let capped = cap(long);
        assert_eq!(capped.chars().count(), MAX_EXTRACTED_CHARS);
        assert!(capped.contains("(truncated)"), "cap() must leave a visible cue when it cuts: {capped:?}");
    }

    #[test]
    fn size_cap_leaves_short_text_untouched_with_no_marker() {
        let short = "the quick brown fox".to_string();
        let capped = cap(short.clone());
        assert_eq!(capped, short, "text under the cap passes through unchanged, no marker added");
    }

    /// CPE-1448 regression: this module's own 4M-char [`MAX_EXTRACTED_CHARS`] outer cap runs on top of
    /// [`crate::doc_text`]'s 8 MiB inner cap. Since [`crate::doc_text`]'s cap is on RAW (pre-strip) bytes
    /// and its "(truncated)" marker sits right at the end of the ~8M-char stripped result, this module's
    /// 4M-char cap cuts well before reaching it — dropping doc_text's marker along with the rest of the
    /// tail. Before the fix, that left a silently truncated document with no visible cue at all on the
    /// content-search path. Build a docx whose single `<w:t>` run is a multi-tens-of-MiB deflate bomb (so
    /// almost all of the 8 MiB doc_text reads passes straight through stripping as plain text, comfortably
    /// exceeding this module's 4M-char cap) and confirm the text handed to content search still carries a
    /// truncation cue.
    #[test]
    fn content_search_path_preserves_a_truncation_cue_through_both_caps() {
        let d = scratch("docx-both-caps");
        let f = d.join("bomb.docx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("word/document.xml", opts).unwrap();
            // A single open <w:t> run of highly-compressible 'A' bytes (classic zip-bomb shape, same
            // idiom as doc_text's own bomb fixtures): 64 MiB decompressed, 8x doc_text's 8 MiB inner cap,
            // so the entry read stops well inside this one text run — none of it is inside markup, so
            // (almost) the whole ~8 MiB doc_text reads survives stripping as plain chars, well over this
            // module's 4M-char outer cap too.
            zip.write_all(b"<w:document><w:body><w:p><w:r><w:t>").unwrap();
            let chunk = vec![b'A'; 1024 * 1024];
            for _ in 0..64 {
                zip.write_all(&chunk).unwrap();
            }
            zip.finish().unwrap();
        }
        let bytes = fs::read(&f).unwrap();
        let text = content_text_of(&f, &bytes).unwrap();
        assert!(text.chars().count() <= MAX_EXTRACTED_CHARS, "outer cap still bounds the output");
        assert!(
            text.contains("(truncated)"),
            "a truncation cue must survive both the inner and outer caps: len={}",
            text.chars().count()
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unsupported_extension_with_binary_content_is_none() {
        let d = scratch("unsupported");
        let f = d.join("image.png");
        // PNG magic bytes include a NUL-adjacent byte pattern; use an explicit NUL to be unambiguous.
        fs::write(&f, [0x89u8, b'P', b'N', b'G', 0x00, 0x0d, 0x0a]).unwrap();
        let bytes = fs::read(&f).unwrap();
        assert!(content_text_of(&f, &bytes).is_none());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn no_extension_falls_back_to_plain_text_path() {
        let d = scratch("noext");
        let f = d.join("README");
        fs::write(&f, b"read me please").unwrap();
        let bytes = fs::read(&f).unwrap();
        let text = content_text_of(&f, &bytes).unwrap();
        assert_eq!(text, "read me please");
        let _ = fs::remove_dir_all(&d);
    }
}
