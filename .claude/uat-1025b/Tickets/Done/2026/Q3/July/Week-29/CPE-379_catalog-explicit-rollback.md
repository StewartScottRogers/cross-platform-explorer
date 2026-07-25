---
id: CPE-379
title: "Catalog rollback: reset to the shipped agents (CPE-308 part 2, slice 4b)"
type: Feature
status: Done
priority: Low
component: Multiple
tags: [ready]
estimate: 1h
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The practical rollback: undo a bad catalog update by reverting to the **shipped** (bundled) agents.
`POST /api/catalog/reset` clears the fetched verified source (manifests + sigs + version map) and
hot-reloads, so the registry returns to the first-party bundled set. Rolling back to a *specific
intermediate* published version (needs release enumeration + a downgrade override) is split to
CPE-383.

## Acceptance Criteria

- [x] `POST /api/catalog/reset` clears the fetched signed source + version map and reloads.
- [x] Launcher **Reset** button → reset + re-render agents.
- [x] Test: reset reverts a fetched agent back to the shipped set + clears the files.
- [x] clippy `--all-targets -- -D warnings` clean.

## Notes
Reset also wipes the anti-rollback version map, so a subsequent refresh starts fresh (fetches
latest). Specific-version rollback → [[CPE-383]]. Part of [[CPE-308]]. Launcher button needs a GUI
eyeball.

## Work Log
2026-07-14 — Implemented reset-to-shipped rollback. 124 ai-console tests; clippy (all-targets) clean.
