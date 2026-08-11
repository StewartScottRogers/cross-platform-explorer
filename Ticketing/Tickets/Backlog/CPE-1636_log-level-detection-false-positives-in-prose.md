---
id: CPE-1636
title: "Log level detection fires on prose: a level word with nothing lowercase before it on the line is classified as a real level"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Reviewer of CPE-1618 (PR #829), the log viewer. The reviewer went looking for
over-eager detection specifically — a viewer that paints ordinary lines red is worse than one that doesn't
highlight at all — and found a real, reproducible class of false positive.

## The gap
`detectLevel` (`src/lib/preview/logViewer.ts`) guards against prose with a "no lowercase in the lead-in"
heuristic (~L1203-1210): it only accepts a level word if nothing lowercase precedes it on the line. That
check looks only at the text **before** the match, so a level word preceded by a quote mark, a digit-dot, or
all-caps prose passes even though it clearly isn't a level marker.

Reproduced by the reviewer against the real function:

| Line | Classified as |
|---|---|
| `"ERROR" is a reserved word in this DSL, see docs.` | `error` |
| `"ERROR: connection refused" appears in the logs when the DB is down.` | `error` |
| `1. ERROR handling guide` | `error` |
| `SEE ERROR HANDLING DOCS FOR MORE INFO` | `error` |

This contradicts the module's own doc comment, which claims a level is "never guessed at".

## What is already correct (don't regress it)
The same review confirmed these are all correctly left unclassified, and they should stay that way:
JSON-per-line (`{"level":"error",...}`), logfmt (`level=error`), syslog with a lowercase hostname before the
level, a URL path containing `/error-report`, `ERRORLEVEL=1`, lowercase Android-style traps, mid-sentence
prose, and stack-trace continuation lines. A line containing two levels resolves to the first, which the
reviewer judged reasonable.

## Fix
Tighten the lead-in heuristic so a level word only counts when it is genuinely in level *position* — e.g.
require it to be delimited as a standalone field (line start, or after a timestamp/bracket/pipe), and reject
it when the surrounding characters mark it as quoted or as part of running prose. Consider looking at what
follows the match as well as what precedes it (a real level marker is typically followed by a separator such
as `:`, `]`, `|` or whitespace-then-message, not by ordinary words).

Keep the work bounded — detection currently scans only the first 48 chars of a line (`LEVEL_SCAN_CHARS`),
and the reviewer stress-tested the regexes for catastrophic backtracking and found none. **Do not regress
either property**: any new pattern must stay flat/unambiguous and stay inside the scan window.

## Acceptance criteria
- All four lines in the table above are left unclassified; tests cover them.
- Every correctly-unclassified case listed under "What is already correct" still behaves the same.
- Genuine level lines across the formats the viewer does support are still detected — no new false negatives;
  this is the trade-off to watch, since tightening detection is exactly how you lose real errors.
- Regex stays non-backtracking and within `LEVEL_SCAN_CHARS`; add a cheap guard if one doesn't already exist.

**Conflict surface:** `src/lib/preview/logViewer.ts` and its test file. Self-contained and pure-logic.

## Notes
Low priority: the failing shape needs *no* lowercase character anywhere before the level word, so it bites
documentation- and README-style text far more often than genuine log output. The reviewer judged it not
disqualifying for CPE-1618 and approved that PR on that basis.
