---
id: CPE-526
title: "Workbench — git diff model + Diff view"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-505
sprint: SPR-06
estimate: 3-4h
created: 2026-07-16
---

## Summary
Core of the integrated workbench ([[CPE-505]]): read an agent's changes as a **diff**. A pure
**unified-diff parser** (→ files → hunks → lines with add/del/context), a `workbench_diff` command that
runs `git diff` (working tree vs HEAD) for a repo, and a **Diff view** component. Diff source =
working-tree-vs-HEAD (activation decision).

## Acceptance Criteria
- [ ] A pure parser turns unified-diff text into files/hunks/typed lines; malformed input ⇒ empty, no panic.
- [ ] A `workbench_diff(root, path?)` command returns the parsed diff (git diff working tree vs HEAD).
- [ ] A Diff view renders per-file hunks with add/del/context styling; large/binary handled gracefully.
- [ ] Tests for the parser (multi-file, multi-hunk, add/del/context, rename/binary tolerance).

## Notes
Wave 1 of [[CPE-505]]. The most self-contained + testable workbench piece.
