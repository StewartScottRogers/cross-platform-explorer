---
id: CPE-374
title: "Catalog refresh UI: 'Update agents' in the launcher (CPE-308 part 2, slice 3)"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 1h
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The user-facing control for catalog updates: a launcher **Update agents** button that fetches +
applies the signed catalog (CPE-376) and re-renders the agent list. The persisted auto-update
toggle + pin/rollback (which need new backend state + an anti-rollback override) are split to
CPE-378.

## Acceptance Criteria

- [x] "Update agents" toolbar button → `POST /api/catalog/refresh` → host fetch + apply + hot-reload.
- [x] Result surfaced: updated N / already up to date / none available (offline or unpublished) / failed.
- [x] On a non-empty apply, the agent list re-renders (`load()`).
- [x] ai-console build clean.

## Notes
Depends on [[CPE-376]]/[[CPE-375]]. Auto-update toggle + pin/rollback → [[CPE-378]]. Visual QA
pending — the launcher panel can't be verified headlessly (needs a GUI eyeball, and is dormant until
a signed catalog is published per CPE-377).

## Work Log
2026-07-14 — Implemented the Update-agents button + `refreshCatalog()` wired to `/api/catalog/refresh`.
Rescoped from the full controls set; toggle + pin/rollback filed as CPE-378.
