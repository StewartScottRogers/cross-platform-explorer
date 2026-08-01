---
id: CPE-1225
title: "snapshot_schedule::Catalog (path-keyed by root) orphans on folder rename/move"
type: Bug
priority: Low
component: cpe-server
tags: [ready]
estimate: 1h
created: 2026-08-01
closed:
---

## Problem
Surfaced during CPE-1222. `crates/server/src/snapshot_schedule.rs`'s `Catalog` is keyed by the `root`
folder path and backend-persisted; renaming/moving a folder that has a scheduled-snapshot entry orphans
that entry at the old path (the schedule silently stops applying to the folder's new location).
`snapshot::BlobStore` is hash-keyed and immune; only the schedule catalog is affected.

## Acceptance criteria
- Renaming/moving a folder migrates its `snapshot_schedule::Catalog` entry to the new root path.
- Hook it into the same `ServerCtx`-threaded rename/move primitives CPE-1222 established (best-effort,
  never fails the fs op).
- Regression test covering a scheduled folder renamed + moved.
