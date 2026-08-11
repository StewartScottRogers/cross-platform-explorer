---
id: CPE-1610
title: "Submenu flyout: clamp the left edge, cap its height, and close it when its parent row scrolls away"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-10
closed:
---

> ID note: 1605–1609 are deliberately reserved for bug tickets a concurrent docs worker (CPE-1604) may file.

## Why
Three follow-ups from the independent reviewer's round-2 pass on CPE-1601 / PR #808, which made the context
menu scrollable and re-anchored the submenu flyout with `position: fixed`. That fix is correct and merged
on its own merits; these are narrower edge cases it either left in place or newly exposed. Two of the three
are the *same class of defect* CPE-1601 exists to eliminate — a menu item you cannot reach — so they belong
together.

## 1. The left-edge flip has no bounds check (reproduced)
In `Submenu.svelte`'s `positionFlyout()`, the vertical axis gets a full clamp (`Math.min` then `Math.max`),
but the horizontal axis only computes a binary flip — `left = flip ? pr.left - fw : pr.right` — with no
final bounds check. The reviewer reproduced a wide flyout (a long macro or command name) opened near the
left edge of a narrow window computing **`left = -1252px`**: off-screen, with nothing to pull it back.

Fix: clamp `left` exactly as `top` is clamped —
`left = Math.max(pad, Math.min(left, window.innerWidth - fw - pad));`

## 2. A flyout taller than the viewport still overflows (reproduced, pre-existing)
`.flyout` has no `max-height` and no `overflow-y` (confirmed via `getComputedStyle`:
`max-height: none`, `overflow-y: visible`). The reviewer simulated a 40-row "Run macro ▸" flyout — entirely
plausible, since `macros` and `userCommands` are unbounded props — and it rendered **1292px tall in an
853px viewport**, overflowing the bottom by 445px with no scrollbar. Screenshot-confirmed.

This is CPE-1601's reachability bug recurring one level deeper, and it is **pre-existing** — the old
`position: absolute` flyout had the same unbounded height before any of this work started.

Fix: give `.flyout` its own `max-height: calc(100vh - 12px); overflow-y: auto;` — the same pattern `.ctx`
now uses. **Note the trap that bit round 1**: setting `overflow-y` forces `overflow-x` to compute as `auto`,
making the element a clip box on both axes. That is safe *here* only because no nested submenu exists in
the codebase today (`Submenu` is never nested inside another `Submenu`'s slot). If nested submenus are ever
added, this fix must be revisited — say so in a code comment.

## 3. A keyboard-opened flyout can float detached (new, narrow)
The hover path closes correctly when the menu scrolls (a real `mouseleave` fires). A **keyboard**-opened
flyout (`ArrowRight`) has no such gate: with the parent row scrolled fully outside `.ctx`, `positionFlyout()`
recomputed `top: 6px` and left the flyout open, pinned to the viewport edge, floating over unrelated rows
with its parent row invisible. This scenario was impossible before the menu became scrollable, so it is new
— but narrow (needs keyboard-open plus a scroll that doesn't cross `.submenu`'s bounds).

Fix: in the scroll listener, **close** the submenu rather than repositioning it when the parent row's rect
falls outside `.ctx`'s clip rect (`pr.bottom < ctxRect.top || pr.top > ctxRect.bottom`).

## Acceptance criteria
- A wide flyout near the left edge of a narrow window stays fully on-screen.
- A 40-row flyout is scrollable and every row reachable; a short flyout gains no scrollbar.
- A keyboard-opened flyout closes when its parent row scrolls out of view.
- Verified **in a real browser**, not only jsdom — jsdom returns zero-size rects and cannot see any of this
  (3,231 tests passed while every flyout in the app was clipped). Add real-geometry assertions to the
  `gui-smoke` submenu spec the way PR #808 did.

## Notes
Also worth checking whether `AgentMenu` / `TabMenu` share the same flyout mechanism and the same gaps —
CPE-1601 already carries that as an open question. Conflict surface: `src/lib/components/Submenu.svelte`,
`ContextMenu.test.ts`, `gui-smoke/specs/macro-in-menu.smoke.ts`. Model: sonnet.
