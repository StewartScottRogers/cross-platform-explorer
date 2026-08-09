---
id: CPE-1203
title: "gui-smoke spec pinning the similar-images dialog"
type: chore
component: Testing
priority: low
status: Done
tags: ready
created: 2026-08-01
epic: CPE-997
---

## Summary
Part of CPE-997 (after CPE-1202). Pin the similar-images review dialog in gui-smoke.

## Build
- New `gui-smoke/specs/similar-images.smoke.ts` + a `wdio.conf.ts` fixture seeder writing two near-duplicate
  PNGs (one re-encoded/resized copy of the other), modelled on `specs/batch-media.smoke.ts`. Drive the real
  built app: open the dialog via its real opener, scan, assert one group with BOTH images renders, `snap("similar-images-dialog")` + `snapFailure` (CPE-1149).

## Acceptance Criteria
- [ ] Spec passes headless against the built app; assertion tied to the seeded fixture (both filenames in the
      one group). `cd gui-smoke && npm run typecheck` clean.

## Work Log
- 2026-08-01 — Filed by Foreman (sprint, epic CPE-997). Depends on CPE-1202; can fold into 1202's DoD.
- 2026-08-01 — Done (Worker, sprint). Added `gui-smoke/specs/similar-images.smoke.ts` + a
  `wdio.conf.ts#seedSimilarImagesFixture` seeder writing two near-duplicate gradient PNGs (`CPE-1203-scene-a
  .png` 240×160 and `scene-b.png` 120×80 — the same horizontal grayscale gradient at two sizes, a genuine
  dHash near-duplicate pair; a new `gradientPng()` helper built on the existing PNG chunk/CRC/deflate
  machinery). A GRADIENT (not a solid fill) is used deliberately: solids hash to all-zero and would wrongly
  cluster with the batch-media solid PNGs already in the tmpDir, whereas the gradients sit far in Hamming
  bits from them and form their own group of two. Spec drives the real built app: opens the dialog via its
  real opener (Command Palette → "Find similar images…"), scans, locates the `sim-group` containing scene-a
  and asserts it also contains scene-b with exactly two `sim-image` cards and two thumbnail `<img>`s, then
  asserts the SAFETY guard — "Move to Bin" disabled with nothing selected, enabled with one copy selected,
  disabled again once the second (whole-group) copy is selected. `snap("similar-images-dialog")` +
  `snapFailure` (CPE-1149). Non-destructive (never clicks Move to Bin; closes via the close button).
  `cd gui-smoke && npm run typecheck` clean. Live headless run is CI.
