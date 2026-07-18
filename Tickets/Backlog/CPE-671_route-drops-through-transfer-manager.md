---
id: CPE-671
title: Route drag copy/move through the transfer manager
type: enhancement
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-661
estimate: 2-3h
---

## Summary
Make drag-copy/move tracked operations by routing `dropInto` through the transfer manager (CPE-613,
`start_transfer`) instead of the direct `copy_entries`/`move_entries` invoke — so a big drag shows the
progress panel + conflict chooser. Must preserve the current undo push and tag-follow (retag) behaviour
on completion. Prereq: CPE-669.

## Acceptance Criteria
- [ ] Drag copy/move runs through the transfer queue with progress + batch conflict handling.
- [ ] Undo (move) and retag-on-move still work after a dragged transfer completes.
- [ ] Small/same-folder drops stay snappy (no visible regression); `npm run check` + suite green.

## Work Log
