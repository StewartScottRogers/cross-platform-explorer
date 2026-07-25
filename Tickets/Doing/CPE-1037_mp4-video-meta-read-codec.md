---
id: CPE-1037
title: MP4/MOV video metadata read codec (iTunes atoms → MetaFields)
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-725
estimate: 2-3h
---

## Summary
Extends the media-metadata studio's **read** coverage to **video** (epic CPE-725). Adds a pure,
headless MP4/MOV (ISO-BMFF / QuickTime) metadata reader that surfaces the iTunes-style `moov ▸ udta ▸
meta ▸ ilst` atoms — title (`©nam`), artist (`©ART`), album (`©alb`), year (`©day`), comment (`©cmt`),
genre (`©gen`/`gnre`), etc. — plus basic container facts (major brand from `ftyp`), as `MetaField`s in
group `"video"`. Also feeds CPE-707 columns.

New module `crates/server/src/video_meta_read.rs`:
- `pub fn read_mp4(bytes: &[u8]) -> Vec<MetaField>` — a bounded ISO-BMFF **box/atom walker**: each box is
  `size(4, big-endian) + type(4)`; `size==1` means a 64-bit largesize follows; `size==0` means "to EOF".
  Descend `moov → udta → meta → ilst`; note `meta` has a 4-byte version/flags prelude before its child
  boxes in the QuickTime/MP4 `udta.meta` layout — handle it defensively (probe both). Each `ilst` entry
  is a box whose type is the tag key, containing a `data` box: `type(4)="data" + version/flags(4) +
  reserved(4) + payload`. Decode text payloads (type flag 1 = UTF-8) to String.
- Map the common atoms to friendly keys (`©nam`→"Title", `©ART`→"Artist", `©alb`→"Album", `©day`→"Year",
  `©cmt`→"Comment", `©gen`→"Genre", `©wrt`→"Composer", `©too`→"Encoder", `cprt`→"Copyright"). All
  fields `group: "video"`, `editable: false` (write-back is a later ticket). Skip unknown atoms.
- Robust + never-panic: every box length is clamped to the buffer; a lying/huge size can't over-read;
  cap descent depth. Returns empty when there's no `ftyp`/`moov`.

## Acceptance Criteria
- [ ] `read_mp4(bytes)` on an in-test-constructed minimal MP4 (`ftyp` + `moov/udta/meta/ilst` with a
      couple of `©nam`/`©ART` `data` atoms) returns the expected Title/Artist fields in group `"video"`.
- [ ] Returns empty (never panics) for: non-MP4 bytes, truncated/garbage boxes, a box with a lying
      oversized length, and an MP4 with no `udta`.
- [ ] Pure `std`, **no new deps**; registered `pub mod video_meta_read;` in
      `crates/server/src/lib.rs`; matches the `MetaField` shape for later `column_extract` reuse.
- [ ] `cargo test -p cpe-server` green; `cargo clippy -p cpe-server --all-targets -D warnings` clean in
      **both** feature modes (default and `--features specta`).

## Work Log
2026-07-25 (workshift) — Filed + dispatched. Disjoint new file (`video_meta_read.rs`) so it runs safely
in parallel with CPE-1035 (`media_meta_write.rs`) and CPE-1036 (`read_pdf` in `media_meta_read.rs`).
Completes the read arc: audio (ID3/FLAC/OGG) + images (EXIF) + documents (PDF) + video (MP4/MOV).
