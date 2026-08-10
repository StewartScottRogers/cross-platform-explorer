---
id: CPE-1578
title: "Archive actions in the preview pane (Extract / Extract to… / Check safety)"
type: Task
status: Doing
priority: Medium
component: Frontend
epic: CPE-1568
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1568 slice 4. Archives preview as an entry list, but Extract/Extract-to/Check-safety live only in the
context menu — not the pane. Surface them on the CPE-1570 action bar (pure UI wiring onto existing backend commands).

## Scope
- Declare `actions` on the `archive` provider in `src/lib/preview/provider.ts` (CPE-1570 API): **Extract** (here),
  **Extract to…** (pick dest), **Check safety** (the archive-safety scan). Reuse the EXACT backend commands the
  ContextMenu already invokes for these (grep `ContextMenu.svelte` + `App.svelte` for the extract / archive-safety
  actions — `archiveEligible`/`extractable`/archive-safety). Do NOT duplicate backend logic.
- Labels via `$t()` (12 locales, CPE-481); Icon glyphs; theme-only colors (MENUS.md).

## Acceptance criteria
- Opening an archive shows Extract / Extract to… / Check safety in the action bar; each runs the same operation the
  context-menu items do.
- Unit/component tests: actions render + run (mock backend), enablement gating (mirror the JWT/JSON/image action tests).
- `npm run check` clean; vitest green. Frontend-only wiring; no new deps.

## Notes
Builds on CPE-1570 action bar. Touches `provider.ts`/`PreviewPane.svelte` — serialize vs other CPE-1568 preview
slices (none currently in flight). Model: sonnet.
