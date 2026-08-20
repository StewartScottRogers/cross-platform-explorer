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

## Work Log

**2026-08-20** — Fixed by a sprint worker. Root cause confirmed: `PtySession::kill()` reaps the child via
`self.child.wait()` synchronously, but on Windows the OS's own process-table entry (what `tasklist`
enumerates) is scrubbed only once the last `HANDLE` closes, and `CloseHandle` schedules that deletion
without guaranteeing it completes before the call returns — so `pid_is_alive` can still see the PID for a
few milliseconds after `drop(session)` returns, worse under a loaded runner.

Added `assert_pid_reaped_within(pid, timeout, msg)` in `src-tauri/src/pty.rs` (`#[cfg(test)]` only): polls
`pid_is_alive` every 20ms against a 2s deadline instead of checking once, and only panics (with the
original message intact) if the PID is still alive when the deadline passes. Applied at both
drop/kill-then-check sites in that file:
- `kill_reaps_the_child_synchronously_with_no_zombie_left_behind` (the ticket's line 509-510)
- `close_all_kills_every_session_and_leaves_no_process_behind` (same shape: OS check right after the
  registry's `close_all()` drops its last handles)

Swept the sibling `sidecar/ai-console/src/pty.rs` (explicitly documented as mirroring this file
byte-for-byte in shape) and found the identical pattern, pre-dating this file's own CPE-1707 fix, in
`kill_reaps_the_child_synchronously_with_no_zombie_left_behind` (checked `!pid_is_alive` immediately after
`kill()`, handle still held) and `dropping_a_live_session_without_an_explicit_kill_still_reaps_the_child`
(checked immediately after `drop`). Ported the same bounded-poll helper there and applied it to both.

**Red-proof**: temporarily changed `src-tauri/src/pty.rs`'s `pid_is_alive` to `return true;`
unconditionally (production line 333, the test helper itself — the ticket's own suggested sabotage:
"stub `pid_is_alive` to stay true"), simulating a genuine leak. Both leak-check tests went red at the 2s
deadline (not a hang, not a pass) with their original failure messages intact:
```
test pty::tests::kill_reaps_the_child_synchronously_with_no_zombie_left_behind ... FAILED
test pty::tests::close_all_kills_every_session_and_leaves_no_process_behind ... FAILED
test result: FAILED. 10 passed; 2 failed; ...; finished in 2.10s
```
Reverted the sabotage immediately after confirming red; `git grep` confirms no trace of it remains.

**Gates** (both crates, both `src-tauri` feature modes):
- `cargo clippy --all-targets -- -D warnings` — clean, default features and `--features sidecar-platform`.
- `cargo test` — 200 passed (default), 255 passed (`--features sidecar-platform`), all 0 failed.
- `sidecar/ai-console`: `cargo clippy --all-targets -- -D warnings` clean; `cargo test` 382 passed, 0
  failed, 2 ignored (pre-existing, unrelated).
- 20/20 pass for the affected `pty::tests::` group in `src-tauri`, run twice: once idle, once concurrently
  with a full parallel `cargo build --all-targets -j 28` of `crates/server` in a separate target directory
  (32-core box) to reproduce the loaded-runner condition the ticket describes.
- 20/20 pass for the affected `pty::` tests in `sidecar/ai-console`.
- No frontend files touched — `npm run check`/`vitest` not run (out of scope per the gate rules).

Branch `cpe-1818-pty-leak-async-teardown`, PR opened against `main`.

**2026-08-20, round 2** — PR #967 Reviewer returned CHANGES REQUESTED with one empirically-reproduced
blocker: the 2s deadline still flaked, 1/10, under *heavier* combined contention than round 1 used
(parallel `cargo build --all-targets -j16` **and** a second competing `cargo test` loop — the sidecar's
own `pty::` group — simultaneously on the same 32-core box), panicking cleanly at 6.59s wall. Not a hang;
the polling loop worked exactly as designed, just at a deadline too short for that load level. A
build-only control run (no second test loop) came back 10/10, confirming it takes the *combined* load to
surface — consistent with `windows-latest` runners being weaker/more oversubscribed than this dev box.

Fix: widened the deadline from 2s to 10s in both files (`src-tauri/src/pty.rs` and
`sidecar/ai-console/src/pty.rs`, both call sites in each), matching this file's own existing 10s
convention for output-drain polling rather than inventing a third number. Costs nothing on the happy
path — every idle run still finishes in well under a second, since the loop returns the instant
`pid_is_alive` goes false. Rewrote `assert_pid_reaped_within`'s doc comment in both files to state the
real reasoning (a generous ceiling for a scheduling gap, not a tuned estimate; a genuine leak still reds
because a leaked process never disappears no matter how long you wait) and to record the round-1
measurement that justified 10s over a smaller number.

Also documented, per the Foreman's instruction, an accepted residual the Reviewer identified and
explicitly chose not to block on: `pid_is_alive` matches purely on numeric PID (`tasklist`/`ps`
containment) with no process-name or start-time cross-check, so it can't distinguish "our child still
alive" from "an unrelated process reused the same PID after ours exited." The pre-fix single-shot check
had a near-zero exposure window to that race; polling widens it to the full deadline, and widening the
deadline to 10s widens it further. Accepted because Windows doesn't rapidly recycle recently-freed PIDs
and this helper is test-only; closing it properly would mean cross-checking process start time or image
name (`GetProcessTimes`/`CreateToolhelp32Snapshot`, or `tasklist`'s image-name column), which is out of
scope here. Written into `assert_pid_reaped_within`'s doc comment in both files so it's a recorded,
accepted risk rather than a silent one.

**Note on test-catchability (per the Foreman's request to record it)**: the Reviewer also tried a mutation
closer to a real leak than the `pid_is_alive` stub — neutering `PtySession::kill()` and `impl Drop` to
no-ops — and `close_all_kills_every_session_and_leaves_no_process_behind` stayed **green**. Reason: Rust
runs field destructors after a custom `Drop::drop()` body returns, so even with the `Drop` body neutered,
`master`'s own destructor still runs and closes the ConPTY, which alone kills the child on Windows
(already documented in this file per CPE-1707). So a total kill/drop-suppression bug is structurally hard
to introduce in this codebase — good news for robustness, but it means the `pid_is_alive`-stub red-proof
(this ticket's method) is the practical ceiling for demonstrating this specific test shape can still fail;
a "neuter kill+Drop" mutation is not a meaningful red-proof here and shouldn't be expected to redden this
test.

**Re-verified gates after the widen** (both crates, both `src-tauri` feature modes):
- `cargo clippy --all-targets -- -D warnings` — clean: `src-tauri` default features, `src-tauri
  --features sidecar-platform`, and `sidecar/ai-console`.
- `cargo test` — `src-tauri`: 200 passed (default), 255 passed (sidecar-platform), 0 failed either way.
  `sidecar/ai-console`: 382 passed, 0 failed, 2 ignored (pre-existing, unrelated).
- **Reproduced the Reviewer's heavier condition**: parallel `cargo build --all-targets -j16` of
  `crates/server` (fresh, from a `cargo clean`) running concurrently with a second competing `cargo test
  --lib pty::` loop in `sidecar/ai-console`, both confirmed still actively running throughout via their
  background output. Against that, the `src-tauri` `pty::tests::` group ran **20/20 pass** (wall times per
  run ranged 3.5s-12.6s including cargo's own recompile/lock-wait overhead under contention; none hit the
  10s per-assertion deadline).
- No frontend touched — `npm run check`/`vitest` not applicable.
