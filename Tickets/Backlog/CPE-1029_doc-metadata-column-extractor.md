---
id: CPE-1029
title: Document-family metadata-column extractor (PDF page count)
type: feature
component: Backend
priority: medium
tags: ready
status: Backlog
created: 2026-07-25
epic: CPE-707
estimate: 1-2h
---

## Summary
Epic CPE-707 (custom metadata columns) slice. Audio (CPE-971) and image (CPE-974) per-family extractors
already exist, dispatched by `column_extract::extract_column(ext, bytes, MetaColumn)`. Add the **document**
family: a new pure module `crates/server/src/doc_column.rs` that reads a **PDF** file's bytes and returns
its **page count** as a typed `CellValue::Int`, then wire it into the dispatcher so a "Pages" column sorts
numerically.

Follow the established shape exactly — study `image_column.rs` (`image_dimensions_cell`) and
`column_extract.rs` first. Pure: no filesystem, the adapter supplies the bytes. **No new dependency**
(byte scan only — do not pull a PDF crate).

## Design
- New `pub fn doc_pages_cell(bytes: &[u8]) -> CellValue` in `doc_column.rs`:
  - Count PDF pages by scanning the raw bytes for page objects. Robust primary heuristic: count
    occurrences of the `/Type` `/Page` object marker — i.e. `/Type` followed by optional whitespace then
    `/Page` that is **not** immediately followed by `s` (to exclude `/Pages`, the tree node). Return
    `CellValue::Int(count)` when `count >= 1`.
  - If no `/Page` markers are found (e.g. object streams / compressed xref), fall back to `Empty` rather
    than guessing. Non-PDF bytes (no `%PDF` header) ⇒ `Empty`. Never panic; bounds-check all slicing.
- Register `pub mod doc_column;` in `lib.rs`.
- In `column_extract.rs`: add a `MetaColumn::DocPages` variant, an `is_doc_ext` guard (`pdf` for v1), and
  the match arm calling `doc_pages_cell` when the ext is a doc kind else `Empty`.

## Acceptance Criteria
- [ ] `doc_pages_cell` returns `CellValue::Int(n)` counting `/Type /Page` objects in a synthetic PDF body,
      correctly **excluding** the `/Pages` tree node; `Empty` for non-PDF / no-page-marker bytes; never
      panics.
- [ ] `extract_column(ext, bytes, MetaColumn::DocPages)` returns the count for a `.pdf`, `Empty` for a
      non-doc ext.
- [ ] ≥4 unit tests with hand-built PDF byte fixtures (3 pages → Int(3); a `/Pages` node present but not
      miscounted; non-PDF → Empty; empty input → Empty). No new dependency.
- [ ] `cargo clippy --all-targets -- -D warnings` and the `--all-features` variant both clean.

## Notes
Own **only** `doc_column.rs` + your enum/match arm in `column_extract.rs` + the `mod` line in `lib.rs`.
A sibling worker (CPE-1028) also adds an arm to `column_extract.rs`'s `MetaColumn`/match — keep your arm
self-contained so the merge conflict is trivial. Do not touch `metadata_column.rs` (`CellValue::Int`
already exists) or the existing `doc_text.rs` text extractors. Keep the never-panic convention.
