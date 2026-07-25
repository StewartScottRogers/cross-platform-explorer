---
id: CPE-375
title: "Runtime catalog reload (sidecar registry hot-swap) (CPE-308 part 2, slice 2b)"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-07-14
closed: 2026-07-14
---

## Summary

After a catalog update is applied to disk, the running console must pick up the new/updated agents
without a restart. Make the sidecar's registry reloadable and add a trigger.

## Acceptance Criteria

- [x] `ConsoleState.registry` becomes hot-swappable (behind a lock); all read sites updated.
- [x] `ConsoleState` remembers its catalog dir + trusted keys so a reload re-runs
      `load_signed_source` over the bundled dirs + verified source.
- [x] `reload_catalog()` + `POST /api/catalog/reload` → re-scan and swap the registry atomically;
      returns the new agent count.
- [x] Tests: after adding a newly-signed manifest to the source dir, reload surfaces it; a bad
      source leaves the current registry intact.
- [x] clippy clean; no regression to launch/catalog/install paths.

## Notes
No network — fully testable. Feeds [[CPE-376]] (fetch calls reload after apply). Part of [[CPE-308]].

## Work Log
2026-07-14 — Filed (rescoped from the old fetch+reload 375 once the GitHub-releases source was chosen).
