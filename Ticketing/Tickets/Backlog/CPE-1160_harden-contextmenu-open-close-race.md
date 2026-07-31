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
- [ ] A menu opened by a `contextmenu` handler that does NOT call `stopPropagation` still stays open (the
      footgun is removed) — proven by a test that opens the menu via a bubbling `contextmenu` with no
      stopPropagation and asserts it survives.
- [ ] Right-click-elsewhere and left-click-outside dismissal still work (regression tests).
- [ ] Optionally simplify the now-unnecessary `stopPropagation` calls in the openers (or leave them — they're
      harmless), documenting the new contract.
- [ ] `npm run check` green; a CDP-harness (CPE-1155) test confirms the real behaviour end-to-end.

## Notes
- Origin: the CPE-1154 → 1157 → 1159 recurrence. This removes the class, not just the instance.
- Depends conceptually on CPE-1155 (faithful non-grabbing repro) for end-to-end verification.
