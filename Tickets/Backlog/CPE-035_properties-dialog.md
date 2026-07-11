---
id: CPE-035
title: Properties dialog (Alt+Enter)
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed:
---

## Summary

Show name, type, location, size, created/modified dates, and attributes for the selection. For a
multi-selection, show the aggregate count and total size.

## Acceptance Criteria

- [ ] Alt+Enter (and the context menu) opens Properties for the selection
- [ ] Single item: name, type, full path, size, created, modified, read-only/hidden flags
- [ ] Multi-selection: item count and total size
- [ ] Folder size is computed recursively, off the UI thread, and can be cancelled
- [ ] Dialog closes on Escape

## Resolution
## Work Log
## Notes
Recursive folder sizing must not freeze the UI on a large tree.
