---
id: CPE-691
title: Perf benchmark harness + regression budget
type: test
component: Frontend
priority: medium
status: Deferred
tags: deferred-internal
created: 2026-07-18
epic: CPE-688
estimate: 2-3h
---

## Summary
Child of CPE-688. A repeatable measure of time-to-first-paint and time-to-settled for folders of
~100/1k/10k/50k entries, split backend-walk vs frontend-render (console marks or a dev overlay), so the
"10×" is a falsifiable before/after. Add a smoke/budget test that fails if the file list regresses to
full-list rendering.

## Acceptance Criteria
- [x] Time-to-first-paint + time-to-settled marks (dev-gated) in loadPath — the core measurement.
- [ ] A regression guard (test/budget) against full-list rendering.
- [ ] `npm run check` + suite green.

## Work Log
2026-07-18 (dayshift) — Picked up. Doing the safe 'measure-first' part now (dev-gated time-to-first-paint/settle marks in loadPath); the full-list-rendering regression guard waits for virtualization (CPE-690).

## Deferred
Landed the safe **measure-first** part: dev-gated `[perf] first paint / settled` console marks in
`loadPath` (App.svelte), free in production (tree-shaken). Deferred the rest — a scripted multi-size
(100/1k/10k/50k) benchmark and a **regression budget test** — until **CPE-690 (virtualization)** lands, so
the budget guards the windowed renderer rather than the current full-list one, and the before/after 10×
is measured against the real change. deferred-on: CPE-690. revisit-when: virtualization is in.
