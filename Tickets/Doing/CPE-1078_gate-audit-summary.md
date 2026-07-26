---
id: CPE-1078
title: "Gate audit summary — ai_console::gate_audit (decision-ledger fold + rates)"
type: feature
component: Backend
priority: medium
status: Doing
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
