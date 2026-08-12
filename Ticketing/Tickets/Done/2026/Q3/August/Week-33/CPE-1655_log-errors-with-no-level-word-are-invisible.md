---
id: CPE-1655
title: Logs whose errors carry no level word are invisible to the Errors filter
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-11
closed:
---

## Problem

Found by the independent UAT on PR #842, testing against real logs pulled off this machine rather than
the repo's fixtures.

Level detection — and therefore the CPE-1638 stack-trace grouping that hangs off it — fires **only** when
a level word (`ERROR`/`WARN`/`INFO`) appears on the header line. Three real cases from that run show what
that costs:

1. **A bare crash dump is entirely invisible.** A real `python -c` traceback and a real Rust panic
   backtrace (`RUST_BACKTRACE=1`) contain no level word anywhere, so **no line is classified** and
   filtering to Errors shows **0 of 10** and **0 of 30** lines respectively — even though the whole file
   *is* the error. A user who pipes a crash straight to a `.log` gets nothing from the feature.
2. **A real native error format is missed.** `C:\Windows\Logs\DISM\dism.log` reports errors as
   `[pid.tid] [0x8007007b] FIOReadFileIntoBuffer:(1454): ...incorrect.` — no level word, not stack-frame
   shaped, so 6 genuine error lines in a 500-line sample were left unclassified. Conservative and
   deliberate per the "never guess" design, but a real class of error text the viewer cannot see.
3. **One residual false positive**, the other direction: `"## Error handling"` — a markdown ATX heading —
   classifies as `error`, because `leadHasIsolatedLetterWord` only rejects letter-runs and `"## "`
   contains no letters. 1 hit in 3,859 real prose lines from `src/docs/*.md`; only bites a markdown file
   opened as a `.log`.

## Why this is not a CPE-1636 regression

CPE-1636 was about prose false POSITIVES and it fixed them (0 in 1,500 real log lines). This ticket is the
opposite axis — the detector's reach. Filed separately so the two are not confused.

## Acceptance criteria

- [ ] A file that is wholly a crash dump (Python traceback, Rust panic + backtrace, Node throw, Go panic)
      is recognised as such and its lines are reachable through the Errors filter — decide whether that is
      a whole-file heuristic ("this file has no levels; treat trace blocks as errors") or a widened
      per-line rule, and write the reasoning down.
- [ ] The `dism.log`-style `[pid.tid] [0xNNNNNNNN] Func:(line): msg` shape is detectable, or a documented
      decision explains why it is deliberately left alone.
- [ ] `"## Error handling"` and similar markdown-heading shapes no longer classify as a level.
- [ ] The CPE-1636 prose corpus stays at **zero** false positives — re-run the 3,859-line `src/docs/*.md`
      sweep and the 1,500-line real-log sweep as the guard, and record both numbers.
- [ ] Whatever widening lands is measured both ways (missed real errors AND false positives), not argued.

## Notes

- Source: independent UAT on PR #842, 2026-08-11 — measured against real files, with counts.
- Related: [[CPE-1636]] prose false positives, [[CPE-1638]] stack traces survive filtering,
  [[CPE-1644]] UTF-16 logs.
- The UAT's own summary of the risk: "a bare interpreter/panic dump with no logging-framework prefix is
  invisible to the Errors filter entirely."

## Work Log

- 2026-08-11 — Filed by the Foreman from the PR #842 UAT findings.
