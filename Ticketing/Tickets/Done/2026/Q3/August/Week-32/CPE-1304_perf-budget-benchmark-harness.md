---
id: CPE-1304
title: Multi-size perf benchmark harness + dev-gated timing marks (completes CPE-691)
type: test
component: Frontend
priority: medium
tags: ready
created: 2026-08-03
epic: CPE-688
estimate: 2-3h
---

## Summary
Child of CPE-688 (explorer 10× perf). CPE-691 shipped only the single-size (5000-entry) full-list
regression guard (`FileList.virtualize-guard.test.ts`); its other two claimed deliverables are NOT in the
codebase today:
- the dev-gated **time-to-first-paint / time-to-settled** marks were added (`a6c981cd`) then **removed**
  by CPE-757 (`5dd563b0`) as a "temporary on-screen perf readout"; grep for `performance.mark`/`[perf]`
  today → zero hits.
- the scripted **multi-size (100 / 1k / 10k / 50k)** benchmark was never built.

This ticket finishes the harness so the "10×" is a falsifiable, pinned before/after — not a vibe.

## Acceptance Criteria
- [ ] Permanent **dev-gated** (tree-shaken in production, NOT a user-visible on-screen readout) timing
      instrumentation around the streaming-fetch → settled path. NB: that path moved to
      `ExplorerPane.loadListing` (CPE-676 domino 3b) — `App.svelte:loadPath` is the wrong touch point now.
- [ ] A `vitest`/jsdom **multi-size benchmark budget** test driving FileList with 100/1k/10k/50k synthetic
      entries, recording mount/settle timing, that FAILS if rendered-row count scales with N (reuse the
      `getBoundingClientRect` stub pattern from `FileList.virtualize-guard.test.ts`) or if a size exceeds a
      budget. Proven falsifiable (document forcing a regression → it fails → revert).
- [ ] No new deps. `npm run check` clean + full `npm run test:unit` green.
- [ ] Correct CPE-691's mis-filed status (its boxes are checked but it sits in Deferred with unbuilt ACs) —
      leave a one-line Work Log note pointing at this ticket; do not silently rewrite history.

## Notes
Traps (from the survey researcher): (1) do NOT re-add the CPE-757-removed on-screen readout — dev-gated
console marks / `performance.measure` only. (2) Instrument `ExplorerPane.svelte`, not `App.svelte`.
(3) Fresh ticket, don't reopen CPE-691.

## Work Log
2026-08-03 (workshift) — Filed by the Foreman from the survey researcher's finding (CPE-691 harness half is
genuinely unbuilt + fully headless). Dispatched to a worker.
