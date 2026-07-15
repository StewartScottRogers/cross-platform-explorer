---
id: CPE-415
title: "Flaky reattach tests — replay/output race on loaded CI"
type: Defect
status: Done
priority: High
component: Backend
tags: [ready]
estimate: 30m
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Discovered during the Nightshift: `session_daemon`'s reattach tests intermittently fail on the
loaded ubuntu CI runner (e.g. `a_session_survives_a_client_dropping_and_a_new_client_reattaches`
at `session_daemon.rs:258`). Cause: a client waits for a marker (READY/TICK) on the *live* stream
only, but on a slow machine the marker is emitted BEFORE the client attaches, so it arrives in the
`replay` instead → the live wait times out. This is the same race already fixed in `session_server`;
`session_daemon`'s own tests (slice 1) still had it. It threatens the green pipeline.

## Acceptance Criteria
- [x] Reattach tests accept a marker from `replay` OR `live` (accumulate both), removing the race.
- [x] `cargo test -p ai-console` green repeatedly; no timing-dependent live-only waits remain in the
      affected tests.

## Work Log
2026-07-15 — Nightshift discovery. Fix: a `drain_attach` test helper that seeds the accumulator with
the attachment's `replay` before draining `live`; apply to the racy `session_daemon` tests.

2026-07-15 — Fixed. Added a `drain_attach(att, marker, timeout)` test helper (seeds the accumulator with `att.replay`, then drains `att.live`) and applied it to the racy `session_daemon` reattach tests (`a_session_survives…`, `two_clients…`, `reaping…`). Ran `cargo test session_` 3× — green each time; `cargo test --lib` 147 passed; clippy clean. Root cause was slice-1's live-only marker waits; `session_server` (slice 2) was already fixed the same way.

## Resolution
`sidecar/ai-console/src/session_daemon.rs` tests only. No production code changed — a pure test-reliability fix removing a timing race that intermittently reddened CI.
