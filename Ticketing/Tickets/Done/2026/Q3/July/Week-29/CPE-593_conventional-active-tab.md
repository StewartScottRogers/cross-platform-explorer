---
id: CPE-593
title: "Tab bars: conventional, unmistakable active-tab styling (app-wide standard)"
type: Enhancement
status: Done
priority: Medium
component: Frontend
tags: [ready]
created: 2026-07-17
closed: 2026-07-17
---

## Summary
In the AI Console the active tab was nearly indistinguishable from inactive ones (a faint border only).
Make the active tab **conventional and unmistakable**, apply it to **all** tab strips, and record it as
an app-wide standard so future tabs follow it.

## Changes
- `src/app.css` (main window `.tab`) + `sidecar/ai-console/src/launcher.html` (`#tabs .tab`): the active
  tab gets an **accent top-bar** (`inset 0 2px 0 var(--accent)`) + content-surface background + 3-sided
  border + `font-weight:600`; **inactive tabs** become recessed chips (subtle fill + inset border +
  dimmed text) with a hover lighten. All theme-variable / system-colour based.
- `docs/design/TABS.md` — the standard; **CLAUDE.md** UI-conventions gains a Tabs bullet pointing at it.

## Acceptance Criteria
- [x] The active tab is obvious at a glance (accent bar + lifted surface) in both the main window and the
      AI Console; inactive tabs read as distinct chips.
- [x] Both tab strips share one convention, documented in `docs/design/TABS.md` + CLAUDE.md.
- [x] Launcher jsdom tests + `npm run check` green.

## Notes
Colours are theme/system variables only (identical light/dark, cross-platform) — same discipline as the
menu + pill standards.
