---
id: CPE-611
title: "dir_size can stack-overflow (crash) on a symlink cycle"
type: Bug
component: Backend
priority: high
status: Done
tags: ready
estimate: 30m
created: 2026-07-18
closed: 2026-07-18
---

## Summary
`dir_size` (the Properties dialog's recursive folder-size) walks with **unbounded recursion** and
follows symlinks (`entry.metadata()`). A symlinked directory pointing at an ancestor is a cycle, so the
recursion never terminates → the thread **stack overflows and the app crashes**. Opening Properties on
a folder that contains a symlink/junction loop (some `node_modules`, a self-referential junction, etc.)
would take the app down. `find_duplicates` shares the (bounded but wasteful) non-guard. Sibling of
CPE-609, but higher severity because `dir_size` recurses without a cap.

## Acceptance Criteria
- [x] `dir_size` does not recurse into symlinked directories (no crash on a cycle); a symlinked dir's
      target size is not counted, matching `du`/Explorer.
- [x] `find_duplicates` skips symlinked directories too (no wasted walk to its cap on a cycle).
- [x] A cargo test builds a symlink cycle and asserts `dir_size` returns the real total (no overflow)
      and `find_duplicates` terminates; skips gracefully where symlink creation is unprivileged.
- [x] cargo tests + clippy (both feature modes) clean.

## Resolution
Both walks now gate directory descent on `!entry_is_symlink(&entry)` (the helper added in CPE-609).
Extended `recursive_walks_skip_symlinked_dirs_and_do_not_cycle` to assert `dir_size` returns 6 (the one
real file) and `find_duplicates` is not truncated on a `loop -> root` symlink. The test ran for real on
this machine (symlink creation succeeded), so the crash path is verified fixed — without the guard the
test binary itself would stack-overflow.

## Work Log
2026-07-18 (Nightshift Loop 4) — Found by auditing the app's other recursive walks after fixing the
same class in CPE-609; dir_size was the severe one (unbounded recursion → crash, not just wasted work).
