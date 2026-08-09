---
id: CPE-1080
title: "Revert safety — cpe_server::revert_safety (3-way conflict classifier)"
type: feature
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-25
epic: CPE-732
estimate: 2-3h
---

## Summary
Child of CPE-732 (Checkpoint & rollback). Classify each restore action as safe vs conflicting so a blind
revert never silently clobbers a concurrent EXTERNAL edit — the epic's "revert safety when files changed
outside the agent" question. **Pure** in `crates/server`, `cargo test` on the 3-OS matrix — no GUI, no user
resource, no new deps. Reuses `restore_plan::RestoreAction` + `Snapshot`; does NOT modify them.
Parallel-independent (takes the agent-touched set as a plain param).

## Design (buildable)
New module `crates/server/src/revert_safety.rs`, registered `pub mod revert_safety;` in
`crates/server/src/lib.rs` **immediately after `pub mod restore_plan;`**. Read `restore_plan.rs` for
`RestoreAction` (+ the `Snapshot` type it imports — grep for `struct Snapshot`, likely in `snapshot.rs`).

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind { ChangedOutsideAgent, RecreatedOutsideAgent, DeletedOutsideAgent }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertSafety { Safe, Conflict(ConflictKind) }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedAction { pub action: RestoreAction, pub safety: RevertSafety }

pub fn classify_plan(
    plan: &[RestoreAction], checkpoint: &Snapshot, current: &Snapshot,
    agent_touched: &BTreeSet<String>,   // root-relative keys from CPE-1079 (plain param → independent)
) -> Vec<ClassifiedAction>;
pub fn partition(classified: &[ClassifiedAction]) -> (Vec<RestoreAction> /*safe*/, Vec<ClassifiedAction> /*conflicts*/);
```
An action is **Safe** iff the agent is recorded as having touched that path (`agent_touched.contains(key)`);
**Conflict** iff `current` diverges from `checkpoint` for that path in a way the agent's recorded activity
does NOT explain (an outside edit the revert would overwrite/delete). Pick the `ConflictKind` from the
checkpoint-vs-current shape (changed/recreated/deleted outside). Preserve the plan's deterministic order.

## ⚠ Notes
Pure map/set diff over the same `/`-segment keys as CPE-1079. No arithmetic, no recursion, no division. Plain
`PartialEq`/`Eq` (no f64), no serde/specta. Cross-OS: string keys, no `std::path`.

## Acceptance Criteria
- [x] Agent-only modification → Safe; a third-party change (path NOT in `agent_touched`) →
      `Conflict(ChangedOutsideAgent)`; a Delete of a file the agent never created → Conflict.
- [x] Empty `agent_touched` + a real diff → all Conflict; `partition` splits safe vs conflicts correctly.
- [x] Deterministic order preserved; empty plan → empty.
- [x] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean default AND
      `--features index`; no new deps.

## Work Log
2026-07-25 (sprint) — Filed by the Product Manager as CPE-732's flagged un-mined vein. Parallel-independent
(agent_touched is a plain param). Distinct lib.rs anchor. CPE-1082 depends on this module's `ClassifiedAction`.

2026-07-25 (sprint Worker) — Built `crates/server/src/revert_safety.rs` per design; registered
`pub mod revert_safety;` immediately after `pub mod restore_plan;` in `lib.rs`. `classify_plan` marks an
action `Safe` iff `agent_touched.contains(&action.path)` (action.path IS the root-relative key — it's
already the same string `RestoreAction`/`Snapshot` key `restore_plan.rs` uses, so no extra normalization
was needed beyond what CPE-1079's `agent_touched` set is documented to produce). Otherwise `Conflict`,
kind picked from `(checkpoint.contains_key(path), current.contains_key(path))`: `(true,false)` →
`DeletedOutsideAgent` (checkpoint had it, current doesn't — matches `RestoreOp::Create` actions, i.e. the
outside world deleted it since the checkpoint); `(false,true)` → `RecreatedOutsideAgent` (current has it,
checkpoint didn't — matches `RestoreOp::Delete` actions, i.e. the outside world created it after the
checkpoint); `(true,true)`/default → `ChangedOutsideAgent` (both have it, content differs — matches
`RestoreOp::Overwrite`). `partition` splits by iterating `classified` in order, so plan order is preserved
in both outputs. 9 unit tests added (thorough per ticket: agent-only→Safe, third-party change→
`ChangedOutsideAgent`, delete of never-agent-created file→`RecreatedOutsideAgent`, recreate of
agent-never-deleted file→`DeletedOutsideAgent`, empty `agent_touched`+real diff→all Conflict, `partition`
split+order, empty plan→empty, deterministic order). No dependency on CPE-1079's module (took
`agent_touched` as the plain `&BTreeSet<String>` param per the parallel-independence design) — that ticket
hadn't landed yet when this was built, confirming the independence held. Verify: `cargo test -p cpe-server`
946 passed / 0 failed; `cargo clippy --all-targets -- -D warnings` clean; `cargo clippy --all-targets
--features index -- -D warnings` clean; no `Cargo.toml`/`Cargo.lock` changes. Branch
`cpe-1080-revert-safety`, PR opened against `main`. Left ticket in `Doing` (moves to `Done` on merge per
repo convention, e.g. CPE-1048).
