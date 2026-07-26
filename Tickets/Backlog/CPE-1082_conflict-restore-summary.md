---
id: CPE-1082
title: "Conflict-aware restore summary + safe-subset gate — cpe_server::revert_safety (summarize/plan_for)"
type: feature
component: Backend
priority: high
status: Backlog
tags: ready
created: 2026-07-25
epic: CPE-732
depends-on: CPE-1080
---

## Summary
Child of CPE-732 (Checkpoint & rollback). Package the 3-way classification into what a confirm dialog needs
and what the revert engine executes — a summary + a "revert only safe changes" vs "revert anyway" gate.
**Pure** in `crates/server`, `cargo test` on the 3-OS matrix — no GUI, no user resource, no new deps.
**Depends on CPE-1080** (consumes its `ClassifiedAction`) — dispatch after CPE-1080 merges. EXTENDS
`revert_safety.rs` (adds functions; the module already exists from CPE-1080).

## Design (buildable)
Add to `crates/server/src/revert_safety.rs` (from CPE-1080) — no new `pub mod` line needed.

```rust
#[derive(Debug, Clone, PartialEq, Default)]   // if it carries a conflict-% f64 → PartialEq NOT Eq
pub struct ConflictReport {
    pub safe: usize, pub conflicts: usize,
    pub conflict_paths: Vec<String>,
    // optional: pub conflict_ratio: Option<f64>,   // conflicts / (safe+conflicts) — None if 0
}
pub fn summarize_conflicts(classified: &[ClassifiedAction]) -> ConflictReport;
/// The plan to actually execute: include_conflicts=false → only Safe actions (default "revert safe only");
/// true → all actions ("revert anyway").
pub fn plan_for(classified: &[ClassifiedAction], include_conflicts: bool) -> Vec<RestoreAction>;
```
If you include a `conflict_ratio`, it MUST be **division-safe**: denominator 0 → `None`/0.0, never NaN/inf
(and then `ConflictReport` is `PartialEq` not `Eq`). Deterministic ordering. No recursion.

## Acceptance Criteria
- [ ] `summarize_conflicts` counts safe/conflicts + collects conflict paths correctly (deterministic order).
- [ ] `plan_for(false)` drops conflict actions (safe subset only); `plan_for(true)` keeps ALL actions.
- [ ] Empty / no-conflict cases handled; any ratio is division-safe (no NaN/inf).
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean default AND
      `--features index`; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the CPE-732 report/gate slice. Held in Backlog:
depends on CPE-1080 (`ClassifiedAction`) landing first; extends its `revert_safety.rs`.
