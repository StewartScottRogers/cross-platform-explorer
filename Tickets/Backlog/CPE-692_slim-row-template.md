---
id: CPE-692
title: Slim the file-list row template
type: enhancement
component: Frontend
priority: low
status: Open
tags: needs-prereq
created: 2026-07-18
epic: CPE-688
estimate: 1-2h
---

## Summary
Child of CPE-688. Reduce per-row handlers/bindings and hoist shared work out of the row template so the
~30–50 rows on screen (post-virtualization) are cheap. Prereq: CPE-690.

## Acceptance Criteria
- [ ] Fewer per-row handlers/bindings; no behaviour change; `npm run check` + suite green.

## Work Log
