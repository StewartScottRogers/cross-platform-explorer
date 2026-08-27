---
id: CPE-1910
title: GUI smoke shard 2 dies on a WebDriver socket failure often enough to cost a re-run per PR
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-26
---

## Summary

`GUI smoke (ubuntu-latest) shard 2` fails with a WebDriver session crash — not a test failure —
frequently enough that it is now a routine tax on merging. Observed **three times on 2026-08-26 alone**,
on three unrelated PRs (#1032, #1033, #1034), none of which touched any GUI code: one changed markdown
and two standalone `.mjs` scripts, one changed a release workflow and a Rust crate, one changed a Rust
crate and a workflow.

The signature is identical every time:

    Error: WebDriverError: Request failed with error code UND_ERR_SOCKET when running "elements" ...
    ERROR @wdio/local-runner: Failed launching test session: ... UND_ERR_SOCKET ... 127.0.0.1:4444/session
    [gui-smoke log-signature] 0 AssertionError occurrence(s), 4 environment-signature occurrence(s)
    [gui-smoke log-signature] ENVIRONMENT SIGNATURE ONLY — no AssertionError anywhere in this run's output
    [gui-smoke ratchet] FAILED — 0 new failing case(s), 0 now-passing entries, 0 stale entries ...

The driver's socket dies, often **before a session is even created** (`POST /session` itself fails), so
the run reports failure having asserted nothing. The suite's own log-signature check correctly
identifies this as environment-only — which is excellent, and is the reason each occurrence was
diagnosable in minutes rather than chased as a real regression.

**The diagnosis already works. What is missing is the response.** Every occurrence costs a manual
`gh run rerun --failed` and a wait, and a re-run cannot even be issued while the rest of the workflow is
still in progress. On an unattended run that is a stall; with a human it is a tax.

## Acceptance criteria

- [ ] Measure the real rate before changing anything. Pull the recent history of this job and report how
      often it fails with the environment signature versus a genuine assertion. Three in one day is the
      observation that prompted this; the fix should be sized to the measured number, not to the anecdote.
- [ ] Establish **why the socket dies**, at least to the point of naming the layer. Is `tauri-driver`
      crashing, is the app under test dying and taking the session with it, is it runner resource
      exhaustion (shard 2 specifically, or any shard), or is it a race between driver startup and the
      first command? "Retry it" without this is a guess.
- [ ] Make the suite **retry a session that dies before any assertion runs**, automatically and once,
      rather than failing the job. The `log-signature` check already distinguishes this case reliably —
      reuse that signal rather than inventing a second classifier.
- [ ] A retried-and-recovered run must say so **loudly** in the job summary, with a count. Silent retries
      hide a worsening rate, and this repo has a live ticket (CPE-1893) about a job whose silence hid a
      month-long outage.
- [ ] Do not retry a genuine assertion failure. Red-proof that: make a spec fail for real and confirm it
      is not retried and the job stays red.
- [ ] If shard 2 is disproportionately affected, say why — the shard split is by spec file, so an uneven
      split or one heavy spec could be the whole story. `gui-smoke/README.md` and CPE-1753/CPE-1858
      cover the sharding.

## Notes

Filed 2026-08-26 by the sprint Foreman after the third occurrence in one day. Each was diagnosed from
the raw job log (fetched via `gh api repos/:owner/:repo/actions/jobs/<id>/logs`, ~14,000 lines, and
confirmed complete by reading the tail) and re-run; each re-run then passed, which is itself evidence
the failures were environmental.

Related: **CPE-1171** (the shard), **CPE-1753** (build once for every shard), **CPE-1858**
(shard balance), **CPE-1886** (a double reset failure escaping per-file containment in `wdio.conf.ts` —
possibly the same underlying fragility, worth reading first), **CPE-1832** (`guismoke-driver-race`), and
**CPE-1866** (session-per-shard, which established that the per-spec overhead was app-launch).

Note the `log-signature` and `ratchet` machinery that made this diagnosable is genuinely good work and
should not be disturbed — the fix belongs upstream of it, in how a dead session is handled, not in how
it is reported.
