---
id: CPE-1096
title: "QA: GUI-smoke asserts the code-preview renders (burn down CPE-1090/1091 visual debt)"
type: chore
component: CI
priority: medium
status: Done
tags: ready
created: 2026-07-26
epic: CPE-396
---

## Summary
QA-Architect debt burndown. CPE-1090 (outline strip/breadcrumb/jump) and CPE-1091 (per-line rows + fold
gutter + indent guides + minimap) both shipped with **headless-only** verification — their actual on-screen
render (`MANUAL-TEST-BURNDOWN.md` rows, 2026-07-26) still needs human eyes. Extend the existing GUI-smoke
test (tauri-driver + WebdriverIO, the `gui-smoke` CI job) to **open a code file in the preview and assert the
code-intelligence UI actually renders**, so these surfaces are pinned by CI and the manual rows can close.

## Context (verified)
- The GUI-smoke harness exists and drives the real built app (tauri-driver + WebdriverIO on windows-latest;
  see the `gui-smoke` workflow + the Library entries `headless-gui-smoke-test-tauri-driver.md` and
  `gui-smoke-devtoolsactiveport-webview2-ci.md`). It already asserts `--open <tmpdir>` navigated via the
  breadcrumb. CPE-1048 made it non-blocking (`continue-on-error`) due to WebView2 flakiness — keep that.
- Code preview renders per-line rows as `.cl-row[data-line=N]` inside `.preview-text code`; the outline strip
  is `.outline-bar` with `.outline-pill` buttons; the minimap block renders only when data is present. These
  are the stable selectors to assert against.

## Design (buildable)
1. In the GUI-smoke spec, after the app opens on a temp dir, have the harness open a **known code file**
   (ship a tiny fixture, e.g. a `.rs`/`.ts` with a couple of functions and a foldable block, into the smoke
   test's temp dir — or navigate to a repo source file the test controls). Select it so the preview renders.
2. Assert the code-intelligence UI is present:
   - at least one `.cl-row[data-line]` exists (per-line rows rendered),
   - the `.outline-bar` with ≥1 `.outline-pill` exists (outline strip populated for a file with symbols),
   - (optional, best-effort) the minimap container exists.
   Assert the plain `<pre>`/highlight still renders too (no regression).
3. Keep the job **non-blocking** (`continue-on-error`, per CPE-1048) — this is a smoke signal, not a gate,
   until WebView2 flakiness is fully tamed. Log clearly on failure.
4. Once green in CI, mark the CPE-1090 + CPE-1091 rows in `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md`
   as **automated** (pinned by `gui-smoke`), closing that visual debt.

## Acceptance Criteria
- [ ] GUI-smoke opens a code file and asserts `.cl-row[data-line]` + `.outline-bar .outline-pill` render
      (plus the highlight `<pre>` still present); passes locally against a `tauri build` binary.
- [ ] Job stays non-blocking (CPE-1048) but the new assertions run and log; no new flakiness introduced.
- [ ] `MANUAL-TEST-BURNDOWN.md` CPE-1090 + CPE-1091 rows flipped to automated/pinned once the assertion is green.

## Work Log
2026-07-26 (sprint, QA Architect) — Filed to burn down the manual visual-verification debt accrued by the
code-preview upgrade (GUI #1). Automating the render assertion pins the outline/gutter/minimap surfaces so
they never silently regress and the human-eyes rows can close.
