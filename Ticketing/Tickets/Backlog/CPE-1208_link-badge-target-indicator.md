---
id: CPE-1208
title: "GUI: link badge + resolves-to target indicator in FileList (+ broken state)"
type: feature
component: Frontend
priority: medium
status: Backlog
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
- [ ] gui-smoke render pin over a listing with an intact symlink + a broken symlink (POSIX runner);
      `npm run check` + `npm test` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-715). Prereq CPE-1206. Disjoint files from 1207 → parallel.
