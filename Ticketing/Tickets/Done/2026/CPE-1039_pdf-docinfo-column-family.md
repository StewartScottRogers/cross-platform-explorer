---
id: CPE-1039
title: Document-metadata column family (PDF Title/Author/… via read_pdf)
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-25
epic: CPE-707
estimate: 1-2h
---

## Summary
Surfaces the new PDF `/Info` read codec (CPE-1036, `media_meta_read::read_pdf`) through the
metadata-column system (epic CPE-707) so a user can add sortable **Title / Author / Subject / Keywords /
Creator / Producer / Date Created / Date Modified** columns for PDFs in the details view — exactly the
way audio tags already surface via `media_column::audio_cell`. Today `read_pdf` exists but nothing in the
column dispatcher reaches it.

New module `crates/server/src/doc_info_column.rs` (mirrors `media_column.rs`):
- `pub enum DocInfoColumn { Title, Author, Subject, Keywords, Creator, Producer, DateCreated, DateModified }`
  with a `key()` returning the friendly `MetaField::key` that `read_pdf` emits (`"Title"`, `"Author"`,
  `"Subject"`, `"Keywords"`, `"Creator"`, `"Producer"`, `"Date Created"`, `"Date Modified"`).
- `pub fn doc_info_cell(fields: &[MetaField], col: DocInfoColumn) -> CellValue` — look up the field by
  `key()`; return `CellValue::Text(trimmed)` (all doc-info fields are text), or `CellValue::Empty` when
  absent/blank (sorts last, per `metadata_column`).

Wire into `crates/server/src/column_extract.rs`:
- Add a `MetaColumn::DocInfo(DocInfoColumn)` variant.
- In `extract_column`, gate on `is_doc_ext(ext)` (pdf) and extract via `read_pdf(bytes)` + `doc_info_cell`.

## Acceptance Criteria
- [ ] `extract_column("pdf", <pdf bytes with /Info>, MetaColumn::DocInfo(DocInfoColumn::Title))` returns
      the PDF's Title as `CellValue::Text`; a non-pdf extension yields `CellValue::Empty` (gated, not
      attempted); a pdf with no `/Info` yields `Empty`.
- [ ] `doc_info_cell` unit-tested for present/absent/blank across the column variants (construct fields
      in-test, or a minimal PDF fed through `read_pdf`).
- [ ] Pure `std`, **no new deps**; `pub mod doc_info_column;` registered in `lib.rs`; style mirrors
      `media_column.rs`.
- [ ] `cargo test -p cpe-server` green; `cargo clippy -p cpe-server --all-targets -D warnings` clean in
      **both** feature modes (default and `--features specta`).

## Work Log
2026-07-25 (sprint) — Filed + dispatched. Completes the value chain for CPE-1036 (`read_pdf`) by making
it reachable/sortable as a column, using the same per-family-extractor pattern as `audio_cell`
(precedent: DocPages/VideoDuration columns already ship headlessly). The column-picker UI stays attended.

2026-07-25 (sprint) — **DONE, merged PR #356.** New `doc_info_column` module (`DocInfoColumn` +
`doc_info_cell`) + `MetaColumn::DocInfo` wired into `column_extract` (gated by `is_doc_ext`, dispatched
via `read_pdf`) — PDFs now expose sortable Title/Author/Subject/Keywords/Creator/Producer/Date columns,
mirroring the audio-column pattern. Independently reviewed (APPROVE — key() strings verified verbatim
against read_pdf's output incl. the space-bearing "Date Created"/"Date Modified") + UAT PASS (own
hand-built PDFs, extension gating, empty-sorts-last). PR was rebased off its stacked base onto main
(3-file diff). Non-blocking follow-up noted: extend the routing test to cover dates end-to-end. Clippy
clean both modes. Makes CPE-1036's read_pdf reachable in the details view.
