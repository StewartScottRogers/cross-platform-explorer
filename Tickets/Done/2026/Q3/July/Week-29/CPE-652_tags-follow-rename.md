---
id: CPE-652
title: Tags follow an in-app rename (wire retag_path)
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
Child of CPE-614. The `retag_path` backend (CPE-650) was unwired, so tags orphaned when a file was
renamed in-app. `commitRename` now calls `retagPath(old, new)` after a successful rename so the tags
follow the file (and the in-memory store updates). Added the `retagPath`/`renameTag`/`deleteTag`/
`importTags`/`exportTags` service functions to `tags.ts`.

## Acceptance Criteria
- [x] After an inline rename, the file's tags/label move to the new path (frontend store + tags.json).
- [x] `tags.ts` exposes retagPath/renameTag/deleteTag/importTags/exportTags (wrap the backend commands).
- [x] `npm run check` clean; suite green.

## Work Log
2026-07-18 (dayshift) — Wired tag-following on rename; added the management service functions the
sidebar tag-menu + import/export UIs (CPE-653/654) will use.
