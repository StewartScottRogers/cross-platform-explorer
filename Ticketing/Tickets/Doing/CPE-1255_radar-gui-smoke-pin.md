---
id: CPE-1255
title: "gui-smoke: pin the Agent Watch Radar tab render (CPE-1100)"
type: chore
component: Testing
priority: medium
status: Doing
tags: ready
created: 2026-08-02
epic: CPE-1148
---

## Summary
The Agent Watch drawer's **Radar tab** (`AgentTimeline.svelte`, CPE-1100 — activity-overlap signal: a path
touched by ≥2 *distinct actors* within `OVERLAP_WINDOW_MS` = 5000ms, folded from the live timeline by
`agentConflicts.ts#foldOverlaps`) is the **last live-IPC-fed Agent-Watch surface with no `gui-smoke` render
pin**. `MANUAL-TEST-BURNDOWN.md` row CPE-1100 reads "needs eyes + two concurrent actors racing a file on the
installed build; stays open." A read-only spike (2026-08-02) **overturned that** and confirmed it is
headlessly seedable, exactly like the sibling CPE-1173 cost-ledger pin did for its own "not seedable" claim.

## Spike findings (feasibility confirmed)
- The radar renders purely from `$: overlaps = foldOverlaps(entries)` — no new listener/timer.
- `AgentActivity`/`TimelineEntry` carry an optional `actor` field (CPE-1101). `ingestActivity(payload, now)`
  (`src/lib/agentActivity.ts:173`) threads `actor` through to each timeline entry (line 118).
- The **existing** test-mode hook `window.__CPE_TEST_INGEST_ACTIVITY__(payload, at?)` (`src/App.svelte:272`)
  folds a synthetic activity payload into the live `agentTimeline` store; its `at` param sets each batch's
  timestamp explicitly — so two batches can land distinct, ordered entries inside the 5s window instead of
  racing the same millisecond. **No new hook is needed** (unlike CPE-1173).
- Seeding two batches — **same path, two distinct `actor` values, timestamps < 5000ms apart** — makes
  `foldOverlaps` emit one overlap with `actors: [A, B]`, rendering `.rd-list` / `.rd-item` / `.rd-row` /
  `.rd-actors` / `.rd-pill` (AgentTimeline.svelte:963–985).

## Build
Add `gui-smoke/specs/radar.smoke.ts`, modelled on `gui-smoke/specs/cost-ledger.smoke.ts`:
1. Wait for the `[aria-current="page"]` breadcrumb (initial `--open=<tmpDir>` navigation settled).
2. Seed a synthetic "started" session anchored to `tmpDir` via `__CPE_TEST_INGEST_SESSION__` so the
   `.agent-log-btn` opener renders (same as cost-ledger.smoke.ts), open the drawer, click the **Radar** tab.
3. Seed two `__CPE_TEST_INGEST_ACTIVITY__` batches for one path under `tmpDir` with two distinct actors, the
   second `at` within 5000ms of the first.
4. Assert `.rd-list` exists and contains a `.rd-item` whose `.rd-actors` shows **two** `.rd-pill`s; `snap("radar")`.
5. Add a `snapFailure(this.currentTest, "radar")` afterEach (per CPE-1149), matching the sibling spec.

No production code change is expected (the hook already exists) — if the spec proves otherwise, keep any
change minimal and test-mode-gated.

## Acceptance criteria
- `gui-smoke/specs/radar.smoke.ts` drives the real `tauri build` binary to the Radar tab and asserts a
  non-degenerate overlap row (two actor pills) renders from the seeded two-actor same-path activity.
- Spec typechecks; `gui-smoke` `test:unit` (if any) + `npm run check` clean; no `.skip`/TODO left.
- Non-blocking CI leg (`continue-on-error`, CPE-1048 WebView2 caveat) — a smoke signal, not a hard gate,
  consistent with every other `gui-smoke` pin.
- `MANUAL-TEST-BURNDOWN.md` row CPE-1100 flipped to render-automated (feel residual), naming this pin.

## Notes
Sibling of CPE-1173 (cost-ledger) / CPE-1135 (replay) / CPE-1130 (cost-history). After this lands, every
live-IPC-fed Agent-Watch drawer tab has a render pin.
