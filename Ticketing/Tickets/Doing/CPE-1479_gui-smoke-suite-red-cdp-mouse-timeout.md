---
id: CPE-1479
title: "GUI-smoke suite is RED on every main run — CDP mouse-injection unavailable on driver + ~20min job timeout (Visual Critic/UAT substrate down)"
type: Bug
status: Doing
priority: High
component: CI/QA-infra
tags: [ready]
epic: CPE-810
qa-architecture: true
created: 2026-08-08
---
## Why this matters (QA-architecture, high leverage)
The `GUI smoke` CI leg (tauri-driver + WebdriverIO, CPE-1171) is the **automated GUI-verification substrate** the
workshift's Visual Critic + UAT Tester depend on to check user-facing changes *without pulling in the user*. It is
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
Found during the CPE-1477 workshift while validating the CSP change's runtime render (the gui-smoke leg was the
prescribed verification and turned out to be independently broken). Epic CPE-810 (client/server contract +
security-adjacent CI). Coordinate with the concurrent workshifts_* process. Filed by the QA Architect.
