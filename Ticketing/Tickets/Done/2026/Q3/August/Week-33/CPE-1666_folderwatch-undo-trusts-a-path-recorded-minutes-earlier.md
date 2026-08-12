---
id: CPE-1666
title: Folder-watch Undo deletes a path recorded minutes earlier without re-checking it, and can take the recursive branch
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Found by the independent Security Auditor on PR #844, verified mechanically as
`probe_swapping_the_recorded_copy_for_a_real_directory_recursively_wipes_it`.

`folderWatch.ts`'s `undoFire` (`src/lib/folderWatch.ts:159-167`) is one of the three call sites allowed to
pass `confirmed: true` to `delete_permanent`. The justification — that `plan.deletes` only ever holds
copies the rule itself created — **is correct at fire time**; the auditor attacked that claim directly and
could not break it (`do_copy_into` uses `src.file_name()` with separators stripped, then `unique_target`,
so a copy never resolves onto an existing entry, and `watchLog` is in-memory only, never persisted or
parsed).

The gap is *when* the path is used. It is recorded at fire time and consumed at undo time — a user gesture
that may be minutes later — with **no re-stat in between**, and `delete_permanent_impl`
(`src-tauri/src/lib.rs:2205-2208`) dispatches on `path.is_dir()` → `remove_dir_all`.

Sequence: a rule copies `invoice.pdf` → `D:\Archive\invoice.pdf` and records the fire. Before the user
presses Undo, anything with write access to `D:\Archive` deletes that file and renames a real directory
tree into its place. Undo then takes the recursive branch and destroys it.

Rated **LOW** by the auditor: the attacker needs write access both to the destination folder and to the
tree they redirect, so it buys little they don't already have. It is filed anyway because "Undo deletes
something the app did not create" is exactly the assumption the code comment asserts cannot happen.

## Scope

1. In `undoFire`, re-stat each recorded delete before acting and **skip anything that is not a regular
   file** — the cheapest form is an `entryInfo` call refusing `is_dir` (and, ideally, refusing a symlink).
   A recorded copy is always a regular file; if it is now a directory, the thing being undone is not the
   thing that was done.
2. Update the comment at `folderWatch.ts:161-165` so it claims what is actually true: app-created *at fire
   time*, re-verified at undo time.

Already fixed in PR #844, so **not** in scope here: `undoFire` was discarding `deletePermanent`'s `Result`,
so a backend refusal or a per-path failure vanished silently. It now warns on both.

## Acceptance criteria

- [ ] A recorded copy path that has become a directory by undo time is **skipped**, with a warning, and its
      tree is intact afterwards — verified by listing the directory back off disk.
- [ ] A normal undo still deletes the app-created copies and moves the source back.
- [ ] Removing the new re-stat turns a test red.

## Notes

Filed by the Foreman from the PR #844 security audit, 2026-08-12. Two related, higher-priority items from
the same audit are **CPE-1664** (`apply_backup_plan`) and **CPE-1665** (`run_command`); **CPE-1662**
(`start_transfer` Overwrite) came from the same PR's review.

One stale comment worth fixing while in the area, also from this audit: `RepairLinkDialog.svelte:64-66`
claims Rust's `remove_dir_all` follows a junction. It does not — the auditor planted an NTFS junction at a
delete target and watched `remove_dir_all` unlink the junction while leaving the target directory and its
files intact (same for a directory symlink). The re-check that comment justifies is harmless, but the
reasoning under it is wrong.
