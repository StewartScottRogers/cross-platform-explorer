---
id: CPE-1205
title: "Near-dup: exclude featureless/near-uniform images from grouping (fixes solid-colour over-grouping)"
type: bug
component: Backend
priority: high
status: Doing
tags: ready
created: 2026-08-01
epic: CPE-997
---

## Summary
Caught by the live Visual-Critic capture of the similar-images dialog: a **red solid square, a blue solid
square, and two gradients** were all grouped as "similar images." Root cause: dHash (`perceptual::phash`)
reduces featureless / near-uniform / monotonic images to a near-degenerate hash (popcount near 0 or 64), so
low-entropy images cluster with each other regardless of colour/content — a visibly wrong result (red ≠ blue).
The core near-dup detection is correct for structured photos (backend UAT proved resize/re-encode pairs group,
distinct images don't); this is specifically the low-entropy edge.

## Build
- In `crates/server/src/image_similarity.rs`, before clustering, **exclude images whose phash is
  featureless** — a principled guard, e.g. skip an image whose phash popcount is `< LOW` or `> 64-LOW`
  (near-all-zeros/near-all-ones = no discriminative structure; a balanced photo hash sits near 32). Pick +
  document `LOW` (e.g. 6). Featureless images can only form false near-dup groups; byte-identical ones are
  still caught by the separate exact-duplicate feature. Keep genuine structured near-dups grouping.
- **Fix the gui-smoke fixture (CPE-1203):** `gui-smoke/specs/similar-images.smoke.ts` + `wdio.conf.ts` seeder
  must seed a **structured** image + a resized/re-encoded near-dup of it (high-entropy, survives the guard),
  so the scan yields exactly ONE group of the TWO seeded images — not the low-entropy solids from other
  fixtures in the shared tmpDir.

## Acceptance Criteria
- [ ] `cargo test -p cpe-server`: two solid-colour (e.g. red + blue) images do NOT group; two structured
      near-dups (resize/re-encode) still group; a monotonic gradient is excluded. clippy clean both modes.
- [ ] The similar-images gui-smoke spec passes on the real build (exactly the 2 seeded structured near-dups in
      one group); `npm run check` + gui-smoke typecheck green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift) from the epic-997 Visual-Critic capture (red-square ~ blue-square
  over-grouping). Fixes both the detection edge and the gui-smoke fixture.
