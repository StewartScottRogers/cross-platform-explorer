---
id: CPE-1785
title: empty_trash_purges_only_the_selected_probe_item panics inside the trash crate on Linux CI
type: bug
priority: Medium
status: Done
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

## Work Log — 2026-08-19, branch `CPE-1785-serialise-shared-os-trash-tests`

### The fix

`lock_real_trash()`, an RAII guard held for the duration of every real-OS-trash call site (five
`#[test]` functions plus the shared `trash_roundtrip_available()` helper they all call first — six
call sites in total, sharing one process-global resource):

- Serialises them against each other on every platform, via a named poison-tolerant mutex
  (`TRASH_ENV_LOCK`, matching `crates/server/src/shell_menu.rs`'s `HOME_ENV_LOCK`).
- On Linux, additionally redirects `XDG_DATA_HOME` to a private
  `cpe_server::fsutil::scratch_dir` for the guarded section — the `trash` crate's `home_trash()`
  honours it (`freedesktop.rs:688-701`) — removing the sharing entirely instead of merely
  scheduling around it, and stopping the suite from ever touching a real Linux developer's actual
  trash.
- `trash_roundtrip_available()` now takes `&TrashTestGuard` (compiler-enforced ordering) and, on
  Linux, asserts the listed probe's `.trashinfo` path sits inside the redirected scratch directory,
  so a silent degrade back to the shared trash (e.g. `scratch_dir` and `tempfile::tempdir()`
  landing on different mounts) fails loudly rather than passing from the wrong directory.
- The three `env::set_var`/`remove_var` sites are `unsafe` with `// SAFETY:` comments matching
  `shell_menu.rs`'s precedent.

PR: #940. Commits: `edf68487` (initial fix), `b2717976` (PR review round 1: guard-typed roundtrip
check, redirect assertion, unsafe+SAFETY, field order), plus throwaway diagnostic commits below.

**Considered and rejected:** teaching `list_trash_impl` to catch the dependency's panic
(`catch_unwind`) and skip-on-error, matching `list_dir`'s policy. The production exposure this
would close is real — one malformed `.trashinfo` file still panics `list_trash_impl` on a real
user's machine — but it's out of scope for this test-only ticket. Tracked separately as
**CPE-1791**.

### Red-first evidence (Ticketing/wiki.md Evidence Rules)

**Natural (non-fault-injected) reruns of pre-fix `main` (`17513930`), Backend (ubuntu-latest)
only:** 12 independent CI job executions (1 original + 11 reruns via `gh run rerun --job`), 11
valid (1 excluded — `cancelled` by an unrelated concurrency-group collision, not a test outcome).
**0/11 reproduced the panic.** Run IDs: 96264930490, 96280319772, 96284882057, 96286216187,
96286976528, 96287744843 (cancelled, excluded), 96288231164, 96289247018, 96291246834,
96292879000, 96293797923, 96295386250.

This is an honest null result, not proof of absence: the ticket's own account is that the same
test passed on two of three sibling PRs in the same window, i.e. the race is rare even
unforced. A dozen unforced reruns is not the right instrument for a rare race — hence the
fault-injected diagnostic below, per the PR #940 review (blocker 1).

**Fault-injected diagnostic, per Evidence Rule 1 (break one guard at a time):** a temporary
`#[cfg(target_os = "linux")]` test, `cpe_1785_diagnostic_stress_loop_temporary` (commit
`b2717976`, not `#[ignore]`d so the existing unfiltered `cargo test` CI step runs it
automatically), shells out to a fresh `cargo test --lib trash -- --test-threads=8` 50 times per CI
job and tallies pass/fail via direct (capture-proof) stderr writes.

- **Variant A** (commit `611f0a71`, throwaway): mutex neutralised (each `lock_real_trash()` call
  locks its own private, unshared `Mutex` instead of the shared `TRASH_ENV_LOCK` static), redirect
  kept. Run `32327426732`, `Backend (ubuntu-latest)`, commit `f6341a43` (the code from `611f0a71`
  carried forward under a new commit sha after an unrelated ticket-only push restarted CI on this
  ref — see below). **Result: REPRODUCED DECISIVELY — 33/50 nested runs failed (66%).**

  ```
  CPE-1785 STRESS RESULT: 33/50 nested runs failed
  test result: FAILED. 187 passed; 2 failed
  ```

  And the failure carries the exact mechanism, not just a bare panic — the blocker-2 assertion
  fired and named it:

  ```
  thread 'tests::macro_run_convert_step_then_undo_restores_the_original_bytes_via_trash'
  panicked at src/lib.rs:14279:13:
    the probe's trashinfo file (/home/runner/.local/share/Trash/info/cpe-trash-roundtrip-probe.tmp.trashinfo)
    is not inside the redirected scratch trash (/tmp/cpe-1785-trash-xdg-5935-3)
    — XDG_DATA_HOME redirection silently did not take effect, so this test just touched the
    REAL shared OS trash (CPE-1785, PR #940 review, blocker 2)
  ```

  Two distinct failure shapes were observed across the 33: one run's redirect landed on the
  runner's own real trash (`/home/runner/.local/share/Trash`) instead of any scratch directory at
  all; another landed in a *different concurrently-running test's* private scratch directory
  (`.../cpe-1785-trash-xdg-6670-4` while its own was `...-6670-3`). Both are exactly what
  `std::env::set_var` being process-global predicts without the mutex: two concurrent tests race
  to set `XDG_DATA_HOME` between one test's `set_var` and its own later read, so the loser silently
  operates on the wrong directory — either the real shared trash, or another test's directory —
  which is the same list-then-read race the ticket started from, now reproduced on demand instead
  of waited for.

  **For context on why fault injection was necessary:** this exact race resisted 46 *unforced*
  attempts earlier the same day — this worker's 12 natural `gh run rerun` attempts against pre-fix
  `main` (0/11 valid reproduced), plus an independent UAT's 6 fresh Linux pre-fix reruns and 40
  Windows iterations, all clean. One fault-injected CI run found what a day of unforced repetition
  could not.

- **Variant B** (commit `35e608f1`, throwaway): redirect neutralised (the scratch directory is
  still created but `XDG_DATA_HOME` is deliberately left unset), mutex kept (restored to the real
  shared `TRASH_ENV_LOCK` via `git checkout b2717976 -- src-tauri/src/lib.rs` before applying B).
  Intended to answer: with tests serialised but still sharing the one real OS trash, does the
  original `freedesktop.rs:140` panic return? Run `32329693244`:

  ```
  CPE-1785 STRESS RESULT: 50/50 nested runs failed
  test result: FAILED. 183 passed; 6 failed
  ```

  **This result is confounded and does NOT show the redirect is independently necessary to
  prevent the race — the PR #940 review caught this before it was mis-banked as evidence.**
  Counting the failure modes in the log: **260** occurrences of *"the probe's trashinfo file … is
  not inside the redirected scratch trash — XDG_DATA_HOME redirection silently did not take
  effect"* (the blocker-2 assertion), and **0** occurrences of `freedesktop.rs` — the original
  panic never appeared once. Commenting out `set_var` makes the blocker-2 assertion fail
  *deterministically*, on every guarded test, immediately, long before any race between
  concurrent tests could manifest. So variant B as constructed only re-proves that the assertion
  detects a missing redirect (already known from variant A, where it caught the clobbering
  in-flight) — it says nothing about whether the redirect independently prevents
  `freedesktop.rs:140`. The clean design would have neutralised the redirect **and** its own
  assertion together, so the guarded tests actually ran unprotected against the shared trash. That
  rerun was not taken: variant A already establishes the mutex is load-bearing with a number and a
  mechanism, and the redirect is kept regardless of what a corrected variant B would show (see
  below), so a second CI cycle would have bought a satisfying number rather than a different
  decision.

  **Methodological lesson, worth carrying forward:** an assertion added to catch a silent failure
  can mask the very failure a later fault-injection experiment is trying to measure. Blocker 2's
  assertion is exactly right for production/test use — it is what caught variant A's clobbering —
  and it is precisely what made variant B unable to see past it. If a fault-injection variant
  neutralises the thing an assertion watches, disable that assertion in the same variant, or the
  result is a tautology dressed as evidence.

- **Redirect kept regardless, on independent grounds:** the redirect's value never rested solely
  on race prevention. It also stops these tests touching a **developer's real trash** on Linux —
  the same objection the test's own comment raises about the Windows Recycle Bin, and the
  independent UAT confirmed that exact exposure remains on Windows today precisely because no
  redirect knob exists there. So both mechanisms (mutex + redirect) stay in the merged fix
  regardless of variant B's outcome.
- **Fix restored, diagnostic test removed** (this commit): variant B's redirect-neutralisation
  reverted, `cpe_1785_diagnostic_stress_loop_temporary` deleted entirely (it was un-ignored and
  shelled out to `cargo test` 50 times — left in place it would have added roughly a minute to
  every future CI run). `git diff b2717976..HEAD -- src-tauri/src/lib.rs` shows only the
  stress-test removal — no residue from either throwaway variant.

**Interim conclusions already supported by variant A alone:**
1. The mutex is load-bearing, now with a number: 66% failure without it (33/50) vs. 0 failures
   across every other run this ticket has produced with it (3 Linux CI runs, 40 Windows
   iterations, this worker's own 10/10 local Windows run).
2. The redirect alone is not sufficient — variant A is exactly "redirect present, mutex absent",
   and it failed 66% of the time. The doc comment's claim that the mutex is not redundant with the
   redirect is now backed by measurement, not just reasoning.
3. Blocker 2's assertion earned its keep on its first outing: without it, all 33 failing runs
   would have *passed* while silently touching the wrong trash directory — exactly the "silently
   degraded, every test still green" failure mode the review predicted for a missing invariant
   check.

Both the PR body's Evidence section and this Work Log now carry final numbers for variant A,
variant B (with its confound explained), and the restore-and-cleanup commit.

### Gates

- `cargo clippy --all-targets -- -D warnings` — clean, both `src-tauri` feature modes (default and
  `sidecar-platform`) — verified locally on Windows.
- `cargo test --lib trash -- --test-threads=8` — 10/10 passing locally on Windows.
- No new dependencies. Stayed inside `src-tauri/src/lib.rs`.
- **Windows-only local verification cannot exercise the `#[cfg(target_os = "linux")]` half of the
  fix** (the `XDG_DATA_HOME` redirect, the redirect assertion, the diagnostic test) — that half is
  validated only by Linux CI (Evidence Rule 2: a check run under conditions the real caller never
  sets confirms nothing about the real caller).
