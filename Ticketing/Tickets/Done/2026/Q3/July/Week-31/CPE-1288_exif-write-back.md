---
id: CPE-1288
title: "EXIF write-back codec (JPEG/TIFF)"
type: feature
component: cpe-server
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-725
---

## Summary
`media_meta` reads EXIF for jpg/jpeg/tif/tiff (`read_exif`) but `is_writable`/`write_back` only support
mp3/flac — EXIF fields can be read but not saved. Add an EXIF write-back codec so the editable EXIF tags
round-trip. Headless, cargo round-trip-tested.

## Build
- New `pub fn write_exif(orig: &[u8], edits: &[MetaEdit]) -> Result<Vec<u8>, String>` in
  `crates/server/src/media_meta_write.rs` that rebuilds the editable EXIF/IFD tags (the fields `read_exif`
  marks editable — ImageDescription/Artist/Copyright/UserComment) into a JPEG APP1 segment, replacing the
  existing APP1 (strip-and-replace) while preserving the rest of the image byte-for-byte.
- Wire `"jpg" | "jpeg" | "tif" | "tiff"` into `media_meta::write_back` and add them to `is_writable`
  (`crates/server/src/media_meta.rs`).
- Reuse the vendored `kamadak-exif` crate already used by `read_exif` (NO new dependency). Never panic on a
  malformed image (return `Err`, don't unwrap).
- Match the module's existing `MetaEdit`/result-fields flow (see how `write_id3v2` / `write_flac` are
  invoked from `write_back`).

## Acceptance criteria
- A round-trip test (read EXIF → edit a field via `write_back` → read back) shows the edited value, with the
  image payload otherwise intact; `is_writable("jpg")` is now true.
- `cargo test -p cpe-server` green (round-trip + a malformed-input Err case); `cargo clippy` clean both
  feature modes (`-D warnings`, and `--features specta`); no new dep; existing mp3/flac write tests still
  pass.

## Notes
Format-risky (IFD offset rewriting) — flagged for careful review. Epic CPE-725. Shares `media_meta.rs` +
`media_meta_write.rs` with CPE-1289 (OGG write) → sequence that after this.

## Work Log
- 2026-08-03 — EXIF write-back merged (#587) after 1 rework. First review (opus) caught a BLOCKER: rebuilt EXIF from only 4 editable tags -> silently destroyed GPS/DateTime/Orientation/Make on edit (proven with probe). Fixed: re-emit all IFD0+Exif/GPS/Interop sub-IFD fields, override only editable, no dup; +regression test (Make+GPS+Orientation survive, fails-on-old) + UserComment round-trip. Re-review APPROVE (added own sub-IFD probe). JPEG marker-walk panic-safe. TIFF deferred. 1391 green. Gauntlet caught real data-loss pre-merge.
