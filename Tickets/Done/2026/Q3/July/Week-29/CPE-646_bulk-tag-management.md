---
id: CPE-646
title: Bulk tag management (rename/delete a tag everywhere)
type: feature
component: Backend
priority: low
status: Done
tags: ready
estimate: 45m
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
Child of CPE-614. Backend ops to rename or delete a tag across every path (the tag-management UI will
call these) — you can't easily do "rename tag X→Y everywhere" via per-file set_tags.

## Acceptance Criteria
- [x] Pure `tag_store_rename_tag` (de-dupes, empty new = delete), `tag_store_delete_tag`,
      `tag_store_prune_empty`; unit-tested.
- [x] `rename_tag` / `delete_tag` commands (load → modify → prune → save), registered.
- [x] cargo tests + clippy clean.

## Work Log
2026-07-18 (dayshift) — Added bulk tag ops on top of the store.
