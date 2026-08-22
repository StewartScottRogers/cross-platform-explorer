---
id: CPE-1849
title: ffmpeg-pin-freshness pairs --retry with --max-time and has no --retry-max-time
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-21
closed:
---

## Problem

`.github/workflows/ffmpeg-pin-freshness.yml:251` and `:409` both pair `--retry 3` with `--max-time 30`
and no `--retry-max-time`.

CPE-1824 established, by measurement rather than by reading, that this combination does not bound what
it looks like it bounds: `--max-time` is **per attempt** and its counter **resets on every retry**, and
`--retry-all-errors` makes a `--max-time` expiry *itself* a retryable error — so the timeout meant to
stop a stall is what triggers the next attempt. Measured against a deliberately stalling server, the
`ci.yml` sites ran **1101 seconds** against a claimed 180-second bound, printing six separate
`Operation timed out` messages. Adding `--retry-max-time` brought the same case to **182 seconds**.

## Why this is Low and not a repeat of CPE-1824

The defect CPE-1824 fixed was a **false claim recorded as verified fact** — code comments asserting
`--max-time` bounded the whole invocation. This file's own comment is already **accurate**: it says
*"--max-time bounds each attempt"*. So nothing here is lying.

The worst case is also much smaller: `3 x 30s` plus delays, not `5 x 180s`.

What remains is that the workflow has no explicit series bound, so its real worst case rests on
whatever outer `timeout-minutes` applies rather than on anything the curl line states.

## Acceptance criteria

- [ ] Both sites either gain a `--retry-max-time` coherent with the step's `timeout-minutes`, or record
      why the outer backstop is sufficient here. CPE-1824's arithmetic is the precedent: curl checks the
      retry timer *before* starting each retry and lets an in-flight attempt finish, so worst case is
      `retry-max-time + max-time`, and the value must satisfy `N + max-time < timeout-minutes` with margin
      — the point being that curl loses on its own terms, with a real exit code and `--fail`
      diagnostics, rather than being killed opaquely by the runner.
- [ ] Confirm what outer `timeout-minutes` actually applies to each site before choosing a value. Do not
      assume one exists.
- [ ] CPE-1824's guard (`src/lib/releaseHangHardening.test.ts`) scans every non-comment curl line in the
      three workflows it covers. Decide whether to extend its scan to this file, and say why either way.
      If extended, the exclusion note currently recording this file as deliberately out of scope must go.
- [ ] Re-run a positive control after any change: a real download through the modified flags, reporting
      exit code, elapsed time and byte count — not just that the workflow parses.

## Notes

Found by the CPE-1824 round-2 worker, which deliberately did **not** absorb it: the defect that ticket
existed to fix is not present here, and this file has several branches in flight on it. The exclusion is
documented in that ticket's test file so it reads as considered rather than missed.

Read CPE-1824's Work Log first — it carries the stall-server harness and the measured before/after, so
this ticket does not need to re-derive the mechanism.
