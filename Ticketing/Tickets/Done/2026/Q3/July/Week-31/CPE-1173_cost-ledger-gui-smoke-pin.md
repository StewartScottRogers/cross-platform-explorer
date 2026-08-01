---
id: CPE-1173
title: "gui-smoke: pin the Agent Watch cost-ledger tab render (CPE-1098)"
type: chore
component: Testing
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-1148
---

## Summary
The Agent Watch drawer's **Cost ledger tab** (`AgentTimeline.svelte`, CPE-1098 — per-session token/cost usage
bridged from the sidecar's best-effort PTY usage scrape) had no `gui-smoke` render pin. A read-only spike
confirmed the exact seeding mechanism needed: a third test-mode-only ingest hook in `App.svelte`, following the
precedent of `__CPE_TEST_INGEST_SESSION__` (CPE-1130) and `__CPE_TEST_INGEST_ACTIVITY__` (CPE-1135). This
ticket adds that hook plus the render-pin spec.

## Build
- `src/App.svelte`: import `ingestCost` alongside `initAgentCost` from `./lib/agentCost`, and add a third
  `if (testMode) { ... }` hook — `window.__CPE_TEST_INGEST_COST__` — that JSON-parses its payload and folds it
  into the live `agentCost` store via `ingestCost`. Gated behind the same `--test-mode` /
  `window.__CPE_TEST_MODE__` global as the other two hooks; absent (zero cost, zero attack surface) outside
  `--test-mode`.
- New `gui-smoke/specs/cost-ledger.smoke.ts`, modelled closely on `cost-history.smoke.ts`: waits for the
  initial `--open=<tmpDir>` nav, seeds a synthetic watched session via `__CPE_TEST_INGEST_SESSION__` (same
  `cwd`, sessionId `gui-smoke-cpe-1173`), clicks `.agent-log-btn`, waits for the 5-tab strip, switches to the
  tab labelled "Cost", then seeds a synthetic usage snapshot via `__CPE_TEST_INGEST_COST__`
  (`{sessionId, inputTokens: 12345, outputTokens: 6789, costUsd: 0.4321}` — same sessionId, so the
  watched-session chip also renders). Asserts `.cl-card`/`.cl-row`/`.cl-label`/`.cl-value`/`.cl-chip` render,
  then `snap("cost-ledger")`. `afterEach` calls `snapFailure(this.currentTest, "cost-ledger")` per the
  CPE-1149 convention.

## Acceptance Criteria
- [x] `App.svelte` gains the `__CPE_TEST_INGEST_COST__` hook, test-mode-gated only, matching the exact
      casting/comment idiom of the two existing hooks.
- [x] `gui-smoke/specs/cost-ledger.smoke.ts` seeds a session + a cost snapshot and asserts the Cost tab's
      `.cl-*` DOM renders; ends with `snap("cost-ledger")` and a `snapFailure` `afterEach`.
- [x] `npm run check` green (Svelte+TS typecheck, including the App.svelte change).
- [x] `gui-smoke` `tsc --noEmit` typecheck green (the new spec typechecks).
- [x] No new dependencies added.
- [x] Live screenshot render is exercised by the gui-smoke CI job (non-blocking, CPE-1048) — not run locally
      here (no tauri build + msedgedriver harness in this worktree run); same posture as every other pin.

## Notes
- Test-infra + a 2-line `App.svelte` hook only; no other app-code change. Epic CPE-1148 (gui-screenshot
  capture + Visual Critic). Sibling of CPE-1130 (cost-History pin) and CPE-1135 (Replay-scrubber pin).

## Work Log
- 2026-07-31 — Added the `__CPE_TEST_INGEST_COST__` test-mode hook to `App.svelte` (imports `ingestCost`
  alongside `initAgentCost` from `./lib/agentCost`; the hook JSON-parses its payload and calls `ingestCost`,
  gated behind the same `testMode` block as `__CPE_TEST_INGEST_SESSION__`/`__CPE_TEST_INGEST_ACTIVITY__`).
  Added `gui-smoke/specs/cost-ledger.smoke.ts`, copying the structure of `cost-history.smoke.ts`: seeds a
  synthetic watched session (`gui-smoke-cpe-1173`), opens the Agent Watch drawer, switches to the Cost tab,
  seeds a synthetic usage snapshot with the same sessionId, and asserts `.cl-card`/`.cl-row`/`.cl-label`/
  `.cl-value`/`.cl-chip` all render before `snap("cost-ledger")`.
- Verification: `npm run check` → 0 errors / 0 warnings. `cd gui-smoke && npm ci && npm run typecheck` →
  clean (`tsc --noEmit -p tsconfig.json`, no output). Root `npm test` (vitest) → 1482 tests passed; the only
  2 failing suites (`gui-smoke/lib/compare.filters.test.ts`, `gui-smoke/lib/compare.test.ts`, "No test suite
  found") are pre-existing and unrelated — reproduced with this branch's changes stashed away, confirming
  they aren't caused by this change. Could NOT run the new spec's actual headless browser render here — that
  needs a real tauri build + msedgedriver, which this worktree run didn't perform; the live screenshot
  (`cost-ledger.png`) will be produced by the gui-smoke CI job (non-blocking per CPE-1048), same as every
  other pin in this series. Not faking a pass on that leg.
