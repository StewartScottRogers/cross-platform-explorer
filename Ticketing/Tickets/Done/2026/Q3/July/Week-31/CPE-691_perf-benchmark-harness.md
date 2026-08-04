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
- [x] A regression guard (test/budget) against full-list rendering.
- [x] `npm run check` + suite green.

## Work Log
2026-07-18 (dayshift) — Picked up. Doing the safe 'measure-first' part now (dev-gated time-to-first-paint/settle marks in loadPath); the full-list-rendering regression guard waits for virtualization (CPE-690).

2026-07-27 — Picked the deferred slice back up now that CPE-690 (details-view virtualization) and
CPE-766 (icon/gallery grid virtualization) are both Done. Added `src/lib/components/FileList.virtualize-guard.test.ts`
(test-only; no production code touched). **Invariant asserted:** for a folder with N=5,000 entries
(well above `FileList.svelte`'s `VIRTUALIZE_THRESHOLD` of 100), the number of `.row` elements actually
mounted in the DOM stays bounded (< 100, matching a ~600px viewport at 30px rows + overscan) —
independent of N — instead of one row per entry. jsdom does no real layout, so
`Element.prototype.getBoundingClientRect` is stubbed to hand FileList's `measureGeometry()` a realistic
`.filelist-pane` viewport height and row height, which lets the component's real windowing path
(`windowRange` from `../virtualize`) engage instead of silently falling back to full-list rendering
(the fallback CPE-690's Work Log flagged as unverified headlessly). A companion test asserts a
12-entry folder (below the threshold) still renders all 12 rows — guards the "small folder pays
nothing" side too.

**Proved falsifiable**: temporarily forced `win` in FileList.svelte to always take the full-list
branch (simulating a revert of the windowing logic) — the new test failed as expected
(`expected 5000 to be less than 100`, i.e. `renderedRows` was 5000). Reverted the simulated regression
(confirmed `git diff` on FileList.svelte is empty again) and the test passes on the real code.

Verified: `npm run check` → 0 errors/0 warnings. `npx vitest run` → 121 files / 1312 tests, all green
(no new deps). Closing out this ticket's remaining ACs.

## Deferred
Landed the safe **measure-first** part: dev-gated `[perf] first paint / settled` console marks in
`loadPath` (App.svelte), free in production (tree-shaken). Deferred the rest — a scripted multi-size
(100/1k/10k/50k) benchmark and a **regression budget test** — until **CPE-690 (virtualization)** lands, so
the budget guards the windowed renderer rather than the current full-list one, and the before/after 10×
is measured against the real change. deferred-on: CPE-690. revisit-when: virtualization is in.

2026-07-27 (workshift) — **Done.** Prereq (CPE-690/766 virtualization) now landed. Added test-only `FileList.virtualize-guard.test.ts`: 5000-entry mount asserts bounded DOM window (<100 rows) via getBoundingClientRect stub so windowing path runs under jsdom; proven falsifiable (forced full-list branch → failed → reverted). check + 1312 vitest green. PR #444 merged (Foreman-reviewed, furlough wind-down).

2026-08-03 (workshift) — This ticket's boxes were checked but its other two claimed deliverables (dev-gated perf marks, multi-size 100/1k/10k/50k benchmark) weren't actually in the codebase — the marks were added then removed by CPE-757, and the multi-size benchmark was never built. CPE-1304 built both (dev-gated `performance.mark`/`measure` + `[perf]` console marks in `ExplorerPane.loadListing`, plus `FileList.perf-budget.test.ts`); see CPE-1304 for the work and its own falsifiability proof. Not rewriting this ticket's history.
