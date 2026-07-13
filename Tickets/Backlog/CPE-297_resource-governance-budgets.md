---
id: CPE-297
title: Resource governance & performance budgets
type: Feature
status: Open
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
---

## Summary

"Off = off" protects the explorer when a sidecar is disabled; this protects it when
one is **enabled**. A runaway sidecar (or a spawned agent) must not degrade the
explorer. Define and enforce per-sidecar resource budgets and back-pressure.

## Acceptance Criteria

- [ ] Documented budgets: zero startup cost when disabled; a per-sidecar memory
      ceiling; bounded IPC buffer with backpressure (no unbounded PTY/log growth).
- [ ] Supervisor monitors CPU/memory; a sidecar breaching limits is throttled or
      restarted with a surfaced warning, never silently.
- [ ] Child processes a sidecar spawns (agents, MCP servers) count toward its budget
      and are reaped with it.
- [ ] A benchmark proves the plain explorer's startup/memory are unchanged with the
      platform present-but-idle.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-265]]. **Phase:** P2. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
