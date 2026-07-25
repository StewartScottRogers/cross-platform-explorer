---
id: CPE-569
title: "Workbench — Collapse all / Expand all"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
epic: CPE-505
estimate: 15m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Companion to CPE-568: on a multi-file diff, add **Collapse all** / **Expand all** toolbar buttons so you
can fold the whole diff at once instead of clicking each file header.

## Acceptance Criteria
- [x] "Collapse all" / "Expand all" buttons appear in the Workbench toolbar when more than one file changed.
- [x] They fold/unfold every file's hunks.
- [x] `npm run check` clean; a component test covers both.

## Resolution
`WorkbenchView`: `collapseAll()` seeds the `collapsed` set with every file key; `expandAll()` clears it.
Two `wb-btn`s render before Refresh when `files.length > 1`. `WorkbenchView.test.ts` +1 (2-file diff →
Collapse all hides both → Expand all shows both). Full suite **605 pass / 63 files**; `npm run check` 0/0.
Non-i18n. Epic CPE-505.
