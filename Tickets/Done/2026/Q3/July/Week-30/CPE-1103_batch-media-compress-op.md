---
id: CPE-1103
title: "Batch media: Compress/optimize op (re-encode at a target quality)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-723
---

## Summary
Fills a Definition-of-Done gap in epic CPE-723 (Batch media operations): the DoD names
"resize/convert/rotate/**compress**/watermark", but the shipped `MediaOp` enum has no compress/optimize op.
Add a **`Compress { quality: u8 }`** op that re-encodes an image at a target quality to shrink file size,
composing with the existing batch-media pipeline (planner + transform engine + dialog) exactly like the other
ops. Backend + a dialog control. (Watermark is the other DoD gap but needs a product decision on text-vs-image
source — filed separately / deferred.)

## Context (verified)
- `crates/server/src/batch_media.rs` — `enum MediaOp { Resize{max_px}, Convert{to_ext}, Rotate{degrees},
  Flip{horizontal}, Rename{template}, StripMetadata }` (~:13-26); `plan()` builds the per-file summary;
  `validate()` rejects bad params. Add a `Compress { quality: u8 }` variant.
- `crates/server/src/batch_transform.rs` — `apply_ops(input, ops) -> Result<Vec<u8>,String>` decodes once,
  folds ops, re-encodes via `ext_to_format` (png/jpg/jpeg/gif/webp/bmp/tif/tiff). Compress means re-encoding
  the (possibly lossy) format at the given quality — for JPEG/WebP use the encoder's quality param; for
  lossless formats (png/gif/bmp/tif) either no-op with a note or apply the format's optimization level.
- `src/lib/components/BatchMediaDialog.svelte` + `src/lib/batchMedia.ts` (`mediaOpLabel`, op builder) — add a
  Compress entry to the op dropdown with a quality field (1-100), and a pill label (`Compress q80`).

## Design (buildable)
1. **`MediaOp::Compress { quality: u8 }`** — add the variant (plain derives, serde tag `compress`,
   `rename_all="snake_case"` consistent with siblings). `validate()`: reject `quality == 0` or `> 100` with a
   clear message. `plan()` summary: `"compress q{quality}"`.
2. **`apply_ops`** — handle `Compress`: it affects the ENCODE step (quality), not the pixel data. Thread a
   per-run "encode quality" through the fold so a `Compress{quality}` sets the re-encode quality for
   lossy-capable targets (jpeg/webp); for formats without a quality knob, document it as a no-op (still
   succeeds, no error) or use the format's compression level where the `image` crate exposes one. Keep the
   existing decode-bomb / bounded-alloc guards. Deterministic + saturating; no panic on any `quality` 1-100.
3. **Frontend** — `mediaOpLabel` case for `compress` (`Compress q{quality}`); the op-builder dropdown gains
   "Compress" with a number input (1-100, default e.g. 80); wire into `opsToJob`. Reflowing pill as usual.
4. **Bindings** — regenerate `bindings.gen.ts` so `MediaOp` includes the `compress` variant; drift-guard passes.

## ⚠ Notes / guardrails
- No new deps (use the `image` crate's existing encoders). Saturating/validated `quality`. Non-destructive by
  default (the existing `BatchJob.non_destructive` path is unchanged). Order-independent w.r.t. other ops
  except that Convert changes the target format Compress then applies to — document that ordering.
- Add tests: `validate` rejects q0/q101; `apply_ops` with `Compress{quality:60}` on a JPEG produces a smaller
  (or equal) byte length than q100 and still decodes; a lossless-format compress doesn't error.

## Acceptance Criteria
- [ ] `MediaOp::Compress{quality}` plans + validates (q must be 1-100) + re-encodes lossy targets at that
      quality; lossless targets don't error; `cargo test -p cpe-server` green.
- [ ] Dialog offers a Compress op with a quality field; pill label `Compress q{n}`; bindings regenerated,
      drift-guard passes, `npm run check` clean.
- [ ] clippy clean (default + `--features index`); no new deps; no panic on any valid quality; existing
      batch-media tests green.

## Work Log
2026-07-26 (workshift) — Filed to fill the CPE-723 DoD compress gap (found by the PM epic-closure assessment).
Watermark (the other gap) deferred pending a product decision (text vs image-overlay source). After this +
watermark, CPE-723 is closeable.
