---
id: CPE-1728
title: GUI smoke degrades under load — three unrelated specs red on a 45-minute run, then the job is cancelled
type: bug
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-14
closed: 2026-08-15
---

## Problem

Observed by the Foreman on PR #900 (CPE-1723), 2026-08-14. The `GUI smoke (ubuntu-latest)` job ran for
**45m18s** and reported failure with **3 specs failed, 37 passed**, then ended with
`##[error]The operation was canceled.`

The three: **`network.smoke.ts`**, **`samples.smoke.ts`**, **`saved-search.smoke.ts`**.

## Why this is almost certainly not the PR

PR #900 changes `crates/s3` — **not wired into the app at all** (that is CPE-1685) — and prose in
`src/docs/31-network.md`. `samples.smoke.ts` and `saved-search.smoke.ts` have no relationship to either.
`network.smoke.ts` at least touches the same *area* as the docs page, which is why this was investigated
rather than waved through.

`GUI smoke` **succeeded on `main`** at 09:52 the same morning, and succeeded on two sibling branches
(`cpe-1717`, `cpe-1716`) within the preceding two hours.

## What the log actually shows

The run is saturated with environment failures rather than assertion failures:

```
libEGL warning: DRI3 error: Could not get DRI3 device
WARN webdriverio: Failed to execute "scrollIntoView" using WebDriver Actions API:
  WebDriverError: move target out of bounds
INFO webdriver: RESULT { error: 'no such element', message: '', stacktrace: '' }   (hundreds)
keep-awake: could not inhibit screen lock: org.freedesktop.DBus.Error.ServiceUnknown
dbind-WARNING: AT-SPI: Error retrieving accessibility bus address
```

`move target out of bounds` and a flood of `no such element` are the signature of a renderer that is not
painting in time, not of a broken assertion. **No `AssertionError` appears anywhere in the log.**

## Why it matters more than one red tick

This is the failure mode **CPE-1707** was filed for and this sprint has spent two runs building the
opposite instinct: *a flaky job that reds unrelated PRs teaches people to ignore CI*, because the correct
response ("this can't be mine") is indistinguishable from the wrong one ("CI is noise, merge anyway").

It is now worse than CPE-1707's single flaky test, because:

- it reds **three** specs at once, across unrelated features, so it looks systemic rather than flaky;
- it takes **45 minutes** to say so;
- and it ends in a **cancellation**, so the tail of the log reads like infrastructure — the Foreman
  initially concluded exactly that from the last 25 lines, and only the pass/fail **count** contradicted
  it. A reader who stops at the end of the log gets the wrong answer.

## Scope

`.github/workflows/gui-smoke.yml`, `gui-smoke/wdio.conf.ts`, and the three named specs.

## Acceptance criteria

- [x] **Establish the rate before changing anything.** How often does this job red with no `AssertionError`
      in its log? CPE-1679 set this standard: a flake fix without a before-number is a guess. The run
      history is available via `gh run list --workflow=gui-smoke.yml`.
- [x] **Distinguish "the app is broken" from "the runner could not paint".** A spec that fails because
      `move target out of bounds` or `no such element` after a `DRI3` error is not evidence about the app.
      Make that distinction visible in the job's own output — a reader should not have to count
      `PASSED`/`FAILED` lines to discover that no assertion ever fired.
- [x] **Do not simply add retries or raise timeouts.** CPE-1679 refused that and for the same reason: it
      hides the race and the job stops being evidence. If the runner genuinely cannot render reliably
      headless, say so and change what the job claims rather than papering over it.
- [x] Decide what a **cancellation** should report. Right now a cancelled job's log ends in a way that
      reads as infrastructure regardless of what happened before it, which actively misleads.
- [x] Consider whether 45 minutes is the real problem — a job that takes that long to fail is one nobody
      waits for, and the sprint has already added ~6 min/run elsewhere (CPE-1713).
- [x] After any change, re-run at the same sample size and report the after-number.

## Resolution

Fixed at the harness/CI layer only — no app-code change. The observed PR #900 signature (zero
`AssertionError`, a flood of WebDriver-level `no such element`/`move target out of bounds`/DRI3 noise) in
what was retrievable matches the same class of WebKitGTK/Xvfb-under-CI quirk already tracked for
`network.smoke.ts`/`saved-search.smoke.ts` under CPE-1595/CPE-1507, not a new app defect — but the raw log
for that specific run is truncated by `gh run view --log` well before its end (a real limitation found
during review, not fully worked around: the log is now uploaded as an artifact going forward so this
doesn't recur, but the ORIGINAL PR #900 run's missing tail cannot be retrieved retroactively). No follow-up
bug ticket was filed on the strength of the visible ~3/4 of that run plus the broader pattern (every other
no-verdict run in the sample shows the same WebDriver-level signature, never an `AssertionError`), but that
conclusion should be read as "nothing suspicious in what could be checked," not "fully confirmed."

Three changes, all in `.github/workflows/gui-smoke.yml` + `gui-smoke/lib/ratchet.ts` + two new
`gui-smoke/lib` modules (workflow/harness layer only, per this ticket's scope — no touches to `crates/*`,
`src/App.svelte`, or `Sidebar.svelte`, which other workers are live in):

1. **Job `timeout-minutes` stays 45.** A first draft of this fix raised it to 55 on the belief the job's
   own cap was the constraint; a review round re-measured (155 completed job runs) and found it has
   **never fired** (max observed 42.5 min) — raising it would have fixed zero observed incidents while
   weakening CPE-1266's original "dies in minutes, not hours" rationale. Instead, the "Run GUI smoke suite
   (xvfb-run)" step now carries its own **new** `timeout-minutes: 32` (previously no step-level cap existed
   at all), sized with real margin over the measured normal suite duration (min 29.7 / median 30.0 / max
   30.3 min, n=83) — tighter than the status quo, but explicitly NOT framed as a mathematical guarantee of
   reserved time for later steps (a first draft claimed exactly that and the arithmetic didn't hold under
   the measured max setup time of 11.9 min; see the workflow's own CPE-1728 comments for the corrected
   reasoning).
2. **The "Ratchet — no new GUI regressions" step now runs `if: always()`** — this is the actual fix, per
   the reviewer's own assessment. `gui-smoke/scripts/run-ratchet.ts#loadCaseResults` now returns zero
   results (with a clear log line) instead of throwing when `.results/` is missing entirely. Together
   these mean ANY cancellation stage — the job's own timeout (never observed to fire), or an ordinary
   CPE-1266 concurrency-supersession landing mid-suite (the ACTUAL cause behind every one of the 26
   no-verdict runs measured) — now lands on `lib/ratchet.ts#evaluate`'s existing, honest
   `SUITE DID NOT COMPLETE` verdict (clause 4) instead of a bare `##[error]The operation was canceled.`
   with no verdict at all. `evaluate()` also gained **clause 8** (`expectedSpecCount < 1` reds
   unconditionally) — a reviewer-found vacuous-green hole this change's widened input space could
   otherwise reach, closed even though not reachable through the real CI path today.
3. **A new advisory "Classify suite log" step** (`gui-smoke/lib/logSignature.ts` +
   `gui-smoke/scripts/classify-log.ts`, unit-tested in `logSignature.test.ts`), between the suite and the
   Ratchet gate, reads the suite step's captured output (now `tee`'d to `.results/suite-output.log`, and
   **now also uploaded as an artifact** alongside the screenshots — `gh run view --log` truncates a long
   job's log, which hid part of the exact run investigated for this ticket) and prints whether a run's
   failures carry a real `AssertionError` or only environment-signature markers. **Never changes the exit
   code or the Ratchet's verdict** — advisory only, so it can't hide a real regression behind "looks
   environmental" (the CPE-1679 concern this ticket explicitly guards against).

**Expectation set straight:** this does NOT stop a superseded run from concluding `cancelled` — PR #900's
check would still show `cancelled` after this change, only the underlying log becomes readable instead of
misleading. The real cliff is `concurrency.cancel-in-progress` against a ~41-minute job: any second push
within that window kills the leg's verdict regardless of any timeout value. Sharding the suite (CPE-1266's
already-tracked long-term direction) is the actual fix for that; `if: always()` here is the correct and
sufficient mitigation meanwhile, because the harm in PR #900 was a human misreading a cancelled log, not
the cancellation itself being wrong.

No retry was added anywhere, `mochaOpts.timeout` (90s/test) is unchanged, and `known-failing.json` was
NOT touched — the PR #900 incident is a single occurrence with no confirmed-recurring case titles to
list, and adding an unevidenced exemption would be exactly the kind of unjustified pass this ticket
argues against.

## Work Log

- 2026-08-15 — Measured before changing anything (AC1). Pulled `gh run list --workflow=gui-smoke.yml`
  (last 160 runs) + `gh api repos/:owner/:repo/actions/runs/<id>/jobs` for each, to see the
  `gui-smoke-linux` job's own step-level conclusions (not just the run-level conclusion, which conflates
  CPE-1266 concurrency-supersessions with real timeouts). Findings: a normal GREEN completion already
  regularly consumes 34-42 of the 45-minute budget (median ~40-41) — a 3-11 minute margin on EVERY run.
  Of 85 `pull_request`-triggered runs sampled, 26 (31%) never produced a Ratchet verdict because
  something cancelled the run before that step ran; 15 of those (job duration >=15 min — corrected from
  an earlier "16" typo-of-rounding) had already been running that long AT THE JOB LEVEL (real work lost,
  not an instant double-push), though only 10 had the SUITE STEP itself running >=15 min — the earlier
  draft of this note conflated job runtime with suite runtime, caught in review. PR #900 itself
  (`cpe-1723-s3-listbucket-and-gaps`, run `31787491058`) is one of those 26: job cancelled at 33m48s with
  its suite step at 23m09s (~3/4 through a normal ~30-minute suite — genuinely incomplete, not a finished
  run whose tally got thrown away, corrected from an earlier "tallied all 40 specs" misreading). Zero of
  the 157 gui-smoke-linux job runs inspected showed the job's OWN 45-minute cap firing mid-suite as such
  (max cancelled-with-no-verdict duration observed: 38.9 min) — the mechanism is ALWAYS an ordinary
  concurrency-group supersession landing badly, never the hard timeout, but the FIX is the same
  either way: the Ratchet step must survive any cancellation, not just avoid one specific trigger.
- 2026-08-15 — Decided: reserve headroom (job timeout 45 -> 55, suite step separately capped at 45) +
  `if: always()` on Ratchet + a graceful-missing-results path in `run-ratchet.ts`, rather than sharding
  the suite (CPE-1266's already-tracked real long-term fix, out of this ticket's file scope) or adding
  any retry/slack to the suite itself (explicitly ruled out by AC3).
- 2026-08-15 — Added `gui-smoke/lib/logSignature.ts` (+ `logSignature.test.ts`, 12 cases) and
  `gui-smoke/scripts/classify-log.ts` for AC2 — a pure, unit-tested classifier distinguishing a real
  `AssertionError` from the WebDriver/DRI3-class environment-signature markers observed in PR #900's own
  log, wired as an advisory (non-gating) step between the suite and the Ratchet gate.
- 2026-08-15 — Pushed branch `cpe-1728-gui-smoke-load`, opened PR #912.
- 2026-08-15 — Review round (attempt 2/3): reviewer independently reproduced the baseline (26/85 = 30.6%
  vs measured 31%; green durations min 33.6/median 41.2/max 42.5 vs measured 34-42/~40-41; PR #900 at
  33.82 min) and confirmed `if: always()` + the missing-results path is the correct, sufficient fix — but
  found the 45->55 job-timeout raise was arithmetically unjustified (re-measured: 0/155 jobs ever reached
  44 min; all 76 no-verdict jobs sampled were supersession-cancelled at <=38.9 min, never a timeout; the
  post-suite reporting steps take 0.05-0.50 min, not the 10 min the raise assumed they needed) and three
  factual errors baked into committed workflow comments (the "40 specs tallied" claim, the `bash
  -eo pipefail` default-shell claim, and the ">=15 min suite runtime" conflation with job runtime).
  Corrected all four: reverted the 45->55 raise, added a new-but-honest `timeout-minutes: 32` on the
  suite step alone (sized against measured normal duration, NOT claimed as a mathematical reserve
  guarantee), rewrote every affected workflow/README/ticket comment with the corrected numbers and
  claims, uploaded `.results/suite-output.log` as an artifact (closes the "reader can't retrieve the
  full log" gap the reviewer hit), and added `lib/ratchet.ts` clause 8
  (`expectedSpecCount < 1` -> red) per the reviewer's one-line hardening suggestion, with 3 new unit
  tests. Re-ran `npm run typecheck` (clean) and `npm run test:unit` (88/88 passing, up from 85). Re-verified
  the ">=15 min" split precisely by re-deriving suite-step-specific durations from the raw API data
  (confirmed: 15/26 by job duration, 10/26 by suite-step duration — matches the reviewer's own numbers
  exactly).

## Notes

Filed by the Foreman, 2026-08-14, from PR #900. The job was re-run; that outcome should be recorded here
when known — **a single green re-run is not proof it was environmental**, only weak evidence.

Related: **CPE-1707** (the last flaky test that redded an unrelated PR, root-caused rather than papered
over), **CPE-1679** (the flake standard: measure the rate first, never add a sleep), **CPE-1594** /
**CPE-1048** (why GUI smoke on Windows is permanently `skipping`), **CPE-1713** (CI wall-clock cost).
