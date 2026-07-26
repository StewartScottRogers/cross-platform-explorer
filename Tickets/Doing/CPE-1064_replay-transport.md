---
id: CPE-1064
title: "Scrubber transport model — cpe_server::replay_transport (step / window / advance)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-728
estimate: 2-3h
---

## Summary
Child of CPE-728 (Activity replay & scrub). The playback transport for the scrubber — step to event
boundaries, slice a playback window, and advance the playhead at a speed. **Pure `ts`-only logic** (no
dependency on the projection core), backend-only, `cargo test` on the 3-OS matrix — no GUI, no user
resource, no new deps. Reuses `audit_journal::AuditEvent`.

## Design (buildable)
New module `crates/server/src/replay_transport.rs`, registered `pub mod replay_transport;` in `lib.rs`
**immediately after `pub mod audit_journal;`**. Operates only on `AuditEvent.ts`.

```rust
/// Next / previous DISTINCT event timestamp strictly after / before the playhead cursor.
pub fn step_next(events: &[AuditEvent], cursor: u64) -> Option<u64>;
pub fn step_prev(events: &[AuditEvent], cursor: u64) -> Option<u64>;
/// Half-open [t0, t1) slice of events for one playback tick (boundary event at t0 included, t1 excluded).
pub fn events_in_window(events: &[AuditEvent], t0: u64, t1: u64) -> Vec<&AuditEvent>;
/// Advance the cursor by delta_ms × (speed_num/speed_den), clamped to [start, end].
pub fn advance(cursor: u64, delta_ms: u64, speed_num: u64, speed_den: u64, start: u64, end: u64) -> u64;
```

**⚠ Checked/saturating arithmetic (learned this shift — a reviewer caught overflow panics on user input):**
`advance` MUST use `saturating_mul`/`saturating_add` for `delta_ms × speed_num` and the division, then clamp
to `[start, end]`. A huge `delta_ms`/speed from the UI must **saturate to `end`, never panic or wrap**.
`speed_den == 0` → treat as no-op (return clamped cursor). `speed_num == 0` → no-op. Events assumed/sorted by
ts (sort defensively if needed).

**Cross-OS:** pure integer/`ts` math — no `std::path`, no `#[cfg]`.

## Acceptance Criteria
- [ ] `step_next`/`step_prev` land exactly on the next/previous distinct event ts and return `None` past the
      ends; duplicate timestamps skipped to the next *distinct* value.
- [ ] `events_in_window` is half-open: an event at exactly `t0` is included, at `t1` excluded.
- [ ] `advance` past `end` clamps to `end`; below `start` clamps to `start`; `speed 0`/`den 0` → no-op;
      `advance(cursor, u64::MAX, u64::MAX, 1, …)` **saturates without overflow/panic**.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-728 slice. Independent of the projection core
(ts-only). One-line lib.rs `pub mod` at a distinct anchor. Saturating-arithmetic requirement per this shift's
reviewer-caught overflow bugs.
