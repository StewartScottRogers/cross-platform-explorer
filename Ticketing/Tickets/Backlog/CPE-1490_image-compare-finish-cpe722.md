---
id: CPE-1490
title: "Finish image compare (side-by-side / onion-skin / pixel-diff heatmap) — the deferred CPE-722 scope"
type: Feature
status: Backlog
priority: Medium
component: Multiple
tags: [ready]
epic: CPE-722
created: 2026-08-08
---
## What (close a capstone epic that's visibly 3/4 done)
CPE-722 (compare studio) named **image compare** in its Definition of Done, but CPE-779 shipped
folder/binary/text compare and **explicitly deferred image compare** as a follow-up — it was never built. This
is the most visually GUI-exclusive comparison mode (a pixel-diff heatmap is meaningless in a terminal), so it's
a strong "GUI beats TUI" capstone. Surfaced by the competitive-landscape GUI survey (Directory Opus / ForkLift
image tools).

## Scope
Add an **image compare** mode to the existing compare shell (reuse CPE-722/779's compare view + `cpe-server`
compare crate). Two selected images →:
- **Side-by-side** (synced zoom/pan).
- **Onion-skin** (opacity slider blend).
- **Pixel-diff heatmap** (per-pixel delta highlighted; report % different + bounding region).
Handle differing dimensions gracefully (align/letterbox + note the mismatch). Bounded decode (mirror the
thumbnail pipeline's size/resource-exhaustion guards — never decode unbounded).

## How
- Backend: extend the `cpe-server` compare module with an image-diff function (decode both — reuse the
  thumbnail/`image` decode path already in-tree, **no new heavy dep**; bounded), returning the diff mask +
  stats. Headless-testable with small fixture images.
- Frontend: a new tab/pane in the compare shell (side-by-side / onion-skin / heatmap toggle). Wire selection →
  the two images, per existing compare UX.

## Verify
Backend: `cargo test` with fixture image pairs (identical → 0% diff; one-pixel change → detected; differing
sizes → graceful; garbage → Err). `cargo clippy --all-targets -D warnings`. Frontend: `npm run check`;
gui-smoke can exercise the pane once the gui-smoke suite is green (CPE-1481).

## Effort
Medium. Backend diff = pure logic + fixtures (headless-buildable half is a clean batch); the view is the GUI
half. Splits cleanly along the crate seam. Epic CPE-722.
