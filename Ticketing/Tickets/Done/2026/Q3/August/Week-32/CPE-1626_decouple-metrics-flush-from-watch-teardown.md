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

## Work Log — 2026-08-11 (round 2: PR #830 CHANGES REQUESTED — two loss paths, confirmed by both an
independent Reviewer and a separate UAT harness)

**What was found.** Removing CPE-1606's retention (round 1, above) exposed two ways to lose data that
were both worse than the original bug:

1. **Loss path 1 — closing the whole Agent Deck wiped a never-ended session's ENTIRE history.**
   `closeAllConsoles()` (`App.svelte`) reaps the console process, so a still-running session never gets a
   real `ended` announcement. `flushSession`'s `endedAt` gate correctly refused to persist it (round 1's
   fix, working as designed) — but `clearAgentSessions()` right after emptied `$agentSessions`, and
   `reconcileAgentWatch`'s full-stop branch then called `clearAgentSessionMetrics()`, wiping the whole
   accumulator store with nothing ever persisted. UAT confirmed this is strictly worse than the PRE-CPE-
   1626 code, which (despite fabricating an inaccurate `endedAt`) at least persisted *something*.
2. **Loss path 2 — a session that ends while paused, with an armed sibling, sat unflushed.** The "stop the
   removed" loop only ever iterates `armedWatches.keys()`; a paused session had already left that map, so
   the loop never even looks at it when it later ends. UAT measured this as *deferred* rather than
   permanently lost (a later reconcile that drains the armed set to empty does catch it via
   `flushAllSessions()`), but it's a real visibility-latency gap, and it becomes loss path 1 if the deck
   closes (or the app quits) before that drain happens.

**The fix — flush follows the session's own lifecycle, not watch/arm state, plus an explicit forced flush
before any full wipe:**
- `agentSessions.ts`'s `ingestSessionState` now calls `flushSession(ann.session.sessionId)`
  (fire-and-forget) the INSTANT a real `ended` announcement is folded — independent of whether that
  session happens to be currently armed. This is the primary flush trigger now, and it makes loss path 2
  disappear for free: an ended-while-paused session flushes immediately, with no latency gap, regardless
  of any sibling's arm state. `reconcileAgentWatch`'s own `flushSession` call (on the armed-set diff)
  becomes a redundant, harmless backstop (idempotent no-op for anything the new hook already flushed).
- Added `flushAllSessionsForcibly()` (`agentSessionMetrics.ts`): flushes EVERY currently-known session,
  including one that never got a real `ended` — for a still-running session it persists the CURRENT
  accumulator as-is (real activity tallies, nothing fabricated), with `endedAt` stamped at flush time and
  a new **`endedCleanly: false`** marker so the row is structurally, honestly distinguishable from a clean
  end (never silently masquerading as one) — per this crew's standing rule (CPE-1591/CPE-1615) that "we
  don't know" must never look like "it's fine". `closeAllConsoles()` now calls
  `await flushAllSessionsForcibly()` BEFORE `clearAgentSessions()`, so a never-ended session's activity is
  captured before the store wipes, beating the pre-CPE-1626 code's bar (which persisted an inaccurate row)
  rather than merely matching the stricter pause/end gate's refusal.
- New wire field `endedCleanly: boolean` added to `SessionMetricsRecord` (Rust:
  `crates/server/src/metrics_journal.rs`, `#[serde(default = "default_ended_cleanly")]` → `true`, so an
  OLD journal row written before this field existed — every one of which came from a genuine end — still
  reads as clean, never misread as forced). Regenerated `src/lib/bindings.gen.ts` via
  `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` (run from `src-tauri`).

**Negative controls (both run against the pre-this-round branch HEAD — commit `ce86b784`, PR #830's
current state — via `git stash` of just the source-fix files, keeping the new tests):**
- Loss path 1 (`src/App.agentWatchPauseMetrics.test.ts`, drives the REAL production path: right-click the
  "Agent Deck" toolbar button → click "Close all consoles" → `closeAllConsoles()`): observed failure
  `AssertionError: expected 0 to be greater than 0` — `metrics_record` was never called for a session with
  2 accrued edits that never announced `ended`. Matches UAT's exact finding.
- Loss path 2 (same file: a session ends while paused with a sibling still armed, asserting an IMMEDIATE
  flush within 500ms): observed failure `AssertionError: expected false to be true` — no flush call landed
  in the window, matching UAT's "deferred, not immediate" measurement.
- After the fix: same test file, all 5 tests pass (`npx vitest run src/App.agentWatchPauseMetrics.test.ts`
  → `Test Files 1 passed (1)` / `Tests 5 passed (5)`).

**Re-confirmed still holding after this round's changes** (per the coordinator's explicit ask):
- CPE-1606 boundary (never-visited session never armed) — `src/lib/agentSessions.test.ts`, still 20/20,
  plus the integration test's "never navigated into stays fully unarmed" case.
- CPE-1625 co-location (two sessions sharing one cwd both armed) — same files, unchanged, still passing;
  the reviewer's note that `.filter()` makes this structural rather than incidental was not touched.
- Pause/resume headline (paused-then-resumed session → ONE complete row) — unchanged and still covered;
  UAT independently confirmed `editCount: 3` for a 3-edit pause/resume/end sequence.
- Single-agent end-while-away (`editCount: 2`, correct `endedAt`) — untouched, still passing.
- Disarm-on-navigate-away with no debounce — untouched. Per the coordinator: the "no thrash measurements"
  disclosure was confirmed accurate and fair by the reviewer; **thrash remains unmeasured** — noted here
  again explicitly as instructed, not re-investigated this round.

**Docs corrected again:** `AGENT-WATCH.md`'s Boundaries section gained a new sub-bullet describing both
loss paths and their fixes (so the doc doesn't just describe the FIRST fix as if it were the final state).
`src/docs/explorer-agent-watch.md`'s "Limits/notes" section: the previous wording flatly claimed a
session's Cost/History row "still covers the session's whole lifetime once it actually ends" — true for a
real end, but overclaiming for the closed-deck case. Replaced with an explicit statement that closing the
Agent Deck still saves what happened so far, honestly labelled `endedCleanly: false` rather than silently
lost or faked as a clean finish.

**Full verification, this round:**
- `npm run check` → 0 errors, 0 warnings.
- `npx vitest run` (full suite) → `Test Files 278 passed (278)` / `Tests 3414 passed (3414)`.
- Rust: `cargo test` in `crates/server` → `1874 passed; 0 failed; 1 ignored` (lib) + all integration test
  binaries green, including the new `metrics_journal` tests (`ended_cleanly_round_trips_and_defaults_true
  _for_a_pre_cpe_1626_row`, `camelcase_wire_shape_matches_the_frontend` updated for the new field).
- `cargo test typed_bindings_are_committed_and_routed_through_busy_cursor` (src-tauri, with
  `specta-bindings sidecar-platform`) → passes — the committed `bindings.gen.ts` matches the regenerated
  output.
- `cargo clippy --all-targets -- -D warnings` clean in BOTH `crates/server` and `src-tauri` (default
  features AND `sidecar-platform specta-bindings`), per this crew's standing "match CI clippy" rule.
- No Cargo.lock changes needed (no new dependencies — one new struct field only).

**Could not verify:** real-world `notify`-watcher thrash cost from rapid sibling-folder navigation — still
no live Tauri host in this environment to measure it; the coordinator confirmed this gap is acceptable and
the disarm-on-navigate-away decision stands regardless.
