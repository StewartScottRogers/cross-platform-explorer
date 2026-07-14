---
id: CPE-378
title: "Catalog auto-update toggle + per-agent pin (CPE-308 part 2, slice 4a)"
type: Feature
status: Done
priority: Low
component: Multiple
tags: [ready]
estimate: 2h
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The two persisted, testable catalog controls beyond manual refresh: an opt-in **auto-update**
toggle (default off; refreshes on open when on) and per-agent **pin** (catalog updates skip pinned
agents). Explicit rollback — which fights anti-rollback and needs release enumeration — is split to
CPE-379.

## Acceptance Criteria

- [x] `PresetStore` gains `autoUpdateCatalog` + `pinnedAgents` (persisted via storage); `set_pinned`.
- [x] `POST /api/catalog/settings {autoUpdate}` + `POST /api/catalog/pin {agent,pinned}`.
- [x] Refresh passes pins to the host; `apply_bundle(..., pinned)` skips them (`ApplyOutcome::Pinned`).
- [x] Launcher: Auto + Pin checkboxes (reflect state, persist on change); auto-refresh on open when on.
- [x] Tests: settings/pin persist + bad input rejected (ai-console); pinned agent skipped (host).

## Notes
Explicit rollback → [[CPE-379]]. Depends on [[CPE-374]]/[[CPE-376]]. Part of [[CPE-308]]. Launcher
controls need a GUI eyeball; dormant until catalog signing is set up.

## Work Log
2026-07-14 — Implemented + landed. ai-console 123 tests, host 11 tests; clippy clean.
