---
id: CPE-1149
title: "gui-smoke snap(): capture a screenshot on assertion FAILURE too (afterEach hook)"
type: chore
component: Testing
priority: low
status: Backlog
tags: ready
created: 2026-07-30
epic: CPE-579
---

## Summary
Follow-up surfaced by the independent review of CPE-1148 Part A (PR #464). The `snap(name)` calls are placed
**inline after each spec's assertions**, so a PNG is only written on a **passing** run — if an earlier `expect`
throws, `snap()` is never reached. But `gui-smoke/lib/snap.ts` + the specs + the README all claim "a failed
assertion still leaves a shot of whatever state it failed in," which is inaccurate for the current placement.

Two ways to reconcile — prefer the one that makes the docs true, because **capture-on-failure is the genuinely
more useful behaviour**: the failing frame is exactly what the Visual Critic (and a human) most wants to see.

## Acceptance Criteria
- [x] Screenshots are captured on **both** pass and fail. Preferred: move/duplicate the capture into a
      WebdriverIO `afterEach`/`finally` hook (per spec, or a shared hook) that snaps the surface regardless of
      the test outcome, naming the PNG after the surface (keep the existing names; a failing shot may get a
      `-fail` suffix or overwrite — decide and document).
- [x] The `snap.ts` header comment, the per-spec comments, and the `gui-smoke/README.md` claim are made
      **accurate** relative to the actual behaviour (no overstatement).
- [x] Existing assertions and the non-blocking (`continue-on-error`) CI behaviour are unchanged; `snap` still
      swallows its own errors; `npm run check` + `gui-smoke` typecheck green; a real run still leaves the
      gallery of PNGs.

## Notes
- Small, isolated, test-infra-only change. Under epic CPE-579 (self-maintaining quality infra).
- Origin: CPE-1148 Part A reviewer's single non-blocking finding.

## Work Log
- 2026-07-30 (CPE-1149, branch `cpe-1149-snap-on-failure`):
  - **Design chosen — inline pass shot + fail-only `afterEach`.** Kept each spec's existing inline
    `snap("<surface>")` for the PASS case (it fires at the exact right instant — several specs dismiss
    their dialog with Cancel at the end of the `it` body, so a shot taken in an always-on `afterEach`
    would capture the *dismissed* surface). Added a per-spec `afterEach(function () { await
    snapFailure(this.currentTest, "<surface>"); })` that captures **only when the test failed**. Because
    a thrown `expect` never reaches the inline `snap()`, the two are mutually exclusive by outcome — no
    surface is captured twice.
  - **New helper `snapFailure(test, name)` in `gui-smoke/lib/snap.ts`** — writes `<name>-fail.png` iff
    `test.state === "failed"`, else no-op. Delegates to `snap()`, so it inherits the same
    swallow-own-errors behaviour (a screenshot can never fail/mask a real assertion). Failing shots use a
    `-fail` suffix so they never clobber the last good `<name>.png` baseline.
  - **Names preserved.** Pass shots keep `open-dir/organize-dialog/instant-search/batch-media-dialog/
    replay-tab/cost-history.png`; failing runs add the `-fail` variants.
  - **Docs made accurate.** Rewrote the false "a failed assertion still leaves a shot" claim in
    `snap.ts`'s header, each spec's inline `snap()` comment, and the README "Screenshots for the Visual
    Critic" section (now documents both helpers, a pass/fail file table, and *why* the split is needed).
  - **No app-code / no-dep / CI-behaviour change** — `gui-smoke/` test-infra only; no `src/`, `src-tauri/`,
    or npm-dep edits; CI stays `continue-on-error`.
  - **Verification:**
    - Root `npm run check` → `0 errors and 0 warnings`.
    - `gui-smoke` `npm run typecheck` (`tsc --noEmit`) → exit 0, clean.
    - **Live harness run** (`cd gui-smoke && npm test`, real built binary + tauri-driver/msedgedriver
      from `~/.cargo/bin`): 4 specs passed, 2 failed (cost-history + replay failed on their own
      pre-existing app-behaviour assertions, unrelated to this change). `.screenshots/` afterwards held
      exactly: `open-dir.png`, `organize-dialog.png`, `instant-search.png`, `batch-media-dialog.png`
      (the 4 passes) **plus** `cost-history-fail.png` + `replay-tab-fail.png` (the 2 fails, via the new
      hook). This is live proof of capture-on-failure — the real failures made a forced-fail scratch run
      unnecessary — and of "no double capture" (each surface produced exactly one PNG).
  - Left ticket in `Backlog/` for the Foreman to move to Done after the gauntlet.
