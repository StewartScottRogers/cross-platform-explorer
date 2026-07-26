---
id: CPE-1083
title: "Batch media transform engine — cpe_server::batch_transform (apply_ops bytes→bytes)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-723
estimate: 3-4h
---

## Summary
Child of CPE-723 (Batch media operations). CPE-940 shipped the pure PLAN/validate (`batch_media.rs`); its
doc-comment says "the transform engine executes the returned plan" — **that engine does not exist yet**. Build
it: a pure bytes→bytes transform that applies an ordered list of `MediaOp`s to an image. Backend-only,
`cargo test` on the 3-OS matrix — no GUI, no user resource, **no new deps** (`image = 0.25` + `kamadak-exif`
already vendored).

## Design (buildable)
New module `crates/server/src/batch_transform.rs`, registered `pub mod batch_transform;` in
`crates/server/src/lib.rs` **immediately after `pub mod batch_media;`**. **REUSE `batch_media::MediaOp`** —
read `batch_media.rs` for its exact variants (Resize/Convert/Rotate/Flip/StripMetadata or similar) + field
names; do NOT redefine the enum.

```rust
/// Decode `input` once, fold the ordered `ops`, encode to the resulting format. Err (not panic) on any
/// decode/encode/unsupported failure.
pub fn apply_ops(input: &[u8], ops: &[batch_media::MediaOp]) -> Result<Vec<u8>, String>;
```
Implement every `MediaOp` variant via the vendored `image` crate:
- **Resize { max_px }**: downscale-only via `img.thumbnail(max_px, max_px)` (keeps aspect, NEVER upscales).
- **Convert { to_ext }**: map ext → `image::ImageFormat` over the 6 ENABLED encoders (png/jpeg/gif/webp/bmp/
  tiff); an unsupported ext (heic/avif/psd/svg) → `Err` (graceful, documented).
- **Rotate { degrees }**: `rotate90/180/270` (90/180/270 only; reject others via Err — plan already validates).
- **Flip { horizontal }**: `fliph` / `flipv`.
- **StripMetadata**: decode + re-encode SAME format (re-encoding drops EXIF/IPTC/XMP). Honest caveat: JPEG
  round-trips through recompression (note a lossless APP1-segment stripper as a FOLLOW-UP, don't build it).
  Include an EXIF-orientation normalize helper (read `kamadak-exif` Orientation, apply the matching rotate/
  flip) so a strip doesn't silently re-orient a phone photo.

## ⚠ Bounded allocation — MANDATORY (a crafted small-file/huge-canvas image can OOM)
Decode via `image::ImageReader` (or `Reader`) with `image::Limits` set to a sane max pixel + alloc budget, so
a decompression-bomb image returns `Err`, not an OOM. Add a test (or at least set + document the limit).

## ⚠ Cross-OS
Ext compares lowercased; pure bytes/`image` API — no `std::path`, no `#[cfg]` assertion. serde/specta derives
are FINE here (crates/server, not sidecar) — but this fn returns `Vec<u8>`, no new public struct likely needed.

## Acceptance Criteria
- [x] 100×40 PNG + Resize max_px=32 → decode output, `width==32` and height in [10,16] (aspect kept); a 20×20
      input is NOT upscaled by Resize max_px=64.
- [x] PNG + Convert{jpg} → `image::guess_format(&out) == Jpeg`; unsupported `to_ext` → `Err`.
- [x] Rotate{90} on a 10×4 → output decodes to 4×10 (dims swap); Flip{horizontal} moves an asymmetric pixel
      pattern (decode + assert the moved pixel).
- [x] StripMetadata drops EXIF (assert via `kamadak-exif` that input has an Orientation/APP1 and output does
      not); orientation=6 input → normalized output has the expected rotated dims.
- [x] A decompression-bomb (tiny file, huge declared dims) → `Err` via `image::Limits`, not OOM/panic.
- [x] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean default AND
      `--features index`; no new deps.

## Work Log
2026-07-26 (workshift) — Filed by the Product Manager as the CPE-723 transform engine (CPE-940 built only the
plan). Reuses `batch_media::MediaOp` + the already-vendored `image`/`kamadak-exif`. CPE-1084 (execute_plan
runner) depends on this. Kept as one cohesive module (all ops fold into one `apply_ops`) to avoid a shared-
function merge conflict.

2026-07-26 (workshift Worker, overnight) — Implemented `crates/server/src/batch_transform.rs::apply_ops`,
registered `pub mod batch_transform;` in `lib.rs` immediately after `batch_media`. Reused
`batch_media::MediaOp` as-is (Resize/Convert/Rotate/Flip/StripMetadata/Rename — did not touch the enum).
`Rename` is a no-op in this fn (it's a filename-only concern the planner already owns; this fn only ever
sees bytes). Decode-once via `ImageReader::with_guessed_format()` + explicit `image::Limits`
(`max_image_width`/`max_image_height` = 20 000px, `max_alloc` = 256 MiB) so a decompression-bomb PNG (huge
IHDR dims, no real pixel data) fails fast at header-parse time — covered by a dedicated test that hand-builds
a minimal IHDR chunk (own tiny CRC-32, no new dep). `Resize` guards `thumbnail()` behind an explicit
"only if actually larger" check, since `image`'s `thumbnail()` does **not** itself refuse to upscale (its
`resize_dimensions(..., fill=false)` will scale *up* to fit a larger box) — this is a correction to the
ticket's assumption that `thumbnail()` alone is upscale-safe; worth relaying upstream to whoever documents the
`image` crate usage next. `StripMetadata` reads the EXIF `Orientation` tag (`kamadak-exif`, same
`exif::Reader::read_from_container` pattern as `media_meta_read.rs`/`image_preview.rs`) and bakes the standard
8-value orientation transform into the pixels before re-encoding (values 5/7 via a rotate+flip compose), so
dropping the tag doesn't silently re-orient a phone photo. EXIF test fixtures are hand-built minimal
TIFF-in-APP1 blocks spliced after a JPEG's SOI marker (no camera file needed). All acceptance criteria above
verified via 11 new unit tests. Full crate suite: `cargo test` → 981 passed, 0 failed. Clippy
`--all-targets -- -D warnings` clean; `--all-targets --features index -- -D warnings` clean. No new
dependencies. Branch `cpe-1083-batch-transform`, PR opened for review/merge (ticket stays in Doing until
merged, per the normal PR-gated workflow).
