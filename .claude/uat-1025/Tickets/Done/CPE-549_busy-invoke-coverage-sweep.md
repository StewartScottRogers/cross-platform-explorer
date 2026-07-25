---
id: CPE-549
title: "Busy cursor — migrate all invoke call sites to the wrapper + boundary guard test"
type: Task
status: Open
priority: Medium
component: Frontend
tags: [needs-prereq]
epic: CPE-547
estimate: 2-3h
created: 2026-07-16
closed:
---

## Summary
Wave 2 of [[CPE-547]]. With the wrapper from [[CPE-548]] in place, make "everywhere" real: migrate
every `invoke(...)` call site in `src/` that imports raw `@tauri-apps/api/core` over to
`src/lib/invoke.ts`, **except** the streaming/self-progress opt-outs (handled by [[CPE-550]]). Then lock
it in with a **guard test** so new raw-invoke imports can't silently regress coverage.

## Prereq
`needs-prereq`: [[CPE-548]] (the wrapper module) must land first.

## Acceptance Criteria
- [ ] Every non-opt-out `.svelte`/`.ts` in `src/` that called core `invoke` now imports from
      `src/lib/invoke.ts` (import swap; no logic change).
- [ ] A guard test (vitest) asserts that no file under `src/` imports `invoke` from
      `@tauri-apps/api/core` except `src/lib/invoke.ts` itself and an explicit **opt-out allowlist**
      (the streaming call sites) — so regressions fail CI.
- [ ] The opt-out allowlist is defined in one place the guard test reads, not scattered.
- [ ] `npm run check` clean; existing component/unit tests still pass (update any test that mocks the
      raw import path to the wrapper as needed).

## Notes
Pure import migration + a durability test — this is what turns the epic's "coverage sweep" from a
one-time audit into an enforced invariant. Streaming opt-outs are enumerated + justified in [[CPE-550]];
this ticket only needs the allowlist mechanism to exist.
