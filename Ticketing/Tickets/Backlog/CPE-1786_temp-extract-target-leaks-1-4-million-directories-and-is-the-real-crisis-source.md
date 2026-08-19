---
id: CPE-1786
title: temp_extract_target leaks 1.4 million directories in production code — the real source CPE-1693 did not touch
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-19
closed:
---

## Problem

`crates/server/src/archive.rs::temp_extract_target` (around line 749) creates
`%TEMP%\cpe-archive\<pid>-<seq>\` for every archive extraction and **never removes it**. Its own doc
comment states this outright: *"Nothing here cleans up... this change adds one more directory per
extraction just as before."*

Measured on this machine during PR #934's UAT, 2026-08-19:

- `%TEMP%\cpe-archive\` contains **1,394,403** subdirectories.
- One isolated, 8-way-parallel run of just the archive test module added **12** more.

That is essentially the entire "1.29 million directory" crisis figure from CPE-1693's own Work Log,
**alive today and still growing**, in production code.

## Why this was missed, twice

CPE-1693 fixed the leak at the **test-helper** level — `scratch()` now returns a `Drop` guard, and a
measured 532 leaked directories per full `crates/server` test run became 2. That work is real and
correct. But it is a different leak from this one, and two things hid the difference:

1. **The counting method has a blind spot.** CPE-1693 counted **top-level** `%TEMP%\cpe-*` directories.
   `temp_extract_target` creates its directories *inside* one pre-existing top-level directory
   (`cpe-archive`), so 1.39 million of them register as **one** entry. Before/after counts across a full
   test run showed +1, which looked like the leak was closed. Counting one level deeper shows +12 from a
   single module.
2. **PR #934's Work Log dismissed it as "already guarded."** That conflates two different properties:
   CPE-1733 gave this function an exclusive `create_dir` to defend against **squatting/redirect**; it
   says nothing about **leak-freedom**. Being safe against a hostile pre-existing directory is not the
   same as removing your own afterwards.

## Why it matters — this is the site of both originally-reported failures

CPE-1693 was escalated to High because the backlog started manufacturing false test failures. Both were
here, not in the test helpers:

- `zip_lists_real_tree_and_extracts_inner_file` failed on a **PID collision** — so many
  `%TEMP%/cpe-archive/<pid>-<seq>` directories exist that a reused process id finds its scratch name
  already taken.
- `could not claim a private extraction directory ... after 1024 attempts` — `temp_extract_target`'s
  retry loop exhausting its **entire** budget, because 1024 consecutive candidate names were all taken.

CPE-1745's Done record already says this explicitly: *"CPE-1693 tracks the leak; not touched by this
ticket."* Nothing has touched it since. Both failures passed on rerun, which is the property that
teaches whoever sees them to press rerun instead of reading.

Note also that this is **user-facing**, not merely a test annoyance: every archive a real user extracts
leaves a directory behind in their `%TEMP%` forever.

## What to do

This is production code with a live consumer, so it cannot simply take the test helper's `Drop` guard —
the extracted content must outlive the function that creates it. The lifetime question is the whole
ticket:

- Establish who actually owns an extraction directory and for how long — is it the extraction call, the
  preview/transfer that consumes it, or the app session?
- Give it that lifetime explicitly. Options worth weighing: tie the directory to the consuming
  operation's lifetime and remove it on completion; or keep a session-scoped root cleaned on startup
  and shutdown; or a generation/age sweep on app start. Sweeping on startup is the cheapest thing that
  bounds the growth, but it does not fix a long-running session.
- Whatever the shape, **the PID/sequence namespace must stop being able to exhaust**. 1024 consecutive
  taken names is a symptom of unbounded accumulation, not of an unlucky PID.
- **Prove it red first**, per the Evidence Rules in `Ticketing/wiki.md`: measure
  `%TEMP%\cpe-archive`'s subdirectory count before and after an extraction run — one level down, not
  top-level — and show the growth stopping. Do not reuse CPE-1693's top-level counting method; it is
  precisely what hid this.
- Clear the existing 1.39 million as a one-shot, reusing the junction-safe purge from CPE-1693's PR
  #934 (verified there against a canary behind a nested junction).

## Notes

Filed by the Foreman from PR #934's independent UAT, 2026-08-19. The UAT could not force a live
1024-attempt exhaustion on demand — it needs a specific PID-reuse-into-an-occupied-range condition — so
the crash itself was not reproduced this time; the root cause was measured directly instead and is
fully intact.

Related: **CPE-1693** (the test-helper half of this leak, genuinely fixed), **CPE-1733** (the
squatting/redirect guard that was mistaken for a leak fix), **CPE-1745** (whose Done record already
recorded the gap), **CPE-1782** (sftp/ftp/net helper leaks).
