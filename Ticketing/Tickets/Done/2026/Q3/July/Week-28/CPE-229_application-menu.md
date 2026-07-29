---
id: CPE-229
title: Application menu for app-wide actions (updates, settings, docs, about)
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Extend the menu bar (CPE-227) with a single **Application** menu that gathers
app-wide operations — the ones that aren't about the current folder:

- **Check for Updates…** — manual trigger of the existing updater plugin, with
  user feedback (up to date / downloading / will relaunch).
- **Settings…** — opens a dialog with the app-wide settings (show hidden files,
  show details/preview pane, reset all to defaults).
- **Documentation** — opens the project page in the default browser.
- **About** — a dialog with app name, version, and a documentation link.

Plugin downloading is deliberately out of scope for this ticket — the app has no
plugin architecture yet, so no plugin entry is shown (user decision, 2026-07-12).

## Acceptance Criteria

- [ ] Menu bar shows `File` and `Application`; the Application menu lists the four
      items above with a separator before Documentation/About.
- [ ] Check for Updates gives feedback in the status bar in all cases, and never
      crashes when the updater is unreachable/misconfigured (e.g. dev build).
- [ ] Settings… opens a modal mirroring the app-wide settings; changes persist.
- [ ] Documentation opens the project URL via the opener plugin.
- [ ] About shows the running version (read at runtime, not hard-coded).
- [ ] Menus close on Escape, click-away, and after choosing an item; hovering
      another top-level menu while one is open switches to it.
- [ ] `npm run check` passes; `npm run build` compiles.

## Resolution

Reworked `MenuBar.svelte` into a data-driven bar (a `menus` table) dispatching a
single `select` event with the chosen item id. Added the **Application** menu:
Check for Updates…, Settings…, (separator), Documentation, About. Hovering a
different top-level title while one is open now switches menus; Escape /
click-away / choosing still close.

Added two modal dialogs following the ConfirmDialog pattern:
- `AboutDialog.svelte` — app name, running version (passed in from
  `getVersion()`), description, and a Documentation link (delegated to App via an
  `openurl` event so URL-opening stays in one place).
- `SettingsDialog.svelte` — app-wide settings (show details/preview pane, show
  hidden files, reset all). A dumb view: values in via props, changes out via
  events; App applies + persists, so there is one source of truth (no drift with
  the existing app Toolbar gear).

`App.svelte`: `onMenuSelect` routes the ids; `manualUpdateCheck()` gives status
feedback in every branch (up to date / downloading / error) and cannot crash on a
dev/misconfigured updater; `openExternal()` opens URLs via the opener plugin;
`appVersion` is read once on mount (failure is non-fatal — version is cosmetic).

Capabilities: `opener:default` already grants `allow-open-url` (verified in the
crate's default.toml), so only `core:app:allow-version` was added for
`getVersion()`. Plugin management was intentionally omitted (no plugin
architecture exists yet).

Verified: `npm run check` → 0/0; `npm run build` compiles; `cargo build`
re-validated the capability set (both `core:app:allow-version` and
`window-state:default` accepted) and compiled clean.

## Work Log

2026-07-12 — User chose a single "Application" menu; items = updates/settings/docs/about; plugins left out entirely.
2026-07-12 — Made MenuBar data-driven (select event); added Application menu with separator + hover-to-switch.
2026-07-12 — Added AboutDialog + SettingsDialog (ConfirmDialog pattern); wired onMenuSelect, manualUpdateCheck, openExternal, getVersion in App.
2026-07-12 — Verified opener:default already grants allow-open-url; added only core:app:allow-version. check + both builds clean. Closed.

## Notes

Decisions (user, 2026-07-12): single "Application" menu (not Help+Tools split);
include Check for Updates / Settings / Documentation / About; leave plugins out
entirely for now. `opener:default` already grants `allow-open-url`; added
`core:app:allow-version` for getVersion(). Builds on CPE-227 (menu bar).
