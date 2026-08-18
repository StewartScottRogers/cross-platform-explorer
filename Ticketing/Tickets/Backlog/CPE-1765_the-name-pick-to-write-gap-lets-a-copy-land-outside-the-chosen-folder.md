---
id: CPE-1765
title: The name-pick-to-write gap lets a copy land outside the folder the user chose (measured TOCTOU)
type: bug
priority: High
status: Backlog
tags: ready
estimate: L
created: 2026-08-17
closed:
---

## Problem

**Demonstrated**, not theorised, by the independent Security Auditor on PR #924 (CPE-1715) while auditing
that PR's symlink handling. It is **pre-existing** — CPE-1715 neither introduced nor worsened it, and that
PR correctly did not claim to fix it — but it is now measured, so it should stop being a comment.

Every name-picking write in the app is **probe-then-write, not atomic-create**:

- `do_copy_into` (`src-tauri/src/lib.rs:2576-2577`) and `do_move_into` (`:2649`, `:2662`) call
  `unique_target` to *pick* a free name, then write to that same `PathBuf` with a plain
  `fs::copy` / `fs::File::create` / `fs::rename` — no re-check, no `create_new(true)`, no lock.
- `run_transfer` (`:3282`) does the same via `resolve_conflict` + `fs::rename`, and its
  `copy_tree_streamed` / `stream_copy_file` path via `File::create`.

The code's own docs already flag this as unfixed — `crates/server/src/fsutil.rs:116-118` ("TOCTOU. Nothing
between this probe and the write is atomic… this is not a substitute for it") and `:1757` ("It is not
atomic with the primitive"). This ticket is the work those comments describe.

## The measurement

Planting a **live symlink at the picked name after the probe and before the write**:

**Copy-shaped write** (`fs::write` / `File::create` — `do_copy_into`, `stream_copy_file`): the write
**follows the link straight out of the destination folder**.

```
[SEC-AUDIT] evil_target now contains: Ok("USER CONTENT")
[SEC-AUDIT] target slot is still a symlink: Ok(true)
```

Content the user believed went to `dest\victim.txt` landed at `evil_dir\outside.txt` — **outside the folder
they chose**. The user chose the destination *folder*; anything landing outside it is the bug.

**Rename-shaped write** (`fs::rename` — `do_move_into`, `run_transfer`'s same-volume branch): does not
write through (rename does not follow the final component), but **silently destroys** the link and replaces
it, with no error:

```
[SEC-AUDIT] evil_target contents unchanged: Ok("PRE-EXISTING OUTSIDE FILE")
[SEC-AUDIT] target slot is now a symlink: Ok(false)
```

## Why High despite needing a race

It requires concurrent write access to the destination directory during the operation — but that is not an
exotic precondition for a file explorer. A shared drive, a sync client (OneDrive/Dropbox) rewriting a folder
mid-copy, an extraction into a watched directory, or any second process are all ordinary. And the failure
shape is the one this repo keeps paying for: **the operation reports success** while the bytes are somewhere
the user never chose.

## What to do

This is a class fix, not a one-liner — hence L. Sketch, not prescription:

- **Copy path:** create with `OpenOptions::new().write(true).create_new(true)` so the write is the
  existence check. `create_new` refuses to follow an existing final-component symlink and fails if the name
  appeared in the gap — turning the race into a loud, retryable error instead of an escape. Then the picked
  name and the created file are the same decision.
- **Move/rename path:** decide what `fs::rename` should do when the slot was taken in the gap. Note the
  platform split — Windows `MoveFileEx` without `MOVEFILE_REPLACE_EXISTING` fails if the target exists,
  POSIX `rename(2)` silently replaces. Both need stating and testing, per OS.
- **Do not** "fix" this by re-probing just before the write. That narrows the window and leaves the bug —
  and a narrower race is harder to test, not safer.
- Audit every sibling write site, not the three named here. `unique_target` / `resolve_conflict` /
  `probe_name_pick_slot` callers are the entry points.

## Acceptance criteria

- [ ] A link planted at the picked name **between probe and write** cannot cause a copy to write outside the
      destination folder. Demonstrate with the auditor's reproduction, showing the new behaviour.
- [ ] The move/rename case is decided, documented per-OS, and tested on all three — not left implicit.
- [ ] The failure is **loud**: a name taken in the gap surfaces an error naming the path, never a silent
      success and never a silent clobber.
- [ ] Every name-picking write site is covered or explicitly listed as out of scope with a reason. A fix at
      three sites that leaves four siblings is how this class keeps returning.
- [ ] Each test asserts the **harm** (where the bytes actually landed, whether the link survived) **before**
      unwrapping the `Result` — every defect in this family fails by succeeding, so an assertion after an
      `unwrap` is unreachable exactly when it matters.
- [ ] The two `fsutil.rs` doc comments (`:116-118`, `:1757`) that currently say this is unfixed are updated
      to describe what is now true.

## Notes

Found by the Security Auditor on **PR #924 / CPE-1715**, 2026-08-17, during the batched sprint. Related:
CPE-1715 (the dangling-link probe, which this sits underneath), CPE-1744, CPE-1758, CPE-1709 — the same
"reports success, bytes elsewhere" family.
