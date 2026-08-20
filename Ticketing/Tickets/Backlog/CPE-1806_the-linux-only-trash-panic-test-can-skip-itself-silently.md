---
id: CPE-1806
title: the Linux-only trash-panic test can skip itself silently, making its assertions vacuous under a green tick
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
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
