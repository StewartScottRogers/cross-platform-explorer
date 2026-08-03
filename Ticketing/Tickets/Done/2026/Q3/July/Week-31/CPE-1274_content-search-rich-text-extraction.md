---
id: CPE-1274
title: "Content search: extract text from PDF + Office docs so they're indexed (not just plain text)"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-976
---

## Summary
Content search (CPE-1262) currently indexes only plain-text files — `content_index.rs` does
`String::from_utf8_lossy(&bytes)` and NUL-sniffs out binaries, so PDFs, Word/Excel/PowerPoint, etc. are
skipped entirely. Add a text-extraction step so those common document types are indexed too — big relevance
win, and it makes the just-shipped configurable embedder (CPE-1273) far more useful. pdfium is ALREADY
bundled (CPE-1256), so PDF text is cheap.

## Build
- New extractor `content_text_of(path, bytes) -> Option<String>` (cpe-server) that dispatches by extension:
  - **.pdf** → extract the document's text via pdfium (`pdfium-render` exposes page text; reuse the existing
    `pdf-thumb` feature's pdfium bind in `thumb_pdf.rs`, or add a small `pdf_text` path behind the same/an
    analogous feature). Cap total extracted length (e.g. a few MB of text) to bound memory.
  - **.docx/.xlsx/.pptx** → these are ZIPs of XML; extract visible text from the relevant parts
    (`word/document.xml`, `xl/sharedStrings.xml`, `ppt/slides/*.xml`) — strip tags, concatenate text nodes.
    Reuse the existing archive/zip reading already in cpe-server (CPE-673/archive.rs) rather than a new dep.
  - **plain text / code / md / etc.** → current utf8 path (unchanged).
  - unknown/binary/unsupported → `None` (skip, as today).
- Wire `content_text_of` into `content_index.rs`'s walk (replace the raw `from_utf8_lossy` gate) so indexed
  documents contribute their extracted text; keep the per-file size cap + skip-on-error (never fail the whole
  build for one bad doc; never panic).
- Snippet read-back (content_index.rs re-reads the file for the snippet) must still work for these — either
  re-extract for the snippet or fall back to a path/filename snippet for non-plain-text docs (don't dump raw
  binary into a snippet). Handle gracefully.
- Feature-gate the pdfium/heavy bits consistent with `pdf-thumb`; base build (feature off) keeps current behavior.

## Acceptance criteria
- A folder with a PDF + a .docx containing a known word is indexed so a search for that word returns the doc.
- cargo build/test/clippy clean (all feature modes); no NEW dependency (reuse pdfium + the zip reader);
  CPE-1271 guard + bindings drift green; never panic / size-capped / skip-on-error preserved.
- Unit tests: pdf text extraction (small fixture), docx/xlsx text extraction (small fixture), size cap,
  unsupported → None, snippet doesn't emit binary.

## Notes
Directly boosts the AI content search the user is about to enable (CPE-1273). Attended: none needed beyond the
existing content-search verification (this is headless-testable with fixtures).

## Work Log
- 2026-08-03 — content_text_of dispatches PDF (thumb_pdf::extract_text via bundled pdfium) + docx/xlsx/pptx (existing zip dep + tag-strip); wired into index walk + snippet; 4M-char char-safe cap; no new dep (cargo tree identical); base feature-off byte-identical. 19 new tests; 1310 default / 1318 pdf-thumb pass. Reviewer APPROVE (char-boundary-safe by construction, never-panic, snippet-safe). Merged #572.
