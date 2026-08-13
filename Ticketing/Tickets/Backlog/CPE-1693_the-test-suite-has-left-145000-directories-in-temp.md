---
id: CPE-1693
title: The test suite has left 145,000 directories in %TEMP% and is still adding them
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-12
closed:
---

## Problem

Found by the PR #869 reviewer while checking that PR's own cleanup, and independently re-counted by the
Foreman before filing. On this machine:

```
reviewer's count (19:0x)   144,699 cpe-* directories in %TEMP%
Foreman's count (19:2x)    145,207
```

It went up by ~500 in the minutes between the two counts, because the suite was running. **This is not a
historical mess that stopped growing; it is the current steady state.**

Worst offenders from the reviewer's breakdown:

| prefix | count |
|---|---|
| `cpe-binprev-pe-trunc` | 43,770 |
| `cpe-dotnetmeta-trunc` | 31,533 |
| `cpe-binprev-elf-trunc` | 29,298 |
| … | the balance |

Those names say what they are: per-case scratch directories from the truncation tests, one per case per
run, never removed.

## Why this is worth fixing rather than tolerating

1. **It makes "leave no orphans" unenforceable.** This sprint has repeatedly held individual tests to a
   zero-orphan standard — CPE-1678 leaked permanently-unreadable files and was fixed with a `Drop` guard;
   PR #869's new tests were checked for orphans across three separate red runs. That standard is being
   applied to new tests while ~145,000 directories from old ones sit next to them. A reviewer counting
   orphans has to filter out the noise to see the signal, which is exactly how the signal gets missed.
2. **It is a real resource problem.** Directory enumeration in `%TEMP%` degrades badly at this scale, and
   every tool that scans it — including this app's own explorer, antivirus, and backup software — pays.
3. **It hides the diagnostic value of what is left behind.** An orphan should mean something went wrong.
   When there are 145,000, it means nothing.

## Scope

`crates/server` primarily, but check every crate — the pattern is a `scratch()`-style helper that
`create_dir_all`s a uniquely-named directory and relies on a `remove_dir_all` at the end of the test, which
does not run when an assertion panics.

The fix that this repo has already converged on, twice, is a **`Drop` guard armed before the assertions**
(see `split_join.rs` and `dispatch.rs`). Applying it at the *helper* level rather than per test would fix
the whole class at once and stop the next test from reintroducing it: have `scratch()` return a guard type
that owns the directory and removes it on drop.

Consider also whether these tests need a temp directory at all — a truncation test that writes a byte
pattern and reads it back may be able to work in memory, which is faster and cannot leak.

**Clean up the existing 145,000** as part of this, and say how many were removed.

## Acceptance criteria

- [ ] A test that panics mid-assertion leaves **no** directory behind. Prove it by forcing a panic and
      counting before and after, with the real numbers in the PR.
- [ ] The fix is at the shared helper, not applied test-by-test — a newly written test that uses the
      helper cannot leak even if its author does not think about it.
- [ ] `%TEMP%` is cleaned of the existing `cpe-*` backlog, with the count reported.
- [ ] A full `cargo test` run across the workspace adds a net **zero** `cpe-*` directories. Count before,
      count after, put both numbers in the PR — this is the assertion that actually proves the class is
      closed, and it is cheap.
- [ ] Any test that *deliberately* leaves something behind (if one exists) says so explicitly.

## Notes

Filed by the Foreman from the PR #869 review, 2026-08-12, after independently reproducing the count and
watching it grow between two measurements.

Related: **CPE-1678** (the `Drop`-guard pattern this should generalise) and the Evidence Rules in
`Ticketing/wiki.md` — the guard-neutralisation rule mandates a red run per guard, which means every leaking
test leaks *by design of our own process*, once per ticket per developer.
