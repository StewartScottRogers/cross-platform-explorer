---
id: CPE-1072
title: "Fleet aggregation — ai_console::fleet_metrics (cross-session / per-agent / per-model rollup)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-731
depends-on: CPE-1071
---

## Summary
Child of CPE-731 (Agent cost & resource dashboard). Aggregate many sessions' metrics into fleet totals +
per-agent + per-model breakdowns + averages. **Pure fold** in the sidecar `ai-console` crate, `cargo test` on
the 3-OS Sidecar-platform CI — no GUI, no user resource, no new deps. **Depends on CPE-1071** (consumes
`session_metrics::SessionMetrics`) — dispatch after CPE-1071 merges.

## Design (buildable)
New module `sidecar/ai-console/src/fleet_metrics.rs`, registered `pub mod fleet_metrics;` in
`sidecar/ai-console/src/lib.rs` at a distinct anchor (e.g. **immediately after `pub mod model_catalog;`**).
Reuse `session_metrics::SessionMetrics`; mirror `cost::rollup`'s `BTreeMap` shape for deterministic order.

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionAverages { pub avg_cost_usd: f64, pub avg_tokens: f64, pub avg_wall_clock_ms: f64,
                             pub avg_files_touched: f64, pub avg_churn_bytes: f64 }
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FleetRollup {
    pub totals: SessionMetrics,
    pub by_model: std::collections::BTreeMap<String, SessionMetrics>,
    pub by_agent: std::collections::BTreeMap<String, SessionMetrics>,
    pub session_count: usize,
    pub averages: SessionAverages,
}
pub fn aggregate(sessions: &[(String /*agent_id*/, String /*model_id*/, SessionMetrics)]) -> FleetRollup;
```
`totals` = saturating-sum of every session's metrics; `by_model`/`by_agent` = per-key saturating rollups
(BTreeMap → sorted, deterministic). `averages` are **division-safe** — `session_count == 0` → all-zero
averages, NEVER divide-by-zero (compute `sum as f64 / count as f64` only when count > 0).

## ⚠ Arithmetic + derives
Sums `saturating_add`; averages guarded (count 0 → 0.0). All compute structs `Debug, Clone, PartialEq, Default`
(f64 fields → not Eq); no serde/specta. No recursion.

## Acceptance Criteria
- [ ] Per-model and per-agent breakdowns split correctly; `totals` = saturating sum of all sessions; keys
      sorted (deterministic).
- [ ] 0-session aggregate → zero totals + zero averages (no div-by-zero); averages correct for a known set.
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-731 slice. Held in Backlog: depends on CPE-1071
(`SessionMetrics`) landing first.
