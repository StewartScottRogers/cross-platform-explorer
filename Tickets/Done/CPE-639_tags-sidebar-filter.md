---
id: CPE-639
title: Tags sidebar section + filter the view by a tag
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
Child of CPE-614. A "Tags" section in the sidebar lists every tag with its count; clicking a tag
filters the current folder to entries carrying it (click again or navigate to clear). Uses the pure
`tagCounts` + `filterEntriesByTag` helpers.

## Acceptance Criteria
- [x] Pure `tagCounts(store)` (most-used first); unit-tested.
- [x] Sidebar renders a collapsible Tags section (tag + count), highlights the active tag, dispatches `filterTag`.
- [x] App filters the `visible` pipeline by `selectedTag`; toggling the same tag or navigating clears it.
- [x] `npm run check` clean; full suite green (653).

## Work Log
2026-07-18 (dayshift) — Wired the sidebar Tags section + folder-scoped tag filter.
