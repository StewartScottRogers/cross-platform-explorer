---
id: CPE-1078
title: "Gate audit summary — ai_console::gate_audit (decision-ledger fold + rates)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-25
epic: CPE-729
estimate: 2-3h
---

## Summary
Child of CPE-729 (Intervene & approve — pure policy core). A pure fold over the gate-decision history —
per-resolution counts, per-rule rollup, and division-safe rates — so every intervene/approve decision is
auditable. **Pure** in the sidecar `ai-console` crate, `cargo test` on the 3-OS Sidecar-platform CI — no GUI,
no user resource, no new deps. **Independent** of the A/B/C chain (defines its own domain enum) — dispatch in
the first wave alongside CPE-1075.

## Design (buildable)
New module `sidecar/ai-console/src/gate_audit.rs`, registered `pub mod gate_audit;` in
`sidecar/ai-console/src/lib.rs` **immediately after `pub mod swarm_locks;`**. Mirror the
`conflict_region`/`cost` rollup style (BTreeMap for deterministic order).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution { Approved, Rejected, EditedScope, AutoAllowed, AutoBlocked }   // recorded outcome
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateRecord { pub seq: u64, pub path: String, pub resolution: Resolution, pub rule: Option<String> }
#[derive(Debug, Clone, PartialEq, Default)]   // approval_rate is f64 → PartialEq NOT Eq; NO serde/specta
pub struct AuditSummary {
    pub total: u64,
    pub by_resolution: std::collections::BTreeMap<String, u64>,   // or per-variant counts
    pub by_rule: std::collections::BTreeMap<String, u64>,
    pub approval_rate: Option<f64>,   // approved / (approved + rejected); None if denominator 0
}
pub fn summarize_audit(records: &[GateRecord]) -> AuditSummary;
```
Per-resolution counts + per-rule rollup (BTreeMap → sorted, deterministic). `approval_rate` =
`approved as f64 / (approved + rejected) as f64` — **None when denominator 0**, never NaN/inf.

## ⚠ Arithmetic + derives
Every counter `u64::saturating_add`. `approval_rate` division-safe (denominator 0 → None). `AuditSummary`
derives `Debug, Clone, PartialEq, Default` (**PartialEq NOT Eq** — f64 field); no serde/specta. No recursion.

## ⚠ Cross-OS — strings/integers/BTreeMap only; no `std::path`, no `#[cfg]`.

## Acceptance Criteria
- [ ] Per-resolution counts + per-rule rollup correct and deterministically ordered (BTreeMap).
- [ ] `approval_rate == None` when approved+rejected == 0 (no NaN/inf); correct rate for a known set.
- [ ] Counters saturate at `u64::MAX` (no wrap/panic); empty input → all-zero/None summary.
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as an independent CPE-729 pure-core slice (own
`Resolution` enum, so independent of A/B/C). Distinct lib.rs anchor. Dispatched in the first wave alongside
CPE-1075.

2026-07-25 (workshift, Worker) — Implemented `sidecar/ai-console/src/gate_audit.rs` exactly to the design:
`Resolution` (Copy + Eq, 5 variants), `GateRecord` (Eq), `AuditSummary` (`PartialEq, Default`, no `Eq`
because of the `f64` `approval_rate` field), `summarize_audit(&[GateRecord]) -> AuditSummary`. Registered
`pub mod gate_audit;` immediately after `pub mod swarm_locks;` in `lib.rs` (line 79/80). No serde/specta, no
`std::path`, no `#[cfg]`, no recursion, no new deps — matches the `cost.rs`/`conflict_region.rs` rollup
convention (BTreeMap for deterministic order).
- **0-denominator approval_rate**: computed from a running `approved`/`rejected` tally (not from
  `by_resolution`, to avoid a second map lookup); `denom = approved.saturating_add(rejected)`; `None` when
  `denom == 0`, else `Some(approved as f64 / denom as f64)`. `EditedScope`/`AutoAllowed`/`AutoBlocked` never
  enter the rate's numerator or denominator (only `Approved`/`Rejected` do) — this is an assumption the
  design left implicit ("approved / (approved+rejected)" only mentions those two resolutions), confirmed by
  a dedicated test (`approval_rate_none_when_no_approved_or_rejected`, using only the other three variants).
- **Saturation**: every counter (`total`, each `by_resolution` entry, each `by_rule` entry, and the internal
  `approved`/`rejected` tallies) increments via `u64::saturating_add`, so a `u64::MAX`-sized history
  saturates instead of wrapping/panicking; covered by
  `counters_saturate_at_u64_max_without_panicking`.
- **`by_rule`**: records with `rule: None` contribute to `by_resolution`/`total`/the approval tally but add
  no `by_rule` entry (there's nothing to key on) — covered by
  `per_rule_rollup_correct_and_ordered_and_skips_none`.
- 8 new unit tests, all green: empty input, per-resolution counts + BTreeMap ordering, per-rule rollup +
  ordering + None-skip, rate-is-None on empty denominator, a known-ratio rate (0.75), a never-NaN/inf check
  (1 rejection only → rate 0.0, finite), saturation, and a mixed end-to-end case.
- Verify (from `sidecar/ai-console`): `cargo test --lib` → **389 passed, 0 failed, 2 ignored** (whole crate,
  no regressions; `gate_audit::` subset is 8/8). `cargo clippy --all-targets -- -D warnings` → clean, no
  warnings. No new dependencies added to `Cargo.toml`.
- No blockers. PR opened from branch `cpe-1078-gate-audit`.
