---
id: CPE-1066
title: "Journal-backed replay assembly — cpe_server::replay_session (load_replay)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-728
depends-on: CPE-1063
---

## Summary
Child of CPE-728 (Activity replay & scrub). The assembly seam a future `#[tauri::command]` dispatches into:
read a session's durable audit journal back from disk and package it for replay (events + time bounds +
summary). Closes the epic's "persist beyond the transient cap → read back after restart → reconstruct"
DoD **headlessly**. Backend-only, `cargo test` on the 3-OS matrix — no GUI, no user resource, no new deps.
**Depends on CPE-1063** (uses `replay::bounds`) — dispatch after CPE-1063 merges.

## Design (buildable)
New module `crates/server/src/replay_session.rs`, registered `pub mod replay_session;` in `lib.rs` at a
distinct anchor (chosen at dispatch from the then-current lib.rs). Reuses
`audit_journal::read_session(base, session) -> Vec<AuditEvent>`, `replay::bounds`, and
`activity_timeline::summarize`.

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReplayData {
    pub events: Vec<audit_journal::AuditEvent>,
    pub bounds: Option<(u64, u64)>,
    pub summary: activity_timeline::ActivitySummary,
}

pub fn load_replay(base: &std::path::Path, session: &str) -> ReplayData;
```
Read the session via `audit_journal::read_session`, compute `bounds` (CPE-1063) + `summary`
(`activity_timeline::summarize`) over the events, return them in event order. A missing/empty session →
`ReplayData` with empty events, `bounds: None`, and an empty summary (no panic/error).

## Acceptance Criteria
- [x] `cargo test` writes events to a temp journal dir (mirror the `audit_journal` test pattern:
      process-unique dir + atomic counter, `remove_dir_all` cleanup) via the journal's own write path, then
      asserts `load_replay` returns them in order with correct `bounds` + `summary`.
- [x] A missing session yields empty events / `bounds: None` / empty summary (no panic).
- [x] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the CPE-728 assembly seam. Held in Backlog:
depends on CPE-1063 (`replay::bounds`) landing first.

2026-07-25 (workshift, Worker) — Built on branch `cpe-1066-replay-session`, off `main` (CPE-1063/1064
already merged). New module `crates/server/src/replay_session.rs`: `ReplayData { events, bounds,
summary }` + `load_replay(base, session)`, exactly as designed — no deviations from the spec. Registered
`pub mod replay_session;` in `lib.rs` immediately after `pub mod replay_transport;`, per the anchor
instruction.

Tests (4, in `#[cfg(test)] mod tests`) write events via `audit_journal::record` into a temp dir built
the same way `audit_journal`'s own tests do it (`std::env::temp_dir()` + `cpe-replay-session-{pid}-{seq}`
using a process-wide `AtomicU64` counter, `remove_dir_all` cleanup) — no hardcoded path, no new
platform-specific logic. Covered: normal multi-event load (order + bounds + summary), a session that
was never recorded at all under a real base dir, an empty session that exists alongside another
session's journal, and a single-event session (bounds `(t,t)`, `span_ms == 0`).

Verify: `cargo test` in `crates/server` — 917 passed, 0 failed (4 are the new `replay_session` tests,
individually re-run to confirm: 4 passed). `cargo clippy --all-targets -- -D warnings` clean; `cargo
clippy --all-targets --features index -- -D warnings` clean. No new deps (std + existing
`audit_journal`/`replay`/`activity_timeline`/`serde` only).

No assumptions beyond what the ticket already specified — the design's `ReplayData` shape, derive
stack, and empty-session behavior were followed as written. No blockers.
