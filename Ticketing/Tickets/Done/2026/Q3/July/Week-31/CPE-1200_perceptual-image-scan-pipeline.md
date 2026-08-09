---
id: CPE-1200
title: "Perceptual image-scan pipeline (streaming near-duplicate image grouping)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-08-01
epic: CPE-997
---

## Summary
Part of CPE-997. The near-duplicate image cores are built but unwired: `perceptual::phash` (dHash),
`hamming`, `cluster` (union-find single-link) in `crates/server/src/perceptual.rs`. Build the scan pipeline
that walks a folder, computes phash per image, clusters near-duplicates, and STREAMS groups (mirroring the
exact-duplicate `stream_duplicates`).

## Build
- New module `cpe_server::image_similarity` (+ `mod` line): a recursive walk mirroring
  `duplicates.rs::stream_duplicates` (skip dot/symlinked dirs, cap files, skip-on-error), filtered to
  decodable-image extensions; read bytes → `perceptual::phash` → collect `(path, u64)` → `perceptual::cluster(items, max_distance)`.
- Expose `stream_similar_images(root, flush)` + collect `find_similar_images(root)` → `SimGroup { paths }` +
  `files_scanned` + `truncated`. Pick + DOCUMENT a default `max_distance` (dHash ~8–12 bits/64; resolve the
  epic's threshold question, log the choice).
- Follow STREAMING liveness ([[prefer-streaming-liveness]]).

## Acceptance Criteria
- [ ] `cargo test -p cpe-server`: real image fixtures (reuse `perceptual.rs` helpers) — a re-encoded/resized
      near-duplicate groups with its original; a distinct image is a dropped singleton; walk skips non-images +
      unreadable entries; `truncated` set at the cap; deterministic order.
- [ ] `cargo clippy --all-targets -D warnings` clean (both feature modes).

## Work Log
- 2026-08-01 — Filed by Foreman (sprint, epic CPE-997). Foundation; built with CPE-1201 by one worker.
- 2026-08-01 — Built `crates/server/src/image_similarity.rs` (+ `mod` line in `crates/server/src/lib.rs`).
  Recursive walk mirrors `duplicates::stream_duplicates` exactly (skip dot/symlinked dirs, cap at
  `SIM_MAX_FILES = 50_000` → `truncated`, skip-on-error), filtered to a decodable-image extension set,
  read bytes → `perceptual::phash` → `perceptual::cluster(items, max_distance)`. Exposes
  `stream_similar_images(root, max_distance, flush)` + collect `find_similar_images(root)` →
  `SimGroup { paths }` + `SimResult { groups, files_scanned, truncated }`. Clustering is a whole-set
  operation (single-link needs every hash first), so the single result batch is flushed once after the
  walk — honours the streaming shape (UI flips `loading` off on the first/only batch) without a
  payload it can't produce incrementally. A private `stream_similar_images_capped(..max_files..)` makes
  the cap injectable so the `truncated` path is tested without writing 50k files.
- **CHOSEN `max_distance` = `DEFAULT_MAX_DISTANCE` = 10 bits / 64.** Justification: dHash near-duplicates
  (re-encode / resize / minor edit) cluster within ~0–8 Hamming bits (the `perceptual.rs` golden tests
  show resized copies within <10 bits); visually distinct images sit tens of bits apart. 10 is the
  midpoint of the epic's 8–12 bits/64 guidance — just above the near-dup band: high enough to absorb
  JPEG/resize bit-flips, low enough that unrelated images aren't pulled in. Conservative, favouring
  precision (few false groupings) over recall, since a wrongly-grouped pair is more jarring to a user
  than a missed near-duplicate. Documented in the module doc comment + on the const.
- Tests (5, all green in `cargo test -p cpe-server`, using `image`-crate PNG fixtures like `perceptual.rs`):
  resized/re-encoded gradient groups with its original + distinct bands image is a dropped singleton;
  walk skips non-image extensions, undecodable `.png`, and dot-dir contents; `truncated` set when a
  cap of 2 is hit; empty/single-image → no group + non-folder root is `Err`; output order deterministic
  across two scans.
