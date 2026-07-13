---
id: CPE-265
title: Process supervisor (spawn / health / restart / shutdown)
type: Task
status: Done
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

The host-side runtime that owns sidecar processes. Spawns a sidecar from its
manifest, performs the handshake, monitors health (heartbeat/liveness), restarts
on crash with backoff, and shuts down cleanly on app exit. Each sidecar is its own
crash domain — a sidecar dying never takes down the explorer or another sidecar.

## Acceptance Criteria

- [ ] Spawn sidecar per manifest (per-OS entry), establish the contract channel,
      complete handshake, reach Ready.
- [ ] Heartbeat/liveness monitoring; crash detected → restart with capped backoff.
- [ ] Graceful drain + shutdown on app quit; orphan processes reaped.
- [ ] Per-sidecar stdout/stderr captured to a log the host can surface.
- [ ] Tests with the hello sidecar ([[CPE-273]]): start, kill, auto-restart, stop.

## Resolution

Implemented `supervisor` in `sidecar-host`: `spawn_process` launches a child with
piped stdio and a reader thread that decodes JSON-line envelopes onto a channel;
`ProcessConnection` implements a `Connection` (send/recv with timeout, `is_alive` via
try_wait, `shutdown`/Drop that kill+reap so there are never orphans). `handshake()`
receives Hello, negotiates the contract version, computes grants via the broker
([[CPE-266]]), sends Welcome, and awaits Ready — and on a major mismatch sends
`Rejected` and refuses to mount (this is the enforcement half of [[CPE-263]]).
`RestartPolicy` gives capped exponential backoff.

Added a minimal `echo_sidecar` bin (a test fixture, not the full hello sidecar
[[CPE-273]]) and `tests/supervisor_e2e.rs`, which spawns it as a **real process** and
asserts: the conformance kit ([[CPE-301]]) passes end-to-end over the OS boundary, the
handshake completes with the expected grants, and liveness/shutdown track the process.
29 unit + 3 E2E tests; clippy clean.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented supervisor + real-process E2E during dayshift; conformance
kit passes against a spawned process. Done. Also closes the negotiation-enforcement
half of [[CPE-263]].
