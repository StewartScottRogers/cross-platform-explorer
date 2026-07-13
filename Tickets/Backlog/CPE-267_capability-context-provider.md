---
id: CPE-267
title: "Capability: context provider (folder / repo / selection)"
type: Task
status: Open
priority: Medium
component: Backend
estimate: 1-2h
created: 2026-07-13
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

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-266]]. **Phase:** P3. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
