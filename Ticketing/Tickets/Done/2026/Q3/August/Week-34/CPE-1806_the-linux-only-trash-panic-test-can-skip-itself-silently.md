---
id: CPE-1806
title: the Linux-only trash-panic test can skip itself silently, making its assertions vacuous under a green tick
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-20
---

## Problem

The Linux-only test covering the malformed-`.trashinfo` panic boundary (`src-tauri/src/lib.rs:14986-14992`)
can **skip itself via `skip_notice!`** when its preconditions are not met. The tick stays green either way.

CPE-1803 added new assertions to that test — that `degraded` flips true on the panic path and false on
recovery. Those assertions are the *only* execution any code path in that fix ever gets, since the panic
is Linux-only and the crew develops on Windows. If the test silently skips on the Linux runner, the
assertions are **vacuous** and the green tick means nothing.

## Why it matters

This is the cannot-fail-test problem wearing a different hat. This crew found **nine candidate cannot-fail
tests in a single sprint and eight were real**. A test that skips itself is worse than one that cannot
fail, because the skip is invisible in the summary — it reads as a pass.

It is also the exact failure mode a green tick is least able to warn you about: the more platform-specific
the bug, the more the guard depends on a runner nobody watches.

## What to do

- **First, just look.** Pull the Linux CI log for a recent run and check whether `skip_notice!` fired. If
  it never fires, this is cheap: make that a guarantee rather than an observation.
- Make a silent skip **impossible or loud**. Options: fail the test when the precondition is missing on a
  platform where it should hold; emit a distinguishable marker CI greps for; or assert the precondition
  instead of testing it. A skip that is *correct* on Windows and *a bug* on Linux needs to be told apart
  by the code, not by the reader.
- Sweep for siblings — `skip_notice!` is presumably used elsewhere, and the same reasoning applies to any
  test guarding a platform-specific defect.

## Notes

Filed by the Foreman from the independent review of PR #957, 2026-08-20. Pre-existing; CPE-1803 inherited
it rather than introduced it, but CPE-1803's coverage now depends on it.

Related: **CPE-1803**, **CPE-1791** (the panic boundary itself).

## Work Log

- 2026-08-20 — merged as **#961** (`e3d3f596`), batch 35. Two rework rounds.

### The answer to the question this ticket opened with
**The skip had NOT been firing.** The worker read two `Backend (ubuntu-latest)` logs before writing any
code; the reviewer then upgraded that from an observation to a proof by pulling the **raw** job logs via
`gh api .../actions/jobs/<id>/logs` and grepping them whole rather than by region:

- runs `32361564571` job `96402134715` and `32374146099` job `96441615073` (ubuntu) — **0** `CPE-1268`
  notices, both logs complete, with unrelated CPE-1696/1705 notices present **proving the emitter reaches
  that code**;
- the same run's windows job `96441615090` — **exactly 5**, one per shared site.

So CPE-1803's Linux coverage has been real all along. That was the good branch of the two this ticket
anticipated.

### What shipped
All **7** `trash_roundtrip_available()` call sites route through `cpe_server::fsutil::require_staged`
(CPE-1717's gate) instead of a bare skip-and-return. `supported_here = true` for the two
`#[cfg(target_os = "linux")]` panic-boundary tests; `supported_here = cfg!(target_os = "linux")` for the five
`cfg(any(windows, linux))` sites. Under CI a Linux staging failure now **panics**; Windows keeps a
legitimate loud skip. The five Windows notices, appearing by name every run, make that platform split
**measured fact rather than folklore**.

### The rework, which is the interesting part
**Round 1 blocker: the guard was itself unguarded.** Deleting the new routing from any of the 7 sites left
all of CI green. A guard against silent vacuity that could be silently removed. Fixed with two OS-asymmetric
blocks in `ci.yml`, handling the trap that **a zero-match `cargo test <filter>` exits 0** — which would have
been the same joke one layer up.

**Round 1 blocker: five comments cited a measurement that was not there.** They pointed at
`trash_roundtrip_available`'s doc comment for "the measured per-platform verdict"; that comment named only
the Windows failure case. The measurement existed only in the PR body, where nobody would look. Now in the
comment with full run/job provenance, independently re-verified against GitHub by the reviewer.

**Round 2: the new comment was pasted twice, verbatim**, inside the very comment the previous round asked to
be edited.

### Honest limits recorded
The round-2 review established that forcing `RUNNER_OS=Linux` on Windows exercised the **shell branch, not
the Linux outcome** — it proved block 2's detector and nothing about block 1, and reached neither block's
success path. Not fatal, because the step ships in this diff and its only green outcome on the ubuntu leg is
the intended one, so the guard is self-proving on this PR. But "partially proved" is not "properly proved".

### Left as tickets
**CPE-1817** (the guard covers 2 of 7 sites while reading as if it covers the mechanism; and the Windows arm
of block 2 is zero-match-vulnerable in isolation), **CPE-1815** (the probe collapses six failure causes into
one bare `false`, so the loud red it now enables will name none of them).
