---
id: CPE-050
title: New folder should auto-number instead of failing when "New folder" exists
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

`newFolder()` always asks the backend to create a folder literally named "New folder". The
`create_dir` command errors if that name already exists, so pressing New a second time in the same
directory surfaces a `"New folder" already exists` error notice instead of creating another folder.
Windows Explorer and macOS Finder instead auto-number: "New folder", "New folder (2)", "New folder
(3)", … . Match that.

Implemented purely on the frontend by computing a collision-free name from the current listing before
invoking `create_dir` (no backend change, so the existing server-side guard stays as a safety net).

## Acceptance Criteria

- [ ] With no "New folder" present, the created folder is named "New folder"
- [ ] When "New folder" exists, the next is "New folder (2)", then "New folder (3)", …
- [ ] Gaps are filled by the lowest free number (matches Explorer)
- [ ] Name matching is case-insensitive (Windows/macOS filesystems are)
- [ ] Logic lives in a pure, tested helper; `newFolder()` uses it
- [ ] Unit tests added; `npm run check` clean; full suite green

## Resolution

Added `uniqueName(base, existing)` in `src/lib/naming.ts` — returns the base if free, else the
lowest-numbered `base (n)` (Explorer-style), case-insensitive. `newFolder()` now computes the name
from the current listing before calling `create_dir`, so repeated New presses yield "New folder",
"New folder (2)", … instead of an error. Backend guard left intact as a safety net. 7 unit tests
added. `npm run check` 0 errors; suite 113 passed; `vite build` clean. Committed on branch, merged to
`main`, pushed.

## Work Log

2026-07-11 — Nightshift loop: researched new-folder/rename handling. Found create_dir rejects a duplicate name, so a second New folder fails with an error notice. Chose a frontend unique-name helper (no Rust rebuild; fully unit-testable) over changing the backend.
2026-07-11 — Implemented naming.ts + tests; wired newFolder(). check 0 errors, 113 tests pass, build clean. GUI verify (press New twice, see "New folder (2)") DEFERRED — user present, GUI paused per Nightshift rules.

## Notes

Same helper will suit paste-collision naming later ("file (2).txt"), though that is out of scope here.
