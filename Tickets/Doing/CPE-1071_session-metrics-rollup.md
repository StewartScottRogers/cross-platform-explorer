---
id: CPE-1071
title: "Session metrics rollup — ai_console::session_metrics (per-session ledger row)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-731
estimate: 2-3h
---

## Summary
Child of CPE-731 (Agent cost & resource dashboard). The FOUNDATION row: fold a session's runs into one
multi-metric ledger entry (tokens, cost, wall-clock, files, churn, edits). **Pure fold** in the sidecar
`ai-console` crate, `cargo test` on the 3-OS Sidecar-platform CI — no GUI, no user resource, no new deps.
Builds on `usage::Usage` + `model_catalog::Pricing::estimate_cost`; does NOT modify them.

## Design (buildable)
New module `sidecar/ai-console/src/session_metrics.rs`, registered `pub mod session_metrics;` in
`sidecar/ai-console/src/lib.rs` **immediately after `pub mod cost;`**. Read `cost.rs` for the derive/BTreeMap
convention.

```rust
pub struct RunRecord {
    pub input_tokens: u64, pub output_tokens: u64, pub wall_clock_ms: u64,
    pub files_touched: u64, pub churn_bytes: u64, pub edit_count: u64, pub model_id: String,
}
#[derive(Debug, Clone, PartialEq, Default)]   // PartialEq-not-Eq: cost_usd is f64. NO serde/specta.
pub struct SessionMetrics {
    pub input_tokens: u64, pub output_tokens: u64, pub total_tokens: u64, pub cost_usd: f64,
    pub wall_clock_ms: u64, pub files_touched: u64, pub churn_bytes: u64, pub edit_count: u64,
}
pub fn fold_session(runs: &[RunRecord], pricing: &Pricing) -> SessionMetrics;
```
Saturating-sum every counter (`u64::saturating_add`); `total_tokens = input.saturating_add(output)`;
`cost_usd` derived from summed tokens via `pricing.estimate_cost(...)` (so it works even for agents that
print no cost line — treat `None` as 0.0, document it). `files_touched` is a caller-supplied count (if you
instead accept a `&[&str]` path slice, dedupe via `BTreeSet<String>` on EXACT strings — no `std::path`).

## ⚠ Arithmetic + derives
Every sum `saturating_add`. `SessionMetrics` derives `Debug, Clone, PartialEq, Default` (**PartialEq not Eq** —
it has an f64 field); NO serde/specta (mirror `cost.rs`'s `CostRollup`). No recursion.

## Acceptance Criteria
- [x] N runs sum correctly; two `u64::MAX` records → `u64::MAX` (saturate, no wrap/panic).
- [x] Empty session → all-zero `SessionMetrics::default()`; `total_tokens == input+output` (saturating).
- [x] `cost_usd` matches `estimate_cost` over the summed tokens (None → 0.0, documented).
- [x] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the CPE-731 foundation. Independent module in the
sidecar ai-console crate; distinct lib.rs anchor. CPE-1072/1074 depend on this module's `SessionMetrics`.

2026-07-25 (workshift, Worker) — Built end-to-end in worktree `agent-a25460a717a9f076a`, branch
`cpe-1071-session-metrics`. Added `sidecar/ai-console/src/session_metrics.rs` (`RunRecord`, `SessionMetrics`,
`fold_session`) and registered `pub mod session_metrics;` in `lib.rs` immediately after `pub mod cost;` per the
anchor instruction (note: the existing file already had `pub mod conflict_window;` right after `cost;`, not
alphabetical — inserted `session_metrics` between them, before `conflict_window`). Used
`crate::model_catalog::Pricing` and its `estimate_cost(&self, input_tokens: u64, output_tokens: u64) ->
Option<f64>`; `None` (no prompt/completion price at all) folds to `cost_usd = 0.0`, documented in the doc
comment. Every counter sum uses `u64::saturating_add`; `SessionMetrics` derives `Debug, Clone, PartialEq,
Default` only (no `Eq`, no serde/specta), mirroring `cost::CostRollup`. No `std::path`, no recursion, no
`#[cfg]`, no new dependencies. `files_touched` implemented as a caller-supplied `u64` count per the ticket's
primary option (no `&[&str]`/`BTreeSet` dedupe needed since callers already track a running count — flagged
as an assumption, not blocking).
6 new unit tests (N-run fold, two-`u64::MAX` saturation, empty→default, `total_tokens` invariant, cost match
against `estimate_cost`, `None`→0.0 fold) — all pass. Full crate `cargo test` green (no failures anywhere in
the suite). `cargo clippy --all-targets -- -D warnings` clean. Pushed branch, opened PR.
