---
id: CPE-1183
title: "Extract-to… destination picker + .tar.gz compress option"
type: feature
component: Frontend
priority: medium
status: Doing
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
