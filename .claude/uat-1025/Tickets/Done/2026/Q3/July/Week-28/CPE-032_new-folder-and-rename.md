---
id: CPE-032
title: New folder (Ctrl+Shift+N) and inline rename (F2)
type: Feature
status: Done
priority: High
component: Frontend
estimate: 2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Create folders and rename entries, with inline editing in the list like Explorer.

## Acceptance Criteria

- [x] Ctrl+Shift+N creates a folder, inline-editing its name immediately
- [x] F2 renames the selected entry inline
- [x] Enter commits, Escape cancels
- [x] Invalid/duplicate names show an error and keep the editor open
- [x] Listing refreshes and the new/renamed item stays selected
- [x] Empty or whitespace-only names are rejected

## Resolution

Ctrl+Shift+N creates "New folder" and immediately drops it into inline rename; F2 renames the
selection. Enter commits, Escape cancels, blur commits.

The editor pre-selects the **stem only**, not the extension — renaming `photo.png` shouldn't make it
one keystroke to destroy the `.png`. Duplicate and empty names are rejected by the backend and the
error surfaces in the status bar. Key events inside the editor stop propagating, so list shortcuts
(Delete, arrows) can never fire while you are typing a name.

## Work Log

2026-07-11 — Inline editor selects the stem, not the extension.
2026-07-11 — Editor keydown stops propagation so Delete/arrows can't fire mid-rename. Closing as Done.

## Notes
