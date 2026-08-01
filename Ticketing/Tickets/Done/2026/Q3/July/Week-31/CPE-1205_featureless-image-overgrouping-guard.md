---
id: CPE-1205
title: "Near-dup: exclude featureless/near-uniform images from grouping (fixes solid-colour over-grouping)"
type: bug
component: Backend
priority: high
status: Done
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
- 2026-08-01 — Implemented (Worker). **Backend guard** in `crates/server/src/image_similarity.rs`: added
  `FEATURELESS_LOW = 6` + `is_featureless(hash)` (featureless ⇔ popcount outside `6..=58`, i.e. within 6 bits
  of all-zeros/all-ones), and `items.retain(!is_featureless)` right before `perceptual::cluster`. Excluded
  images are STILL counted in `files_scanned` (they were walked/read/hashed; only clustering participation is
  suppressed) — documented in code. **Chosen LOW = 6** justification: a balanced photo dHash sits near 32 set
  bits; solids and smooth left→right gradients collapse to popcount 0 (dHash only compares horizontal
  neighbours, so a monotonic image gives all-same-sign bits); the degenerate cases sit at 0–2 while every
  genuine structured image clears 6 comfortably, keeping the retained band wide (`6..=58`) so no real near-dup
  is ever dropped. **Key finding:** a horizontal gradient is itself featureless under dHash (popcount 0), so
  the old gradient-based test fixtures AND the gui-smoke fixture had to switch to a structured **multi-band**
  pattern (alternating black/white vertical bands, popcount ~32).
  - Backend tests (`cargo test -p cpe-server`, 1157+ green): new `featureless_images_are_excluded_and_do_not_group_by_colour`
    (red solid + blue solid + gradient + a banded near-dup pair → exactly ONE group of the 2 bands; red/blue/gradient
    never grouped; `files_scanned == 5`), new `is_featureless_flags_degenerate_hashes_only` (asserts gradient
    hashes all-zero, bands ~32 retained), and rewrote the 5 existing tests off gradients onto structured bands.
  - gui-smoke fixture (`gui-smoke/wdio.conf.ts#seedSimilarImagesFixture`): replaced `gradientPng` with
    `bandedPng` (9 alternating vertical bands), seeding the same pattern at 216×160 and 108×80 — a structured
    near-dup pair that survives the guard → exactly one group of the two seeded images. Spec
    (`similar-images.smoke.ts`) comments updated; assertions unchanged (still assert the 2 seeded files form the group).
  - Verify: `cargo test -p cpe-server` green; `cargo clippy --all-targets -- -D warnings` clean in BOTH feature
    modes (default and `--features specta,index`); `npm run check` 0 errors; `cd gui-smoke && npm run typecheck`
    clean. The live gui-smoke browser leg is NOT runnable in this worktree — the Foreman re-runs it on the real
    build after merge.
