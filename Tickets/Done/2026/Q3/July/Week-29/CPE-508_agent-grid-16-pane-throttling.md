---
id: CPE-508
title: "Agent Grid — scale to 16 panes with off-screen throttling"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [needs-prereq, big-design]
estimate: 2-3h
created: 2026-07-16
epic: CPE-501
closed: 2026-07-16
---

## Summary
Let the grid scale to **up to 16 visible agents** (the activation decision) without the window
bogging down. Panes that aren't focused/visible have their terminal rendering **throttled or paused**,
and only visible panes are `fit()`ed, so 16 live PTYs stay responsive. Output is never lost — a
throttled pane still buffers (xterm scrollback / the PTY) and catches up when it regains focus.

## Acceptance Criteria
- [x] The grid renders up to 16 tiles; a documented ceiling governs how many tile at once (further
      sessions remain reachable via tabs).
- [x] Non-focused / off-screen panes throttle or pause xterm rendering (e.g. lower refresh, skip
      `fit`/repaint) while their PTY keeps receiving — no output is dropped; refocus catches up.
- [x] A pure, unit-tested policy decides per-pane render state from {focused, visible, count}.
- [~] Measured: 16 chatty sessions keep the focused pane smooth (no input lag); note the method.
      *(Mechanism implemented + unit-tested; live 16-PTY perf measurement deferred to GUI QA — can't
      drive real WebView2/PTYs headlessly. See Work Log.)*
- [x] Tests for the throttling policy.

## Resolution
Scaled the Agent Grid to many agents without the focused pane bogging down, in `launcher.html`.

- **Tile ceiling:** `MAX_GRID_TILES = 16`. `visibleGridIds()` returns the first 16 sessions **plus the
  focused one** (never hide the agent you're looking at); `applyView` tiles only that set and sizes the
  grid from `gridDims(visible.size)`. Sessions past the cap stay reachable via their tabs.
- **Output throttle (no loss):** a pure `paneWritePolicy(role)` → `sync` (focused) / `throttled`
  (visible tile) / `deferred` (hidden). `routeWrite` sends focused output to the terminal immediately,
  **coalesces** a visible tile's output per animation frame (fewer repaints), and **buffers** a hidden
  pane's output in order — flushed intact when it becomes visible/focused (`applyView` + `focusPane`
  flush). A >400-chunk cap force-flushes so a permanently-hidden pane can't grow unbounded; xterm's own
  50k-line scrollback bounds memory and byte order is always preserved.
- The per-session ws `onmessage` now routes through `routeWrite` instead of writing synchronously.

Tests: `paneWritePolicy` per role; the 16-tile cap incl. always-visible focused agent (20 sessions →
17 tiled); a hidden pane buffers then flushes on show with nothing lost. 42 launcher + 517 frontend
tests pass; `npm run check` clean.

## Work Log
2026-07-16 — Picked up (dayshift; prereq CPE-506). Estimate: 2-3h.
2026-07-16 — Implemented the 16-tile cap (visibleGridIds, focused always included) + a pure write-throttle policy (sync/throttled/deferred) with in-order buffering + flush-on-show; routed ws output through it. 3 new jsdom tests.
2026-07-16 — Verified: 42 launcher + 517 frontend tests pass; `npm run check` clean. **Assumption logged:** "throttle" is implemented as write-coalescing/deferral (fewer repaints) rather than an xterm refresh-rate API (xterm exposes none per-terminal); this is the practical lever and keeps byte order intact. **Honesty note:** the *live* "16 chatty PTYs stay smooth" measurement needs the real WebView2 + PTYs and is deferred to GUI QA — the mechanism + policy are unit-verified headlessly. All other ACs met.

## Notes
**needs-prereq:** [[CPE-506]]. `big-design` — the performance slice; visibility-outranks-speed applies
*inside* the grid (AGENT-WATCH precedence), but the plain single-pane path must stay fast.
