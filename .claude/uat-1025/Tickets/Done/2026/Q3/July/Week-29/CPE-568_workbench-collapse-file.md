---
id: CPE-568
title: "Workbench — collapse/expand a file's diff"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
epic: CPE-505
estimate: 30m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
On a large diff the Workbench is one long scroll. Let you **collapse a file** by clicking its header, so
you can fold away files you've reviewed and focus on the rest.

## Acceptance Criteria
- [x] Clicking a file header toggles its hunks collapsed/expanded, with a ▸/▾ chevron indicator.
- [x] The Edit button in the header still works (doesn't toggle collapse).
- [x] `npm run check` clean; a component test covers collapse + expand.

## Resolution
`WorkbenchView` tracks a `collapsed` `Set<string>` keyed by the file's `old→new` path; the file header is
now clickable (`toggleCollapse`, pointer cursor, hover) with a ▸/▾ `.chevron`, and the hunks render only
`{#if !isCollapsed}`. The Edit button uses `on:click|stopPropagation` so it doesn't fold the file. Added
`WorkbenchView.test.ts` (first component test for the view): loads a mocked diff, clicks the header →
hunks hidden, clicks again → shown. Full suite **604 pass / 63 files**; `npm run check` 0/0. Non-i18n.

## Notes
Third Workbench diff improvement (with CPE-566 line numbers + CPE-567 per-file stats); epic CPE-505.
