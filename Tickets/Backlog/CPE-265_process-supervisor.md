---
id: CPE-265
title: Process supervisor (spawn / health / restart / shutdown)
type: Task
status: Open
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
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

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-262]], [[CPE-264]]. **Phase:** P2. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
