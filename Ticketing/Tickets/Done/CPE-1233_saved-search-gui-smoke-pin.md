---
id: CPE-1233
title: "QA: gui-smoke pin + Visual Critic screenshot for the Saved Searches flow (CPE-1229)"
type: Task
priority: Medium
component: gui-smoke
tags: [ready]
estimate: 1h
created: 2026-08-01
epic: CPE-978
closed: 2026-08-01
status: Done
---

## Context
CPE-1229 (PR #532) shipped the structured Saved Searches UI: a "Save search…" affordance in
SelectByDialog + palette, a new sidebar "Saved Searches" section, and an open-evaluator. It has
Reviewer + UAT sign-off but no gui-smoke screenshot for the Visual Critic yet (marquee epic-978 slice).

## Repro to drive (from the CPE-1229 worker's own notes)
Open a real folder → Ctrl+Shift+P → "Save search…" → pick a criterion (e.g. Extension = png) → type a
name → click Save search. The sidebar's "Saved Searches" section appears below Smart Folders; clicking
the entry shows the filtered recursive view.

## Acceptance criteria
- New `gui-smoke/specs/saved-search.smoke.ts` (mirror similar-images/near-duplicates specs) drives the
  real built app through the repro above against the seeded tmpDir (which already has CPE-1203 .png
  fixtures), asserts the "Saved Searches" sidebar section renders the saved entry, opens it, asserts a
  seeded .png shows and a non-matching file doesn't, and `snap("saved-search")`s the sidebar + result.
- Spec passes green + captures `saved-search.png`. Visual Critic judges the sidebar section + the
  SelectByDialog "Save search…" inline capture (section styling matches Smart Folders, menu conventions,
  dialog border, on-theme, no clipping).

## Notes
QA-Architect burndown item for the new surface; mirrors CPE-1221 (near-dup pin).

## Work Log
- 2026-08-01 — Added `gui-smoke/specs/saved-search.smoke.ts`, mirroring `near-duplicates.smoke.ts` /
  `similar-images.smoke.ts`'s structure: opens the Command Palette (Ctrl+Shift+P), scans `.cp-row`
  HTML for "Save search…" (`tool.saveSearch` in `App.svelte`, `enabled: inFolder`), clicks it to open
  `SelectByDialog.svelte` (`aria-label="Select by criteria"`) with `autoReveal` already showing the
  name field. Fills `[aria-label="Extensions"]` with "png" (default criterion kind is `ext`) and
  `[aria-label="Search name"]` with "CPE-1233 PNG search", clicks the real
  `[data-testid="save-search-confirm"]` button, and waits for the dialog to close.
- Asserts the Sidebar's "Saved Searches" section (`.fav-title` text, gated on
  `savedSearches.length > 0` in `Sidebar.svelte`) renders, then finds the new entry among
  `.nav-item.fav-item` buttons by scanning for the saved name in each button's HTML (no dedicated
  testid on that button — same HTML-scan approach as the dialog specs) and clicks it — dispatching
  `openSavedSearch` -> `openStructuredSearch` in `App.svelte`, which recursively `scanTree`s the
  seeded tmpDir and filters via `evaluateSavedSearch` (CPE-1229/CPE-986).
- Core assertion: scans `.row` (FileList row class, same selector `link-badge.smoke.ts` uses) for the
  seeded `CPE-1203-scene-a.png` (present) and `CPE-1143-archive.zip` (absent) — proving the
  ext=png filter actually narrowed the recursive listing rather than showing everything. No new
  fixture needed; reused the already-seeded CPE-1203/CPE-1144/CPE-1143 PNGs and the CPE-1143 `.zip`.
  `snap("saved-search")` captures the sidebar section + filtered result; `afterEach` calls
  `snapFailure(this.currentTest, "saved-search")` per CPE-1149.
- Built the real app in this worktree (`npm run build && npm run tauri build -- --no-bundle`,
  release profile, `src-tauri/target/release/cross-platform-explorer.exe`) and ran the spec against
  it: `npx wdio run ./wdio.conf.ts --spec saved-search` -> **1 passing (6.5s)**. Captured
  `gui-smoke/.screenshots/saved-search.png` (Saved Searches section with the new entry active,
  filtered file list showing exactly the 5 seeded PNGs); no `-fail.png` produced. Visually reviewed
  the screenshot: sidebar section renders correctly below Explore/Quick Access, active entry
  highlighted, breadcrumb reads "Home > CPE-1233 PNG search", status bar reads
  `Saved search "CPE-1233 PNG search"` — matches Smart Folders/menu conventions, on-theme, no
  clipping.
- Verification: `gui-smoke` typecheck (`tsc --noEmit -p tsconfig.json`) clean. No product/src change
  — `SelectByDialog.svelte` / `Sidebar.svelte` / `App.svelte` are untouched; this is gui-smoke-only.
