---
id: CPE-679
title: Persist each pane's path/history across sessions
type: feature
component: Frontend
priority: low
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-617
estimate: 1-2h
---

## Summary
Child of CPE-617. Remember each pane's path (and history) across restarts, reusing the existing tab/settings
persistence, so a dual-pane layout comes back where the user left it. Prereq: CPE-677.

## Acceptance Criteria
- [ ] Each pane's path/history is restored on launch when dual-pane was last active.
- [ ] Reuses the existing persistence layer; `npm run check` + suite green.

## Work Log
