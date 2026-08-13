---
id: CPE-1707
title: pty::kill_reaps_the_child_synchronously flakes on Windows CI, reddening unrelated PRs
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-13
closed: 2026-08-13
---

## Problem

`src-tauri/src/pty.rs:427` â€” `kill_reaps_the_child_synchronously_with_no_zombie_left_behind` â€” failed the
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

The test spawns `ping -n 10 127.0.0.1 >NUL` on Windows, kills it, and asserts â€” with **no sleep and no
poll, deliberately** â€” that the process is both reaped and gone by the time `kill()` returns.

That is the right thing to assert (the comment says the design predates a fix that tolerated a delayed
reap, and the strictness is the point). But it makes the test sensitive to scheduler latency on a loaded
CI runner: Windows can leave the PID observable for a short window after termination, and a GitHub runner
under contention is exactly where that window widens. Our runners are heavily contended â€” this sprint has
had four PRs in CI at once for hours.

## Why it matters more than one red tick

**A flaky test that reds unrelated PRs teaches people to ignore CI.** This repo has spent two runs
building the opposite instinct â€” CPE-1677's ratchet, CPE-1679's flake root-cause, CPE-1690 and CPE-1694
getting never-run tests into CI at all. A test that cries wolf on a docs-and-workflow PR undoes that,
because the correct response ("this can't be mine") is indistinguishable from the wrong one ("CI is noise,
merge anyway").

## Scope

`src-tauri/src/pty.rs` â€” the test, not the production `kill()`.

## Acceptance criteria

- [ ] **Establish the rate before changing anything.** Run the test in a loop (100+ iterations) locally and
      on a loaded machine, and report the failure rate. CPE-1679 set the standard here: a flake fix
      without a before-number is a guess. If it will not reproduce locally, say so and use CI.
- [ ] Determine whether the flake is in `try_wait()` (reap bookkeeping) or `pid_is_alive()` (the OS still
      reporting a dying PID). **They are different bugs** and only one of them is a real defect in
      `kill()`; the other is a test asserting something the OS does not actually promise synchronously.
- [ ] **Do not simply add a sleep or a retry.** That is what CPE-1679 explicitly refused, and for the same
      reason: it hides the race and the test stops being evidence. If the OS genuinely does not promise
      synchronous PID disappearance, the assertion is wrong and should say what it can actually rely on â€”
      with the reasoning recorded, since the current strictness is deliberate and predates a real fix.
- [ ] If `kill()` itself has a genuine race, fix `kill()` and keep the assertion strict.
- [ ] After the fix, re-run at the same iteration count and report the after-number.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per
      the Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman, 2026-08-13, from an unrelated failure on PR #887. Not caused by that PR; the PR was
re-run rather than blocked on it.

Related: **CPE-1679** (the last GUI flake root-caused rather than papered over â€” read its approach),
**CPE-1677** / **CPE-1680** (the ratchet that keeps known-failing cases honest).

## Work Log

**Closed 2026-08-13, merged as PR #891 (`c82f3006`).** Four rounds; the last two were the valuable ones.

### What the flake actually was

Not a race in `kill()`. The test's own `session` kept a Win32 handle to the child open across the
assertion, and Windows keeps a process object â€” and its enumerable PID â€” alive while *any* handle
references it. `pid_is_alive()` shells out to `tasklist`, which enumerates that table. So the test was
asking an external tool to forget a PID while holding the reason it could not. Verified against vendored
`portable-pty` 0.8.1 and `filedescriptor` 0.8.3 source, not inferred: `WinChild` wraps exactly one
`Mutex<OwnedHandle>` around `CreateProcessW`'s `pi.hProcess`, no job object, no duplicate handle, and
`OwnedHandle::drop` calls `CloseHandle`.

The fix removes the cause rather than papering over it â€” `drop(session)` before the check â€” which is the
position the never-flaking sibling `close_all_kills_every_session_and_leaves_no_process_behind` was
already in.

Nobody reproduced the original flake locally: **1,000 iterations** by the worker, **420** by the UAT (idle
and under 48-thread load), **440** by the reviewer (idle and under 24-way parallelism). Treated as
corroborating, not decisive.

### The finding that mattered â€” and why one mutation was worth more than another

Both independent checks ran mutation tests and appeared to disagree:

| Mutation | Result |
|---|---|
| `kill()` returns without terminating, `try_wait()` left honest | caught **50/50** (reviewer), **1/1** (UAT) â€” at the `try_wait()` line |
| `kill()` fully inert **and `try_wait()` stubbed to lie** and `Drop` disabled | **0/90** post-drop shape Â· **20/20** pre-fix shape |

They did not disagree. The review's mutation left the measuring instrument honest, so the instrument
caught it. The UAT sabotaged the instrument too, which isolated the real question â€” *what does the
`pid_is_alive()` line contribute on its own?* â€” and answered it: **nothing**. `drop(session)` drops
`master`; `PsuedoCon::drop` calls `ClosePseudoConsole`; documented Win32 behaviour is that closing the
pseudoconsole ends any client still attached. So teardown terminates the child regardless of `kill()`.
Probed directly: dropping only `master`, with no `kill()`/`wait()` at all, left `pid 43892 alive = false`.

The reviewer then reconciled the two datasets rather than leaving the tension: `try_wait()` runs
in-process microseconds after `kill()` returns, while `pid_is_alive()` spawns `tasklist.exe` costing tens
of milliseconds. The same teardown can be invisible to the fast in-process check and already resolved by
the time the slow external one runs. Both results are consistent; there was nothing further to chase.

### Resolution

No logic change in round 4. The assertion stays â€” "nothing of the child is left behind once the session
is gone" is true and is what CPE-1244's module doc promises for the direct, non-registry path. The
restructure to isolate `kill()`'s own effect on the PID was declined: it needs the child handle released
without `master`, i.e. production API surgery to serve one test, and buys nothing because `try_wait()`
already owns that failure deterministically.

What shipped instead is the **attribution**, written into the file with the measured numbers and each
number credited to the agent that took it:

- `try_wait().is_some()` is the real `kill()` guard.
- `!pid_is_alive(pid)` is a **leak check, not a kill check**, and cannot fail while only `kill()` is broken.

Plus the note that the guard is deterministic *by construction* â€” `TerminateProcess` either fires or it
does not â€” so the counts corroborate the mechanism rather than stand in for it.

Test name left unchanged, deliberately: it does not claim `kill()` causes the zombie's absence, and with
the attribution adjacent in the comment a rename would move the honesty somewhere it could only be
hinted at. Both independent checks agreed.

### Lesson worth keeping

**A mutation test is only as strong as its willingness to break the measuring instrument.** Sabotaging the
code under test while leaving the assertion machinery intact proves the assertion machinery works â€” not
that the assertion is load-bearing. The two rounds here differ by exactly that, and only one of them found
the truth.

Verdicts: Reviewer **APPROVE**, UAT **PASS**. 13/13 CI checks green (GUI smoke windows `skipping`,
pre-existing per CPE-1594/CPE-1048).

