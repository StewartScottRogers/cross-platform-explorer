---
id: CPE-1370
title: "Dual-pane: keyboard nav + destructive keys act on the LEFT pane, ignoring the active pane"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (selection-model audit, Finding 2)

In dual-pane (commander) mode, `handleKeydown` (App.svelte) unconditionally reads/writes **pane A**'s
`selection`/`visible`/`selectedEntries` for ArrowUp/Down/Left/Right, Home/End, Ctrl+A, type-ahead, Delete,
F2, and Enter. `activePane` (App.svelte) is consulted only for Tab and the F5/F6/Ctrl+U commander ops — never
for navigation. Pane B has its own `selectionB`/`visibleB` and is only mouse-selectable.

Repro: enable dual-pane, click into the right pane, mouse-select files, press ↓ or Delete → the arrow moves
the LEFT pane's lead; Delete targets the LEFT pane's selection (deleting nothing visible, or the wrong
files). `currentGridCols()` also always queries the first `.rows.grid` (pane A).

## Fix direction

Route `handleKeydown` navigation/destructive keys through `activePane` — read/write pane B's
`selectionB`/`visibleB`/`selectedEntriesB` when `activePane === 1`. Factor the per-pane state so the key
handler picks the active pane's set. Add a dual-pane keyboard test.
