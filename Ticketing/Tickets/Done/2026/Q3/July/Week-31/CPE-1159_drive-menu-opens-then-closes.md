---
id: CPE-1159
title: "Disk right-click menu opens then instantly closes (drive handlers miss stopPropagation)"
type: bug
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-31
closed: 2026-07-31
---

## Summary
User-found (2026-07-31): right-clicking a disk still shows **no** context menu after CPE-1158. Root cause is
the **same open-then-close race** CPE-1157 fixed for the pane — CPE-1158 shipped before that race was
understood, and its jsdom test only asserted the drive menu *mounts*, not that it *stays*.

## Root cause (confirmed by reading)
The drive-tile / drive-row `contextmenu` handlers dispatch `driveContext` and `preventDefault()` but do **not**
`stopPropagation()`, so the same `contextmenu` event bubbles to `window`, where `ContextMenu.svelte`'s
`<svelte:window on:contextmenu|preventDefault={close}>` dismisses the menu it just opened (~5 ms flash → "no
menu"). Both sites:
- `src/lib/components/HomeView.svelte:85-90` — drive tile handler (has `preventDefault`, no `stopPropagation`).
- `src/lib/components/Sidebar.svelte:375-380` — drive row handler (same). Note its sibling `agentMenu`
  (line 202) uses `on:contextmenu|preventDefault|stopPropagation` — the drive one just omitted `stopPropagation`.

## Fix
- Add `e.stopPropagation()` to BOTH drive `contextmenu` handlers (HomeView tile + Sidebar row), right after
  `e.preventDefault()` — matching `rowContext`/`emptyContext`/`paneContext` (CPE-1157) and the neighbouring
  agentMenu handler.

## Acceptance Criteria
- [x] Right-clicking a drive (Home tile AND sidebar row) opens the drive menu and it **STAYS open** (no flash).
- [x] A **CDP-harness (CPE-1155 `mouse.ts`) regression test** right-clicks a drive tile and asserts the drive
      menu is present AND still present a tick later (the open-then-close signature that the jsdom mount-only
      test missed). This is the faithful test that would have caught it.
- [x] Blank Home background still opens no menu; empty-area / on-item / white-space menus all still work;
      native menu still suppressed. `npm run check` green.

## Work Log
- 2026-07-31 — Fixed both drive `contextmenu` handlers to `stopPropagation()` right after
  `preventDefault()`, mirroring the CPE-1157 pane fix and the neighbouring `agentMenu` handler:
  - `src/lib/components/HomeView.svelte` — drive-tile handler.
  - `src/lib/components/Sidebar.svelte` — drive-row handler.
  No other drive-menu code touched (App.onDriveContext, the ContextMenu drive branch, and the actions
  were already correct — the menu only self-closed).
- Added CDP-harness regression spec `gui-smoke/specs/drive-menu.smoke.ts` (uses `lib/mouse.ts`,
  CPE-1155): navigates to Home, does a faithful non-grabbing right-click on a drive TILE and a sidebar
  drive ROW, and asserts the drive menu is present AND still present after a 500 ms beat (stay-open
  contract), with a MutationObserver probe recording `.ctx` presence transitions. Also verifies it is
  the drive variant (Open in Terminal + Copy as path, no on-item quickrow, no Paste/Ctrl+V).
- **Falsifiability proven by rebuild-and-run both ways:**
  - WITHOUT the fix: both tests FAIL; probe shows the exact open-then-close signature
    `present:false → present:true → present:false` (menu flashed ~11-19 ms then self-closed).
  - WITH the fix: all 3 tests PASS; probe shows `present:false → present:true` and stays.
- Kept `DriveContextMenu.test.ts` (6 jsdom tests) intact and green.
- Verification: `npm run check` → 0 errors / 0 warnings; `vitest run DriveContextMenu.test.ts` → 6
  passing; `gui-smoke drive-menu.smoke.ts` → 3 passing against a real Tauri release build.

## Notes — systemic (see [[CPE-1160]])
This window-close race has now bitten THREE times (CPE-1154 native leak → CPE-1157 pane → CPE-1159 drive)
because EVERY menu-opening `contextmenu` handler must remember to `stopPropagation` or `ContextMenu`'s
window-level dismisser closes it. Filed **CPE-1160** to harden `ContextMenu.svelte` so the event that OPENED
the menu can never be the one that closes it (e.g. ignore a `contextmenu` in the same tick as open / that
targets inside the just-opened menu's origin) — making future menu-openers robust without the stopPropagation
footgun. This ticket is the immediate fix; CPE-1160 is the durable one.
