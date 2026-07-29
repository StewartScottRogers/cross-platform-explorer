---
id: CPE-035
title: Properties dialog (Alt+Enter)
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Show name, type, location, size, created/modified dates, and attributes for the selection. For a
multi-selection, show the aggregate count and total size.

## Acceptance Criteria

- [x] Alt+Enter (and the context menu) opens Properties for the selection
- [x] Single item: name, type, full path, size, created, modified, read-only/hidden flags
- [x] Multi-selection: item count and total size
- [x] Folder size is computed recursively, off the UI thread, and can be cancelled
- [x] Dialog closes on Escape

## Resolution

Alt+Enter (and the context menu) opens Properties. Single selection shows name, type, full path,
size in both human and exact bytes, created/modified, and read-only/hidden attributes. Multi-selection
shows folder count, file count, and total size.

Folder size is computed **recursively in the backend, after the dialog is already open**, showing
"Calculating…" meanwhile — so opening Properties on a huge tree never freezes the UI. The dialog sets
a cancelled flag on close so a slow calculation can't write into a dismissed dialog. Unreadable
subtrees are skipped rather than failing the whole total.

## Work Log

2026-07-11 — Folder sizing runs after the dialog opens, never blocking it; cancelled on close. Closing as Done.

## Notes
Recursive folder sizing must not freeze the UI on a large tree.
