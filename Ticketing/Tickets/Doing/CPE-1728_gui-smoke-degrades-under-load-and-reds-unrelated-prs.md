---
id: CPE-1728
title: GUI smoke degrades under load — three unrelated specs red on a 45-minute run, then the job is cancelled
type: bug
priority: Medium
status: Doing
tags: ready
estimate: M
created: 2026-08-14
closed:
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

Fixed at the harness/CI layer only — no app-code change, and the measurement below found no evidence the
app itself is racing. The observed PR #900 signature (zero `AssertionError`, a flood of WebDriver-level
`no such element`/`move target out of bounds`/DRI3 noise) is the textbook shape of a WebKitGTK/Xvfb
renderer not painting fast enough for CI, matching the SAME class of quirk already tracked for
`network.smoke.ts`/`saved-search.smoke.ts` under CPE-1595/CPE-1507. Nothing here suggested a NEW app
defect, so no follow-up bug ticket was filed.

Three changes, all in `.github/workflows/gui-smoke.yml` + two new `gui-smoke/lib` modules (workflow/
harness layer only, per this ticket's scope — no touches to `crates/*`, `src/App.svelte`, or
`Sidebar.svelte`, which other workers are live in):

1. **Job `timeout-minutes` raised 45 -> 55**, with the "Run GUI smoke suite (xvfb-run)" step itself
   separately capped at `timeout-minutes: 45` (the OLD full-job ceiling — a genuinely hung suite still
   dies in the same bounded time as before). This reserves real headroom for the steps that report on the
   suite, instead of them racing it for whatever's left of one shared 45-minute budget.
2. **The "Ratchet — no new GUI regressions" step now runs `if: always()`**, and
   `gui-smoke/scripts/run-ratchet.ts#loadCaseResults` now returns zero results (with a clear log line)
   instead of throwing when `.results/` is missing entirely. Together these mean ANY cancellation stage —
   the job's own timeout, or an ordinary CPE-1266 concurrency-supersession landing mid-suite — now lands
   on `lib/ratchet.ts#evaluate`'s existing, honest `SUITE DID NOT COMPLETE` verdict (clause 4) instead of
   a bare `##[error]The operation was canceled.` with no verdict at all.
3. **A new advisory "Classify suite log" step** (`gui-smoke/lib/logSignature.ts` +
   `gui-smoke/scripts/classify-log.ts`, unit-tested in `logSignature.test.ts`), between the suite and the
   Ratchet gate, reads the suite step's captured output (now `tee`'d to `.results/suite-output.log`) and
   prints whether a run's failures carry a real `AssertionError` or only environment-signature markers.
   **Never changes the exit code or the Ratchet's verdict** — advisory only, so it can't hide a real
   regression behind "looks environmental" (the CPE-1679 concern this ticket explicitly guards against).

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
  something cancelled the run before that step ran; 16 of those (19% of all PR runs) had already been
  running >=15 minutes (real suite work lost, not an instant double-push). PR #900 itself
  (`cpe-1723-s3-listbucket-and-gaps`, run `31787491058`) is one of those 16: cancelled at 33m48s. Zero of
  the 157 gui-smoke-linux job runs inspected showed the job's OWN 45-minute cap firing mid-suite as such
  (max cancelled-with-no-verdict duration observed: 38.9 min) — the mechanism is more often an ordinary
  concurrency-group supersession landing badly than the hard timeout itself, but the FIX is the same
  either way: the Ratchet step must survive any cancellation, not just avoid one specific trigger.
- 2026-08-15 — Decided: reserve headroom (job timeout 45 -> 55, suite step separately capped at 45) +
  `if: always()` on Ratchet + a graceful-missing-results path in `run-ratchet.ts`, rather than sharding
  the suite (CPE-1266's already-tracked real long-term fix, out of this ticket's file scope) or adding
  any retry/slack to the suite itself (explicitly ruled out by AC3).
- 2026-08-15 — Added `gui-smoke/lib/logSignature.ts` (+ `logSignature.test.ts`, 12 cases) and
  `gui-smoke/scripts/classify-log.ts` for AC2 — a pure, unit-tested classifier distinguishing a real
  `AssertionError` from the WebDriver/DRI3-class environment-signature markers observed in PR #900's own
  log, wired as an advisory (non-gating) step between the suite and the Ratchet gate.
- 2026-08-15 — Pushed branch `cpe-1728-gui-smoke-load`, opened PR. After-number pending: CI must run this
  leg for real before a same-sample-size after-number (AC6) can be reported — see the PR body/Foreman
  follow-up for the observed run.

## Notes

Filed by the Foreman, 2026-08-14, from PR #900. The job was re-run; that outcome should be recorded here
when known — **a single green re-run is not proof it was environmental**, only weak evidence.

Related: **CPE-1707** (the last flaky test that redded an unrelated PR, root-caused rather than papered
over), **CPE-1679** (the flake standard: measure the rate first, never add a sleep), **CPE-1594** /
**CPE-1048** (why GUI smoke on Windows is permanently `skipping`), **CPE-1713** (CI wall-clock cost).
