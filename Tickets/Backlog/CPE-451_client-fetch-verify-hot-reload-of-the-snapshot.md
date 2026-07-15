---
id: CPE-451
title: "Client fetch + verify + hot-reload of the snapshot"
type: Feature
status: Open
priority: Medium
component: Backend
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-15
epic: CPE-444
---

## Summary
The client downloads the signed model-catalog snapshot (host-mediated, allow-listed), verifies signature + anti-rollback, and hot-reloads the registry without a restart. Offline/stale handling + refresh cadence.

## Acceptance Criteria
- [ ] Host-mediated fetch of the snapshot from the app's Releases (reuse CPE-376 fetch path); signature + monotonic-version verified before use.
- [ ] Hot-reload swaps the model catalog live (mirror CPE-375 registry reload).
- [ ] Offline/stale: fall back to the last good snapshot with a clear 'as of <date>' indication; manual + periodic refresh.
- [ ] Unit tests on the verify + stale-fallback logic.

## Notes
needs-prereq: CPE-450 (the snapshot must exist) + shares its signing-key gate.
