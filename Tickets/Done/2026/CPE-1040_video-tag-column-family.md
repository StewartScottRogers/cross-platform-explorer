---
id: CPE-1040
title: Video-tag column family (MP4/MOV Title/Artist/… via read_mp4)
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
Mirror of CPE-1039 (PDF doc-info columns) for **video**: surfaces the MP4/MOV iTunes-tag read codec
(CPE-1037, `media_meta_read::read_mp4`) through the metadata-column system so a user can add sortable
**Title / Artist / Album / Year / Comment / Genre / Composer / Encoder / Copyright** columns for video
files in the details view. Today `read_mp4` exists but nothing in the column dispatcher reaches it
(`MetaColumn::VideoDuration` covers only duration, a different codec).

New module `crates/server/src/video_tag_column.rs` (mirrors `media_column.rs` / `doc_info_column.rs`):
- `pub enum VideoTagColumn { Title, Artist, Album, Year, Comment, Genre, Composer, Encoder, Copyright }`
  with `key()` returning the friendly `MetaField::key` `read_mp4` emits (verbatim: `"Title"`, `"Artist"`,
  `"Album"`, `"Year"`, `"Comment"`, `"Genre"`, `"Composer"`, `"Encoder"`, `"Copyright"`).
- `pub fn video_tag_cell(fields: &[MetaField], col: VideoTagColumn) -> CellValue` — look up by `key()`;
  **Year is numeric** (`CellValue::Int` from the value's leading integer, like `media_column`'s Year, so
  it sorts 1999 < 2001; fall back to Text when no leading number); all others `CellValue::Text`; absent/
  blank → `CellValue::Empty`.

Wire into `crates/server/src/column_extract.rs`:
- Add a `MetaColumn::VideoTag(VideoTagColumn)` variant.
- In `extract_column`, gate on the existing `is_video_ext(ext)` (mp4/mov/m4v) and extract via
  `read_mp4(bytes)` + `video_tag_cell`.

## Acceptance Criteria
- [ ] `extract_column("mp4", <mp4 with ilst tags>, MetaColumn::VideoTag(VideoTagColumn::Title))` returns
      the video's Title as `CellValue::Text`; `VideoTagColumn::Year` returns `CellValue::Int`; a non-video
      extension yields `CellValue::Empty` (gated, not attempted); an MP4 with no `udta`/tags yields
      `Empty`.
- [ ] `video_tag_cell` unit-tested for present/absent/blank + the numeric-Year path (construct fields
      in-test).
- [ ] Pure `std`, **no new deps**; `pub mod video_tag_column;` registered in `lib.rs`; style mirrors
      `doc_info_column.rs` / `media_column.rs`.
- [ ] `cargo test -p cpe-server` green; `cargo clippy -p cpe-server --all-targets -D warnings` clean in
      **both** feature modes (default and `--features specta`).

## Work Log
2026-07-25 (workshift) — Filed + dispatched after CPE-1039 merged (column_extract.rs now free). Completes
the read→column value chain for CPE-1037's `read_mp4`, same per-family-extractor pattern as `audio_cell`.
Column-picker UI stays attended.

2026-07-25 (workshift) — **DONE, merged PR #357.** New `video_tag_column` module + `MetaColumn::VideoTag`
wired into `column_extract` (gated by `is_video_ext`, dispatched via `read_mp4`) — MP4/MOV videos now
expose sortable Title/Artist/Album/Year/Comment/Genre/Composer/Encoder/Copyright columns; Year is numeric
(Int, sorts 1999<2001). Faithful mirror of CPE-1039. Independently reviewed (APPROVE — all 9 key() strings
verified verbatim vs read_mp4, numeric-Year confirmed) + UAT PASS (own MP4 builder, Int(2018) from
"2018-06", numeric sort). Clippy clean both modes. Read→column value chain now complete for both new
codecs (PDF + video).
