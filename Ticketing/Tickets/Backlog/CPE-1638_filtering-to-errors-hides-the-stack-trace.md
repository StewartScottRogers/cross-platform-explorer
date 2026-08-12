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

## Work Log
2026-08-11 (sprint, Worker) — Implemented: `LogLine` gained `filterLevel` (the level a filter should key
on — a line's own `level`, or the inherited level of the classified line it continues) and `isContinuation`
(true when grouped, so it renders subordinate rather than getting its own badge/full-strength border).
`parseLog` runs a second `groupContinuations()` pass (O(n), one bounded `CONTINUATION_SCAN_CHARS`-window
(64 chars) check per line — never rescans full line text) over the already-detected lines: an unleveled
line immediately following a classified line inherits its level when it looks like a continuation
(leading whitespace, `at `, `Caused by:`, a trailing `...`, or — only right after an error — a bare
`XError:`/`XException:` header). The chain breaks the instant a line doesn't match, so unrelated trailing
lines are never swept in. `filterLines` now keys on `filterLevel` instead of `level`. `LogPreview.svelte`
renders continuation rows with `data-continuation`/`data-group-level` attributes and a faint indented
border in the group's colour (35% mix) rather than the header's full-strength one — the Visual Critic's
"wall of red" risk on a 10-frame trace is avoided since `level` (and thus the badge) stays `null` on every
continuation line.

Verified against the ticket's real excerpt (`electron-2026-07-24.log` shape, reproduced as
`REAL_EXCERPT_LINES` in `logViewer.test.ts`): filtering to Errors-only now shows the header AND all 5
continuation lines (6 of 7 total), not just the bare header; the one deliberately unrelated trailing line
("Server accepted a new connection...") is correctly excluded — the explicit boundary test the ticket
asked for. Independent verification pass (composed separately from the ticket's own fixture, in the shape
of a real Node.js unhandled-rejection dump with 4 `at ...` frames) confirmed the same behavior end-to-end:
all 4 frames + the `TypeError` header survive an errors-only filter, the following unrelated `info` line
does not. "Showing N of M" counts stay accurate (asserted directly in the test). No regression to
CPE-1636's detection-accuracy corpus (JSON/logfmt/syslog/URL/`ERRORLEVEL=1`/prose all still unclassified —
same test file, same describe blocks, all still passing).

Verification: `npm run check` clean; `npx vitest run` — all 287 files / 3660 tests pass. JS/TS-only change,
no Rust touched.
