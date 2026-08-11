---
id: CPE-1638
title: "Filtering a log to Errors hides the stack trace — you get the bare exception header and nothing else"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Reproduced on a **real** log by the independent UAT tester of CPE-1618 (PR #829) — not a constructed case.
The filter works exactly as specified, and the specification is the problem.

## The gap
Real excerpt from `C:\Users\...\AppData\Local\Docker\log\host\electron-2026-07-24.log`, a genuine uncaught
JS exception:

    2026-07-24T13:03:50.615Z error [BUGSNAG] Uncaught exception…      <- detected as Error
    AbortError: Request aborted                                        <- no level word -> unclassified
        at ... (9 further stack frames)                                <- no level word -> unclassified

Filtering to **Errors only** left **1 of 22 lines** visible: the bare header, with no indication of what the
exception was or where it happened. Every line that actually tells you anything was hidden, because
continuation lines carry no level word of their own.

This is the exact failure mode worth caring about: the filter is most useful on a big noisy log, which is
precisely when an error's detail is most likely to be multi-line.

## Fix
Group continuation lines with the line they belong to, so filtering keeps a finding intact:
- A line that carries no level of its own, and follows a classified line, should **inherit** that line's
  level for filtering purposes — at minimum when it looks like a continuation (leading whitespace, a
  leading `at `/`Caused by:`/`...`, or a bare exception-type line immediately after an error).
- Filtering should then show the whole group, not just its header. Consider showing continuation lines
  visually subordinate to their header rather than fully re-tinted, so a 10-frame trace doesn't read as ten
  separate errors — the Visual Critic has already flagged an all-red wall as the risk to avoid here.
- Be conservative about what counts as a continuation. Over-eager grouping would sweep unrelated following
  lines into an error, which is its own kind of lie. Prefer under-grouping to over-grouping and say which
  way you erred.

Keep the existing bounds: detection scans only the first `LEVEL_SCAN_CHARS` (48) of a line and the regexes
are deliberately flat/non-backtracking — a grouping pass must stay O(n) over already-sliced lines and must
not reintroduce per-line rescanning.

## Acceptance criteria
- The real excerpt above, filtered to Errors, shows the header **and** its trace.
- A test uses a realistic multi-line trace and asserts the group survives filtering; it fails against
  today's code (negative control).
- Unrelated lines following an error are NOT swept in — test the boundary explicitly.
- Filter counts ("Showing N of M") stay accurate with grouping applied.
- No regression in the detection accuracy CPE-1618's review verified: JSON-per-line, logfmt, syslog, URLs,
  `ERRORLEVEL=1` and prose must all stay unclassified.

**Conflict surface:** `src/lib/preview/logViewer.ts` and `src/lib/components/LogPreview.svelte`, plus tests.
Overlaps CPE-1637 (large-log support) and CPE-1636 (detection false positives) — all three touch the same
two files, so sequence them rather than running them in parallel.
