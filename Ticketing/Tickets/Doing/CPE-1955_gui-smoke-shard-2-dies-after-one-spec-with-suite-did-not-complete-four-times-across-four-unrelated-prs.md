---
id: CPE-1955
title: `gui-smoke` shard 2 dies after one spec with "SUITE DID NOT COMPLETE" — four times today, on four unrelated PRs
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

`GUI smoke (ubuntu-latest) shard 2` has failed **four times on 2026-08-27**, across four PRs with
nothing in common, always the same way:

    [gui-smoke ratchet] SUITE DID NOT COMPLETE: expected 14 spec file(s)
    (globbed from specs/*.smoke.ts) but only 1 reported any result. A timeout,
    crash, or hang killed the job before it finished — this is treated as RED.
    [gui-smoke ratchet] 1/14 spec file(s) reported, 1 case(s) — 1 passed, 0 failed
    [gui-smoke ratchet] FAILED — 0 new failing case(s) … incomplete=true

**0 new failing cases every time.** One spec reports, thirteen never run.

Observed on **#1039** (twice), **#1056**, and **#1063** — release-plumbing, dialog copy, and catalog
trust-engine changes respectively. **None of the four diffs touches a GUI spec**, and shard 2's
fourteen specs are all main-explorer surfaces (archive-browse, trash, thumbnails, drive-menu,
instant-search, native-tags, …) unrelated to any of them.

Each occurrence was re-run and passed. That is why it has been treated as flake — but four times in
one day, always the same shard, always after exactly one spec, is a defect with a re-run as its
workaround.

## Why this is worth fixing rather than re-running

The ratchet is behaving **correctly** — CPE-1753's `incomplete=true` rule exists precisely so a
suite that dies is red rather than "everything else happened to pass". That is the right design and
must not be softened.

The cost is elsewhere: each occurrence blocks a merge for a full CI cycle, and it trains the crew to
reach for `gh run rerun` on a red GUI-smoke shard — which is exactly the habit that lets a **real**
regression through. A guard people learn to re-run is a guard that has stopped working.

## Acceptance criteria

- [x] **Find out what dies.** The ratchet reports only the aftermath. Get the job's own log for a
      failing run and identify whether it is a timeout, a crash, a hang in `tauri-driver`, an OOM, or
      a spec that never returns. **Do not fix anything until you can name the cause** — this repo has
      spent the week finding that plausible explanations are not measurements.
- [x] Establish whether it is **shard 2 specifically** or the second shard *whatever it contains*.
      Those have completely different fixes. The shard plan is deterministic, so this is answerable by
      changing the split and re-running.
- [x] Check whether the **first spec that reports** is always the same one, and whether the dying spec
      is always the second in the shard's order. If so, it is a specific spec, not the shard.
- [x] **Make the failure legible.** Whatever the cause, the job should say which spec it died in and
      why — "only 1 of 14 reported" is a symptom, not a diagnosis. That is most of the value here even
      if the underlying hang proves hard to fix.
- [x] **Do not weaken the ratchet.** `incomplete=true ⇒ RED` stays. If the fix is a retry, it must be
      a bounded retry **inside** the job that still reds when it exhausts, never an exemption.
- [x] Check the other shards' failure rates over the same period. If shard 2 is an outlier, that is
      evidence; if all four shards do this occasionally, the diagnosis changes completely.

## Notes

Filed 2026-08-27 by the sprint Foreman after the fourth occurrence, having re-run the first three.
The standing rule that produced this ticket: **re-run an infra-looking failure once, then investigate
rather than re-running a third time.**

Related: **CPE-1753** (the verdict-across-all-shards job and the `incomplete` rule — working as
designed), **CPE-1171** (the gui-smoke harness), **CPE-1679** (a prior gui-smoke timeout where
`this.timeout()` inside an `it()` body was found not to be honoured — documented at
`wdio.conf.ts:1358-1372`, and worth re-reading here).

## Work Log

### 2026-08-27 — diagnosis (measured, from the four failing jobs' own logs)

Pulled the four failing shard-2 job logs via the Actions API (three were hidden behind a re-run, so
they had to be fetched from `attempts/1`):

| # | PR | Run | Job | Time |
|---|----|-----|-----|------|
| 1 | #1039 | 33082174707 | 98553879134 | 14:33Z |
| 2 | #1056 | 33093506431 | 98601625108 | 17:33Z |
| 3 | main push | 33107885332 | 98646323315 | 19:44Z |
| 4 | #1063 | 33108852412 | 98647909000 | 19:46Z |

**All four are the same chain**, and it is not a timeout, an OOM, or a hang: the job dies in ~3.5
minutes against a 35-minute cap (shard 1 in the same run takes 9.5 min).

1. `archive-browse.smoke.ts` (shard 2's spec #1) runs and passes — 1 case.
2. `handleRunnableStart` runs `resetAppState` before **`checkpoint-restore.smoke.ts`** (spec #2). It
   fails with an ordinary app-level assertion: `expected the breadcrumb to show "cpe-gui-smoke-XXXXXX"
   after navigating to /tmp/cpe-gui-smoke-XXXXXX` — the CPE-1728 slow-renderer signature, a soft
   failure that CPE-1866's recovery path exists to absorb. **All four name this same spec.**
3. The recovery calls `browser.reloadSession()`, whose first act is `DELETE /session/<id>`.
4. ~600 ms later the **native driver behind tauri-driver** (WebKitWebDriver on `NATIVE_DRIVER_PORT`) is
   gone. tauri-driver logs, in its own voice, `Error serving connection: hyper::Error(User(Service),
   client error (SendRequest) ... connection closed before message completed)`, and thereafter
   `client error (Connect) ... Connection refused (os error 111)` on every request. Nothing respawns it.
   *Not* always fatal: job 98697809924 hit the identical step-2 failure on the identical spec and
   `reloadSession()` recovered in 35.4 s — which is why this is intermittent rather than constant.
5. Every remaining spec's before-hook then fails instantly against the dead socket.

**Why "0 new failing cases" — a second, independent defect, and the reason the failure was unreadable.**
`currentSpecFile` was assigned at the *end* of `handleRunnableStart`, i.e. only when the reset
**succeeded**. `flushFileResult` is only ever called with `currentSpecFile`, so the moment step 3 threw,
it froze on `archive-browse.smoke.ts` for the rest of the shard. The other thirteen specs still ran
(WDIO's `executeHooksWithArgs` *resolves* with a throwing config hook's error rather than rejecting —
verified in `@wdio/utils/build/index.js:967-996` — so `testFrameworkFnWrapper` carries on to the
runnable, which is why the cascade kept going), still failed, and `afterTest`/`afterHook` still
accumulated their results into `fileResults` — **which nothing ever wrote to disk**.

Confirmed against the artifact, not just by reading code: run 33107885332's own
`gui-smoke-results-ubuntu-shard-2` contains exactly one file,
`wdio-shard-2-of-4-archive-browse.smoke.json`, for a shard that had visibly executed all fourteen.
That is the whole "1/14 reported, 1 case, 1 passed, 0 failing" symptom, exactly.

**Shard, or its contents? Its contents.** The trigger is a specific spec (`checkpoint-restore.smoke.ts`,
shard 2's #2) tripping the reset; the blast radius was a generic containment bug that would hit any
shard whose reset ever failed. In the same run, shards 1, 3 and 4 logged **zero** `resetAppState failed`
lines — shard 2 is a genuine outlier, and the reason is what it contains, not its index.

### 2026-08-27 — changes

- **`wdio.conf.ts` — the attribution fix (the bulk of the value).** `currentSpecFile` is advanced
  *before* the reset, so each spec's accumulator is flushed regardless of what happens next. A shard that
  dies now reports **every** spec by name with its real error — still RED, more loudly, and diagnosable
  instead of a mystery. Side benefit: the reset is attempted exactly once per file (the freeze made
  `file !== currentSpecFile` true for every subsequent runnable), removing ~10k lines of duplicate stack
  traces that buried the evidence.
- **`lib/driverHealth.ts` (new) + `lib/driverHealth.test.ts` (new, 12 cases).** `isTransportDead()`
  separates "the app misbehaved" (wants a cheap `reloadSession()`) from "the plumbing is gone" (needs the
  driver restarted). Every test input is copied verbatim from the four job logs, in both directions —
  the step-2 breadcrumb assertion must NOT match; the step-4 socket errors must.
- **`wdio.conf.ts` — bounded containment.** On a transport death, tauri-driver is respawned **once per
  worker** (`MAX_DRIVER_RESPAWNS = 1`) through the same `startTauriDriver` helper `beforeSession` uses,
  readiness waits included, with the old process killed *and waited for* so the replacement is not racing
  a dying listener on the same fixed ports. When the budget is spent it throws, `shardAborted` latches,
  and a `[gui-smoke] SHARD ABORTED` block names the spec, the cause, and the fact that the N following
  failures are one death rather than N regressions.
- **`lib/beforeSessionAwaits.test.ts` — retargeted, deliberately, and strengthened.** The spawn + both
  port waits moved into `startTauriDriver`, which is exactly the restructure that guard's own `FIX_HINT`
  anticipates and instructs be updated in the same PR. It now additionally proves `beforeSession`
  *awaits* the helper, the CPE-1955 respawn goes through the same helper, and `tauri-driver` is spawned
  in exactly one place. Both new assertions red-proofed by sabotage (dropping each `await` in turn).
- **`README.md`** — a "Reading a CI run" entry so the next person to meet this shape gets the diagnosis
  instead of reaching for `gh run rerun`.

**The ratchet was not touched.** `incomplete=true => RED` (CPE-1753) is correct and stays; the retry is
bounded, inside the job, and still reds when it exhausts. Nothing here can turn a red run green.

**Verification:** `gui-smoke` typecheck clean; `gui-smoke` unit tests **149/149, up from 130** at the
merge base `5de1978a` — **+19 cases**, not the "+4" an earlier draft of this log claimed. That number was
wrong and in the wrong direction: it was measured *after* `driverHealth.test.ts` already existed, so it
recorded the absolute at a moment mid-change instead of the delta (exactly what `8106f88b` warns about,
one commit earlier). Re-measured directly: the base `beforeSessionAwaits.test.ts` extracted from
`5de1978a` and run in place reports **5** cases against this PR's **9** (+4), plus `driverHealth.test.ts`
at **15**. Root `npm run check` 0 errors; root `npm test` 4926 passed / 2 skipped across 345 files.

**What remains unproven.** *Why* the native driver dies on the `DELETE /session` in the first place is
not established — it survives the identical sequence in other runs, so it is timing-dependent inside
tauri-driver 2.0.6 / WebKitWebDriver and would need instrumentation of the driver itself to pin down.
The respawn is therefore containment for a cause that is named at the WebDriver-transport layer but not
below it. The respawn path itself has **not** been exercised on a real Linux runner (it cannot be
reproduced on demand — the trigger is a slow-renderer race), so its value is proven only by
construction + the guard tests; if it fails, the outcome is exactly today's red, now with a diagnosis
attached. Also unproven: whether `checkpoint-restore.smoke.ts` is intrinsically reset-hostile or merely
unlucky in following `archive-browse.smoke.ts` — worth a look if this recurs after this lands.


### 2026-08-27 — review round 2, and what the fix immediately found

Three corrections from review, all taken:

- **The test-count claim was wrong** (see the corrected Verification line above): 130 -> 149, **+19**, not
  "+4". Measured at the merge base, not asserted.
- **The budget-spent throw carried no transport marker.** `respawnTauriDriver`'s error read
  `[gui-smoke] the WebDriver transport is gone and the tauri-driver respawn budget (1) is spent`, which
  matched none of the markers. Concrete silent path: a **second** transport death in one shard where the
  reset's own error is app-level (the observed breadcrumb shape) and the death first surfaces at
  `reloadSession()` — then `isTransportDead(recoveryErr)` is false *and* `isTransportDead(err)` is false,
  so `shardAborted` never latches and `formatShardAbort` never prints. Red, but silent — and
  `respawnTauriDriver`'s own docstring asserted the opposite. Fixed with an exported
  `TRANSPORT_DEAD_SENTINEL` that the throw interpolates and `isTransportDead` matches, plus a test that
  reds if the throw stops interpolating it (a test building its input *from* the constant cannot catch
  that drift on its own). Docstring corrected to say what the code does.
- **`EPIPE` was an unanchored substring** — it matched `STAGEPIPELINE`. Codes are now matched whole-word
  via an explicit character check; phrases stay substrings. The header's "the bias is towards not
  matching" was overstated and now says plainly that a marker quoted inside an assertion message *would*
  match, that none exists in `gui-smoke/lib` or `gui-smoke/specs` today (grepped, not assumed), and not to
  write one.

Both new guard-shaped assertions red-proofed by sabotage (removing the sentinel from the throw; reverting
whole-word matching to `includes`) — each reds exactly its own named case.

#### The fix found a real failing case on its first CI run

Shard 2 on this PR reported **`14/14 spec file(s) reported ... incomplete=false`** and named
`macro-param-prompt.smoke.ts :: running a bound {ask:suffix} macro opens MacroParamPrompt before any
dry-run confirm` — instead of the old `1/14 ... 0 new failing cases`. The attribution fix working in
production, first run.

**Is that failure ours? No — and it is not new.** Job **98697809924** (shard 2, 22:53Z, branch
`worktree-agent-add93db74672448c3`, sha `373ee259` — a different agent's branch, **before this change
existed**) reported the identical thing: `14/14 spec file(s) reported, 26 case(s) — 23 passed, 1 failed,
2 skipped/pending` and the same `NEW GUI REGRESSION` for the same case, with the real UI error
`element (".ctx .flyout .row") still not existing after 5000ms`. Byte-identical counts to this PR's run,
which also answers the "2 skipped/pending" question: pre-existing, unchanged, not a new symptom.

**Was it hidden inside the swallowed thirteen?** Checked, and the honest answer is **no, not by the
swallowing** — it was hidden by something adjacent. `grep -c 'ctx .flyout .row'` on the swallowed run
(job 98646323315) is **0**, and `macro-param-prompt.smoke.ts` appears there only as cascade noise:
`handleRunnableStart:newFile` -> `resetFailedRestartingSession` in ~3 ms with a `WebDriverRequestError`.
The transport died at spec #2 (`checkpoint-restore`) at 19:43:18; `macro-param-prompt` is spec #6, so the
app was already gone and the spec never really ran. Those four runs therefore cannot say anything about
that spec's real health.

**The masking is real, just one step earlier than assumed.** Shard 2 has had two independent failure
modes on the same day: an illegible transport death that reports "0 new failing cases", and a genuine
intermittent `macro-param-prompt` failure. A run that died reported nothing actionable and got re-run; a
run that survived reported the regression — and by the ticket's own record those were re-run too. So the
concrete worry this ticket was filed about ("a guard people learn to re-run is a guard that has stopped
working") is not hypothetical: there was already a real failing case behind the re-run reflex. Surfacing
it is the fix working, not the fix breaking.

**Deliberately not done here:** `macro-param-prompt` is **not** added to `known-failing.json` and the GUI
bug is not chased — this PR stays the harness fix, and exempting a real regression to land the tool that
surfaced it would be self-defeating. Handed to the Foreman to file separately.