---
id: CPE-609
title: Recursive walks skip symlinked directories (avoid cycles)
type: enhancement
component: Backend
priority: low
status: Done
tags: ready
estimate: 30m
created: 2026-07-18
closed: 2026-07-18
---

## Summary
`find_files_by_name` (CPE-603) and `search_file_contents` (CPE-416) descend into every sub-directory,
following symlinks. A symlinked directory pointing at an ancestor is a **cycle** — the walk re-enters
the tree until it hits its cap (50k dirs / 20k files), wasting work and truncating real results. Skip
symlinked directories, matching ripgrep's default (which requires `-L` to follow) and the app's
"fast/predictable" ethos.

## Acceptance Criteria
- [x] A shared `entry_is_symlink` helper detects a symlink without following it.
- [x] Both recursive walks skip descending into symlinked directories (symlinked *files* unaffected; a
      symlinked dir is still *reported* as a match in find-by-name).
- [x] A cargo test builds a symlink cycle and asserts both walks terminate (not `truncated`, few dirs
      scanned), skipping gracefully where symlink creation is unprivileged.
- [x] cargo tests + clippy (both feature modes) clean.

## Resolution
Added `entry_is_symlink(&fs::DirEntry)` (uses `file_type()`, which does not follow the link) and gated
the `stack.push` in both `find_files_by_name` and `search_file_contents` on it. New test
`recursive_walks_skip_symlinked_dirs_and_do_not_cycle` creates a `loop -> root` symlink and asserts
both walks terminate immediately (verified live on this machine — symlink creation succeeded, so the
cycle path was actually exercised).

## Work Log
2026-07-18 (Nightshift Loop 3) — Found while reviewing the CPE-603 walk for defects; the existing
content-search shared the issue, so fixed both.
