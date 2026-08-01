---
id: CPE-1224
title: "Frontend path-keyed stores (favourites/frecency/recents) orphan on rename/move"
type: Bug
priority: Low
component: frontend
tags: [ready]
estimate: 1h
created: 2026-08-01
closed:
---

## Problem
Surfaced during CPE-1222 (which fixed the *backend* tag store). The frontend-only `localStorage`
stores keyed by path — favourites, spotlight frecency (`cpe.spotlightFrecency`), and recents — are NOT
migrated when a file/folder is renamed or moved, so their entries orphan at the old path (a favourited
folder loses its star after rename; frecency/recents keep dead paths). Same class of bug CPE-1222 fixed
server-side.

## Acceptance criteria
- On rename/move, the frontend migrates favourites + frecency + recents entries (exact path + subtree)
  to the new path, mirroring the backend `tag_store_rename_subtree` behaviour.
- Consider a single shared path-migration helper the rename/move handlers call for all three stores.
- Tests for each store (rename, move, subtree).
