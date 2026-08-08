---
id: CPE-1475
title: "read_bounded_line can overshoot the cap by ~1 BufReader chunk (~8 KiB) when a newline immediately follows the cap"
type: Bug
status: Done
priority: Low
component: Backend
tags: [ready, security]
epic: CPE-862
created: 2026-08-08
closed: 2026-08-08
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

## Work Log

- 2026-08-08 — Fixed exactly as the ticket directed: the newline branch of `read_bounded_line`
  (`sidecar/host/src/supervisor.rs`) now applies the cap before appending, erroring with the same
  "frame too large" message when `buf.len() + pos > cap`. Four lines, no behaviour change for any line
  that fits.

  Two tests added. The existing tests all use `std::io::Cursor`, which returns the whole buffer in a
  single `fill_buf` and therefore **cannot** exercise the cross-chunk path this bug lives in — so the
  tests introduce a small `ChunkReader` (a `BufRead` handing out fixed-size chunks, the way `BufReader`
  fills from a pipe):
  - `bounded_reader_applies_the_cap_to_the_newline_branch_too` — 68-byte line fed in 10-byte chunks
    against a 64-byte cap, so the buffer reaches 60 and the newline lands at offset 8 of the next chunk.
    Asserts the error and `buf.len() <= 64`.
  - `bounded_reader_still_accepts_a_line_that_ends_exactly_at_the_cap` — the boundary the fix must not
    break: a line of exactly `cap` bytes stays legal.

  **Verified the test is not vacuous** by backing the fix out and re-running: the new test fails
  (`unwrap_err` on an `Ok`), the exactly-at-cap test still passes, then restored. Full sidecar-host suite
  104 passed / 0 failed; `cargo clippy --all-targets -D warnings` clean.

  Note on the environment: a concurrent process reset the shared checkout to `origin/main` partway
  through this work and wiped the first application of this fix; it was re-applied and re-verified. See
  CPE-1476's work log.
