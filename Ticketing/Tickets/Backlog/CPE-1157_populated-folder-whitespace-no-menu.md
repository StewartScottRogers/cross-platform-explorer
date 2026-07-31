---
id: CPE-1157
title: "Populated folder: right-clicking white space doesn't open the empty-area menu"
type: bug
component: Frontend
priority: high
status: Backlog
tags: ready
created: 2026-07-30
---

## Summary
User-found (2026-07-30, after CPE-1154/1156). In a folder that **has one or more files**, right-clicking the
**white space** does not open the app's empty-area context menu (New ▸ / View ▸ / Sort by ▸ / Paste). Empty
folders work; populated ones don't. This is the SECOND time the populated case has been reported — CPE-1154
claimed to fix it but only proved it with a synthetic-event component test (fired directly on
`.filelist-pane`), which passed while the real click still fails.

## Why this is hard / why it needs CPE-1155 first
On paper every path already dispatches the empty menu:
- `FileList.svelte` `.rows` (populated) and `.empty-state` (empty) both have `on:contextmenu={emptyContext}`
  → dispatch `contextEmpty` (+ `stopPropagation`, CPE-1154).
- `ExplorerPane.svelte` `.filelist-pane` catch-all `paneContext` → dispatch `contextEmpty` (guarded
  `!inHome && !inReplay`).
- `App.svelte` `on:contextEmpty` → `ctx = {target:"empty"}` → ContextMenu empty branch.
So the failure is NOT visible in the source — it needs a **faithful real right-click** in a real populated
layout to pinpoint (which DOM element actually receives the event, whether a child intercepts, whether the
menu opens then closes, positioning, etc.). Synthetic `dispatchEvent` lies (that's how CPE-1154 slipped), and
a real OS-pointer click grabs the user's mouse.

**Depends on [[CPE-1155]]** (CDP non-grabbing mouse input): use it to reproduce the real right-click on
populated white space, capture the actual behaviour (which element, console, screenshot), then fix the real
cause and add a `mouse.ts`-driven regression test that would have caught it.

## Acceptance Criteria
- [ ] Using the CPE-1155 non-grabbing mouse helper, a test reproduces the failure (real right-click on the
      blank area of a **populated** folder) and then passes once fixed: the empty-area menu opens.
- [ ] Right-clicking white space in a populated folder (details AND grid/gallery views; below the last row;
      to the right of short names) reliably opens the empty-area menu.
- [ ] Empty-folder menu, on-item menu, CPE-1153 submenus, CPE-1154 native suppression all still work.
- [ ] `npm run check` green; the regression test is driven by the faithful (non-grabbing) harness, not a bare
      synthetic event.

## Notes
- Root-cause candidates to check with the real harness: `.rows` not filling the pane so clicks land on an
  un-handled child; a virtualization spacer / inner element swallowing the event; `.filelist-pane` not being
  the element under the cursor; or the menu opening then being dismissed by a follow-on handler.
