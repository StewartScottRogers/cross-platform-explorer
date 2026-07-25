---
id: CPE-506
title: "Agent Grid — auto-reflow layout engine + Tabs⇄Grid toggle"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [ready]
estimate: 3-4h
created: 2026-07-16
epic: CPE-501
closed: 2026-07-16
---

## Summary
Foundation of the Agent Grid ([[CPE-501]]). Today the AI Console launcher (`sidecar/ai-console/src/
launcher.html`) shows one agent at a time: a tab strip over per-session `.term-pane` xterm terminals,
only the active one visible. Add an **auto-reflowing grid** view that tiles the existing term-panes,
and a **Tabs ⇄ Grid toggle** in the toolbar (tabs stay the default — opt-in grid, nothing existing
breaks). Per the activation decision: **auto grid** (uniform tiles that reflow: 1→full, 2→side-by-side,
3-4→2×2, …), not manual splits.

## Acceptance Criteria
- [x] A toolbar control toggles between **Tabs** (current single-pane) and **Grid** views; Tabs is the
      default and unchanged when selected.
- [x] In Grid view every live session renders in its own tile; tiles **auto-reflow** to a best-fit
      rows×cols as sessions are added/closed (pure, unit-tested `gridDims(n)` → {rows, cols}).
- [x] Each tile hosts the **existing** `.term-pane` terminal; every visible xterm is re-`fit()`ed on
      layout change / window resize so no terminal is clipped.
- [x] Exactly one tile is the **focused** pane; typing routes to it, and clicking a tile focuses it.
- [x] Closing a session removes its tile and reflows; ending the last one falls back cleanly.
- [x] Unit tests (jsdom launcher harness) for `gridDims` + the view-toggle state.

## Resolution
Added the Agent Grid foundation to `sidecar/ai-console/src/launcher.html` — a **Tabs ⇄ Grid** view over
the *same* per-session `.term-pane` terminals, no terminal/session rewrite.

- **`gridDims(n)`** (pure): near-square, cols-first best fit — 1→1×1, 2→1×2 (side by side), 3-4→2×2,
  5-6→2×3, … 16→4×4. Unit-tested.
- **`applyView()`** is the single layout authority: in **grid** view `#terms` becomes a CSS grid
  (`--grid-cols`/`--grid-rows` from `gridDims`) with every pane tiled and re-`fit()`ed; in **tabs** view
  only the active pane shows (unchanged behaviour). `.term-pane` flips from `absolute inset:0` to a
  relative grid item via a `#terms.grid-view` override; the focused tile carries `.focused`.
- **`activate(id)`** now just sets `activeId` + calls `applyView()` + focuses — so add/close/switch all
  reflow through one path. `addSession` / `closeSession` (incl. a background tile) / `closeAllSessions`
  reflow the grid and show/hide the toggle bar.
- A **`#view-toggle`** button (▦ Grid / ▤ Tabs) appears only while sessions exist; wired at boot.
- The per-pane `ResizeObserver` now refits the active pane (tabs) **or** any visible tile (grid).

The plain single-pane path is the default and untouched (PURPOSE tiebreaker) — grid is purely additive.
Tests: `gridDims` values + toggle behaviour (tabs shows 1 pane, grid shows all with the right
`--grid-cols`, one `.focused`, bar hidden until sessions exist, columns reflow 3→5) in the jsdom
launcher harness. 36 launcher tests (3 new) + 511 frontend tests pass; `npm run check` clean.

## Work Log
2026-07-16 — Picked up (dayshift; foundation of CPE-501). Estimate: 3-4h.
2026-07-16 — Researched launcher: sessions Map → per-session term-pane (absolute inset:0), only active shown; `activate()` toggled display. Chose to make `#terms` a CSS grid in grid view and route all layout through a new `applyView()`.
2026-07-16 — Implemented gridDims + applyView + view toggle; rewired activate/add/close; added CSS + the toggle button; refit visible tiles on resize. Added 3 jsdom tests.
2026-07-16 — Verified: 36 launcher tests + 511 frontend tests pass; `npm run check` clean. Assumption logged: auto grid math is cols-first `ceil(sqrt(n))` (matches the activation preview 4→2×2, 5→2×3). All ACs met.

## Notes
Auto grid + toggle per CPE-501 activation. Prereq for CPE-507/508/509/510. Reuses the launcher jsdom
test harness. Keep the plain single-pane path fast (PURPOSE tiebreaker) — grid is additive.
