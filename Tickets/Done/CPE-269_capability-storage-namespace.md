---
id: CPE-269
title: "Capability: storage namespace"
type: Task
status: Done
priority: Medium
component: Backend
estimate: 1h
created: 2026-07-13
closed: 2026-07-13
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

## Resolution

Implemented `providers::storage`: `StorageProvider` rooted at a host base dir serves
`Capability::Storage` via `storage.dir`, returning (and creating on first use) the
namespace `base/<sidecar_id>`. The id comes from the broker, and `is_safe_segment`
requires a single `Normal` path component — so `..`, `.`, `a/b`, `a\b`, absolute paths
and Windows prefixes are all rejected and a sidecar can never escape base or reach
another's dir. `clear()` removes a namespace for uninstall ([[CPE-276]]). 4 tests
(created-under-base, isolation, traversal rejected, clear); 36 unit + 3 E2E + clippy
green.

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented + tested during dayshift. Done.
