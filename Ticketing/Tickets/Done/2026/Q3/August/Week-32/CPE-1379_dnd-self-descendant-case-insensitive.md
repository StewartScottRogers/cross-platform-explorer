---
id: CPE-1379
title: "dnd self-descendant guard is case-sensitive — a folder-into-itself drop can slip through"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-661
created: 2026-08-06
---

## Problem (drag-drop audit)

`src/lib/dnd.ts` `norm()` (~L16-18) only normalises path separators, never case-folds. The
self-descendant guard uses a case-SENSITIVE `startsWith`, so dropping `C:\Foo` onto `C:\FOO\sub` (same
physical directory on case-insensitive Windows/macOS filesystems, different case) passes the guard —
allowing a folder to be dropped into its own subtree. No case-insensitivity test exists in `dnd.test.ts`.

## Fix direction

Case-fold (lowercase, or platform-conditional) both sides in `norm()` before the `startsWith`/equality
compare. Keep it a pure function. Add a `dnd.test.ts` case: `C:\Foo` onto `C:\FOO\sub` is rejected as a
self-descendant drop; `C:\Foo` onto `C:\Bar` still allowed. **Parallel-safe against the App.svelte pane-B
chain — but it edits `dnd.ts`, so it must land AFTER CPE-1372 (PR #656) merges (same file).**
