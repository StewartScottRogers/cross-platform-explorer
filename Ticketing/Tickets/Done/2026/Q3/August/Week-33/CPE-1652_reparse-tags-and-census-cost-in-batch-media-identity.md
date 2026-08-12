---
id: CPE-1652
title: Batch Media identity — name-surrogate reparse tags, and the link-census cost cliff
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-11
closed:
---

## Problem

Two findings from the independent security review of PR #840 (CPE-1642), split out so they don't
scope-creep that fix. Neither was reproduced live; both are reasoned from the code and the Win32 docs.

### A. Reparse points `std` doesn't call symlinks are probed as themselves but followed on write

`crates/server/src/batch_media.rs:948-952` falls back to `std`'s `is_symlink()`, which recognises only
`IO_REPARSE_TAG_SYMLINK` and `IO_REPARSE_TAG_MOUNT_POINT`. The probe opens the path with
`FILE_FLAG_OPEN_REPARSE_POINT`; the subsequent **write does not**, so the write follows whatever the
tag actually does. For a **name-surrogate** tag `std` doesn't know about, the probe identifies the stub
while the write redirects elsewhere — the probe and the writer disagree again, which is the same shape
of bug as the long-path fail-open (F1) that blocked PR #840.

The existing doc comment's rationale (cloud placeholders, dedup stubs) covers **non-surrogate** tags,
which is a different set — so the comment does not actually justify the current behaviour.

Suggested direction: key off the name-surrogate bit (`tag & 0x2000_0000`) via `FILE_ATTRIBUTE_TAG_INFO`
rather than `std`'s narrow `is_symlink()`, and fail closed (`Unverifiable`) on a surrogate tag the code
does not understand.

### B. The link census has a cost cliff on large folders

`crates/server/src/batch_media.rs:849-872` does a full non-recursive `read_dir` plus one `CreateFileW`
per entry of the selected folder whenever any output is multiply-linked. It is memoized per
`ParentCache`, but `plan()` and `execute_plan_walk` build **separate** caches, so a single batch can pay
up to two full censuses. On a 100k-entry folder that is roughly 200k handle opens on a user-facing path —
against PURPOSE.md's fast/small/predictable tiebreaker.

Suggested direction: share one cache across `plan()` and `execute_plan_walk`, and add a cap that
degrades to `Containment::Unverifiable` past a threshold — staying fail-closed rather than fail-slow.

## Acceptance criteria

- [ ] A name-surrogate reparse tag the code does not understand resolves to `Unverifiable`, not to the
      stub's own identity — with a test that plants one (or, if none can be created on this machine,
      an injected-tag unit test plus an explicit note saying which real tag was not exercised).
- [ ] The probe and the writer provably agree on what a given path refers to, for every reparse shape
      the test suite can construct.
- [ ] One census per batch, not two; a stated entry-count cap past which the verdict degrades to
      `Unverifiable`; a test proving the cap fails closed rather than allowing the write.
- [ ] The `cpe_1623_plan_timing_for_2000_files` perf test stays within its ~209-219 ms budget, and a new
      measurement is recorded for a large-folder census case.
- [ ] `cargo clippy --all-targets -D warnings` clean in all three CI feature combos; crates/server suite green.

## Notes

- Source: independent reviewer/security findings F3 and F4 on PR #840 (CPE-1642), 2026-08-11.
- Related: [[CPE-1642]] output identity resolution, [[CPE-1623]] batch-media output containment,
  [[CPE-1624]] TOCTOU per-write re-check + ADS paths.
- Sequencing: land after CPE-1642. CPE-1624 touches the same files — design the two together.

## Work Log

- 2026-08-11 — Filed by the Foreman from the PR #840 security review, deliberately kept out of that
  PR's scope so the blocking long-path fail-open could land on its own.
