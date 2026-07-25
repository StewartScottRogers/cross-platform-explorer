---
id: CPE-698
title: Hoist Agent-Watch activity-path normalization out of the per-row folder check
type: enhancement
component: Frontend
priority: low
status: Done
created: 2026-07-18
closed: 2026-07-18
epic: CPE-396
estimate: 45m
---

## Summary
`folderHasActivity(activityPaths, entry.path)` runs for every folder row (`FileList.svelte:304`) and
internally re-normalizes **every** activity path on **every** call
(`paths.some((p) => normalizePath(p).startsWith(prefix))`). During Agent Watch with M active paths and F
folder rows that's O(F·M) `normalizePath` allocations per render. The activity paths only change when the
activity map does, so normalize them **once** and pass the normalized list to the per-row check.

Agent Watch's precedence (AGENT-WATCH.md) puts visibility above speed, but this is a pure, behaviour-
preserving reduction of redundant work on a live feature — no visibility cost. Headless (pure logic +
`agentActivity.test.ts`), so safe to land unattended.

## Acceptance Criteria
- [x] A `folderHasActivityNorm(normPaths, dir)` takes already-normalized paths; `folderHasActivity` stays
      (delegates) so existing callers/tests are unchanged.
- [x] FileList precomputes the normalized activity paths once (reactive) and the per-row check uses them.
- [x] Behaviour identical (existing agentActivity tests pass) + a test for the normalized variant.
- [x] `npm run check` + full suite green.

## Work Log
2026-07-18 (nightshift) — Picked up during the backend audit tick; found this live-feature redundancy on
the way. Estimate: 45m.

2026-07-18 — `agentActivity.ts`: split `folderHasActivity` into `normalizeActivityPaths(paths)` (map
`normalizePath`, drop empties) + `folderHasActivityNorm(normPaths, dir)` (prefix test on already-normalized
paths). `folderHasActivity` now delegates to both, so its callers/tests are unchanged. `FileList.svelte`:
`$: activityPaths = normalizeActivityPaths(Object.keys(activity))` (once per activity change) and the
per-row `{@const insideActive}` calls `folderHasActivityNorm` — so the M activity paths are normalized once
per render, not once per folder row (was O(F·M) normalizePath allocations).

2026-07-18 — Tests: parity test (folderHasActivityNorm on pre-normalized ≡ folderHasActivity on raw across
several dirs), a normalize/drop-empties test, and the self/prefix-sibling exclusion. Caught that
`normalizePath` lowercases (case-insensitive compare) — fixed the expectation. `npm run check` clean; full
suite green: 704 tests / 74 files (+3).

## Resolution
The Agent-Watch folder-highlight check re-normalized every activity path for every folder row — O(F·M)
`normalizePath` allocations per render. Extracted `normalizeActivityPaths` + `folderHasActivityNorm` so
FileList normalizes the activity set once per activity change and each folder row does only a cheap prefix
test (plus normalizing its own path). `folderHasActivity` is preserved as a delegating convenience.
Behaviour identical (parity test). Files: `src/lib/agentActivity.ts`, `src/lib/agentActivity.test.ts`,
`src/lib/components/FileList.svelte`. Improves Agent Watch responsiveness (epic CPE-396) without touching
its visibility.
