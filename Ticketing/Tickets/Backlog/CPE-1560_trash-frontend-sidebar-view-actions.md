---
id: CPE-1560
title: "Trash frontend: sidebar section + TrashView + Restore/Empty actions + docs"
type: Task
status: Backlog
priority: Medium
component: Frontend
epic: CPE-1486
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1486 slice 3 — the user-facing Trash. Depends on the bindings from CPE-1559.

## Scope
- Add a **Trash** entry to `src/lib/components/Sidebar.svelte` as its **own section** (like Smart Folders — a trash
  entry isn't a navigable directory `Place`). Gate visibility/enabled-state on a `can_browse_trash` availability
  probe (same Windows/Linux-only gate as `can_restore_from_trash`); **macOS shows a clean "open your OS Trash / Finder" message**, not a broken empty view.
- New `src/lib/components/TrashView.svelte` — reuse `FileList.svelte` row/virtualization plumbing where practical;
  trash-specific columns: name, original path, deleted date. Consume `list_trash` streaming (paint progressively).
- Restore / Empty actions via a menu per `docs/design/MENUS.md`: item text stays `var(--text)` (**never red**);
  "Empty Trash" is irreversible → route through `ConfirmDialog` (red belongs only on the dialog's primary button).
- Docs per CPE-579: new `src/docs/NN-trash.md` page + a new `Section` id (e.g. `"trash"`) in `src/lib/sectionDocs.ts`
  (the `sectionDocs.test.ts` guard fails CI without it).

## Acceptance criteria
- Trash entry appears on Win/Linux; macOS shows the Finder message.
- Listing paints progressively for a large trash; Restore returns an item to origin and drops it from the list;
  Empty (confirmed) purges and clears.
- Menu passes the MENUS.md checklist; `sectionDocs.test.ts` passes; `npm run check` + vitest green.
- Component tests (jsdom) cover the wiring headlessly; GUI visual pass via screenshots (Visual Critic).

## Notes
**Must serialize after CPE-1559.** Touches hot `Sidebar.svelte`/`App.svelte` — check for concurrent sidebar-touching
tickets before starting ([[parallel-pr-duplicate-import-trap]]). Blocked-by: CPE-1559. Model: sonnet (opus if App.svelte wiring gets gnarly).
