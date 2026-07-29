---
id: CPE-632
title: "Compressing a folder into a .zip inside it added the archive to itself"
type: Bug
component: Backend
priority: low
status: Done
tags: ready
estimate: 30m
created: 2026-07-18
closed: 2026-07-18
---

## Summary
`compress_to_zip` creates the output `.zip` first, then walks the sources. If the dest lives inside a
source folder, the recursive walk reaches the (growing) output archive and tries to add it to itself —
a file-sharing violation or a corrupt, bloated archive. Skip the output file during the walk.

## Acceptance Criteria
- [x] `zip_add_path` skips a source whose canonical path equals the output archive (threaded from
      `compress_to_zip`).
- [x] Compressing a folder into a `.zip` inside it succeeds and the archive doesn't contain itself.
- [x] cargo test + clippy clean.

## Resolution
Added a `skip: Option<&Path>` parameter to `zip_add_path` (canonical-path compared, returns early) and
pass the canonicalized dest from `compress_to_zip`. Test `compress_skips_the_output_archive_inside_a_source`.

## Work Log
2026-07-18 (dayshift) — Found auditing the archive-creation path (sibling of the copy self-descendant guard).
