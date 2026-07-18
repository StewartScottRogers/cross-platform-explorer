---
id: CPE-622
title: Frontend transfer store (consumes progress events)
type: feature
component: Frontend
priority: high
status: Done
tags: ready
estimate: 1h
created: 2026-07-18
closed: 2026-07-18
epic: CPE-613
---

## Summary
Child of CPE-613. A reactive store that folds the backend's `transfer://progress` / `transfer://done`
events (CPE-620) into a list the operations panel will render, plus `startTransfer`/`cancelTransfer`
helpers. Pure reducer (upsert/markFinished/dismiss/percent) is unit-tested; the store tail wires the
Tauri events. Idle by default.

## Acceptance Criteria
- [x] Pure `upsertProgress` / `markFinished` / `dismiss` / `percent` reducers, unit-tested (incl. the
      "late progress event must not wipe the report" edge and the bytes→items percent fallback).
- [x] `transfers` readable store + idempotent `initTransfers()` listener; `startTransfer`/`cancelTransfer`
      wrap the backend commands (via the busy-tracking invoke).
- [x] `npm run check` clean; vitest green.

## Resolution
Added `src/lib/transfers.ts` + `transfers.test.ts` (4 tests). Panel UI (CPE-623) and wiring copy/paste
through it (CPE-625) follow.

## Work Log
2026-07-18 (dayshift) — Built + tested the store.
