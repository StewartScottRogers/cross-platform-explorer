---
id: CPE-1388
title: "Dual-pane: the Column Picker dialog always edits pane A's columns, even opened from pane B"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-07
---

## Problem (CPE-1382 follow-up)

CPE-1382 made the READ side of custom metadata columns per-pane (`activeMetaColumnsB`) and fixed pane B's
column-RESIZE to save under `paneBPath`. But the **Column Picker dialog** (which chooses *which* columns are
active) still always edits/saves pane A's column set regardless of which pane's `openColumnPicker` opened it.
So a user opening the picker from pane B would edit pane A's columns — a write-routing gap symmetric to the
resize bug already fixed.

## Fix direction

Route the Column Picker's open + save through the originating pane: capture `inPaneB` when `openColumnPicker`
fires from pane B, load the picker's initial state from `paneBPath`'s config, and save the chosen columns
back under `paneBPath` (updating `activeMetaColumnsB`), mirroring the CPE-1382 resize fix. Touches
`src/App.svelte` (openColumnPicker handler + the ColumnPickerDialog save path). Add a test: open the picker
from pane B, choose columns, assert they save under pane B's folder and pane A's set is untouched.
