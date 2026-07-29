---
id: CPE-1132
title: "Recent" middle-pane view doesn't drive the right (preview/detail) pane like normal views
type: Defect
status: Open
priority: Medium
component: Frontend
estimate:
created: 2026-07-29
closed:
tags: [ready]
---

## Summary

In the main explorer, selecting an entry in the **middle pane** normally populates the **right pane**
(preview / details) for that entry. The **Recent** view is inconsistent: selecting an item in Recent does
**not** update the right pane the way every other middle-pane view (regular folders, other smart/saved
views) does. Recent should behave like all other middle-pane listings — clicking an item drives the right
pane to that item's preview/details.

## Environment

- OS: Windows 11
- App version: 0.57.38 (sidecar build)
- Area: main explorer — middle (list) pane ↔ right (preview/detail) pane wiring

## Steps to Reproduce

1. Open the explorer; select a normal folder → click an item in the middle pane → the right pane shows its
   preview/details. (Correct baseline.)
2. Switch the middle pane to the **Recent** view.
3. Click an item in the Recent list.

## Expected Behavior

The right pane updates to show the selected Recent item's preview/details — identical to selecting an item
in any other middle-pane view.

## Actual Behavior

The right pane does not update for Recent selections (it stays blank / unchanged), so Recent is the odd one
out among middle-pane views.

## Acceptance Criteria

- [ ] Selecting an item in the Recent view drives the right pane exactly like a normal folder view.
- [ ] Behaviour is consistent across the other special/smart views too (no regression).
- [ ] Covered by a test where practical (the selection→preview wiring is unit-testable in the pane logic).

## Notes

Likely in the explorer pane ↔ preview wiring (`src/lib/components/ExplorerPane.svelte` + the right/preview
pane component) and the special-view handling (Recent appears to be a smart/saved-search view — cf.
`App.svelte`'s "This is a smart folder — a saved search view" path). Probable cause: the Recent/smart-view
code path emits a different (or no) selection event, or passes a synthetic entry the right pane can't
resolve to a real path for preview. Investigate the selection event + the entry's path shape in the Recent
view vs. a normal listing. Filed at the user's request 2026-07-29.
