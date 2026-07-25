---
id: CPE-427
title: "Batch rename: Change case mode"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
estimate: 30m
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Fourth batch-rename mode: Change case (lowercase / UPPERCASE / Title Case). Applies to the base name,
preserving the extension (README.TXT -> lower -> readme.TXT). Rounds out the rename tool's common
operations. Nightshift research loop 16.

## Acceptance Criteria
- [x] `planCase(names, mode)` pure + unit-tested (base-only transform, extension preserved, no-change
      when already matching, Title Case on separators).
- [x] Dialog gains a "Change case" mode with a lower/UPPER/Title select, driving the shared preview.
- [x] npm run check clean; JS suite green.

## Work Log
2026-07-15 - Nightshift loop 16. planCase + CaseMode in batchRename.ts (+2 tests); BatchRenameDialog
fourth mode (fixed the {#if} chain: number is now {:else if}, case is the final {:else}). npm run
check 0/0, npm test 419 passed.

## Resolution
Batch rename now offers the four most-wanted operations: find/replace, add-text, number, change-case
- all pure + tested on the shared preview + conflict machinery.
