---
id: CPE-1529
title: "Compact density: instant toggle control + docs"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1488
created: 2026-08-09
---
## Context
CPE-1526/1527/1528 build the setting and both consumers (file list, chrome) of density, but nothing lets
the user actually flip it yet. This ticket adds the user-facing control and its docs — the last slice
that makes the feature reachable and discoverable.

## Scope
- An **instant, inline** toggle for `comfortable` / `compact` — per [[prefer-inline-instant-controls]]:
  a small segmented control or two-state button changeable on a dime, **not** a modal/Browse-style
  dialog and not buried only in a Settings sub-page. Natural home is next to the existing view-mode
  control in `NavToolbar.svelte` (details/icons/gallery already lives there).
- Wire it to the `setDensity(d)` handler CPE-1526 added in `App.svelte` (prerequisite — reuse it, don't
  add a second write path to the setting).
- Update the in-app docs: `src/docs/03-explorer.md` (the existing "explorer" section's doc page) gets a
  short paragraph documenting the density toggle. **No new `Section` or `sectionDocs.ts` entry is
  needed** — density is a view option of the existing Explorer section, which is already mapped
  (`explorer: "03-explorer"` in `src/lib/sectionDocs.ts`); confirm this in the Work Log rather than
  silently skipping CPE-579's guard test. If in implementation it turns out a dedicated section is
  actually warranted, add the mapping and satisfy `src/lib/sectionDocs.test.ts` — don't skip it either
  way.

## How
- Icon-only or short-label button pair/segmented control, themed from variables, matching the existing
  view-mode control's interaction pattern in `NavToolbar.svelte` so it reads as "the same kind of
  control" rather than a bespoke widget.
- No new dependency.

## Verify
`npm run check` + `npx vitest run` covering `src/lib/components/NavToolbar.test.ts` — assert clicking the
toggle flips the `density` prop/emits the change event and (via a mocked `settings` module, matching how
other toolbar-triggered settings writes are already tested in that file) persists through
`saveDensity`. Fully headless. Also run `src/lib/sectionDocs.test.ts` to confirm the docs guard still
passes.

## Notes
**Conflict surface:** `src/lib/components/NavToolbar.svelte` (adds the toggle control itself),
`src/docs/03-explorer.md`. **Prereq: CPE-1526** (needs `setDensity`). **Sequence after CPE-1528**, not
parallel with it — both land changes in `NavToolbar.svelte` and CPE-1528 is the larger/foundational
chrome change; landing it first avoids CPE-1529 rebasing around chrome-density churn in the same file.
