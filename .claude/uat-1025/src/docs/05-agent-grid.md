---
title: Agent Grid
order: 5
category: Agent Deck
categoryOrder: 3
---

# Agent Grid

The Agent Grid tiles several running agents so you can watch and interact with them at once — instead of
one tab at a time.

## Tabs vs Grid

Use the **Tabs ⇄ Grid** toggle in the Agent Deck toolbar. **Tabs** is the default single-pane view;
**Grid** tiles every live session. The tiles **auto-arrange** into a near-square layout and reflow as you
add or close agents (2 side by side, 4 in a 2×2, and so on).

## Working in the grid

- Each tile has a header with the agent's **colour chip**, name, and usage — the same chip as its tab and
  its left-pane Agents leaf, so they're easy to correlate.
- Click a tile to focus it, or press **Ctrl+Alt+Arrow** to move between tiles.
- The **focused** agent renders live; off-screen tiles are throttled so many agents stay responsive
  (up to about a dozen at once).

The grid is also the natural way to watch a **swarm** — several agents working one task together each get
their own tile. See the **Swarms** page.

## Persistence

Your view (Tabs or Grid) and layout are remembered per workspace and restored when you reopen the
console or reattach sessions.
