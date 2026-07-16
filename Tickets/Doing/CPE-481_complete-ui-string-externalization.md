---
id: CPE-481
title: "Complete UI string externalization — migrate remaining components to i18n"
type: Task
status: In Progress
priority: Low
component: Frontend
tags: [ready]
estimate: 3-4h
created: 2026-07-15
epic: CPE-261
---

## Summary
The i18n **system** landed in CPE-362 (`src/lib/i18n.ts`: store, 4-locale catalogs, reactive `t()`,
`Intl` date/number formatting, persistence, a live language switcher), wired into the primary chrome
(NavToolbar, MenuBar titles + language menu, Sidebar section headers, TabBar). This ticket is the
**mechanical tail**: migrate the remaining hardcoded strings across the rest of the components to
catalog keys, so the app is fully translatable.

## Acceptance Criteria
- [ ] Every user-facing string in the remaining components (dialogs: Properties, BatchRename,
      Duplicates, Update, Consent; FileList, PreviewPane, HomeView, CommandBar, ContextMenu,
      SidecarManager, App.svelte chrome) is a catalog key, not a literal.
- [ ] Each new key is added to all four locale catalogs (English authoritative; others may fall back).
- [ ] `npm run check` clean; existing tests pass; a lint/test guards against a stray hardcoded string
      in a migrated component.

## Notes
Split from CPE-362 (the i18n framework). Deliberately Low — a single-user local explorer — but now
that the framework exists, this is straightforward per-component work. The framework's fallback
(locale → English → key) means partial migration is always safe/legible in the meantime.

## Work Log
2026-07-16 (Nightshift loop 10) — Migrated the **MenuBar dropdown items** (File/Tools/Application:
Exit, Search in files, Find duplicate files, Copy file names, Copy file list, Save file list, Check
for Updates, Settings, Keyboard shortcuts, Documentation, About) to i18n. Added 11 `mi.*` keys to all
4 locales (en/es/de/fr) in `src/lib/i18n.ts`; `MenuBar.svelte` items now carry a `labelKey` rendered
via `$t()`. Test added (menu-item translations + all-locale coverage). `npm run check` clean; 468
frontend tests pass. Remaining to migrate (future loops): dialogs (Properties, BatchRename,
Duplicates, Update, Consent), ContextMenu, CommandBar, FileList, HomeView, App.svelte chrome.
2026-07-16 (Nightshift loop 15) — Migrated **ContextMenu.svelte** (the right-click menu) to i18n: 30
`ctx.*` keys added to all 4 locales (en/es/de/fr), every visible item label rendered via `$t()`
including interpolation (`ctx.selectAllExt` with `{ext}`) and the conditional pin/favorite toggles.
Test added (context-menu translations + all-locale coverage). `npm run check` clean; 469 frontend
tests pass. Remaining: the ContextMenu top-row icon-button tooltips (Cut/Copy/Rename/Delete — carry
hotkeys), and the dialogs (Properties, BatchRename, Duplicates, Update, Consent), HomeView, CommandBar.
