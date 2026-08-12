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

## Work Log
2026-08-11 (sprint, Worker) — Implemented: `detectLevel` (`src/lib/preview/logViewer.ts`) now requires
BOTH a real lead-in and a real trailing separator around the level word. Lead-in check replaced the old
two narrow rules (reject-any-lowercase, reject-2+-uppercase-run) with one general
`leadHasIsolatedLetterWord()`: a level marker's lead-in may contain letters only when they're glued
directly onto a digit (the ISO `T`/`Z` in `09:14:05Z`) — any standalone letter run (any case, any length)
disqualifies it — plus the existing numbered-list-marker and trailing-quote checks. Trailing check requires
whitespace/`:`/`]`/`|`/EOL right after the match, rejecting glued compounds like `error-like`. All four of
the reviewer's reproduced false positives (quoted mention, quoted phrase, numbered-list heading, ALL-CAPS
sentence) are now unclassified; every previously-correct shape (JSON, logfmt, syslog, URL fragment,
`ERRORLEVEL=1`, android-style, mid-sentence prose, stack continuation) still unclassified; every
previously-detected real-log shape still detected. Regex/scan work stays flat and bounded by the existing
`LEVEL_SCAN_CHARS` (48) window — no backtracking risk introduced.

**Independent verification found a fifth false positive** beyond the four in this ticket, not from the
same predecessor who wrote the fix: composed a batch of ordinary README/docs-style sentences independently
(not derived from the regex internals) and ran them through `detectLevel` — `"A warning icon appears next
to any file that couldn't be scanned."` was misclassified as `warn`, because a lone capitalized lead-in
word ("A") has neither a lowercase letter nor a 2+ uppercase run, so it passed both of the original fix's
narrower checks. Root-caused and fixed by generalizing to the single `leadHasIsolatedLetterWord()` rule
above, which subsumes both narrower checks and closes this gap too. Added permanent regression tests
(`logViewer.test.ts`, "CPE-1636 fifth false positive: a lone capitalized lead-in word") for this case plus
a same-family "I saw an ERROR dialog..." case, and a positive-control test confirming the ISO-timestamp
`T`/`Z` shape still detects correctly (the exact mechanism the new rule depends on).

Verification: `npm run check` clean; `npx vitest run` — all 287 files / 3660 tests pass (up from 3657
before this ticket's added tests). No Rust touched by this ticket's own fix (JS/TS-only).

2026-08-11 (sprint, Worker, PR #842 review round 2 — F1) — An independent reviewer of PR #842 (which
bundled this ticket with CPE-1638/CPE-1644) confirmed the detector's prose-false-positive fix is solid,
but flagged a pre-existing gap (not a regression of this ticket's own change): two mainstream real log
formats are never detected because their lead-in contains an "isolated letter word" by the letter of the
new rule —
- Logback's own documented `%d [%thread] %level` pattern, e.g.
  `17:04:22.123 [main] ERROR c.e.MyService - Failed to connect` (the thread tag `main` inside `[...]`).
- RFC3164 syslog/journald with a month-name date prefix, e.g.
  `Aug 11 17:04:22 myhost myapp[1234]: ERROR Failed to connect to database` (`Aug`, plus the bare
  hostname `myhost`).

**Fixed the Logback case:** added `BRACKET_TOKEN_REGEX` (`src/lib/preview/logViewer.ts`) — a bracket span
with no internal whitespace (`[main]`, `[Thread-3]`, `[pool-2-thread-1]`, `[http-nio-8080-exec-1]`, …) is
the shape of a logger/thread-name tag, not a standalone prose word, so `leadHasIsolatedLetterWord` now
exempts any letter run wholly inside one. Deliberately narrow: a bracket span containing whitespace (a
parenthetical prose remark) never matches, so it can't quietly admit ordinary bracketed asides. Verified
against the full existing prose-false-positive corpus (all five known false positives, JSON/logfmt/syslog/
URL/`ERRORLEVEL=1`/stack-continuation) — none regressed; none of them contain a no-whitespace bracket span.

**Left the RFC3164 gap open, deliberately:** the offending token is a bare hostname (`myhost`) with no
digit, bracket, or other structural marker touching it — indistinguishable, by any general rule, from an
ordinary English word starting a sentence (the exact ambiguity this whole ticket exists to resolve on the
side of caution). Any rule broad enough to admit it would also admit real prose. Per the ticket's own
"if you genuinely cannot separate the two, say so explicitly, leave the gap" guidance: left unclassified,
same as before this round — not a regression, since this shape was never detected in the first place.
Documented as a permanent, intentionally-failing-null test in `logViewer.test.ts` rather than silently
dropped.

Re-ran the reviewer's format classification before/after (12 representative formats spanning the already-
supported shapes, both gap formats, and the five prose false-positive negative controls):

| Format | Line | Before | After |
|---|---|---|---|
| Bracketed timestamp | `[2026-08-11 09:14:05] ERROR Failed to connect` | error | error |
| ISO timestamp | `2026-08-11T09:14:05Z ERROR Payment gateway timeout` | error | error |
| Colon-suffixed level at line start | `ERROR: Unhandled exception in request handler` | error | error |
| Bracket-wrapped level | `[WARN] disk space low` | warn | warn |
| Android logcat | `E/NetworkClient: Failed to reach api.example.com` | error | error |
| RFC3164 syslog w/ month prefix | `Aug 11 17:04:22 myhost myapp[1234]: ERROR Failed to connect to database` | null | null (documented gap) |
| Logback `%d [%thread] %level` | `17:04:22.123 [main] ERROR c.e.MyService - Failed to connect` | **null (gap)** | **error (fixed)** |
| syslog, lowercase hostname+level | `Aug 11 09:14:05 web-server-01 app[1234]: error occurred during checkout` | null | null |
| JSON-per-line | `{"level":"error","msg":"payment failed"}` | null | null |
| logfmt | `time=2026-08-11T09:14:05Z level=error msg=timeout` | null | null |
| Quoted mention of a level word | `"ERROR" is a reserved word in this DSL, see docs.` | null | null |
| Fifth false positive (lone capitalized lead-in word) | `A warning icon appears next to any file that couldn't be scanned.` | null | null |

Only the Logback row changed; every other row — including all prose negative controls — is identical
before/after, confirming no new false positives were introduced while closing one of the two named gaps.

Added `detectLevel — F1: previously-undetected mainstream formats` describe block (4 new tests: Logback
bracket-tag detection, a second Logback line with a longer thread-pool-shaped tag, a bracketed-but-
whitespace-containing prose remark confirmed still NOT flagged, and the RFC3164 gap committed as an
explicit "stays null" regression test with its reasoning in the test name/body).

Verification: `npm run check` 0 errors/0 warnings; `npx vitest run` — 287 files / 3670 tests pass (up from
3660 baseline; +10 across this round's F1 and CPE-1638's F2 fixes, no regressions). No Rust touched by this
round (JS/TS-only, same as before).
