---
id: CPE-675
title: Collapsible core sidebar sections
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-660
estimate: 3-4h
---

## Summary
The whole of CPE-660. Give the three core sidebar groups — pinned nav ("Explore"), quick-access
`places` ("Quick access"), and `drives` ("Drives") — a header row with a collapse twisty matching the
existing Favorites/Agents pattern, and persist every section's open/closed state via a new shared
`src/lib/sidebarSections.ts` store (localStorage). Convert the existing transient twisties (Favorites,
Agents, Tags, Smart) to the same store so all sections behave + persist identically. Default all-expanded.

## Acceptance Criteria
- [x] The three core groups each have a header + working collapse twisty; dividers kept.
- [x] `sidebarSections.ts` persists per-section open state (default open); pure reducer unit-tested.
- [x] Existing Favorites/Agents/Tags/Smart twisties use the same persisted store (consistent + persist).
- [x] Section labels added to all 12 locales; headers are keyboard-focusable with `aria-expanded`, themed.
- [x] Per-node drive/folder tree expansion still works independently; `npm run check` + suite green.

## Work Log
2026-07-18 (nightshift) — Picked up as the sole CPE-660 child. No questions; best-guess. Estimate 3-4h.

## Resolution
New `src/lib/sidebarSections.ts` — a localStorage-persisted `id→open` store (unset = open) with pure
`isOpen`/`toggled`/`parseSections` reducers (3 unit tests). Sidebar now derives every section's open state
from it (`$sidebarSections`) and toggles via `toggleSection(id)`. Added header rows with collapse twisties
for the three core groups — **Explore** (Home/Gallery/Repos/Board/Workbench), **Quick access** (`places`),
**Drives** (`drives`) — matching the `fav-head` pattern; the places/drives loop is split by the `isDrive`
flag with each item gated on its group's open state, dividers kept. Converted the existing Favorites/
Agents/Tags/Smart twisties to the same store, so all sections persist + behave identically. Headers are
`aria-expanded` + keyboard-focusable, themed from variables. Reused the pre-existing `sidebar.quickAccess`/
`sidebar.drives` i18n keys; added `sidebar.explore` ×12. Per-node tree expansion unchanged. check clean;
suite green (669); bundle clean. Live GUI collapse-toggle check recommended on /run.
Files: src/lib/sidebarSections.ts(+test), src/lib/components/Sidebar.svelte, src/lib/i18n.ts.
