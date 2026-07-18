---
id: CPE-607
title: Integration tests for the command palette + find-by-name wiring
type: test
component: search
priority: low
status: Done
tags: ready
estimate: 30m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
The palette (CPE-602/605) and find-by-name (CPE-603) had pure-helper unit tests but no App-level
test proving the keyboard wiring — the exact layer the live GUI check covered. Add integration tests
in `App.features.test.ts` so a regression in the shortcuts or dialog mounting fails CI.

## Acceptance Criteria
- [x] A test fires Ctrl+Shift+P, confirms the palette opens, and filters "terminal" → "Open terminal here".
- [x] A test fires Ctrl+P, confirms the Find-files dialog opens, and submitting a glob calls
      `find_files_by_name` with the current folder + query.
- [x] Frontend suite green.

## Resolution
Added two `describe` blocks to `src/App.features.test.ts` (driving the real App with the mocked
backend) and a `find_files_by_name` case to the shared mock. 14 App tests pass.

## Work Log
2026-07-17 (Nightshift Loop 6) — Added the wiring tests guarding this shift's flagship features.
