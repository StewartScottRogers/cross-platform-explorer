---
id: CPE-640
title: Import/export the tag store
type: feature
component: Backend
priority: low
status: Done
tags: ready
estimate: 30m
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
Child of CPE-614. `import_tags(json)` merges a previously-exported tag store into the current one
(non-destructive: existing tags kept, imported unioned in). Export is just `load_tags` + `JSON.stringify`
on the frontend, so no separate export command is needed.

## Acceptance Criteria
- [x] Pure `tag_store_merge` (union tags, prefer non-empty imported label); unit-tested.
- [x] `import_tags` command (parse → merge → save), registered; invalid JSON errors cleanly.
- [x] cargo tests + clippy clean.

## Work Log
2026-07-18 (dayshift) — Added the merge/import; export is a frontend stringify.
