---
id: CPE-1072
title: "Fleet aggregation — ai_console::fleet_metrics (cross-session / per-agent / per-model rollup)"
type: feature
component: Backend
priority: medium
status: Done
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
2026-07-25 (sprint) — Filed by the Product Manager as a CPE-731 slice. Held in Backlog: depends on CPE-1071
(`SessionMetrics`) landing first.

2026-07-25 (sprint, Worker, overnight) — Built. CPE-1071 (`session_metrics::SessionMetrics`) had already
merged to `main` by the time this was picked up; branched `cpe-1072-fleet-metrics` off latest `main`. Added
`sidecar/ai-console/src/fleet_metrics.rs` with `SessionAverages`, `FleetRollup`, a private
`add_saturating(a, b) -> SessionMetrics` helper (every integer field via `u64::saturating_add`, `cost_usd` via
plain `f64 +=`), and `aggregate(sessions: &[(String, String, SessionMetrics)]) -> FleetRollup`. Registered
`pub mod fleet_metrics;` in `lib.rs` immediately after `pub mod model_catalog;` as directed — auto-merged
cleanly alongside CPE-1073's unrelated `pub mod throughput;` addition that had landed on `main` in the
meantime.

Division-safe averages: `session_count == 0` short-circuits to `SessionAverages::default()` (all `0.0`)
before any division is attempted — the `sum / count as f64` path only runs inside the `else` branch where
`count > 0`, so there is no div-by-zero/NaN/inf path at all, not even a guarded one.

Assumption: the design's `SessionMetrics` has no `Eq`/`Ord`, and `by_model`/`by_agent` values are whole
`SessionMetrics` structs (not scalars) per the ticket's struct sketch — summed with the same
`add_saturating` helper as `totals`, keyed by the tuple's `model_id`/`agent_id` strings respectively.

7 new tests: 0-session (zero totals + zero averages, asserted field-by-field), saturating totals across
mixed agents/models, per-model + per-agent split correctness + sorted-key order, a known-averages case,
u64::MAX + u64::MAX saturation without panic (also fixed the test helper itself, which initially overflowed
computing `total_tokens` with plain `+` in debug mode — switched it to `saturating_add`), and a single-session
identity check.

Verify (from `sidecar/ai-console`): `cargo test` → 371 passed, 0 failed, 2 ignored (up from 362 pre-change +
7 new + CPE-1073's tests that landed concurrently). `cargo clippy --all-targets -- -D warnings` → clean, no
new deps added. Pushed `cpe-1072-fleet-metrics`, opened the PR.
