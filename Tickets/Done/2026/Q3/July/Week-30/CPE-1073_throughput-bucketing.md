---
id: CPE-1073
title: "Throughput time-series bucketing — ai_console::throughput (sparkline series)"
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
Child of CPE-731 (Agent cost & resource dashboard). Downsample timestamped runs into fixed time buckets for
a throughput sparkline (tokens/cost/files over time). **Pure flat pass** in the sidecar `ai-console` crate,
`cargo test` on the 3-OS Sidecar-platform CI — no GUI, no user resource, no new deps. Standalone (own record;
does not depend on the other CPE-731 slices).

## Design (buildable)
New module `sidecar/ai-console/src/throughput.rs`, registered `pub mod throughput;` in
`sidecar/ai-console/src/lib.rs` **immediately after `pub mod catalog;`**. Read `cost.rs` for derive convention.

```rust
pub struct TimedRun { pub start_ms: u64, pub tokens: u64, pub cost_usd: f64, pub files_touched: u64 }
#[derive(Debug, Clone, PartialEq, Default)]   // PartialEq-not-Eq (f64); NO serde/specta
pub struct Bucket { pub window_start_ms: u64, pub tokens: u64, pub cost_usd: f64, pub files_touched: u64 }
pub fn bucketize(runs: &[TimedRun], origin_ms: u64, bucket_ms: u64, max_buckets: usize) -> Vec<Bucket>;
```
Bucket index = `(start_ms.saturating_sub(origin_ms) / bucket_ms)`, **clamped to `max_buckets - 1`** (out-of-range
/ absurdly-late runs fall into the last bucket — never allocate unbounded). **Guard `bucket_ms == 0`** →
return empty (or a single bucket), NEVER divide-by-zero. **Cap the output vector length at `max_buckets`** so a
huge time span can't allocate unbounded. Per-bucket sums via `saturating_add`. Flat pass, no recursion.

## ⚠ Arithmetic + derives
`saturating_sub` for the offset, guarded division (`bucket_ms == 0`), `saturating_add` for bucket sums, output
capped at `max_buckets`. `Bucket` derives `Debug, Clone, PartialEq, Default` (f64 → not Eq); no serde/specta.

## ⚠ Cross-OS — integers + f64 only, no `std::path`, no `#[cfg]` assertion.

## Acceptance Criteria
- [ ] Runs land in the correct bucket by `(start - origin)/bucket_ms`; per-bucket sums correct + saturating.
- [ ] A `start_ms == u64::MAX` run clamps into the FINAL bucket (no panic/wrap); `bucket_ms == 0` handled (no
      div-by-zero); output length never exceeds `max_buckets`.
- [ ] Empty runs → empty; deterministic output.
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-731 slice. Standalone (own `TimedRun`); distinct
lib.rs anchor. Bounded (max_buckets) + saturating arithmetic per this session's reviewer-caught bugs.
