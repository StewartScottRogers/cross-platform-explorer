---
id: CPE-1107
title: "Cost dashboard: fuller per-session metrics (files / churn / wall-clock / throughput)"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-731
---

## Summary
CPE-731 slice **a** (frontend, no backend — lowest risk, ship first). Extend the live per-session cost view
(CPE-1098) beyond tokens+cost to the fuller DoD metric set — **files-touched, edit-count, churn-bytes,
wall-clock, throughput ratios** — all derived by JOINing streams we ALREADY receive on `sessionId`. No new
sidecar capture. Design + rationale: `.claude/research-library/entries/cost-dashboard-full-ledger-history-plan.md`
(READ IT). Persistence + cross-session history are later slices (731b/c).

## Context (verified — file:line)
- `src/lib/agentCost.ts` — `agentCost: Record<sessionId, AgentCost{inputTokens,outputTokens,costUsd}>` (folds
  `ai-console://agent-cost`); `totalTokens()` (:68). Cost tab renders it in `AgentTimeline.svelte:372-393`.
- `src/lib/agentDiffs.ts` — `agentDiffs` FsDiff{path,before,after,actor} (`actor==sessionId`, CPE-1101);
  `ingestDiff` (:209); `diffLineStats` (:191). Churn = `Σ |after.len − before.len|`.
- `src/lib/agentActivity.ts` — `agentTimeline` TimelineEntry{kind,path,actor,at} (cap 300, :31). Files-touched
  = distinct write paths where `actor==sessionId`; wall-clock (approx) = `max(at)−min(at)` for that actor.
- `src/lib/sidecar.ts` — `session` announcements (`started`/`ended`, :139-144) — stamp arrival `Date.now()`
  for a better wall-clock than timeline min/max.
- **Caps truncate** (timeline 300 / diffs 200-latest-per-path) → do NOT re-scan stores at read; maintain a
  **running per-session accumulator** folded from each batch as it arrives (cap-immune).

## Design (buildable)
1. **`src/lib/agentSessionMetrics.ts`** — a store `agentSessionMetrics: Record<sessionId, SessionMetrics>`
   with a **running accumulator** folded inside the existing ingest paths (NO new listener — hook into the
   same `ingestDiff`/session-fold the app already runs, preserving the single-shared-listener invariant):
   - on `session` `started` → stamp `startedAt=Date.now()`, capture identity (agentId/name/provider/model/cwd
     from the announcement) if available.
   - on each `fs-diff` batch item → by `actor`: add path to a `filesTouched` set, `editCount++`, `churnBytes
     += |after.len−before.len|`.
   - `endedAt` stamped on `ended`; `wallClockMs = endedAt−startedAt` (or now−startedAt while live).
   - expose a derived per-session `SessionMetrics{sessionId, inputTokens,outputTokens,totalTokens,costUsd (from
     agentCost), filesTouched (count), editCount, churnBytes, wallClockMs, + throughput ratios}`. Ratios
     (tokens/min, usd/file, churn/1k-tokens) **division-safe** (0 denom → hidden/None, mirror `efficiency.rs`
     formulas). `clear()` on stop, gated exactly like `agentCost`/`agentActivity` (off-means-off).
2. **Extend the Cost tab** (`AgentTimeline.svelte:372-393`) — add rows/cards for files-touched, edit-count,
   churn (human bytes), wall-clock (human duration), and the throughput ratios, per session. Keep the
   **advisory / best-effort / not-billing** framing (churn/files/wall-clock are approximations). Theme vars
   only; chip rows reflow.

## ⚠ Guardrails
- Pure frontend; NO backend; NO new deps; NO new listener/timer (fold inside existing ingest paths). Theme
  vars only. Division/NaN-safe on every ratio + human-format helper. Off-means-off: accumulator empty + no
  cost when not watching (mirror agentCost gating).
- Do NOT add persistence here (that's 731b) — this is the live per-session panel only.
- Keep the SessionMetrics shape aligned with what CPE-731b will persist + what CPE-728 identity uses (agentId/
  model/cwd/started/ended) — see the Library plan's "agree identity schema once" note.

## Acceptance Criteria
- [ ] The Cost tab shows, per watched session: tokens (in/out/total), cost, files-touched, edit-count, churn,
      wall-clock, and throughput ratios — updating live; advisory note present; all ratios NaN/0-safe.
- [ ] Metrics come from a cap-immune running accumulator folded from existing batches (not an end-scan of the
      capped stores); nothing runs / accumulator empty when not watching.
- [ ] `npm run check` clean; vitest green (accumulator fold + ratio/format helpers tested: empty, single, many,
      0-denominator, churn on repeated edits); no new deps; theme vars only.

## Work Log
2026-07-26 (workshift) — CPE-731 slice a, from the filed plan. User chose full ledger + history; this delivers
the fuller per-session live panel with NO new capture (all derivable from existing streams). 731b (sibling
metrics_journal persistence + flush-on-end) and 731c (cross-session rollup dashboard) follow.
