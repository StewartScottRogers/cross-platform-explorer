---
id: CPE-1221
title: "QA: gui-smoke render-pin + Visual Critic screenshot for NearDuplicatesDialog"
type: Task
status: Done
priority: Medium
component: gui-smoke
tags: [ready]
estimate: 45m
created: 2026-08-01
closed: 2026-08-01
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

## Work Log
- 2026-08-01 — Added `gui-smoke/specs/near-duplicates.smoke.ts`, mirroring `similar-images.smoke.ts`
  (CPE-1203) closely: opens `NearDuplicatesDialog.svelte` (CPE-1204) via its real entry point — the
  Command Palette (Ctrl+Shift+P → "Find similar documents…", `tool.findSimilarDocuments` in
  `App.svelte`) — waits for `[aria-label="Find similar documents"]`, clicks
  `[data-testid="nd-scan-btn"]`, asserts one `[data-testid="nd-group"]` contains BOTH seeded
  near-identical docs (and only those two — no stray members), `snap("near-duplicates")`s the
  grouped-results state, and dismisses via `[data-testid="nd-close-btn"]` (the dialog is read-only,
  so — unlike the similar-images spec — there is no keeper-guard/Move-to-Bin assertion to mirror).
  `afterEach` calls `snapFailure(this.currentTest, "near-duplicates")` per CPE-1149.
- Added `seedNearDupDocsFixture(tmpDir)` to `gui-smoke/wdio.conf.ts#onPrepare`, seeding
  `CPE-1221-notes-a.md` / `-b.md` (near-identical) + `CPE-1221-unrelated.md`. Rather than inventing
  new prose, the fixture reuses the EXACT three paragraphs `crates/server/src/simhash.rs`'s own unit
  test (`near_duplicate_docs_groups_close_pairs_and_separates_far_ones`) measures: the near pair
  (one word changed + a sentence appended) at Hamming distance ≤8 and the unrelated paragraph at
  ≥12 — inside/outside `document_similarity::DEFAULT_MAX_DISTANCE` (8) by construction, not just
  plausible-looking text.
- Tidied the garbled `folder_similarity_scan` module doc comment in `crates/server/src/lib.rs`
  (cosmetic only — no code change): now reads "...clusters near-identical folders via
  [`folder_similarity`] — this is the adapter that [`folder_similarity`]'s own docs describe as the
  caller's job."
- Built the real app in this worktree (`npm run build && npm run tauri build -- --no-bundle`,
  release profile, `src-tauri/target/release/cross-platform-explorer.exe`) and ran the spec against
  it: `npx wdio run ./wdio.conf.ts --spec near-duplicates` → **1 passing (7.4s)**. Captured
  `gui-smoke/.screenshots/near-duplicates.png` (one group, two items: `CPE-1221-notes-a.md` /
  `-b.md`); no `-fail.png` produced.
- Verification: `gui-smoke` typecheck (`tsc --noEmit`) clean; `gui-smoke` `npm run test:unit` — 21/21
  passing; `cargo clippy --all-targets -- -D warnings` for `cpe-server` (its own standalone
  workspace at `crates/server/`, separate `Cargo.lock` from `src-tauri/`) — clean. No product/src
  change beyond the `lib.rs` doc-comment tidy; `NearDuplicatesDialog.svelte` itself is untouched.
