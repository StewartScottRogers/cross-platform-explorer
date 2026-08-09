---
id: CPE-1479
title: "GUI-smoke suite is RED on every main run — CDP mouse-injection unavailable on driver + ~20min job timeout (Visual Critic/UAT substrate down)"
type: Bug
status: Done
priority: High
component: CI/QA-infra
tags: [ready]
epic: CPE-810
qa-architecture: true
created: 2026-08-08
---
## Why this matters (QA-architecture, high leverage)
The `GUI smoke` CI leg (tauri-driver + WebdriverIO, CPE-1171) is the **automated GUI-verification substrate** the
sprint's Visual Critic + UAT Tester depend on to check user-facing changes *without pulling in the user*. It is
currently **failing/cancelled on every recent `main` run** (CPE-1477 file: cancelled 20m35s; CPE-1414 close:
cancelled; CPE-1478 file: in-progress → same pattern), so:
- it produces **no green signal** — GUI changes can't be gated on it, and it can't hand the Visual Critic fresh
  screenshots; and
- merges that touch the UI currently proceed with the gui-smoke leg overridden (`--admin`), which erodes the whole
  "minimise the user's eyes-on" model.

## Evidence (from PR #720 run 31268075683, both OSes fail; job log 93129381731, ubuntu)
The app **renders fine** (webdriver finds elements, `getElementRect` returns real rects, `takeScreenshot`
succeeds — so this is NOT a render/CSP problem). Two distinct failures:
1. **CDP mouse injection unavailable on the driver:**
   `[mouse.ts] Neither browser.sendCommandAndGetResult nor browser.sendCommand is available — CDP input injection
   is not reachable through this driver. See cdpAvailable().`
   thrown at `gui-smoke/lib/mouse.ts:69` (`cdp`) → `mouseEvent` (:141) → `doubleClick` (:183), from
   `gui-smoke/specs/metadata-studio.smoke.ts:80` (CPE-1331 spec). On Linux the driver is **WebKitWebDriver** (wry),
   which has **no CDP** — so any spec that routes a mouse action through the CDP path throws instead of falling
   back to a WebDriver-native Actions API (`performActions`/`releaseActions`) or the existing non-grabbing harness
   (CPE-1155). Windows (Edge/WebView2) also fails — confirm whether it's the same CDP-path issue or a distinct one.
2. **~20-minute job timeout → `The operation was canceled.`** The suite runs specs sequentially and gets cancelled
   at the job time limit — either the spec errors cascade/retry, or startup+per-spec cost has crept back over the
   budget (see the prior fix CPE-1266 "ci-gui-smoke-timeout-concurrency", now regressed/insufficient).

## Fix direction (root-cause first — do NOT blind-iterate against the 20min CI loop)
- **Mouse harness:** make `mouse.ts` fall back to the WebDriver **Actions API** (or the CPE-1155 non-grabbing
  input harness) when `cdpAvailable()` is false, so `click`/`doubleClick`/drag work on WebKitWebDriver (Linux) and
  on any driver lacking CDP — never throw. `git log`/blame `mouse.ts`, `cdpAvailable()`, and CPE-1155 to see what
  the intended fallback was and why it isn't taken here.
- **Determine if this is a regression** from a specific merge (e.g. the CPE-1331 metadata-studio spec's doubleClick,
  or a `tauri-driver`/`webdriverio` version bump) vs a long-standing Linux gap — fix at the root, not per-spec.
- **Timeout:** re-check the CPE-1266 concurrency/timeout budget; if per-spec cost regressed, shard or raise the cap,
  and make a single hung spec fail fast (per-test timeout) instead of consuming the whole 20min job window.
- **Verification is CI-only** (gui-smoke needs the tauri-driver + xvfb/WebView2 environment). So land a
  **high-confidence** fix from local reading + reasoning, then confirm on ONE gui-smoke run that the leg goes
  **green** (or at least the CDP-mouse error is gone and it completes under the timeout). Budget for possibly one
  follow-up iteration; don't spray attempts.

## Acceptance
- `GUI smoke` (ubuntu + windows) completes **within the job timeout** and **passes** on `main` (or the specific
  CDP-mouse failure is eliminated and any remaining red is a separate, newly-filed issue).
- No spec routes a mouse action through an unavailable CDP path without a working WebDriver-native fallback.
- Flip the QA burndown row for "gui-smoke GUI-driving" back to green + name the leg that pins it.

## Notes
Found during the CPE-1477 sprint while validating the CSP change's runtime render (the gui-smoke leg was the
prescribed verification and turned out to be independently broken). Epic CPE-810 (client/server contract +
security-adjacent CI). Coordinate with the concurrent sprints_* process. Filed by the QA Architect.

## Work Log

**2026-08-08 — root-caused & fixed (Worker, sprint).**

**Root cause (both failures, one mechanism — LONG-STANDING, not a fresh regression).**
`gui-smoke/lib/mouse.ts` has had exactly ONE commit since it was written (CPE-1155, PR #474): the
non-grabbing mouse harness was built CDP-only. Every public fn (`click`/`doubleClick`/`rightClick`/
`hover`/`scroll`/`dragTo`) called `cdp()` → `browser.sendCommandAndGetResult`/`sendCommand`
UNCONDITIONALLY. `cdpAvailable()` existed but was only ever used by specs to *document* the finding —
the mouse fns themselves never consulted it and had NO fallback.

- **Failure 1 (CDP mouse injection throws).** On Linux the native driver is **WebKitWebDriver** (wry/
  WebKitGTK), which exposes **no CDP vendor endpoint** — `sendCommand*` isn't attached — so `cdp()`
  hit its throw branch (`mouse.ts:69`), surfacing as the exact log line in the evidence. The Linux CI
  leg (CPE-1171) + mouse-using specs (16 of 39, incl. CPE-1331 metadata-studio's `doubleClick`)
  accumulated over a CDP-only harness that never had a WebKit path. This is a long-standing gap
  exposed by coverage growth, not a single bad merge.
- **Failure 2 (~20-min job timeout → "operation was canceled").** A **cascade** from Failure 1, not
  an independent budget regression. With the shared single-session suite (maxInstances 1, 39 specs),
  a broken click never opens the dialog/navigation a spec then `waitUntil`/`waitForExist`s for, so
  each affected spec burned its full 10–30s waits (some multiple) before failing; summed across the
  suite that blew past the 20-min job cap (mocha's 90s per-test cap can't save a 39-spec suite that
  is uniformly slow). Fixing the mouse harness removes the cascade.
- **Windows leg** is a **separate, already-filed** issue (CPE-1048): the WebView2 DevToolsActivePort
  startup crash on stock `windows-latest` (session-not-created) — NOT the CDP-mouse issue. On Windows
  msedgedriver DOES attach CDP, so the fast-path is correct there; no separate mouse fix was needed.
  That leg is already `continue-on-error: true` and out of scope here.

**Fix.** `gui-smoke/lib/mouse.ts` — add a WebDriver-native fallback. A once-per-session `useCdp()`
probe (memoizes `cdpAvailable()`) picks the path: **CDP fast-path where available** (Windows/Edge/
WebView2 — preserves the CPE-1155 non-grabbing guarantee on the interactive machine), else the **W3C
Actions API** (`browser.performActions([...])` + `releaseActions`, the `POST /session/:id/actions`
endpoint WebKitWebDriver implements). `click`/`doubleClick`/`rightClick`/`hover`/`dragTo` build
`pointer`-source sequences; `scroll` builds a `wheel`-source `scroll`. The Actions path only runs
where CDP is absent — Linux CI under xvfb (virtual display, no user to hijack) and attended macOS/
Linux local runs (the harness already steals window focus at launch there, and those OSes have no
non-grabbing tauri-driver path), so [[automation-must-not-hijack-screen]] is preserved where it
matters (Windows). No new deps; all public signatures unchanged, so no spec edits were needed.

**Verification.** Local `npm run typecheck` in `gui-smoke/` passes clean. The suite itself is
CI-only (needs tauri-driver + WebKitWebDriver/xvfb, or WebView2) — **final verification is the CI
gui-smoke leg on this PR**: confirm the `[mouse.ts] … CDP input injection is not reachable` error is
gone on the ubuntu-latest leg and the suite completes under the 20-min timeout (and ideally green).

## Foreman outcome note (2026-08-08, post-merge of PR #722, aed89022)
The mouse-CDP root cause is **fixed and proven**: PR #722's ubuntu gui-smoke run shows **0 CDP-mouse errors**
(the `CDP input injection is not reachable` throw is gone), the W3C-Actions `performActions` pointer sequences
execute on WebKitWebDriver, and **9 specs now PASS** that previously died on the first click. Merged on that
basis (its other checks all green; gui-smoke is admin-overridden like every merge until it's green).

The leg is **not yet fully green**, but for reasons this fix did not cause and were previously masked: ~8 specs
now fail on **missing seeded content** (drive tiles / seeded folders / symlink / `--open` folder not present on
the Linux runner — environmental), and the **20-min job timeout** is now too short (the run reached only ~17 of
~39 specs before cancellation, because specs actually RUN now instead of failing instantly). Both are tracked as
**CPE-1481** (raise the timeout FIRST to see the true failing set, then triage the revealed failures). This
ticket is correctly Done for its own scope (the mouse harness); CPE-1481 owns "gui-smoke fully green."
