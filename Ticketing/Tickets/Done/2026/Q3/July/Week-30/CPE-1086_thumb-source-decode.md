---
id: CPE-1086
title: "Thumbnail source decode (PSD + bomb-guard + orientation) — cpe_server::thumb_source + thumbnail integration"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-718
depends-on: CPE-1085
---

## Summary
Child of CPE-718 (Universal thumbnail pipeline). Today `make_thumbnail_png` uses `image::open`, which can't
decode PSD (→ generic-icon fallback) and ignores EXIF orientation. Add a source-decode module that handles
PSD (via vendored `psd`), guards raster decode with `image::Limits`, and returns bytes so orientation can be
applied — then wire it + CPE-1085's orientation into `make_thumbnail_png`. **Pure** in `crates/server`,
`cargo test` on the 3-OS matrix — no GUI, no user resource, **no new deps** (psd/image/kamadak-exif vendored).
**Depends on CPE-1085** — dispatch after it merges.

## Design (buildable)
1. New module `crates/server/src/thumb_source.rs`, registered `pub mod thumb_source;` in `lib.rs` **immediately
   after `pub mod thumb_cache;`** (distinct anchor from CPE-1085's).
   ```rust
   use std::path::Path; use image::DynamicImage;
   /// Decode a thumbnail source to an image + its raw bytes (bytes let the caller read EXIF orientation).
   pub fn decode_thumb_image(path: &Path) -> Result<(DynamicImage, Vec<u8>), String>;
   ```
   - Read bytes once (`std::fs::read`, map err to String).
   - If ext (lowercased) is `psd`: `psd::Psd::from_bytes(&bytes)` → composite RGBA → `image::RgbaImage::from_raw`
     → `DynamicImage` (mirror `image_preview.rs` ~lines 19-24). PSD bytes yield no EXIF → orientation no-op, fine.
   - Else: decode via `image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?` with the
     **`image::Limits` bomb guard** (reuse batch_transform's `bounded_limits` pattern: max 20k px / 256 MiB —
     copy the limits locally since it's private) so a decompression-bomb returns `Err`, not OOM.
2. Rewrite the body of `make_thumbnail_png` in `crates/server/src/thumbnail.rs` (the ONE integration edit,
   owned solely by this ticket): `let (img, bytes) = thumb_source::decode_thumb_image(path)?; let img =
   thumb_orient::orient_for_display(img, &bytes); <existing thumbnail(edge,edge) + PNG encode>`. Keep the
   existing signature + PNG output + downscale-preserve-aspect behavior; the existing `thumbnail` tests must
   still pass.

## ⚠ Notes
`image::Limits` on the raster path (bomb guard). Ext lowercased; no `std::path` platform semantics beyond
reading the extension. No `#[cfg]`. **Assert dims/aspect, never exact PNG byte lengths** (encoder variance).

## Tests (`#[cfg(test)] mod tests`)
- Hand-build a **minimal raw-compressed PSD fixture** (8BPS signature + header + merged image data — the
  "build-the-bytes-with-own-checksum" discipline used for CPE-1084/1083's minimal PNG) and assert
  `decode_thumb_image` returns the expected dims. **If a minimal valid PSD proves impractical, fall back** to
  asserting the `.psd` dispatch branch (psd-decoder error path vs image-decoder error path) + the orientation
  integration, and note a follow-up for a real PSD fixture — don't burn the whole ticket on the fixture.
- A normal PNG still decodes; a 100k×100k IHDR decompression-bomb PNG (reuse the CPE-1083/1084 fixture style)
  is REJECTED by the limits guard.
- **Integration**: `make_thumbnail_png` on a PSD returns a valid downscaled PNG; on a wide orientation=6 JPEG
  returns a **portrait** thumbnail (proves orientation is baked end-to-end); the existing downscale/aspect
  test still passes.

## Acceptance Criteria
- [ ] PSD decodes to the right dims (or, fallback: the `.psd` dispatch branch is exercised + a follow-up filed).
- [ ] Decompression-bomb PNG → `Err` via `image::Limits` (no OOM/panic); normal PNG still works.
- [ ] `make_thumbnail_png` produces a portrait thumbnail for a wide orientation=6 JPEG (end-to-end); existing
      thumbnail tests still pass.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean default AND
      `--features index`; no new deps.

## Work Log
2026-07-26 (sprint) — Filed by the Product Manager as the CPE-718 source-decode + integration slice. Held
in Backlog: depends on CPE-1085 (`thumb_orient`) landing first. The minimal-PSD fixture is the riskiest bit —
fallback path documented above.

2026-07-26 (sprint, Worker) — Built end-to-end:
- New `crates/server/src/thumb_source.rs`: `decode_thumb_image(path) -> Result<(DynamicImage, Vec<u8>), String>`.
  Reads bytes once; `.psd` (lowercased ext via `crate::model::extension_of`) dispatches to
  `psd::Psd::from_bytes` → RGBA composite → `image::RgbaImage::from_raw` → `DynamicImage::ImageRgba8`
  (mirrors `image_preview.rs`); everything else decodes via `image::ImageReader` with a local
  `bounded_limits()` copy of `batch_transform::bounded_limits` (`max_image_width`/`max_image_height` =
  20,000, `max_alloc` = 256 MiB) — same values, same "documented local copy" precedent `thumb_orient`
  established. Registered `pub mod thumb_source;` in `lib.rs` immediately after `pub mod thumb_cache;`.
- `thumbnail.rs::make_thumbnail_png` rewritten to `decode_thumb_image` + `thumb_orient::orient_for_display`
  + the existing `thumbnail(edge,edge)` downscale + PNG encode. Signature/behavior unchanged; the
  existing `make_thumbnail_png_downscales_and_preserves_aspect` test still passes untouched.
- **Minimal-PSD fixture: built successfully, no fallback needed.** Hand-built an uncompressed 8BPS PSD
  (26-byte header, three empty length-0 sections for color-mode-data/image-resources/layer-and-mask, then
  a raw-compression planar R/G/B image-data section) by reading the vendored `psd` crate's own section
  parsers (`sections/file_header_section.rs`, `sections/mod.rs`, `sections/image_data_section.rs`) to get
  the exact byte layout right (big-endian ints; each "empty" section is just its own zero `u32` length,
  which every section parser accepts). `decode_thumb_image` on this fixture returns the declared dims and
  the correct composited (opaque) pixel from the R/G/B planes.
- Tests added: `thumb_source.rs` (minimal-PSD dims + pixel check, corrupt-`.psd` vs corrupt-raster error
  paths are distinct, normal PNG decodes, 100k×100k IHDR decompression-bomb PNG → `Err`); `thumbnail.rs`
  (PSD → downscaled PNG via `make_thumbnail_png`, wide orientation=6 JPEG → portrait thumbnail end-to-end).
  Assertions are dims/aspect only, never exact PNG byte lengths (3-OS encoder variance per project
  convention).
- Verify: `cargo test` — 995 passed, 0 failed (from `crates/server`). `cargo clippy --all-targets -- -D
  warnings` clean; `cargo clippy --all-targets --features index -- -D warnings` clean. No `Cargo.toml`
  changes — confirmed no new dependencies (`git diff --stat` touches only the three `.rs` files).
- Assumption: none beyond the ticket's own design — no ambiguity hit during implementation.
- No blockers.
