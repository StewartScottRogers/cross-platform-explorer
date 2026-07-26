---
id: CPE-1063
title: "Replay projection core — cpe_server::replay (state_at: reconstruct FS state at moment T)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-728
estimate: 3h
---

## Summary
Child of CPE-728 (Activity replay & scrub). The FOUNDATION: reconstruct the folder state at any moment `T`
by folding the already-recorded audit-event stream. **Pure event-sourcing fold**, backend-only, `cargo test`
on the 3-OS matrix — no GUI, no user resource, no new deps. Reuses `audit_journal::AuditEvent`; does not
rebuild it.

## Design (buildable)
New module `crates/server/src/replay.rs`, registered `pub mod replay;` in `lib.rs` **immediately after
`pub mod activity_timeline;`**. Input type (already exists, `crates/server/src/audit_journal.rs`):
`AuditEvent { ts: u64, session: String, kind: String, path: String, detail: Option<String> }`.

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FsNode { pub ts: u64, pub kind: String }   // last-touch ts + last event kind

pub type FsState = std::collections::BTreeMap<String, FsNode>;   // path -> node

/// Reconstruct the live set at time `t_ms`: fold every event with `ts <= t_ms`, in ts order.
pub fn state_at(events: &[AuditEvent], t_ms: u64) -> FsState;
pub fn bounds(events: &[AuditEvent]) -> Option<(u64, u64)>;      // (min_ts, max_ts); None if empty
pub fn seek_index(events: &[AuditEvent], t_ms: u64) -> usize;    // count of events with ts <= t_ms
```
Fold rules by `kind`: `created` → insert path; `modified` → update node ts/kind (insert if absent);
`removed` → delete path; `renamed` → move entry from `path` to the target (parse target out of `detail` —
document the format assumption, e.g. detail holds the new path; if unparseable, treat as a no-op and note);
`read` → no state change. **Sort events by `ts` (stable) before folding** so unsorted input still projects
correctly. Empty stream → empty state.

**Cross-OS:** treat paths as opaque normalized strings — NO `std::path` platform semantics, no `#[cfg]`.

## Acceptance Criteria
- [ ] `state_at` correct at several T (live set matches the folded events ≤ T); a `removed` deletes; a
      `renamed` relocates (old path gone, target present); `read` is a no-op; `modified` updates ts/kind.
- [ ] Unsorted input folded in ts order (same result as pre-sorted).
- [ ] `bounds([])==None`, `bounds` returns (min,max) otherwise; `seek_index` = count of events ≤ T; empty
      stream → empty state (no panic).
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the CPE-728 foundation (virtual-time projection
over the audit journal). Independent module; one-line lib.rs `pub mod` at a distinct anchor. CPE-1065/1066
depend on this module's `FsState`.
