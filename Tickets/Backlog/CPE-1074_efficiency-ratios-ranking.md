---
id: CPE-1074
title: "Efficiency ratios + ranking — ai_console::efficiency (cost-per-progress)"
type: feature
component: Backend
priority: medium
status: Backlog
tags: ready
created: 2026-07-25
epic: CPE-731
depends-on: CPE-1071
---

## Summary
Child of CPE-731 (Agent cost & resource dashboard). Division-safe efficiency ratios over a session's metrics
(spend vs. files/churn/tokens/time) + a deterministic ranking of agents/models. **Pure** in the sidecar
`ai-console` crate, `cargo test` on the 3-OS Sidecar-platform CI — no GUI, no user resource, no new deps.
**Depends on CPE-1071** (consumes `session_metrics::SessionMetrics`) — dispatch after CPE-1071 merges.

## Design (buildable)
New module `sidecar/ai-console/src/efficiency.rs`, registered `pub mod efficiency;` in
`sidecar/ai-console/src/lib.rs` at a distinct anchor (e.g. **immediately after `pub mod usage;`**). Reuse
`session_metrics::SessionMetrics`.

```rust
pub fn usd_per_file(m: &SessionMetrics) -> Option<f64>;         // None if files_touched == 0
pub fn tokens_per_file(m: &SessionMetrics) -> Option<f64>;      // None if files_touched == 0
pub fn churn_per_1k_tokens(m: &SessionMetrics) -> Option<f64>;  // None if total_tokens == 0
pub fn tokens_per_minute(m: &SessionMetrics) -> Option<f64>;    // None if wall_clock_ms == 0

pub enum EffMetric { UsdPerFile, TokensPerFile, ChurnPer1kTokens, TokensPerMinute }
pub fn rank_by(entries: &[(String /*id*/, SessionMetrics)], metric: EffMetric) -> Vec<(String, f64)>;
```
Every ratio returns `None` when its denominator is 0 — **never inf/NaN**. `rank_by` orders entries by the
chosen metric (document asc/desc), with a **deterministic id tie-break** on equal values (and a documented
placement for entries whose ratio is `None` — e.g. sort last). Convert `u64` to f64 carefully so huge inputs
don't produce a nonsense rank (they're finite after f64 cast — fine, but document).

## ⚠ Arithmetic + derives
Division-safe (denominator 0 → `None`, never inf/NaN). `EffMetric` may derive `Debug, Clone, Copy, PartialEq,
Eq` (no f64); no serde/specta. No recursion.

## Acceptance Criteria
- [ ] Zero-files → `usd_per_file`/`tokens_per_file` = `None` (not inf/NaN); zero-tokens/zero-time similarly.
- [ ] A known ratio computes exactly; ranking is deterministic INCLUDING equal-value ties (id-ordered) and a
      documented placement for `None`-ratio entries.
- [ ] Huge/`u64::MAX` inputs don't overflow into a nonsense rank; empty entries → empty.
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-731 slice. Held in Backlog: depends on CPE-1071
(`SessionMetrics`) landing first.
