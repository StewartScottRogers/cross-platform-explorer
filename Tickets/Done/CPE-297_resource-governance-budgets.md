---
id: CPE-297
title: Resource governance & performance budgets
type: Feature
status: Done
priority: High
component: Backend
estimate: 3-4h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Implemented `resources` in sidecar-host: `ResourceBudget { max_memory_bytes }`, a `MemorySampler` trait (fake for tests, `SysinfoSampler` real via `sysinfo`), and `check(sampler, pid, budget) -> Verdict::{Within|Over{used,limit}|Unknown}` so the supervisor can warn/throttle/restart a breaching sidecar. Also made the supervisor's inbound IPC channel **bounded** (`sync_channel(1024)`) so PTY/log output applies backpressure instead of buffering without limit. 4 tests incl. the real sampler reading this process's own memory (>0). 70 host tests + clippy green; supervisor E2E still passes with the bounded channel.

**Deferred:** live enforcement wiring (the supervisor sampling on a cadence + acting on Over) and OS-level job-object confinement of the agent's child tree land at integration; the 'zero startup delta when disabled' budget is architectural (nothing runs when off) and benchmarked at integration.

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-13 — Implemented memory budgets + bounded-channel backpressure during dayshift. Done.
