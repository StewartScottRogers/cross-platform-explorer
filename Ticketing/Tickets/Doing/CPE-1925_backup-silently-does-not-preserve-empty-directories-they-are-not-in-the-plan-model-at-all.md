---
id: CPE-1925
title: backup silently does not preserve empty directories — they are not in the plan model at all
type: bug
priority: Medium
status: In Progress
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

- [x] Decide, and record, whether empty directories are **in scope** for backup. Both answers are
      defensible; silence is not. If out of scope, say so in `src/docs/safety-undo.md` and surface it
      in the UI where a user chooses a source — not only in a doc nobody opens mid-task.
- [x] If in scope: add a directory entry kind to the plan model so directories are planned, counted,
      reported, and restored like anything else — including in the plan preview, so the user can see
      them before the run.
- [x] Either way, make the **count honest**. A run that skips part of the tree must not report a
      clean `ok` with no mention of what was not carried.
- [x] Mind the containment work that just landed: **CPE-1896** made every destination open atomic via
      a per-component handle-relative walk. A new directory-creation path must go through
      `open_beneath` (`mkdirat` / `NtCreateFile` with the parent handle), **not** a fresh
      `create_dir_all` by path — re-introducing a path-resolving write would re-open exactly the race
      CPE-1896 closed.
- [x] Cover restore too: the round trip must reproduce the directory structure, not just the files.
- [x] Pin it with a test that goes red if an empty directory stops being carried (or, if out of
      scope, one that pins the *disclosure*).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1043's UAT observation. Related: **CPE-1896** (the
atomic containment walk any new directory-creation path must use), **CPE-1890** (backup and restore
copies drop the Mark-of-the-Web), **CPE-1777** (directory-creation errors drop the path at 23 sites).


## Work Log

### 2026-08-27 — reproduced, then fixed. Empty directories are IN SCOPE.

**Reproduced first, end to end, on `main`, with the assertion on the filesystem.** A three-phase
harness put the **real** frontend planner between the **real** backend scan and the **real** backend
engine — `compare::scan_tree` → JSON → `src/lib/backup.ts`'s `planBackup` (run under `vite-node`, not
reimplemented) → JSON → `backup::apply_backup_plan` → `is_dir()` on the destination. The tree carried
empty directories at four depths, including one whose only content is another empty directory:

| | before | after |
|---|---|---|
| plan fields | `[copy,update,delete,unchanged]` | `[copy,update,delete,createDirs,skippedDirs,unchanged]` |
| directory entries planned | **0** | 4 (`a/empty-at-depth-2`, `b/only-an-empty-dir/leaf-empty`, `c/d/e/deep-empty`, `empty-at-depth-1`) |
| engine verdict, backup | `ok=3 fail=0` | `ok=7 fail=0` |
| **missing on disk after backup** | **5 / 5** | **0 / 5** |
| engine verdict, restore | `ok=3 fail=0` | `ok=7 fail=0` |
| **missing on disk after restore** | **5 / 5** | **0 / 5** |

The restore leg is an independent measurement, not a consequence of the backup leg: the empty
directories were planted by hand in the backup destination first, so the restore had something real
to lose. It lost all five, and reported success.

**Which end loses them: both, from one place.** The loss is in `planBackup`'s `walk`
(`src/lib/backup.ts`) — it recursed into directories and only ever pushed *file* leaves, so a
directory with no files under it produced no entry of any kind. Restore is that same planner with the
roots swapped, so it is the same defect rather than a second one. There was also a second, independent
gap behind it: `apply_backup_plan_walk` had no directory entry kind, so even a plan that carried
directories could not have created them.

**Decision: the behaviour and the silence ship together.** The plan model could carry directories
without a larger change (one list, one loop, one existing primitive), so there was no case for
shipping only a disclosure.

**What was built.**
- `BackupPlan.createDirs` — the **minimal** set: only directories that no `copy`/`update` entry would
  create as a side effect, and only the deepest path of a chain. A first full backup of a large tree
  gains a handful of entries, not one per folder.
- `apply_backup_plan[_walk]` take `create_dirs` and apply it **before** the copy loop, through
  `open_beneath::create_dir_beneath` — the CPE-1896 handle-relative walk, never `create_dir_all`.
  Each entry emits its own `OpResult`, so a directory that cannot be created is a reported per-entry
  failure and the rest of the run continues.
- `BackupPlan.skippedDirs` — the disclosure (see below), shown in the Dry-run preview and on the
  unattended auto-run toast (`notice.autoBackupSkippedFolders`, 12 locales).

**Telling a genuinely empty directory from one the scan could not see inside.** `scan_tree` reported
`children: []` for three different reasons — genuinely empty, `read_dir` refused, or the depth cap
stopped there — and only the first means "empty". Creating a directory in the destination on the
strength of the other two would be asserting a fact the scan never established. `TreeNode` therefore
gained `unreadable` / `truncated` (`Option<bool>`, skipped when `None`, so an ordinary listing gains
no bytes), carried through `DiffNode` to the planner. Directories the scan could not look inside get
**no** `createDirs` entry and are named in `skippedDirs` with the reason. Note what this does *not*
try to separate: a directory that looks empty because its only contents were symlinks or unreadable
*files* is treated as empty, deliberately — the scan looked, and those things are excluded by design,
so the destination correctly mirrors what a backup carries.

**A destructive consequence of the same ambiguity, fixed with the same field.** An unreadable source
directory came back with no children, so every file the *destination* held under it diffed as
"removed" and a **mirror** run deleted the very copies it exists to protect. Deletes are now
suppressed for the subtree of any directory the scan could not read, and the directory is disclosed.
Pinned by `never mirror-deletes the destination's copies under a source directory it could not read`.

**Directory metadata: existence only, and said so at the site.** A created directory gets the platform
default mode (`0o777 & !umask` on Unix, the parent's inherited ACL on Windows). Mode bits, owner,
timestamps, Windows attributes, xattrs and alternate data streams are **not** carried. That is
deliberately the same contract the file leg already has (`copy_one_verified` carries bytes and not
even the mtime), so this ticket does not invent a richer answer for directories than files get. It is
recorded on `apply_backup_plan_walk` and in `src/docs/safety-undo.md`, and is worth its own ticket
covering both legs together. PR #1070 (CPE-1958) was read first: it is a hard-link TOCTOU fix in
`fsutil`'s staged-write path and says nothing about metadata carryover, so nothing here contradicts it.

**Neighbours that share the plan-model shape (enumerated with `git grep`, not recalled).**

| consumer | verdict |
|---|---|
| `backup` (`planBackup` + `apply_backup_plan`) | **was broken — fixed here.** |
| `revert_engine` + `restore_plan` + `snapshot_capture` | **same defect, still open.** `snapshot_capture::scan_dir` inserts only regular files into a `Snapshot` (`BTreeMap<String, FileState>`), and `restore_plan`'s own doc says "Directories are implied by their files" — so a checkpoint does not record an empty directory and a revert cannot put one back. Separate module, separate model, separate ticket; **not** fixed here. |
| `transfer::download_tree` | **clean.** Its entry model already has `is_dir`, and it already creates such entries through `create_dir_beneath`. It is the precedent this fix follows. |
| `archive::extract_zip_archive_stream` | **clean.** Directory records are extracted through `create_dir_beneath`. |
| `compare` / `CompareDialog` | **clean.** `scan_tree` always listed directories; the folder-compare view already renders an empty one. Only the *reason* for childlessness was missing, and it is added here. |

**Also found, not fixed, stated rather than implied.** A **file→directory type change** is emitted by
`diffTrees` as `changed` with no children at all, so the whole source subtree under it is dropped from
the plan without a word. The directory now at least gets a `createDirs` entry, which the engine
refuses loudly with the file standing in the way — an improvement on silence, not a fix. And a
directory at the depth cap has its whole subtree silently not backed up; `skippedDirs` now names it,
which is disclosure, not coverage.

**Red-proof — four sabotages, all run, all measured.**
1. Engine `create_dirs` loop emptied → 3 new Rust tests red, quoting the engine's own `ok: true`
   report next to the missing directory on disk.
2. `create_dir_beneath` swapped for `std::fs::create_dir_all` → the planted-link containment test red
   with the directory created **outside** the destination root and reported `ok: true`.
   **This sabotage is why that test is not the `..` test it started as:** the `..` version stayed
   **green** under it, because `safe_join`'s textual filter answers first — a shadowed guard
   (CPE-1929) reading as coverage. Both tests are kept, separately, and say so.
3. `plan.createDirs.push` disabled → 8 frontend tests red across the planner and the dashboard.
4. The `unreadable`/`truncated` read forced to `null` → 4 frontend tests red, including the
   mirror-delete suppression.

**Checks.** `cargo clippy --all-targets -- -D warnings` clean for `cpe-server` on Linux in both
feature modes (default + `index`) and for `src-tauri` on Windows in both (default +
`sidecar-platform`); `src-tauri` clippy on Linux was not runnable here (no GTK/webkit in the WSL
sysroot) and is left to CI's matrix. `cargo test`: 2418 + 5 integration binaries green on Linux
(real ext4 — the `unreadable` test is Unix-only for the reason `fsutil::deny_dir_traversal`'s doc
records, and it is a live test rather than a skip: running as root would fail it, not pass it), 230
green for `src-tauri` on Windows. Frontend: 4997 passed / 2 skipped, `npm run check` 0 errors,
`bindings.gen.ts` regenerated.

**Ratchet.** `bidi-render-registry` 1553 → 1555, declared in `docs/design/RATCHETS.md`: two new render
sites in `BackupDashboard.svelte`, both `.length` **numbers**. The markup was reworked to keep the
raise at two rather than seven — the one value in that block that is a path goes through
`displaySafePath`. Two line-address baselines (`bidi-app-markup-offenders`,
`bidi-app-script-basename-allowlist`) and one `mojibakeGuard` anchor were re-based for shifted lines;
all three counts are unchanged.
