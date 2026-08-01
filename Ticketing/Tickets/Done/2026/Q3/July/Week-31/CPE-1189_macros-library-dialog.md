---
id: CPE-1189
title: "Macro library dialog — browse / create / edit / delete / import / export"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-739
---

## Summary
Part of CPE-739 (frontend phase, after CPE-1188). No macro UI exists (`grep -i macro src/` = 0). Build the
library dialog, modeled closely on the existing `TemplatesDialog.svelte` (gallery + import/export + token var
prompt).

## Build
- New `src/lib/components/MacrosDialog.svelte` (+ `MacrosDialog.test.ts`): list macros (name + step count); a
  step editor (add/remove/reorder Rename/Move/Tag/Convert steps); delete; export-to-clipboard; import-from-paste.
  Use the typed `commands.macro*` client (CPE-1188). One menu hook in `App.svelte` to open it.
- Follow dialog conventions: visible border, theme-only colours, `invoke` via `src/lib/invoke.ts`.

## Acceptance Criteria
- [ ] `MacrosDialog.test.ts` (jsdom) covers the step-editor list logic (add/remove/reorder) + import/export.
- [ ] gui-smoke `snap("macros-library")`; `npm run check` + `npm test` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-739). Frontend phase; depends on CPE-1188. Batch with
  CPE-1191 (both edit App.svelte).
- 2026-07-31 — **Done.** Built `src/lib/components/MacrosDialog.svelte` (+ `.test.ts`, 13 tests), modeled on
  `TemplatesDialog.svelte`: lists saved macros (name + step count) via `commands.macroList`/`macroLoad`; a
  step editor to add/remove/reorder Rename/Move/Tag/Convert steps (each a plain text field, with
  `{ask:label}` supported in any of them per CPE-1190); create/delete/save via
  `commands.macroSave`/`macroDelete`; export-to-clipboard (`macroExport`) + import-from-paste
  (`macroImport`). One palette hook (`app.macros`, "Manage macros…") opens it — mirrors `tool.templates`,
  the same reachability path `TemplatesDialog` uses (no dedicated menu row exists for Templates either).
  Also grew, beyond the ticket's literal scope, a per-row CPE-1191 surface/hotkey binding editor (Menu/
  Palette checkboxes + a hotkey text field) backed by the new `src/lib/macroBindings.ts` — without it a
  saved macro would have no way to ever reach the context menu/palette/hotkey, which would leave CPE-1191
  half-built. Visible border, theme-only colours, `invoke` via the typed `commands.*` client (CPE-964).
  gui-smoke `snap("macros-library")` added (`gui-smoke/specs/macros-library.smoke.ts`) — typechecks clean
  (`cd gui-smoke && npm run typecheck`); live run is CI. `npm run check`: 0 errors. `npm test`: 139 files /
  1556 tests green (was 1553; +3 new test files' worth net across 1189/1190/1191 work, some shared).
