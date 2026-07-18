---
id: CPE-653
title: Rename/delete a tag from the sidebar (tag context menu)
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
Child of CPE-614. Right-click a tag in the sidebar Tags section → a small popover to **rename** it
(across every file) or **delete** it — making the `rename_tag`/`delete_tag` backend commands (CPE-646)
reachable. Reuses existing i18n keys (ctx.rename/menu.delete/common.apply/cancel); theme-only colours
per MENUS.md.

## Acceptance Criteria
- [x] `TagMenu.svelte` popover (rename input + Delete + Cancel) at the click position.
- [x] Sidebar dispatches `tagMenu` on right-click of a tag; App calls `renameTag`/`deleteTag` and keeps
      the active filter consistent (rename carries the filter, delete clears it).
- [x] `npm run check` clean; suite green.

## Work Log
2026-07-18 (dayshift) — Wired tag rename/delete UI onto the sidebar tags.
