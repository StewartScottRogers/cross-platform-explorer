---
id: CPE-006
title: Open files with the OS default application on double-click
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 1h
created: 2026-07-10
closed: 2026-07-11
---

## Summary

Double-clicking a file previously did nothing (only folders navigated). Use the `opener` plugin to
open files with their default OS application.

## Acceptance Criteria

- [x] Double-clicking a file opens it via the opener plugin
- [x] Double-clicking a folder still navigates into it (unchanged)
- [x] Errors (missing handler, permission) surface in the status bar rather than crashing
- [x] The opener permission is present in capabilities/default.json

## Resolution

`open()` in `src/App.svelte` now branches: directories still call `load()`, files are handed to
`openPath()` from `@tauri-apps/plugin-opener`.

**Important permission fix.** The final acceptance criterion assumed `opener:default` already
granted this — it does not. Per the Tauri docs, `opener:default` expands to only `allow-open-url`,
`allow-reveal-item-in-dir`, and `allow-default-urls`. The `open_path` command is deliberately
excluded because it is more sensitive. Without an explicit grant, `openPath()` would have been
**denied at runtime**, and neither `svelte-check` nor CI would have caught it — the failure only
appears when a user double-clicks a file in a real build. Added `opener:allow-open-path` to
`src-tauri/capabilities/default.json` (unscoped, which is correct here: a file explorer's purpose is
to open arbitrary user-chosen files).

Error handling: added a transient `notice` state, distinct from `error`. `error` blanks the listing
(right for an unreadable directory); a failed file-open should not — the listing is still valid. A
failed `openPath` shows "Can't open "<name>" — no app is associated with this file type." in the
status bar for 4 seconds, then clears.

Verified: `npm run check` -> 0 errors; `npm test` -> 10 passed.

Files changed: `src/App.svelte`, `src-tauri/capabilities/default.json`.

## Work Log

2026-07-10 — Picked up. Estimate: 1h. Plan: branch open() on is_dir, call openPath for files, surface failures in the status bar.
2026-07-10 — Confirmed plugin-opener exports openPath(path, openWith?) in the installed d.ts.
2026-07-10 — Added transient `notice` state so a failed file-open does not blank the listing the way `error` does.
2026-07-11 — Checked the opener permission table rather than trusting the ticket's assumption: `opener:default` does NOT include allow-open-path. openPath would have been denied at runtime with no compile-time signal. Added `opener:allow-open-path` to capabilities.
2026-07-11 — svelte-check 0 errors; vitest 10/10. All criteria met. Closing as Done.

## Notes

The permission gap is a good argument for the Rust-side integration test suggested in CPE-003's
notes — capability misconfiguration is invisible to the frontend toolchain.
