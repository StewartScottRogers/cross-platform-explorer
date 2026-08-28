---
id: CPE-1971
title: `revert_engine` / `restore_plan` / `snapshot_capture` drop empty directories the same way backup did — and `restore_plan`'s own doc says so out loud
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Found by CPE-1925's worker (PR #1083) while sweeping the consumers that share the plan-model shape.
Backup's version is fixed there; **these three are the same defect in a separate model** and were left
deliberately, with reasons, rather than folded into a PR about a different subsystem.

- `scan_dir` records **only files**.
- `restore_plan`'s own doc comment says it in as many words: ***"Directories are implied by their
  files."*** That is true for every directory that contains one, and silently false for every
  directory that does not.

So a snapshot taken over a tree with empty directories does not record them, and a revert or restore
from it does not recreate them — **while reporting success**. CPE-1925 measured backup's version of
exactly this at `ok=3 fail=0` with **5 of 5 directories missing on disk**, on both the backup and the
restore legs.

## Why it is its own ticket

CPE-1925's fix does not transfer. Backup carries directories through `planBackup`'s single walk plus one
new engine entry kind; these three have their **own** model (`scan_dir` → `restore_plan` →
`revert_engine`), so the change is a second design, not a second call site. The worker was right not to
widen its PR — but the defect is real, measured in its sibling, and stated in this model's own doc.

## The harder half — do not skip it

CPE-1925's real work was not carrying directories; it was **telling three different `children: []`
apart**. `scan_tree` returned an empty child list for three distinct reasons and only one meant *empty*:

- genuinely empty,
- **unreadable** — the scan could not look inside,
- **truncated** — the scan stopped early.

Creating a directory for the second or third case *"asserts a fact never established."* CPE-1925 added
`unreadable`/`truncated` to `TreeNode` and gives those no `createDirs` entry, naming them in
`skippedDirs` instead, surfaced in the Dry-run preview and the unattended toast.

**And that same distinction fixed a destructive bug**: an unreadable source directory was making a
**mirror** run delete the destination's copies of everything inside it. Whatever `scan_dir` does with
an unreadable directory needs the same scrutiny — **check whether the destructive shape exists here
too, before adding anything.**

## Acceptance criteria

- [ ] **Reproduce first, end to end, with on-disk evidence.** Snapshot a tree with empty directories at
      several depths (including one whose only content is another empty directory), revert/restore, and
      **assert on the filesystem** — directories present or absent — never on a verdict enum. Report the
      `ok=N fail=N` alongside the on-disk count, as CPE-1925 did; the gap between them is the ticket.
- [ ] **Check both ends.** In backup the loss was in one shared walk used with the roots swapped; here
      the capture and the apply are separate code. Say which end loses them, or both.
- [ ] **Check for the destructive shape.** Does an unreadable directory cause deletions in any
      revert/restore/mirror path? If so **that is the priority**, it exists on `main` today, and it may
      deserve to be split out and fixed first.
- [ ] **Enumerate every way `scan_dir` can report no children**, and assign each to empty / unreadable /
      truncated. Getting this wrong either drops real directories or fabricates ones never observed.
      Reuse CPE-1925's `TreeNode` flags if the models can share them; say why not if they cannot.
- [ ] **Do not silently carry metadata.** CPE-1925's contract is existence only — default mode,
      inherited ACL — deliberately matching the file leg. Match it or argue a different one.
- [ ] **Use `open_beneath::create_dir_beneath`**, not `create_dir_all`. CPE-1925 red-proved the
      difference: the `create_dir_all` sabotage created a directory **outside the root** and returned
      `ok: true`. It also found the `..` version of that test stayed **green** under the same sabotage —
      **a shadowed guard (CPE-1929)** — and kept both tests for that reason. Expect the same trap.
- [ ] Red-proof by sabotage, and report which of your tests each sabotage reds. A sabotage that reds
      nothing means the guard is unreachable.
- [ ] Fix `restore_plan`'s doc comment either way — *"Directories are implied by their files"* should
      not survive this ticket as an unqualified statement.

## Notes

Filed 2026-08-27 by the sprint Foreman from CPE-1925's consumer sweep (PR #1083), which found these
three and correctly declined to widen its own PR into them. `transfer::download_tree`, the `archive`
zip extractor, and `compare`/`CompareDialog` were swept in the same pass and are **clean**.

Related: **CPE-1925** (backup's version, PR #1083 — the model for the reproduction, the trichotomy and
the `create_dir_beneath` red-proof), **CPE-1929** (shadowed guards — one is waiting in this area),
**CPE-1932** (enumerate, don't recall).
