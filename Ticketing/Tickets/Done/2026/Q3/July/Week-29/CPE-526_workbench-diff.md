---
id: CPE-526
title: "Workbench — git diff model + Diff view"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-505
sprint: SPR-06
closed: 2026-07-16
estimate: 3-4h
created: 2026-07-16
---

## Summary
Core of the integrated workbench ([[CPE-505]]): read an agent's changes as a **diff**. A pure
**unified-diff parser** (→ files → hunks → lines with add/del/context), a `workbench_diff` command that
runs `git diff` (working tree vs HEAD) for a repo, and a **Diff view** component. Diff source =
working-tree-vs-HEAD (activation decision).

## Acceptance Criteria
- [x] A pure parser turns unified-diff text into files/hunks/typed lines; malformed input ⇒ empty, no panic.
- [x] A `workbench_diff(root, path?)` command returns the parsed diff (git diff working tree vs HEAD).
- [x] A Diff view renders per-file hunks with add/del/context styling; large/binary handled gracefully.
- [x] Tests for the parser (multi-file, multi-hunk, add/del/context, rename/binary tolerance).

## Notes
Wave 1 of [[CPE-505]]. The most self-contained + testable workbench piece.

## Resolution
Added the diff core of the workbench.

- **`src/lib/diff.ts`** (new, pure, 6 tests): `parseDiff(text)` turns `git diff` into
  files → hunks → typed lines (add/del/context), cleaning `a/`/`b/` prefixes + `/dev/null`, flagging
  binary files, tolerant of malformed input (never throws); `diffStats` (added/removed/files) and
  `fileLabel` (new/deleted/renamed/modified).
- **`workbench_diff(root, path?)` command** (native): runs `git diff` (working tree vs HEAD) and returns
  the raw text; read-only.
- **`WorkbenchView.svelte`** (new): a bordered overlay that loads + parses the diff and renders each file
  with a header, `@@` hunk headers, and add/del/context line styling + a `+N −M · files` summary; empty /
  binary handled. Opened from a **"Workbench"** Sidebar entry on the current folder.

Diff = working-tree-vs-HEAD (activation decision). `npm run check` clean; app clippy clean; 540 frontend
tests pass (6 new diff-parser tests). First ticket of SPR-06.

## Work Log
2026-07-16 — Picked up (SPR-06). Built the unified-diff parser (+stats/label) with 6 tests, the
workbench_diff command, and WorkbenchView + a Sidebar entry. npm check + app clippy clean; 540 tests pass. All ACs met.
