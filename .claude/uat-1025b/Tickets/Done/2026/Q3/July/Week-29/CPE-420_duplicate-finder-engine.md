---
id: CPE-420
title: "Find duplicate files (engine)"
type: Feature
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 1-2h
created: 2026-07-15
closed: 2026-07-15
---

## Summary

The natural culmination of the integrity tools (checksum CPE-412, compare CPE-418): scan a folder for
**duplicate files** and report them grouped, with the reclaimable space. Efficient by construction —
group by size first (a unique size can't be a duplicate), then hash only the size-collision
candidates (SHA-256), so most files are never read. Bounded like the content search. Engine this
loop; the results UI follows (CPE-421). Nightshift research loop 9.

## Acceptance Criteria

- [x] `find_duplicates(root)` returns groups of >=2 identical files `{ size, hash, paths[] }`, plus
      `files_scanned` and a `truncated` flag; sorted largest-wasted-space first.
- [x] Size-prefiltered (only size collisions are hashed); skips dot-dirs, empty files, symlink loops
      via a scan cap; unreadable entries skipped, never failing the whole scan.
- [x] A folder with no duplicates returns an empty list; a non-folder root errors.
- [x] Unit tests over a temp tree (dupes across subfolders, unique sizes ignored, 3-way group);
      `cargo clippy` clean.

## Work Log

2026-07-15 — Nightshift loop 9. Estimate 1-2h. Backend engine + tests; UI in CPE-421.

2026-07-15 - Done. Extracted a shared sha256_file helper (hash_file now calls it). find_duplicates: size-group pass 1 (skip dot-dirs/empty/non-file, cap 50k, truncated flag), then hash only size-collision candidates and group by digest; groups sorted by reclaimable space (size*(copies-1)). Unit test: 3-way group across subfolders, same-size-different-bytes decoy excluded, unique/empty ignored, no-dup folder empty, non-folder Err. cargo test pass, clippy clean. UI in CPE-421.

## Resolution
Backend engine landed + tested; the results/cleanup UI is the follow-up. Reuses the CPE-412 hashing so behaviour is consistent with the checksum tool.
