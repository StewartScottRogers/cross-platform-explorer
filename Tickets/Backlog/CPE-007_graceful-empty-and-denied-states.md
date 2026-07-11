---
id: CPE-007
title: Show graceful empty and permission-denied states
type: Defect
status: Open
priority: Medium
component: Frontend
estimate: 30m
created: 2026-07-10
closed:
---

## Summary

An empty directory shows a blank pane with no explanation, and a permission-denied directory only
surfaces a raw error string. Give both clear, friendly empty states.

## Environment

- OS: Windows/macOS/Linux
- App version: 0.1.0

## Steps to Reproduce

1. Navigate into an empty folder — the pane is blank with no message.
2. Navigate into a folder the user cannot read — a raw error string appears in the status bar.

## Expected Behavior

Empty folders show an "This folder is empty" message; unreadable folders show a clear
"Can't open this folder — permission denied" state.

## Actual Behavior

Blank pane for empty folders; raw error text for denied folders.

## Acceptance Criteria

- [ ] Empty directory shows a centered "This folder is empty" message
- [ ] Permission-denied shows a friendly message, not a raw error
- [ ] The status bar item count stays accurate
- [ ] Navigation out of the failed folder still works

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

*(Agent appends dated entries here throughout — do not fill in)*

## Notes

Backend `list_dir` already skips unreadable entries; this is about presenting the top-level failure.
