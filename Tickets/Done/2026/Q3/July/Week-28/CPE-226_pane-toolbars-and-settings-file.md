---
id: CPE-226
title: Per-pane + app toolbars with a Settings gear, backed by a single settings file
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 3-4h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Add a reusable toolbar strip to the application and to each of the three panes (Navigation, File
List, Preview). Each toolbar's first (v1: only) button is a Settings gear that opens a popover scoped
to that surface: the app gear edits general settings, each pane gear edits that pane's own settings,
independently. All settings persist to ONE `settings.json` on disk (app config dir), read/written via
new Rust backend commands, replacing the scattered `localStorage` keys. Existing prefs migrate into
the file on first run.

## Acceptance Criteria

- [x] A reusable `Toolbar` component renders a strip whose first button is a Settings gear
- [x] Toolbars appear in four places: app-level, Navigation pane, File List pane, Preview pane
- [x] Each gear opens a settings popover scoped to its surface (app / nav / list / preview), independent
- [x] Settings persist to a single `settings.json` in the app config dir via `read_settings` /
      `write_settings` backend commands (registered in `generate_handler!`)
- [x] Existing `localStorage` preferences are migrated into `settings.json` on first run (no data loss)
- [x] Pane popovers expose relevant prefs — nav: pane width; list: view, sort key, direction, hidden;
      preview: default tab (Preview/Details), pane width; app: show-details, hidden, reset-to-defaults.
      (Scoped v1 to prefs already wired end-to-end; the pins/recents visibility toggles were dropped —
      hiding those sections cleanly needs HomeView changes, deferred to keep v1 low-risk.)
- [x] Corrupt/absent settings file degrades to defaults, never crashes on launch
- [x] Unit tests; `npm run check` clean; JS suite green; Rust green (backend commands tested)

## Resolution

Added a reusable `Toolbar.svelte` (a thin strip whose first button is a Settings gear that toggles a
popover scoped to that surface) and placed it in four spots: an app-level toolbar above the TabBar,
and one at the top of each pane — Navigation, File List, Preview. Each gear's popover edits only its
surface's settings, independently.

Settings moved from ~10 scattered `localStorage` keys to a single on-disk `settings.json` in the app
config dir, via new `read_settings` / `write_settings` Rust commands (thin wrappers over testable
`read_settings_from` / `write_settings_to` helpers). `settings.ts` was reworked to hold one in-memory
document, loaded once at startup by `initSettings()` (bootstrapped in `main.ts` before the App
mounts), with saves debounced back to the file. On first run, legacy `localStorage` values are
migrated in via the pure `mergeLegacy(fileObj, get)` (file wins; missing keys backfilled). Absent or
corrupt files degrade to `{}`/defaults — launch never breaks.

**Per-pane popover contents (v1):** Navigation → pane width; File List → view, sort key, direction,
show-hidden; Preview → default tab (Preview/Details) + pane width; Application → show-details,
show-hidden, and Reset-all-to-defaults. All are prefs that were already wired end-to-end, so no new
state plumbing. The originally-sketched pins/recents visibility toggles were dropped from v1 (they'd
need HomeView changes to hide those sections cleanly) — noted in the AC.

**Files:** `src-tauri/src/lib.rs` (read_settings/write_settings + helpers, registered),
`src/lib/components/Toolbar.svelte` (new), `src/lib/settings.ts` (rework + migration),
`src/main.ts` (async settings bootstrap), `src/App.svelte` (four toolbars + popover controls +
`applySettings`/`resetAllSettings` + pane-col wrappers), `src/app.css` (toolbar/popover/pane-col
styles). Tests: `Toolbar.test.ts` (new), `settings.test.ts` (mergeLegacy), backend settings
round-trip/mkdir tests, and the resize integration test updated to assert the settings-file write.

**Verification:** `npm run check` clean; frontend suite green (231); Rust `cargo test` (56) + clippy
clean; production `vite build` succeeds.

## Work Log

2026-07-12 — Picked up. Estimate: 3-4h. Design confirmed with the user: fully separate per-pane
settings popovers; a real on-disk settings.json via a Rust backend; new toolbar in all four places;
v1 toolbar contents = the gear only. Plan: (1) backend read_settings/write_settings, (2) rework
settings.ts to a single object synced to the file with localStorage migration, (3) Toolbar +
SettingsPopover components, (4) wire into App + the three panes, (5) tests.

## Notes

Builds on the existing preference set in `src/lib/settings.ts` (currently ~10 separate localStorage
keys). PURPOSE.md tiebreaker still applies to the plain explorer — the toolbars must stay light.
2026-07-12 — Implemented: backend settings file + commands, settings.ts rework with localStorage
migration, reusable Toolbar, four placements with scoped popovers, async bootstrap. Fixed a
self-inflicted `*/`-in-JSDoc bug that briefly broke the build, and a stray `as` cast in markup.
Verified: npm run check clean, 231 JS tests, 56 Rust tests, clippy clean, vite build OK. Moved to Done.
