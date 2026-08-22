---
id: CPE-1846
title: restore can close the final-component link swap with the NOFOLLOW pattern the crate already ships
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-21
closed:
---

## Problem

CPE-1823 left one recorded residual: at the final path component, `confined_to` canonicalises and
**refuses** a planted link — the Security Auditor confirmed this with **17,488 successful symlink plants
across a live restore and zero writes through** — but the check and the subsequent `fs::copy` are not
atomic. The residual is the microseconds between them. The Auditor could not win it; it is real but narrow.

The relevant discovery is that **this crate already ships the structural fix.**

`crates/server/src/batch_media.rs:1587-1600` documents a four-step pattern. Step 2 is *never follow a
link at the final component* — `O_NOFOLLOW` on Unix, `FILE_FLAG_OPEN_REPARSE_POINT` on Windows —
hard-coded per target with **no `libc` dependency** (`:1679-1691`) and pinned by a runtime test.
`batch_execute.rs:583` already uses it.

Opening the restore target with that flag and writing through the handle, instead of `fs::copy`, closes
the final-component link swap **structurally** at both `snapshot_capture::restore` and
`revert_engine::apply_write` — without refusing a single legitimate overwrite.

## Why this is separate from CPE-1823

It changes `fs::copy`'s attribute-preserving behaviour on Windows, which is a real behavioural change
needing its own measurement and its own review. CPE-1823 correctly declined `copy_file_into_claimed_slot`
(that helper uses `create_new`, which refuses an existing name — and restore-over-a-tree and
`revert_engine`'s first-class `Overwrite` both depend on writing onto one). But that rejection covered
only **step 1** of the crate's four-step pattern. Step 2 is the half restore can actually use, because
opening an existing regular file for truncate-and-write is exactly what overwrite means.

## Acceptance criteria

- [ ] `restore` and `apply_write` open the final component with the no-follow flag and write through the
      handle. Reuse `batch_media.rs`'s existing per-target implementation — do not write a second one, and
      do not add a `libc` dependency.
- [ ] Measure and record what changes about attribute preservation on Windows versus `fs::copy`
      (timestamps, ADS, attributes, sparseness). If anything regresses, say so and decide explicitly.
- [ ] A legitimate overwrite of an existing regular file still succeeds — restore-over-a-tree and
      `Overwrite` both. This is the constraint that killed the `create_new` approach; do not reintroduce it.
- [ ] Re-run the Auditor's final-component race (a racer replacing the target with a symlink throughout a
      multi-thousand-entry restore) and report plants attempted versus writes through.
- [ ] The interior-component race remains the recorded residual either way. Say so plainly rather than
      implying the class is fully closed.
- [ ] Red-proof each new test with the minimal realistic change, observe red, revert, record the line.

## Notes

Found by the independent Reviewer during CPE-1823's round-4 review. Its exact objection is worth keeping:
the CPE-1823 code comment should not describe the residual as irreducible when `batch_execute.rs:583`
already reduces it. That wording is being corrected in CPE-1823's round 5; this ticket is the actual work.

Read CPE-1823's final Work Log first — it carries the attack record, including why `canonicalize` cannot
see a hard link and the rule that a guard belongs where callers inherit it rather than at each call site.
