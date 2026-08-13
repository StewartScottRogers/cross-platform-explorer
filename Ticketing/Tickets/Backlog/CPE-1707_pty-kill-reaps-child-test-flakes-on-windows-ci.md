---
id: CPE-1707
title: pty::kill_reaps_the_child_synchronously flakes on Windows CI, reddening unrelated PRs
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

`src-tauri/src/pty.rs:427` — `kill_reaps_the_child_synchronously_with_no_zombie_left_behind` — failed the
**Backend (windows-latest)** leg on PR #887, a PR whose entire diff is
`.github/workflows/gui-smoke.yml`, `gui-smoke/README.md`, `gui-smoke/specs/samples.smoke.ts`,
`gui-smoke/wdio.conf.ts`, `samples/README.md` and `samples/audio/corrupt.mp3`. **Nothing it touches can
reach `pty.rs`.**

```
test pty::tests::kill_reaps_the_child_synchronously_with_no_zombie_left_behind ... FAILED
thread '...' panicked at src\pty.rs:442:9
test result: FAILED. 159 passed; 1 failed
```

Line 442 is one of the two post-kill assertions:

```rust
session.kill().unwrap();
// No sleep/poll: kill()'s own follow-up wait() must have already reaped it.
assert!(session.try_wait().is_some(), "child was not reaped synchronously by kill()");
assert!(!pid_is_alive(pid), "OS process should be gone right after kill()");
```

The test spawns `ping -n 10 127.0.0.1 >NUL` on Windows, kills it, and asserts — with **no sleep and no
poll, deliberately** — that the process is both reaped and gone by the time `kill()` returns.

That is the right thing to assert (the comment says the design predates a fix that tolerated a delayed
reap, and the strictness is the point). But it makes the test sensitive to scheduler latency on a loaded
CI runner: Windows can leave the PID observable for a short window after termination, and a GitHub runner
under contention is exactly where that window widens. Our runners are heavily contended — this sprint has
had four PRs in CI at once for hours.

## Why it matters more than one red tick

**A flaky test that reds unrelated PRs teaches people to ignore CI.** This repo has spent two runs
building the opposite instinct — CPE-1677's ratchet, CPE-1679's flake root-cause, CPE-1690 and CPE-1694
getting never-run tests into CI at all. A test that cries wolf on a docs-and-workflow PR undoes that,
because the correct response ("this can't be mine") is indistinguishable from the wrong one ("CI is noise,
merge anyway").

## Scope

`src-tauri/src/pty.rs` — the test, not the production `kill()`.

## Acceptance criteria

- [ ] **Establish the rate before changing anything.** Run the test in a loop (100+ iterations) locally and
      on a loaded machine, and report the failure rate. CPE-1679 set the standard here: a flake fix
      without a before-number is a guess. If it will not reproduce locally, say so and use CI.
- [ ] Determine whether the flake is in `try_wait()` (reap bookkeeping) or `pid_is_alive()` (the OS still
      reporting a dying PID). **They are different bugs** and only one of them is a real defect in
      `kill()`; the other is a test asserting something the OS does not actually promise synchronously.
- [ ] **Do not simply add a sleep or a retry.** That is what CPE-1679 explicitly refused, and for the same
      reason: it hides the race and the test stops being evidence. If the OS genuinely does not promise
      synchronous PID disappearance, the assertion is wrong and should say what it can actually rely on —
      with the reasoning recorded, since the current strictness is deliberate and predates a real fix.
- [ ] If `kill()` itself has a genuine race, fix `kill()` and keep the assertion strict.
- [ ] After the fix, re-run at the same iteration count and report the after-number.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per
      the Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman, 2026-08-13, from an unrelated failure on PR #887. Not caused by that PR; the PR was
re-run rather than blocked on it.

Related: **CPE-1679** (the last GUI flake root-caused rather than papered over — read its approach),
**CPE-1677** / **CPE-1680** (the ratchet that keeps known-failing cases honest).
