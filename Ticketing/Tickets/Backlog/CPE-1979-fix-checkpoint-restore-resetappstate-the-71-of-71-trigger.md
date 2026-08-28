---
id: CPE-1979
title: Fix `checkpoint-restore.smoke.ts`'s `resetAppState` — it is the trigger in 71 of 71 shard-2 jobs, and fixing it makes CPE-1955 and CPE-1910 near-dead code
type: bug
priority: High
status: Open
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
