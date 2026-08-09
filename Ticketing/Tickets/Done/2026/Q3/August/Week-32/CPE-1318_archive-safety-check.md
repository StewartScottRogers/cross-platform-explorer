---
id: CPE-1318
title: Archive-safety check — right-click "Check archive safety" → zip-bomb report dialog
type: feature
component: Frontend
priority: medium
tags: ready
created: 2026-08-05
epic: CPE-1002
estimate: 2-3h
---

## Summary
Final file-inspection-safety surface (completes epic CPE-1002's UI). Unlike the 4 folder-tree scans in the
File-Health panel, archive-safety is SINGLE-ARCHIVE (a zip-bomb check on one file), so it's a right-click
action, not a panel tab (per the vetted scope). Surface the built-but-unsurfaced `analyze_archive_safety`.

## Backend command (exists + specta-bound)
- `analyze_archive_safety(path) → Result<ArchiveSafetyReport, String>` (plain, non-streaming — use `invoke`
  from `src/lib/invoke.ts`). `ArchiveSafetyReport { report: RatioReport, entries_scanned, truncated }`;
  `RatioReport { total_compressed, total_uncompressed, overall_ratio, flagged: FlaggedEntry[], dangerous }`.
  Confirm the exact FlaggedEntry fields in bindings.gen.ts.

## Scope
- Add a **context-menu item** "Check archive safety…" that appears for ARCHIVE files only. Investigate how the
  file/row context menu is built (grep the context-menu construction in App.svelte / the FileList row menu) and
  how archive files are identified in the frontend (there's archive detection — grep isArchive / archive ext
  helpers / how "Browse archive" or "Extract" menu items gate on archive type) and reuse that gating.
- New `ArchiveSafetyDialog.svelte` (model the shell on an existing small result dialog): shows overall_ratio
  (with a clear DANGER indicator when `dangerous` is true), total_compressed → total_uncompressed sizes
  (human-readable), the flagged entries list (each entry's path + its ratio, as reflow pills/rows), and
  entries_scanned (+ a "capped" note when truncated). Loading + error states.

## Acceptance Criteria
- [ ] Right-clicking an archive file shows "Check archive safety…"; non-archives don't. Opens the dialog,
      runs `analyze_archive_safety`, shows the report; `dangerous` is visually unmistakable (but per MENUS/theme
      rules use theme vars — a warning treatment via theme, not a hard-coded red-that-breaks-light/dark).
- [ ] Dialog: visible border, light-theme-only, flagged-entry rows/pills reflow, sizes human-readable.
- [ ] `ArchiveSafetyDialog.test.ts` (jsdom, mock invoke): asserts command+path arg, renders ratio/sizes/flagged
      rows/dangerous indicator, empty-flagged (safe) state, error path. Falsifiable.
- [ ] i18n new keys × 12 locales. If it adds a user-facing Section, wire sectionDocs + a doc page; otherwise
      extend `src/docs/22-file-health.md` to mention archive-safety.
- [ ] `npm run check` clean + full `npm run test:unit` green.

## Work Log
2026-08-05 (sprint run 2) — Filed by the Foreman. Slice 4 of the File-Health feature (single-archive,
right-click). Real GUI verification batched with the panel's build pass.
