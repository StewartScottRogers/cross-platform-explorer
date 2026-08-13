---
id: CPE-1717
title: Every "loud skip" notice is invisible under CI, so a platform leg can silently cover nothing
type: bug
priority: High
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Found by the PR #895 (CPE-1710) reviewer, 2026-08-13. **This undermines a pattern this sprint has been
adding deliberately across several tickets, so it matters more than its size suggests.**

Several tests stage a condition that cannot always be created — a denied stat, a symlink on an
unprivileged Windows runner — and, when staging fails, print a **loud skip notice** via
`writeln!(stderr)` rather than passing silently. CPE-1705 introduced the pattern; CPE-1710 copied it. The
whole point is that a test must never degrade into proving nothing while still showing green.

**libtest captures stderr for passing tests, and CI never asks for it.** `.github/workflows/ci.yml` runs
`cargo test` for `crates/server` with **no `--nocapture`** — grep confirms `--nocapture` appears only on
the `--ignored` network and keyring legs.

So the sequence is:

1. A Windows runner loses both symlink privilege and junction creation (or an ACL deny stops taking effect).
2. The tests stage nothing, print their notice to a captured stderr, and **pass**.
3. The leg reports green. **Nobody ever sees the notice.**
4. That platform's coverage is zero, and the dashboard says otherwise.

This is precisely the failure mode the family "has spent six rounds on", written into the doc comments as
solved. The claim — *"a loud `writeln!(stderr)` skip, never a silent pass"* — is **true locally and false
under the harness that actually runs it.**

## Scope

`.github/workflows/ci.yml`'s `cargo test` invocations, and every skip notice added by **CPE-1705** and
**CPE-1710** in `crates/server` and `src-tauri`.

## The decision to make

Two shapes, and the second is stronger:

- **(a) Add `--nocapture` to the CI test steps.** One-line, makes every notice visible. Cost: it also
  un-captures every other test's output, which is noisy on a 2,100-test suite and may bury the thing you
  are looking for.
- **(b) Make an uncoverable platform leg *fail* rather than skip**, at least on the platforms where the
  staging is supposed to work. A skip is right on a platform where the condition genuinely cannot exist
  (an ACL test on Linux); it is wrong on Windows, where failing to stage means the runner changed under us
  and we should find out loudly.

(b) is closer to the sprint's own standard: a test that cannot verify its subject should be **red**, not
quiet. (a) may still be worth doing alongside for the genuinely-not-applicable cases.

There may be a third option worth considering: have the skip path record into a machine-readable artifact
the CI step checks, so a skip is visible without un-capturing everything.

## Acceptance criteria

- [ ] A test that cannot stage its condition on a platform where it is *supposed* to work is **visible in
      CI** — either red, or with the notice actually reaching the log. Prove it: force the staging to fail
      and show what CI reports.
- [ ] Enumerate every skip notice currently in the tree (CPE-1705 and CPE-1710 at minimum) and state, for
      each, which platforms it may legitimately skip on and which it must not.
- [ ] The `#[cfg(not(windows))]` skip notices that are *correctly* skipping — an ACL test on Linux — keep
      working and do not become noise or false failures.
- [ ] Record the choice and the reasoning. If `--nocapture` is rejected for noise, say so; if it is
      accepted, check it does not push the log past any size limit on the 3-OS matrix.
- [ ] Breaking the mechanism turns a **distinct** test or CI step red, per the Evidence Rules in
      `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #895 review, 2026-08-13.

**High** despite being small: it silently voids coverage the last two tickets were specifically built to
add, and it does so on the platform (Windows) where most of this family's bugs actually live. Every
"assert your own premise" test added this sprint is only half-working until this is fixed — the assertion
fires, and then nobody hears it.

Related: **CPE-1705** (which introduced the pattern), **CPE-1710** (which copied it), **CPE-1694** (an
earlier instance of tests that never gated CI at all).
