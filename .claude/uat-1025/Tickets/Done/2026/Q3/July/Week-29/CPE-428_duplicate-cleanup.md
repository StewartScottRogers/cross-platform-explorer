---
id: CPE-428
title: "Duplicate finder: safe cleanup (move copies to Recycle Bin)"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 1-2h
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Complete the duplicate finder (CPE-420/421) with cleanup: tick redundant copies and move them to the
Recycle Bin (recoverable). Guard rails keep it safe - at least one copy of every set is always kept,
and "Select redundant" ticks all-but-the-first as a safe default. Nightshift research loop 17.

## Acceptance Criteria
- [x] Pure guards unit-tested: `redundantPaths` (all but first), `keepsOnePerGroup` (blocks wiping a
      whole set), `pruneGroups` (drop trashed paths + sets that fall below 2).
- [x] Dialog: per-copy checkbox, "Select redundant", and a "Move N to Recycle Bin" (delete_to_trash)
      disabled unless >0 selected AND every set retains a copy; on success the sets re-prune.
- [x] Recoverable (Recycle Bin, not permanent). Component test (select-redundant -> trash -> prune).
- [x] npm run check clean; JS suite green.

## Work Log
2026-07-15 - Nightshift loop 17. src/lib/duplicates.ts (redundantPaths/keepsOnePerGroup/pruneGroups)
+ test; DuplicatesDialog cleanup toolbar + checkboxes wired to delete_to_trash with the keep-one
guard; component test. npm run check 0/0, npm test 423 passed. Destructive action is user-initiated +
Recycle-Bin (reversible); not auto-run.

## Resolution
Finishes the duplicate feature end-to-end (find -> review -> reclaim). Safety is enforced by pure,
tested guards; deletions are recoverable via the Recycle Bin. A "delete permanently" option was
deliberately NOT added.
