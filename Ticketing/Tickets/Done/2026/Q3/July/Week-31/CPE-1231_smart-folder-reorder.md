---
id: CPE-1231
title: "Reorder smart folders in the sidebar"
type: Task
priority: Low
component: frontend
tags: [ready]
created: 2026-08-01
epic: CPE-978
closed:
---

## Context
Smart folders render in insertion order only; DoD calls for edit/reorder/remove (rename+remove exist,
reorder is missing). Add reorder for both the tag-only (`smartFolders.ts`) and structured
(CPE-1228 store) smart folders.

## Acceptance criteria
- A `moveSmartFolder`/reorder helper for each store (persisted), + a Sidebar reorder affordance
  (drag handle or up/down), consistent with other reorderable lists in the app.
- Order persists across sessions.
- REAL tests for the reorder helper(s).

## Notes
Can follow independently; touches Sidebar + the stores.
