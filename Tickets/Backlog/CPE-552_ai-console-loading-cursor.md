---
id: CPE-552
title: "AI Console shows no busy cursor while it is loading"
type: Bug
status: Open
priority: Medium
component: Frontend
tags: [ready]
estimate: 30m
created: 2026-07-16
closed:
---

## Summary
User QA (2026-07-16): "The AI Console needs a mouse cursor when loading." When the AI Console pane/window
is booting (sidecar starting + launcher page loading + catalog fetch), there is no wait-cursor, so it
looks frozen. The launcher already has a busy-cursor system (`body.busy { cursor: progress }` +
`beginBusy`/`withBusy`), but the **initial boot** (before/while `load()` runs, and the sidecar UI iframe
mounts) isn't covered.

## Expected Behavior
From the moment the AI Console starts opening until its launcher UI is interactive, the cursor shows the
platform wait/progress indicator — consistent with the busy-cursor convention (CPE-547 /
[docs/design/BUSY-CURSOR.md](../../docs/design/BUSY-CURSOR.md)).

## Acceptance Criteria
- [ ] The AI Console shows the wait/`progress` cursor during its initial boot (sidecar start + launcher
      `load()` + first render), clearing once the UI is ready.
- [ ] No flicker on a fast boot (respect the existing debounce) and no stuck cursor if boot fails.
- [ ] Covered where testable: the launcher jsdom harness asserts boot wraps in the busy tracker; if the
      pane-mount side is host/frontend, wrap that invoke in the CPE-548 busy `invoke`.
- [ ] `npm run check` clean.

## Notes
Two surfaces may be involved: the **launcher page** boot (`launcher.html` `load()`), and the **explorer
side** mounting the console pane/window. Cover whichever lacks the cursor; reuse the existing
`beginBusy`/`withBusy` (launcher) and the busy `invoke` wrapper (explorer).
