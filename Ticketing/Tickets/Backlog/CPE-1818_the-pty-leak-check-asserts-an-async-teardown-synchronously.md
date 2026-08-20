---
id: CPE-1818
title: the PTY leak check asserts an asynchronous teardown synchronously, so it flakes on a loaded Windows runner
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

`src-tauri/src/pty.rs:509-510`:

```rust
drop(session); // release the last handle we hold to the child
assert!(!pid_is_alive(pid), "OS process should be gone once we drop our handle");
```

The assertion fires **immediately** after the drop. But process teardown is asynchronous — Windows does not
guarantee the PID is reaped by the time `drop` returns. On a loaded runner the check can observe the child
still alive and red.

Observed 2026-08-20 on `Backend (windows-latest)`, run `32396406159`, on a PR that touches nothing in
`pty.rs`:

```
test pty::tests::kill_reaps_the_child_synchronously_with_no_zombie_left_behind ... FAILED
panicked at src\pty.rs:510:9
test result: FAILED. 199 passed; 1 failed
```

## Why it matters

A flake that reds an unrelated PR is worse than an outright failure. It costs a CI cycle — and on this repo
the Windows leg is the ~55-minute bottleneck — and it teaches everyone to re-run rather than read, which is
exactly how a *real* red gets waved through.

It also mis-attributes. The failure lands on whoever happens to be pushing, and the natural first move is to
look for a cause in their own diff, which is not there.

## What the code already knows

The comment immediately above the line is unusually good and **already contains the diagnosis**: it states
that this line is *"a **leak check, not a kill check** … delivered by teardown, not by `kill()`"*, and that
the real kill guard is the `try_wait().is_some()` assertion earlier, which two agents measured as
deterministic by construction.

So the file already records that this assertion depends on teardown rather than on the thing the test is
named for. What it does not do is wait for that teardown.

## What to do

- **Bound-and-poll rather than assert instantly**: retry `pid_is_alive` for a short deadline (a second or
  two is generous) and fail only if the PID is still alive at the end. Keep the failure message, which is
  good.
- **Do not** simply delete the assertion. It pins a genuine guarantee — CPE-1244's module doc promises
  exactly this for the direct, non-registry path — and the comment argues correctly that isolating it
  further would mean production API surgery to serve one test.
- **Do not** paper over it with a fixed `sleep`. A sleep long enough to be reliable is long enough to slow
  every run, and it will still flake on a slower machine.
- Sweep `pty.rs` and its siblings for other assertions made immediately after a `drop` or a `kill` — this
  shape rarely appears once.

## Evidence

Per the Evidence Rules in `Ticketing/wiki.md`. **Red-proof the fix, not the flake**: make teardown genuinely
fail to reap (or stub `pid_is_alive` to stay true) and confirm the polled version still reds at the deadline
rather than hanging or passing. A retry loop that can never fail would be the wrong cure entirely.

## Notes

Filed by the Foreman during the batched sprint, 2026-08-20, after this test red-ed PR #962 — which touches
only trash listing and could not have caused it. The failed job was re-run rather than the PR being sent
back.

Related: **CPE-1244** (the guarantee this line pins).
