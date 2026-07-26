---
id: CPE-1094
title: "Agent Watch: replay scrubber over the session activity timeline"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-396
---

## Summary
GUI #3, slice 1 of 3 (the buildable-now one). Add a **replay scrubber** to Agent Watch: a timeline slider that
lets the user scrub back and forth through the watched agent's filesystem activity, seeing what was touched up
to a chosen moment and (where retained) the diff at that point. **Pure frontend** — the data already exists in
the `agentTimeline` + `agentDiffs` stores; no backend change. Design from the Library entry
`.claude/research-library/entries/agent-watch-dashboards-substrate.md` (read it).

## Context (verified — file:line)
- `src/lib/agentActivity.ts` — `agentTimeline` store: durable, newest-first, cap 300 (`TIMELINE_CAP`),
  entries `{id, kind, path, at}` with real epoch `at` — this IS the scrubbable event log. `clearActivity()`
  wipes on stop-watching. In-memory only (no cross-session/restart persistence — this is live-session replay).
- `src/lib/agentDiffs.ts` — `agentDiffs` store: **latest-only** before/after per path (`foldDiffs` overwrites
  on repeat edits, cap 200/4MB). So a file edited >1×/session only retains its newest diff.
- `src/lib/components/AgentTimeline.svelte` — the fixed right drawer (`{#if activeWatchCwd && showTimeline}`
  in `App.svelte:3456`), already holds the timeline list + `ConsultedFiles` + `DiffPeek`/`DiffSideBySide`.
  This is the mount point — grow it into a **tabbed** dashboard (e.g. tabs: "Live" | "Replay") using the
  standard TABS.md active-tab treatment, rather than a new fixed panel.
- Off-means-off gate: everything Agent-Watch is gated on `activeWatchCwd` being non-empty and listeners attach
  only inside `syncAgentWatch`'s `if (cwd)` branch (`App.svelte:790-809`). The scrubber must add ZERO cost
  when not watching — it only reads existing stores that are already empty when off.

## Design (buildable)
1. **Replay tab** in `AgentTimeline.svelte` (TABS.md treatment; default "Live" = today's list). The Replay tab
   renders a horizontal slider spanning `[firstAt, lastAt]` of `agentTimeline` (min→max `at`). Disabled/empty
   state when the timeline has <2 entries.
2. **Scrub position** = a selected timestamp `t` (slider value). Derive reactively:
   - the timeline **up to `t`** (entries with `at <= t`), rendered like the live list but frozen at `t`;
   - the **current entry** (the last entry `<= t`) highlighted, with its `path`/`kind`/time shown;
   - if that path has a retained diff in `agentDiffs`, show it via the existing `DiffPeek`/`DiffSideBySide`.
     If the path was edited more than once in the session (only latest retained), **badge it** ("content at
     this point not retained — showing latest") so the fidelity ceiling is honest, per the Library caveat.
3. **Transport controls** — play/pause (step through entries at a fixed cadence, e.g. one entry / 400ms, using
   a cancel-on-unmount interval), step-forward/back (prev/next entry), and jump-to-start/end. Play advances `t`
   through the entry timestamps; pause on reaching the end.
4. **Group by nothing fancy** — v1 is a linear replay of the event sequence; no reconstruction of full file
   content at time `t` (that needs a per-path diff history — explicitly out of scope, note as a follow-up).
5. **Reset** on entry/watch change (`clearActivity()` already empties the stores; the scrubber's local `t`/play
   state must reset when `agentTimeline` empties or `activeWatchCwd` changes — no dangling interval).

## ⚠ Notes / guardrails
- Pure frontend; no backend, no new deps. Theme vars only; any chip row reflows (tick-tacks). Tabs use the
  `.tab`/`.tab.active` convention (TABS.md).
- Division-safe slider math (single-entry or all-same-timestamp timeline → no NaN; `lastAt==firstAt` →
  degenerate slider handled).
- The play interval MUST be cleared on destroy / watch-off / timeline-clear (no leak, honours off-means-off).
- Badge multiply-edited files rather than silently showing a wrong-for-that-moment diff.

## Acceptance Criteria
- [ ] While watching an agent, the Agent Watch drawer has a Replay tab with a working scrubber over the
      session's activity; scrubbing shows the timeline frozen at the chosen moment + the current entry's diff
      when retained.
- [ ] Play/pause/step/jump controls work; play advances through entries and stops at the end; all intervals
      clear on unmount/watch-off (no leak, no cost when not watching).
- [ ] Files edited more than once are badged (latest-diff-only) rather than misrepresented; empty/single-entry
      timelines show a sane disabled state (no NaN slider).
- [ ] Tabs follow TABS.md; colours from theme vars; `npm run check` clean; vitest green (add tests for any
      pure helper, e.g. `entriesUpTo(timeline, t)` / slider-range math incl. degenerate cases); no new deps.

## Work Log
2026-07-26 (workshift, GUI) — Filed by the Foreman as GUI #3 slice 1 (the zero-backend one), from the Library
substrate brief. Slices 2 (cost ledger — needs a sidecar→host `ai-console://agent-cost` bridge) and 3
(conflict radar — needs multi-session AgentWatchState + actor tagging) are larger and will be cut just-in-time
from the same Library entry after this validates the tabbed-drawer mount.
