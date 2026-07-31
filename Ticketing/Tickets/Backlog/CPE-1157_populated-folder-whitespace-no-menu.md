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
- [x] Using the CPE-1155 non-grabbing mouse helper, a test reproduces the failure (real right-click on the
      blank area of a **populated** folder) and then passes once fixed: the empty-area menu opens.
      → `specs/populated-whitespace.smoke.ts`; failed before the fix, passes after.
- [x] Right-clicking white space in a populated folder reliably opens the empty-area menu (below the last
      row verified via the harness; the fix is view-agnostic — it's on the pane catch-all, so grid/gallery
      and right-of-short-names all benefit).
- [x] Empty-folder menu, on-item menu, CPE-1153 submenus, CPE-1154 native suppression all still work.
      → on-item + empty-folder cases assert green in the same spec; `preventDefault` still suppresses the
      native menu (only the redundant window-level suppressor is now bypassed for pane pixels, exactly as it
      already is for rows).
- [x] `npm run check` green (0/0); the regression test is driven by the faithful (non-grabbing) CDP harness,
      not a bare synthetic event. A fast jsdom guard was also added to `ExplorerPane.test.ts`.

## ROOT CAUSE (found 2026-07-31 with the CPE-1155 CDP harness — FIXED in this PR)
The source really did dispatch `contextEmpty` on every path (as the ticket suspected). The failure was a
**menu-open-then-instant-close race**, invisible to source review and to synthetic-event tests:

- A real right-click on blank pane pixels below a populated `.rows` hit `.filelist-pane`, whose CPE-1154
  catch-all `paneContext` ran (`defaultPrevented` was `true` at the window bubble, proving it fired),
  dispatched `contextEmpty`, and the empty-area `.ctx` **did** mount.
- **~5 ms later it was removed.** `paneContext` (unlike FileList's `rowContext`/`emptyContext`) did **not**
  `stopPropagation`, so the SAME `contextmenu` event kept bubbling to `window`, where
  `ContextMenu.svelte`'s own `<svelte:window on:contextmenu|preventDefault={() => dispatch("close")}>`
  (its click-outside/right-click-elsewhere dismisser) fired and closed the menu it had just opened.
- The on-ITEM menu survived only because `rowContext` stops propagation; the truly-empty folder *looked*
  fine only because a `waitForExist` poll could catch the 5 ms flash (a false pass — the flash was real).
- Harness evidence (`ctxLog` transitions): before fix `present:false → present:true@4042.8 →
  present:false@4048.3`; after fix `present:false → present:true` (stays), and the window-bubble
  `contextmenu` probe records nothing (propagation now stopped).

**Fix (1 line):** `ExplorerPane.svelte#paneContext` now calls `e.stopPropagation()` — matching
`rowContext`/`emptyContext`. `preventDefault` still kills the native WebView2 menu.

## Work Log
- 2026-07-31 (Worker, workshift): Reproduced + diagnosed with `specs/populated-whitespace.smoke.ts`
  (CDP right-click, non-grabbing). Applied the minimal fix and a `mouse.ts`-driven regression test that
  fails-before / passes-after, plus a jsdom propagation-contract guard in `ExplorerPane.test.ts`.
  Verified against a fresh CLI release build: 5/5 harness tests green, `npm run check` 0/0.
