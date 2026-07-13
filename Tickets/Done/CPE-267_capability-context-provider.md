---
id: CPE-267
title: "Capability: context provider (folder / repo / selection)"
type: Task
status: Done
priority: Medium
component: Backend
estimate: 1-2h
created: 2026-07-13
closed: 2026-07-13
---

## Summary

The one place a sidecar may learn about the explorer's world — and only through a
narrow, host-brokered read API. Exposes the current folder, git repo root/remote,
and current selection as immutable snapshots + change events. This is the seam the
AI Console uses to scope a session to the open repo, with no reach-into internals.

## Acceptance Criteria

- [ ] `context.current()` returns { folder, repoRoot?, remote?, selection[] } DTOs
      from the contract crate — no explorer types leak across.
- [ ] Change events pushed when the host's folder/selection changes (opt-in).
- [ ] Read-only; a sidecar cannot mutate explorer state through this capability.
- [ ] Tests via the hello sidecar reading context.

## Resolution

Implemented `providers::context` in `sidecar-host`: a `ContextSnapshot` DTO
(folder / repo_root / remote / selection), a `ContextSource` trait the explorer
implements to supply the live snapshot, and `ContextProvider` (a `CapabilityProvider`
for `Capability::Context`) serving the read-only `context.current` method — no mutation
method exists, and only the DTO crosses the boundary, so a sidecar can't reach or
change explorer state. Tested directly and through the broker (denied when not granted,
served when granted). 3 tests; 32 unit + 3 E2E + clippy green.

**Deferred:** push change-events on folder/selection change — that rides the event-bus
capability ([[CPE-270]]); the on-demand read API (the substance) is complete. The
hello-sidecar E2E read comes with [[CPE-273]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented + tested during dayshift. Done (change-event push deferred to
CPE-270).
