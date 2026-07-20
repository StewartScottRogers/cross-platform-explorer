---
id: CPE-777
title: Pure recursive folder-tree diff
type: feature
status: Open
priority: medium
component: Frontend
tags: ready
created: 2026-07-20
closed:
epic: CPE-722
estimate: 1-2h
---

## Summary
Foundation for the compare studio (epic CPE-722). A pure module (`src/lib/treeDiff.ts`) that compares two
directory trees and classifies every node as added / removed / changed / identical — no DOM/IO, unit-tested
— so the folder-compare view (CPE-779) is a thin render over verified logic.

## Scope
- Input `CompareNode { name, isDir, size?, modified?, children? }` (the GUI builds these from backend
  listings). Matched by name within a level.
- `diffTrees(left: CompareNode[], right: CompareNode[]): DiffNode[]`:
  - only-left → `removed`; only-right → `added`.
  - both files → `changed` if size or modified differ, else `identical`.
  - both dirs → recurse; the dir is `changed` iff any descendant is not `identical`, else `identical`.
  - file-vs-dir type mismatch → `changed`.
- Deterministic order (dirs first, then name). Pure + total (empty sides, deep nesting).

## Acceptance Criteria
- [ ] Each case (added/removed/changed/identical, nested dirs, type mismatch) classifies correctly.
- [ ] A dir with any differing descendant is `changed`; a dir whose whole subtree matches is `identical`.
- [ ] Pure + dependency-free; unit tests cover the above incl. empty trees + nesting; check + suite green.

## Notes
Compares by name+size+mtime (no content hash) per the epic. Foundation for CPE-779. Headless.
