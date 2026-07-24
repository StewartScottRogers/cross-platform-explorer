---
id: CPE-971
title: Audio metadata → typed column cells (ID3 → CellValue)
type: feature
component: Backend
priority: medium
tags: ready
status: Done
created: 2026-07-24
epic: CPE-707
estimate: 2h
---

## Summary
The per-family column extractor CPE-707 has been missing, now that a reader exists. CPE-918
(`metadata_column`) defined the typed `CellValue` + uniform sort/format; CPE-970
(`media_meta_read::read_id3v2`) reads ID3 tags into `MetaField`s. This ticket bridges them: a pure
`cpe-server::media_column::audio_cell(fields, AudioColumn) -> CellValue` that turns a file's read audio tags
into **typed** column cells — so a *Track* or *Year* column sorts **numerically** (9 before 10), not as text.
Completes the read→column path 970 → 918 for audio, and is the pattern the image/video/doc extractors follow.

Dayshift tier-4 (2026-07-24), building directly on CPE-970.

## Design (pure, consumes CPE-970's `MetaField`, produces CPE-918's `CellValue`)
- `AudioColumn` enum: Title, Artist, Album, AlbumArtist, Track, Disc, Genre, Year, Composer, Publisher,
  Bpm, Comment — the friendly keys `read_id3v2` emits.
- `audio_cell(fields, col)`:
  - Numeric columns (Track, Disc, Year, Bpm) → `CellValue::Int` parsed from the **leading integer** of the
    tag value (`"11/12"` → 11, `"1975-06"` → 1975), so they sort numerically; unparseable → `Text` fallback.
  - Text columns → `CellValue::Text`.
  - Missing field → `CellValue::Empty` (sorts last, per CPE-918's rule).
- Case-insensitive key lookup so it's robust to a codec that varies key casing.

## Acceptance Criteria
- [x] `media_column::audio_cell` maps each `AudioColumn` to the right `CellValue` type from a `MetaField`
      slice; numeric columns (Track/Disc/Year/BPM) parse the leading integer (fallback `Text`); missing or
      blank → `Empty`. Case-insensitive key lookup.
- [x] End-to-end test (`end_to_end_from_id3_bytes_to_typed_cells`): `read_id3v2` a synthesised tag →
      `audio_cell` yields `Int` Track/Year + `Text` Title/Artist; `tracks_sort_numerically_not_lexically`
      proves 9 < 10 via `metadata_column::compare`.
- [x] Cargo-tested (5 tests); clippy `--all-targets -D warnings` clean both modes; no new deps (pure std).

## Work Log
- 2026-07-24 (dayshift) — Built `cpe-server::media_column::audio_cell` + `AudioColumn`. Closes the
  970 → 918 read→column path for audio; `leading_int` handles `"11/12"`/`"1975-06"`/`"-5dB"`; numeric-but-
  unparseable falls back to `Text` rather than dropping. 5 tests, clippy clean both modes. Image (dimensions
  primitive exists in `image_preview`) + video/doc extractors follow this `*_cell -> CellValue` shape; the
  column-picker UI stays GUI in CPE-707.

## Notes
- Consumes CPE-970 read output; siblings (image dimensions already a primitive in `image_preview`; video/doc
  extractors) follow the same `*_cell -> CellValue` shape. The column-picker UI wiring stays GUI in CPE-707.
