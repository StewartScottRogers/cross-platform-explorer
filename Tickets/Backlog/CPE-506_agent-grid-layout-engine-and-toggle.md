---
id: CPE-506
title: "Agent Grid — auto-reflow layout engine + Tabs⇄Grid toggle"
type: Feature
status: Open
priority: Medium
component: Sidecar
tags: [ready]
estimate: 3-4h
created: 2026-07-16
epic: CPE-501
---

## Summary
Foundation of the Agent Grid ([[CPE-501]]). Today the AI Console launcher (`sidecar/ai-console/src/
launcher.html`) shows one agent at a time: a tab strip over per-session `.term-pane` xterm terminals,
only the active one visible. Add an **auto-reflowing grid** view that tiles the existing term-panes,
and a **Tabs ⇄ Grid toggle** in the toolbar (tabs stay the default — opt-in grid, nothing existing
breaks). Per the activation decision: **auto grid** (uniform tiles that reflow: 1→full, 2→side-by-side,
3-4→2×2, …), not manual splits.

## Acceptance Criteria
- [ ] A toolbar control toggles between **Tabs** (current single-pane) and **Grid** views; Tabs is the
      default and unchanged when selected.
- [ ] In Grid view every live session renders in its own tile; tiles **auto-reflow** to a best-fit
      rows×cols as sessions are added/closed (pure, unit-tested `gridDims(n)` → {rows, cols}).
- [ ] Each tile hosts the **existing** `.term-pane` terminal; every visible xterm is re-`fit()`ed on
      layout change / window resize so no terminal is clipped.
- [ ] Exactly one tile is the **focused** pane; typing routes to it, and clicking a tile focuses it.
- [ ] Closing a session removes its tile and reflows; ending the last one falls back cleanly.
- [ ] Unit tests (jsdom launcher harness) for `gridDims` + the view-toggle state.

## Notes
Auto grid + toggle per CPE-501 activation. Prereq for CPE-507/508/509/510. Reuses the launcher jsdom
test harness. Keep the plain single-pane path fast (PURPOSE tiebreaker) — grid is additive.
