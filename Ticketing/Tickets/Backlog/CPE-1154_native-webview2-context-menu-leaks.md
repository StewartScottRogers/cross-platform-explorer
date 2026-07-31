---
id: CPE-1154
title: "Right-click leaks the native WebView2 (Edge) menu instead of the app's on empty/unhandled areas"
type: bug
component: Frontend
priority: high
status: Backlog
tags: ready
created: 2026-07-30
---

## Summary
User-found (2026-07-30, GUI test). Right-clicking an **empty folder** (or any region not covered by a
specific handler) shows the **native WebView2 / Edge context menu** ("Back / Refresh / Save as / Print / More
tools / Send tab to your devices") instead of the app's own menu. Confirmed by screenshot.

## Root cause (confirmed)
There is **no global context-menu suppression** anywhere in the frontend — `grep` for a document/window-level
`contextmenu` listener / `oncontextmenu` across `src/**` and `index.html` returns nothing. The app relies
**purely on per-element** `on:contextmenu` handlers:
- `FileList.svelte`: `emptyContext` on `.rows` (populated list) and on `.empty-state` (empty folder), and
  `rowContext` on each file row.
So any right-click on a pixel NOT inside one of those elements — the pane padding, the area around the
centred `.empty-state` box, gaps, toolbar/breadcrumb, etc. — is never `preventDefault`ed, and WebView2 shows
its own browser menu. An **empty folder** is the easy repro: the `.empty-state` box is centred and does not
fill the pane, so right-clicking the surrounding empty space leaks the native menu.

NOT caused by CPE-1153 (the submenu work) — that menu renders correctly when you right-click a file row or the
populated list; this is a longstanding coverage gap made visible during testing.

## Fix
1. **Suppress the native menu app-wide.** Add a single window/document-level `contextmenu` listener (e.g. in
   `App.svelte` `onMount`, or `main.ts`) that calls `e.preventDefault()` so the WebView2/Edge browser menu can
   **never** appear anywhere in the app. This must NOT stop the app's own menus — the per-element handlers
   that open the custom `ContextMenu` still run and still fire (they already `preventDefault` + dispatch); the
   global one is just the catch-all that kills the native menu on otherwise-unhandled regions.
2. **Open the app's menu across the whole file pane, including empty folders.** Right-clicking anywhere in the
   file-list pane should open the app's empty-area menu — extend the `emptyContext` trigger to the pane/scroll
   container (and make `.empty-state` / `.rows` fill the available height) so an empty folder's blank space
   opens New ▸ / View ▸ / Sort by ▸ / Paste / etc., not nothing.
3. Decide sensible behaviour for genuinely non-file regions (toolbar/breadcrumb/sidebar): at minimum the
   native menu must be suppressed there; a custom menu is optional.

## Acceptance Criteria
- [ ] The native WebView2/Edge context menu ("Back/Refresh/Save as/Print/…") **never** appears anywhere in the
      app (verified by right-clicking: an empty folder's blank space, a populated list's padding, the toolbar,
      the sidebar).
- [ ] Right-clicking the file pane — **including an empty folder** — opens the app's empty-area context menu.
- [ ] Right-clicking a file row still opens the item menu; existing menu behaviour (CPE-1153 submenus etc.)
      unchanged.
- [ ] A test covers it: at minimum a gui-smoke spec that opens an **empty** folder, does a **real pointer**
      right-click (`button:2`) on the blank pane area, and asserts the app's `.ctx` appears (the prior driver
      check only used a synthetic event on the handled element, which hid this gap). `npm run check` green.

## Notes
- The global suppressor is the standard desktop-app pattern; without it any unhandled pixel leaks the browser
  menu. Keep it light-theme / cross-platform-agnostic.
- Related discovery path: reported right after CPE-1153 while testing on an empty freshly-created folder; also
  connects to the Home-screen right-click gap noted in CPE-1153.
