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
- [ ] 100×40 PNG + Resize max_px=32 → decode output, `width==32` and height in [10,16] (aspect kept); a 20×20
      input is NOT upscaled by Resize max_px=64.
- [ ] PNG + Convert{jpg} → `image::guess_format(&out) == Jpeg`; unsupported `to_ext` → `Err`.
- [ ] Rotate{90} on a 10×4 → output decodes to 4×10 (dims swap); Flip{horizontal} moves an asymmetric pixel
      pattern (decode + assert the moved pixel).
- [ ] StripMetadata drops EXIF (assert via `kamadak-exif` that input has an Orientation/APP1 and output does
      not); orientation=6 input → normalized output has the expected rotated dims.
- [ ] A decompression-bomb (tiny file, huge declared dims) → `Err` via `image::Limits`, not OOM/panic.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean default AND
      `--features index`; no new deps.

## Work Log
2026-07-26 (workshift) — Filed by the Product Manager as the CPE-723 transform engine (CPE-940 built only the
plan). Reuses `batch_media::MediaOp` + the already-vendored `image`/`kamadak-exif`. CPE-1084 (execute_plan
runner) depends on this. Kept as one cohesive module (all ops fold into one `apply_ops`) to avoid a shared-
function merge conflict.
