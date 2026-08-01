---
id: CPE-1192
title: "Fix archive-browse gui-smoke spec: breadcrumb assertion misses the in-archive crumb"
type: bug
component: Testing
priority: low
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-705
---

## Summary
`gui-smoke/specs/archive-browse.smoke.ts` (CPE-1181) fails on the live build even though the FEATURE works:
its first live run captured `archive-browse-targz-fail.png` which clearly shows the `.tar.gz` **was** entered
(inner file `CPE-1181-note.txt` listed, breadcrumb visually ending on the archive). The assertion queried
`button.crumb` and got only the 8 filesystem crumbs — the **in-archive breadcrumb segment isn't a
`button.crumb`** (the `archiveCrumbs` render uses a different element/class), so the assertion wrongly fails.
gui-smoke is non-blocking, so this doesn't red main, but the pin is broken and doesn't actually guard the
feature.

## Build
- Inspect how the in-archive breadcrumb (`archiveCrumbs`, `src/App.svelte`) renders vs the filesystem
  breadcrumb, and update the spec's assertion to match the actual archive-crumb element (or assert on the
  entered-state a different robust way — e.g. the inner-entry row rendering, which already works). Keep it
  asserting real "we entered the archive" state.

## Acceptance Criteria
- [ ] The archive-browse spec passes on a live gui-smoke run (or is re-scoped to a robust assertion of the
      entered state); `cd gui-smoke && npm run typecheck` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift) after the epic-705 Visual-Critic capture: the browse feature works
  (screenshot-confirmed) but the spec's breadcrumb selector is wrong.
