---
id: CPE-466
title: "AI Console session tabs should look like the main explorer's tabs"
type: Defect
status: Done
closed: 2026-07-15
priority: Medium
component: Sidecar (AI Console)
tags: [ready]
created: 2026-07-15
estimate: 1h
epic: CPE-261
---

## Summary
The AI Console's session tabs are styled differently from the main explorer window's tabs. They
should visually match — same shape, spacing, active/inactive treatment, and close affordance — so the
two surfaces feel like one app.

## Fix
Align the launcher's `.tab` styling with the explorer's tab component (`src/lib/components/`), using
the same theme tokens the console already adopts (system colors + `--line`/`--accent`). Match:
- tab height / padding / font size
- active vs inactive background and bottom-border/underline treatment
- hover state
- the × close button placement and size

## Acceptance Criteria
- [x] AI Console tabs match the main window's tabs in shape, spacing, and active/inactive styling.
- [x] Active tab is clearly distinguished the same way the main window distinguishes it.
- [x] Close (×) affordance matches the main window's.
- [x] Legible in the light theme (no hardcoded dark colors).
- [x] Launcher jsdom test asserts the tab styling contract that was agreed.

## Resolution
Restyled `#tabs` / `.tab` in `launcher.html` to mirror the explorer's `.tabbar` / `.tab` (src/app.css):
- Rounded-top raised "folder" tabs: 34px tall, `border-radius: 6px 6px 0 0`, 12px/8px padding, gap 8px,
  min/max width 120/240 — matching the main window's proportions.
- Active tab lifted onto the page background (`background: Canvas`) with a 3-sided border
  (`box-shadow: 0 -1px 0 var(--line), 1px 0 0 var(--line), -1px 0 0 var(--line)`), exactly the main
  window's active-tab treatment.
- Close (×) is now a 20×20 rounded button with a neutral grey hover (`rgba(128,128,128,0.28)`),
  replacing the old red hover — matching the main window's `.tab-close`.
- Kept it on **system colors** (`Canvas`/`CanvasText`/`--line`) so it stays legible on the light theme.

Files: `sidecar/ai-console/src/launcher.html` (tab CSS), `src/lib/ai-console-launcher.test.ts`
(3 new CPE-466 assertions locking the shape/active-lift/close-hover contract). `npm run check` clean;
28 launcher tests pass.

## Work Log
- 2026-07-15 — Picked up. Estimate: 1h. Compared the main `.tab` (app.css) with the console `.tab`
  (launcher.html): main is a raised rounded-top folder tab, console was a flat underline-accent tab.
- 2026-07-15 — Restyled console tabs to the main-window treatment using system colors; added 3 contract
  tests; check + tests green. Closed.

## Notes
Reported by the user: "The Tabs in the AI Console should look like the tabs on the main form."
Compare against the explorer's own tab styling for the source of truth.
