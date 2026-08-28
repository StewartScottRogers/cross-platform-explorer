---
id: CPE-1910
title: GUI smoke shard 2 dies on a WebDriver socket failure often enough to cost a re-run per PR
type: bug
priority: Medium
status: In Progress
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

## Work Log

### 2026-08-28 — measured first, then built the backstop

**1. The measured rate, extending the Foreman's 69-job / 4.3% enumeration.**

Enumerated the **last 100 `gui-smoke.yml` runs** (2026-08-27T19:19Z → 2026-08-28T09:20Z) = **312 shard
jobs**, then pulled and classified the raw log of **every one of the 30 failures**, plus all **71**
completed `shard 2` jobs. Shard-job outcomes:

| shard | success | failure | cancelled |
|---|---|---|---|
| 1 | 69 | 0 | 9 |
| 2 | 44 | **27** | 7 |
| 3 | 73 | 0 | 5 |
| 4 | 68 | 3 | 7 |

The headline number is not one population — it is **three**, and only the first is this ticket:

| signature | count | shape in the log |
|---|---|---|
| **session died before asserting** (this ticket) | **3** of 30 | `UND_ERR_SOCKET` ×1300, `Failed launching test session` ×1, **1/14 spec files reported**, ratchet `SUITE DID NOT COMPLETE` |
| real regression that *classifies* as environment-only | **24** of 30 | verdict `ENVIRONMENT SIGNATURE ONLY`, **14/14 reported**, one genuine failing case |
| genuine `AssertionError` (verdict `MIXED`) | **3** of 30 | 14/14 reported, real chai assertion |

So the socket death is **3 of 312 shard jobs (0.96%)**, all on shard 2, all **before CPE-1955 merged**
(3 of the 31 pre-merge shard-2 jobs = 9.7%); **0 of the 40 post-merge shard-2 jobs** were fatal.

**The 24-of-30 row is the most important finding in this ticket** and it inverts the obvious design. Those
runs carry the *exact* `ENVIRONMENT SIGNATURE ONLY` line the ticket summary quotes, while having run
every spec and failed a real case (`macro-param-prompt.smoke.ts :: running a bound {ask:suffix} macro …`).
An `expect-webdriverio` wait throws a plain `Error`, never chai's `AssertionError`, so a reproducible
regression classifies as environmental. **Retrying on the log-signature verdict alone would have re-run
genuine regressions** — CPE-1960's `scrollIntoView` defect among them, laundering ~90% into ~99%.

**2. The layer, named — and what was ruled out.**

Traced the full chain in job `98646323315` and confirmed the identical prefix in three more:

1. `resetAppState` fails before `checkpoint-restore.smoke.ts` (breadcrumb never returns to the tmp dir).
2. CPE-1886/CPE-1955's in-process recovery fires: `DELETE /session/<id>` with `shutdownDriver:false`.
3. tauri-driver logs `connection closed before message completed`, then **`tcp connect error: Connection
   refused (os error 111)`** for everything after.

**The dying layer is the native driver (WebKitWebDriver) behind tauri-driver, torn down by that session
DELETE, with a window where 127.0.0.1:**4445** is not listening.** Ruled out, each on evidence:

* **Not tauri-driver.** It is the *surviving* process — it is the one emitting the hyper errors and
  serving 4444 throughout. Its front door never stops answering.
* **Not the app under test.** The session is gone before any app command is issued; the fatal request is
  `POST /session` itself.
* **Not runner resource exhaustion.** A starved runner would not choose the same spec transition
  **71 times out of 71**. There is no OOM, no kill, no swap pressure anywhere in the logs.
* **It IS a race, but a teardown/respawn race, not the CPE-1832 startup race** — measured directly: in
  the two *recovered* jobs the reload hit exactly **one** refusal and then succeeded; in the fatal ones
  `POST /session` landed inside the same window and wdio does not retry it.

**3. Why shard 2 — and it is not the split.** The restart path is only entered when a reset fails, and in
**71 of 71** shard-2 jobs (green ones included) that is `checkpoint-restore.smoke.ts`, which shard 2
happens to own. Exposure is one spec's reset, not one shard's weight; moving the file moves the problem.

**4. What was built.** `scripts/run-suite.ts` now owns the suite step (`npm run suite`). It re-runs the
suite **once**, and only when both of two pre-existing facts hold: `logSignature`'s verdict is
`environment-signature-only` **and** fewer spec files reported than the shard owed (the ratchet's own
`incomplete` input, read through the new shared `lib/resultsDir.ts`). No second classifier was written.
Both conditions are required because they exclude the two different real populations in the table above.

**5. The loud block.** Any recovery — a job-level retry **or** a CPE-1955 in-process respawn — prints a
banner and appends a counted table to `$GITHUB_STEP_SUMMARY`. Reporting the respawn count is the larger
half: **6 of the 40** post-CPE-1955 shard-2 jobs used one, all recovered, and *nothing anywhere said so*.
That silent recovery was already the CPE-1893 shape, live in this suite; it is now counted.

**6. Red-proofs, run by hand, numbers recorded at each site.**

* **Genuine assertion failure is not retried, job stays red** — `lib/runSuite.integration.test.ts` runs
  the REAL `run-suite.ts` and the REAL `run-ratchet.ts` against a stub suite that fails a case for real:
  1 attempt (not 2), decision `suite-completed`, no summary block, **ratchet exits 1**. A second case
  covers a chai `AssertionError` on an *incomplete* shard — where the completeness condition alone would
  have said "retry" — and it is still not retried.
* **Provenance derivation** — rewording `wdio.conf.ts`'s respawn message to
  `"restarting the tauri-driver process (attempt "` fails **1 of 13** `sessionRetry.test.ts` cases;
  reverting returns 13/13.
* **Shadowed-guard sabotage pair (CPE-1929)** — disabling condition 2 reds **3 of 20**, all the
  `AssertionError` cases; disabling condition 3 reds **2 of 20**, both the real-regression cases.
  **Disjoint failure sets**, so neither refusal shadows the other. Recorded in `sessionRetry.ts`.

**7. Not touched.** `known-failing.json` (no ratchet raised, so no `RATCHETS.md` row needed), the ratchet's
verdict logic, `logSignature.ts`, and CPE-1955's respawn. `npm audit fix --force` was never run in either
npm project.

**Follow-up worth filing:** make `checkpoint-restore.smoke.ts`'s `resetAppState` succeed. It is the single
upstream cause — it takes the session-restart path from 71-of-71 to near zero, and would make both
CPE-1955's respawn and this retry near-dead code rather than load-bearing.
**FILED as CPE-1979** in round 2, at the reviewer's instruction, so it is a queue item rather than a
paragraph at the bottom of a closed ticket.

---

## Round 2 (2026-08-28)

The reviewer reproduced every measurement above and strengthened several of them (the crux is **24 of
26**, not 24 of 27; the pre/post-CPE-1955 split is **2 fatal of 26** → **0 fatal of 45, 8 respawned**;
the disjointness of the two sabotage sets was re-run and `comm -12` on them is **empty**). One blocker,
one should-fix, three nits.

**BLOCKER — `archiveAttempt` archived the shard manifest, and the verdict job then said the shard never
ran.** `.results/` holds two kinds of `*.json`: the per-spec reporter chunks the loop is for, and
`shard-manifest-<n>-of-<t>.json`, written by `npm run shard-manifest` at `gui-smoke.yml:775` **before**
the suite step. The loop moved both. The results upload at `gui-smoke.yml:906` is
`path: gui-smoke/.results/*.json` — **flat, non-recursive** — so a retried shard uploaded its results but
not its manifest, and the verdict job's join (`lib/ratchet.ts:670`, `missingShards.length === 0`) then
failed the whole gui-smoke leg with, verbatim:

> `MISSING SHARD: shard 1 of 1 never reported a manifest. Its spec files did not run — …`

about a shard that ran **twice** and **passed**. On the one scenario this script exists to handle, a
green shard produced a red, false verdict — strictly worse than the diagnosable `SUITE DID NOT COMPLETE`
it replaces. **The hazard was already known one file over**: `lib/resultsDir.ts:54` filters manifests by
`SHARD_MANIFEST_PREFIX` and `resultsDir.test.ts` has a case for exactly that co-location.

*Fix:* one line in `archiveAttempt`, the same filter, same constant.

*The finding behind the finding:* **round 1's fixture was unsharded, so no manifest ever existed in its
`.results/`** — a fixture that omits a file CI always has cannot catch a bug about that file. The fixture
is now sharded 1-of-1 and seeds its manifest by running the **real** `scripts/write-shard-manifest.ts`,
in CI's order. Two new cases: the manifest stays in the flat dir, and the **real** ratchet in the **real**
verdict-job mode exits 0. **Red-proofed by hand: reverting the filter fails exactly 2 of the 9 cases in
that file; restoring it returns 9/9.**

**SHOULD FIX — no port-release handshake between attempts.** `run-suite.ts` spawned attempt 2 the instant
attempt 1's `close` fired. wdio's teardown (`killTauriDriver`) is a bare non-waiting `tauriDriver?.kill()`,
and attempt 2 binds the same two **fixed** ports. `wdio.conf.ts:1292-1295` names this exact race in its
own words — *"racing that would have the readiness wait below succeed against the DYING listener"* — and
solves it in-process with `killAndWaitForExit`; a job-level retry cannot reach that handle. 4445 is the
worse case: the native WebKitWebDriver is a **grandchild**, never signalled, and it is precisely the
process already in a bad state on the path being retried. Added `waitForPortFree` (the mirror of
`waitForPort`, poll not sleep) + `settleDriverPorts`, and moved the two port constants into
`lib/driverPorts.ts` so `wdio.conf.ts` and `run-suite.ts` share **one** declaration rather than a copied
literal wearing a provenance claim.

**Nits, all three taken.** (1) The summary formatters printed `MAX_SUITE_ATTEMPTS` while the loop ran on
`maxAttempts()`, so `GUI_SMOKE_MAX_ATTEMPTS=3` rendered *"3 of 2 allowed"* — the budget is passed through
now, pinned by a case at a value the constant cannot produce. (2) `sessionRetry.ts:40`'s *"each attempt
costs a full shard (6-14 min)"* was **nobody's measurement**; over all 71 shard-2 jobs the suite step is
**1-2 min** and the whole job **2-3 min** (job `98661503323`: **119 s**). Replaced with the measurement,
and the budget's justification restated as what it actually is — about what a third attempt would *mean*,
not what it would cost. (3) `.screenshots/` is not attempt-scoped; the README now says the log and JSON
survive a retry and the screenshots do not.

**Scope, stated honestly** (the reviewer's judgement, carried into the PR body): the **loud block earns
its keep unambiguously** — 8 of 45 shard-2 jobs recovered silently, the CPE-1893 shape live in the suite
today. The **job-level retry is a backstop for a 0-of-45 event**, and the blocker above is the evidence
for its cost: a break on the rarely-exercised retry path survived authoring and review. It still lands —
a shard is 2 minutes, the policy is well-guarded, and CPE-1955's budget of 1 leaves a real hole — but
CPE-1979 is the fix, and this is containment.
