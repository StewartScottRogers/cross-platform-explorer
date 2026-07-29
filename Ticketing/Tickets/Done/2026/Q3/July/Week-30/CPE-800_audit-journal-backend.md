---
id: CPE-800
title: On-disk append-only session audit journal
type: feature
status: Done
priority: medium
component: Backend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-733
estimate: 3-4h
---

## Summary
Backend for epic CPE-733: append each session's filesystem-activity events to a durable per-session
JSON-lines journal that survives restart, plus read-back of past sessions (feeding the CPE-799 export).

## Acceptance Criteria
- [x] Events append durably per session; the journal survives app restart; past sessions are listable/readable.
- [x] Bounded/rotated so it can't grow without limit; opt-in with no cost when Agent Watch is off.
- [x] cargo/CI green.

## Notes
Prereq: CPE-799 (shared event model). Shares the log with replay (CPE-728).

## Work Log
- 2026-07-20 — Picked up. Estimate: 3-4h. Prereq CPE-799 (shared event model, `src/lib/auditExport.ts`)
  already shipped, so it's satisfied. Grepped the backend — no existing journal/audit code. Plan: a pure
  `audit_journal` module (append / list / read / bounded rotation) over a base dir, cargo-tested, plus thin
  async Tauri commands that resolve the journal dir under the app-data dir.
- 2026-07-20 — Wrote `src-tauri/src/audit_journal.rs`: `AuditEvent` mirroring the frontend model
  (ts/session/kind/path/detail), `record` (durable append + trim), `list_sessions`, `read_session`
  (malformed-line tolerant), rotation via temp-file + rename. Session ids are sanitized to a safe single
  path segment so they can't escape the journal dir.
- 2026-07-20 — Added `mod audit_journal` + three async commands (`audit_record` / `audit_sessions` /
  `audit_read`) resolving `<app-data>/audit`, registered in `generate_handler!`. `ts` is stamped
  server-side so callers can't skew the log.
- 2026-07-20 — 6 module tests green (`cargo test --no-default-features audit_journal`); clippy clean in
  BOTH feature modes (`--no-default-features` and default/sidecar), `--all-targets -D warnings`.

## Resolution
Added a durable per-session audit journal. New pure module `src-tauri/src/audit_journal.rs` stores each
session's Agent Watch activity as JSON-lines in `<app-data>/audit/<session>.jsonl`:

- `record(base, event, max_events)` — appends one event (create-dir, append, flush for durability) then
  rotates the file to its last `max_events` lines (temp-file + rename, so a crash can't leave a
  half-truncated journal). `MAX_EVENTS_PER_SESSION = 10_000` bounds growth.
- `list_sessions(base)` — sorted session ids that have a journal; missing dir → empty.
- `read_session(base, session)` — reads events back in append order, skipping malformed lines (robust to a
  partial trailing write). This survives restart because it reads straight from disk with no in-memory state.

`lib.rs` gains `mod audit_journal` and three thin async (`spawn_blocking`) commands — `audit_record`,
`audit_sessions`, `audit_read` — that resolve the journal dir under the app-data dir and delegate. `ts` is
stamped server-side. The journal is only written when the frontend records activity, so it costs nothing
when Agent Watch is off. Six module tests cover order-preservation, restart survival, bounded rotation
(newest kept), sorted listing, malformed-line skipping, and detail round-trip.

Tradeoffs: `record` re-reads the file to trim on each append (O(n)); acceptable for a bounded, opt-in
journal — a running byte/line count is a future optimisation if a hot session shows up. Session ids are
sanitized (non-`[A-Za-z0-9_-]` → `_`), so two ids differing only in unsafe characters would collide on one
file — a non-issue for uuid-style session ids. The `AuditEvent` shape mirrors `src/lib/auditExport.ts`, so
the CPE-799 exporter and CPE-801 history browser read it directly. Foundation for CPE-801 (history browser).
