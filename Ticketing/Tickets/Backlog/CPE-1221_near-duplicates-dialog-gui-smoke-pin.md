---
id: CPE-1221
title: "QA: gui-smoke render-pin + Visual Critic screenshot for NearDuplicatesDialog"
type: Task
status: Open
priority: Medium
component: gui-smoke
tags: [ready]
estimate: 45m
created: 2026-08-01
closed:
---

## Context
CPE-1204 (PR #519) shipped `NearDuplicatesDialog.svelte` — a new user-facing GUI surface (find
similar documents / near-identical folders). It is a read-only near-clone of the already-pinned,
already-Visual-Critic-validated `SimilarImagesDialog` (`similar-images.smoke.ts` / CPE-1203):
same dialog chrome, same grouped-results list styling, same theme vars + visible border. Its code
was Reviewer-checked for border/theme/reflow/read-only and UAT-checked for behavior, but it does
not yet have its OWN gui-smoke render-pin + captured screenshot for the Visual Critic.

This is a QA-Architect manual-verification-debt (MVD) row: pin the new surface so it can never
silently regress, and capture a real screenshot for the Visual Critic to judge.

## Acceptance criteria
- New `gui-smoke/specs/near-duplicates.smoke.ts` mirrors `similar-images.smoke.ts`: opens the dialog
  via the Command Palette ("Find similar documents…"), scans a seeded near-identical text fixture,
  asserts one `[data-testid="nd-group"]` contains both seeded docs, and `snap("near-duplicates")`s
  the grouped-results state. Add a `seedNearDupDocsFixture` in `wdio.conf.ts#onPrepare` (two
  near-identical .md/.txt + one unrelated), mirroring `seedSimilarImagesFixture`.
- Spec passes green against the real built app and captures `near-duplicates.png`.
- Visual Critic judges the screenshot (VISUAL PASS expected, given the shared validated pattern).

## Notes
Filed by the Foreman at epic boundary rather than blocking the CPE-1204 merge on a full
rebuild-and-capture chain, since the dialog reuses an already-visually-validated pattern. QA-Architect
burndown item.

## Also fix while here (CPE-1204 review nit)
`crates/server/src/lib.rs`'s module doc comment for `folder_similarity_scan` is grammatically
garbled ("The adapter [`folder_similarity`]'s own docs describe as the caller's job."). Cosmetic —
tidy it up when touching the near-dup area for the render-pin.
