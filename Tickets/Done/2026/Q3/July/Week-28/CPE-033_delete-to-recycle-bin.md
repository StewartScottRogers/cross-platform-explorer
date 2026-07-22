---
id: CPE-033
title: Delete to Recycle Bin (Del) with confirmation
type: Feature
status: Done
priority: High
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Del sends the selection to the Recycle Bin. Shift+Del deletes permanently — and must be explicitly
confirmed, because it is irreversible.

## Acceptance Criteria

- [x] Del moves the selection to the Recycle Bin/Trash
- [x] Shift+Del permanently deletes, behind an explicit confirmation dialog naming the count
- [x] Failures (locked/in-use files) are reported per item, not swallowed
- [x] The listing refreshes after the operation
- [x] No delete path can run without the user having asked for it

## Resolution

Del sends the selection to the Recycle Bin — recoverable, so no modal; it just does it and reports.
Shift+Del deletes permanently and is gated behind an explicit confirmation naming exactly what will
be destroyed, because it is irreversible and does not go to the Recycle Bin.

Per-item failures (locked/in-use files) are named in the status bar rather than swallowed.

## Work Log

2026-07-11 — Recycle bin needs no modal (recoverable); permanent delete always does. Closing as Done.

## Notes
Recycle-bin-by-default is a safety decision, not a convenience one.
