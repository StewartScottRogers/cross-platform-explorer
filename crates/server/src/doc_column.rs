//! Document-metadata column extractor (CPE-1029, epic CPE-707): the document-family counterpart of
//! [`crate::image_column`] (image) and [`crate::media_column`] (audio).
//!
//! [`crate::metadata_column`] (CPE-918) already sorts a [`CellValue::Int`] cell numerically; this module
//! produces that cell for a **PDF page count** from a file's raw bytes. No PDF library is used — this is a
//! byte scan, not a parse, so it stays dependency-free at the cost of only handling the common case (a
//! PDF with `/Type /Page` object markers in its body; object-stream/compressed-xref PDFs, where the page
//! objects aren't visible as plain bytes, fall back to [`CellValue::Empty`] rather than guessing). Pure
//! over bytes — the adapter reads the leading bytes and dispatches by extension; nothing here touches the
//! filesystem.

use crate::metadata_column::CellValue;

/// The [`CellValue::Int`] page count for a PDF's `bytes`, or [`CellValue::Empty`] when the bytes aren't a
/// PDF (no `%PDF` header) or no page objects are found (so the column sorts it last, per
/// [`crate::metadata_column`]).
pub fn doc_pages_cell(bytes: &[u8]) -> CellValue {
    if !bytes.starts_with(b"%PDF") {
        return CellValue::Empty;
    }
    let count = count_page_objects(bytes);
    if count >= 1 {
        CellValue::Int(count as i64)
    } else {
        CellValue::Empty
    }
}

/// Count `/Type /Page` object markers in `bytes`, excluding `/Pages` (the page-tree node). Scans for the
/// literal `/Type` token, skips any ASCII whitespace that follows it, then checks for a `/Page` token
/// immediately after — counted only when the byte right after `/Page` is *not* `s`/`S` (which would make
/// it `/Pages`). Bounds-checked throughout; never panics, even on truncated or malformed bytes.
fn count_page_objects(bytes: &[u8]) -> usize {
    const TYPE: &[u8] = b"/Type";
    const PAGE: &[u8] = b"/Page";
    let mut count = 0;
    let mut i = 0;
    while i + TYPE.len() <= bytes.len() {
        if &bytes[i..i + TYPE.len()] != TYPE {
            i += 1;
            continue;
        }
        let mut j = i + TYPE.len();
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j + PAGE.len() <= bytes.len() && &bytes[j..j + PAGE.len()] == PAGE {
            let after = bytes.get(j + PAGE.len());
            let is_pages_node = matches!(after, Some(b's') | Some(b'S'));
            if !is_pages_node {
                count += 1;
            }
        }
        i += TYPE.len();
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic (but byte-realistic) PDF body: a header, `page_count` `/Type /Page` objects, and one
    /// `/Type /Pages` tree node — enough to exercise the scanner without a real PDF file on disk.
    fn synthetic_pdf(page_count: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"%PDF-1.4\n");
        out.extend_from_slice(
            b"1 0 obj\n<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >>\nendobj\n",
        );
        for n in 0..page_count {
            out.extend_from_slice(
                format!("{} 0 obj\n<< /Type /Page /Parent 1 0 R >>\nendobj\n", 3 + n).as_bytes(),
            );
        }
        out.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n");
        out
    }

    #[test]
    fn counts_type_page_objects_excluding_the_pages_tree_node() {
        let pdf = synthetic_pdf(3);
        assert_eq!(doc_pages_cell(&pdf), CellValue::Int(3));
    }

    #[test]
    fn non_pdf_bytes_yield_empty() {
        assert_eq!(doc_pages_cell(b""), CellValue::Empty);
        assert_eq!(doc_pages_cell(b"not a pdf, just text"), CellValue::Empty);
        assert_eq!(doc_pages_cell(b"%PNG\x89 garbage"), CellValue::Empty);
    }

    #[test]
    fn empty_input_yields_empty() {
        assert_eq!(doc_pages_cell(&[]), CellValue::Empty);
    }

    #[test]
    fn pdf_with_no_page_markers_yields_empty() {
        // Has the header but no /Type /Page objects at all (e.g. an object-stream/compressed-xref PDF
        // where page objects aren't visible as plain bytes) — falls back to Empty rather than guessing.
        let pdf = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\ntrailer\n<< /Root 1 0 R >>\n";
        assert_eq!(doc_pages_cell(pdf), CellValue::Empty);
    }

    #[test]
    fn single_page_pdf_counts_one() {
        let pdf = synthetic_pdf(1);
        assert_eq!(doc_pages_cell(&pdf), CellValue::Int(1));
    }

    #[test]
    fn whitespace_between_type_and_page_is_tolerated() {
        // Newlines/tabs between /Type and /Page shouldn't break the scan.
        let pdf = b"%PDF-1.4\n1 0 obj\n<< /Type \n\t /Page /Parent 2 0 R >>\nendobj\n";
        assert_eq!(doc_pages_cell(pdf), CellValue::Int(1));
    }

    #[test]
    fn never_panics_on_truncated_or_malformed_bytes() {
        // A dangling /Type right at the end of the buffer, with nothing after it.
        assert_eq!(doc_pages_cell(b"%PDF-1.4/Type"), CellValue::Empty);
        // A dangling /Type/Pag (truncated /Page).
        assert_eq!(doc_pages_cell(b"%PDF-1.4/Type /Pag"), CellValue::Empty);
        // Just the header, nothing else.
        assert_eq!(doc_pages_cell(b"%PDF"), CellValue::Empty);
    }
}
