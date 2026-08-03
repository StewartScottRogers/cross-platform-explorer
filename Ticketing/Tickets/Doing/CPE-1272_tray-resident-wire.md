---
id: CPE-1272
title: "Tray-resident: wire the system-tray icon + quick-access menu + minimize/close-to-tray"
type: feature
component: src-tauri
priority: medium
status: Doing
tags: ready
created: 2026-08-02
epic: CPE-713
---

## Summary
Epic CPE-713's model is built (`crates/server/src/tray_quick.rs` — `QuickAccess`: pinned + recent folder entries,
touch/pin/unpin/remove/items, CPE-946) but NO actual system-tray icon is wired. Build the Tauri v2 tray: an icon in
the system tray with a menu rendering `QuickAccess::items()` (one-click jump to pinned/recent folders) + show/hide
window + minimize/close-to-tray, so the app can live in the tray.

## Build (headless code; the tray icon appearing + OS behavior is attended)
- Tauri v2 `TrayIconBuilder`: add a tray icon (reuse the app icon — ensure any new icon resource is bundled so the
  CPE-1271 guard stays green) with a tooltip + a menu.
- Menu: quick-access section from `tray_quick::QuickAccess::items()` (persist the QuickAccess state to app data;
  update recents when folders are opened), plus Show/Hide window + Quit. Clicking a quick-access entry opens that
  folder in the app (reuse existing navigate/open + focus/show the window).
- Left-click tray icon → toggle window show/hide (or show+focus). Optional close-to-tray (window close hides to tray
  instead of quitting) behind a Settings toggle (default off to avoid surprising the user); minimize behavior as fits.
- Wire commands as needed (specta bindings if a new struct crosses the boundary — regen `bindings.gen.ts`).
- Capabilities: add any tray permission needed to `capabilities/default.json`.
- Persist QuickAccess (pins + recents) across restarts.
- Route through the async/spawn_blocking + invoke conventions.

## Acceptance criteria
- `cargo check`/`clippy --all-targets -D warnings` (both feature modes) clean; `npm run check` clean; bindings regen if changed (drift guard green); CPE-1271 bundle guard green (tray icon resource bundled if added).
- Unit tests for the QuickAccess integration / recents update where logic is testable headlessly.
- Tray behavior (icon shows, menu jumps to folders, show/hide, close-to-tray) is ATTENDED verify — skip-and-note.

## Notes
Part of item 3 (shell/OS). Sibling: CPE-712 shell-integration (install_shell_integration already exists) + CPE-716
drive-bay. Tray icon + OS behavior needs a build→install→run for the user to confirm.
