---
id: CPE-1770
title: Trash restore and folder-template creation use a refusal guard that misses a dangling link
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

Found by the **PR #924 (CPE-1715) review** while auditing every "is this name free?" probe. These three are
the *refusal-shaped* siblings — they guard a write by refusing rather than by picking another name — and
each uses a guard that cannot see a dangling link.

### 1 & 2. Trash restore — `src-tauri/src/lib.rs:2092` and `:2325`

Both use `clobber_refusal` **alone**. A dangling link at the original path reads free, so `restore_all`
lands on it — restoring the user's file *through* a link to somewhere they never chose, or destroying the
link.

What makes this one worth calling out: the existing safety net does **not** catch it. The CPE-1710 clippy
ban and the `half_applied_rename_guards_are_rejected` scan both miss these sites, because the write happens
inside the `trash` crate rather than through an `fs::rename` the scan can see. The guard that was supposed
to make this class impossible has a blind spot exactly here.

### 3. Folder templates — `crates/server/src/folder_template.rs:176`

Uses `clobber_refusal` + `fs::write`. But this is a **create** site, not a clobber site, so it wants
`create_slot_refusal` (CPE-1718). Using the wrong member of the guard family means the check it performs is
not the check the call site needs.

## Why this is filed separately rather than folded into CPE-1715

CPE-1715's acceptance criteria named `unique_target` and `resolve_conflict` and it satisfied both. Leaving
these unrecorded is how the class keeps coming back — the same argument CPE-1705 makes about twelve copies
of one check.

## What to do

- Route all three through the correct refusal guard for their shape, and say in each case which one and why.
  The distinction that matters: a *clobber* site is about replacing something known to be there; a *create*
  site is about a name that must be free. They are not interchangeable.
- For the trash sites, decide what should happen when the original path is occupied by a dangling link:
  refuse and tell the user, or restore alongside under a picked name. Either is defensible; leaving it to
  land on the link is not. Record the decision.
- **Extend the scan, not just the sites.** `half_applied_rename_guards_are_rejected` missed these because
  the write is behind a crate boundary. A fix that repairs three call sites and leaves the scan blind means
  the fourth one gets missed the same way. Widen the scan to cover writes that go through the `trash` crate,
  or state explicitly why it cannot and what covers them instead.

## Acceptance criteria

- [ ] Both trash-restore sites treat a dangling link (and an NTFS junction) at the original path as
      occupied, and behave per the recorded decision.
- [ ] `folder_template.rs:176` uses `create_slot_refusal`, with the reason recorded at the call site.
- [ ] Each has a test asserting the **harm** — where the restored bytes actually landed, whether the link
      survived — **before** unwrapping the `Result`. This family fails by succeeding, so an assertion after
      an `unwrap` is unreachable exactly when it matters.
- [ ] The guard scan is widened to cover the `trash`-crate write path, or its inability to is documented
      with what covers those sites instead.
- [ ] Reverting each fix reds a **distinct** test naming the specific site and harm.
- [ ] Tests clean up via a `Drop` guard armed **before** the assertions (CPE-1693).
- [ ] The junction leg is covered, not only the symlink leg — an unprivileged Windows runner stages a
      junction, and `remove_file` refuses one with PermissionDenied while `remove_dir` succeeds.

## Notes

Found by the Reviewer on **PR #924 / CPE-1715**, 2026-08-17, during the batched sprint. Related: CPE-1715,
CPE-1769 (the name-picking siblings), CPE-1710 (the rename-guard ban with the blind spot), CPE-1718
(`create_slot_refusal`), CPE-1705.
