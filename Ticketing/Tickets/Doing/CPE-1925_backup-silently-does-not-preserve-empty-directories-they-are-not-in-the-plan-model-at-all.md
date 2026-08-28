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
covering both legs together.

**PR #1070 (CPE-1958), stated correctly this time.** Round 1 said it "says nothing about metadata
carryover". That is wrong, and CPE-1933 is the reason it matters: #1070 adds a whole `HandleCarryover`
type in `fsutil` that carries the DACL (with its `SE_DACL_PROTECTED` control), the named/alternate data
streams and the attribute word across its staged rename, read off the destination **handle** via
`GetKernelObjectSecurity` and `ReOpenFile` + `BackupRead`. What it carries is the **destination's own
pre-existing** metadata across a `create_new`-then-rename staging step, so that a confirmed overwrite
does not silently reset the file's ACL and streams. It is not source→dest carryover, it is not on the
backup engine's path, and it does not give `copy_one_verified` a metadata leg. So the gap recorded above
is real and #1070 does not close it — but the two are adjacent enough that "says nothing" would send the
next reader looking in the wrong place.

*File overlap, checked rather than asserted:* both PRs touch `src-tauri/src/lib.rs`. The hunks are
disjoint — this PR at `@@ -4455 / -4468 / -4495 / -4503 / -4513` (the `apply_backup_plan[_stream]`
dispatchers), #1070 at `@@ -7160 / -7219` (`macro_preflight_collisions` / `macro_apply_run`) — so git
merges them, but "no file overlap" was the wrong claim and the CPE-1938 duplicate-import trap is what
that claim is normally protecting against.

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

---

### Round 2 — 2026-08-27. The enumeration was three-way; it is seven-way, and three of the four missed cases were destructive.

Round 1 fixed one of the ways a mirror backup deletes data it was built to protect. The reviewer
enumerated the rest. The table now lives on `TreeNode` in `crates/server/src/compare.rs` rather than
only in a PR body:

| # | how a directory's `children` came up short | flag | round-1 verdict |
|---|---|---|---|
| 1 | genuinely empty on disk | none | correct |
| 2 | `read_dir` itself returned `Err` (`d---------`) | `unreadable` | correct |
| 3 | `depth_left == 0` | `truncated` | correct |
| 4 | `read_dir` Ok, **every** `DirEntry::metadata()` failed (`dr--------`) | `unreadable` | **none — destructive** |
| 5 | `read_dir` Ok, iterator yielded `Err` mid-enumeration | `unreadable` | **none — destructive, and PARTIAL** |
| 6 | every child is a symlink / fifo / socket / device | none | correct — a type filter, not an access failure |
| 7 | the **root** handed to `scan_tree` is itself unreadable | `Err` | **silent `Ok([])` — destructive over the whole destination** |

**Case 4 (blocker 2a) — closed.** A `dr--------` directory has the read bit but not the search bit, so
`read_dir` *succeeds*, every name is listed, and every `metadata()` (an `fstatat` needing search) is
refused. Round 1's `let Ok(meta) = ... else { continue }` dropped them all and then reported the
directory readable-and-empty. `scan_children`'s second return value is now `whole_truth` — cleared by
an entry dropped for an **error**, never by one dropped for its **type** (case 6, which must stay
unflagged or every directory holding a symlink reads as unknown and the flag means nothing anywhere).

**Case 5 — handled, and it is the nastiest.** It is the only one whose `children` can be *non-empty*:
a partial listing looks like an ordinary directory, and the files missing from it diff as "removed
from the source", which in a mirror run means *delete the backup's only copy*. `unreadable` is
therefore documented as "**this list is not the whole truth**", not "children is empty". There is no
portable fixture — you cannot make `readdir` fail an entry on demand — so its flag-setting arm is
shared, in three visible lines, with case 4 (which has one), and the *consequences* are pinned in
`backup.test.ts` on the exact node shape case 5 emits: the listed files are still copied, nothing
unlisted is deleted, and the directory earns no `createDirs` entry.

**Case 7 (blocker 2b) — closed, and it was the largest blast radius.** `scan_tree` computed the very
bool this PR introduced and threw it away (`Ok(scan_children(p, max_depth).0)`). `p.is_dir()` stats
the root through its *parent* and succeeds, so nothing upstream noticed. An unlistable root is now an
`Err`, joining the `Err` already returned for "not a folder": `BackupDashboard` shows it on the row,
`runBackupJobNow` raises the failure toast, `CompareDialog` falls through to file compare, the
saved-search loader yields nothing. None of them deletes anything. Below the root the behaviour is
unchanged — there the flag has a node to live on and the rest of the tree is still worth scanning.

**On-disk proof, real ext4 (WSL), real `scan_tree` then real `planBackup` then real
`apply_backup_plan`.**

*2a — source `nosearch/` at `0o400`, destination holding `nosearch/a.txt`:*

| | scan of source | mirror plan | engine | destination afterwards |
|---|---|---|---|---|
| round 1 | `{"name":"nosearch","children":[]}` | `delete:["nosearch/a.txt"]`, `skippedDirs:[]` | `ok=1 fail=0` | `["keep.txt","nosearch/"]` — **a.txt gone, source still has it** |
| now | `{"name":"nosearch","children":[],"unreadable":true}` | `delete:[]`, `skippedDirs:[{nosearch,unreadable}]` | `ok=0 fail=0` | `["keep.txt","nosearch/","nosearch/a.txt"]` — **survived** |

Not a no-op: with a genuinely stale file added at the destination root, the same plan still emits
`delete:["truly-stale.txt"]` alongside the disclosure.

*2b — source root at `0o000`, destination holding `a.txt` + `b.txt`:*

| | scan of source root | mirror plan | engine | destination afterwards |
|---|---|---|---|---|
| round 1 | `[]` | `delete:["a.txt","b.txt"]` | `ok=2 fail=0` | `[]` — **entire destination deleted, source still has both** |
| now | `Err: ... could not be listed completely, so this scan cannot say what is in it` | never built | never ran | untouched |

**Red-proofs, all run, all recorded at the site rather than only here.**
5. Round-1 arm restored in `scan_children` (an error-dropped entry back to a bare `continue`) —
   **1 red**, `scan_tree_marks_a_listable_but_unstatable_directory_as_unreadable`, on `unreadable:
   None` with `children=Some(0)`. The three older flag tests all stayed green; none could reach it.
6. Root refusal disabled (`if false && !whole_truth`) — **1 red**,
   `scan_tree_refuses_an_unlistable_root_instead_of_calling_it_empty`, "got Ok with 0 node(s)".
7. `createDirs: plan.createDirs` in `unattendedBackupArgs` forced to `[]` — **2 red**, one of them the
   new unattended-path test.
8. The `skipped` disclosure dropped from `runBackupJobNow`'s toast — **1 red**, the new one.
9. The partial-list materialisation reverted to round 1's `walk(...); if (inDest)` — **1 red**, a
   `createDirs` entry for a folder a real file copy was already going to create.

**Two corrections to round 1's sabotage record, both now written at the site.** Sabotage 1 (engine
`create_dirs` loop emptied) reds **four** tests, not three — the planted-link test also reds, on
`results.len()`. And the textual-filter test passed that sabotage **vacuously**:
`results.iter().all(|r| !r.ok)` is trivially true over an empty vec. It now asserts `results.len() ==
2` first, and says why.

**The untested surface round 1 argued mattered most is now tested.** `App.autoBackupConsent.test.ts`
drives the real `runBackupJobNow` through a drive-connect event; three cases were added there —
`createDirs` reaching the backend, the skipped-folder disclosure reaching the toast, and a source root
whose scan fails stopping the run rather than mirroring an empty tree. Its poll helper now steps one
second at a time instead of one twenty-second jump — same elapsed time, but the notice auto-clears
after 5s and a single long advance fired that clear inside itself, so no assertion could ever have
seen a toast.

**PR #1070 claim corrected** — see the metadata paragraph above. #1070 *does* add metadata carryover;
what it carries is the destination's own pre-existing DACL and alternate data streams across a staged
rename, in `fsutil`, not source-to-dest on the backup path. The conclusion (this gap is real and open)
is unchanged; the reasoning was wrong, which is CPE-1933 territory. "No file overlap" was also wrong:
both PRs touch `src-tauri/src/lib.rs`, in disjoint hunks (~4455-4519 here, ~7160/7219 there).

**RATCHETS.md merge hazard closed.** The enumeration table's `today` column for `bidi-render-registry`
said `1552` — already stale at the base, and PR #1081 (CPE-1948, approved and unmerged) adds
`ratchetsDoc.test.ts` asserting that column against the live measurer, so whichever merged second
would have gone red. Set to `1555`, which is what `node scripts/ratchet-baselines.mjs print` reports
on this branch. The declared raise row is unchanged.

**Checks (round 2).** `cargo clippy --all-targets -- -D warnings` clean for `cpe-server` on Linux in
both feature modes (default + `index`) and for `src-tauri` on Windows in both (default +
`sidecar-platform`). `cargo test`: **2421** lib tests + the integration binaries green on Linux (real
ext4, and no `SKIPPING` notices — the three new Unix-only tests are live, not skipped), 230 green for
`src-tauri` on Windows. Frontend: **5025 passed / 2 skipped** (was 4997/2), `npm run check` 0 errors.
`bindings.gen.ts` regenerated: `TreeNode`'s doc comments flow into it, so the doc change alone would
have failed CI's typed-bindings drift guard.
