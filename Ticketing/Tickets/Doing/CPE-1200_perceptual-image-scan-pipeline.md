---
id: CPE-1200
title: "Perceptual image-scan pipeline (streaming near-duplicate image grouping)"
type: feature
component: Backend
priority: medium
status: Doing
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
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-997). Foundation; built with CPE-1201 by one worker.
