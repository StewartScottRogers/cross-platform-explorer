---
id: CPE-1111
title: "Activity replay: reconstructed folder listing in the Replay tab"
type: feature
component: Frontend
priority: medium
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-728
---

## Summary
CPE-728 slice **d** — the visible payoff. Wire the `replay_load` command (CPE-1110) + the pure `replayFold.ts`
reconstruction into the Replay tab so scrubbing shows the **folder listing as it looked at time T** (including
pre-existing files), not just a frozen event list. In-drawer (a read-only list in the Replay tab); the
file-pane overlay is the later slice 728e. Design:
`.claude/research-library/entries/activity-replay-event-reconstruction-plan.md` (§3).

## Context (verified — file:line)
- `src/lib/components/AgentTimeline.svelte` — the Replay tab (from CPE-1094/1104) with transport (`t`, play/
  pause/step/jump/slider, variable-speed) driving off in-memory `agentTimeline`. It shows a frozen event list +
  current-entry diff. `sessionId` is a prop.
- New bindings (CPE-1110, merged): `commands.replayLoad(session) -> { replay: ReplayData{events,bounds,summary},
  baseline: Baseline | null }`. `src/lib/replayFold.ts` — `stateAtFrom(baseline, events, tMs)` + `childrenAt(
  state, dir) -> ReplayEntry[]` (pure, tested vs the Rust oracle).
- `currentPath` (the watched folder / navigated dir) is available in the drawer's context (used by the outline/
  jump elsewhere) — the listing is `childrenAt(state, currentPath)`.

## Design (buildable)
1. **Load on Replay-tab enter** — when the Replay tab becomes active (and `sessionId` is set), call
   `commands.replayLoad(sessionId)` ONCE into local `replayData`/`replayBaseline`. Generation-token it so a
   session change supersedes a slow load. Pull-only — nothing runs while the tab is closed (off-means-off; no
   new listener/timer). Handle load error → fall back to the existing event-list view (never break the tab).
2. **Derive the reconstructed listing** reactively from the scrub time `t`:
   `$: replayListing = replayData ? childrenAt(stateAtFrom(replayBaseline, replayData.events, t), currentPath)
   : []`. This re-derives per tick with NO IPC (the fold is pure/local). Division/empty-safe.
3. **Render** the listing as a read-only row list in the Replay tab body: each `ReplayEntry` shows name +
   a kind indicator (created/modified/removed at this point — the entry carries `kind`), dirs vs files
   distinguished. A small header notes it's a **reconstruction at the scrub time** (read-only). Optionally show
   the folder breadcrumb (currentPath). Theme vars only; rows reflow / scroll.
4. **Compose with transport** — the slider/play/step/variable-speed (CPE-1104) already move `t`; the listing
   re-derives automatically. Keep the existing frozen-event-list + diff peek (or place the listing alongside/
   above it — pick the cleaner layout). Empty/no-events/pre-load → sensible empty state.

## ⚠ Guardrails
- Off-means-off: `replayLoad` called only on tab-enter (pull-only); no new listener/timer; nothing when the
  drawer/tab is closed. No new deps. Theme vars only. `replayFold.ts` stays pure (call it, don't modify it).
- Read-only: the reconstructed listing must not mutate any live store or navigation — it's a view of the past.
- Generation-token the load; error → fall back to the event-list, never break the Replay tab or the live view.

## Acceptance Criteria
- [ ] Entering the Replay tab loads the session's replay data once; scrubbing shows the reconstructed folder
      listing at the scrub time (pre-existing + event-derived entries), re-deriving per tick with no per-tick
      IPC; play/step/variable-speed drive it; empty/error states are clean (fall back to event list).
- [ ] Read-only (no live-store/navigation mutation); off-means-off (pull-only load, no new listener/timer,
      nothing when tab closed); theme vars only; reflow/scroll.
- [ ] `npm run check` clean; vitest green (any extracted pure helper tested; component test if the tab logic is
      exercised); no new deps.

## Work Log
2026-07-26 (workshift) — CPE-728 slice d, from the filed plan. Renders `childrenAt(stateAtFrom(baseline,
events, t), currentPath)` in the Replay tab. Slice 728e (read-only file-pane overlay) is the optional graduate.
After this, CPE-728's core DoD (folder view reconstructs state at any point) is met in-drawer.
