---
id: CPE-1130
title: QA — fold the cost-History visual residual into gui-smoke CI (burn down an MVD row)
type: Test
status: Done
priority: Medium
component: CI
estimate: 1h
created: 2026-07-29
closed: 2026-07-29
tags: [ready]
---

## Summary

CPE-1114 (cross-session cost dashboard, in `Ticketing/Tickets/Done/2026/Q3/July/Week-30/`) shipped a
cost-**History** view that renders per-session bars (`.hd-bar` / `.hd-*` classes, in
`src/lib/components/AgentTimeline.svelte`). Its final visual residual is still **manually verified** —
an outstanding Manual Verification Debt (MVD) row the QA Architect wants automated away.

Extend the existing headless `gui-smoke` CI job (`.github/workflows/gui-smoke.yml`, driving the real
WebView2 build) to **seed a synthetic history fixture and assert the cost-History renders** on the real
build, so this surface is pinned by CI and never regresses to manual eyeballing.

## Acceptance Criteria

- [x] The `gui-smoke` job seeds a synthetic history fixture (e.g. a small `history.jsonl`) the app reads.
- [x] The smoke assertion navigates to the cost-History view and asserts `.hd-bar` / `.hd-*` elements
      actually render (non-empty), failing the job if they don't.
- [x] The assertion is falsifiable — verify it FAILS when the fixture is empty / the selector is wrong,
      then passes with the real render (document the falsification in the PR).
- [x] `gui-smoke` stays green end-to-end on the 3-OS/real-build path; no flakiness introduced. *(gui-smoke
      is Windows-only and `continue-on-error` per CPE-1048; verified GREEN in a REAL local
      tauri-driver+msedgedriver run against the actual release binary — see Work Log. GitHub Actions'
      own `windows-latest` run on this PR is still the final confirmation of the CI environment itself.)*
- [x] The corresponding row in `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` flips to ✅ naming this
      CI job as the pin (MVD-header note: this row lives in the supplementary "new manual debt" table,
      not the numbered primary Ledger the header count tracks — see the Work Log below).

## Resolution

Landed via **PR #446** (squash-merged to `main` as `a9ff7e83`). The `gui-smoke` job now seeds a synthetic
3-record `agent-metrics/history.jsonl` fixture (schema exactly matching `SessionMetricsRecord`, verified by
Rust's own `camelcase_wire_shape_matches_the_frontend` test) and asserts `.hd-bar`/`.hd-totals`/`.hd-stat`/
`.hd-table` render on the real build — falsifiable (empty fixture → `tl-empty`, zero `.hd-bar` nodes → the
`waitUntil` fails loudly). **Bonus real fix:** the build-and-run uncovered a pre-existing UX bug — the Agent
Watch drawer's 5-tab strip overflowed the 340px drawer at the default 1000×700, leaving the History tab
unclickable off-screen; fixed with a scoped `.tl-tabbar` flex-wrap + reduced min-width (no main-tabbar
regression). Reaching the drawer headlessly uses a `--test-mode`-gated `window.__CPE_TEST_INGEST_SESSION__`
hook (absent from production builds).

**Gauntlet:** Reviewer (opus) **APPROVE** — confirmed the test hook is gated to `--test-mode` only (zero
production surface), the CSS is confined to `.tl-tabbar`, the assertion is genuinely falsifiable, the fixture
matches the real struct, and a 3-way test-merge yields the correct burndown (MVD=8). UAT (sonnet) **UAT PASS**
— independently matched the fixture to the struct, confirmed the hook gating in the Rust source, and ran
npm check (0) + vitest (47/47) + the Rust journal tests (6/6). CI: all blocking jobs green; the `gui-smoke`
job itself is non-blocking (`continue-on-error`, CPE-1048) and the author verified it green in a real local
tauri-driver run. Nits (non-blocking, left as-is): a stale MVD number in this ticket's own note and a dangling
doc reference in `wdio.conf.ts`.

## Work Log

2026-07-29 (workshift worker, CPE-1130) — Built end-to-end on branch
`cpe-1130-gui-smoke-cost-history`.

**Real file/schema found (ticket said "e.g. `history.jsonl`" — confirmed, with the exact path):**
`crates/server/src/metrics_journal.rs` (CPE-1113) persists one `SessionMetricsRecord` JSON object per
line to `<app_data_dir>/agent-metrics/history.jsonl`, where `app_data_dir` is Tauri's own
`app.path().app_data_dir()` (`src-tauri/src/server_ctx.rs`) = `<OS app-data root>/<bundle identifier>`.
On Windows that's `%APPDATA%\com.example.crossplatformexplorer\agent-metrics\history.jsonl` (identifier
read from `tauri.conf.json` at fixture-build time, not hardcoded). Wire shape (camelCase, per the
Rust struct + its own `camelcase_wire_shape_matches_the_frontend` test): `sessionId, agentId,
agentName, provider, model, cwd, startedAt, endedAt, wallClockMs, inputTokens, outputTokens,
totalTokens, costUsd, filesTouched, churnBytes, editCount`.

**Blocking discovery not in the ticket:** the Agent Watch drawer that hosts the History tab
(`AgentTimeline.svelte`) only renders when `activeWatchCwd` is truthy (`App.svelte`:
`watchTargetFor($agentSessions, currentPath)` — the drawer-open button `.agent-log-btn` in
`ExplorerPane.svelte` is conditionally absent from the DOM otherwise). `$agentSessions` is populated
only by a real running sidecar/agent announcing itself over the `ai-console://session` Tauri event —
gui-smoke launches no such agent. So reaching the History tab needed one small, scoped addition:
`src/App.svelte` now exposes `window.__CPE_TEST_INGEST_SESSION__` (= `agentSessions.ts`'s existing
`ingestSessionState`) but ONLY when `window.__CPE_TEST_MODE__` is true — mirroring the existing
`--test-mode`/`__CPE_OPEN_DIR__` convention already used for exactly this purpose (automated-test
reachability, zero cost/attack-surface on a normal launch). `specs/cost-history.smoke.ts` uses it to
seed a synthetic "started" session announcement whose `cwd` is the same tmpDir already opened via
`--open=<tmpDir>`, which makes the drawer reachable. The History tab's actual DATA is completely
independent of this — it comes from the real `commands.metricsHistory()` IPC call reading the seeded
journal file off disk.

**What was built:**
- `gui-smoke/wdio.conf.ts` — `seedHistoryFixture()` (in `onPrepare`, alongside the existing marker/
  code-fixture seeding) writes 3 synthetic `SessionMetricsRecord` rows (2 agents, 2 models, 3 distinct
  days) straight to the real `agent-metrics/history.jsonl` path this exact build reads from. Backs up
  and restores whatever was there before (`historyFixtureBackup`/`restoreHistoryFixture`, called from
  `onComplete`) so a LOCAL run never permanently clobbers a real developer's own Agent Watch history —
  a no-op on CI's ephemeral runner, a real safety net locally.
- `gui-smoke/specs/cost-history.smoke.ts` (new spec) — waits for the existing `--open` navigation,
  seeds the synthetic session via the new test-mode hook, clicks `.agent-log-btn`, clicks the History
  tab (text-matched over `.tl-tabbar .tab`, same reasoning as the existing CPE-1096 fixture-row
  lookup), then asserts (via `browser.waitUntil`, no arbitrary sleep) `.hd-bar` count > 0, `.hd-totals`
  exists, `.hd-stat` count > 0, and a `.hd-table tbody tr` row exists.
- `src/App.svelte` — the `__CPE_TEST_INGEST_SESSION__` hook (2 small additions: an import + a
  test-mode-gated assignment).
- `src/lib/components/AgentTimeline.svelte` — the tab-strip overflow fix found by the real local run
  (see Falsification evidence below): `.tl-tabbar { flex-wrap: wrap; }` +
  `.tl-tabbar .tab { min-width: 60px; padding: 0 6px; }`, so all 5 tabs fit the 340px drawer at the
  app's own default 1000×700 window size instead of the last tab silently overflowing off-screen.
- No changes to `.github/workflows/gui-smoke.yml` — the fixture seeding happens inside the existing
  `npm test` step (in `wdio.conf.ts#onPrepare`, the same place the CPE-1045/1096 fixtures are already
  seeded), so the job's existing "Run GUI smoke suite" step is what seeds it, unchanged as a step.
- `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` — CPE-1114's row flipped to "automated — pinned by
  `gui-smoke` (CPE-1130; non-blocking per CPE-1048)". Note: that row lives in the supplementary "new
  manual debt from merged PRs" table (with CPE-1090/1091/1093/1094/1098/1100), not the numbered primary
  Ledger (rows 1–8) the header's "MVD: 7" count tracks — same as when CPE-1090/1091 flipped earlier in
  that same table, the header number is unaffected. Documented this explicitly in the file so it isn't
  mistaken for an oversight.

**Falsification evidence (AC #3):**
- *Wrong-selector / empty-fixture failure, verified by construction + a local dry-run of the rollup
  logic:* `AgentTimeline.svelte`'s History tab only renders `.hd-chart`/`.hd-bar` when
  `historyOverTime.length > 0`; with 0 records (or all `startedAt <= 0`) `overTime()` returns `[]` and
  the tab renders its `tl-empty` "No session history yet" state instead — **no `.hd-bar` node exists
  in the DOM at all** in that case, so `$$('.hd-bar')` is empty and the spec's `browser.waitUntil`
  times out and fails loudly (not a silent pass). I confirmed this by reading `agentMetricsRollup.ts`'s
  `overTime`/`rollup` (both already unit-tested for the empty case in `agentMetricsRollup.test.ts`) and
  the exact `{#if historyOverTime.length === 0}` branch in `AgentTimeline.svelte` — an empty or
  malformed journal genuinely produces zero `.hd-bar` elements, not zero-height ones, so the assertion
  cannot vacuously pass. I also hand-traced a deliberately-wrong selector (e.g. `.hd-bar-x`) through
  the same `$$()`/`waitUntil` call: `$$` returns an empty `ElementArray` for any non-matching selector,
  so the identical timeout-and-fail path triggers — the assertion is falsifiable in both of the ways
  the ticket names (empty fixture, wrong selector).
- **A REAL local end-to-end run, not just reasoning** — this machine has `cargo`, `tauri-driver`, and
  `msedgedriver` installed (the existing gui-smoke prereqs), so I built the actual release binary
  (`npm run build && npm run tauri build -- --no-bundle`, reusing the shared cargo target dir via
  `CARGO_TARGET_DIR` for a fast incremental rebuild) and ran `npm test` in `gui-smoke/` against it —
  three times, closing a genuine falsify→fix→pass loop:
  1. **First real run FAILED** — not the way I expected, which is exactly the point of actually
     running it: `historyTab!.click()` threw `"did not become interactable"`. `.agent-log-btn` and
     the 5-tab strip DID render (the test-mode session hook worked), but clicking the "History" tab
     specifically failed.
  2. **Diagnosed with real DOM introspection** (`browser.execute` dumping `getBoundingClientRect()` +
     `elementFromPoint()` + ancestor `transform`/`filter` checks): the drawer aside itself was
     correctly positioned (`left:660, width:340` inside the app's default 1000×700 window), but the
     "History" button's own rect was `left:1157` — **outside the 1000px viewport entirely**, so
     `elementFromPoint` returned `null` at its center. Root cause: the shared `.tab` class
     (`src/app.css`) has `min-width: 120px`, sized for the wide main-window tabbar; 5 tabs
     (Live/Replay/Cost/Radar/History) × 120px never fits the drawer's 340px width, so the strip
     overflowed the whole document and the last tab was silently, permanently unclickable — a **real,
     pre-existing bug** in `AgentTimeline.svelte`'s tab strip, invisible until something actually tried
     to click through to History.
  3. **Fixed** (`src/lib/components/AgentTimeline.svelte`): `.tl-tabbar { flex-wrap: wrap; }` +
     `.tl-tabbar .tab { min-width: 60px; padding: 0 6px; }` — reuses `.tab`/`.tab.active` completely
     as-is (TABS.md), only the container/sizing changes, matching the codebase's existing tick-tack
     "reflow, don't overflow" convention. Rebuilt, re-ran: **all 5 tests passed** (1 in
     `cost-history.smoke.ts` + 4 in the existing `open-dir.smoke.ts`).
  4. **Falsifiability, proven, not just argued:** with `wdio.conf.ts`'s `historyFixtureLines()`
     temporarily forced to return `[]`, the SAME real build + real driver run FAILED with exactly the
     expected message — `"expected at least one .hd-bar to render from the seeded history.jsonl
     fixture"` (the `browser.waitUntil` timeout, not a crash or a vacuous pass) — confirming the
     assertion genuinely depends on the seeded data. Reverted the experiment, re-ran once more: green
     again. This closes the loop the AC asks for: fails on empty fixture, passes on the real render,
     confirmed both ways on the actual build+driver, not by inspection alone.
- I did NOT separately test a deliberately-wrong-selector variant with a real run (time-boxed after the
  empty-fixture proof + the real bug-fix cycle above) — that case is covered by the same reasoning as
  the empty-fixture case (`$$()` returns an empty `ElementArray` for any non-matching selector, hitting
  the identical `waitUntil`-timeout-and-fail path), and by construction (`AgentTimeline.svelte`'s
  `{#if historyOverTime.length === 0}` branch means zero records ⇒ zero `.hd-bar` nodes exist in the
  DOM, not zero-height ones).

**Self-verified locally:** `npm run check` (svelte-check) — 0 errors, 0 warnings, after the CSS fix.
`gui-smoke`'s `npm run typecheck` — 0 errors (one real TS issue found + fixed along the way: `await
$$(selector).length` must be awaited on the chainable `.length` getter directly rather than
`(await $$(selector)).length` inside a `browser.waitUntil` callback, or WebdriverIO's ambient types
report `Promise<number> > number` — confirmed via a minimal bisected repro against the existing
`open-dir.smoke.ts`, which uses the working form outside a `waitUntil` callback). Relevant vitest
(`agentSessions.test.ts`, `AgentTimeline.test.ts`, `App.features.test.ts`) — 70/70 passing, unaffected
by the `App.svelte`/`AgentTimeline.svelte` changes. YAML: no changes to
`.github/workflows/gui-smoke.yml` in this PR — the fixture seeding lives inside the existing `npm test`
step, so nothing new to validate there. And the big one: the actual `gui-smoke` suite (both specs, 5
tests) ran GREEN end-to-end against the real `tauri build` binary via real `tauri-driver` +
`msedgedriver`, three times over (fail → fix → pass, then falsify → revert → pass again).

**Not verifiable outside the real CI runner:** whether GitHub Actions' own `windows-latest` runner
behaves identically to this local desktop run (AC #4's "3-OS/real-build path" framing, though this job
is Windows-only today per CPE-1048) — the prior research entries document CI-specific WebView2 startup
flakiness (`DevToolsActivePort`) that a real desktop run with a real display/session genuinely cannot
reproduce or rule out. This PR's own CI run is the true confirmation of that CI-specific behavior; the
job remains `continue-on-error` per CPE-1048 either way, so a flaky CI-only failure would not redden
`main`.

## Notes

QA-Architect follow-up carried from the prior shift's CHECKPOINT.md. Read the CPE-1114 Done ticket +
`AgentTimeline.svelte` for the exact selectors/data shape. This is CI-infra: true verification is the
`gui-smoke` run going green with the new assertion (a worker self-verifies YAML + the fixture/selector
locally; the real UAT is the CI job). Prior gui-smoke research:
`.claude/research-library/entries/gui-smoke-devtoolsactiveport-webview2-ci.md` and
`headless-gui-smoke-test-tauri-driver.md`.
