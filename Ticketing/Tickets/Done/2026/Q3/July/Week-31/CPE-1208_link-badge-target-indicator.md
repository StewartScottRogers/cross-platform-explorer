---
id: CPE-1208
title: "GUI: link badge + resolves-to target indicator in FileList (+ broken state)"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-08-01
epic: CPE-715
---

## Summary
Part of CPE-715 (after CPE-1206). Show which entries are links + their target, with a distinct broken state.

## Build
- Render a link glyph/badge on entries where `is_symlink` (CPE-1206), reusing `Icon` + the existing FileList
  row-badge system. LAZY `linkStatus` call for the target subtitle/tooltip (on render/hover — NOT in the hot
  listing path; matters for the virtualized 10k-entry FileList). A distinct "broken" badge when
  `link_status.broken`.

## Acceptance Criteria
- [x] gui-smoke render pin over a listing with an intact symlink + a broken symlink (POSIX runner);
      `npm run check` + `npm test` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-715). Prereq CPE-1206. Disjoint files from 1207 → parallel.
- 2026-08-01 — Implemented by Worker (workshift):
  - Added a `link`/`link-broken` glyph pair to `Icon.svelte` and a new `LinkBadge.svelte` component
    (mirrors `ThumbnailImage.svelte`'s CPE-643 lazy-fetch pattern): mounted only for rows where
    `DirEntry.is_symlink` is true, it lazily calls the typed `commands.linkStatus(path)` client
    (busy-tracked, per BUSY-CURSOR.md) once the badge nears the viewport (IntersectionObserver,
    150px rootMargin) or is hovered — never eagerly for every row. Until the fetch resolves it shows
    a neutral badge; a resolved `broken: true` flips it to a distinct red/warning badge state; the
    tooltip/aria-label shows "Resolves to …" or the broken message via new i18n keys
    (`fl.link`/`fl.linkResolvesTo`/`fl.linkBroken`, added to all 12 complete locales in `i18n.ts`).
  - Wired `<LinkBadge path={entry.path}>` into `FileList.svelte`'s `.cell.name`, gated on
    `entry.is_symlink` — a folder with zero symlinks mounts zero badges and fires zero `linkStatus`
    calls, so the hot virtualized listing path is unchanged for the common case.
  - Tests: 3 new cases in `FileList.test.ts` (badge renders for a symlink entry; broken state after
    `link_status` resolves broken; a plain entry has no badge and never calls `link_status`) — routed
    the file's `@tauri-apps/api/core` mock by command name so `link_status` can be controlled per test.
    `npm run check` (svelte-check): 0 errors/warnings. `npm test` (root vitest): 141 files / 1572
    tests, all green.
  - gui-smoke: `wdio.conf.ts` gained `seedLinkBadgeFixture` (an intact symlink + a permanently-broken
    one in the shared seeded tmpDir, best-effort — records `linkBadgeFixture.supported` in
    `.smoke-state.json` so the new spec skips gracefully on a runner where unprivileged symlink
    creation fails, e.g. Windows without Developer Mode) and `specs/link-badge.smoke.ts` asserts the
    intact link's badge, the broken link's `.broken` badge, and that the plain target file gets none.
    `cd gui-smoke && npm run typecheck`: 0 errors (live run is CI, per POSIX runners).
