---
id: CPE-1074
title: "Efficiency ratios + ranking — ai_console::efficiency (cost-per-progress)"
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
- [x] Zero-files → `usd_per_file`/`tokens_per_file` = `None` (not inf/NaN); zero-tokens/zero-time similarly.
- [x] A known ratio computes exactly; ranking is deterministic INCLUDING equal-value ties (id-ordered) and a
      documented placement for `None`-ratio entries.
- [x] Huge/`u64::MAX` inputs don't overflow into a nonsense rank; empty entries → empty.
- [x] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (sprint) — Filed by the Product Manager as a CPE-731 slice. Held in Backlog: depends on CPE-1071
(`SessionMetrics`) landing first.

2026-07-25 (sprint, Worker) — Built end-to-end in worktree `agent-af6dac4b026049a28`, branch
`cpe-1074-efficiency`, off latest `main` (CPE-1071 `session_metrics` already merged). Added
`sidecar/ai-console/src/efficiency.rs` with the four division-safe ratio functions
(`usd_per_file`, `tokens_per_file`, `churn_per_1k_tokens`, `tokens_per_minute`, each `-> Option<f64>`,
`None` exactly when their denominator is 0 — never inf/NaN), `EffMetric` enum (`Debug, Clone, Copy,
PartialEq, Eq` — no f64 field, no serde/specta), and `rank_by(entries, metric) -> Vec<(String, f64)>`.
Registered `pub mod efficiency;` in `lib.rs` immediately after `pub mod usage;` per the anchor
instruction.

**Assumption (ranking order, logged per instructions since the user is asleep):** `rank_by` sorts
**ascending** by the raw ratio value (lowest first). For the three cost-shaped ratios (`UsdPerFile`,
`TokensPerFile`, `ChurnPer1kTokens`) that reads as "cheapest/most-efficient first". `TokensPerMinute`
is a throughput rate, not a cost, so ascending there is slowest-first; documented in the `rank_by`
doc comment that a caller wanting "fastest first" for that one metric should reverse the returned
`Vec` rather than `rank_by` special-casing direction per metric — kept the ordering rule to one
comparator for simplicity/determinism. Tie-break on equal ratio values is `id` ascending
(lexicographic `str::cmp`), applied identically regardless of input order. Entries whose ratio is
`None` sort **last**, after every entry with a real ratio; if every entry is `None` the whole result
falls back to plain id order. `rank_by` reports `0.0` as the paired value for `None` entries (the
`Vec<(String, f64)>` return shape has no room for an `Option`) — the stable last-place position is
what actually signals "no ratio", not the placeholder `0.0`; documented explicitly in the doc comment
so callers don't misread it as a real zero ratio.

11 new unit tests: zero-files → None for both file ratios, zero-tokens → None for churn ratio,
zero-wall-clock → None for throughput, a known-value ratio computed exactly for all four metrics,
ascending-order ranking, deterministic tie-break on equal values (including a reordered-input
regression check), `None`-entries-sort-last (mixed with real ratios), all-`None`-falls-back-to-id-order,
`u64::MAX`/`f64::MAX` inputs stay finite across all four metrics, and empty entries → empty. Full crate
`cargo test`: 366 passed, 2 ignored (pre-existing, unrelated), 0 failed. `cargo clippy --all-targets --
-D warnings`: clean. No new dependencies (`Cargo.toml`/`Cargo.lock` untouched — only
`efficiency.rs` added and one `pub mod` line in `lib.rs`). Pushed branch, opened PR.
