---
id: CPE-1183
title: "Extract-to… destination picker + .tar.gz compress option"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-705
---

## Summary
Part of the CPE-705 GUI remainder. Today compress is hardcoded `.zip` and extract is "here" only. Add an
"Extract to…" destination picker and a compress format choice (.zip vs .tar.gz via `compress_archive`).
**Batched with CPE-1182 in one worker** (both edit `doCompress` + add `ContextMenu.svelte` rows).

## Build
- "Extract to…" context row → `openFolderDialog` (already imported, `App.svelte:8`) → `extractArchive` into the
  chosen dir (alongside the existing extract-here).
- Compress format choice (.zip vs .tar.gz) routed through `compressArchive` instead of the hardcoded
  `compressToZip`.
- Menu rows theme-only + icons per conventions; `invoke` via `src/lib/invoke.ts`.

## Acceptance Criteria
- [ ] Headless: "extract to" a chosen dir places files there; compressing with the tar.gz option produces a
      valid `.tar.gz` (assert via `readArchiveEntries`).
- [ ] gui-smoke `snap` of the enriched context menu; `npm run check` + `npm test` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-705). Runs after/with CPE-1182 (same worker, sequential).
- 2026-07-31 — Done. New "Extract to…" context row → `doExtractTo` opens the native folder picker
  (`openFolderDialog`, cancel-safe like `copyMoveToFolder`) and extracts into the chosen dir via the
  shared `extractWithPasswordFallback` (so an "Extract to…" on a password-protected archive prompts too,
  same as plain "Extract"). Compress format choice: new "Compress to .tar.gz" row → `doCompressAs(".zip"
  | ".tar.gz")` routes through `compressArchive` (format picked by `dest`'s extension), leaving the
  existing quick "Compress to ZIP" → `compressToZip` untouched, per the ticket's own suggestion to keep
  that fast path rather than forcing everything through a picker/submenu. `compressBaseName()` factored
  out of `doCompress` so the naming logic (single-item stem vs "Archive" for a multi-selection) is
  shared across all three compress variants instead of duplicated. Tests: `App.archivePassword.test.ts`
  covers extract-to using the chosen dir (and a no-op on a cancelled picker) and tar.gz routing through
  `compress_archive` (not the hardcoded `compress_to_zip`); `ContextMenu.test.ts` covers the new rows'
  gating + dispatched action names. `gui-smoke/specs/archive-password.smoke.ts`'s first test opens the
  item menu on the already-seeded CPE-1181 `.tar.gz` fixture and snaps `archive-context-menu-enriched`,
  asserting both the Extract*/Compress* groups render together; not run locally (CI's job) but
  `cd gui-smoke && npm run typecheck` passes. `npm run check` (0 errors) and `npm test` (134 files /
  1512 tests) green. Built with CPE-1182 on one branch — see that ticket for its own log.
