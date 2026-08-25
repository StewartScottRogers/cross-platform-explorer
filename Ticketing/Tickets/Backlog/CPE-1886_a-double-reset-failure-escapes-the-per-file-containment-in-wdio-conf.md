---
id: CPE-1886
title: a double reset failure escapes the per-file containment in wdio.conf.ts and repeats for every runnable after it
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-25
closed:
---

## Problem

CPE-1866 moved gui-smoke to one WebDriver session per shard, and added containment so a failing
`resetAppState` fails **that file** rather than poisoning the rest of the shard. For a *single* reset
failure it works, and that is the case CPE-1866's own CI history actually produced.

It does not hold on a **double** failure.

In `gui-smoke/wdio.conf.ts`'s `handleRunnableStart` (~lines 1095-1119): the `catch` calls
`browser.reloadSession()` and retries `resetAppState` once. If the retry succeeds, `currentSpecFile = file`
runs and everything proceeds. But **`currentSpecFile = file` is the last line of the function, after the
try/catch, with no fallback** — if the retried call *also* throws, the exception propagates out of
`handleRunnableStart` and `currentSpecFile` is never updated.

The next runnable — the same file's next test, or the next file entirely — then sees
`file !== currentSpecFile` again, re-enters the reset branch, and repeats the whole
reset → `reloadSession` → retry sequence from scratch. And again for every runnable after that, until
one attempt finally succeeds or the job hits its 35-minute timeout.

Each cycle costs roughly `2 × connectionRetryTimeout` (~180s each) plus an app relaunch.

## Severity, stated precisely

**It never produces a false pass.** Failures stay loud, the incremental per-file result flush still
writes what completed, and the ratchet's "SUITE DID NOT COMPLETE" clause still reports the shortfall
accurately. Green still means green — which is why this is Medium and not High.

What it costs is **budget**: a rare double failure can burn far more of the job's 35 minutes than a
clean single-file failure would, and it does so while looking like a hang rather than a failure.

Found by PR #1011's independent UAT, which was asked whether the mitigation *contains* a cascade or
merely *moves* it. Its answer, verbatim: **"it contains the common case and moves the rare case, more
expensively than before."** That is the correct reading, and it is worth preserving as the framing.

## What to do

The fix is likely one line: set `currentSpecFile = file` **before** the reset attempt, or in a `finally`,
so a file is never retried by the next runnable regardless of how its reset went.

Think about which is right rather than picking the shorter one:
- setting it *before* means a file whose reset failed twice is simply not reset, and its tests run on
  whatever state was there — loud failures, but on an unknown baseline;
- setting it in a `finally` is the same, expressed more explicitly;
- a third option is to fail the **shard** deliberately after a double reset failure, on the grounds
  that two consecutive failures mean the session is unusable and continuing wastes the budget for a
  result nobody can trust.

Record the reasoning. State what a maintainer sees in each case.

**Prove it.** Force `resetAppState` to throw twice (an injected fault is fine — do not break a real
spec) and show the current code repeating the cycle across subsequent runnables, then show the fix
confining it. Both outputs pasted.

## Acceptance criteria

- [ ] A double reset failure is confined to one file, or fails the shard deliberately — decided and recorded.
- [ ] Demonstrated before/after with an injected double failure.
- [ ] The single-failure containment CPE-1866 added still works — do not regress the case that was fixed.
- [ ] Whatever a maintainer sees is actionable: it must not look like a hang.

## Work Log

- **2026-08-25 13:35 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`, from
  PR #1011's UAT. Note the UAT had failed this PR in a previous round for a genuine regression, then
  passed it here after the root cause was found — and still returned this finding rather than waving
  the PR through on the strength of the fix. That is the behaviour the two-check gate exists for.
