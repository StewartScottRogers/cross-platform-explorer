---
id: CPE-426
title: "Batch rename: sequential Number mode"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 45m
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Third batch-rename mode (after Find & replace + Add text): sequential numbering. A name pattern with
a `#` run marks the number, zero-padded to the run width (photo-### start 1 -> photo-001.jpg); no `#`
appends the number; extension preserved. Completes the rename tool. Nightshift research loop 15.

## Acceptance Criteria
- [x] `planNumber(names, pattern, start)` pure + unit-tested (# padding, extension preserved, start
      offset, append-when-no-token, empty-pattern no-op, shared conflict flagging).
- [x] Dialog gains a "Number" mode with a pattern input + start number, driving the same preview.
- [x] npm run check clean; JS suite green.

## Work Log
2026-07-15 - Nightshift loop 15. planNumber in batchRename.ts (+2 tests); BatchRenameDialog third
mode. npm run check 0/0, npm test 417 passed.

## Resolution
Rounds out batch rename with the three most-wanted operations (replace/affix/number), all pure +
tested on the shared preview + conflict machinery.
