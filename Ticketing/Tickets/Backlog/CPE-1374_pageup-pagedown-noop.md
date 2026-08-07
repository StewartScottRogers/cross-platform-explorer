---
id: CPE-1374
title: "PageUp / PageDown don't move the selection lead"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-059
created: 2026-08-06
---

## Problem (selection-model audit, Finding 4)

The key `switch` in `handleKeydown` (App.svelte) has Arrow/Home/End/Enter/Delete/etc. but NO PageUp/PageDown
cases — they fall through with no `preventDefault`, so the container may native-scroll while the lead/
selection stays put (and in a virtualized list the lead then isn't even in the DOM).

## Fix direction

Add PageUp/PageDown cases that move the lead by ~one viewport of rows (page = visible-row count, grid-aware),
with Shift extending the selection from the anchor, matching Arrow-key semantics. Add a gridnav/keyboard test.
