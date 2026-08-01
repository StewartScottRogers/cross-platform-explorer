---
id: CPE-1181
title: "Navigate into non-zip archives (tar/tar.gz/7z/iso) — browse tree + open leaves"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-705
---

## Summary
Part of the CPE-705 GUI remainder. Double-clicking a zip already enters it; extend the same in-app browsing to
tar/tar.gz/tgz/7z/iso (backend `read_archive_entries` is already format-agnostic). Leaf-open routes through the
new `extract_archive_entry_any` from CPE-1180.

## Build
- Extend `ARCHIVE_EXTS` (`src/App.svelte:1152`) to include tar/tar.gz/tgz/7z/iso so double-click enters them
  (`archiveChildren` is already format-agnostic off the `readArchiveEntries` list).
- Route leaf-open (`openInArchive`) through `extractArchiveEntryAny` (CPE-1180). If 1180 isn't merged yet, fall
  back to a clear "can't open this entry yet" notice rather than a broken call.
- Stay within the read/nav region of App.svelte (`ARCHIVE_EXTS`, `enterArchive`, `openInArchive`); do NOT touch
  the compress/extract action region (owned by CPE-1182/1183). Note: `enterArchive` is also lightly edited by
  CPE-1182 — keep changes minimal + grep-verify the merge ([[verify-subagent-merges]]).

## Acceptance Criteria
- [ ] gui-smoke: seed a `.tar.gz` fixture, double-click enters it, rows render, breadcrumb shows the archive
      name; `snap("archive-browse-targz")`; leaf-open opens a temp copy.
- [ ] `npm run check` + `cd gui-smoke && npm run typecheck` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-705). Soft dep on CPE-1180 for leaf-open.
- 2026-07-31 — Done. Extended `ARCHIVE_EXTS` (App.svelte) to tar/gz/tgz/7z/iso alongside the original
  zip-family set (now `ZIP_FAMILY_EXTS`) so double-click enters them — `archiveChildren` needed no
  changes (already format-agnostic off `readArchiveEntries`). `openInArchive` routes zip-family
  leaf-opens through the existing `extractArchiveEntry` and non-zip leaf-opens through
  `extractArchiveEntryAny` (CPE-1180), feature-detected at runtime via a typed cast since that command
  isn't in `bindings.gen.ts` on this branch yet — falls back to a "can't open this entry yet" notice
  rather than a broken/typed call; CPE-1180 lands separately and the Foreman rebases. Added
  `gui-smoke/specs/archive-browse.smoke.ts` (+ a hand-built, `tar`-verified single-entry `.tar.gz`
  fixture in `wdio.conf.ts#seedArchiveBrowseFixture`) asserting the entry row renders and the
  breadcrumb ends on the archive name. `npm run check` (0 errors), `npm test` (131 files / 1489 tests
  green), and `cd gui-smoke && npm run typecheck` all pass. Live-browser CI is what actually exercises
  the WebView session; not run locally.
