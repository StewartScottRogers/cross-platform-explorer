---
title: "How to build the full CPE-731 cost dashboard (fuller per-session metrics + persisted cross-session history)?"
date: 2026-07-26
tags: [cost-dashboard, agent-metrics, persistence, audit-journal, metrics-journal, cpe-731, cpe-733, session-metrics]
status: current
---

## Decision (user, 2026-07-26): full ledger + persisted cross-session history.

## Key insight: the fuller per-session metrics need NO new sidecar capture
Everything the DoD wants (beyond tokens+cost, already live via CPE-1097/1098) is a **frontend JOIN on
`sessionId`** of streams we already receive:
| Metric | Source | Derivation |
|---|---|---|
| tokens/cost | `agentCost` (agentCost.ts:27) | direct, keyed by sessionId |
| files-touched | `agentDiffs`/`agentTimeline` | distinct write paths where `actor==sessionId` |
| edit-count | `agentTimeline`/`agentDiffs` | count writes for actor |
| churn-bytes | `agentDiffs` FsDiff{before,after} | Σ\|after.len−before.len\| |
| wall-clock | session `started`/`ended` announcements (stamp arrival `Date.now()`) | endedAt−startedAt |
| throughput | derived | mirror `efficiency.rs:11-46` ratios, division-safe |
`TimelineEntry.actor` and `FsDiff.actor` == sessionId (CPE-1101); `agentCost` keyed by sessionId → clean join.
The unwired `session_metrics.rs`/`RunRecord`/`fleet_metrics.rs` stay unwired (no live RunRecord capture
possible — PTY subprocesses); **mirror their FORMULAS in TS**, don't wire them.

## Two caveats shaping the design
1. **Wall-clock**: `AgentSession` has no timestamps (sidecar.ts:88-95) — stamp arrival time frontend-side on
   `started`/`ended` folds. Timeline min/max `at` undercounts (misses think time).
2. **Caps truncate**: `agentTimeline` cap 300, `agentDiffs` 200/latest-per-path → end-of-session re-scan
   UNDERCOUNTS. Fix: a **running per-session accumulator** folded from each fs-diff/session batch as it
   arrives (cap-immune) — single source for live panel + persisted row.

## Persistence: reuse CPE-733's audit-journal PATTERN (not its file)
CPE-733 exists: `crates/server/src/audit_journal.rs` (record/list/read/trim, JSONL one-file-per-session under
app_data_dir/audit) + thin spawn_blocking commands `audit_record/audit_sessions/audit_read`
(src-tauri/src/lib.rs:2363-2416) + bindings. **Mirror it** as a sibling `metrics_journal.rs` — grain mismatch
(audit=many events/session, metrics=one row/session) means a SEPARATE `agent-metrics/history.jsonl`, NOT the
audit file. New `SessionMetricsRecord {sessionId,agentId,agentName,provider,model,cwd,startedAt,endedAt,
wallClockMs,input/output/totalTokens,costUsd,filesTouched,churnBytes,editCount}`. Commands `metrics_record`/
`metrics_history`; regen bindings.

## Flush seam
Live accumulator `agentSessionMetrics.ts` folds by actor/sessionId inside existing `ingestDiff`/session paths
(no new listener — preserve single-shared-listener invariant App.svelte:832-841). Flush = build record +
`commands.metricsRecord(rec)` at `reconcileAgentWatch → stopAgentWatch(id)` (App.svelte:825-829) and the
full-stop `clearAgentSessions` loop — BEFORE stores clear (per-session stop doesn't clear agentCost; only full
teardown does, agentCost.ts:100).

## Slicing
- **CPE-731a** (frontend, no backend, LOWEST RISK, ship first): `agentSessionMetrics.ts` accumulator + extend
  the Cost tab (AgentTimeline.svelte:372-393) with files/churn/wall-clock/throughput rows. Delivers the
  per-session half of the DoD. Pure/testable.
- **CPE-731b** (HIGHEST RISK): `metrics_journal.rs` (copy audit_journal verbatim) + commands + bindings + flush
  wiring. De-risk: unit-test the derive+build-record purely first; accumulator avoids cap truncation; flush
  before clear.
- **CPE-731c**: `agentMetricsRollup.ts` (mirror fleet_metrics::aggregate + efficiency) + cross-session
  dashboard component (dataviz skill) reading `metrics_history()`.
Order a→b→c.

## Coordination with CPE-728 (FLAG)
CPE-728/733 own the per-EVENT journal (app_data_dir/audit). Do NOT fold metrics into it (grain mismatch) —
sibling `metrics_journal` + `agent-metrics/` dir under the same app-data root. Share the PATTERN, not storage.
`SessionMetricsRecord` identity fields (agentId/model/cwd/started/ended) overlap CPE-728 replay's needs —
**agree the identity schema once** across both.

## Off-means-off / no-new-deps / advisory
Cost added only in (i) a pure fold over already-received batches (no new listener) + (ii) one metricsRecord IPC
at session end; dashboard reads history only when opened. Zero when idle (armedWatches empty → listeners torn
down, stores cleared, no session-end → no write). No new deps (svelte/writable, tauri event, serde/specta,
hand-written TS rollup). costUsd stays best-effort/advisory/not-billing; churn/files/wall-clock are approximations.
