---
id: CPE-227
title: Add a menu bar with a File > Exit item that closes the app
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 30m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Add a classic desktop menu bar to the very top of the application window. The
first (and, for now, only) menu is **File**, containing a single **Exit** item
that closes the application. This gives users a conventional, discoverable way to
quit — matching the mental model of a native file explorer.

## Acceptance Criteria

- [ ] A horizontal menu bar sits above every other chrome (above the app toolbar).
- [ ] A **File** menu button opens a dropdown containing **Exit**.
- [ ] Choosing **Exit** quits the application.
- [ ] The menu closes on Escape, on click-away, and after choosing an item.
- [ ] No new plugin capability is required (process:default already grants exit).
- [ ] `npm run check` passes.
- [ ] Layout is correct — the file list area still fills the window (the `#app`
      grid `1fr` row lands on `main`, not on a chrome strip).

## Resolution

Added `src/lib/components/MenuBar.svelte` — a horizontal menu bar with a **File**
menu whose only item is **Exit**. It is a dumb component that dispatches an
`exit` event; `App.svelte` handles it by calling `exit(0)` from
`@tauri-apps/plugin-process` (already a dependency; `process:default` grants
`allow-exit`, so no capability change was needed). The dropdown closes on Escape,
on click-away, and after choosing the item.

Mounted the bar as the first child of `#app`, above the app-level Toolbar. Fixed
the `#app` grid in `app.css`: it previously declared 5 rows while the tree had 6
children (the app Toolbar added in CPE-226 shifted the `1fr` off `main` onto the
command bar). Rebuilt it as `auto auto auto auto auto 1fr auto` (7 rows) so the
`1fr` correctly lands on `main`.

Verified: `npm run check` → 0 errors / 0 warnings; `npm run build` compiles;
confirmed `exit` is exported by the installed `@tauri-apps/plugin-process`.

## Work Log

2026-07-12 — Confirmed process:default grants allow-exit — no capability change needed.
2026-07-12 — Added MenuBar.svelte (File > Exit); wired on:exit → exit(0) in App.svelte.
2026-07-12 — Found #app grid had 5 rows for 6 children; fixed to put 1fr on main.
2026-07-12 — check + build clean; verified exit() export. Closed.

## Notes

Closing uses `exit(0)` from `@tauri-apps/plugin-process`, already imported in
`App.svelte` for the updater relaunch. Component follows the ContextMenu /
Toolbar dropdown pattern. Relates to CPE-226 (app + per-pane toolbars), which
added the app-level Toolbar row.
