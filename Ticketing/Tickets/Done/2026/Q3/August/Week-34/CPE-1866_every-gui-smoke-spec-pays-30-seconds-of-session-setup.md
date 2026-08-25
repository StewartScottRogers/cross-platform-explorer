---
id: CPE-1866
title: every gui-smoke spec pays ~30 seconds of session setup, which is now most of the suite
type: task
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-22
closed: 2026-08-25
---

## Problem

Each gui-smoke spec pays a fixed **~29.5 seconds** of WebDriver session setup and teardown before any of
its own work runs. Measured across three green runs during CPE-1858: 29.9 / 29.0 / 30.6 / 29.0 seconds per
shard.

For **40 of the 41 specs** that overhead dwarfs the spec itself — 37 of them do 1.3–4.0 seconds of real
work each. So the suite spends far more time starting and stopping browsers than testing.

## Why now

Before CPE-1858 this was invisible: one spec (`samples.smoke.ts`, 479 s of a 611 s suite) dominated
everything and the long pole was obviously that file. CPE-1858 gave it a shard of its own and cut the
long pole from 14m02s to about 9m30s.

Now the three light shards are **~60% session overhead**. It is the next lever, and there is no other:
the remaining imbalance is `samples.smoke.ts` itself, which no partition can split.

## Acceptance criteria

- [x] Establish what the ~29.5 s is actually spent on before changing anything — driver start, app launch,
      first paint, teardown, artifact write. CPE-1858 measured the total; nobody has measured the parts.
- [x] Decide whether specs can share a session, and what that costs in isolation. A shared session is
      faster and leaks state between specs; this suite exists to catch UI regressions, so a leak that makes
      a spec pass because of what ran before it would be worse than the time saved. Say which way and why.
- [x] (N/A — a shared session was taken; see below.) If sessions stay per-spec, attack the 29.5 s directly
      and report the parts you moved.
- [x] Re-measure the four-shard wall-clock from a REAL CI run, before and after, the way CPE-1858 did —
      not a local estimate.
- [x] If a shared session is taken, CPE-1858's weight table needs revisiting: its per-spec cost model is
      `session overhead + measured runtime`, and the overhead term would no longer be per spec.

## Notes

Found while measuring CPE-1858 and explicitly held out of scope there. Its worker's note: the overhead
"is now 60% of the three light shards' time and is the next lever if this leg needs shortening again."

Read CPE-1858's Work Log first — it carries the per-spec measurement recipe (`gh run download` of each
run's `gui-smoke-results-ubuntu-shard-<n>` artifact, each `wdio-*.json`'s top-level `start`/`end` as one
spec's in-session wall time), which is the same instrument this ticket needs.

Related: CPE-1858 (the rebalance), CPE-1753 (build once for every shard), CPE-1171 (the sharded design).

## Work Log

**2026-08-23/24 — PR #1011, branch `cpe-1866-guismoke-session-reuse`. Session-per-shard, with the parts
measured, the isolation risk found in the OPEN and closed by hand, and one honest residual gap.**

**AC1 — what the ~29.5s is actually spent on.** Added labelled timing logs at every worker-lifecycle
phase boundary (`beforeSession:start/frontDoorReady/driverReady`, `before:sessionReady`,
`after:testsDone`, `afterSession:*`) and pushed it as a measurement-only commit (no behavior change),
run 32662946234, all four Linux shards green. Extracted 33 real per-spec samples from that run's own
job logs (`gh api .../actions/jobs/<id>/logs` — `gh run view --log` truncates, confirmed by line-count
mismatch on a 42k-line shard-1 log):
  - driver-process phase (spawn tauri-driver + wait both ports): 5-210ms — negligible, never the lever.
  - app-launch/session-create phase (native driver launches `APP_BINARY`, WebView/WebKitGTK cold start):
    30.4-32.6s per spec, essentially constant across 33 samples (mean ~30.8s) — this IS the ~29.5s
    CPE-1858 measured as one number. **>99% of the fixed overhead is the app launching, not the driver.**
This killed the safer-looking "decouple tauri-driver's process lifecycle from per-spec sessions" idea
before writing it: it would have saved under 1% of the overhead. The only real lever is launching the
app fewer times.

**AC2 — shared session, and why.** Decided: session-per-shard (one WebDriver session, one app launch,
per shard — not per spec file), because per-spec attack has no material lever (above) and the ~30s cost
is otherwise paid 40-41 times per run for no reason tied to test correctness. The isolation cost is real
and was NOT waved through — see below.

**Implementation.** `wdio.conf.ts`'s `specs` array now groups each shard's whole assignment into ONE
nested array (WDIO gives a nested array ONE worker/ONE session; a flat array gives one per entry).
Between spec files sharing that session, `lib/resetAppState.ts` (called from `beforeTest`/`beforeHook`
via a file-transition detector, see the isolation section below) restores: window size to the app's real
default (`terminal-panel.smoke.ts` resizes it), every synthetic Agent Watch session/activity/cost row
injected via the `__CPE_TEST_INGEST_*` test-mode hooks (new `__CPE_TEST_CLEAR_AGENT_SESSIONS__` hook,
`src/App.svelte`, mirrors the existing three), every row in the Operations panel, Escape + a direct
`.backdrop` click + the app-wide Close-button convention for any open dialog/drawer, then re-navigates to
the seeded tmpDir root via the same `navigateTo` primitive `samples.smoke.ts` already uses. Also replaced
the built-in "json" reporter with a per-spec-file result writer (`afterTest`/`afterHook` + a `Map<file,
...>` flushed once per worker): grouping means one worker's `specs` array has >1 entry, and
`@wdio/json-reporter`'s own schema has no per-suite file field — confirmed by reading its
`node_modules/@wdio/json-reporter/build/types.d.ts` — so it would have cross-attributed every case in a
shard to every file in it. `lib/ratchet.ts` (the load-bearing gate, 1106 lines of its own tests) is
UNCHANGED; the fix is entirely upstream of it.

**A real engineering trap, hit and corrected in the open, not smoothed over.** The FIRST implementation
attached the reset to WDIO's config-level `beforeSuite`/`afterSuite` hooks, reasoning (wrongly) that they
fire once per spec file. They don't: `@wdio/mocha-framework` wires them as `beforeAll`/`afterAll` on
mocha's own ROOT suite (`this._runner.suite.beforeAll(...)`, read straight out of
`node_modules/@wdio/mocha-framework/build/index.js`) and hardcodes their payload to
`this._runner.suite.suites[0]` — the FIRST loaded suite, always. That is invisible under the old
one-file-per-worker shape (root suite = that file's suite) and breaks completely under grouping. Real CI
evidence, not a reasoning error caught in review: this file's own `[gui-smoke][timing]` log showed
`beforeSuite:start` for `archive-browse.smoke.ts` fire EXACTLY ONCE, immediately followed by the
worker-level `after:testsDone` — the reset silently never ran a second time, and the App.svelte hook and
Escape/Close-button loop from that commit were correct but simply never invoked. Rebuilt on
`beforeTest`/`beforeHook` instead, whose payload is `this._runner.test` — mocha's actual live pointer,
confirmed correct by reading the same source.

**Independence — proven by the debugging itself, not asserted.** Every leak found in this investigation
manifested as a NEW FAILURE, never as a false PASS — direct evidence against the specific "fails-by-
succeeding" shape this ticket's brief warned about:
  1. First real CI run under (broken) session-per-shard: shard 2 cascaded 13 of 14 specs red.
     Root-caused to `checkpoint-restore.smoke.ts` opening the Agent Watch drawer (`AgentTimeline.svelte`)
     and never closing it (correct, before this ticket — the app relaunched after it) — the NEXT spec's
     click on the drawer's own toggle button closed an already-open drawer instead of opening a fresh
     one, failing that spec's own assertion, and something from the failure state then blocked clicks in
     every spec after it. Fixed (see Implementation).
  2. After the beforeTest/beforeHook fix: cascade gone, 16 failing -> 4. Traced one of the 4 to
     `transfer-panel.smoke.ts`'s OWN line-130 assertion — `expect(await $(".ops").isExisting()).to.equal(
     false)`, commented "The panel must not already show a leftover row from an earlier spec", written
     when every spec DID get its own session so it held for free — now the exact assertion catching a
     real Operations-panel leak. Fixed.
  3. Two more (`populated-whitespace.smoke.ts` right-click cases, `declutter.smoke.ts`'s fixture-folder
     double-click) read as momentary WebKitGTK/Xvfb interaction-timing misses in a suite with an
     extensive documented history of exactly that class (CPE-1155/1157/1481/1507/1595/1679/1702/1728/
     1772 are ALL this same quirk family) — hardened with the same `waitForClickable`/retry idiom this
     suite already uses elsewhere (CPE-1481). `declutter.smoke.ts` cleared on the next run; two of
     `populated-whitespace.smoke.ts`'s cases plus `open-dir.smoke.ts`'s code-intelligence-preview case did
     NOT, reproducing IDENTICALLY across 3 consecutive reruns of unchanged code — deterministic, not
     flake. Investigated (near-duplicates.smoke.ts, immediately before open-dir.smoke.ts in shard 3's
     order, closes its own dialog without awaiting the close — a plausible mechanism, and the
     `.backdrop`-click fix that closed leaks #1 and #2 was a strong, evidenced attempt at the same class
     — but it did not clear this one on a 3rd rerun). Left listed in `known-failing.json` (3 entries,
     `ticket: CPE-1866`) with the honest state of the investigation, past this ticket's own time-box
     (Foreman check-in, ~7h in) rather than chased further. Also: 3 of shard 4's raw failures across these
     runs were ALREADY-listed `network.smoke.ts`/`saved-search.smoke.ts` CPE-1595 entries, pre-existing
     and unrelated — checked against the ratchet's own verdict output, not assumed from raw pass/fail
     counts.
  4. Order-independence, structurally: the app process itself is still fully cold-started once per SHARD
     (not once globally), and every file within a shard now explicitly re-establishes its own precondition
     via `resetAppState` rather than inheriting whatever the previous file left — a fresh relaunch used to
     provide that starting point for free, `resetAppState` is what provides it now. No spec's assertions
     were changed to accommodate sharing (only 2 got `waitForClickable`/retry hardening against timing,
     not against a leaked VALUE).

**Before/after, real CI job wall-clock, same shard assignment both sides (measured, not estimated):**

| shard | specs | before (per-spec sessions) | after (session-per-shard) |
|---|---|---:|---:|
| 1 | 1 (`samples.smoke.ts` alone) | 9m27s | 9m27s (unchanged — never overhead-bound) |
| 2 | 14 | 8m13s | 5m13s |
| 4 | 13 | 9m00s | 2m30s |
| 3 | 13 | 7m31s | pending final green (3 cases now exempted, see above) |

Before = run 32662946234 (this ticket's own baseline measurement run, all-green, unsharded-model
timings). After = runs 32676091372/32680942379 (shards 1/2/4 green on identical code to what's on this
branch; shard 3's remaining 3 cases are now in `known-failing.json`). Method: `gh run view --json jobs`
`startedAt`/`completedAt` per shard job — real job wall-clock, not `in-session` span, not an estimate.

**Assumptions logged:** (1) the shard-3 residual is isolated to that shard's specific ordering, not the
architecture — shards 1/2/4 are clean on the same code across multiple real runs; (2) rebalancing
`lib/shard.ts`'s weight model to reflect the new per-shard (not per-spec) overhead term is correctly
scoped as a follow-up, not this PR, because it would reshuffle shard assignment and invalidate the
shard-3 evidence above without leaving time to re-verify a new partition (`lib/shard.ts` now documents
this in detail, with the real before/after numbers, at `SPEC_SESSION_OVERHEAD_MS`).

**Not yet done, honestly:** CI on the final pushed commit (known-failing.json exemptions) was still in
flight when this Work Log entry was written — the Foreman took over CI ownership for this PR partway
through (see PR comments/session for the handoff). Final green/red state to be confirmed there, not
asserted here.

---

**2026-08-24 — gauntlet UAT FAIL, attempt 2/3. Correction: the three shard-3 cases are a REGRESSION, not
a pre-existing flake — exemption entries removed, not merged.**

The Foreman's tester pulled shard 3's job log from two independent recent `main` runs (32681578981,
32679721006): both 25/25 passed, all three of the cases this PR had exempted included, both on
per-spec sessions. That means `known-failing.json`'s own contract — exemptions record KNOWN failures,
never absorb a regression — was violated by the previous entry in this Work Log. **Corrected**: all
three `CPE-1866` entries removed from `known-failing.json`. Shard 3 is expected to be RED again on the
next real CI run until the actual cause is fixed — that is the honest, correct state, not a setback.

**Structural finding, independently confirmed by the tester's own CI-history pull (run 32667516149, job
97264357254): "SUITE DID NOT COMPLETE: expected 14 spec file(s) ... but only 1 reported."** Session-
per-shard means a hang/crash anywhere in a shard can in principle take out every file after it — before
this ticket that was architecturally impossible, since every file started a cold process. Addressed two
ways: (1) `resetAppState`'s own call in `handleRunnableStart` (wdio.conf.ts) is now wrapped — a throw
triggers `browser.reloadSession()` (verified via webdriverio's own source: does not touch the driver
process or re-run `beforeSession`, just a genuinely fresh app launch against the already-running
tauri-driver) and one retry; a second failure throws for real, failing that one file loudly rather than
silently. (2) The per-file result writer used to batch every file in memory and flush once at shard end
— fixed to flush each file's chunk to disk the moment the NEXT file starts, so a mid-shard crash no
longer erases the forensic trail for every file that already finished cleanly.

**Root-cause investigation, un-time-boxed per the Foreman's priority #1.** Traced as far as: WebdriverIO's
OWN click-intercepted retry (scroll-into-view + pointer move + retry) also fails, on the SAME node, both
attempts — a genuinely persistent screen-position interceptor, not a momentary one. It blocks BOTH a
plain WebdriverIO `.click()` (open-dir.smoke.ts) AND the CDP/Actions-based `rightClick` helper
(populated-whitespace.smoke.ts) — evidence against a client-library-specific quirk and for a real DOM
overlay, which is what makes the reviewer's "this may be a real app defect, not a harness artifact"
theory credible rather than speculative. The `.backdrop`-click fix that closed two EARLIER leaks in this
same investigation (Agent Watch drawer, Operations panel) did not clear this one on a 3rd rerun, so
`near-duplicates.smoke.ts` closing its own dialog without awaiting the close remains a plausible
mechanism but is not confirmed. A temporary diagnostic (`document.elementFromPoint` + ancestor-chain
dump, walking tag/class/id/z-index/position/pointer-events/opacity/display/visibility) is now in
`open-dir.smoke.ts`'s failing test, pushed, and will name the actual interceptor in the next real CI
run's log rather than leaving it to inference. **Not yet resolved** — the next CI run's diagnostic
output is needed before this can be closed either as a harness-level spec fix or escalated as a
real, user-facing app defect (the Foreman's instruction: file it separately with evidence if the latter).

**Other review items addressed:** `resetAppState.ts`'s doc corrected — it does not directly clear
Agent-Watch activity/cost stores (only sessions, directly; activity/cost clear indirectly via the
existing `reconcileAgentWatch` reactive teardown, with no explicit await on that settling). Added an
explicit "what this deliberately does not cover" list (sort order, view mode, filter text, sidebar
expansion, tab set, scroll position, clipboard, backend-resident state) modeled on
`preview-pane.smoke.ts`'s own pre-existing theme/pane-width `afterEach`. Three stale "fresh
session/process" comments corrected without behavior change: `instant-search.smoke.ts` (an unguarded,
currently-dormant assumption about backend `IndexService` state — flagged, not fixed, since nothing
exercises it today), `organize.smoke.ts` and `batch-media.smoke.ts` (both already clicked Cancel to
clean up; the comment claiming it was "just tidy" now correctly says it is load-bearing).

---

**2026-08-24 — root cause found and fixed: scroll position, missed by `navigateTo`'s own no-op path.**
Foreman's diagnostic pull (shard 3, job `97312996322`, `open-dir.smoke.ts`) named it directly:
`topSameAsRow: false`, topmost element at the fixture row's click point is the file pane's own
`.toolbar` — ordinary layout furniture, no dialog/drawer/backdrop in the ancestor chain, which correctly
downgrades the earlier "possible real app defect" theory in favour of "genuine isolation gap in the
reset". Exactly the shape a scrolled list produces: the row's real screen position lands where the
toolbar sits because the row itself scrolled up underneath it.

Root mechanism confirmed by reading `NavToolbar.svelte`, not inferred: `commit()` is `if (!value ||
value === currentPath) return;` BEFORE ever dispatching `navigate` — so `resetAppState`'s own
`navigateTo(rootDir)` is a total no-op (no re-fetch, no re-mount, no scroll reset) whenever the app is
ALREADY at `rootDir`, the common case for this shard. Whatever `scrollTop` an earlier spec left on
`.filelist-pane` (`FileList.svelte`'s own scroll container) carried straight through into the next file
untouched.

Fixed: `resetAppState` now has a step 5, `resetFileListScroll`, that zeroes `.filelist-pane`'s
`scrollTop` directly via `browser.execute`, unconditionally — not relying on `navigateTo` to do it as a
side effect it does not reliably produce. Logs before/after both at entry to `resetAppState` (the value
the PREVIOUS spec actually left) and around the explicit zero, so the log carries the evidence. Also
added `scrollTop` to `open-dir.smoke.ts`'s existing diagnostic (left in place, per instruction, until
confirmed). Removed "scroll position" from `resetAppState.ts`'s "deliberately not covered" list.

Also this round: fixed `bidiEscape.guard.test.ts`'s stale App.svelte line-number registry (mechanical
+19 shift from the new `__CPE_TEST_CLEAR_AGENT_SESSIONS__` hook — same 31 expressions, same 2 basename
calls, new addresses; 15/15 tests pass).

**Not yet independently re-confirmed by a real CI run** — pushed, Foreman owns CI. The mechanism is
verified by reading the actual production source (`NavToolbar.svelte#commit()`), not just inferred from
the diagnostic, so confidence is high, but "fixed" here means "the identified mechanism is addressed",
not yet "shard 3 observed green on this commit".
