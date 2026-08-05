---
id: CPE-1312
title: folderWatch handleFolderBatch ignores OpResult.ok — fabricates success on failed move/copy
type: bug
component: Frontend
priority: high
tags: ready
created: 2026-08-04
estimate: 1-2h
---

## Summary
Data-integrity bug (found by the shift-3 bench researcher). `src/lib/folderWatch.ts:112-121`
`handleFolderBatch` reads `results[i]?.path` unconditionally and never checks `.ok`. `runWatchActions`
returns `OpResult[]` (`{ path, ok, error }`). On a real failure (disk full, permission denied) the batch
still calls `onFire()` with a fabricated success: the activity log records a false success, and
`undoFire`/`undoPlan` will later try to move a file back FROM a location nothing ever landed at, while the
real file sits untouched at its original path. Every other frontend consumer of `OpResult` (App.svelte + ~16
components) checks `.ok` before treating an op as done — this path is the outlier.

## Acceptance Criteria
- [ ] `handleFolderBatch` only treats an op as fired/undoable when `OpResult.ok === true`; a failed op is NOT
      recorded as a success and NOT made undoable (nothing happened to undo). Surface the failure the way the
      codebase already does elsewhere (e.g. console/log or a failed marker) rather than swallowing it.
- [ ] Follow the existing app convention for OpResult.ok handling (match App.svelte/components).
- [ ] Regression test in `folderWatch.test.ts`: mock `run` returns a mix of `ok:true` and `ok:false`; assert
      the fire only covers the successful path(s), the failed path is excluded, and undo does not fire on it.
      Prove falsifiable (fails against current code).
- [ ] `npm run check` clean + `npm run test:unit` green.

## Work Log
2026-08-04 (workshift) — Filed by the Foreman from the shift-3 bench researcher (grep-verified real bug).
Dispatched to a worker in parallel with CPE-1309.
