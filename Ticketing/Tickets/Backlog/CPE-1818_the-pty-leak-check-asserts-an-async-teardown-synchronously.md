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
