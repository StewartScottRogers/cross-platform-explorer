---
id: CPE-007
title: Show graceful empty and permission-denied states
type: Defect
status: Done
priority: Medium
component: Frontend
estimate: 30m
created: 2026-07-10
closed: 2026-07-10
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

- [x] Empty directory shows a centered "This folder is empty" message
- [x] Permission-denied shows a friendly message, not a raw error
- [x] The status bar item count stays accurate
- [x] Navigation out of the failed folder still works

## Resolution

Reworked the listing pane in `src/App.svelte` to render explicit states instead of a blank pane:

- Added a `loading` flag and a `friendlyError()` mapper that translates raw backend errors
  (`os error 5` / `os error 13` / "denied") into "Can't open this folder — permission denied.",
  missing paths into "This folder no longer exists.", and anything else into "Can't open this folder."
- On failure, `load()` now clears the stale entries and still sets `currentPath` to the attempted
  path, so the ↑ (up) button can navigate back out of an unreadable folder.
- The listing now shows: a 🚫 permission/denied empty-state, a "Loading…" state, a 📂
  "This folder is empty" state, or the file list — chosen in that order.
- Added `.empty-state` styling in `src/app.css` (centered, dimmed, with an icon).

Verified with `npm run check` — 0 errors, 0 warnings.

Files changed: `src/App.svelte`, `src/app.css`.

## Work Log

2026-07-10 — Picked up. Estimate: 30m. Plan: add loading/empty/denied states in App.svelte, map raw backend errors to friendly messages, verify with npm run check.
2026-07-10 — Added `friendlyError()` mapper and a `loading` flag; `load()` now clears stale entries and keeps currentPath on error so up-navigation still works.
2026-07-10 — Replaced the listing markup with error / loading / empty / list branches; added `.empty-state` CSS.
2026-07-10 — Ran `npm run check`: 0 errors, 0 warnings. All acceptance criteria met. Closing as Done.

## Notes

Backend `list_dir` already skips unreadable entries; this is about presenting the top-level failure.
