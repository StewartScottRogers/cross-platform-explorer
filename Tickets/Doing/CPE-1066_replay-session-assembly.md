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
- [ ] `cargo test` writes events to a temp journal dir (mirror the `audit_journal` test pattern:
      process-unique dir + atomic counter, `remove_dir_all` cleanup) via the journal's own write path, then
      asserts `load_replay` returns them in order with correct `bounds` + `summary`.
- [ ] A missing session yields empty events / `bounds: None` / empty summary (no panic).
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the CPE-728 assembly seam. Held in Backlog:
depends on CPE-1063 (`replay::bounds`) landing first.
