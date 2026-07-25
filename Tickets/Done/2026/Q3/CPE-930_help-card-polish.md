---
id: CPE-930
title: Agent Deck swarm/grid "?" help card looks bad — focus + polish it
type: bug
component: Sidecar
priority: medium
tags: ready
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
Clicking the swarm/grid "?" dumped the ENTIRE Agent Deck manual (titled "How the Agent Deck works")
scrolled to an anchor — jarring and ugly. (The CPE-929 cross-window docs event rarely fires because the
console is a remote-URL sidecar window without `window.__TAURI__.event`, so this inline panel is what
actually shows.)

Rebuilt the inline help as a **focused, polished docs card**:
- Each area "?" shows **one topic** with its own title — swarm "?" → "Swarms", grid "?" → "Agent Grid";
  the top-bar "?" still shows the full guide.
- Cleaner styling: sticky titled header with a divider, accent uppercase section labels, roomier
  line-height, readable max-width, proper card border/radius/shadow, theme-matched (`--line`/`--accent`).
- Swarm topic rewritten to be tight and demo-aware.

## Acceptance Criteria
- [x] Swarm/grid "?" show a single focused, titled topic (not the whole manual).
- [x] Top-bar "?" still shows the full guide. Browser-verified appearance.
- [x] Launcher harness (focus + title + full-guide) + full suite green (925 tests).

## Work Log
- 2026-07-23 — Topic-filtered openHelp + restyled #help-panel; verified in a browser.
