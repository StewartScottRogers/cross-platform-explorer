---
id: CPE-1979
title: Fix `checkpoint-restore.smoke.ts`'s `resetAppState` — it is the trigger in 71 of 71 shard-2 jobs, and fixing it makes CPE-1955 and CPE-1910 near-dead code
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Filed by CPE-1910's worker (PR #1092, round 2), at the reviewer's instruction, so the finding is not
lost in a Work Log.

CPE-1910 and CPE-1955 are both **containment** for a WebDriver session death on `gui-smoke` shard 2.
Neither is the fix. The **cause** is measured, single, and upstream of both:

> **EVERY ONE of the 71 completed `gui-smoke (ubuntu-latest) shard 2` jobs** in the 13.5 h window
> ending 2026-08-28 08:47Z logs `handleRunnableStart:resetFailedRestartingSession` on the transition
> into **`checkpoint-restore.smoke.ts`** — green jobs included. 71 of 71.

That is `resetAppState` failing, in one identifiable place, on one identifiable spec. Everything
downstream of it is machinery built to survive it:

- **CPE-1955** (in-process `tauri-driver` respawn) fires because the recovery path's first act is
  `DELETE /session/<id>`, which in ~15% of jobs kills the native WebKitWebDriver behind tauri-driver.
  Measured post-merge: **8 of 45** shard-2 jobs used a respawn (all recovered, `refused=1, sock=5,
  respawn=1, 14/14`).
- **CPE-1910** (job-level suite retry) is the backstop for a **second** transport death in one shard,
  which CPE-1955's budget of 1 leaves red. Pre-merge that was **2 of 26** jobs (`refused=841,
  sock=1300, 1/14`) — each one a manual `gh run rerun`.

**Take the trigger to zero and both mechanisms become near-dead code.** They should still exist (they
are cheap, and they are the honest answer to "the transport can die"), but they would stop firing, and
the ~15% respawn rate — which is a real, ongoing tax on every shard-2 run — would go with it.

## What to do

1. **Read the failure, don't guess it.** The evidence is in the raw job log, not the `gh run view --log`
   spec view (which is pre-truncated and hides it):
   `gh api repos/:owner/:repo/actions/jobs/<id>/logs`. Find the first
   `resetFailedRestartingSession` and the reset error immediately before it. It is an **ordinary
   app-level reset failure** — not a transport problem — which is why it lands on the same spec
   transition every single time rather than randomly.
2. **Fix `lib/resetAppState.ts` (or the spec) so the reset succeeds** on the transition into
   `checkpoint-restore.smoke.ts`. If the reset genuinely cannot succeed there, make the spec not need
   it — a reset that is *expected* to fail must not be routed into `recoverSession`, because that path
   is what issues the session-killing `DELETE`.
3. **Do NOT delete CPE-1955's respawn or CPE-1910's retry as part of this.** They are containment for a
   class, not for this instance. Deleting them would make the next instance illegible again.

## Acceptance

- The reset on the transition into `checkpoint-restore.smoke.ts` succeeds, evidenced by
  `resetFailedRestartingSession` being **absent** from a sample of consecutive shard-2 job logs (state
  the sample size and the window — the current number to beat is 71 of 71 present).
- The in-process respawn rate drops correspondingly. CPE-1910's loud summary block already reports
  every respawn to `$GITHUB_STEP_SUMMARY`, so this is now readable **without** pulling raw logs — use
  it as the measurement, and quote passed/skipped separately.
- Both containment mechanisms remain in place and still have their tests.

## Notes

- The 71-of-71 figure and the pre/post-CPE-1955 splits are the reviewer's independent re-measurement on
  PR #1092, not the author's; the `312`-shard-job enumeration over the same 100-run window found **all**
  failures on shard 2 and **zero** on shards 1 and 3.
- Related: CPE-1955 (in-process respawn), CPE-1910 (job-level retry + the loud recovery block),
  CPE-1893 (silent retries hide a worsening rate), CPE-1886.

## Work Log

### 2026-08-28 — root cause, fix, measurements

**The named layer: the APP's frontend — `src/lib/components/NavToolbar.svelte#commit()`.** Not the
harness, not tauri-driver, not WebKitWebDriver, not the runner.

`commit()` short-circuits with `if (!value || value === currentPath) return;` *before* dispatching
`navigate`. Entering an archive never moves `currentPath` — `App.svelte#enterArchive` sets `archive` and
leaves the tab's history alone — so while you are inside `foo.tar.gz` the address bar still displays the
*containing* folder. `resetAppState`'s step 4 (`navigateTo(rootDir)`) types exactly that folder, which
equals `currentPath`, so **no `navigate` event was ever dispatched**: `onCrumbNavigate`'s `exitArchive()`
and `loadPath`'s `archive = null` (the app's single chokepoint for dismissing the archive / smart-folder /
structured-search views) were both unreachable. The app stayed inside the archive for the full 15 s
breadcrumb wait, which threw, which is `resetFailedRestartingSession`.

Read from the logs, not inferred: throughout the failing wait `[aria-current="page"]` returns
`CPE-1181-archive.tar.gz` (**151** consecutive polls in job 98786417910, spanning 07:42:43.653Z–
07:42:58.984Z = 15.33 s; 151 in 98928416795 too. An earlier draft of this log said 150 — corrected on
the Reviewer's own re-count, which is the number to trust).
`specs/archive-browse.smoke.ts` is shard 2's *first* spec and ends inside the `.tar.gz` by design — it
asserts the breadcrumb ends on the archive name — and never leaves. This is the same
`commit()` no-op that `resetAppState.ts`'s `SCROLL_CONTAINER_SELECTOR` comment already documented as
eating the scroll reset; the archive view is the fatal instance of it.

**Before rate, re-measured for this ticket** (not quoted from the ticket): 97 completed `gui-smoke` runs
over 2026-08-28T00:21:30Z–17:11:19Z (16 h 50 m) → **81** completed `shard 2` jobs → **77 of 77** whose
raw job log is retrievable and which reached the archive-browse → checkpoint-restore transition carry
exactly one `handleRunnableStart:resetFailedRestartingSession`, and it names
`checkpoint-restore.smoke.ts` every time; all 77 also carry exactly one `expected the breadcrumb to
show` error, so cause and symptom are 1:1. The remaining 4 were all `cancelled` — 3 with no retrievable
log (fetch 404), 1 cancelled before the transition. Conclusions in the sample: 63 success, 14 failure,
4 cancelled — i.e. this fires on green jobs too, as the ticket said. **11 of the 77 (14.3 %)** spent a
CPE-1955 driver respawn, confirming the ~15 % figure. Method: `gh api
repos/:owner/:repo/actions/runs/<id>/jobs` to enumerate, `gh api
repos/:owner/:repo/actions/jobs/<id>/logs` for each raw log, with a `<10 000` byte floor so an empty or
failed fetch is reported as `FETCH_FAILED`/`LOG_TOO_SMALL` rather than counted as clean.

**The fix.** `App.svelte` hoists `$: pathOverlaidByView = !!archive || !!smartFolder ||
!!structuredSearch` out of the existing `pathReadoutsSuppressed` (one declaration, two consumers, no
duplicated four-way condition; `isHome` stays on the readouts side only, because at Home `currentPath`
honestly IS the view) and threads it into `NavToolbar`. `commit()` becomes
`if (!value || (value === currentPath && !pathOverlaidByView)) return;`. This is a real user-facing bug
fix, not a harness accommodation: typing/Entering the containing folder is a natural "get me out of this
archive" and it silently did nothing.

**Deliberately NOT done, and why.** No archive escape hatch in `resetAppState` (a Backspace, a
first-crumb click), and no `afterEach` in `archive-browse.smoke.ts` — even though the harness convention
points at exactly that. Either would have made the reset pass while the app stayed broken for every real
user, and would have destroyed the only detector that found this. The harness driving the same primitive
a user drives is the feature.

**Tests + red-proof** (results also recorded at the sites, per CLAUDE.md):
- `src/App.archiveNav.test.ts` — new end-to-end case: enter the zip, open the address bar, press Enter
  with nothing typed, assert the archive is gone and the real listing is back.
- `src/lib/components/NavToolbar.test.ts` — the guard pinned in **both** directions plus the empty-value
  case, so a future "just delete the comparison" cannot sail through.
- `&& !false` (pre-fix behaviour): 2 red of 11 — the new NavToolbar case and the new App case. CPE-1366's
  Back test stayed **green**, so the two exits are covered independently and neither shadows the other.
- `&& !true` (equality comparison made unreachable): 1 red of 9 — the "does not dispatch" case.

**Coverage given up, and the argument.** Taking this trigger to zero makes CPE-1955's respawn and
CPE-1910's retry stop firing on the path that has been exercising them ~77 times a day. Both stay —
neither deleted nor weakened — and both now say at their own sites that they are near-dead on this path.
Rejected compensations, with reasons: a **synthetic respawn exercise** every run (~35 s, a new flake
source inside the gate, and it pegs the respawn counter at ≥1 forever, destroying the one live signal);
an **alert when the respawn count stays at 0 for N runs** (it fires on *health*, so it gets muted, and it
cannot distinguish a healthy-and-unneeded mechanism from a broken one). What we keep instead: CPE-1910's
retry *decision* logic is pure and stays deterministically exercised on every push
(`lib/sessionRetry.test.ts`, `lib/runSuite.integration.test.ts` driving the real `scripts/run-suite.ts`),
so that half loses nothing; the in-process half (`recoverSession`/`respawnTauriDriver`, which need a live
driver and a real socket) is the part that genuinely goes uncovered, and it is named as such rather than
covered by a test that would only assert the mock. The standing signal is that **zero is now the
baseline**: CPE-1910's `$GITHUB_STEP_SUMMARY` block already prints every respawn, so a count going back
above zero is a visible report instead of the daily background noise it has been all month.

**Docs.** `src/docs/explorer-archives.md` gains a "Getting back out" bullet naming all four exits,
including the address-bar one that did not work.

### 2026-08-28 — after rate, measured on the branch

Sample size and window, not an adjective. **0 of 3** consecutive completed `gui-smoke (ubuntu-latest)
shard 2` jobs on this branch, window 2026-08-28T19:02:32Z–19:25:18Z: job 98953638286 (run 33200926262,
head `b4d42738`), job 98956199641 (run 33202369526, head `a62f5535`) and job 98958843376 (run
33203108701, head `ed9ee3fa`). Raw job logs pulled the same way as the before sample; the third was
re-pulled independently by the Reviewer, which is how this figure stopped understating itself at 2.
Identical readings in all three:

- `handleRunnableStart:resetFailedRestartingSession` — **0** (before: 77 of 77)
- `expected the breadcrumb to show` — **0** (before: 77 of 77)
- `respawning tauri-driver` — **0** (before: 11 of 77)
- Every spec-file transition logged `resetDone` (13 resets over 14 files — the first file skips by
  design), the archive-browse → checkpoint-restore one included; `attempt 1: suite exited 0; 14/14 spec
  file(s) reported; ... 0 in-process driver respawn(s)`.
- Rest of the first run: shards 2, 3 and 4 green, layout guard green, launcher contrast green.

**What n=3 does and does not establish.** The before rate was 77 of 77 = **100 %**, so the mechanism was
deterministic; three consecutive clean jobs falsify "always", which is the claim being fixed. It cannot
bound a residual low-rate flake — that needs a post-merge window on `main`, and the reading to take is
CPE-1910's `$GITHUB_STEP_SUMMARY` respawn count, whose expected value is now 0.

**Cost removed, same-transition comparison** (before: job 98928416795; after: job 98953638286):

| | `newFile` → reset resolved |
|---|---|
| Before | **50.19 s** (15.43 s of doomed breadcrumb polling + 34.77 s session restart) |
| After | **0.50 s** |

≈ 49.7 s per shard-2 job, on every run, green ones included.

## Closing record — merged as PR #1094 (`19773815`), 2026-08-28

**The ticket was filed as a harness flake and closed as a user-facing app bug.** That is the whole result.

### What the defect actually was

`NavToolbar.svelte#commit()` returned early on `if (!value || value === currentPath)` **before** dispatching
`navigate`. `App.svelte#enterArchive` sets `archive` and leaves the tab's history alone, so `currentPath`
never moves when you enter an archive — the address bar keeps showing the archive's **containing folder**
while the listing shows its inner entries. Re-entering that same path (a user's natural "get me out") was
**silently swallowed**, so `onCrumbNavigate`'s `exitArchive()` and `loadPath`'s `archive = null` — the app's
single chokepoint for dismissing the archive / smart-folder / structured-search views — were **unreachable
from the address bar**.

Fix: `App.svelte` hoists `$: pathOverlaidByView = !!archive || !!smartFolder || !!structuredSearch` out of
the existing CPE-1854 boolean (one declaration, two consumers) and threads it in; the guard becomes
`value === currentPath && !pathOverlaidByView`.

### Two fixes the worker refused, and said why at the site

An archive escape hatch in `resetAppState`, or an `afterEach` in `archive-browse.smoke.ts`. **Either would
have passed the reset while the app stayed broken for every user — and would have destroyed the only
detector that found this.** The harness was not lying; it was reporting.

### Measured, with the population stated

- **Before: 77 of 77.** 97 completed `gui-smoke` runs over 2026-08-28T00:21:30Z→17:11:19Z (16 h 50 m) → 81
  completed shard-2 jobs → 77 with a retrievable log that reached the transition, **every one** carrying
  `resetFailedRestartingSession` naming `checkpoint-restore.smoke.ts`, with exactly one
  `expected the breadcrumb to show` error (cause/symptom 1:1). 11 of 77 (14.3 %) spent a driver respawn.
  **4 cancelled jobs were excluded and reported as `FETCH_FAILED`/`LOG_TOO_SMALL` by a 10,000-byte floor
  rather than counted as clean.**
- **After: 0 of 3** consecutive completed shard-2 jobs, 19:02:32Z→19:25:18Z. Same-transition cost
  **50.19 s → 0.50 s**, ≈ 49.7 s off every shard-2 job.
- **The ticket title's "71 of 71" and the report's "77 of 77" are both right for their own windows** — 71 was
  CPE-1910's reviewer's figure over a 13.5 h window ending 08:47Z; 77 is this ticket's own later
  re-measurement. Recorded rather than silently reconciled.
- **What n=3 does not do, stated in the PR and unchanged through review:** it cannot bound a residual
  low-rate flake. The before rate was 100 %, so consecutive clean jobs falsify *"always"* — which is the
  claim being fixed. Bounding needs a post-merge window on `main`.

### Coverage given up, and the argument for giving it up

CPE-1955's respawn and CPE-1910's retry go near-dead on this path. Both now say so **at their own sites**.
Rejected: a **synthetic respawn exercise** (~35 s, a real driver kill as a new flake source inside the gate,
and it pegs the counter at ≥1 forever — destroying the one live signal); and an **alert on "0 respawns for N
runs"** (it fires on *health*, so it gets muted, and cannot tell healthy-and-unneeded from broken). Kept:
CPE-1910's retry *decision* logic is pure and stays deterministically exercised every push
(`sessionRetry.test.ts`, `runSuite.integration.test.ts`); the in-process half (`recoverSession` /
`respawnTauriDriver`) **is** genuinely uncovered and is named as such rather than papered over with a test
that would only assert the mock. The standing signal is that **zero is now the baseline** in CPE-1910's
`$GITHUB_STEP_SUMMARY` block — `run-suite.ts:285` gates it on `attempt > 1 || driverRespawns > 0`, so zero
prints "nothing to report" and any respawn prints the loud block.

### What the gauntlet actually proved

**Zero code defects.** An independent Reviewer re-derived the mechanism from source rather than from the
comments — confirming `commit()` is the **only** address-bar route to `navigate` (`on:blur` merely clears
`editingPath`), that `pathOverlaidByView` reaches the guard, and that the extra dispatch is cheap:
`visit()` returns history unchanged for the same path (**no duplicate entry**) and `loadListing` runs with
`useCache = true`. It re-ran both sabotages and got the author's counts exactly — `&& !false` → **2 red of
11** with CPE-1366's Back test green; `&& !true` → **1 red of 9**.

**All six findings were claim-scope**, and the correction round fixed sentences, not behaviour:

1. *"The three views that render a listing other than `currentPath`'s own"* — **Replay mode is a fourth**,
   and the same file says so 100 lines up. The exclusion is functionally correct (`loadPath` never clears
   `replayOverlayEntries`), so the wording became **"the three views that `loadPath` dismisses"**.
2. *"Exactly one declaration of the condition"* held for the **variable**, not the condition — the raw
   boolean still appears inline at four pre-existing sites, two of them pinned on purpose by
   `App.statusBarCountStaleness.test.ts`. Reworded and the four named as out of scope.
3. A comment quantified over "the window" while **4 of 81 jobs were never inspected**. The PR body disclosed
   that; the comment did not — **and the comment is what survives**. The author applied the same correction
   to a second file nobody had flagged, on the grounds that it was the same defect.
4. **151 polls, not 150** — in both jobs. Corrected *as a correction*, naming the old figure, so the change
   is visible rather than silently swapped.
5. The after-rate **understated itself**: a third clean job existed on the PR head. 0 of 2 → **0 of 3**, with
   the "cannot bound a residual flake" sentence otherwise untouched.
6. Docs: `explorer-smart-folders.md` and `explorer-saved-searches.md` now say how to get back out, since the
   fix works there too. `explorer-archives.md` gained "Getting back out". No new section, so no
   `sectionDocs.ts` entry — judgement checked and correct.

### Gates at merge

`npm test` 359 files / 5,380 passed / 2 skipped (pre-existing `it.skipIf(!ghStubWorks)` pair, file
untouched) · `npm run check` 0/0 · `gui-smoke` unit 181 passed / 51 suites / 0 skipped · `gui-smoke`
typecheck clean · `ratchet-baselines.mjs compare origin/main` 13 enumerated, all unchanged ·
`audit-npm-projects.mjs` **both** projects, root 10 + `gui-smoke/` 15 = **25**, identical to `origin/main` ·
CI `completed success — total_count=26 pending=0 skipped=1 coverage=ok`.

`bidiEscape.guard.test.ts`'s two `App.svelte` line registries shifted twice (9/10 lines, then 17 more);
counts unchanged at 31 and 2, so **no `RATCHETS.md` row** — reasoning verified against the doc.

**Family:** CPE-1955 and CPE-1910 (the respawn and retry this makes near-dead), CPE-1965 (the sibling
spec-side fix), CPE-1854 (the boolean this hoists), CPE-1366 (the Back-navigation test that had to stay
green), CPE-1866 (session-per-shard, why the spec reaches the dialog so fast).
