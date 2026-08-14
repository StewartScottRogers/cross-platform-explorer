---
id: CPE-1729
title: Splitting into a linked output directory fails with a message that names neither the path nor the real problem
type: task
priority: Low
status: Backlog
tags: ready
estimate: 30m
created: 2026-08-14
closed:
---

## This ticket was rewritten before it was ever worked — read why

**The original version described a bug that does not happen.** It was filed during CPE-1718 by reasoning
from a true observation — *"`create_dir_all` follows a link"* — one step past its evidence, and it claimed:

> a **dangling** link at `out_dir` … `create_dir_all` then creates the missing target directory and writes
> the manifest and the entire numbered part series into it … a split that reports success with its whole
> output in a directory the user never named.

**The CPE-1718 UAT measured it twice, on two dangling-link shapes** (`make_dangling_link`, and `mklink /D`
pointing at a missing target):

```
[T20] split -> Err("Cannot create a file when that file already exists. (os error 183)")
[T20] post: is_link=Ok(true)  missing_dir_created=Ok(false)  missing_census=[]
```

**The target directory is not created, nothing is written, and the split fails rather than reporting
success.** `std::fs::create_dir_all` tests `is_dir()` — which follows the link and answers `false` for a
dangling one — then calls `create_dir`, gets `AlreadyExists` because the *name* is held by the reparse
point, and returns the error. **It never walks through the link.**

*Scope of that measurement: Windows 11, two link shapes, on the CPE-1718 PR head. Not measured on Linux or
macOS.*

This rewrite is kept rather than the ticket being deleted, because "we thought this was a bug and it isn't,
here is the measurement" is worth more to the next reader than silence — and because the module doc **at
the site** was careful and correct all along. Only the ticket overstated.

## The real residual, which the original missed

`create_dir_all(out_dir).map_err(|e| e.to_string())` **discards the path**. So the user gets:

```
Cannot create a file when that file already exists. (os error 183)
```

No path. And **"file"** for what is a directory problem — the OS's wording, passed through unexamined. A
user who has pointed their output at a symlinked drive gets an error that names neither what failed nor
where.

This is a **message-quality chore, not a bug**. Nothing is lost, nothing is written, and the refusal is
correct — it just does not explain itself. Byte-identical on `main`, so it is pre-existing rather than
introduced by CPE-1718.

## Scope

`crates/server/src/split_join.rs` — the `create_dir_all(out_dir)` call and its `map_err`.

## Acceptance criteria

- [ ] The error names the **path** and describes the situation in terms the user can act on. "Cannot create
      a file" for a directory that is actually a link is three kinds of unhelpful at once.
- [ ] Distinguish the cases if it is cheap: the name is held by a **dangling link** (the measured case),
      by a **file**, or the parent is unwritable. They need different advice.
- [ ] A test pins the message, and breaking it turns a **distinct** test red, per the Evidence Rules in
      `Ticketing/wiki.md`.
- [ ] **Do not add a guard here.** CPE-1718 deliberately left this site alone and the reasoning holds:
      `create_dir_all` cannot truncate and cannot delete, and a live directory link is an ordinary way to
      name a drive — refusing would break a real use. This ticket is about the wording of an existing,
      correct refusal.
- [ ] Confirm the Linux/macOS behaviour, which nobody has measured. If `create_dir_all` there *does* walk
      through a dangling link, the original ticket was right on those platforms and this rewrite needs
      re-opening — say so loudly rather than assuming Windows generalises.

## Notes

Filed during CPE-1718 (2026-08-14), rewritten the same day by the Foreman after that PR's UAT falsified its
premise. The pattern is the one this sprint keeps hitting: **a correct observation, generalised one step
past what was measured.** It is recorded here rather than quietly corrected because the failure mode is
worth more than the ticket.

Related: **CPE-1718** (which guarded the module's four destructive primitives and deliberately left this
fifth alone), **CPE-1719** (`fs::write` follows a link and writes *through* it — the class this was assumed
to belong to), **CPE-1687** (refusals that name the wrong thing).
