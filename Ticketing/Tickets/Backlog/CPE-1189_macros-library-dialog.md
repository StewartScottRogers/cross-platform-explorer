---
id: CPE-1189
title: "Macro library dialog — browse / create / edit / delete / import / export"
type: feature
component: Frontend
priority: medium
status: Backlog
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
