---
id: CPE-707
title: "EPIC: Custom & metadata columns"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Extend the details view with rich, sortable, per-folder columns pulled from file internals: image
dimensions/EXIF, audio ID3 (artist/album/bitrate), video length, PDF page count, and OS extended
attributes/permissions.

## Why
Directory Opus / Finder power users live in metadata columns. The app already has a column framework and
sort infra (CPE-017, columns.ts) to build on; this is the natural next layer.

## Rough scope (areas, not child tickets)
- Rust metadata extractors per family (image/audio/video/document), lazy + streamed per visible row.
- A column-picker UI (add/remove/reorder, per-folder persistence) reusing the columns model.
- Sort/format integration so new columns sort and render like built-ins.
- Coordinate with virtualization (CPE-690) so only visible rows extract metadata.

## Open questions (resolve at activation)
- Extraction cost vs. the 10× perf epic — must only extract for on-screen rows and cache results.
- Per-folder vs. global column sets; how columns persist across sessions.
- Overlap with the media-metadata studio ([[CPE-725]]) — display here, editing there.

## Definition of Done
- Users can add metadata columns from a picker; they sort and format correctly.
- Metadata is extracted lazily for visible rows only, with no regression to open/scroll speed.
- Column choices persist per folder (or per configured scope).

## Work Log
2026-07-22 (nightshift) — **Activated.** First slice: **CPE-918** — `metadata_column::CellValue` + `compare`
/ `sort_rows` / `display`: the typed cell every metadata column produces, with uniform type-aware sort
(numeric, not lexical; Dimensions by area; Empty pinned last both directions) and human formatting. This is
the "sort/format like built-ins" seam. Remaining: per-family Rust extractors (image/audio/video/doc, lazy
for visible rows only), the column-picker UI (add/remove/reorder), and per-folder persistence.

2026-07-24 (dayshift) — **CPE-971** landed the first per-family extractor: `media_column::audio_cell` maps
read ID3 tags (via CPE-970) to typed `CellValue`s so Track/Year columns sort numerically. Establishes the
`*_cell -> CellValue` pattern. Remaining: image (a dimensions primitive already exists in `image_preview`) /
video / doc extractors, the column-picker UI, and per-folder persistence.

2026-07-24 (dayshift) — **CPE-974** added the image-family extractor: `image_column::image_dimensions_cell` (header-only read → `CellValue::Dimensions`, sorts by area). With audio (CPE-971) + image, the two commonest per-family extractors are covered. Remaining: video/doc extractors, the column-picker UI (GUI), and per-folder persistence.

2026-07-24 (dayshift) — **CPE-975** added the dispatcher `column_extract::extract_column(ext, bytes, MetaColumn)` — the single seam routing a file to its per-family extractor (audio ID3/FLAC/OGG → typed audio cell; image header → Dimensions). Adding video/doc later is one more arm. Remaining: video/doc extractors, the column-picker UI (GUI), per-folder persistence.
