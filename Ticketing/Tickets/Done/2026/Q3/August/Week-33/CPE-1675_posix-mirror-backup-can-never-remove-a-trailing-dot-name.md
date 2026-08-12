---
id: CPE-1675
title: A POSIX mirror backup refuses to remove a stale trailing-dot name forever, so the job never reports clean
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-12
closed:
---

## Problem

Raised by the independent Reviewer on PR #855 while judging the Write/Delete split, and explicitly not
blocked on.

CPE-1664/1662 introduced `win32_name_is_unstable` — a name ending in a dot or a space is refused, because on
Windows those characters are stripped when the path is opened, so `dest/notes.` resolves to `dest` itself.
Round 3 scoped that refusal correctly for **writes** (Windows only, since `root/notes.` is a real, distinct,
perfectly ordinary path on Linux and macOS), but kept it for **deletes on every platform**, on the reasoning
that a refused delete destroys nothing and is a reported no-op.

The consequence: a POSIX mirror-backup job whose destination holds a stale file named `notes.` will refuse
that delete on **every run, forever**. The file is copyable but never removable through the app, and the job
never reports clean again.

The refusal buys no safety there. `contained_under` — the containment assertion that is now the actual
guarantee — already makes that delete safe on POSIX: the path resolves strictly inside the root, returns
`Ok`, and the removal proceeds correctly. So the uniform refusal costs convergence and gains nothing on the
platforms where the name is legitimate.

## Why it wasn't blocked

The direction is the safe one (a refused delete destroys nothing), the names are rare, and — the deciding
factor — it is **documented accurately rather than papered over**, in two places:
- `PlanEntry`'s doc: *"The cost is that a POSIX mirror job will report rather than remove a stale entry named
  `notes.`; renaming it clears that."*
- `src/docs/safety-undo.md`, for the user: *"The one exception is a mirror backup's delete step, which still
  declines such a name and reports it rather than removing it."*

A known, stated limit is a different thing from a silent one.

## Scope

Scope the **delete-side** refusal to `cfg!(windows)` as well, and let `contained_under` carry the guarantee
on POSIX — which it already does, and which the reviewer confirmed by mutation (neutralising
`contained_under`'s equality branch turns exactly one test red in each crate).

Then update both doc sites, since the exception they describe will no longer exist.

## Acceptance criteria

- [ ] On Linux and macOS, a mirror backup removes a stale destination entry named `notes.` or `My Report `
      and reports clean — verified by listing the directory back off disk.
- [ ] On Windows the refusal is unchanged, and the existing tests for it stay green.
- [ ] `contained_under` still refuses an entry that resolves to the root itself on **every** platform —
      neutralise it on its own and confirm a distinct test goes red.
- [ ] `PlanEntry`'s doc and `src/docs/safety-undo.md` no longer describe an exception that no longer exists.

## Notes

Filed by the Foreman from the PR #855 round-3 review, 2026-08-12.

Context worth carrying into this ticket: the trailing-dot rule exists because a security audit drove 34
spellings through `apply_backup_plan` and found five that wiped the entire backup destination —
`" "`, `"  "`, `"..."`, `". "`, `" ."`. The lesson recorded there is that **classifying path components
cannot work** (Rust treats every string that is not exactly `.` or `..` as `Normal`); the guarantee has to be
containment asserted on the *resolved* path. The name rule is only a cheap first filter in front of that, and
this ticket narrows the filter without touching the guarantee.
