---
id: CPE-357
title: "Tab context menu: Duplicate / Close others / Close to the right"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

Right-clicking a tab should offer the standard tab-management actions: **Duplicate**,
**Close others**, **Close tabs to the right**. Complements Ctrl+Shift+T (CPE-356).

## Design (frontend)
- `tabs.ts`: pure `keepOnly(ids, id)` and `keepThroughRight(ids, id)` (which tab ids survive).
  Unit-tested.
- `TabBar.svelte`: right-click a tab → `menu` event `{ id, x, y }`.
- `TabMenu.svelte`: a small positioned menu; items gated (Close others only with >1 tab; Close
  to the right only when tabs exist after the target).
- `App.svelte`: `duplicateTab` (new tab at that tab's folder), and close-others / close-right
  via the pure helpers (recording closed folders into the CPE-356 stack).

## Acceptance
- Right-click a tab → the three actions; each does the right thing; the active tab stays valid.
- `npm run check` + `npm test` green.

## Work Log
2026-07-13 — Filed during Nightshift (loop 3). Extends tab management (CPE-356). Implementing.

2026-07-13 — Implemented on branch `CPE-357-tab-context-menu`.
- `tabs.ts`: pure `keepOnly` / `keepThroughRight` (+4 unit tests).
- `TabBar.svelte`: right-click a tab dispatches `menu {id,x,y}`.
- `TabMenu.svelte`: positioned menu; Close others gated on >1 tab, Close-to-right on tabs
  existing after the target; Esc/click-away close.
- `App.svelte`: `tabMenu` state + `onTabMenuAction` (duplicate / close-others / close-right),
  recording closed folders into the Ctrl+Shift+T stack and keeping the active tab valid.
- `npm run check` 0 errors; suite 311 pass; `npm run build` ok. Menu interaction is a GUI
  eyeball; the keep-logic + gating are unit-tested.
