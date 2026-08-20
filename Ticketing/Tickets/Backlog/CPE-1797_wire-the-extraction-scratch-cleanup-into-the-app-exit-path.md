---
id: CPE-1797
title: wire cleanup_extraction_scratch() into the app's exit path
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-19
closed:
---

## Problem

CPE-1786 gave archive-extraction temp directories an owner: they now live under a per-session root and
are swept once a foreign session root has been idle for an hour. It also ships
`cleanup_extraction_scratch()`, intended to be called when the app shuts down — but **it is not wired
into the exit path**.

The consequence is small and bounded: a session's own directories are reclaimed by the next run's sweep
about an hour later rather than immediately on quit. No correctness depends on it. But a user who
extracts a few large archives and quits leaves that disk space occupied for an hour for no reason, and
the function currently sits in the codebase with no caller, which invites someone to delete it as dead.

## Why it was deferred

Purely a concurrency call by the Foreman, not a technical one: `src-tauri/src/lib.rs` was held by
another worker during the same shift (CPE-1785), and two workers editing that file would have collided
on merge. The CPE-1786 worker flagged it rather than reaching into a file it had been told to leave
alone — the right call.

## What to do

- Call `cleanup_extraction_scratch()` from the Tauri app's shutdown path in `src-tauri/src/lib.rs`.
  Check how the app currently handles exit — there may already be an `on_window_event` /
  `RunEvent::ExitRequested` hook to hang it on rather than adding a new one.
- Keep it **best-effort and non-blocking on failure**, matching the sweep's existing philosophy: this
  must never delay or fail a quit. A user closing the window should not wait on a recursive delete, so
  consider whether a bounded time budget is wanted, and say what you chose.
- Consider the kill case honestly: a force-quit or a crash runs no cleanup at all, so the hour-idle
  sweep remains the real backstop. This ticket makes the common case immediate; it does not replace the
  sweep, and the comment should say so.
- Verify by extracting a few archives, quitting normally, and confirming the session root is gone —
  then force-killing and confirming the sweep still reclaims it on the next run.

## Notes

Filed by the Foreman from PR #945, 2026-08-19, where the CPE-1786 worker flagged it explicitly as a
deliberate deferral.

Related: **CPE-1786** (the ownership model this completes), **CPE-1693** (the test-helper half of the
same leak family).
