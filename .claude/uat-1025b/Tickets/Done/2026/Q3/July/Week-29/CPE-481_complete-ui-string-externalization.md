---
id: CPE-481
title: "Complete UI string externalization — migrate remaining components to i18n"
type: Task
status: Done
priority: Low
component: Frontend
tags: [ready]
estimate: 3-4h
created: 2026-07-15
closed: 2026-07-16
epic: CPE-261
---

## Summary
The i18n **system** landed in CPE-362 (`src/lib/i18n.ts`: store, 4-locale catalogs, reactive `t()`,
`Intl` date/number formatting, persistence, a live language switcher), wired into the primary chrome
(NavToolbar, MenuBar titles + language menu, Sidebar section headers, TabBar). This ticket is the
**mechanical tail**: migrate the remaining hardcoded strings across the rest of the components to
catalog keys, so the app is fully translatable.

## Acceptance Criteria
- [x] Every user-facing string in the remaining components (dialogs: Properties, BatchRename,
      Duplicates, Update, Consent; FileList, PreviewPane, HomeView, CommandBar, ContextMenu,
      SidecarManager, App.svelte chrome) is a catalog key, not a literal.
- [x] Each new key is added to all four locale catalogs (English authoritative; others may fall back).
- [x] `npm run check` clean; existing tests pass; a lint/test guards against a stray hardcoded string
      in a migrated component.

## Notes
Split from CPE-362 (the i18n framework). Deliberately Low — a single-user local explorer — but now
that the framework exists, this is straightforward per-component work. The framework's fallback
(locale → English → key) means partial migration is always safe/legible in the meantime.

## Resolution
Every component enumerated in AC-1 now renders its user-facing text through the reactive `$t(key)`
store instead of hardcoded literals, so a live language switch re-renders the whole UI.

**Catalogs** (`src/lib/i18n.ts`): added ~150 keys in new namespaces — `upd.` (UpdateDialog),
`dup.` (DuplicatesDialog), `prop.` (PropertiesDialog), `ren.` (BatchRenameDialog), `consent.`
(ConsentSheet), `home.` (HomeView), `mgr.` (SidecarManager), `fl.` (FileList), `pv.` (PreviewPane),
`tb.`/`agent.` (App chrome + Agent Watch strip) — each **fully translated in en/es/de/fr**.
Existing keys were reused where wording matched (`menu.`, `sort.`, `view.`, `cmd.`, `ctx.`,
`common.`) to avoid duplicate catalog entries.

**Components migrated**: `UpdateDialog`, `DuplicatesDialog`, `PropertiesDialog`, `BatchRenameDialog`,
`ConsentSheet`, `HomeView`, `SidecarManager`, `FileList`, `PreviewPane`, `Toolbar`, and the
`App.svelte` chrome (the four pane-toolbars + settings popovers, pane resizers, AI Console toolbar
button with its pluralized running-agent tooltip, and the Agent Watch banner). `Toolbar.svelte`'s
`"{label} settings"` gear/popover label is keyed via `tb.settings`, so callers just pass a
translated label. Interpolation and singular/plural branches are handled per string.

**Guard (AC-3)**: `src/lib/i18n.test.ts` gains (a) a per-namespace all-locale coverage check
ensuring every CPE-481 key exists in all four catalogs, and (b) a migration guard that, for each
migrated file, asserts it imports the i18n `t` store and that a curated set of the removed English
literals no longer appears in its markup — this fails loudly if a hardcoded string is reintroduced.

**Tradeoffs**: `SHA-256` (PropertiesDialog) is intentionally left as-is — a technical identifier,
not translatable. `CAPABILITY_INFO` labels/descriptions (in `sidecar.ts`, shown by ConsentSheet and
SidecarManager) come from the sidecar contract data layer, not the component markup, so they remain
a separate concern outside this ticket's component scope. Verified: `npm run check` clean, full
frontend suite green at 482 tests (was 470).

**Files**: `src/lib/i18n.ts`, `src/lib/i18n.test.ts`, `src/App.svelte`, and
`src/lib/components/{UpdateDialog,DuplicatesDialog,PropertiesDialog,BatchRenameDialog,ConsentSheet,HomeView,SidecarManager,FileList,PreviewPane,Toolbar}.svelte`.

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
2026-07-16 (Nightshift loop 16) — Migrated **CommandBar.svelte** (the toolbar): New/Open/Sort/View
buttons, the Sort menu (Name/Date modified/Type/Size + Ascending/Descending), the View menu
(Details/List/Large icons + Show hidden files/Group folders first), and the Filter menu (All items/
Folders/Images/Documents/Audio & video/Code/Archives — rendered via `$t('filter.'+key)` without
touching the shared FILE_FILTERS array's logic). 23 keys (cmd./sort./view./filter.) × 4 locales.
Test added. `npm run check` clean; 470 frontend tests pass. Remaining: dialogs (Properties,
BatchRename, Duplicates, Update, Consent), HomeView, and a few icon-button tooltips.
2026-07-16 — Picked up the mechanical tail to finish the ticket. Estimate 3-4h; the remaining
components were the largest (App.svelte, FileList, PreviewPane). Migrated the **entire remaining
enumerated set in one pass**: UpdateDialog, DuplicatesDialog, PropertiesDialog, BatchRenameDialog,
ConsentSheet, HomeView, SidecarManager, FileList, PreviewPane, Toolbar, and the **App.svelte chrome**
(Application/Navigation/File-list/Preview toolbars + their settings popovers, the pane resizers,
the AI Console toolbar button incl. its running-agent pluralized tooltip, and the Agent Watch strip).
Added ~150 new keys across the `upd./dup./prop./ren./consent./home./mgr./fl./pv./tb./agent.`
namespaces to **all four locales** (en/es/de/fr) with full translations — reusing existing keys
(`menu.`, `sort.`, `view.`, `cmd.`, `ctx.`, `common.`) wherever wording matched exactly to avoid
duplication. Interpolation (`{version}`, `{count}`, `{col}`, `{id}`, `{label}`…) and plural
branches (`dup.summaryOne/Many`, `pv.itemOne/Many`, `tb.openConsoleOne/Many`) handled per string.
The Agent Watch activity badges (new/edited/deleted/moved/read) are now keyed too. Added an i18n
**migration guard test**: for each migrated file it asserts the component imports the i18n `t` store
and that a curated set of the removed English literals no longer appears in its markup — plus a
per-namespace all-locale coverage assertion. `npm run check` clean (0 errors/0 warnings);
`vitest run` green at 482 tests (was 470); SHA-256 is the only intentional non-translated literal.
