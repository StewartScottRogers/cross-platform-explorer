---
id: CPE-1028
title: Video-family metadata-column extractor (MP4/MOV duration)
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
already exist, dispatched by `column_extract::extract_column(ext, bytes, MetaColumn)`. Add the **video**
family: a new pure module `crates/server/src/video_column.rs` that reads an **MP4/MOV (ISO-BMFF)** file's
leading bytes and returns its **duration in seconds** as a typed `CellValue`, then wire it into the
dispatcher so a "Duration" column sorts numerically.

Follow the established shape exactly — study `image_column.rs` (`image_dimensions_cell`) and
`column_extract.rs` first. Pure: no filesystem, the adapter supplies the bytes.

## Design
- New `pub fn video_cell(bytes: &[u8]) -> CellValue` in `video_column.rs`:
  - Walk the ISO-BMFF box tree (each box = `u32 size` big-endian + 4-byte type, `size==1` ⇒ 64-bit
    largesize; `size==0` ⇒ to EOF). Descend into the `moov` container box and read the **`mvhd`** box.
  - From `mvhd` (version 0: 32-bit fields; version 1: 64-bit) read `timescale` and `duration`; return
    `CellValue::Float(duration as f64 / timescale as f64)` seconds. Guard against `timescale == 0`.
  - Any parse failure / truncation / non-BMFF bytes ⇒ `CellValue::Empty` (never panic; bounds-check every
    slice — heed [[cpe-server-logic-audited]]: no `bytes[a..b]` without checking `b <= len`).
- Register `pub mod video_column;` in `lib.rs` (alongside `image_column`).
- In `column_extract.rs`: add a `MetaColumn::VideoDuration` variant, an `is_video_ext` guard
  (`mp4`, `mov`, `m4v`, `m4a`? — no, m4a is audio; use `mp4`/`mov`/`m4v`), and the match arm calling
  `video_cell` when the ext is video else `Empty`.

## Acceptance Criteria
- [ ] `video_cell` returns `CellValue::Float(seconds)` for a valid MP4 `moov/mvhd`; `Empty` for
      truncated/garbage/non-video bytes; never panics on malformed input.
- [ ] `extract_column(ext, bytes, MetaColumn::VideoDuration)` returns the duration for a `.mp4`, and
      `Empty` for a non-video ext.
- [ ] ≥4 unit tests using a **hand-built synthetic box tree** (valid mvhd v0 → known seconds; mvhd v1;
      timescale 0 → Empty; truncated → Empty). Do not require a real video file on disk.
- [ ] `cargo clippy --all-targets -- -D warnings` and the `--all-features` variant both clean.

## Notes
Own **only** `video_column.rs` + your enum/match arm in `column_extract.rs` + the `mod` line in `lib.rs`.
A sibling worker (CPE-1029) also adds an arm to `column_extract.rs`'s `MetaColumn`/match — keep your arm
self-contained so the merge conflict is trivial. Do not touch `metadata_column.rs` (`CellValue::Float`
already exists). Keep the never-panic / skip-on-error convention.
