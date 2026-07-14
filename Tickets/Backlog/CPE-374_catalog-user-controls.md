---
id: CPE-374
title: "Catalog user controls: refresh, auto-update toggle, pin/rollback (CPE-308 part 2, slice 3)"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-14
---

## Summary

The launcher UI for catalog updates: a manual **Refresh**, an **auto-update** toggle (opt-in,
default off — design D4), and **pin/rollback** per agent id. Surfaces provenance + the last-applied
catalog version.

## Acceptance Criteria

- [ ] Manual refresh triggers CPE-373's apply; result (updated/no-change/offline/failed) shown.
- [ ] Auto-update toggle persisted; default off (matches the CPE-296 no-unconsented-execution posture).
- [ ] Pin an agent to its current version (ignore newer) and rollback to a prior version explicitly
      (the only way past anti-rollback).
- [ ] Panel gets the standard visible border; visual QA noted (launcher UI isn't headlessly verifiable).

## Notes
Depends on [[CPE-373]] (fetch/apply) which depends on [[CPE-372]]. `needs-prereq` until CPE-373 lands.

## Work Log
2026-07-14 — Filed as slice 3 of CPE-308 part 2.
