---
id: CPE-1104
title: "Activity replay: variable playback speed"
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-26
epic: CPE-728
---

## Summary
Fills a small Definition-of-Done gap in epic CPE-728 (Activity replay & scrub): the DoD says events replay
"with variable speed", but the shipped scrubber (CPE-1094) plays at a fixed ~400ms/entry cadence. Add a
speed control (e.g. 0.5× / 1× / 2× / 4×) to the Replay tab. Pure frontend. (The two LARGER CPE-728 gaps —
folder-view state reconstruction at time T, and persisting the event log beyond the 300-cap — are separate and
need a design decision; not this ticket.)

## Context (verified)
- `src/lib/components/AgentTimeline.svelte` — the Replay tab (from CPE-1094) has play/pause/step/jump transport
  driven by a `setInterval` at a fixed cadence constant (~400ms). `src/lib/agentReplay.ts` holds the pure
  helpers.
- Off-means-off + interval-cleanup rules from CPE-1094 must be preserved (the interval is created only on
  user-initiated play and cleared on pause/end/unmount/watch-off).

## Design (buildable)
1. A small **speed selector** in the Replay transport (a segmented control or dropdown: 0.5×, 1×, 2×, 4× —
   reflowing/theme-vars). Store the chosen multiplier in component state (default 1×).
2. The play interval's period = `BASE_CADENCE_MS / speed` (base = the current ~400ms). Changing speed while
   playing restarts the interval with the new period (clear + re-create — no leak; reuse the existing
   stop/start). Division-safe (speed is always a positive constant from the fixed set — no user-typed 0).
3. Preserve all CPE-1094 invariants: interval cleared on pause/end/unmount/watch-off/timeline-clear; nothing
   runs when not watching; play still self-stops at the last entry.

## ⚠ Notes / guardrails
- Pure frontend; no backend; no new deps. Theme vars only; the speed control reflows if it's a pill row.
- Do NOT change the event/store model — this is transport-only. Keep the pure helpers pure.
- If you extract a helper (e.g. `cadenceForSpeed(base, speed)`), unit-test it (positive speeds, default).

## Acceptance Criteria
- [ ] The Replay tab has a working speed selector; playback advances faster/slower accordingly; changing speed
      mid-play takes effect without leaking an interval.
- [ ] All CPE-1094 invariants hold (self-stops at end; interval cleared on pause/unmount/watch-off; zero cost
      when not watching).
- [ ] `npm run check` clean; vitest green (incl. any new helper test); no new deps; theme vars only.

## Work Log
2026-07-26 (workshift) — Filed to fill the CPE-728 variable-speed DoD gap (from the PM epic-closure
assessment). The bigger CPE-728 gaps (folder-state reconstruction, event-log persistence) remain open pending
a design decision.
