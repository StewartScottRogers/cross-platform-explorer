---
id: CPE-1222
title: "Bug (pre-existing): rename/move leaves an orphaned tag-store entry at the old path"
type: Bug
status: Open
priority: Low
component: cpe-server
tags: [ready]
estimate: 1h
created: 2026-08-01
closed:
---

## Problem
Surfaced (not caused) during the CPE-1194 review. `rename_entry_impl` / `move_exact_impl` never
migrate a path's tag-store entries when the file moves, so tags recorded against the OLD path are
orphaned after a rename/move — e.g. a "Tag then Rename" macro that is never undone leaves a stale
tag entry keyed on the pre-rename path, and the renamed file loses its tags. Pre-dates CPE-1194 and
is out of that ticket's scope.

## Acceptance criteria
- Renaming or moving a tagged file/folder migrates its tag-store entries to the new path (single-file
  and directory-subtree cases).
- A rename/move followed by reload shows the tags on the new path and none orphaned at the old path.
- Regression test covering rename + move, including a tagged file inside a renamed directory.

## Notes
Consider doing the migration in the same `cpe-server` layer that owns the tag store, invoked from the
rename/move commands. Check whether other path-keyed stores (favourites, frecency, snapshots) have the
same orphaning issue and note them here if so.
