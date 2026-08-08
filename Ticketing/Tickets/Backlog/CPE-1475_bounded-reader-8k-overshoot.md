---
id: CPE-1475
title: "read_bounded_line can overshoot the cap by ~1 BufReader chunk (~8 KiB) when a newline immediately follows the cap"
type: Bug
status: Backlog
priority: Low
component: Backend
tags: [ready, security]
epic: CPE-862
created: 2026-08-08
---
## Observation (from CPE-1471 review of PR #718)
`sidecar/host/src/supervisor.rs` `read_bounded_line`: the newline-search branch
(`available.iter().position(|b| *b == b'\n')`) runs BEFORE the cap check, on the whole current ≤8 KiB chunk. If
`buf` has accumulated up to `cap - 1` bytes across prior no-newline chunks and the next `fill_buf()` chunk
contains a `\n`, the code appends `available[..pos]` unconditionally and returns — so the effective worst-case
buffered size is `cap + ~8 KiB` (one BufReader-internal-buffer overshoot), not exactly `cap`.

## Severity
LOW / not exploitable — the audited DoS threat (an attacker flooding gigabytes with NO `\n` at all) never takes
this branch and IS capped precisely at `MAX_LINE_BYTES` (16 MiB). This only affects a contrived message that
crosses the cap and then immediately supplies a newline — a fixed ~8 KiB slack on a 16 MiB cap, not an OOM.

## Fix direction
Apply the cap check to the newline branch too: when a `\n` is found at `pos`, only append `available[..pos]` if
`buf.len() + pos <= cap`, else treat it as an overflow (same "frame too large" error). Trivial. Add a test: a line
that reaches `cap-1` then delivers a chunk containing a newline past the cap → errors, buffered ≤ cap.

## Notes
Also informational (no ticket needed): the switch from `BufRead::lines()` to manual `from_utf8_lossy` means invalid
UTF-8 on a line is now a soft per-message decode failure instead of a hard connection-killing read error — arguably
more graceful. Epic CPE-862.
