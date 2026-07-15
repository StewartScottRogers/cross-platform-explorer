---
id: CPE-424
title: "Batch rename: Add text (prefix/suffix) mode"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 1h
created: 2026-07-15
closed: 2026-07-15
---

## Summary
The batch-rename dialog only did find/replace. Add an "Add text" mode: prepend a prefix and/or append
a suffix, with the suffix landing before the extension (report.pdf + "-v2" -> report-v2.pdf). A mode
selector toggles between "Find & replace" and "Add text"; the live preview + conflict flagging are
shared. Nightshift research loop 13.

## Acceptance Criteria
- [x] `planAffix(names, prefix, suffix)` + `splitExt` are pure and unit-tested (extension preserved,
      dotfiles have no extension, no-op when both empty, conflict flagging shared via markConflicts).
- [x] Dialog gains a mode selector; "Add text" shows Prefix/Suffix inputs and drives the same preview.
- [x] npm run check clean; JS suite green.

## Work Log
2026-07-15 - Nightshift loop 13. Refactored batchRename.ts (extracted markConflicts + splitExt, added
planAffix); BatchRenameDialog gained a mode selector + affix fields switching the reactive plan.
Tests: 3 new (splitExt, planAffix positions, no-collision). npm run check 0/0, npm test 414 passed.

## Resolution
Pure-logic enhancement to a tested tool. Suffix-before-extension is the standard, least-surprising
behaviour; case-transform / sequential numbering are possible future modes on the same selector.
