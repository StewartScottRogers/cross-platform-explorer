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
- [x] `read_exif` returns group-`"exif"` `MetaField`s for a real JPEG/TIFF EXIF block (Make/Model/
      DateTimeOriginal etc.), with camera intrinsics `editable:false` and descriptive tags `editable:true`.
- [x] Non-image / no-EXIF / truncated bytes ⇒ empty vec; never panics (bounds/error-safe like `read_id3v2`).
- [x] ≥4 unit tests. Build a minimal EXIF fixture in-test (a tiny JPEG/TIFF with a known Make/Model +
      DateTimeOriginal — synthesise the bytes, or embed a small constant byte array); assert the mapped
      fields + editable flags; plus non-EXIF → empty and truncated → empty (no panic).
- [x] No new dependency (reuse `kamadak-exif`). `cargo clippy --all-targets -- -D warnings` and
      `--all-features` both clean.

## Notes
Add to the EXISTING `media_meta_read.rs` (one new `pub fn` + tests) — no new module, no `lib.rs` change.
Do NOT touch `media_meta_edit.rs` (`MetaField` as-is) or `column_extract.rs`. Wiring EXIF into an image
metadata column / the studio is a later slice. Keep the never-panic / skip-on-error convention.

## Work Log

**2026-07-25** — Implemented `pub fn read_exif(bytes: &[u8]) -> Vec<MetaField>` in
`crates/server/src/media_meta_read.rs`, following `read_id3v2`'s shape: pure, bytes-in, bounds/error-safe,
never panics, empty vec on absence. Uses `exif::Reader::new().read_from_container(&mut
std::io::Cursor::new(bytes))` from the existing `kamadak-exif = "0.6"` dependency (no new dep added) —
`read_from_container` auto-detects TIFF/JPEG/PNG/WebP/HEIF containers and locates the embedded Exif block;
any error (unknown format / no Exif / truncated) is mapped to an empty vec via a single `let Ok(exif) =
... else { return Vec::new() }`.

A static `EXIF_TAGS: &[(exif::Tag, &str, bool)]` table maps `Make`, `Model`, `LensModel`,
`DateTimeOriginal`, `ExposureTime`, `FNumber`, `PhotographicSensitivity` (key `"ISO"`), `FocalLength`,
`Orientation`, `PixelXDimension`, `PixelYDimension`, `GPSLatitude`/`GPSLongitude` (key `"GPS
Latitude"`/`"GPS Longitude"`) to `editable:false`, and `ImageDescription`, `Artist`, `Copyright`,
`UserComment` to `editable:true` — matching the ticket's exact split (LensModel was grouped with the
camera intrinsics since the ticket's editable list only names the four descriptive tags). Each present
tag's value is `field.display_value().with_unit(&exif).to_string()`, skipped when blank; the group is
always `"exif"`.

**Fixture strategy**: rather than hand-typing a byte-exact JPEG, the test module builds a minimal
*raw little-endian TIFF/Exif container* in-test (`build_exif_tiff` + `RawEntry`/`ascii_entry`/
`short_entry`/`long_entry`/`rational_entry`/`rational3_entry` helpers). `exif::Reader::read_from_container`
recognises raw TIFF bytes directly via the `II*\0` signature (this is exactly what a `.tif` file's header
— and a JPEG's embedded Exif APP1 payload — look like), so no JPEG wrapping was needed. The builder is a
small two-pass layout engine: it serializes IFD0, an Exif sub-IFD (pointed to by `ExifIFDPointer`), and a
GPS sub-IFD (pointed to by `GPSInfoIFDPointer`), computing each sub-IFD's absolute file offset from the
preceding blocks' sizes before patching the pointer entries — a generic, reusable builder rather than
manually hand-computed offsets, so it's easy to extend and hard to get subtly wrong.

Verified the crate's own `Display` formatting by reading its source (`tag.rs`/`value.rs` in
`kamadak-exif-0.6.1`) rather than guessing: ASCII fields render **quoted** (e.g. `"Acme"`, via the
crate's `d_sub_ascii`), rational fields carry their declared unit suffix from each tag's `unit!` macro
(`ExposureTime` → `"1/200 s"`, `FNumber` → `"f/2.8"`, `FocalLength` → `"50 mm"`,
`PixelXDimension`/`PixelYDimension` → `"4000 pixels"`/`"3000 pixels"`), and GPS DMS fields append their
Ref tag (`"37 deg 46 min 30 sec N"`). Tests assert these exact strings — genuine positive coverage of the
real parse+format pipeline, not guessed values.

Added 5 unit tests (exceeds the ≥4 minimum):
1. `read_exif_maps_common_fields_with_editable_flags` — full fixture (IFD0 + Exif sub-IFD + GPS sub-IFD)
   asserting all 13 intrinsic keys' exact display values, group `"exif"`, and `editable:false`, plus
   `ImageDescription`'s `editable:true`.
2. `read_exif_maps_descriptive_tags_as_editable` — Artist/Copyright/UserComment `editable:true`, and
   LensModel confirmed `editable:false` (a camera intrinsic, not in the ticket's descriptive list).
3. `read_exif_skips_absent_gps_and_descriptive_tags` — a Make/Model-only fixture confirms no GPS/
   descriptive/exposure keys appear when their tags are absent (`f.len() == 2`).
4. `read_exif_returns_empty_for_non_image_bytes` — empty bytes, plain text, and a real ID3 tag (valid
   *audio* metadata, not an image) all yield an empty vec.
5. `read_exif_tolerates_truncation_without_panicking` — every truncation length of the full fixture is
   fed through `read_exif` (no panic asserted structurally by the loop itself), plus a header-only cut
   asserts an explicit empty vec.

**Verification** (run from `crates/server`, cargo at `%USERPROFILE%\.cargo\bin\cargo.exe`):
- `cargo test -q read_exif` → 5 passed, 0 failed.
- `cargo test -q media_meta_read` → 21 passed, 0 failed (16 existing + 5 new, no regressions).
- `cargo test -q` (full `cpe-server` suite) → 662 passed, 0 failed.
- `cargo clippy --all-targets -- -D warnings` → clean.
- `cargo clippy --all-targets --all-features -- -D warnings` → clean.

No new dependency added. Scope held to `crates/server/src/media_meta_read.rs` (one new `pub fn` + the
`EXIF_TAGS` table + tests) and this Work Log — `media_meta_edit.rs`, `column_extract.rs`, `lib.rs`, and
`Cargo.toml` were not touched.
