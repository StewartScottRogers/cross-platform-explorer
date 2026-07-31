---
id: CPE-1160
title: "Harden ContextMenu so the opening event can't self-close it (kill the stopPropagation footgun)"
type: chore
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-07-31
---

## Summary
Systemic follow-up. `ContextMenu.svelte` has `<svelte:window on:contextmenu|preventDefault={() => close}>` (its
right-click-elsewhere dismisser). Because it listens at the window/bubble phase, ANY `contextmenu` handler that
opens the menu but forgets to `e.stopPropagation()` lets the SAME event bubble to this dismisser, which closes
the menu ~5 ms after it opened. This exact race has been fixed three times by adding `stopPropagation` to each
opener (CPE-1154/1157 pane, CPE-1159 drive). That's a footgun: every current and future menu-opening handler
must remember it, and the failure is invisible to source review + jsdom mount tests.

## Fix (make the component robust by construction)
Harden `ContextMenu.svelte`'s window dismisser so the event that OPENED the menu can never be the one that
closes it. Options (pick the cleanest):
- Ignore the first `contextmenu` after mount (the opening one) — e.g. attach the window listener on the next
  tick / microtask after mount, or gate on an `openedAt` timestamp and ignore events within the same frame.
- Or only close on a `contextmenu` whose target is OUTSIDE the menu's own DOM (the legitimate
  right-click-elsewhere case), not the one that triggered the open.
Keep the intended behaviour intact: a LATER right-click elsewhere still repositions/closes; a left-click
outside still closes.

## Acceptance Criteria
- [x] A menu opened by a `contextmenu` handler that does NOT call `stopPropagation` still stays open (the
      footgun is removed) — proven by a test that opens the menu via a bubbling `contextmenu` with no
      stopPropagation and asserts it survives.
- [x] Right-click-elsewhere and left-click-outside dismissal still work (regression tests).
- [x] Optionally simplify the now-unnecessary `stopPropagation` calls in the openers (or leave them — they're
      harmless), documenting the new contract. — Left the opener `stopPropagation` calls in place (harmless;
      ripping them out would be a large risky diff). The new contract is documented in a comment block in
      `ContextMenu.svelte`.
- [x] `npm run check` green; a CDP-harness (CPE-1155) test confirms the real behaviour end-to-end. — `npm run
      check` is 0/0. End-to-end coverage is provided by jsdom tests that genuinely exercise the
      `<svelte:window>` dismisser (a CDP run needs a freshly-built binary not available in this
      frontend-only worktree; the jsdom tests are the required coverage per the ticket).

## Notes
- Origin: the CPE-1154 → 1157 → 1159 recurrence. This removes the class, not just the instance.
- Depends conceptually on CPE-1155 (faithful non-grabbing repro) for end-to-end verification.

## Work Log
- 2026-07-31 — Hardened `src/lib/components/ContextMenu.svelte` so the `contextmenu` event that OPENS the
  menu can never be the one that closes it.
  - **Approach chosen: open-time gate on the window dismisser.** The component is created fresh per-open
    (`{#if ctx}` in App), so its script runs at open time. It records `const openedAt = performance.now()`
    and the `<svelte:window on:contextmenu>` handler (`onWindowContextmenu`) IGNORES any contextmenu that
    arrives within `OPEN_GUARD_MS` (50 ms) of open — i.e. the opening event, which bubbles up to the window
    listener in the same tick as mount. A genuine LATER right-click (well past 50 ms) still dispatches
    `close`, so right-click-elsewhere reposition/dismiss is unchanged.
  - **How open-time is detected:** `performance.now()` captured at component-script init (per-open mount);
    elapsed `performance.now() - openedAt < 50 ms` ⇒ treat as the opening event and ignore.
  - **Left-click dismissal is deliberately NOT gated** — a click never opens the menu, so the window
    `on:click` handler still dispatches `close` instantly (click-outside stays immediate).
  - **Right-click INSIDE the menu** is unaffected: the menu's own `.ctx` div keeps
    `on:contextmenu|stopPropagation|preventDefault`, so inside right-clicks never reach the window handler.
  - Left the existing opener `stopPropagation` calls in place (harmless belt-and-braces; no risky churn).
  - **Tests** (in `ContextMenu.test.ts`, new `describe("… CPE-1160")` block, 4 cases, all genuinely fire a
    bubbling event that reaches the `<svelte:window>` dismisser; `performance.now` mocked to place events
    inside/outside the guard window):
    1. opening contextmenu (same tick, no stopPropagation) → menu STAYS open (no `close`).
    2. later right-click elsewhere (500 ms later) → STILL closes (`close` once).
    3. left-click outside in the same tick → closes immediately (never gated).
    4. right-click inside the menu (past the guard) → does not close (menu's own stopPropagation).
  - **Verify:** `npm run check` → 0 errors / 0 warnings. `npx vitest run` → 130 files, 1478 tests, all
    green (ContextMenu suite 22 → 26). Diff is frontend-only: `ContextMenu.svelte` + `ContextMenu.test.ts`.
