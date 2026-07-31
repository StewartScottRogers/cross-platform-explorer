---
id: CPE-1155
title: "QA Architect: faithful mouse input (click/scroll/hover/right-click) in tests WITHOUT grabbing the user's cursor"
type: chore
component: Testing
priority: high
status: Backlog
tags: ready
created: 2026-07-30
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
- [ ] A reusable `gui-smoke` helper (e.g. `mouse.ts`) exposing `click(sel)`, `rightClick(sel|point)`,
      `scroll(sel, dy)`, `hover(sel)`, and `dragTo(from, to)` implemented via **CDP input injection** (or an
      equivalent WebView2 input-injection API), verified to NOT move the physical cursor while running.
- [ ] A proof spec uses it to **reproduce the CPE-1154 class of bug**: a real right-click on an empty folder's
      blank pane asserts the app's `.ctx` opens and the native menu does not — i.e. the helper catches what the
      synthetic-event check missed.
- [ ] Documented in `gui-smoke/README.md` + a note in the QA charter (`.claude/qa-architecture/`): "for mouse
      behaviour, use `mouse.ts` (CDP, non-grabbing), NOT `browser.action('pointer')` (grabs the cursor) and NOT
      bare `dispatchEvent` (unfaithful)." Update existing specs that use grabbing pointer actions to the helper
      where practical.
- [ ] Runs headless/non-blocking in CI (continue-on-error, CPE-1048) the same as the rest of gui-smoke; a
      short local run demonstrates it works while the user's real cursor stays put.
- [ ] If CDP mouse injection turns out to be unavailable/unreliable through tauri-driver's msedgedriver on
      Windows, document the finding and the best available fallback (e.g. a Windows `SendInput` to the specific
      off-screen window, or WebView2's `CoreWebView2` input APIs via a test seam) rather than silently
      reverting to cursor-grabbing actions.

## Notes
- Epic CPE-579 (self-maintaining quality infra). This is the QA Architect making the whole app more
  automatically + **non-intrusively** testable — a direct step toward "never test mouse behaviour by hand,
  and never have the tests steal my mouse."
- Immediate payoff: retroactively guards CPE-1154 (native-menu leak) and enables faithful hover/scroll/drag
  tests (e.g. the CPE-1153 submenu flyout on hover, drag-out interactions) that were previously
  human-only or cursor-grabbing.
- Related: [[visual-critic-and-screenshots]] (screenshots for looks) + [[gui-verify-needs-build-deploy-run]];
  this adds faithful, non-grabbing *interaction* to the automated toolkit.
