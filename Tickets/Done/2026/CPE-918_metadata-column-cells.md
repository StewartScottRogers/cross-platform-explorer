---
id: CPE-918
title: Metadata-column cell model (uniform sort + format)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-707
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
First headless slice of custom & metadata columns (CPE-707) — the "sort/format integration so new columns
sort and render like built-ins" seam. `cpe_server::metadata_column`:
- `enum CellValue { Text, Int, Float, Bytes, Dimensions{w,h}, Empty }` — the typed value a family
  extractor yields per row.
- `display(placeholder)` — human format (bytes 1024-based `1.5 KB`, `w × h`, trimmed floats; `Empty` →
  placeholder).
- `compare(a, b, ascending)` + `sort_rows(rows, key, ascending)` — type-aware ordering (numeric not
  lexical, Text case-insensitive, Dimensions by area-then-width) with **Empty pinned last in both
  directions**.

Family extractors (image dims / audio bitrate / page count / duration) all plug into this one cell type so
they sort and render like the built-in name/size columns.

## Acceptance Criteria
- [x] Each variant formats correctly; Ints/Bytes sort numerically (not "10" < "9"); Dimensions by area.
- [x] Empty always sorts last ascending AND descending. 5 unit tests; clippy `-D warnings` clean.

## Work Log
- 2026-07-22 — Activated CPE-707 with the column cell/sort/format core. The per-family Rust extractors
  (lazy, visible-rows-only) + the column-picker UI + per-folder persistence are the remainder.
