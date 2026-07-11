---
id: CPE-033
title: Delete to Recycle Bin (Del) with confirmation
type: Feature
status: Open
priority: High
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed:
---

## Summary

Del sends the selection to the Recycle Bin. Shift+Del deletes permanently — and must be explicitly
confirmed, because it is irreversible.

## Acceptance Criteria

- [ ] Del moves the selection to the Recycle Bin/Trash
- [ ] Shift+Del permanently deletes, behind an explicit confirmation dialog naming the count
- [ ] Failures (locked/in-use files) are reported per item, not swallowed
- [ ] The listing refreshes after the operation
- [ ] No delete path can run without the user having asked for it

## Resolution
## Work Log
## Notes
Recycle-bin-by-default is a safety decision, not a convenience one.
