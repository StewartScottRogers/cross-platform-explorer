---
id: CPE-1785
title: empty_trash_purges_only_the_selected_probe_item panics inside the trash crate on Linux CI
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

`empty_trash_purges_only_the_selected_probe_item` (`src-tauri/src/lib.rs:14475`) failed on
`Backend (ubuntu-latest)` during CI for PR #935, with a panic raised **inside the dependency**, not in
our assertion:

```
thread 'tests::empty_trash_purges_only_the_selected_probe_item' (22815) panicked at
  .../trash-5.2.6/src/freedesktop.rs:140:42
test result: FAILED. 186 passed; 1 failed
```

PR #935 changes **only** `.github/workflows/ci.yml`, `.github/workflows/gui-smoke.yml` and
`gui-smoke/wdio.conf.ts`. It contains no Rust change of any kind, so it cannot have caused this. The same
test passed on `Backend (ubuntu-latest)` for PRs #936 and #937 in the same window.

## Why this is worth a ticket rather than a re-run

This is the **CPE-1693 shape again**, in a different resource: a test that shares mutable global OS state
with every other test running in parallel, fails non-deterministically, and passes on re-run. That last
property is the dangerous one — it teaches whoever sees it to press re-run rather than read the failure.

The test deliberately operates on the **real** OS trash (its own comment explains why it never calls
`empty_trash_impl(None)`: on a developer machine that would purge the actual Recycle Bin). On Linux that
means the shared XDG trash directory — `$XDG_DATA_HOME/Trash/{files,info}` — which is process-global and
shared by every other trash test in the same binary, all running concurrently under the default test
harness.

`freedesktop.rs:140` is in the trashinfo-parsing path. A panic there, at a column consistent with an
unchecked index/parse, is what you would expect when a `.trashinfo` file is enumerated and then read
after a **concurrent** test has removed or rewritten it — a classic list-then-read race on shared state.
The dependency panicking rather than returning an `Err` means we cannot distinguish "environment raced"
from "our input was malformed" at the call site today.

## What to do

- Reproduce it deliberately rather than waiting for it: run the trash tests in a tight loop under
  parallelism (`cargo test --lib trash -- --test-threads=8`, repeated) on Linux, and confirm the race.
- Serialise the tests that touch the real OS trash against each other. A shared mutex around the
  trash-touching tests is the smallest honest fix; a per-test `XDG_DATA_HOME` pointed at a scratch
  directory is the better one where the trash crate honours it, because it removes the sharing entirely
  instead of just scheduling around it.
- If `XDG_DATA_HOME` redirection works, prefer it — it also stops these tests from touching the
  developer's real trash on a Linux workstation, which is the same objection the test's own comment
  raises for the Windows Recycle Bin.
- Whatever the fix, **prove it red first**: demonstrate the race reproducing before the change and not
  after, per the Evidence Rules in `Ticketing/wiki.md`. A green run proves nothing here — the bug's whole
  character is that it is usually green.
- Consider whether `list_trash_impl` should tolerate an entry vanishing mid-enumeration (skip-on-error,
  the same policy `list_dir` already follows per `CLAUDE.md`) rather than propagating a dependency panic.

## Notes

Filed by the Foreman from PR #935's CI, 2026-08-19, after confirming the PR contains no Rust change and
that the same test passed on two sibling PRs in the same window.

Related: **CPE-1693** (the shared-temp-state family this belongs to), **CPE-1268** (the
`trash_roundtrip_available()` environment gate this test already carries).
