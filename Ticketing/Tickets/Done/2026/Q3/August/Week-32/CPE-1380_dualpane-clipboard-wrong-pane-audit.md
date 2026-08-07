---
id: CPE-1380
title: "Dual-pane: audit clipboard ops (copy/cut/paste) for the CPE-1370 wrong-pane bug"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-06
---

## Problem (selection-model audit follow-up)

CPE-1370 routes keyboard nav + destructive keys through `activePane`, but the clipboard operations
`doCopy`/`doCut`/`doPaste` (`src/App.svelte` ~L2621–2867) may still unconditionally read pane A's
`selectedEntries`/`currentPath` — the same wrong-pane bug class. In dual-pane mode with pane B active,
Ctrl+C/Ctrl+X/Ctrl+V could copy/cut the wrong pane's selection or paste into the wrong folder.

## Fix direction

**Audit first** (may already be partially handled by CPE-1370's scope — verify before duplicating work).
Where `doCopy`/`doCut`/`doPaste`/`copyMoveToFolder` read pane state unconditionally, route through
`activePane` so they act on the active pane's selection + target folder. Add a dual-pane clipboard test in
the same style as CPE-1370's keyboard test. **Different App.svelte functions than the pane-B `<ExplorerPane>`
block, so parallel-safe against CPE-1376/1377/1378 — but depends on CPE-1370 landing first.**
