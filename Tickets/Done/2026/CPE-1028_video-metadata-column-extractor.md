---
id: CPE-1028
title: Video-family metadata-column extractor (MP4/MOV duration)
type: feature
component: Backend
priority: medium
tags: ready
status: Done
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
- [x] `video_cell` returns `CellValue::Float(seconds)` for a valid MP4 `moov/mvhd`; `Empty` for
      truncated/garbage/non-video bytes; never panics on malformed input.
- [x] `extract_column(ext, bytes, MetaColumn::VideoDuration)` returns the duration for a `.mp4`, and
      `Empty` for a non-video ext.
- [x] ≥4 unit tests using a **hand-built synthetic box tree** (valid mvhd v0 → known seconds; mvhd v1;
      timescale 0 → Empty; truncated → Empty). Do not require a real video file on disk.
- [x] `cargo clippy --all-targets -- -D warnings` and the `--all-features` variant both clean.

## Notes
Own **only** `video_column.rs` + your enum/match arm in `column_extract.rs` + the `mod` line in `lib.rs`.
A sibling worker (CPE-1029) also adds an arm to `column_extract.rs`'s `MetaColumn`/match — keep your arm
self-contained so the merge conflict is trivial. Do not touch `metadata_column.rs` (`CellValue::Float`
already exists). Keep the never-panic / skip-on-error convention.

## Work Log

**2026-07-25** — Implemented `crates/server/src/video_column.rs`: a bounds-checked ISO-BMFF box walker
(`read_box_header` / `find_child_box`) that descends `moov` → `mvhd`, reads `timescale`/`duration` for
both `mvhd` version 0 (32-bit fields) and version 1 (64-bit fields), guards `timescale == 0`, and returns
`CellValue::Float(duration / timescale)` seconds — `CellValue::Empty` on any malformed/truncated/non-BMFF
input (never panics; every slice is bounds-checked before use). Registered `pub mod video_column;` in
`lib.rs` next to `image_column`. Wired into `column_extract.rs`: added `MetaColumn::VideoDuration`, an
`is_video_ext` guard (`mp4`/`mov`/`m4v`), and the dispatch arm calling `video_cell` when the extension
matches, else `Empty`.

Tests: 8 unit tests in `video_column.rs` (valid mvhd v0 → known seconds; mvhd v1 64-bit → known seconds;
`timescale == 0` → `Empty`; truncated bytes at 8 different cut points → `Empty`, never panics; non-BMFF
bytes → `Empty`; missing `moov`/`mvhd` → `Empty`; unrecognised `mvhd` version → `Empty`; a box declaring a
size larger than the available bytes → `Empty`), plus a routing test in `column_extract.rs` covering
`extract_column` gating by extension.

Verification (Windows, from `src-tauri` workspace root unless noted):
- `cargo test -q` in `crates/server` (ran directly since `cpe-server` isn't a workspace test member from
  `src-tauri`): **635 passed, 0 failed** (includes the 8 new `video_column` tests + 1 new
  `column_extract` routing test).
- `cargo clippy --all-targets -- -D warnings`: clean, exit 0.
- `cargo clippy --all-targets --all-features -- -D warnings`: clean, exit 0.

No new dependencies added. Touched only `video_column.rs` (new), the one `pub mod video_column;` line in
`lib.rs`, the `MetaColumn::VideoDuration` variant + `is_video_ext` + match arm (+ one routing test) in
`column_extract.rs`, and this Work Log.
