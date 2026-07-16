---
id: CPE-481
title: "Complete UI string externalization — migrate remaining components to i18n"
type: Task
status: Open
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
