---
id: CPE-868
title: Resilient sidecar launch — retry spawn/handshake with backoff (L3)
type: feature
component: Sidecar
priority: medium
status: Done
tags: ready
epic: CPE-862
created: 2026-07-21
closed: 2026-07-21
---

## Summary
Epic CPE-862 L3. A transient spawn/handshake failure (a slow disk, an AV scan holding the exe for a beat,
a port race) currently fails the launch outright. Retry the spawn+handshake a few times with a short
linear backoff so a momentary hiccup self-recovers before the user sees an error.

## Approach
A pure, injected-sleep `retry_with_backoff(attempts, base_delay, sleep, op)` helper (unit-testable), used
to wrap the `spawn_process_with_env` + `handshake` in `sidecar_start_ai_console` and
`sidecar_start_agent_board`. On a failed attempt the wedged connection is dropped (killing the process)
before the next try, so no orphan is left. (Crash **auto-restart** while running is a further L3 piece,
tracked separately.)

## Acceptance Criteria
- [x] `retry_with_backoff` returns the first Ok, or the last Err after N attempts; sleeps between tries
      (not after the last). Unit-tested with an injected sleep spy (success-on-Nth + always-fail cases).
- [x] Both `sidecar_start_*` launch commands wrap spawn+handshake in it; a failed attempt drops the conn.
- [x] `cargo test --features sidecar-platform` + clippy `-D warnings` green in both feature modes.

## Work Log
- 2026-07-21 — Picked up autonomously after CPE-867 (L2).
- 2026-07-21 — Implemented: pure retry_with_backoff (injected sleep, unit-tested) wrapping spawn+handshake in both sidecar_start_* commands; a failed attempt drops its conn (kills the process). cargo test + clippy (both modes) green. Closing.
