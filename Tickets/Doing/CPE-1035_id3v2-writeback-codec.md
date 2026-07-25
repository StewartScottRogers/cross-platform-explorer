---
id: CPE-1035
title: ID3v2 write-back codec (edit audio tags → new ID3v2 tag)
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
First **write-back** codec for the media-metadata studio (epic CPE-725). Today the studio can *read*
ID3v2 audio tags (`media_meta_read::read_id3v2`) and *apply an edit policy* over `MetaField`s
(`media_meta_edit::apply_edits`), but there is no way to serialise edited fields back into a real file.
This adds a pure, headless **ID3v2 writer**: given the original file bytes and the desired
`MetaField`s, produce new file bytes carrying a freshly-built ID3v2 tag, with the audio payload
preserved byte-for-byte.

New module `crates/server/src/media_meta_write.rs`:
- `pub fn write_id3v2(orig: &[u8], fields: &[MetaField]) -> Vec<u8>` — build a valid **ID3v2.4** tag
  from the `group == "id3"` fields and prepend it to the original audio payload (the bytes *after* any
  existing ID3v2 tag in `orig`, so re-writing is idempotent, not cumulative). Non-`id3` fields are
  ignored. A robust never-panic implementation.
- Reverse the `friendly_key` map (Title→TIT2, Artist→TPE1, …, Comment→COMM); a friendly key with no
  known frame id that is a raw 4-char `T...` id is written under that id; anything unmappable is skipped
  (reported is not required — this is the serialiser, the policy layer already gated edits).
- Text frames: encoding byte `0x03` (UTF-8, valid in v2.4) + UTF-8 bytes. `COMM`: encoding(1)+lang
  `"eng"`(3)+empty description NUL + UTF-8 text. Frame size = **syncsafe28**. Tag size = syncsafe28 of
  the frame block. No extended header, no unsync, no padding required.

## Acceptance Criteria
- [ ] `write_id3v2(orig, fields)` returns bytes whose leading ID3v2 tag, when fed back through
      `read_id3v2`, yields the same editable fields (round-trip: read → edit via `apply_edits` → write →
      read again == expected). Cover v2.3-input / v2.4-output, a file with **no** existing tag, and a
      file **with** an existing tag (old tag replaced, not stacked).
- [ ] Audio payload after the tag is preserved byte-for-byte; writing twice is idempotent.
- [ ] Never panics on empty/short/garbage `orig`; pure `std`, **no new deps**; registered
      `pub mod media_meta_write;` in `crates/server/src/lib.rs`.
- [ ] `cargo test -p cpe-server` green; `cargo clippy -p cpe-server --all-targets -D warnings` clean in
      **both** feature modes (default and `--features specta`).

## Work Log
2026-07-25 (workshift) — Filed + dispatched to a worker. Pairs `read_id3v2` (CPE-970) + `apply_edits`
(CPE-942); the flagship write-back that turns the studio editable. Follow-ups (separate tickets):
Vorbis/FLAC/OGG write-back, EXIF write-back.
