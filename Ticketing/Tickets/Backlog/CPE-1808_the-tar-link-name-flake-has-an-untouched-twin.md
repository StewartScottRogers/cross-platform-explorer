---
id: CPE-1808
title: the tar link-name flake fixed in CPE-1759 has an untouched twin fixture
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

CPE-1759 found one of its own tests flaking: tar's **100-byte link-name field** overflowed, but only under
a full-suite run, because the scratch path was longer then. It fixed that fixture and recorded the reason
at the fixture.

**The twin at `crates/server/src/archive.rs:6409` / `6637` was not touched** and has the same shape.

## Why it matters

A load-dependent flake is the worst kind of test: it passes in isolation, so the person who wrote it
believes it, and it reds at random on a busy CI runner — where it gets re-run rather than diagnosed. This
one is worse than average because its trigger is **path length**, which varies with the scratch directory
CPE-1786 just made session-scoped and longer.

So the flake gets *more* likely over time, not less.

## What to do

- Apply the same fix, or explain why the twin is genuinely safe — with the arithmetic, not an impression.
  100 bytes is a hard limit; show the worst-case path length under the current scratch-dir scheme.
- **Red-proof it the way the original was proved**: reproduce the overflow deliberately (long scratch path
  or a forced-long name), watch it fail, then apply the fix.
- Sweep for any other fixture in this file constructing a tar link entry from a scratch path.

## Notes

Filed by the Foreman from the independent review of PR #958, 2026-08-20. Pre-existing, not introduced by
that PR — which is why it was left rather than widening the change.

Related: **CPE-1759** (the original flake and its fix), **CPE-1786** (session-scoped scratch dirs, which
lengthened the paths).
