---
id: CPE-1811
title: two S3 doc comments are falsified by the CPE-1801 fix, and the guard's own doc omits the arm it now leans on
type: task
priority: Low
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-20
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

## Work Log

**2026-08-20** — Fixed both falsified doc comments in `crates/s3/src/provider.rs`, re-reading the code
each describes rather than editing around the old wording:

- `is_safe_s3_leaf`'s own doc (was `:1148-1151`, now `:1148-1165`) claimed it checks "only what is
  actually about a leaf escaping the listed prefix" and omitted `!leaf.is_empty()` from its enumeration.
  Rewrote it to enumerate all seven arms (the four escape-related ones, the control-byte arm, the
  empty-leaf arm, and the `MAX_KEY_LEAF_BYTES` length bound) and to state the true, narrower reason the
  empty-leaf arm exists: it is not about escaping, it is about addressability — an accepted `""` would
  resolve to a self-referential row once `remote_dir_entries` calls this guard on every name.
- `is_safe_s3_leaf_rejects_the_two_arms_that_no_other_test_covers`'s doc (was `:3558-3571`, now
  `:3572-3598`) claimed disabling `parse_list_bucket_result`'s separate `if leaf.is_empty() { continue }`
  "changes nothing here". Reproduced the reviewer's red-proof directly: disabled the `Contents` loop's
  marker-arm `if leaf.is_empty() { continue; }` (line 1378 post-edit) with `if false && leaf.is_empty()`,
  ran the suite, and got a real red —
  `a_freshly_created_empty_directory_reports_nothing_filtered_so_no_phantom_hidden_entry_is_shown` failed
  with `left: 1, right: 0` (the marker started being counted into `filtered_count` instead of being
  ignored). Reverted the mutation (confirmed via `git diff --numstat` back to clean) and rewrote the
  comment: the CPE-1801-era `CommonPrefixes` copy of that check is gone, so "the latter" now unambiguously
  names the `Contents`-loop marker arm — a different, non-redundant thing — and disabling it is not inert
  in general even though it is still true that *this specific test* (which calls `is_safe_s3_leaf`
  directly, never through the parser) doesn't move.
- Grepped the module for every other mention of `leaf.is_empty()`, `filtered_count`, and
  `entries.len() + filtered_count` (~35 hits) and read each in context against the current code; all the
  others already describe the CPE-1801 state correctly (e.g. the `CommonPrefixes`-loop comment at the old
  `:1392-1401` and the round-trip test's `None` arm at the old `:7255-7264` already say the count is `1`,
  not `0`). No third stale comment found.

Gates run from `crates/s3`: `cargo clippy --all-targets -- -D warnings` — clean, 0 warnings.
`cargo test` — `test result: ok. 202 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`. Also ran
`cargo doc --no-deps`: the pre-existing 64 private-link warnings are unrelated to this change (all in
`delete`/`probe_prefix` doc links elsewhere in the file); no new warnings in the edited regions.

Scope: comments only, no behaviour change, no new dependency, no touch to the CPE-1735 delete asymmetries
or the CPE-1800 ureq migration. `crates/server` untouched, so its test gate was not required and not run
beyond what `cpe-s3`'s own dependency compile already exercised.
