---
id: CPE-654
title: Import/export the tag store (UI)
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
Child of CPE-614. Command-palette actions "Export tags…" (save the whole store as JSON via the save
dialog) and "Import tags…" (pick a JSON file, merge via `import_tags`) — making the import backend
(CPE-640) reachable and giving tags backup/portability. Reuses `write_file_text`/`read_file_text`.

## Acceptance Criteria
- [x] `exportTagsToFile` / `importTagsFromFile` (save/open dialog + write/read + `exportTags`/`importTags`).
- [x] Palette commands "Export tags…" / "Import tags…" (App group); new `palette.exportTags`/`importTags`
      keys added to all 12 locales (coverage gate green).
- [x] `npm run check` clean; suite green.

## Work Log
2026-07-18 (dayshift) — Completed the tags UI: import/export now reachable from the command palette.
