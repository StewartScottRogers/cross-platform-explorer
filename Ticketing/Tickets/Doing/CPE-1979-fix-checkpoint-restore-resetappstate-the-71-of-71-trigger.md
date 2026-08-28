---
id: CPE-1979
title: Fix `checkpoint-restore.smoke.ts`'s `resetAppState` — it is the trigger in 71 of 71 shard-2 jobs, and fixing it makes CPE-1955 and CPE-1910 near-dead code
type: bug
priority: High
status: In Progress
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
`CPE-1181-archive.tar.gz` (150 consecutive polls in job 98786417910; same in 98928416795).
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
