---
id: CPE-625
title: Route the "Copy to…" picker through the transfer engine
type: enhancement
component: Frontend
priority: low
status: Done
tags: ready
estimate: 30m
created: 2026-07-18
closed: 2026-07-18
epic: CPE-613
---

## Summary
Child of CPE-613. The "Copy to a folder…" action (`copyMoveToFolder`) used the synchronous
`copy_entries`; route its COPY through `start_transfer` so it shows the operations panel like Ctrl+V.
Move stays on the synchronous path (undo). Consistency, no new behaviour.

## Acceptance Criteria
- [x] `copyMoveToFolder` copy uses `startTransfer(..., "copy", "keepboth")`; the done listener refreshes + reports.
- [x] Move unchanged (undo preserved).
- [x] `npm run check` clean; suite green.

## Work Log
2026-07-18 (dayshift) — Routed the picker copy through the engine.
