---
id: CPE-974
title: Image dimensions → typed column cell (image-family extractor)
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-707
estimate: 1h
---

## Summary
The image-family per-column extractor for CPE-707, counterpart to the audio one (CPE-971). CPE-918
(`metadata_column`) already sorts a `CellValue::Dimensions` cell by pixel area; this adds
`cpe-server::image_column::image_dimensions_cell(bytes) -> CellValue` that reads an image's **header only**
(no full decode) to produce it — cheap enough to fill per visible row.

Dayshift tier-4 (2026-07-24), extending the CPE-707 `*_cell -> CellValue` family beyond audio.

## Design
- `image_dimensions_cell(bytes)` — `image::ImageReader` guesses the format and reads `(w,h)` from the header
  without decoding pixels; success → `CellValue::Dimensions { w, h }`, unrecognised/garbled → `CellValue::
  Empty` (sorts last). Reuses the already-vendored `image` crate (no new dep).
- Pure over bytes: the adapter reads the leading bytes + dispatches by kind; no filesystem here.

## Acceptance Criteria
- [x] `image_dimensions_cell` returns `Dimensions{w,h}` for PNG/BMP/GIF, `Empty` for non-images (no panic on
      a bogus magic + garbage body).
- [x] Header-only read (no full decode); `Dimensions` cells sort by area via `metadata_column::compare`
      (asserted).
- [x] Cargo-tested (4 tests, fixtures encoded in-test via the `image` crate — no external files); clippy
      `--all-targets -D warnings` clean both modes; no new deps.

## Notes
- With audio (CPE-971) + image here, CPE-707's per-family extractors cover the two commonest kinds; video/doc
  extractors + the column-picker UI + per-folder persistence remain (UI is GUI).

## Work Log
- 2026-07-24 (dayshift) — Built `image_column::image_dimensions_cell` reusing the `image` crate's header
  reader. 4 tests (PNG/BMP/GIF + non-image + area-sort), clippy clean both modes.
