---
id: CPE-1489
title: "EPIC: Drop Stack — cross-navigation multi-source file basket (a GUI-only superpower)"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (sprint PM, competitive-landscape pass — GUI survey).** Dormant brief — decompose on
> `/ticketing-epic activate CPE-1489`.

## Why (the one genuinely NOVEL, unclaimed feature the whole survey surfaced)
Path Finder's signature **Drop Stack**: as you browse *different* folders, drop files onto a persistent shelf
one-by-one, then move/copy the whole collected set in a single action. Ordinary copy/cut holds only ONE
selection from ONE location at a time; the Drop Stack accumulates across many navigations. **No existing CPE
epic covers it, and no TUI can do it well** — it leans on GUI persistence + drag-and-drop, so it's a clean
"GUI beats TUI" capability. Most of the plumbing already exists (selection engine CPE-711, transfer queue
CPE-613).

## Goal
A persistent, optional **Drop Stack** panel/tray: add selected files/folders from any location (drag onto it,
or a "Add to Drop Stack" action/hotkey), see the accumulated set with source paths, remove items, then
**Move all** / **Copy all** to the current folder via the existing transfer queue. Survives navigation;
clearable.

## Rough slices (JIT)
- A Drop Stack store (list of {path, addedFrom}) — client-side; persists across navigation, optionally across
  restarts.
- A dockable panel/tray showing the stack (reflow pills per [[tick-tacks-reflow]]) with per-item remove +
  clear-all.
- "Add to Drop Stack" in the context menu (MENUS standard) + a hotkey (via CPE-1484 keymap) + drag-onto-tray.
- Move-all / Copy-all into the active folder through the CPE-613 transfer queue (progress/undo for free).
- Docs page per CPE-579.

## Notes
Mostly frontend (reuses the transfer queue for the actual ops). Opt-in panel, zero cost when unused — fits
fast/small/predictable. The standout differentiator to lead the "best-explorer-ever" push with.
