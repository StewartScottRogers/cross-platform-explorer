---
id: CPE-1114
title: "Cost dashboard: cross-session history rollup view"
type: feature
component: Frontend
priority: medium
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-731
---

## Summary
CPE-731 slice **c** (the final slice — closes the epic). Render the **cross-session history** persisted by
CPE-1113 (`commands.metricsHistory() -> SessionMetricsRecord[]`) as a rollup dashboard: totals, per-model +
per-agent breakdowns, averages/throughput ratios, and an over-time view. Frontend only (the persistence +
commands already exist). Design: `.claude/research-library/entries/cost-dashboard-full-ledger-history-plan.md`
(§3-4). Mirror the tested Rust rollup formulas (`fleet_metrics::aggregate`, `efficiency` ratios) in TS.

## Context (verified)
- `commands.metricsHistory()` (CPE-1113, merged) → `SessionMetricsRecord[]` (`{sessionId, agentId, agentName,
  provider, model, cwd, startedAt, endedAt, wallClockMs, inputTokens, outputTokens, totalTokens, costUsd,
  filesTouched, churnBytes, editCount}`, camelCase).
- `sidecar/ai-console/src/fleet_metrics.rs::aggregate` + `efficiency.rs` — the ROLLUP formulas to mirror in TS
  (totals, per-model, ratios). `src/lib/agentSessionMetrics.ts` — the per-session `SessionMetrics` shape + the
  human-format helpers (formatBytes/Duration) to reuse.
- The Agent-Watch drawer `AgentTimeline.svelte` has Live/Replay/Cost/Radar tabs — the cross-session history is
  session-independent, so a **new "History" tab** (or a dedicated view) that reads `metricsHistory()` on open
  fits (pull-only). Use judgment on the mount; keep TABS.md if a tab.

## Design (buildable)
1. **`src/lib/agentMetricsRollup.ts`** — PURE rollup over `SessionMetricsRecord[]`: `rollup(records)` →
   `{ totals (sessions, tokens, costUsd, wallClock, files, churn, edits), byModel: Map, byAgent: Map, averages,
   ratios (tokens/min, usd/session, usd/file, churn/1k-tok) }` mirroring `fleet_metrics::aggregate`/`efficiency`
   formulas; `overTime(records, bucket)` → day/hour buckets of cost+tokens for a sparkline. Division/empty/NaN-
   safe (0 records → zeroed, no NaN). Unit-tested (empty, single, multi-model, multi-agent, ratios 0-safe,
   over-time bucketing).
2. **History dashboard component/tab** — reads `commands.metricsHistory()` ONCE on open (pull-only, gen-token,
   error → empty state) into a store; renders: a totals summary (sessions / total cost / total tokens / total
   time / files / churn), a **per-model** table (sessions, tokens, cost, share), a **per-agent** table, and a
   simple **over-time** view (a sparkline/bars of cost or tokens per day — theme-var SVG, no chart dep). An
   empty state ("No session history yet"). Advisory / not-billing note. Theme vars only; tables/rows reflow/
   scroll; pills reflow.

## ⚠ Guardrails
- Pure frontend; no backend (commands exist). No new deps (hand-rolled SVG sparkline, not a chart lib). Theme
  vars only. Division/NaN/empty-safe everywhere. Advisory framing (best-effort, not billing). Off-means-off:
  `metricsHistory` is pull-only (read on open), no listener/timer — nothing runs when the view is closed.
- Mirror the Rust `fleet_metrics`/`efficiency` formulas so the numbers match the tested backend logic.

## Acceptance Criteria
- [ ] A cross-session History view reads `metricsHistory()` on open and shows totals + per-model + per-agent
      rollups + an over-time view; empty history → clean empty state; error → clean fallback; advisory note.
- [ ] `agentMetricsRollup.ts` rollup + over-time are pure + vitest-covered (empty/single/multi-model/multi-agent/
      0-safe ratios/bucketing) and match the Rust formula semantics.
- [ ] Pull-only (no listener/timer; nothing when closed); theme vars only; reflow/scroll; `npm run check` clean;
      `npm test` green; no new deps.

## Work Log
2026-07-26 (workshift) — CPE-731 slice c (final), from the filed plan. Rollup (mirror fleet_metrics/efficiency)
+ cross-session dashboard reading the CPE-1113 journal. Closes epic CPE-731 (fuller per-session metrics 731a +
persistence 731b + this history view).
