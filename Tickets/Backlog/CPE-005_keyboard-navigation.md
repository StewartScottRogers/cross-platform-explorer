---
id: CPE-005
title: Add keyboard navigation to the file list
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 1h
created: 2026-07-10
closed:
---

## Summary

The listing is mouse-only. Add keyboard support so the file list can be driven without a mouse:
arrow keys to move a selection, Enter to open a folder, Backspace to go up.

## Acceptance Criteria

- [ ] Up/Down arrows move a visible selection highlight through the list
- [ ] Enter opens the selected folder (no-op on files, or opens file — align with CPE-006)
- [ ] Backspace navigates to the parent directory
- [ ] Selection is preserved/reset sensibly when the directory changes
- [ ] Focus and selection styles are visible and accessible

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

*(Agent appends dated entries here throughout — do not fill in)*

## Notes

Pairs well with CPE-004 and CPE-006.
