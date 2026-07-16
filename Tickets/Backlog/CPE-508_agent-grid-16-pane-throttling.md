---
id: CPE-508
title: "Agent Grid — scale to 16 panes with off-screen throttling"
type: Feature
status: Open
priority: Medium
component: Sidecar
tags: [needs-prereq, big-design]
estimate: 2-3h
created: 2026-07-16
epic: CPE-501
---

## Summary
Let the grid scale to **up to 16 visible agents** (the activation decision) without the window
bogging down. Panes that aren't focused/visible have their terminal rendering **throttled or paused**,
and only visible panes are `fit()`ed, so 16 live PTYs stay responsive. Output is never lost — a
throttled pane still buffers (xterm scrollback / the PTY) and catches up when it regains focus.

## Acceptance Criteria
- [ ] The grid renders up to 16 tiles; a documented ceiling governs how many tile at once (further
      sessions remain reachable via tabs).
- [ ] Non-focused / off-screen panes throttle or pause xterm rendering (e.g. lower refresh, skip
      `fit`/repaint) while their PTY keeps receiving — no output is dropped; refocus catches up.
- [ ] A pure, unit-tested policy decides per-pane render state from {focused, visible, count}.
- [ ] Measured: 16 chatty sessions keep the focused pane smooth (no input lag); note the method.
- [ ] Tests for the throttling policy.

## Notes
**needs-prereq:** [[CPE-506]]. `big-design` — the performance slice; visibility-outranks-speed applies
*inside* the grid (AGENT-WATCH precedence), but the plain single-pane path must stay fast.
