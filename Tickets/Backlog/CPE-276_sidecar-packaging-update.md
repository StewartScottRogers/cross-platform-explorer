---
id: CPE-276
title: Sidecar packaging & independent update
type: Task
status: Open
priority: Medium
component: Packaging
estimate: 3-4h
created: 2026-07-13
---

## Summary

How sidecar binaries ship and update **on their own cadence** — the point of "keep
up with the market without ricochet." A sidecar can be bundled with the app or
fetched/updated independently of the explorer's release, without breaking the
delete-test or the updater pipeline.

## Acceptance Criteria

- [ ] Defined layout for bundled sidecars + an install location for
      fetched-at-runtime sidecars.
- [ ] A sidecar can update to a new version (compatible contract) without an
      explorer release; incompatible versions are refused cleanly ([[CPE-263]]).
- [ ] Uninstalling a sidecar removes its binary, storage ([[CPE-269]]), and
      registry entry.
- [ ] Does not disturb the explorer's own updater/latest.json flow.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-264]], [[CPE-265]]. **Phase:** P5. **Epic:** [[CPE-260]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
