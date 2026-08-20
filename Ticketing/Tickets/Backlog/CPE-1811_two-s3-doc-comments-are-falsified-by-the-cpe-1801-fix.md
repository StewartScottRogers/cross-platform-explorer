---
id: CPE-1811
title: two S3 doc comments are falsified by the CPE-1801 fix, and the guard's own doc omits the arm it now leans on
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

CPE-1801's sweep covered the **code** and missed the **comments**. Two doc comments in
`crates/s3/src/provider.rs` are now false:

1. **`:3550-3563`** — `is_safe_s3_leaf_rejects_the_two_arms_that_no_other_test_covers`'s doc points at
   "`parse_list_bucket_result`'s separate `if leaf.is_empty() { continue }`" and says disabling it
   "changes nothing here". After CPE-1801 the only such `if` left is the **`Contents` marker** arm at
   `:1364` — a different thing, which that PR's own comment works hard to keep distinct. The claim is now
   false twice over: the referent moved, and the reviewer's red-proof showed that arm *does* red the
   round-trip test.
2. **`:1148-1151`** — `is_safe_s3_leaf`'s doc enumerates its arms and **omits `!leaf.is_empty()`**, the
   arm CPE-1801's fix now leans on, and its framing ("only what is about a leaf escaping") is wrong for
   that arm. It is documented at the call site (`:1392-1401`) but not at the guard — **and an auditor
   reads the guard.**

## Why it matters

This repo has now shipped a factually wrong comment more than once, each time by restating it from memory
of its shape rather than re-checking it. The cost is specific: the next person auditing the counting
contract reads `:3550`, believes the empty-leaf arm is inert, and reasons from a premise that stopped being
true. That is precisely how CPE-1744's "abort is atomic" premise survived two tickets.

Neither is a behaviour bug, which is why CPE-1801's reviewer left them out of that PR rather than widening
it. But a wrong comment in a guard is a trap with a delay fuse.

## What to do

- Fix both comments **by re-reading the code they describe**, not by editing around the wrong words.
- While in `is_safe_s3_leaf`'s doc, make the enumeration complete rather than adding one arm — if it was
  incomplete once it is probably incomplete twice.
- Grep the module for other comments naming `leaf.is_empty()`, `filtered_count` or
  `entries.len() + filtered_count` and check each against the current code. The sweep that missed these two
  would have missed a third.

## Notes

Filed by the Foreman from the independent review of PR #959, 2026-08-20.

That review also recorded an observation worth keeping, which needs **no ticket**: a non-conforming server
that echoes the requested prefix back as its own `CommonPrefix` now adds 1 to `filtered_count` where it
previously added nothing, making an empty-directory delete refuse. That is the safe direction and is
consistent with the module's posture — recorded so it is not later rediscovered as a mystery.

Related: **CPE-1801**, **CPE-1704** (the counting contract), **CPE-1722**.
