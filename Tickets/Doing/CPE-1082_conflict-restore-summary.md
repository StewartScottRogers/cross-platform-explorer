---
id: CPE-1082
title: "Conflict-aware restore summary + safe-subset gate — cpe_server::revert_safety (summarize/plan_for)"
type: feature
component: Backend
priority: high
status: Doing
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

2026-07-25 (workshift, overnight Worker) — Dependency CPE-1080 confirmed merged into `main` (PR #398); ticket
moved Backlog → Doing. Extended `crates/server/src/revert_safety.rs` per the design — no new `pub mod` line
(module already registered). Added:

`ConflictReport { safe: usize, conflicts: usize, conflict_paths: Vec<String>, conflict_ratio: Option<f64> }`
— `#[derive(Debug, Clone, PartialEq, Default)]`, deliberately NOT `Eq` because of the `f64` field.
`summarize_conflicts(&[ClassifiedAction]) -> ConflictReport` — single pass over the classified plan, counting
`Safe` vs `Conflict` and collecting conflict paths in `classified`'s own order (which `classify_plan` already
preserves as the underlying plan's deterministic path order, so no extra sort needed).
`plan_for(&[ClassifiedAction], include_conflicts: bool) -> Vec<RestoreAction>` — `false` filters to just the
`Safe` actions (the default "revert safe only" gate); `true` keeps every action ("revert anyway"). Order
preserved either way.

Included `conflict_ratio: Option<f64>` (assumption: the ticket marked it optional but the design's worked
example strongly implied it). Division-safe as required: `total = safe + conflicts`; `total == 0 → None`
(covers the empty-plan case), otherwise `Some(conflicts as f64 / total as f64)` — never a 0/0 NaN or inf,
verified by an explicit test asserting `.is_finite()` on the all-safe (ratio 0.0) and all-conflict (ratio 1.0)
cases plus a `None`-on-empty test.

Tests added to the existing `#[cfg(test)] mod tests` in `revert_safety.rs` (8 new, reusing the module's
`snap`/`touched` helpers and `plan_restore`): counts+paths+ratio on a mixed safe/conflict plan; empty plan →
`ConflictReport::default()` and `conflict_ratio == None`; all-safe → ratio `Some(0.0)`, finite; all-conflict →
ratio `Some(1.0)`, finite; `plan_for(false)` drops conflicts (safe subset only); `plan_for(true)` keeps every
action (compared directly against the classified actions' own `RestoreAction`s); `plan_for` on an empty
classified slice returns empty for both `true` and `false`.

Verify (from `crates/server`): `cargo test` — 970 passed, 0 failed (incl. the 8 new + all 7 pre-existing
CPE-1080 `revert_safety` tests, still green, untouched). `cargo clippy --all-targets -- -D warnings` clean;
`cargo clippy --all-targets --features index -- -D warnings` clean. No new deps — `Cargo.toml`/`Cargo.lock`
untouched (verified via `git diff --stat`: only `revert_safety.rs` changed).

Opened PR from branch `cpe-1082-conflict-summary`. Ticket left in Doing (status unchanged) pending Foreman
review + merge, matching the CPE-1080/CPE-1081 pattern.
