---
id: CPE-650
title: Tags follow an in-app rename/move (retag_path)
type: feature
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

## Summary
Child of CPE-614. `retag_path(from, to)` re-keys a path's tags/label so they follow a file renamed or
moved within the app (path-keyed tags would otherwise orphan). The tags UI calls it after a successful
rename/move (a small frontend follow-up).

## Acceptance Criteria
- [x] Pure `tag_store_rename` (re-key; no-op for an untagged path); unit-tested.
- [x] `retag_path` command (load → re-key if present → save), registered.
- [x] cargo tests + clippy clean.

## Work Log
2026-07-18 (dayshift) — Backend re-key; the frontend calls it on rename (follow-up in the tags UI).
