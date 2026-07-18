---
id: CPE-605
title: Command palette — cover the rest of the app's actions
type: enhancement
component: navigation
priority: low
status: Done
tags: ready
estimate: 30m
created: 2026-07-17
closed: 2026-07-17
epic:
sprint:
---

## Summary

The new command palette (CPE-602) exposed ~30 actions but omitted several the app already has:
rename, duplicate, delete, delete permanently, properties, select all, reveal in OS file manager,
open terminal here, toggle the details panel, pop out the preview, and close/reopen tab. It should
cover them so the palette is a complete keyboard-driven surface. Also fixes a small consistency bug:
the palette's Sort commands changed the sort but didn't persist it (unlike the sort dropdown).

## Acceptance Criteria

- [x] Palette adds: Rename, Duplicate, Delete, Delete permanently, Properties, Select all, Reveal in
      file manager, Open terminal here, Toggle details panel, Pop out preview, Close tab, Reopen
      closed tab — each with the right `enabled` predicate and shortcut hint.
- [x] The Sort commands persist the choice (`saveSortKey`/`saveSortDir`), matching the sort dropdown.
- [x] `npm run check` clean; frontend suite green.

## Resolution
Extended `paletteCommands` in `src/App.svelte` with the missing actions, reusing the existing
handlers (`beginRename`, `doDuplicate`, `doDelete`, `openProperties`, `selectAll`,
`revealInExplorer`, `openTerminal`, `popOutPreview`, `closeTab`, `reopenClosedTab`, the details
toggle) and correct enable predicates. Sort commands now call `settings.saveSortKey/saveSortDir`.

## Work Log
2026-07-17 (Nightshift Loop 4) — Built + verified. Pure wiring over existing handlers; no new UI.
