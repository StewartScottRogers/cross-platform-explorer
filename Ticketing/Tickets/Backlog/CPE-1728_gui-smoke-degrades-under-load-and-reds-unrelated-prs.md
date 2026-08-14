---
id: CPE-1728
title: GUI smoke degrades under load — three unrelated specs red on a 45-minute run, then the job is cancelled
type: bug
priority: Medium
status: Backlog
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

- [ ] **Establish the rate before changing anything.** How often does this job red with no `AssertionError`
      in its log? CPE-1679 set this standard: a flake fix without a before-number is a guess. The run
      history is available via `gh run list --workflow=gui-smoke.yml`.
- [ ] **Distinguish "the app is broken" from "the runner could not paint".** A spec that fails because
      `move target out of bounds` or `no such element` after a `DRI3` error is not evidence about the app.
      Make that distinction visible in the job's own output — a reader should not have to count
      `PASSED`/`FAILED` lines to discover that no assertion ever fired.
- [ ] **Do not simply add retries or raise timeouts.** CPE-1679 refused that and for the same reason: it
      hides the race and the job stops being evidence. If the runner genuinely cannot render reliably
      headless, say so and change what the job claims rather than papering over it.
- [ ] Decide what a **cancellation** should report. Right now a cancelled job's log ends in a way that
      reads as infrastructure regardless of what happened before it, which actively misleads.
- [ ] Consider whether 45 minutes is the real problem — a job that takes that long to fail is one nobody
      waits for, and the sprint has already added ~6 min/run elsewhere (CPE-1713).
- [ ] After any change, re-run at the same sample size and report the after-number.

## Notes

Filed by the Foreman, 2026-08-14, from PR #900. The job was re-run; that outcome should be recorded here
when known — **a single green re-run is not proof it was environmental**, only weak evidence.

Related: **CPE-1707** (the last flaky test that redded an unrelated PR, root-caused rather than papered
over), **CPE-1679** (the flake standard: measure the rate first, never add a sleep), **CPE-1594** /
**CPE-1048** (why GUI smoke on Windows is permanently `skipping`), **CPE-1713** (CI wall-clock cost).
