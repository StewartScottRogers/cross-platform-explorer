---
id: CPE-1155
title: "QA Architect: faithful mouse input (click/scroll/hover/right-click) in tests WITHOUT grabbing the user's cursor"
type: chore
component: Testing
priority: high
status: Done
tags: ready
created: 2026-07-30
closed: 2026-07-31
epic: CPE-579
---

## Summary
User-requested (2026-07-30). The QA Architect should build a way to drive **real mouse behaviour** — clicking,
scrolling, hovering, right-clicking, drag — in the GUI tests **without hijacking the user's physical mouse**,
so mouse-driven tests can run in the background while the user keeps working. This is the QA Architect's
mission (eliminate manual/intrusive testing) and it closes a gap that has bitten us twice.

## Why (the exact tension this resolves)
Today the GUI harness (`gui-smoke`, tauri-driver + WebdriverIO) has two mouse options and both are bad:
- **Real OS-pointer actions** (`browser.action('pointer')...`) move/grab the **physical cursor** — they hijack
  the screen, which violates [[automation-must-not-hijack-screen]] and makes tests unrunnable while the user
  is at the machine.
- **Synthetic DOM events** (`browser.execute(() => el.dispatchEvent(new MouseEvent(...)))`) don't grab the
  mouse, but they are **unfaithful**: they go straight to a chosen element's handler and bypass real hit-testing
  and native browser behaviour. This is exactly how the **CPE-1154 native-context-menu leak escaped detection**
  — a synthetic `contextmenu` on `.rows` "worked", but a real right-click in an empty folder showed the Edge
  menu. A test that can't catch that class of bug is a false sense of safety.

## The approach (recommended)
Drive input at the **browser input layer via CDP** (Chrome/Edge DevTools Protocol), which WebView2/msedgedriver
exposes: `Input.dispatchMouseEvent` (mousePressed/mouseReleased/mouseMoved, incl. `button:"right"` for
context menu), `Input.dispatchMouseWheelEvent` (scroll), and drag. These inject through the **real** input
pipeline (true hit-testing, native context menu, real event order) — as faithful as a physical click — but
they do **NOT** move the OS cursor, so the user's mouse is never grabbed. WebdriverIO can send CDP via
`browser.cdp('Input', 'dispatchMouseEvent', {...})` / `browser.sendCommandAndGetResult` against the
Chromium-based Edge driver on Windows.

Combine with the existing off-screen, non-focused **test-mode window** ([[automation-must-not-hijack-screen]])
so the whole run is invisible + non-intrusive.

## Acceptance Criteria
- [x] A reusable `gui-smoke` helper (e.g. `mouse.ts`) exposing `click(sel)`, `rightClick(sel|point)`,
      `scroll(sel, dy)`, `hover(sel)`, and `dragTo(from, to)` implemented via **CDP input injection** (or an
      equivalent WebView2 input-injection API), verified to NOT move the physical cursor while running.
      → `gui-smoke/lib/mouse.ts` (also `doubleClick` + `cdp`/`cdpAvailable`).
- [x] A proof spec uses it to **reproduce the CPE-1154 class of bug**: a real right-click on an empty folder's
      blank pane asserts the app's `.ctx` opens and the native menu does not — i.e. the helper catches what the
      synthetic-event check missed. → `specs/populated-whitespace.smoke.ts` (empty-folder case), and
      `specs/context-menu.smoke.ts` converted to the helper.
- [x] Documented in `gui-smoke/README.md` + a note in the QA charter (`.claude/qa-architecture/`): "for mouse
      behaviour, use `mouse.ts` (CDP, non-grabbing), NOT `browser.action('pointer')` (grabs the cursor) and NOT
      bare `dispatchEvent` (unfaithful)." Update existing specs that use grabbing pointer actions to the helper
      where practical. → README "Faithful mouse input" section + charter bullet; `context-menu.smoke.ts` converted.
- [x] Runs headless/non-blocking in CI (continue-on-error, CPE-1048) the same as the rest of gui-smoke; a
      short local run demonstrates it works while the user's real cursor stays put. → picked up by the
      `specs/**/*.smoke.ts` glob; local run shows OS cursor byte-identical before/after `rightClick`.
- [x] If CDP mouse injection turns out to be unavailable/unreliable through tauri-driver's msedgedriver on
      Windows, document the finding and the best available fallback (...) rather than silently
      reverting to cursor-grabbing actions. → CDP **is** available here (`cdpAvailable()` → true); documented,
      and `cdpAvailable()` reports the negative case if a future driver drops the endpoint.

## Work Log
- 2026-07-31 (Worker, workshift): Built `gui-smoke/lib/mouse.ts` — CDP `Input.dispatchMouseEvent` /
  `Input.dispatchMouseWheelEvent` via msedgedriver's vendor endpoint
  `POST /session/:id/chromium/send_command_and_get_result`, surfaced by WebdriverIO as
  `browser.sendCommandAndGetResult`. (`browser.cdp(...)` — the puppeteer-backed variant the ticket
  mentioned — is NOT wired for wry here; the vendor endpoint is.) Helpers: `click`, `rightClick`
  (selector OR explicit `{x,y}` point), `doubleClick`, `hover`, `scroll`, `dragTo`, plus `cdp` +
  `cdpAvailable`.
- **Runtime verdict — CDP mouse injection WORKS and is NON-grabbing here.** Verified locally against a
  fresh CLI release build (Edge/WebView2 150 + msedgedriver 150, classic WebDriver against wry):
  `cdpAvailable()` → true; a real CDP right-click on a `.row` opens the app's item menu and on blank
  pane pixels fires the pane `contextmenu`; and the **physical OS cursor position (read via PowerShell
  `[System.Windows.Forms.Cursor]::Position`) was byte-identical before and after** every `rightClick`
  (`3521,1817 → 3521,1817`). CDP input never moves the OS pointer by design and the tauri-driver window
  can stay unfocused/off-screen.
- Proof/regression spec `specs/populated-whitespace.smoke.ts` uses the helper to reproduce the CPE-1154
  class faithfully AND drives the CPE-1157 diagnosis/regression. Converted `context-menu.smoke.ts` off
  `browser.action('pointer')` to `mouse.ts`. `npm run check` 0/0; `gui-smoke` typecheck clean; both
  specs green (7 passing) against the built app.

## Notes
- Epic CPE-579 (self-maintaining quality infra). This is the QA Architect making the whole app more
  automatically + **non-intrusively** testable — a direct step toward "never test mouse behaviour by hand,
  and never have the tests steal my mouse."
- Immediate payoff: retroactively guards CPE-1154 (native-menu leak) and enables faithful hover/scroll/drag
  tests (e.g. the CPE-1153 submenu flyout on hover, drag-out interactions) that were previously
  human-only or cursor-grabbing.
- Related: [[visual-critic-and-screenshots]] (screenshots for looks) + [[gui-verify-needs-build-deploy-run]];
  this adds faithful, non-grabbing *interaction* to the automated toolkit.
