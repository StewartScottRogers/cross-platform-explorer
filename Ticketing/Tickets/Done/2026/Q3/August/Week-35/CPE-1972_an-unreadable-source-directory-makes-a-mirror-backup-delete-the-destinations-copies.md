---
id: CPE-1972
title: an unreadable source directory makes a **mirror** backup delete the destination's copies of everything inside it — reproduced on `main`, `ok=3 fail=0`
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

**Data loss on `main` today**, on a path users run deliberately. Found by CPE-1925's worker as a side
effect and **independently reproduced on disk** by that PR's Reviewer, using the real
`compare::scan_tree` → the real `src/lib/backup.ts` `planBackup` (run under `vite-node`, `git show`n
from each revision, never reimplemented) → the real `backup::apply_backup_plan`, on real ext4.

Fixture: source has `locked/` at mode `000` containing `a.txt` and `b.txt`, plus `plain/` holding a
genuinely stale `stale.txt`. Destination holds the backup's copies of all of them.

```
main scan of source:   {"name":"locked","isDir":true,"children":[]}      <- no flag
main mirror plan:      delete: ["locked/a.txt","locked/b.txt","plain/stale.txt"]
MIRROR RUN on main:    ENGINE REPORTED: ok=3 fail=0
what survived:         DEST/plain/keep.txt      <- locked/a.txt and locked/b.txt GONE
```

**The mechanism:** `scan_tree` reports `children: []` for a directory it could not read, mirror mode
reads "no children" as "these files were deleted at the source", and dutifully deletes the
destination's copies. The one place the data still existed is the place that gets cleaned.

**And it reports `ok=3 fail=0`.** Nothing in the plan, the progress stream, or the result says a
directory was never looked at.

## Why this is its own ticket

**CPE-1925 (PR #1083) fixes one of three ways it fires.** Its Reviewer measured the other two still
live on that PR's branch:

| # | shape | on `main` | on #1083's branch |
|---|---|---|---|
| 1 | `read_dir` returns `Err` (mode `000` dir) | **destroys** | **fixed** |
| 2 | readable-but-not-searchable (`r--`): `read_dir` succeeds, every `entry.metadata()` fails `EACCES` | **destroys** | **still destroys**, `skippedDirs: []` |
| 3 | the **source root itself** unreadable | **destroys** | **still destroys — plans deletion of the ENTIRE destination** |

Shape 3 is the worst and is **reachable on the unattended path**: `runBackupJobNow` is fired by the
drive-connect scheduler against a stored `job.source`, and a remounted volume with different ownership
is exactly this shape. Nobody is watching that run.

Shapes 2 and 3 are being closed in **#1083 round 2**. **This ticket exists because shape 1 is on `main`
now** and will stay there until #1083 merges — and because if #1083 is ever reverted or delayed, the
defect must not lose its own record.

## What "correct" looks like — the seven-way split

`children: []` arises seven ways, and only some mean *empty*:

| # | path | must set |
|---|---|---|
| 1 | genuinely empty | nothing |
| 2 | `read_dir` returns `Err` | `unreadable` |
| 3 | depth cap reached | `truncated` |
| 4 | `read_dir` Ok, every `entry.metadata()` fails | `unreadable` |
| 5 | `read_dir` Ok, iterator yields `Err` mid-enumeration | `unreadable` — and it can yield a **partial** list, which is worse than empty, because the missing files diff as *removed* |
| 6 | all children are symlinks / fifos / sockets / devices | nothing — deliberate type filter |
| 7 | the **root** itself unreadable | must not return `Ok([])` |

**The distinction that matters is a type filter versus an access failure.** Excluding a symlink is a
decision; failing to stat an entry is an absence of information, and an absence of information must
never license a delete.

## Acceptance criteria

- [ ] **Verify against `main` first**, with the fixture above, and assert **on the filesystem** — which
      files survived — never on the `ok=N fail=N` verdict. That verdict says `ok=3 fail=0` while
      destroying data, which is the entire point.
- [ ] **Confirm all three shapes are closed** wherever the fix lands (#1083 round 2 or here), including
      case 5's **partial** list.
- [ ] **No delete may be planned from an absence of information.** State that as the invariant and pin
      it, rather than fixing the three shapes and leaving an eighth for next time.
- [ ] **Cover the unattended path.** `src/App.autoBackupConsent.test.ts` already drives the real
      `runBackupJobNow` through a drive-connect event with `scan_tree` mocked and
      `apply_backup_plan_stream` args captured — the harness exists and shape 3 is reachable exactly
      there.
- [ ] **Check the sibling engines** for the same read-absence-as-deletion inference — `revert_engine`,
      `restore_plan`, `snapshot_capture` (**CPE-1971**), and any transfer/sync path that deletes.
      Enumerate at run time (CPE-1932).
- [ ] If this closes entirely inside #1083, close this ticket **as verified there** with the
      measurements — do not close it as a duplicate without them.

## Notes

Filed 2026-08-27 by the sprint Foreman. Found by CPE-1925's worker while fixing a different defect
(backup silently dropping empty directories), and reproduced independently on disk by that PR's
Reviewer, which also measured the two surviving shapes.

Related: **CPE-1925** (PR #1083 — where it was found and where shapes 2 and 3 are being closed),
**CPE-1971** (the same empty-directory model in revert/restore/snapshot — check it for this too),
**CPE-1932** (enumerate, don't recall).
