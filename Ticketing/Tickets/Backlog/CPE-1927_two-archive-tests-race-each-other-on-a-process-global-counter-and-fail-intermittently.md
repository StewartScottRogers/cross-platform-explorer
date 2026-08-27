---
id: CPE-1927
title: two `archive.rs` tests race each other on a process-global counter and shared session root — an intermittent failure that passes on rerun
type: bug
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

A worker on CPE-1896 hit **a single `cpe-server` lib-test failure that did not reproduce in two
subsequent runs**, and could not capture the name. PR #1043's independent Reviewer went looking for
the shape and found a well-supported candidate — in a file CPE-1896 never touched.

`crates/server/src/archive.rs` has two process-global statics:

- `archive.rs:662` — `static EXTRACT_SEQ: AtomicU64`
- `archive.rs:1608` — `static SESSION_ROOT: OnceLock<PathBuf>`

libtest runs lib tests **in parallel inside one process**, so these are shared across concurrently
running tests. Two of them contend:

**`row1_a_squatted_temp_directory_is_stepped_over_not_written_into` (`archive.rs:4965`)** snapshots
`EXTRACT_SEQ` at `:4999`, then pre-creates `e{seq}` for a 64-wide block inside the **live** session
root — while other threads are incrementing that same counter and creating those same names. It
retries only `STAGE_ATTEMPTS = 5` times (`:4970`). Its own doc comment at `:4941-4943` already says
so: *"There was no announce mechanism, and `EXTRACT_SEQ` is shared with every sibling test that…"*.
A bounded retry against a counter another thread is moving is exactly "fails once in a while, passes
on rerun".

**`cpe_1786_many_extractions_add_one_directory_to_the_shared_root` (`archive.rs:5079`)** performs 25
extractions and asserts `parents.len() == 1` and `dirs.len() == 25` into the same shared session root,
against `MAX_LIVE_EXTRACTIONS = 512` (`archive.rs:1575`) — also process-global and consumed by every
concurrent extraction test.

## Candidates the Reviewer ruled out

Recorded so the next person does not re-derive them:

- `fsutil::scratch_dir` — names are already `tag-<pid>-<counter>`; verified in output.
- `shell_menu.rs:705`'s `set_var("HOME")` — guarded by `HOME_ENV_LOCK`, and Linux-only.
- `transfer.rs:1237`'s fixed `cpe-gj-base-dir` — never touches the filesystem; `guarded_join` is pure
  path math.
- `CPE_ARCHIVE_TEMP_TTL_SECS` — nothing in the suite sets it, so the aggressive foreign-session sweep
  is not in play.

## Why it matters beyond the flake

An intermittent red that passes on rerun trains the crew to re-run rather than read. This repo has
spent the night finding guards that prove nothing; a test that fails for a reason unrelated to what
it pins is the same disease from the other end — and it burns a full CI cycle (~1 hour on this repo)
each time it fires.

## Acceptance criteria

- [ ] **Reproduce it first.** Run the `archive` module's tests repeatedly (and under load — this
      machine runs a dozen agents during a sprint, which is when it fired) until it fails, and capture
      the actual failing test name and message. The Reviewer was explicit that this is the
      best-supported candidate, **not a diagnosis**; do not fix on the strength of the analysis alone.
- [ ] Fix the contention properly rather than by widening `STAGE_ATTEMPTS`. Options worth weighing:
      give the contending tests their own session root; make `EXTRACT_SEQ` reservations atomic against
      the directory creation; or serialise just these tests behind a lock the way `shell_menu.rs`
      already does with `HOME_ENV_LOCK`. Retry-count inflation hides the race, it does not remove it.
- [ ] Whatever the fix, it must go red against the racing shape — a test that only passes because the
      race is rare is not a fix.
- [ ] Sweep `crates/server` for the same pattern: any other `static` counter or `OnceLock` root that a
      test both reads and writes while siblings run in parallel.

## Notes

Filed 2026-08-27 by the sprint Foreman. Origin: an unresolved observation in CPE-1896's Work Log,
diagnosed by PR #1043's Reviewer while re-reviewing an unrelated change. `archive.rs` is **not**
touched by CPE-1896.
