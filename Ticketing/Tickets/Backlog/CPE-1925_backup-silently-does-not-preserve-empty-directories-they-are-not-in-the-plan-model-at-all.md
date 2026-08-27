---
id: CPE-1925
title: backup silently does not preserve empty directories — they are not in the plan model at all
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

An empty source directory is **never backed up**, and nothing tells the user. `planBackup` /
`apply_backup_plan` carry only **file** copy/update/delete entries — there is no entry kind that
represents "this directory exists". Confirmed by grep over `src/lib/backup.ts` and by direct
observation: a 10-entry plan over a tree containing an empty directory copied all ten files
correctly and reported `ok=10 fail=0`, and the empty directory simply was not there afterwards.

Found 2026-08-27 by PR #1043's independent UAT tester, which correctly scored it **not** a defect of
that PR — the behaviour is identical on `main`.

## Why it matters

A backup that silently drops part of the tree is the same honesty failure this repo keeps finding
elsewhere: the operation says `ok`, and the result is not what the user asked for. Empty directories
are not exotic — scaffolded project folders, `logs/`, `tmp/`, mount points, output directories, and
anything whose contents are gitignored all commonly exist empty and are meaningful. Restoring a tree
and finding its structure quietly altered is worse than being told up front that directories are not
covered.

Directory **permissions and timestamps** have the same gap by implication: with no directory entry
in the plan, there is nowhere to carry them even for non-empty directories, which are currently
created implicitly as a side effect of writing the files inside them.

## Acceptance criteria

- [ ] Decide, and record, whether empty directories are **in scope** for backup. Both answers are
      defensible; silence is not. If out of scope, say so in `src/docs/safety-undo.md` and surface it
      in the UI where a user chooses a source — not only in a doc nobody opens mid-task.
- [ ] If in scope: add a directory entry kind to the plan model so directories are planned, counted,
      reported, and restored like anything else — including in the plan preview, so the user can see
      them before the run.
- [ ] Either way, make the **count honest**. A run that skips part of the tree must not report a
      clean `ok` with no mention of what was not carried.
- [ ] Mind the containment work that just landed: **CPE-1896** made every destination open atomic via
      a per-component handle-relative walk. A new directory-creation path must go through
      `open_beneath` (`mkdirat` / `NtCreateFile` with the parent handle), **not** a fresh
      `create_dir_all` by path — re-introducing a path-resolving write would re-open exactly the race
      CPE-1896 closed.
- [ ] Cover restore too: the round trip must reproduce the directory structure, not just the files.
- [ ] Pin it with a test that goes red if an empty directory stops being carried (or, if out of
      scope, one that pins the *disclosure*).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1043's UAT observation. Related: **CPE-1896** (the
atomic containment walk any new directory-creation path must use), **CPE-1890** (backup and restore
copies drop the Mark-of-the-Web), **CPE-1777** (directory-creation errors drop the path at 23 sites).
