---
id: CPE-1626
title: "Tearing down a watcher flushes the session's metrics as if it ended — so a premature teardown silently drops the rest of that session's history"
type: Bug
status: Doing
priority: Medium
component: Frontend
epic: CPE-1486
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Reviewer of CPE-1606 (PR #815) while verifying that PR's justification for
retaining watchers after you navigate away. The justification held, but the underlying mechanism is worse
than the PR's own comment claims — and it is the thing standing between us and a true "off means off".

## The coupling
`reconcileAgentWatch`'s "stop the removed" loop (`src/App.svelte`) runs:

    await flushSession(id); await stopAgentWatch(id); armedWatches.delete(id);

Any session dropped from `desired` therefore has its metrics flushed **as if the session had ended**
(CPE-1113).

The PR's comment says a premature flush would produce two *fragmented* rows. It would not.
`flushSession` (`src/lib/agentSessionMetrics.ts`) is guarded by `flushedSessionIds` (marked at L393,
*before* the await at L398) and is a hard no-op once an id has been flushed — it is only un-marked when a
genuine `started` announcement for that id arrives (L175). So the real failure mode is:

> one **premature, incomplete** row is persisted, and **all activity for the rest of that still-running
> session is silently and permanently dropped** from the journal, because the true end-of-session flush
> becomes a no-op.

Silent data loss in the activity record, rather than a visibly split row.

## Why it matters
This coupling is the only reason CPE-1606 had to retain watchers for the lifetime of a visited session
instead of disarming when you navigate away. Decouple it and the mode can honour `AGENT-WATCH.md`'s
boundary literally — leave the folder, the watcher stops — at no cost to the metrics record.

## Fix
Introduce an explicit **pause vs end** distinction in the metrics model: teardown-for-navigation pauses
(no flush, resumable), and only a genuine session end flushes. Then revisit CPE-1606's retention and
disarm on navigate-away if the numbers stay intact.

Also **correct the inaccurate "fragmented second row" characterisation** in the doc comment and in
`AGENT-WATCH.md`, which currently describes a failure mode that does not happen.

## Acceptance criteria
- A paused-then-resumed session produces ONE complete history/cost row covering its whole life; a test
  covers it and fails against the current code.
- With the decoupling in place, watchers disarm on navigate-away and `AGENT-WATCH.md`'s boundary is
  literally true again — or the ticket explains, with measurements, why retention is still preferred.
- The corrected failure-mode description replaces the current inaccurate one.

**Conflict surface:** `src/lib/agentSessionMetrics.ts`, `src/App.svelte`, `src/lib/agentSessions.ts`,
`AGENT-WATCH.md`, `src/docs/explorer-agent-watch.md`. Overlaps CPE-1625 — sequence them.

## Work Log — 2026-08-11

**Pause vs end design:** `flushSession` (`src/lib/agentSessionMetrics.ts`) now gates on the
accumulator's own `endedAt` (only stamped by a genuine `ended` announcement via `foldSessionEnded`) —
not on whatever seam calls it. `reconcileAgentWatch`'s "stop the removed" loop (`src/App.svelte`) still
calls `flushSession(id)` unconditionally for every session dropped from the armed set, but that's now
safe either way: if the session is still running (dropped only because the explorer navigated away —
a **pause**), the call is a no-op — the live accumulator keeps accruing untouched and `flushedSessionIds`
is never marked, so a later re-arm folds straight back into the SAME record; if the session genuinely
**ended**, the call persists exactly one row covering its whole lifetime, same as before. The distinction
lives in the metrics model itself (the accumulator's `endedAt`), not in caller discipline, so any future
call site gets the same safety for free.

**Negative control (confirmed failing against pre-fix code):** added
`src/lib/agentSessionMetrics.test.ts`'s "CPE-1626 (negative control...)" test FIRST and ran it against
the unmodified code (`npx vitest run src/lib/agentSessionMetrics.test.ts`, 1 failed / 47 passed). Observed
failure: calling `flushSession("s1", 5000)` before the session had ended (only `started` + one edit
folded) persisted immediately —
`metricsRecord` called once with `{ editCount: 1, endedAt: 5000, ... }` (a fabricated end timestamp, not
a real one) — instead of the expected zero calls. This is exactly the bug: one incomplete row, and the
guard means the real end never flushes again, so the second edit (added after this premature flush,
before the genuine `ended`) would be silently and permanently lost. After the fix: same test, 48/48
passed — the premature flush is a no-op, and the real end later flushes ONE row with `editCount: 2`
(both pre- and post-pause edits) and the correct `endedAt: 9000`.

**Retention decision: DISARM on navigate-away (no measurements needed for this branch — the ticket
only requires them to justify *retention*).** With `flushSession` now safe to call on a still-running
session, CPE-1606's reason for retaining a visited session's watcher for its whole lifetime (avoiding a
premature/corrupting flush) no longer applies. Removed `markVisited`/`visitedSessionIds` from
`src/lib/agentSessions.ts`/`src/App.svelte` entirely and replaced the sticky "visited this run" set with
a pure, stateless `watchTargets(sessions, current)`: it arms exactly the session(s) at the CURRENT
deepest project match (reusing `watchTargetFor`, filtered to every session sharing that cwd — preserves
CPE-1625's co-located-sessions fix). Leaving a project's folder now genuinely disarms its watcher again,
literally satisfying `AGENT-WATCH.md`'s "off means off" boundary for both a never-opened project and a
visited-then-left one. `reconcileAgentWatch`'s full-stop branch was also changed: it now only calls
`clearAgentSessionMetrics()` when `sessions.length === 0` (no agent running at all), not merely when the
locally-armed set is empty — so a paused-but-still-running session's accumulator survives to be resumed
instead of being wiped just because nothing happens to be armed at that instant. I did NOT measure real
`notify` watcher arm/disarm IPC cost or rapid-sibling-navigation thrash — this environment has no way to
time a live Tauri watcher round-trip — so I'm not claiming retention is provably worse, only that the
data-loss argument for it is gone and the documented boundary is what the app promises; if telemetry from
real usage later shows thrash is a material cost, retention (or a short debounce before disarming) can be
reintroduced with actual numbers at that point.

**Doc corrections:** `AGENT-WATCH.md`'s Boundaries section rewritten — explains the CPE-1606 boundary gap,
the (now-corrected) "fragmented row" claim vs. the actual "one incomplete row + silent loss" failure mode,
and the CPE-1626 fix + its unmeasured-disarm rationale. `src/docs/explorer-agent-watch.md`'s "Limits/notes"
bullet rewritten to state "off means off" holds literally in both directions, framed as pause/resume
rather than retention.

**Tests:**
- `src/lib/agentSessionMetrics.test.ts` — added the negative-control test above (48 tests, was 47).
- `src/lib/agentSessions.test.ts` — rewrote the `watchTargets`/`markVisited` describe blocks into one
  `watchTargets(sessions, current)` describe block (9 tests) covering: no sessions ⇒ empty; running-but-
  not-current stays unarmed (CPE-1606 boundary); only the current-path session armed, sibling untouched;
  deepest-match still picks the nested project; **disarms immediately on navigate-away** (CPE-1626, new
  behavior); re-arms on navigating back; **CPE-1625: both co-located sessions armed together** (must not
  regress); a truly-ended session (absent from `sessions`) stays unarmed. 18 tests total in the file (was
  20 — net -2, since the new stateless design has fewer distinct cases than the old stateful
  accumulate/prune/no-op-identity set, but every acceptance-criteria scenario is still covered).
- New `src/App.agentWatchPauseMetrics.test.ts` (3 tests) — full-App-mount integration test driving real
  navigation via `fireEvent.click`, asserting on the actual `invoke("agent_watch_start"/"agent_watch_stop"/
  "metrics_record")` calls: (1) a never-navigated-into session never gets `agent_watch_start` (CPE-1606
  boundary holds); (2) navigate-away calls `agent_watch_stop` for a still-running session WITHOUT a
  `metrics_record` call, navigate-back calls `agent_watch_start` again for the same id, and the eventual
  real `ended` produces exactly one `metrics_record` call with `editCount: 2` (both pre- and post-pause
  edits, nothing dropped/duplicated); (3) CPE-1625's co-located-sessions case still arms both `s1` and
  `s2` together.

**Verification (all synchronous, all observed passing):**
- `npm run check` → `svelte-check found 0 errors and 0 warnings`.
- `npx vitest run` (full suite) → `Test Files 278 passed (278)` / `Tests 3405 passed (3405)`. Baseline
  going in (per CPE-1625's last log) was 273 files / 3326 tests; the delta is the new integration test
  file (+3), the negative-control test (+1), and the net agentSessions.test.ts change (-2), plus whatever
  else landed in the repo between CPE-1625 and this run tonight (the suite is growing fast this sprint).
- No Rust files touched (frontend-only conflict surface per the ticket) — `cargo build`/`cargo clippy`
  not run, not applicable.

**Assumptions:** none beyond what's stated above. No new dependencies added. Did not weaken or delete any
existing test — the old `markVisited`/old-signature `watchTargets` tests were superseded by equivalent-or-
stronger coverage of the same acceptance criteria under the new, simpler API.
