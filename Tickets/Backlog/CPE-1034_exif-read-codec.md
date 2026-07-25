---
id: CPE-1034
title: EXIF read codec (image metadata → MetaFields)
type: feature
component: Backend
priority: medium
tags: ready
status: Backlog
created: 2026-07-25
epic: CPE-725
estimate: 1-2h
---

## Summary
Epic CPE-725 (media metadata studio); also feeds CPE-707 columns. The audio read-codec arc is done
(CPE-970 ID3, 972 FLAC, 973 OGG → `media_meta_read`). The next read codec is **EXIF** for images. Add
`pub fn read_exif(bytes: &[u8]) -> Vec<MetaField>` to `crates/server/src/media_meta_read.rs`, following the
`read_id3v2` shape exactly: pure, bytes-in, group `"exif"`, bounds-checked, **never panics**, empty vec when
there's no EXIF.

**Reuse the existing `kamadak-exif` crate** (already a dependency — `kamadak-exif = "0.6"` in
`crates/server/Cargo.toml`). Do NOT hand-roll a TIFF/IFD parser and do NOT add a new dep.

## Design
- `read_exif(bytes)`: use `exif::Reader::new().read_from_container(&mut Cursor::new(bytes))` (or the 0.6
  equivalent — check the crate's API). On any error (not an image / no EXIF / truncated) return an **empty
  vec**, never propagate a panic.
- Map the common, useful fields to `MetaField`s in group `"exif"` with friendly keys, e.g.:
  `Make`, `Model`, `LensModel`, `DateTimeOriginal`, `ExposureTime`, `FNumber`, `ISO`/`PhotographicSensitivity`,
  `FocalLength`, `Orientation`, `PixelXDimension`/`PixelYDimension`, and GPS lat/long if present. Use each
  field's display value (`Field::display_value().with_unit(&exif)` → string).
- **Editable flag:** camera-set intrinsics (Make/Model/dimensions/exposure/etc.) are **read-only**
  (`editable: false`); descriptive tags that a user legitimately edits — `ImageDescription`, `Artist`,
  `Copyright`, `UserComment` — are `editable: true`. (Matches the `MetaField` doc comment's guidance.)
- Keep the friendly-key naming consistent with the existing codecs' style. Don't emit fields with empty
  values.

## Acceptance Criteria
- [ ] `read_exif` returns group-`"exif"` `MetaField`s for a real JPEG/TIFF EXIF block (Make/Model/
      DateTimeOriginal etc.), with camera intrinsics `editable:false` and descriptive tags `editable:true`.
- [ ] Non-image / no-EXIF / truncated bytes ⇒ empty vec; never panics (bounds/error-safe like `read_id3v2`).
- [ ] ≥4 unit tests. Build a minimal EXIF fixture in-test (a tiny JPEG/TIFF with a known Make/Model +
      DateTimeOriginal — synthesise the bytes, or embed a small constant byte array); assert the mapped
      fields + editable flags; plus non-EXIF → empty and truncated → empty (no panic).
- [ ] No new dependency (reuse `kamadak-exif`). `cargo clippy --all-targets -- -D warnings` and
      `--all-features` both clean.

## Notes
Add to the EXISTING `media_meta_read.rs` (one new `pub fn` + tests) — no new module, no `lib.rs` change.
Do NOT touch `media_meta_edit.rs` (`MetaField` as-is) or `column_extract.rs`. Wiring EXIF into an image
metadata column / the studio is a later slice. Keep the never-panic / skip-on-error convention.
