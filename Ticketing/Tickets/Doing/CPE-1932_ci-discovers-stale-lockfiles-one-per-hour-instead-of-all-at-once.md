---
id: CPE-1932
title: CI discovers stale `Cargo.lock` files one per hour-long run instead of all at once — a seconds-long pre-flight would catch every one
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

This repo has **seventeen independent `Cargo.lock` files**. Nine of them pin `cpe-server`. A
dependency change in `cpe-server` therefore needs up to nine lockfiles regenerated, and CI builds
each crate with `--locked`, so a stale one is a hard failure.

The problem is not the rule — it is the **discovery cadence**. CI hits stale lockfiles **serially**,
one per job, and each discovery costs a full CI cycle (~1 hour on this repo). On CPE-1896 the matrix
named `crates/net` first; there were in fact **seven** stale:

    crates/ftp  crates/mdns  crates/net  crates/s3  crates/sftp  crates/vfs  crates/webdav

Had they been discovered one at a time, that is seven CI cycles to learn something that takes
seconds to check.

## The check already exists and is nearly free

`cargo metadata --locked` is resolve-only — **no compilation**. CPE-1896's worker ran it across all
seventeen lockfiles in seconds, including the eight that cannot be affected, precisely because
checking everything was cheaper than another round-trip to discover otherwise.

## The real lesson, worth carrying past the fix

From that worker, and it is sharper than "remember to update the lockfiles":

> Round 1 didn't break the rule, it got the **enumeration** wrong — it updated the two lockfiles it
> knew about and never asked how many existed.

Both reviewers independently verified `crates/server/Cargo.lock` and `src-tauri/Cargo.lock`, and both
were **correct**. The two everyone knew about were the two that were fine. Nothing in the process
asked *how many there are*, so nothing caught the other seven. A rule you follow from memory is a
rule you follow incompletely; make it complete by construction.

The mechanical form:

    git ls-files '*Cargo.lock'            # enumerate, do not recall
      -> grep each for the changed package
      -> cargo metadata --locked in each directory

## Acceptance criteria

- [ ] Add a fast CI job that runs `cargo metadata --locked` over **every** `Cargo.lock` in the tree
      and fails naming **all** stale files at once, not the first one reached. It must not compile
      anything — the whole value is that it finishes in seconds and runs before the expensive matrix.
- [ ] Have it enumerate lockfiles from `git ls-files` rather than a hand-maintained list, so a new
      crate is covered the day it lands. A hardcoded list would reproduce the exact defect this fixes.
- [ ] Red-proof it: stale one lockfile deliberately and confirm the job fails naming that file; stale
      two and confirm it names **both** rather than stopping at the first.
- [ ] Make the failure message say what to do — regenerate with `cargo metadata` (resolve-only), not
      `cargo update`, which would turn a build fix into an unreviewed dependency bump.
- [ ] Note the format-version trap: regenerating can silently bump a lockfile's `version = 3` to `4`
      as a side effect. CPE-1896 reverted that by hand both times. Either the guard or the docs should
      warn, so a format bump does not ride in on a build fix.
- [ ] Consider whether the same enumerate-don't-recall shape applies elsewhere — the repo already has
      a "keep five files in sync" release rule in CLAUDE.md that is maintained by memory and,
      per that document, is the one that gets missed.

## Notes

Filed 2026-08-27 by the sprint Foreman after PR #1043 spent a CI cycle discovering one stale lockfile
and would have spent six more discovering the rest. Related: **CPE-1896**, **CPE-1904**
(`package-lock.json` drift has no build-time backstop either), **CPE-1922** and **CPE-1931** (other
guards that measure less than they appear to).
