---
id: CPE-1323
title: File-Health panel — exclude-glob configuration UI
type: feature
status: Done
component: Frontend
priority: medium
tags: ready
estimate: 2h
created: 2026-08-05
closed: 2026-08-05
epic: CPE-1002
---

## Summary
The File-Health panel (`src/lib/components/FileHealthDialog.svelte`) already threads
`excludes: string[]` through to all four scan commands (`find_dangling_links_stream`,
`find_type_mismatches_stream`, `find_orphan_sidecars_stream`, `find_empty_dirs`) but hardcodes it to
`[]` at every call site. The exclude-glob BACKEND (CPE-1302) is Done — walkers already prune a
directory/entry whose name matches any exclude glob. This ticket is the UI to let the user actually
configure those excludes instead of always scanning everything.

## Build
1. Add an exclude-glob input to `FileHealthDialog.svelte`, shared across all four tabs (one array of
   patterns, not per-tab). Parse entered patterns into `string[]` and feed that array to all four scan
   calls in place of the hardcoded `excludes: []`.
2. Render active excludes as reflowing pills/tick-tacks (`display:flex; flex-wrap:wrap; gap` container,
   `white-space:nowrap; flex:0 0 auto` pills — CLAUDE.md's pill convention). Each pill has a remove (×)
   affordance. Mirror `TagEditor.svelte`'s existing chip-input pattern (Enter-to-add, dedupe,
   Backspace-peels-last-chip) rather than inventing a new interaction.
3. Quick-add suggestion chips for `node_modules`, `.git`, `target` — add-on-click, NOT pre-applied (must
   not silently change scan results for a user who never touches the excludes UI).
4. Excludes take effect on the next Scan/Rescan click only (no auto-rescan on keystroke). Preserve each
   tab's existing generation-token / cancel-prior-stream / drop-stale-batch discipline unchanged.
5. Light-theme palette + existing dialog styling only (app is light-theme-only — no dark overrides).

## Acceptance Criteria
- [x] All four scan calls pass the configured `excludes` array instead of a hardcoded `[]`.
- [x] Exclude pills reflow (wrap container, non-shrinking nowrap pills), each removable.
- [x] Quick-add chips for `node_modules` / `.git` / `target` add on click only, never pre-applied.
- [x] Editing excludes never triggers a scan by itself; only Scan/Rescan reads the current list.
- [x] Per-tab generation/cancel/supersede behavior is unchanged.
- [x] `src/docs/22-file-health.md` documents the exclude input (same section/slug).
- [x] `npm run check` clean; the five FileHealthDialog vitest suites green, including new
      add/remove-exclude coverage and updated call-shape assertions (configured array, not `[]`).
- [x] i18n: new keys defined for all `COMPLETE_LOCALES` (en/es/de/fr/it/pt/nl/pl/ru/zh/ja/ko).

## Notes
Frontend-only — no Rust changes, no bindings regen (CPE-1302 already shipped the backend + typed
`excludes` params). Epic CPE-1002 (File-Health).

## Work Log
- 2026-08-05 — Picked up by a sprint Worker. Ticket file was missing from this worktree at pickup
  time (not yet committed anywhere); recreated from the assigning brief before starting, since the spec
  was fully detailed there. Backend dependency CPE-1302 confirmed Done via `Ticketing/Tickets/Done/`.
- 2026-08-05 — Implemented: shared `excludes: string[]` + `excludeDraft` state in
  `FileHealthDialog.svelte`, threaded into all four scan calls (was `excludes: []` at every site).
  Exclude pills rendered between the tab strip and each tab's body (`.excludes`/`.chips`/`.chip`
  tick-tacks, mirroring `TagEditor.svelte`'s Enter-to-add/dedupe/Backspace-peel interaction) plus
  quick-add suggestion chips for `node_modules`/`.git`/`target` (add-on-click only, filtered out of the
  suggestion row once added). 7 new i18n keys (`fh.exclude*`) added to all 12 `COMPLETE_LOCALES`.
  `src/docs/22-file-health.md` got a new "Excluding folders from a scan" section. Added/updated
  vitest coverage across all five FileHealthDialog suites (add/remove/dedupe/quick-add/no-scan-on-edit/
  cross-tab-shared-state + configured-array-reaches-each-of-the-four-commands). `npm run check`: 0
  errors. Full `npx vitest run`: 180 files / 2014 tests green (no regressions). PR opened for review.
