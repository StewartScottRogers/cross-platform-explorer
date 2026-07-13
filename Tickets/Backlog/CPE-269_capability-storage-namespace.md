---
id: CPE-269
title: "Capability: storage namespace"
type: Task
status: Open
priority: Medium
component: Backend
estimate: 1h
created: 2026-07-13
---

## Summary

Each sidecar gets a private, host-assigned storage directory for its own
settings/state/caches — isolated from the explorer's config and from other
sidecars. Keeps sidecars self-contained (part of "could stand alone") and prevents
shared-global entanglement.

## Acceptance Criteria

- [ ] `storage.dir()` resolves a per-sidecar path under the app data dir, created
      on first use.
- [ ] No sidecar can read another's storage or the explorer's settings via this
      capability.
- [ ] Cleared on sidecar uninstall (see [[CPE-276]]).
- [ ] Test: two sidecars get distinct, isolated dirs.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-266]]. **Phase:** P3. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
